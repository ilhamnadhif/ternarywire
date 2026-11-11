#!/usr/bin/env python3
"""Local servers for the redesign fixture (stdlib only).

    server.py start     start everything in the background (no-op when running)
    server.py stop      stop everything
    server.py status    show what is listening
    server.py run       run in the foreground (what `start` launches)
    server.py poke webhook|capture [--port N]   send sample traffic (shoot.sh)

One set of servers is shared by every worktree of the repository: the pid
file, log and lock live in $TMPDIR/tw-fixture-servers/.

Ports (127.0.0.1):
  18795  HTTP  Bookstore API: /books, /books/<id>, /auth/login, /me, /orders,
               /graphql, /payments, /v1/charges, /health; /__slow/<path> answers
               <path> after 15 s; /__fixture/update.json is an update feed
  18796  HTTP  OpenAI-compatible API: /v1/chat/completions (stream or not), /v1/models
  8931   HTTP  MCP Streamable HTTP endpoint /mcp (tools, resources, prompts, elicitation)
  18900  WS    WebSocket echo (+ subscribe/ping replies, server pushes)
  1883   MQTT  MQTT 3.1.1 broker with a sensor simulator
  18902  SIO   Socket.IO: crates/ternarywire-socketio/examples/echo_server.rs
  50051  gRPC  forwarder to crates/ternarywire-grpc/examples/greeter_server.rs
               (it picks a random port; reflection v1, helloworld.Greeter)
"""

import base64
import fcntl
import hashlib
import http.client
import http.server
import json
import os
import random
import signal
import socket
import socketserver
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
STATE = os.path.join(tempfile.gettempdir(), "tw-fixture-servers")
PIDFILE = os.path.join(STATE, "servers.json")
LOGFILE = os.path.join(STATE, "servers.log")
LOCKFILE = os.path.join(STATE, "start.lock")

HTTP_PORT, AI_PORT, MCP_PORT, WS_PORT, MQTT_PORT, SIO_PORT, GRPC_PORT = 18795, 18796, 8931, 18900, 1883, 18902, 50051
PORTS = [
    ("http", HTTP_PORT, "Bookstore API  http://127.0.0.1:%d" % HTTP_PORT),
    ("ai", AI_PORT, "OpenAI-compatible  http://127.0.0.1:%d/v1" % AI_PORT),
    ("mcp", MCP_PORT, "MCP  http://127.0.0.1:%d/mcp" % MCP_PORT),
    ("ws", WS_PORT, "WebSocket echo  ws://127.0.0.1:%d" % WS_PORT),
    ("mqtt", MQTT_PORT, "MQTT broker  mqtt://127.0.0.1:%d" % MQTT_PORT),
    ("sio", SIO_PORT, "Socket.IO  http://127.0.0.1:%d" % SIO_PORT),
    ("grpc", GRPC_PORT, "gRPC  localhost:%d" % GRPC_PORT),
]


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, flush=True)


def jdump(v):
    return json.dumps(v, separators=(",", ":"), ensure_ascii=False)


# ---------------------------------------------------------------------------
# Sample data (design/redesign/data.js and the screen files)
# ---------------------------------------------------------------------------

BOOKS = {
    1: {"id": 1, "title": "The Pragmatic Programmer", "author": "Andrew Hunt", "price": 42.5, "inStock": True, "tags": ["craft", "classic"]},
    2: {"id": 2, "title": "Designing Data-Intensive Applications", "author": "Martin Kleppmann", "price": 55, "inStock": True, "tags": ["data"]},
    3: {"id": 3, "title": "Laskar Pelangi", "author": "Andrea Hirata", "price": 12, "inStock": False, "tags": ["novel"]},
    4: {"id": 4, "title": "Rust in Action", "author": "Tim McNamara", "price": 39.99, "inStock": True, "tags": ["rust"]},
    5: {"id": 5, "title": "Hands-on Rust", "author": "Herbert Wolverson", "price": 42.0, "inStock": True, "tags": ["rust", "games"]},
}
FAKE_JWT = ("eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1XzE5MiIsInNjb3BlIjoiYm9va3M6cmVhZCBib29rczp3cml0ZSBwcm9maWxlIiwi"
            "aXNzIjoiaHR0cHM6Ly9hdXRoLmJvb2tzdG9yZS5kZXYifQ.c2lnbmF0dXJlLW9mLXRoZS1kZXNpZ24tZml4dHVyZQ")
ME = {"id": "u_192", "name": "Ilham", "email": "ilham@example.com", "scopes": ["books:read", "books:write", "profile"],
      "memberSince": "2024-03-02"}
SESSION_COOKIE = "session=s%3Ab2f9c1e0d4a7f3b29a; Path=/; HttpOnly; SameSite=Lax"


def book_page(previous=False, tag=None):
    data = [dict(BOOKS[i]) for i in (1, 2, 3, 4)]
    total = 5
    if previous:  # what the response looked like before (http-compare)
        data[3]["inStock"] = False
        total = 4
    if tag:
        data = [b for b in BOOKS.values() if tag in b["tags"]]
        total = len(data)
    return {"data": data, "page": 1, "limit": 20, "total": total}


# GraphQL schema (also specs/schema.graphql), for introspection.
GQL_TYPES = {
    "Query": ("OBJECT", [("search", "SearchResult!", [("query", "String!"), ("first", "Int")]), ("book", "Book", [("id", "ID!")]),
                         ("books", "[Book!]!", [("page", "Int"), ("limit", "Int")]), ("me", "User", [])]),
    "Mutation": ("OBJECT", [("createBook", "Book!", [("input", "BookInput!")]), ("deleteBook", "Boolean!", [("id", "ID!")])]),
    "Subscription": ("OBJECT", [("bookAdded", "Book!", [])]),
    "Book": ("OBJECT", [("id", "ID!", []), ("title", "String!", []), ("author", "Author!", []), ("price", "Float!", []),
                        ("inStock", "Boolean!", []), ("tags", "[String!]!", [])]),
    "Author": ("OBJECT", [("name", "String!", [])]),
    "User": ("OBJECT", [("id", "ID!", []), ("name", "String!", []), ("email", "String!", [])]),
    "SearchResult": ("OBJECT", [("totalCount", "Int!", []), ("nodes", "[Book!]!", [])]),
    "BookInput": ("INPUT_OBJECT", [("title", "String!", []), ("author", "String!", []), ("price", "Float!", [])]),
}
GQL_SCALARS = ["String", "Int", "Float", "Boolean", "ID"]


def gql_ref(t):
    if t.endswith("!"):
        return {"kind": "NON_NULL", "name": None, "ofType": gql_ref(t[:-1])}
    if t.startswith("["):
        return {"kind": "LIST", "name": None, "ofType": gql_ref(t[1:-1])}
    kind = "SCALAR" if t in GQL_SCALARS else GQL_TYPES[t][0]
    return {"kind": kind, "name": t, "ofType": None}


def gql_introspection():
    types = [{"kind": "SCALAR", "name": s, "description": None, "fields": None, "inputFields": None, "interfaces": None,
              "enumValues": None, "possibleTypes": None} for s in GQL_SCALARS]
    for name, (kind, fields) in GQL_TYPES.items():
        t = {"kind": kind, "name": name, "description": None, "interfaces": [] if kind == "OBJECT" else None,
             "enumValues": None, "possibleTypes": None, "fields": None, "inputFields": None}
        if kind == "OBJECT":
            t["fields"] = [{"name": f, "description": None, "isDeprecated": False, "deprecationReason": None, "type": gql_ref(ft),
                            "args": [{"name": a, "description": None, "defaultValue": "10" if a == "first" else None, "type": gql_ref(at)}
                                     for a, at in args]} for f, ft, args in fields]
        else:
            t["inputFields"] = [{"name": f, "description": None, "defaultValue": None, "type": gql_ref(ft)} for f, ft, _ in fields]
        types.append(t)
    return {"data": {"__schema": {"queryType": {"name": "Query"}, "mutationType": {"name": "Mutation"},
                                  "subscriptionType": {"name": "Subscription"}, "types": types, "directives": []}}}


# ---------------------------------------------------------------------------
# HTTP servers
# ---------------------------------------------------------------------------

class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True


class Base(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "fixture"
    sys_version = ""

    def log_message(self, fmt, *args):
        log("%s %s" % (self.server.server_address[1], fmt % args))

    def body_bytes(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n) if n else b""

    def body_json(self):
        try:
            return json.loads(self.body_bytes() or b"null")
        except ValueError:
            return None

    def reply(self, status, body=None, headers=(), ctype="application/json; charset=utf-8"):
        data = b"" if body is None else body if isinstance(body, bytes) else (
            body.encode() if isinstance(body, str) else jdump(body).encode())
        # No Server header: the response headers match the prototype's list.
        self.send_response_only(status)
        self.send_header("Date", self.date_time_string())
        if data or status not in (204, 304):
            self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        for k, v in headers:
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    def start_chunked(self, ctype):
        self.send_response_only(200)
        self.send_header("Date", self.date_time_string())
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()

    def chunk(self, data):
        if isinstance(data, str):
            data = data.encode()
        self.wfile.write(b"%x\r\n%s\r\n" % (len(data), data))
        self.wfile.flush()

    def end_chunked(self):
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()


class BookstoreHandler(Base):
    def do_GET(self):
        self.route("GET")

    def do_HEAD(self):
        self.route("GET")

    def do_POST(self):
        self.route("POST")

    def do_PUT(self):
        self.route("PUT")

    def do_PATCH(self):
        self.route("PATCH")

    def do_DELETE(self):
        self.route("DELETE")

    def do_OPTIONS(self):
        self.reply(204, None, [("Allow", "GET, POST, PUT, PATCH, DELETE, OPTIONS")])

    def route(self, method):
        u = urllib.parse.urlsplit(self.path)
        path, q = u.path, urllib.parse.parse_qs(u.query)
        if path.startswith("/__slow/"):
            # A response that takes long (http-loading): the screenshot is
            # taken while the app still waits.
            time.sleep(15)
            path = path[len("/__slow"):]
        parts = [p for p in path.split("/") if p]
        rid = str(uuid.uuid4())
        common = [("X-Request-Id", rid)]
        body = self.body_json() if method in ("POST", "PUT", "PATCH") else None

        if path == "/__fixture/update.json":
            return self.reply(200, {
                "version": "0.3.0", "published_at": "2026-11-10T09:00:00Z",
                "url": "https://github.com/ilhamnadhif/ternarywire/releases/tag/v0.3.0",
                "notes": "## Workbench\n- Request on the left, response on the right, with a status row that always shows code, "
                         "time, size and tests.\n- Local servers (mock, webhooks, capture) get their own sidebar section.\n\n"
                         "## Fixes\n- Large responses no longer freeze the tree view.\n- OAuth 2.0 tokens refresh before a "
                         "collection run starts.\n- gRPC reflection works with servers that only offer v1alpha.",
                "assets": []})
        if path in ("/", "/health"):
            return self.reply(200, {"status": "ok", "service": "bookstore", "version": "1.2.0"}, common)

        if parts[:1] == ["books"]:
            if len(parts) == 1:
                if method == "GET":
                    page = book_page(previous=q.get("snapshot") == ["previous"], tag=(q.get("tag") or [None])[0])
                    return self.reply(200, page, common + [
                        ("Cache-Control", "no-store"), ("ETag", 'W/"3bc-9fkq2"'), ("X-RateLimit-Remaining", "58"),
                        ("Set-Cookie", SESSION_COOKIE)])
                if method == "POST":
                    b = body if isinstance(body, dict) else {}
                    created = {"id": 6, **b, "createdAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                    return self.reply(201, created, common + [("Location", "/books/6")])
            elif len(parts) == 2:
                try:
                    bid = int(parts[1])
                except ValueError:
                    bid = -1
                book = BOOKS.get(bid)
                if not book:
                    return self.reply(404, {"error": "not_found", "message": "No book with id %s" % urllib.parse.unquote(parts[1])}, common)
                if method == "GET":
                    return self.reply(200, book, common + [("Cache-Control", "max-age=60"), ("ETag", 'W/"b-%d"' % bid)])
                if method in ("PUT", "PATCH"):
                    return self.reply(200, {**book, **(body if isinstance(body, dict) else {}), "updatedAt": "2026-11-11T09:41:07Z"}, common)
                if method == "DELETE":
                    return self.reply(204, None, common)
        if path == "/auth/login" and method == "POST":
            return self.reply(200, {"token": FAKE_JWT, "tokenType": "Bearer", "expiresIn": 3600,
                                    "user": {"id": ME["id"], "name": ME["name"]}}, common + [("Set-Cookie", SESSION_COOKIE)])
        if path == "/me":
            return self.reply(200, ME, common + [("Cache-Control", "private, no-cache")])
        if parts[:1] == ["orders"]:
            if len(parts) == 1 and method == "GET":
                return self.reply(200, {"data": [{"id": 14, "status": "paid", "total": 94.99}, {"id": 13, "status": "shipped", "total": 12}]}, common)
            if len(parts) == 1 and method == "POST":
                items = (body or {}).get("items", []) if isinstance(body, dict) else []
                total = round(sum(BOOKS.get(int(i.get("bookId", 0)), {"price": 0})["price"] * int(i.get("qty", 1)) for i in items
                                  if isinstance(i, dict)), 2)
                return self.reply(201, {"id": 15, "status": "pending", "items": items, "total": total}, common + [("Location", "/orders/15")])
            if len(parts) == 2:
                status = (body or {}).get("status", "paid") if isinstance(body, dict) else "paid"
                return self.reply(200, {"id": int(parts[1]) if parts[1].isdigit() else parts[1], "status": status,
                                        "updatedAt": "2026-11-11T09:41:07Z"}, common)
        if path == "/graphql" and method in ("POST", "GET"):
            query = (body or {}).get("query", "") if isinstance(body, dict) else (q.get("query") or [""])[0]
            if "__schema" in query:
                return self.reply(200, gql_introspection(), common)
            if "createBook" in query:
                return self.reply(200, {"data": {"createBook": {**BOOKS[4], "id": "6", "title": "Bumi Manusia"}}}, common)
            return self.reply(200, {"data": {"search": {"totalCount": 2, "nodes": [
                {"id": "4", "title": "Rust in Action", "author": {"name": "Tim McNamara"}, "price": 39.99},
                {"id": "7", "title": "Programming Rust", "author": {"name": "Jim Blandy"}, "price": 49.5}]}}}, common)
        if parts[:2] == ["v1", "charges"] and method == "POST":
            return self.reply(201, {"id": "ch_3QK9vLJ2eZvKYlo2", "object": "charge", "status": "succeeded",
                                    **(body if isinstance(body, dict) else {})}, common)
        if parts[:2] == ["v1", "customers"]:
            return self.reply(200, {"object": "list", "data": [{"id": "cus_Q1", "email": "reader@bookstore.dev"},
                                                               {"id": "cus_Q2", "email": "ilham@example.com"}], "has_more": False}, common)
        if parts[:2] == ["v1", "webhooks"]:
            return self.reply(200, {"received": True}, common)
        if parts[:1] == ["payments"]:
            return self.reply(201 if method == "POST" else 200, {"id": "pay_3QK9vL", "status": "succeeded"}, common)
        return self.reply(404, {"error": "not_found", "message": "No route for %s %s" % (method, path)}, common)


# --- OpenAI-compatible chat completions --------------------------------------

AI_ANSWER = ("Two hands-on picks under $45:\n\n"
             "1. **Rust in Action** — Tim McNamara, $39.99, in stock. Builds real tools (a CPU emulator, a key-value store) "
             "chapter by chapter.\n"
             "2. **Hands-on Rust** — Herbert Wolverson, $42.00, 2 left. Teaches the language by writing a small game.\n\n"
             "Both assume you know another language. Want me to place an order?")
AI_SUMMARY = "Ilham, the catalogue has 5 books and 4 are in stock. Laskar Pelangi is the cheapest at 12.00."
MODELS = ["gpt-5-mini", "gpt-5", "gpt-4.1-mini", "llama3.2"]


class AiHandler(Base):
    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            return self.reply(200, {"object": "list", "data": [{"id": m, "object": "model", "created": 1762851667,
                                                                "owned_by": "fixture"} for m in MODELS]})
        self.reply(404, {"error": {"message": "not found", "type": "invalid_request_error"}})

    def do_POST(self):
        if not self.path.rstrip("/").endswith("/chat/completions"):
            return self.reply(404, {"error": {"message": "Unknown path %s" % self.path, "type": "invalid_request_error"}})
        req = self.body_json() or {}
        messages = req.get("messages") or []
        model = req.get("model") or "gpt-5-mini"
        text_in = " ".join(m.get("content") if isinstance(m.get("content"), str) else jdump(m.get("content"))
                           for m in messages if isinstance(m, dict))
        has_tool_result = any(isinstance(m, dict) and m.get("role") == "tool" for m in messages)
        tools = [t.get("function", {}).get("name") for t in req.get("tools") or [] if isinstance(t, dict)]
        call = None
        if "search_books" in tools and not has_tool_result:
            call = {"id": "call_search_books_1", "type": "function",
                    "function": {"name": "search_books", "arguments": jdump({"query": "rust", "maxPrice": 45})}}
            text, finish = "", "tool_calls"
        elif "catalog" in text_in.lower() and "summar" in text_in.lower():
            text, finish = AI_SUMMARY, "stop"
        elif has_tool_result or "rust" in text_in.lower():
            text, finish = AI_ANSWER, "stop"
        else:
            text, finish = "Hello from the fixture model. Ask me about the Bookstore catalogue.", "stop"
        usage = {"prompt_tokens": 96, "completion_tokens": 412 if finish == "stop" else 24, "total_tokens": 508}
        cid = "chatcmpl-fixture-%d" % random.randint(1000, 9999)
        if not req.get("stream"):
            msg = {"role": "assistant", "content": text or None}
            if call:
                msg["tool_calls"] = [call]
            return self.reply(200, {"id": cid, "object": "chat.completion", "created": int(time.time()), "model": model,
                                    "choices": [{"index": 0, "message": msg, "finish_reason": finish}], "usage": usage})
        self.start_chunked("text/event-stream")

        def event(delta, finish_reason=None, with_usage=False):
            ev = {"id": cid, "object": "chat.completion.chunk", "created": int(time.time()), "model": model,
                  "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}]}
            if with_usage:
                ev["choices"] = []
                ev["usage"] = usage
            self.chunk("data: %s\n\n" % jdump(ev))

        try:
            time.sleep(0.35)  # time to first token
            event({"role": "assistant", "content": ""})
            if call:
                event({"tool_calls": [{"index": 0, "id": call["id"], "type": "function",
                                       "function": {"name": "search_books", "arguments": ""}}]})
                args = call["function"]["arguments"]
                for i in range(0, len(args), 12):
                    event({"tool_calls": [{"index": 0, "function": {"arguments": args[i:i + 12]}}]})
            else:
                words = text.split(" ")
                for i in range(0, len(words), 4):
                    event({"content": " ".join(words[i:i + 4]) + (" " if i + 4 < len(words) else "")})
                    time.sleep(0.03)
            event({}, finish)
            event({}, None, with_usage=True)
            self.chunk("data: [DONE]\n\n")
            self.end_chunked()
        except (BrokenPipeError, ConnectionResetError):
            pass


# --- MCP Streamable HTTP -----------------------------------------------------

def schema(props, required=()):
    return {"type": "object", "properties": props, "required": list(required)}


MCP_TOOLS = [
    ("search_books", "Search books", "Search the catalogue by title, author or tag and return matching books.",
     schema({"query": {"type": "string", "description": "Words to look for"}, "limit": {"type": "integer", "default": 10},
             "maxPrice": {"type": "number"}}, ["query"])),
    ("get_book", "Get book", "Fetch one book with its stock and price history.", schema({"id": {"type": "integer"}}, ["id"])),
    ("create_order", "Create order", "Place an order for one or more books for the signed-in user.",
     schema({"bookIds": {"type": "array", "items": {"type": "integer"}}}, ["bookIds"])),
    ("list_orders", "List orders", "Orders of the signed-in user, newest first.", schema({"limit": {"type": "integer"}})),
    ("recommend", "Recommend", "Suggest books similar to the given ones.", schema({"bookIds": {"type": "array", "items": {"type": "integer"}}})),
    ("update_stock", "Update stock", "Change the stock count of a book (admin only).",
     schema({"id": {"type": "integer"}, "stock": {"type": "integer"}}, ["id", "stock"])),
    ("list_authors", "List authors", "Authors in the catalogue with their number of books.", schema({})),
    ("get_author", "Get author", "One author with their books.", schema({"name": {"type": "string"}}, ["name"])),
    ("check_stock", "Check stock", "Stock level of a book in every warehouse.", schema({"id": {"type": "integer"}}, ["id"])),
    ("cancel_order", "Cancel order", "Cancel an order that has not shipped yet.", schema({"orderId": {"type": "integer"}}, ["orderId"])),
    ("add_review", "Add review", "Add a review of a book by the signed-in user.",
     schema({"id": {"type": "integer"}, "stars": {"type": "integer"}, "text": {"type": "string"}}, ["id", "stars"])),
    ("list_reviews", "List reviews", "Reviews of a book, best first.", schema({"id": {"type": "integer"}}, ["id"])),
]
MCP_RESOURCES = [
    {"uri": "book://catalog", "name": "Catalogue", "title": "Catalogue", "description": "Catalogue as JSON, refreshed every minute.",
     "mimeType": "application/json"},
    {"uri": "book://stock.csv", "name": "Stock levels", "title": "Stock levels", "mimeType": "text/csv"},
    {"uri": "file:///srv/README.md", "name": "Readme", "title": "Readme", "mimeType": "text/markdown"},
]
MCP_TEMPLATES = [{"uriTemplate": "book://books/{id}", "name": "Book by id", "title": "Book by id", "mimeType": "application/json"}]
MCP_PROMPTS = [
    {"name": "summarize_catalogue", "title": "Summarize catalogue", "description": "Short summary of what is in stock.",
     "arguments": [{"name": "audience", "required": False}]},
    {"name": "write_blurb", "title": "Write blurb", "description": "Back-cover blurb for a book.",
     "arguments": [{"name": "topic", "required": True}, {"name": "audience", "required": False}]},
    {"name": "compare_books", "title": "Compare books", "description": "Compare two books for a reader.",
     "arguments": [{"name": "first", "required": True}, {"name": "second", "required": True}]},
]
ELICIT_SCHEMA = {"type": "object", "properties": {
    "warehouse": {"type": "string", "title": "Warehouse", "enum": ["JKT-2", "SUB-1", "SIN-1"],
                  "enumNames": ["Jakarta (JKT-2)", "Surabaya (SUB-1)", "Singapore (SIN-1)"], "default": "JKT-2"},
    "quantity": {"type": "integer", "title": "Quantity", "minimum": 1, "default": 2},
    "giftNote": {"type": "string", "title": "Gift note"},
    "giftWrap": {"type": "boolean", "title": "Gift wrap", "description": "Wrap each book", "default": True}},
    "required": ["warehouse", "quantity"]}
MCP_PENDING = {}  # JSON-RPC id of a request sent to the client -> [Event, response]
MCP_LOCK = threading.Lock()


def tool_result(name, args):
    if name == "search_books":
        q = str(args.get("query", "")).lower()
        found = [b for b in BOOKS.values() if q in b["title"].lower() or q in " ".join(b["tags"])] or list(BOOKS.values())[:2]
        if q == "rust":
            found = [{"id": 4, "title": "Rust in Action", "price": 39.99, "inStock": True},
                     {"id": 7, "title": "Programming Rust", "price": 49, "stock": 3}]
            text = "Found 2 books for “rust”: Rust in Action (39.99, in stock) and Programming Rust (49.00, 3 left)."
        else:
            text = "Found %d books for “%s”." % (len(found), q)
        return {"content": [{"type": "text", "text": text}], "structuredContent": {"books": found, "total": len(found)}}
    if name == "get_book":
        b = BOOKS.get(int(args.get("id", 0) or 0))
        if not b:
            return {"content": [{"type": "text", "text": "No book with id %s" % args.get("id")}], "isError": True}
        return {"content": [{"type": "text", "text": jdump(b)}], "structuredContent": b}
    return {"content": [{"type": "text", "text": "%s done." % name}]}


class McpHandler(Base):
    def do_GET(self):
        # No server-initiated stream outside of requests.
        self.reply(405, {"error": "use POST"}, [("Allow", "POST, DELETE")])

    def do_DELETE(self):
        self.reply(200, None)

    def do_POST(self):
        if urllib.parse.urlsplit(self.path).path.rstrip("/") != "/mcp":
            return self.reply(404, {"error": "the MCP endpoint is /mcp"})
        msg = self.body_json()
        if isinstance(msg, list):  # batch: answer each request in a JSON array
            out = [r for r in (self.handle_one(m, sse=False) for m in msg) if r is not None]
            return self.reply(200, out) if out else self.reply(202, None)
        if not isinstance(msg, dict):
            return self.reply(400, {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "Parse error"}})
        if "method" not in msg:  # the client answering a request of ours
            with MCP_LOCK:
                slot = MCP_PENDING.get(str(msg.get("id")))
            if slot:
                slot[1] = msg
                slot[0].set()
            return self.reply(202, None)
        sse = "text/event-stream" in (self.headers.get("Accept") or "")
        res = self.handle_one(msg, sse)
        if res == "streamed":
            return None
        if res is None:
            return self.reply(202, None)
        headers = []
        if msg.get("method") == "initialize":
            headers.append(("Mcp-Session-Id", uuid.uuid4().hex))
        return self.reply(200, res, headers)

    def handle_one(self, msg, sse):
        method, mid, params = msg.get("method"), msg.get("id"), msg.get("params") or {}
        if mid is None:  # notification
            return None

        def ok(result):
            return {"jsonrpc": "2.0", "id": mid, "result": result}

        if method == "initialize":
            return ok({"protocolVersion": params.get("protocolVersion") or "2025-06-18",
                       "capabilities": {"tools": {"listChanged": False}, "resources": {"subscribe": False}, "prompts": {},
                                        "logging": {}, "completions": {}},
                       "serverInfo": {"name": "bookstore-tools", "version": "1.4.0"},
                       "instructions": "Use search_books before recommending; prices are in USD."})
        if method == "ping" or method == "logging/setLevel":
            return ok({})
        if method == "tools/list":
            return ok({"tools": [{"name": n, "title": t, "description": d, "inputSchema": s} for n, t, d, s in MCP_TOOLS]})
        if method == "resources/list":
            return ok({"resources": MCP_RESOURCES})
        if method == "resources/templates/list":
            return ok({"resourceTemplates": MCP_TEMPLATES})
        if method == "resources/read":
            uri = params.get("uri", "")
            text = jdump(book_page()) if uri == "book://catalog" else "id,stock\n1,4\n2,7\n3,0\n4,12\n" if uri.endswith(".csv") \
                else "# Bookstore tools\n\nTools for the Bookstore catalogue."
            return ok({"contents": [{"uri": uri, "mimeType": "application/json" if uri == "book://catalog" else "text/plain", "text": text}]})
        if method == "prompts/list":
            return ok({"prompts": MCP_PROMPTS})
        if method == "prompts/get":
            a = params.get("arguments") or {}
            return ok({"description": "Prompt from the fixture", "messages": [{"role": "user", "content": {
                "type": "text", "text": "Write a short blurb about %s for a %s reader." % (a.get("topic", "books"), a.get("audience", "curious"))}}]})
        if method == "completion/complete":
            return ok({"completion": {"values": ["rust", "novel", "data"], "total": 3, "hasMore": False}})
        if method == "tools/call":
            name, args = params.get("name"), params.get("arguments") or {}
            if not sse:
                return ok(tool_result(name, args))
            self.stream_call(mid, name, args)
            return "streamed"
        return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "Method not found: %s" % method}}

    def stream_call(self, mid, name, args):
        """Answers tools/call as an event stream: a log message first, and for
        create_order an elicitation request the client must answer."""
        self.start_chunked("text/event-stream")
        n = [0]

        def send(obj):
            n[0] += 1
            self.chunk("id: %d\nevent: message\ndata: %s\n\n" % (n[0], jdump(obj)))

        try:
            send({"jsonrpc": "2.0", "method": "notifications/message", "params": {
                "level": "info", "logger": "bookstore", "data": "querying index books_v3 (2 shards)"}})
            if name == "create_order":
                eid = "elicit-%s" % uuid.uuid4().hex[:8]
                slot = [threading.Event(), None]
                with MCP_LOCK:
                    MCP_PENDING[eid] = slot
                send({"jsonrpc": "2.0", "id": eid, "method": "elicitation/create", "params": {
                    "message": "Which warehouse should the order ship from? See the shipping policy.",
                    "requestedSchema": ELICIT_SCHEMA}})
                # Waits for the answer (the mcp-elicit screenshot is taken meanwhile).
                for _ in range(600):
                    if slot[0].wait(0.5):
                        break
                    self.chunk(": waiting\n\n")
                with MCP_LOCK:
                    MCP_PENDING.pop(eid, None)
                answer = (slot[1] or {}).get("result") or {}
                content = answer.get("content") or {}
                if answer.get("action") != "accept":
                    result = {"content": [{"type": "text", "text": "Order cancelled: no warehouse chosen."}], "isError": True}
                else:
                    result = {"content": [{"type": "text", "text": "Order 15 placed from %s." % content.get("warehouse", "JKT-2")}],
                              "structuredContent": {"orderId": 15, "status": "pending", **content}}
            else:
                time.sleep(0.08)
                result = tool_result(name, args)
            send({"jsonrpc": "2.0", "id": mid, "result": result})
            self.end_chunked()
        except (BrokenPipeError, ConnectionResetError):
            pass


# ---------------------------------------------------------------------------
# WebSocket echo (RFC 6455, stdlib sockets)
# ---------------------------------------------------------------------------

WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def recv_exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("closed")
        buf += chunk
    return buf


class WsConn:
    def __init__(self, sock):
        self.sock = sock
        self.lock = threading.Lock()
        self.open = True

    def send(self, opcode, payload):
        if isinstance(payload, str):
            payload = payload.encode()
        n = len(payload)
        head = bytes([0x80 | opcode])
        if n < 126:
            head += bytes([n])
        elif n < 65536:
            head += bytes([126]) + struct.pack("!H", n)
        else:
            head += bytes([127]) + struct.pack("!Q", n)
        with self.lock:
            if self.open:
                self.sock.sendall(head + payload)

    def recv(self):
        """(opcode, payload) of the next whole message."""
        data, first = b"", None
        while True:
            b1, b2 = recv_exact(self.sock, 2)
            fin, op = b1 & 0x80, b1 & 0x0F
            n = b2 & 0x7F
            if n == 126:
                n = struct.unpack("!H", recv_exact(self.sock, 2))[0]
            elif n == 127:
                n = struct.unpack("!Q", recv_exact(self.sock, 8))[0]
            mask = recv_exact(self.sock, 4) if b2 & 0x80 else b"\0\0\0\0"
            payload = bytes(c ^ mask[i % 4] for i, c in enumerate(recv_exact(self.sock, n)))
            if op >= 8:  # control frames may come between fragments
                return op, payload
            if first is None:
                first = op
            data += payload
            if fin:
                return first, data


class WsHandler(socketserver.BaseRequestHandler):
    def handle(self):
        sock = self.request
        raw = b""
        while b"\r\n\r\n" not in raw:
            chunk = sock.recv(4096)
            if not chunk:
                return
            raw += chunk
            if len(raw) > 65536:
                return
        head = raw.split(b"\r\n\r\n", 1)[0].decode("latin-1").split("\r\n")
        headers = {k.strip().lower(): v.strip() for k, v in (l.split(":", 1) for l in head[1:] if ":" in l)}
        key = headers.get("sec-websocket-key")
        if not key or "websocket" not in headers.get("upgrade", "").lower():
            sock.sendall(b"HTTP/1.1 426 Upgrade Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            return
        accept = base64.b64encode(hashlib.sha1((key + WS_GUID).encode()).digest()).decode()
        lines = ["HTTP/1.1 101 Switching Protocols", "Upgrade: websocket", "Connection: Upgrade", "Sec-WebSocket-Accept: " + accept]
        protos = [p.strip() for p in headers.get("sec-websocket-protocol", "").split(",") if p.strip()]
        if protos:
            lines.append("Sec-WebSocket-Protocol: " + protos[0])
        sock.sendall(("\r\n".join(lines) + "\r\n\r\n").encode())
        log("ws %s connected %s" % (WS_PORT, head[0]))
        conn = WsConn(sock)
        try:
            while True:
                op, payload = conn.recv()
                if op == 8:
                    conn.send(8, payload[:2] or struct.pack("!H", 1000))
                    return
                if op == 9:
                    conn.send(10, payload)
                    continue
                if op == 10:
                    continue
                if op == 2:
                    conn.send(2, payload)
                    continue
                self.on_text(conn, payload.decode("utf-8", "replace"))
        except (ConnectionError, OSError):
            pass
        finally:
            conn.open = False

    def on_text(self, conn, text):
        if text.strip() == "ping":
            return conn.send(1, "pong")
        try:
            msg = json.loads(text)
        except ValueError:
            msg = None
        if isinstance(msg, dict) and msg.get("type") == "subscribe":
            channel = msg.get("channel")
            if channel == "orders":
                return conn.send(1, jdump({"type": "error", "message": "Error: channel “orders” needs the orders:read scope"}))
            conn.send(1, jdump({"type": "subscribed", "channel": channel}))
            if channel == "books":
                def push():
                    for delay, ev in ((1.5, {"type": "book.added", "id": 6, "title": "Bumi Manusia", "author": "Pramoedya Ananta Toer",
                                             "price": 15.5}),
                                      (2.5, {"type": "stock", "id": 3, "inStock": True}),
                                      (2.5, {"type": "book.updated", "id": 2, "price": 52})):
                        time.sleep(delay)
                        try:
                            conn.send(1, jdump(ev))
                        except OSError:
                            return
                threading.Thread(target=push, daemon=True).start()
            return None
        return conn.send(1, text)


class TcpServer(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = True


# ---------------------------------------------------------------------------
# MQTT 3.1.1 broker
# ---------------------------------------------------------------------------

class Broker:
    def __init__(self):
        self.lock = threading.Lock()
        self.clients = {}  # MqttClient -> {filter: qos}
        self.retained = {}  # topic -> (payload, qos)
        self.sessions = set()  # client ids with a persistent session

    @staticmethod
    def matches(flt, topic):
        f, t = flt.split("/"), topic.split("/")
        if topic.startswith("$") and f[0] in ("#", "+"):
            return False
        for i, part in enumerate(f):
            if part == "#":
                return True
            if i >= len(t) or (part != "+" and part != t[i]):
                return False
        return len(f) == len(t)

    def publish(self, topic, payload, qos, retain, sender=None):
        if retain:
            with self.lock:
                if payload:
                    self.retained[topic] = (payload, qos)
                else:
                    self.retained.pop(topic, None)
        with self.lock:
            targets = [(c, max(q for f, q in subs.items() if self.matches(f, topic)))
                       for c, subs in self.clients.items() if any(self.matches(f, topic) for f in subs)]
        for c, sub_qos in targets:
            c.deliver(topic, payload, min(qos, sub_qos), False)


BROKER = Broker()


def mqtt_str(b, i):
    n = struct.unpack("!H", b[i:i + 2])[0]
    return b[i + 2:i + 2 + n], i + 2 + n


def mqtt_enc(s):
    if isinstance(s, str):
        s = s.encode()
    return struct.pack("!H", len(s)) + s


def mqtt_packet(kind_flags, body):
    n, out = len(body), b""
    while True:
        d, n = n % 128, n // 128
        out += bytes([d | (0x80 if n else 0)])
        if not n:
            break
    return bytes([kind_flags]) + out + body


class MqttClient(socketserver.BaseRequestHandler):
    def setup(self):
        self.wlock = threading.Lock()
        self.next_id = 1
        self.will = None
        self.cid = "?"

    def send(self, data):
        with self.wlock:
            self.request.sendall(data)

    def deliver(self, topic, payload, qos, retain):
        flags = 0x30 | (qos << 1) | (1 if retain else 0)
        body = mqtt_enc(topic)
        if qos:
            self.next_id = self.next_id % 65535 + 1
            body += struct.pack("!H", self.next_id)
        try:
            self.send(mqtt_packet(flags, body + payload))
        except OSError:
            pass

    def read_packet(self):
        first = recv_exact(self.request, 1)[0]
        mult, n = 1, 0
        while True:
            d = recv_exact(self.request, 1)[0]
            n += (d & 0x7F) * mult
            mult *= 128
            if not d & 0x80:
                break
        return first, recv_exact(self.request, n) if n else b""

    def handle(self):
        clean_exit = False
        try:
            first, body = self.read_packet()
            if first >> 4 != 1:
                return
            name, i = mqtt_str(body, 0)
            level, flags = body[i], body[i + 1]
            i += 4  # level, flags, keep alive
            if level not in (3, 4):
                # MQTT 5 is not supported: "unacceptable protocol version".
                self.send(mqtt_packet(0x20, bytes([0, 1])))
                return
            cid, i = mqtt_str(body, i)
            self.cid = cid.decode() or "auto-%s" % uuid.uuid4().hex[:8]
            if flags & 0x04:
                wt, i = mqtt_str(body, i)
                wm, i = mqtt_str(body, i)
                self.will = (wt.decode(), wm, (flags >> 3) & 3, bool(flags & 0x20))
            clean = bool(flags & 0x02)
            with BROKER.lock:
                present = (not clean) and self.cid in BROKER.sessions
                if clean:
                    BROKER.sessions.discard(self.cid)
                else:
                    BROKER.sessions.add(self.cid)
                BROKER.clients[self] = {}
            self.send(mqtt_packet(0x20, bytes([1 if present else 0, 0])))
            log("mqtt %s connected %s (%s)" % (MQTT_PORT, self.cid, name.decode()))
            while True:
                first, body = self.read_packet()
                kind = first >> 4
                if kind == 3:  # PUBLISH
                    qos, retain = (first >> 1) & 3, bool(first & 1)
                    topic, i = mqtt_str(body, 0)
                    pid = None
                    if qos:
                        pid = body[i:i + 2]
                        i += 2
                    if qos == 1:
                        self.send(mqtt_packet(0x40, pid))
                    elif qos == 2:
                        self.send(mqtt_packet(0x50, pid))
                    BROKER.publish(topic.decode(), body[i:], qos, retain, self)
                elif kind == 6:  # PUBREL
                    self.send(mqtt_packet(0x70, body[:2]))
                elif kind == 5:  # PUBREC of our QoS 2 delivery
                    self.send(mqtt_packet(0x62, body[:2]))
                elif kind == 8:  # SUBSCRIBE
                    pid, i, granted, new = body[:2], 2, b"", []
                    while i < len(body):
                        flt, i = mqtt_str(body, i)
                        q = min(body[i] & 3, 2)
                        i += 1
                        granted += bytes([q])
                        new.append((flt.decode(), q))
                    with BROKER.lock:
                        BROKER.clients[self].update(dict(new))
                        retained = list(BROKER.retained.items())
                    self.send(mqtt_packet(0x90, pid + granted))
                    for t, (payload, q) in retained:
                        for flt, sq in new:
                            if Broker.matches(flt, t):
                                self.deliver(t, payload, min(q, sq), True)
                                break
                elif kind == 10:  # UNSUBSCRIBE
                    pid, i = body[:2], 2
                    with BROKER.lock:
                        while i < len(body):
                            flt, i = mqtt_str(body, i)
                            BROKER.clients[self].pop(flt.decode(), None)
                    self.send(mqtt_packet(0xB0, pid))
                elif kind == 12:  # PINGREQ
                    self.send(bytes([0xD0, 0]))
                elif kind == 14:  # DISCONNECT
                    clean_exit = True
                    return
        except (ConnectionError, OSError, IndexError, struct.error):
            pass
        finally:
            with BROKER.lock:
                BROKER.clients.pop(self, None)
            if self.will and not clean_exit:
                BROKER.publish(self.will[0], self.will[1], self.will[2], self.will[3])


def sensor_simulator():
    BROKER.publish("status/kitchen", b"online", 0, True)
    BROKER.publish("status/garage", b"online", 0, True)
    n = 0
    while True:
        time.sleep(5)
        n += 1
        now = time.strftime("%H:%M:%S")
        BROKER.publish("sensors/kitchen/temp", jdump({"c": round(21.4 + 0.1 * (n % 6), 1), "at": now}).encode(), 1, False)
        if n % 3 == 0:
            BROKER.publish("sensors/garage/humidity", jdump({"rh": 60 + n % 7}).encode(), 1, False)


# ---------------------------------------------------------------------------
# gRPC forwarder (the example server picks its own port)
# ---------------------------------------------------------------------------

def forwarder(listen_host, listen_port, target):
    family = socket.AF_INET6 if ":" in listen_host else socket.AF_INET
    srv = socket.socket(family, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((listen_host, listen_port))
    srv.listen(64)

    def pump(a, b):
        try:
            while True:
                data = a.recv(65536)
                if not data:
                    break
                b.sendall(data)
        except OSError:
            pass
        finally:
            for s in (a, b):
                try:
                    s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass

    def serve():
        while True:
            client, _ = srv.accept()
            try:
                up = socket.create_connection(target, timeout=5)
                up.settimeout(None)
            except OSError:
                client.close()
                continue
            threading.Thread(target=pump, args=(client, up), daemon=True).start()
            threading.Thread(target=pump, args=(up, client), daemon=True).start()

    threading.Thread(target=serve, daemon=True).start()


# ---------------------------------------------------------------------------
# Process management
# ---------------------------------------------------------------------------

def port_open(port, host="127.0.0.1"):
    try:
        with socket.create_connection((host, port), timeout=0.5):
            return True
    except OSError:
        return False


def read_pids():
    try:
        with open(PIDFILE) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except (OSError, TypeError):
        return False


def target_dir():
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit("cargo metadata failed: " + out.stderr)
    return json.loads(out.stdout)["target_directory"]


def build_examples():
    env = dict(os.environ)
    env.setdefault("CARGO_BUILD_JOBS", "4")
    for pkg, ex in (("ternarywire-socketio", "echo_server"), ("ternarywire-grpc", "greeter_server")):
        print("server.py: building %s example %s (first time takes a while)…" % (pkg, ex), file=sys.stderr, flush=True)
        r = subprocess.run(["cargo", "build", "-q", "-p", pkg, "--example", ex], cwd=ROOT, env=env)
        if r.returncode != 0:
            raise SystemExit("cargo build of %s failed" % ex)
    t = target_dir()
    return os.path.join(t, "debug", "examples", "echo_server"), os.path.join(t, "debug", "examples", "greeter_server")


def run(sio_bin, grpc_bin):
    """Foreground: every server of this file plus the two example servers."""
    os.makedirs(STATE, exist_ok=True)
    children = []

    def stop(*_):
        for c in children:
            try:
                c.terminate()
            except OSError:
                pass
        try:
            if (read_pids() or {}).get("pid") == os.getpid():
                os.remove(PIDFILE)
        except OSError:
            pass
        os._exit(0)

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)

    for port, handler in ((HTTP_PORT, BookstoreHandler), (AI_PORT, AiHandler), (MCP_PORT, McpHandler)):
        srv = Server(("127.0.0.1", port), handler)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
    for port, handler in ((WS_PORT, WsHandler), (MQTT_PORT, MqttClient)):
        srv = TcpServer(("127.0.0.1", port), handler)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
    threading.Thread(target=sensor_simulator, daemon=True).start()

    if sio_bin and os.path.exists(sio_bin):
        children.append(subprocess.Popen([sio_bin, str(SIO_PORT)], stdout=sys.stdout, stderr=sys.stderr, stdin=subprocess.DEVNULL))
    if grpc_bin and os.path.exists(grpc_bin):
        g = subprocess.Popen([grpc_bin], stdout=subprocess.PIPE, stderr=sys.stderr, stdin=subprocess.DEVNULL, text=True)
        children.append(g)
        url = g.stdout.readline().strip()  # http://127.0.0.1:PORT
        port = int(url.rsplit(":", 1)[1])
        log("greeter_server on %s" % url)
        forwarder("127.0.0.1", GRPC_PORT, ("127.0.0.1", port))
        try:  # "localhost" may resolve to ::1 first
            forwarder("::1", GRPC_PORT, ("127.0.0.1", port))
        except OSError:
            pass
    with open(PIDFILE, "w") as f:
        json.dump({"pid": os.getpid(), "children": [c.pid for c in children], "root": ROOT, "started": time.time()}, f)
    log("servers running: " + ", ".join(str(p) for _, p, _ in PORTS))
    while True:
        time.sleep(3600)


def status(verbose=True):
    info = read_pids()
    ok = True
    for name, port, label in PORTS:
        up = port_open(port)
        ok = ok and up
        if verbose:
            print("  %-5s %-6s %s" % ("up" if up else "DOWN", port, label))
    if verbose:
        if info and alive(info.get("pid")):
            print("  pid %s (from %s), log %s" % (info["pid"], info.get("root"), LOGFILE))
        else:
            print("  not started by server.py (log %s)" % LOGFILE)
    return ok


def stop():
    info = read_pids()
    if not info:
        print("server.py: not running")
        return
    for pid in info.get("children", []) + [info.get("pid")]:
        try:
            os.kill(pid, signal.SIGTERM)
        except (OSError, TypeError):
            pass
    for _ in range(50):
        if not alive(info.get("pid")):
            break
        time.sleep(0.1)
    try:
        os.remove(PIDFILE)
    except OSError:
        pass
    print("server.py: stopped")


def start():
    os.makedirs(STATE, exist_ok=True)
    with open(LOCKFILE, "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)  # one start at a time, across worktrees
        if status(verbose=False):
            return
        info = read_pids()
        if info and alive(info.get("pid")):
            stop()  # running but incomplete: start afresh
        busy = [p for _, p, _ in PORTS if port_open(p)]
        if busy:
            raise SystemExit("server.py: ports %s are in use by another program; stop it or `server.py stop`" % busy)
        sio_bin, grpc_bin = build_examples()
        logf = open(LOGFILE, "a")
        subprocess.Popen([sys.executable, os.path.abspath(__file__), "run", sio_bin, grpc_bin], stdout=logf, stderr=logf,
                         stdin=subprocess.DEVNULL, start_new_session=True, cwd=STATE)
        for _ in range(300):
            if status(verbose=False):
                break
            time.sleep(0.1)
        else:
            status()
            raise SystemExit("server.py: servers did not come up; see %s" % LOGFILE)


# ---------------------------------------------------------------------------
# Sample traffic for the webhook listener and the capture proxy (shoot.sh)
# ---------------------------------------------------------------------------

def wait_port(port, timeout):
    end = time.time() + timeout
    while time.time() < end:
        if port_open(port):
            return True
        time.sleep(0.2)
    return False


STRIPE_EVENTS = [
    ("/stripe/webhook?attempt=2", "customer.created", "evt_1QK9u2J2eZvKYlo2Lq8wE4tB", {"id": "cus_Q1", "email": "reader@bookstore.dev"}),
    ("/stripe/webhook", "charge.succeeded", "evt_3QK9v7J2eZvKYlo20pQ1mN2s", {"id": "ch_3QK9vLJ2eZvKYlo2", "amount": 9499}),
    ("/stripe/webhook", "payment_intent.succeeded", "evt_3QK9vLJ2eZvKYlo21Ab4Xc7d", {"id": "pi_3QK9vLJ2eZvKYlo2", "amount": 9499,
                                                                                   "currency": "usd", "status": "succeeded"}),
]


def poke(kind, port, timeout):
    if not wait_port(port, timeout):
        raise SystemExit("poke: nothing listens on %d" % port)
    time.sleep(0.5)
    if kind == "webhook":
        for path, typ, eid, obj in STRIPE_EVENTS:
            body = jdump({"id": eid, "object": "event", "api_version": "2025-10-29", "created": int(time.time()), "type": typ,
                          "livemode": False, "data": {"object": obj}}).encode()
            req = urllib.request.Request("http://127.0.0.1:%d%s" % (port, path), data=body, method="POST", headers={
                "User-Agent": "Stripe/1.0 (+https://stripe.com/docs/webhooks)", "Content-Type": "application/json; charset=utf-8",
                "Stripe-Signature": "t=%d,v1=%s" % (int(time.time()), hashlib.sha256(body).hexdigest())})
            try:
                urllib.request.urlopen(req, timeout=5).read()
            except OSError as e:
                print("poke: %s" % e, file=sys.stderr)
            time.sleep(0.3)
    else:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({"http": "http://127.0.0.1:%d" % port}))
        base = "http://127.0.0.1:%d" % HTTP_PORT
        for method, path, body in (("GET", "/books?page=1&limit=20", None), ("POST", "/books", {"title": "Bumi Manusia", "price": 15.5}),
                                   ("GET", "/me", None), ("PATCH", "/orders/14", {"status": "shipped"}), ("GET", "/books/99", None),
                                   ("GET", "/health", None)):
            data = jdump(body).encode() if body else None
            headers = {"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_6) BookstoreWeb/1.4", "Accept": "application/json"}
            if data:
                headers["Content-Type"] = "application/json"
            req = urllib.request.Request(base + path, data=data, method=method, headers=headers)
            try:
                opener.open(req, timeout=5).read()
            except urllib.error.HTTPError:
                pass
            except OSError as e:
                print("poke: %s" % e, file=sys.stderr)
            time.sleep(0.2)


def main():
    args = sys.argv[1:]
    cmd = args[0] if args else "status"
    if cmd == "start":
        start()
        status()
    elif cmd == "stop":
        stop()
    elif cmd == "status":
        sys.exit(0 if status() else 1)
    elif cmd == "run":
        if len(args) >= 3:
            run(args[1], args[2])
        else:
            run(*build_examples())
    elif cmd == "poke" and len(args) >= 2:
        port = int(args[args.index("--port") + 1]) if "--port" in args else (8787 if args[1] == "webhook" else 8888)
        poke(args[1], port, timeout=float(os.environ.get("POKE_TIMEOUT", "90")))
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
