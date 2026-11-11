//! The "Bookstore" workspace of the redesign prototype (`design/redesign/`),
//! written with the `ternarywire_core` API so its format always matches the
//! app. Used by `design/redesign/fixture/shoot.sh` to screenshot the real app
//! in the same state as each prototype screen.
//!
//! ```text
//! TW_FIXTURE_OUT=/tmp/tw-fixture cargo test -p ternarywire-app --test design_fixture -- --ignored
//! ```
//!
//! Writes `$TW_FIXTURE_OUT/workspace/` (the workspace, with its git-ignored
//! `.ternarywire/` state: active environment, current values, secrets in
//! `secrets.json` for `TERNARYWIRE_SECRETS=file`, OAuth tokens, history,
//! cookies and the open tabs), `$TW_FIXTURE_OUT/config/` (a
//! `TERNARYWIRE_CONFIG_DIR` whose `TernaryWire/settings.json` opens that
//! workspace with Local active) and `$TW_FIXTURE_OUT/fixture.json` (ids and
//! the reference day; `shoot.sh` renews the token expiry and moves the
//! history dates per run).
//!
//! The sample data mirrors `data.js`, `screens-http.js`, `screens-proto.js`,
//! `screens-work.js` and `screens-flow.js`. URLs point at the servers of
//! `design/redesign/fixture/server.py`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use ternarywire_core::auth::{Auth, AuthType};
use ternarywire_core::environment::Environment;
use ternarywire_core::history::{HistoryEntry, HistoryStore};
use ternarywire_core::model::*;
use ternarywire_core::protocols::*;
use ternarywire_core::secrets::{FileBackend, SecretStore};
use ternarywire_core::settings::{AppSettings, ResponseLayout, ThemePreference};
use ternarywire_core::storage::WorkspaceStore;
use ternarywire_core::tokens::{OAuth2Token, TokenStore};
use ternarywire_core::variables::Variable;

// Stable ids: steps, the session file and flows refer to them.
const C_BOOK: &str = "coll-bookstore";
const C_RT: &str = "coll-realtime";
const C_PAY: &str = "coll-payments";
const R_LOGIN: &str = "req-login";
const R_ME: &str = "req-current-user";
const R_LIST: &str = "req-list-books";
const R_GET: &str = "req-get-book";
const R_CREATE: &str = "req-create-book";
const R_UPDATE: &str = "req-update-book";
const R_DELETE: &str = "req-delete-book";
const R_SEARCH: &str = "req-search-catalog";
const R_ORDER: &str = "req-place-order";
const R_STATUS: &str = "req-order-status";
const R_WS: &str = "req-echo-socket";
const R_SIO: &str = "req-chat-events";
const R_MQTT: &str = "req-sensor-stream";
const R_GRPC: &str = "req-greeter";
const R_MCP: &str = "req-tools-server";
const R_AI: &str = "req-assistant";
const E_LOCAL: &str = "env-local";
const E_STAGING: &str = "env-staging";
const E_PROD: &str = "env-production";

/// Local servers of `server.py` (and the repo's example servers).
const HTTP: &str = "http://127.0.0.1:18795";
const AI_BASE: &str = "http://127.0.0.1:18796/v1";
const MCP_URL: &str = "http://127.0.0.1:8931/mcp";
const WS_BASE: &str = "ws://127.0.0.1:18900";
const SIO_URL: &str = "http://127.0.0.1:18902";
const MQTT_URL: &str = "mqtt://127.0.0.1:1883";
const GRPC_URL: &str = "localhost:50051";

/// The prototype's "today" (`data.js`): history dates are written for this
/// day and `shoot.sh` moves them to the day of the run.
const REFERENCE_DAY: &str = "2026-11-11";

/// A fake but well-formed JWT (header.payload.signature), never a real token.
const FAKE_JWT: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1XzE5MiIsInNjb3BlIjoiYm9va3M6cmVhZCBib29rczp3cml0ZSBwcm9maWxlIiwiaXNzIjoiaHR0cHM6Ly9hdXRoLmJvb2tzdG9yZS5kZXYifQ.c2lnbmF0dXJlLW9mLXRoZS1kZXNpZ24tZml4dHVyZQ";

#[test]
#[ignore = "writes the design fixture to $TW_FIXTURE_OUT (design/redesign/fixture/shoot.sh)"]
fn make_fixture() {
    let out = PathBuf::from(std::env::var("TW_FIXTURE_OUT").expect("set TW_FIXTURE_OUT to the output folder"));
    let root = out.join("workspace");
    let config = out.join("config");
    for dir in [&root, &config] {
        if dir.exists() {
            std::fs::remove_dir_all(dir).expect("clear the old fixture");
        }
    }
    std::fs::create_dir_all(&config).unwrap();

    let (store, _) = WorkspaceStore::create(&root, "Bookstore").expect("create the workspace");
    let secrets = SecretStore::new(Box::new(FileBackend { path: root.join(".ternarywire").join("secrets.json") }));

    store.save_collection(&bookstore(), &secrets).expect("save Bookstore API");
    store.save_collection(&realtime(), &secrets).expect("save Realtime & AI");
    store.save_collection(&payments(), &secrets).expect("save Payments");
    for env in environments() {
        store.save_environment(&env, &secrets).expect("save an environment");
    }
    store.save_globals(&globals(), &secrets).expect("save the globals");
    store.set_active_environment(Some(E_LOCAL)).expect("activate Local");

    for (name, value) in [
        ("bookstore-token", FAKE_JWT),
        ("bookstore-client", "bk-client-secret-1b8f2a"),
        ("bookstore-pass", "correct-horse-battery"),
        ("openai", "sk-fixture-not-a-real-key"),
        ("chat-token", "chat-token-7f2e9c1a"),
        ("mcp-token", "mcp-token-3d44b1e0"),
        ("mqtt-pass", "sensor-pass"),
    ] {
        secrets.vault_set(name, value);
    }
    // "Token active · 52 min": shoot.sh renews `expires_at` on every run.
    let now = chrono::Utc::now().timestamp();
    for owner in [C_BOOK, R_ME] {
        let token = OAuth2Token {
            access_token: FAKE_JWT.into(),
            token_type: "Bearer".into(),
            refresh_token: Some("rt-fixture-9fkq2".into()),
            id_token: None,
            expires_at: Some(now + 52 * 60 + 30),
            scope: Some("books:read books:write profile".into()),
            obtained_at: now - 8 * 60,
            name: "Bookstore token".into(),
        };
        TokenStore::set(&secrets, owner, Some(token));
    }

    write_specs(&root);
    write_flows(&root);
    write_data(&root);
    write_packages(&root);
    write_history(&root);
    write_cookies(&root);
    write_session(&root);

    let mut settings = AppSettings::default();
    settings.ui.theme = ThemePreference::Light;
    settings.ui.layout = ResponseLayout::Side;
    settings.ui.sidebar_visible = true;
    settings.updates.check_on_startup = true;
    // Requests go straight to the local servers, whatever the Mac's proxy is.
    settings.proxy.use_system = false;
    settings.request.send_no_cache = true;
    settings.recent_workspaces = vec![root.clone()];
    // AppSettings::config_dir() is $TERNARYWIRE_CONFIG_DIR/TernaryWire.
    settings.save(&config.join("TernaryWire").join("settings.json")).expect("write settings.json");

    let meta = json!({
        "workspace": root,
        "config": config,
        "reference_day": REFERENCE_DAY,
        "oauth_owners": [C_BOOK, R_ME],
        "ids": {
            "collections": {"Bookstore API": C_BOOK, "Realtime & AI": C_RT, "Payments": C_PAY},
            "environments": {"Local": E_LOCAL, "Staging": E_STAGING, "Production": E_PROD},
        },
    });
    std::fs::write(out.join("fixture.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();

    // Read it back the way the app does, so a format slip fails here.
    let (_, ws) = WorkspaceStore::open(&root, &secrets).expect("reopen the fixture");
    assert!(ws.load_errors.is_empty(), "load errors: {:?}", ws.load_errors);
    assert_eq!(ws.collections.len(), 3);
    assert_eq!(ws.active_environment.as_deref(), Some(E_LOCAL));
    let (flows, errors) = ternarywire_flows::storage::list(&root);
    assert!(errors.is_empty(), "flow errors: {errors:?}");
    assert_eq!(flows.len(), 3);
    eprintln!("design fixture written to {}", out.display());
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, value)
}

fn kvd(key: &str, value: &str, description: &str) -> KeyValue {
    KeyValue { description: description.into(), ..KeyValue::new(key, value) }
}

fn var(key: &str, value: &str) -> Variable {
    Variable::new(key, value)
}

fn var_cur(key: &str, value: &str, current: &str) -> Variable {
    Variable { current: Some(current.into()), ..Variable::new(key, value) }
}

fn request(id: &str, name: &str, kind: RequestKind) -> RequestItem {
    RequestItem {
        id: id.into(),
        name: name.into(),
        description: String::new(),
        request: kind,
        scripts: Scripts::default(),
        examples: Vec::new(),
    }
}

fn http(id: &str, name: &str, method: &str, url: &str) -> RequestItem {
    request(id, name, RequestKind::Http(HttpRequest::new(method, url)))
}

fn with_post(mut r: RequestItem, post: &str) -> RequestItem {
    r.scripts.post_response = post.into();
    r
}

fn http_mut(r: &mut RequestItem) -> &mut HttpRequest {
    r.http_mut().expect("an HTTP request")
}

fn folder(id: &str, name: &str, items: Vec<RequestItem>) -> Item {
    Item::Folder(Folder { id: id.into(), items: items.into_iter().map(Item::Request).collect(), ..Folder::new(name) })
}

fn pretty(v: Value) -> String {
    serde_json::to_string_pretty(&v).unwrap()
}

/// A JSON example response (served by the mock server).
fn example(id: &str, name: &str, status: u16, status_text: &str, body: Option<Value>) -> Example {
    Example {
        id: id.into(),
        name: name.into(),
        request: None,
        status,
        status_text: status_text.into(),
        headers: vec![kv("Content-Type", "application/json; charset=utf-8")],
        body: body.map(pretty).unwrap_or_default(),
        language: Some("json".into()),
    }
}

fn json_headers() -> Vec<KeyValue> {
    vec![kv("Content-Type", "application/json")]
}

fn oauth2() -> Auth {
    Auth::new(AuthType::OAuth2)
        .with("grant_type", "authorization_code_with_pkce")
        .with("authUrl", "https://auth.bookstore.dev/authorize")
        .with("accessTokenUrl", "https://auth.bookstore.dev/token")
        .with("clientId", "{{clientId}}")
        .with("clientSecret", "{{vault:bookstore-client}}")
        .with("scope", "books:read books:write profile")
        .with("redirect_uri", "http://127.0.0.1:7777/callback")
        .with("addTokenTo", "header")
        .with("tokenName", "Bookstore token")
        .with("autoRefresh", true)
}

// ---------------------------------------------------------------------------
// Collection "Bookstore API"
// ---------------------------------------------------------------------------

const LIST_POST: &str = r#"pm.test("Status code is 200", () => {
  pm.response.to.have.status(200);
});

pm.test("Every book has a title", () => {
  const { data } = pm.response.json();
  pm.expect(data.every((b) => b.title)).to.be.true;
});

const left = pm.response.json().total - pm.response.json().data.length;
console.log("pages left:", Math.ceil(left / 20));"#;

const GQL_QUERY: &str = r#"query Search($q: String!, $first: Int = 10) {
  search(query: $q, first: $first) {
    totalCount
    nodes {
      id
      title
      author { name }
      price
    }
  }
}"#;

fn bookstore() -> Collection {
    let mut login = with_post(
        http(R_LOGIN, "Login", "POST", "{{baseUrl}}/auth/login"),
        "pm.test(\"Returns a token\", () => {\n  pm.expect(pm.response.json().token).to.be.a(\"string\");\n});",
    );
    {
        let h = http_mut(&mut login);
        h.auth = Auth::none();
        h.headers = json_headers();
        h.body = Body::raw_json(pretty(json!({"username": "{{user}}", "password": "{{vault:bookstore-pass}}"})));
    }
    login.description = "Signs in and returns an access token. Uses no auth itself.".into();

    let mut me = with_post(
        http(R_ME, "Current user", "GET", "{{baseUrl}}/me"),
        "pm.test(\"Status code is 200\", () => {\n  pm.response.to.have.status(200);\n});\n\npm.test(\"Returns the signed-in user\", () => {\n  pm.expect(pm.response.json().id).to.equal(\"u_192\");\n});",
    );
    {
        let h = http_mut(&mut me);
        h.headers = vec![kv("Accept", "application/json")];
        // Its own OAuth 2.0 (the http-auth screen shows the form, not "Inherit").
        h.auth = oauth2();
    }

    let mut list = with_post(
        http(R_LIST, "List books", "GET", "{{baseUrl}}/books?page=1&limit={{pageSize}}&sort=title"),
        LIST_POST,
    );
    {
        let h = http_mut(&mut list);
        for p in &mut h.params {
            p.description = match p.key.as_str() {
                "page" => "Page number, starting at 1".into(),
                "sort" => "title | price | author".into(),
                _ => String::new(),
            };
        }
        h.params.push(kvd("inStock", "true", "Only books in stock").disabled());
        h.headers = vec![
            kv("Accept", "application/json"),
            kvd("X-Client", "ternarywire/{{$appVersion}}", "Sent to the access log"),
        ];
    }
    list.description = "Lists books, newest first. Supports paging with `page` and `limit`, and sorting by `title`, `price` or `author`.\n\nRequires the `books:read` scope.".into();
    list.examples = vec![
        Example {
            id: "ex-list-200".into(),
            name: "Page of books".into(),
            request: None,
            status: 200,
            status_text: "OK".into(),
            headers: vec![kv("Content-Type", "application/json; charset=utf-8")],
            body: pretty(json!({"data": [{"id": 1, "title": "The Pragmatic Programmer"}], "total": 1})),
            language: Some("json".into()),
        },
        Example {
            id: "ex-list-404".into(),
            name: "Unknown page".into(),
            request: None,
            status: 404,
            status_text: "Not Found".into(),
            headers: vec![kv("Content-Type", "application/json; charset=utf-8")],
            body: pretty(json!({"error": "not_found", "message": "Page 9 does not exist"})),
            language: Some("json".into()),
        },
    ];

    let mut get = with_post(
        http(R_GET, "Get book", "GET", "{{baseUrl}}/books/:id"),
        "pm.test(\"Book exists\", () => {\n  pm.response.to.have.status(200);\n});",
    );
    {
        let h = http_mut(&mut get);
        h.path_vars = vec![kvd("id", "{{bookId}}", "Book id; the runner's data file sets it per iteration")];
        h.headers = vec![kv("Accept", "application/json")];
    }

    let mut create = with_post(
        http(R_CREATE, "Create book", "POST", "{{baseUrl}}/books"),
        "pm.test(\"Status code is 201\", () => {\n  pm.response.to.have.status(201);\n});",
    );
    {
        let h = http_mut(&mut create);
        h.headers = json_headers();
        h.body = Body::raw_json(
            "{\n  \"title\": \"Bumi Manusia\",\n  \"author\": \"Pramoedya Ananta Toer\",\n  \"price\": 15.5,\n  \"inStock\": true,\n  \"tags\": [\"novel\", \"classic\"],\n  \"publisherId\": \"{{publisherId}}\"\n}",
        );
    }

    let mut update = http(R_UPDATE, "Update book", "PUT", "{{baseUrl}}/books/:id");
    {
        let h = http_mut(&mut update);
        h.path_vars = vec![kv("id", "2")];
        h.headers = json_headers();
        h.body = Body::raw_json(pretty(json!({"price": 52, "inStock": true})));
    }

    let mut delete = http(R_DELETE, "Delete book", "DELETE", "{{baseUrl}}/books/:id");
    http_mut(&mut delete).path_vars = vec![kv("id", "3")];

    let mut search = http(R_SEARCH, "Search catalog", "POST", "{{baseUrl}}/graphql");
    http_mut(&mut search).body = Body {
        mode: BodyMode::GraphQl,
        graphql: GraphQlBody {
            query: GQL_QUERY.into(),
            variables: "{ \"q\": \"rust\", \"first\": 5 }".into(),
            operation_name: None,
        },
        ..Default::default()
    };

    let mut order = http(R_ORDER, "Place order", "POST", "{{baseUrl}}/orders");
    {
        let h = http_mut(&mut order);
        h.headers = json_headers();
        h.body = Body::raw_json(pretty(
            json!({"items": [{"bookId": 2, "qty": 1}, {"bookId": 4, "qty": 1}], "shipTo": "Jakarta"}),
        ));
    }
    let mut status = http(R_STATUS, "Order status", "PATCH", "{{baseUrl}}/orders/14");
    {
        let h = http_mut(&mut status);
        h.headers = json_headers();
        h.body = Body::raw_json(pretty(json!({"status": "shipped"})));
    }

    // The mock server's routes (mock-routes): one example per request, two on Get book.
    login.examples = vec![example(
        "ex-login-200",
        "Signed in",
        200,
        "OK",
        Some(json!({"token": "eyJhbGciOiJIUzI1NiJ9.mock", "expiresIn": 3600})),
    )];
    me.examples = vec![example(
        "ex-me-200",
        "Profile",
        200,
        "OK",
        Some(json!({"id": "u_192", "name": "Ilham", "email": "ilham@example.com"})),
    )];
    get.examples = vec![
        example("ex-get-200", "One book", 200, "OK", Some(json!({"id": 2, "title": "Clean Code", "price": 42.5}))),
        example("ex-get-404", "Missing book", 404, "Not Found", Some(json!({"error": "not_found"}))),
    ];
    create.examples = vec![example(
        "ex-create-201",
        "Created",
        201,
        "Created",
        Some(json!({"id": 6, "title": "Bumi Manusia", "price": 15.5})),
    )];
    update.examples =
        vec![example("ex-update-200", "Updated", 200, "OK", Some(json!({"id": 2, "price": 52, "inStock": true})))];
    delete.examples = vec![example("ex-delete-204", "Deleted", 204, "No Content", None)];

    let mut c = Collection::new("Bookstore API");
    c.id = C_BOOK.into();
    c.description = "# Bookstore API\n\nBrowse books, place orders and manage your account.\n\n## Authentication\nRequests inherit **OAuth 2.0** (Authorization Code with PKCE).\nRun *Auth › Login* once to get a token; it is renewed automatically.\n\n## Environments\n- **Local**: `http://127.0.0.1:18795`\n- **Staging**: `https://api.staging.bookstore.dev`".into();
    c.auth = oauth2();
    c.variables = vec![
        var("baseUrl", HTTP),
        var_cur("pageSize", "20", "50"),
        var("clientId", "bookstore-desktop"),
        Variable::secret("clientSecret", "bk-client-secret-1b8f2a"),
        var("bookId", "2"),
    ];
    c.scripts = Scripts {
        pre_request: "// Runs before every request in “Bookstore API”\nif (!pm.environment.get(\"token\")) {\n  console.log(\"no token yet — run Auth › Login first\");\n}\npm.request.headers.upsert({ key: \"X-Request-Start\", value: String(Date.now()) });".into(),
        post_response: "pm.test(\"No server errors\", () => {\n  pm.expect(pm.response.code).to.be.below(500);\n});".into(),
        on_message: String::new(),
    };
    c.items = vec![
        folder("fld-auth", "Auth", vec![login, me]),
        folder("fld-books", "Books", vec![list, get, create, update, delete, search]),
        folder("fld-orders", "Orders", vec![order, status]),
    ];
    c
}

// ---------------------------------------------------------------------------
// Collection "Realtime & AI"
// ---------------------------------------------------------------------------

fn realtime() -> Collection {
    let mut ws = request(
        R_WS,
        "Echo socket",
        RequestKind::WebSocket(WsRequest {
            url: "{{wsUrl}}/live?room=books&v=2".into(),
            params: vec![kv("room", "books"), kv("v", "2")],
            headers: vec![kv("Origin", "http://localhost:5173"), kv("X-Client", "ternarywire")],
            subprotocols: vec!["graphql-transport-ws".into()],
            messages: vec![
                WsSavedMessage {
                    id: "ws-msg-subscribe".into(),
                    name: "Subscribe books".into(),
                    body: "{\"type\":\"subscribe\",\"channel\":\"books\"}".into(),
                    kind: WsMessageKind::Text,
                },
                WsSavedMessage {
                    id: "ws-msg-ping".into(),
                    name: "Ping".into(),
                    body: "ping".into(),
                    kind: WsMessageKind::Text,
                },
                WsSavedMessage {
                    id: "ws-msg-raw".into(),
                    name: "Raw frame".into(),
                    body: "01 9f 3c 00 7a".into(),
                    kind: WsMessageKind::Binary,
                },
            ],
            runner: SessionRunner { max_messages: Some(10), wait_ms: 1500, ..Default::default() },
            ..Default::default()
        }),
    );
    ws.scripts = Scripts {
        pre_request: "pm.variables.set(\"connectedAt\", Date.now());".into(),
        post_response: String::new(),
        on_message: "pm.test(\"Echo returns the same payload\", () => {\n  pm.expect(pm.message.text()).to.not.include('\"type\":\"error\"');\n});".into(),
    };

    let mut sio = request(
        R_SIO,
        "Chat events",
        RequestKind::SocketIo(SocketIoRequest {
            url: SIO_URL.into(),
            namespace: "/chat".into(),
            headers: vec![kv("Authorization", "Bearer {{vault:chat-token}}")],
            auth_payload: "{ \"token\": \"{{vault:chat-token}}\" }".into(),
            listeners: vec![
                SocketIoListener { event: "tick".into(), enabled: true },
                SocketIoListener { event: "join".into(), enabled: true },
                SocketIoListener { event: "upload".into(), enabled: false },
            ],
            messages: vec![
                SocketIoMessage {
                    id: "sio-msg-hello".into(),
                    name: "Say hello".into(),
                    event: "message".into(),
                    arguments: vec![SocketIoArg::new(
                        SocketIoArgKind::Json,
                        "{ \"room\": \"books\", \"text\": \"hello\", \"user\": \"{{userId}}\" }",
                    )],
                    ack: true,
                    ..Default::default()
                },
                SocketIoMessage {
                    id: "sio-msg-join".into(),
                    name: "Join room".into(),
                    event: "join".into(),
                    arguments: vec![SocketIoArg::new(SocketIoArgKind::Json, "{ \"room\": \"books\" }")],
                    ..Default::default()
                },
                SocketIoMessage {
                    id: "sio-msg-upload".into(),
                    name: "Upload cover".into(),
                    event: "upload".into(),
                    arguments: vec![
                        SocketIoArg::new(SocketIoArgKind::Json, "{ \"name\": \"cover.jpg\" }"),
                        SocketIoArg::new(SocketIoArgKind::Binary, "ffd8ffe000104a464946"),
                    ],
                    ..Default::default()
                },
            ],
            runner: SessionRunner { ack_reply: Some("[\"ok\"]".into()), max_messages: Some(10), ..Default::default() },
            ..Default::default()
        }),
    );
    sio.scripts.on_message =
        "if (pm.message.event === \"tick\") {\n  pm.test(\"tick counts up\", () => {\n    pm.expect(pm.message.args[0].count).to.be.above(0);\n  });\n}".into();

    let mut mqtt = request(
        R_MQTT,
        "Sensor stream",
        RequestKind::Mqtt(MqttRequest {
            url: MQTT_URL.into(),
            // server.py is an MQTT 3.1.1 broker (the prototype shows MQTT 5).
            version: MqttVersion::V311,
            keep_alive_secs: 60,
            clean_session: false,
            subscriptions: vec![
                MqttSubscription { topic: "sensors/#".into(), qos: 1, enabled: true },
                MqttSubscription { topic: "status/+".into(), qos: 0, enabled: true },
                MqttSubscription { topic: "alerts/critical".into(), qos: 2, enabled: false },
            ],
            messages: vec![
                MqttMessage {
                    id: "mqtt-msg-target".into(),
                    name: "Target 22 °C".into(),
                    topic: "sensors/kitchen/target".into(),
                    payload: "{\"c\":22,\"by\":\"{{clientId}}\"}".into(),
                    qos: 1,
                    retain: true,
                    user_properties: vec![kv("unit", "celsius"), kv("source", "thermostat-2")],
                },
                MqttMessage {
                    id: "mqtt-msg-reboot".into(),
                    name: "Reboot".into(),
                    topic: "cmd/reboot".into(),
                    payload: "now".into(),
                    ..Default::default()
                },
            ],
            last_will: Some(MqttMessage {
                id: "mqtt-will".into(),
                topic: "clients/ternarywire/status".into(),
                payload: "offline".into(),
                qos: 1,
                retain: true,
                ..Default::default()
            }),
            auth: Auth::basic("{{mqttUser}}", "{{vault:mqtt-pass}}"),
            runner: SessionRunner { max_messages: Some(5), wait_ms: 1500, ..Default::default() },
            ..Default::default()
        }),
    );
    mqtt.scripts.on_message = "if (pm.message.topic === \"sensors/kitchen/temp\") {\n  const t = pm.message.json().c;\n  pm.test(\"temperature is plausible\", () => pm.expect(t).to.be.within(-20, 60));\n}".into();

    let mut grpc = request(
        R_GRPC,
        "Greeter",
        RequestKind::Grpc(GrpcRequest {
            url: GRPC_URL.into(),
            service: "helloworld.Greeter".into(),
            method: "SayHello".into(),
            schema: GrpcSchemaSource::Reflection,
            metadata: vec![
                kvd("x-request-id", "{{$guid}}", "Traced in the gateway"),
                kvd("trace-bin", "AAECAw==", "base64"),
            ],
            message: pretty(json!({"name": "{{userName}}", "mood": "HAPPY", "tags": ["vip"]})),
            deadline_ms: Some(5000),
            ..Default::default()
        }),
    );
    grpc.scripts.post_response = "pm.test(\"status is OK\", () => pm.expect(pm.response.code).to.equal(0));".into();

    let mut mcp = request(
        R_MCP,
        "Tools server",
        RequestKind::Mcp(McpRequest {
            transport: McpTransport::Http {
                url: MCP_URL.into(),
                headers: vec![kv("Authorization", "Bearer {{vault:mcp-token}}")],
            },
            saved_calls: vec![
                McpSavedCall {
                    id: "mcp-call-search".into(),
                    name: "Search rust".into(),
                    kind: McpCallKind::Tool,
                    target: "search_books".into(),
                    arguments: "{\"query\":\"rust\",\"limit\":5}".into(),
                },
                McpSavedCall {
                    id: "mcp-call-catalog".into(),
                    name: "Catalogue".into(),
                    kind: McpCallKind::Resource,
                    target: "book://catalog".into(),
                    arguments: String::new(),
                },
            ],
            auth: Auth::none(),
            http_transport: McpHttpTransport::Auto,
        }),
    );
    mcp.scripts.post_response = "const result = pm.response.json();\nif (result && result.structuredContent) {\n  pm.test(\"found books\", () => {\n    pm.expect(result.structuredContent.books.length).to.be.above(0);\n  });\n}".into();

    let mut ai = request(
        R_AI,
        "Assistant",
        RequestKind::Ai(AiRequest {
            provider: AiProvider::OpenAi,
            base_url: AI_BASE.into(),
            model: "gpt-5-mini".into(),
            system: "You are the Bookstore assistant. Use the tools to look up real stock and prices. Answer in short paragraphs.".into(),
            messages: vec![
                AiMessage { role: "user".into(), content: "What do you have for someone learning Rust?".into() },
                AiMessage {
                    role: "assistant".into(),
                    content: "I can check the catalogue. Do you prefer hands-on projects or a reference book?".into(),
                },
                AiMessage {
                    role: "user".into(),
                    content: "Hands-on, and keep it under $45. Recommend two and say if they are in stock.".into(),
                },
            ],
            temperature: Some(0.7),
            max_tokens: None,
            stream: true,
            extra: "{ \"reasoning\": { \"effort\": \"low\" } }".into(),
            mcp_servers: vec![R_MCP.into()],
            auth: Auth::bearer("{{vault:openai}}"),
        }),
    );
    ai.scripts.post_response =
        "pm.test(\"mentions a price\", () => {\n  pm.expect(pm.response.text()).to.match(/\\$\\d+/);\n});".into();

    let mut c = Collection::new("Realtime & AI");
    c.id = C_RT.into();
    c.description = "WebSocket, Socket.IO, MQTT, gRPC, MCP and AI requests against the local fixture servers.".into();
    c.auth = Auth::bearer("{{vault:chat-token}}");
    c.variables = vec![
        var("wsUrl", WS_BASE),
        Variable::secret("wsToken", "ws-token-91c0f2d7"),
        var("userName", "Ada"),
        var("userId", "u_192"),
        var("clientId", "ternarywire-desktop"),
        var("mqttUser", "sensor-reader"),
    ];
    c.items = [ws, sio, mqtt, grpc, mcp, ai].into_iter().map(Item::Request).collect();
    c
}

// ---------------------------------------------------------------------------
// Collection "Payments" (forked by the `fork` automation step)
// ---------------------------------------------------------------------------

fn payments() -> Collection {
    // Names of the prototype's fork dialog (dialogs.js "fork-merge").
    let mut charge = with_post(
        http("req-create-charge", "Create charge", "POST", "{{baseUrl}}/v1/charges"),
        "pm.test(\"Charge created\", () => pm.response.to.have.status(201));",
    );
    {
        let h = http_mut(&mut charge);
        h.headers = json_headers();
        h.body = Body::raw_json(pretty(json!({"orderId": 14, "amount": 9499, "currency": "usd"})));
    }
    let mut customers = http("req-list-customers", "List customers", "GET", "{{baseUrl}}/v1/customers?limit=10");
    http_mut(&mut customers).headers = vec![kv("Accept", "application/json")];
    let mut legacy = http("req-legacy-webhook", "Legacy webhook", "POST", "{{baseUrl}}/v1/webhooks/legacy");
    {
        let h = http_mut(&mut legacy);
        h.headers = json_headers();
        h.body = Body::raw_json(pretty(json!({"type": "charge.succeeded", "id": "evt_legacy_1"})));
    }
    let mut c = Collection::new("Payments");
    c.id = C_PAY.into();
    c.description = "Charges and customers of Bookstore orders.".into();
    c.auth = Auth::bearer("{{vault:bookstore-token}}");
    c.items = [charge, customers, legacy].into_iter().map(Item::Request).collect();
    c
}

// ---------------------------------------------------------------------------
// Environments and globals (env screen, quick look)
// ---------------------------------------------------------------------------

fn environments() -> Vec<Environment> {
    let env =
        |id: &str, name: &str, vars: Vec<Variable>| Environment { id: id.into(), name: name.into(), variables: vars };
    let legacy = || Variable { enabled: false, ..var("legacyHost", "http://old.bookstore.dev") };
    vec![
        env(
            E_LOCAL,
            "Local",
            vec![
                var("baseUrl", HTTP),
                var_cur("pageSize", "20", "5"),
                var("user", "ilham@example.com"),
                Variable::secret("token", FAKE_JWT),
                legacy(),
                var_cur("publisherId", "pub_{{$randomInt}}", "pub_812"),
            ],
        ),
        env(
            E_STAGING,
            "Staging",
            vec![
                var("baseUrl", "https://api.staging.bookstore.dev"),
                var("pageSize", "20"),
                var("user", "reader@bookstore.dev"),
                Variable::secret("token", "staging-token-5b1e"),
                legacy(),
                var("publisherId", "pub_staging"),
            ],
        ),
        env(
            E_PROD,
            "Production",
            vec![
                var("baseUrl", "https://api.bookstore.dev"),
                var("pageSize", "50"),
                var("user", "reader@bookstore.dev"),
                Variable::secret("token", "production-token-c7a0"),
                legacy(),
                var("publisherId", "pub_live"),
                var("rateLimit", "120"),
            ],
        ),
    ]
}

fn globals() -> Vec<Variable> {
    vec![
        var("timeout", "30000"),
        var("region", "ap-southeast-1"),
        Variable::secret("webhookSecret", "whsec_fixture_4b1e"),
    ]
}

// ---------------------------------------------------------------------------
// specs/ (Spec Hub)
// ---------------------------------------------------------------------------

/// The first lines are the prototype's `SPEC` (screens-work.js) verbatim; the
/// rest completes its five operations.
const BOOKSTORE_YAML: &str = r#"openapi: 3.0.3
info:
  title: Bookstore API
  version: 1.2.0
  description: Browse books, place orders and manage your account.
servers:
  - url: http://127.0.0.1:18795
paths:
  /books:
    get:
      summary: List books
      tags: [Books]
      parameters:
        - name: page
          in: query
          description: Page number, starting at 1
          schema: { type: integer, example: 1 }
        - name: tag
          in: query
          description: Only books with this tag
          schema: { type: string, example: classic }
      responses:
        "200":
          description: A page of books
    post:
      summary: Create a book
      tags: [Books]
      requestBody:
        content:
          application/json:
            example: { title: Rust in Action, price: 39.99 }
      responses:
        "201":
          description: Created
  /books/{id}:
    get:
      summary: Get a book
      tags: [Books]
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: integer, example: 2 }
      responses:
        "200":
          description: One book
        "404":
          description: No book with this id
    delete:
      summary: Delete a book
      tags: [Books]
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: integer, example: 3 }
      responses:
        "204":
          description: Deleted
  /auth/login:
    post:
      summary: Log in
      tags: [Auth]
      requestBody:
        content:
          application/json:
            example: { username: reader@bookstore.dev, password: secret }
      responses:
        "200":
          description: Signed in
"#;

const USERS_SMITHY: &str = r#"$version: "2"

namespace dev.bookstore.users

/// Reader accounts of the Bookstore.
service Users {
    version: "2026-11-01"
    operations: [GetUser, ListUsers]
}

@readonly
@http(method: "GET", uri: "/users/{userId}")
operation GetUser {
    input := {
        @required
        @httpLabel
        userId: String
    }
    output := {
        @required
        userId: String
        name: String
        email: String
    }
}

@readonly
@http(method: "GET", uri: "/users")
operation ListUsers {
    input := {
        @httpQuery("page")
        page: Integer
    }
    output := {
        users: UserList
    }
}

list UserList {
    member: UserSummary
}

structure UserSummary {
    @required
    userId: String

    name: String
}
"#;

const CHAT_ASYNCAPI: &str = r#"asyncapi: 3.0.0
info:
  title: Bookstore chat
  version: 1.0.0
  description: Socket.IO events of the Bookstore support chat.
servers:
  local:
    host: 127.0.0.1:18902
    protocol: ws
channels:
  chat:
    address: /chat
    messages:
      message:
        payload:
          type: object
          properties:
            room: { type: string }
            text: { type: string }
      tick:
        payload:
          type: object
          properties:
            count: { type: integer }
operations:
  sendMessage:
    action: send
    channel:
      $ref: '#/channels/chat'
    messages:
      - $ref: '#/channels/chat/messages/message'
  receiveTick:
    action: receive
    channel:
      $ref: '#/channels/chat'
    messages:
      - $ref: '#/channels/chat/messages/tick'
"#;

const GREETER_PROTO: &str = r#"syntax = "proto3";

package helloworld;

// The greeting service of the local test server (localhost:50051).
service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc LotsOfReplies (HelloRequest) returns (stream HelloReply);
}

message HelloRequest {
  string name = 1;
  repeated string tags = 4;
}

message HelloReply {
  string message = 1;
  int32 index = 2;
}
"#;

const SCHEMA_GRAPHQL: &str = r#""A book in the catalogue."
type Book {
  id: ID!
  title: String!
  author: Author!
  price: Float!
  inStock: Boolean!
  tags: [String!]!
}

type Author {
  name: String!
}

type User {
  id: ID!
  name: String!
  email: String!
}

type SearchResult {
  totalCount: Int!
  nodes: [Book!]!
}

input BookInput {
  title: String!
  author: String!
  price: Float!
}

type Query {
  search(query: String!, first: Int = 10): SearchResult!
  book(id: ID!): Book
  books(page: Int, limit: Int): [Book!]!
  me: User
}

type Mutation {
  createBook(input: BookInput!): Book!
  deleteBook(id: ID!): Boolean!
}

type Subscription {
  bookAdded: Book!
}
"#;

fn write_specs(root: &Path) {
    let dir = root.join("specs");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in [
        ("bookstore.yaml", BOOKSTORE_YAML),
        ("users.smithy", USERS_SMITHY),
        ("chat.asyncapi.yaml", CHAT_ASYNCAPI),
        ("greeter.proto", GREETER_PROTO),
        ("schema.graphql", SCHEMA_GRAPHQL),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
}

// ---------------------------------------------------------------------------
// flows/
// ---------------------------------------------------------------------------

fn flow_from(v: Value) -> ternarywire_flows::model::Flow {
    serde_json::from_value(v).expect("a valid flow")
}

fn saved(request: &str) -> Value {
    json!({"source": {"saved": {"collection_id": C_BOOK, "request_id": request}}})
}

fn write_flows(root: &Path) {
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/flows/login-list-summarize.flow.json");
    let base: Value = serde_json::from_str(&std::fs::read_to_string(&testdata).unwrap()).unwrap();
    let block = |id: &str| base["blocks"].as_array().unwrap().iter().find(|b| b["id"] == id).unwrap().clone();

    // "Catalog summary" (screens-flow.js FNODES/FEDGES): the testdata flow's
    // Start and Summarize blocks around three saved requests run in parallel.
    let mut start = block("start");
    start["position"] = json!([14.0, 200.0]);
    let mut summarize = block("summarize");
    summarize["id"] = json!("sum");
    summarize["position"] = json!([774.0, 200.0]);
    summarize["config"]["base_url"] = json!(AI_BASE);
    // Streams, so the run log shows the answer (the prototype's info line).
    summarize["config"]["stream"] = json!(true);
    summarize["config"]["system"] = json!("You summarize the Bookstore catalogue in one or two sentences.");
    summarize["config"]["prompt"] = json!(
        "Summarize the catalog for {{inputs.user}}: {{blocks.Books.output.json}}. The signed-in user: {{blocks.User.output.json}}."
    );
    let req = |id: &str, label: &str, request: &str, x: f32, y: f32| json!({"id": id, "kind": "request", "label": label, "position": [x, y], "config": saved(request)});
    let catalog = flow_from(json!({
        "version": base["version"],
        "id": "flow-catalog-summary",
        "name": "Catalog summary",
        "description": "Fetches the books, the signed-in user and one book at the same time, then asks a local model for a short summary.",
        "inputs": [
            {"name": "user", "default": "Ilham", "required": true},
            {"name": "aiModel", "default": "llama3.2"}
        ],
        "settings": {"max_steps": 1000},
        "blocks": [
            start,
            {"id": "par", "kind": "parallel", "label": "In parallel", "position": [204.0, 200.0], "config": {}},
            req("books", "Books", R_LIST, 394.0, 30.0),
            req("user", "User", R_ME, 394.0, 200.0),
            req("book", "Book", R_GET, 394.0, 370.0),
            {"id": "join", "kind": "join", "label": "Join", "position": [584.0, 200.0], "config": {}},
            summarize
        ],
        "edges": [
            {"from_block": "start", "from_port": "next", "to_block": "par", "to_port": "in"},
            {"from_block": "par", "from_port": "branches", "to_block": "books", "to_port": "in"},
            {"from_block": "par", "from_port": "branches", "to_block": "user", "to_port": "in"},
            {"from_block": "par", "from_port": "branches", "to_block": "book", "to_port": "in"},
            {"from_block": "books", "from_port": "success", "to_block": "join", "to_port": "in"},
            {"from_block": "user", "from_port": "success", "to_block": "join", "to_port": "in"},
            {"from_block": "book", "from_port": "success", "to_block": "join", "to_port": "in"},
            {"from_block": "join", "from_port": "next", "to_block": "sum", "to_port": "in"}
        ]
    }));

    // "Checkout smoke test" (12 blocks): the testdata flow pointed at the
    // bookstore endpoints, ordering every book in stock.
    let mut checkout = base.clone();
    checkout["id"] = json!("flow-checkout-smoke-test");
    checkout["name"] = json!("Checkout smoke test");
    checkout["description"] =
        json!("Logs in, lists the books, orders every book in stock and asks the model to summarize the orders.");
    checkout["inputs"] = json!([
        {"name": "username", "default": "reader@bookstore.dev", "description": "Account to log in with"},
        {"name": "password", "default": "{{vault:bookstore-pass}}"}
    ]);
    let blocks = checkout["blocks"].as_array_mut().unwrap();
    blocks.retain(|b| b["id"] != "count");
    for b in blocks.iter_mut() {
        let id = b["id"].as_str().unwrap().to_string();
        let label = match id.as_str() {
            "list" => Some("List books"),
            "each" => Some("Each book"),
            "active" => Some("In stock?"),
            "detail" => Some("Place order"),
            _ => None,
        };
        if let Some(l) = label {
            b["label"] = json!(l);
        }
        let cfg = &mut b["config"];
        match id.as_str() {
            "login" => {
                cfg["source"]["inline"]["request"]["url"] = json!("{{baseUrl}}/auth/login");
                cfg["source"]["inline"]["collection_id"] = json!(C_BOOK);
            }
            "list" => {
                cfg["source"]["inline"]["request"]["url"] = json!("{{baseUrl}}/books?page=1&limit=20");
                cfg["source"]["inline"]["collection_id"] = json!(C_BOOK);
            }
            "each" => cfg["items"] = json!("{{blocks.list.output.json.data}}"),
            "active" => cfg["left"] = json!("{{loop.item.inStock}}"),
            "detail" => {
                let r = &mut cfg["source"]["inline"]["request"];
                r["method"] = json!("POST");
                r["url"] = json!("{{baseUrl}}/orders");
                r["body"] = json!({"mode": "raw", "raw": "{\"items\": [{\"bookId\": {{loop.item.id}}, \"qty\": 1}]}"});
                cfg["source"]["inline"]["collection_id"] = json!(C_BOOK);
            }
            "summarize" => {
                cfg["base_url"] = json!(AI_BASE);
                cfg["model"] = json!("llama3.2");
            }
            _ => {}
        }
    }
    checkout["edges"].as_array_mut().unwrap().retain(|e| e["to_block"] != "count" && e["from_block"] != "count");
    let checkout = flow_from(checkout);

    // "Nightly sync" (5 blocks).
    let nightly = flow_from(json!({
        "version": base["version"],
        "id": "flow-nightly-sync",
        "name": "Nightly sync",
        "description": "Copies the stock levels from the catalogue into the orders service every night.",
        "blocks": [
            {"id": "start", "kind": "start", "label": "Start", "position": [0.0, 120.0], "config": {}},
            {"id": "books", "kind": "request", "label": "Books", "position": [220.0, 120.0], "config": saved(R_LIST)},
            {"id": "diff", "kind": "script", "label": "Diff stock", "position": [440.0, 120.0],
             "config": {"code": "const books = blocks.Books.output.json.data;\nreturn books.filter((b) => !b.inStock).map((b) => b.id);"}},
            {"id": "save", "kind": "set_variable", "label": "Remember", "position": [660.0, 120.0],
             "config": {"name": "outOfStock", "value": "{{blocks.diff.output}}"}},
            {"id": "log", "kind": "log", "label": "Log", "position": [880.0, 120.0],
             "config": {"message": "Out of stock: {{vars.outOfStock}}"}}
        ],
        "edges": [
            {"from_block": "start", "from_port": "next", "to_block": "books", "to_port": "in"},
            {"from_block": "books", "from_port": "success", "to_block": "diff", "to_port": "in"},
            {"from_block": "diff", "from_port": "success", "to_block": "save", "to_port": "in"},
            {"from_block": "save", "from_port": "next", "to_block": "log", "to_port": "in"}
        ]
    }));

    for f in [&catalog, &checkout, &nightly] {
        ternarywire_flows::storage::save(root, f).expect("save a flow");
    }
}

// ---------------------------------------------------------------------------
// Runner data file, history, cookies, open tabs
// ---------------------------------------------------------------------------

fn write_data(root: &Path) {
    let dir = root.join("data");
    std::fs::create_dir_all(&dir).unwrap();
    let mut csv = String::from("iteration,bookId,title\n");
    let ids = [1, 2, 9, 4, 1, 2, 4, 1, 2, 4, 1, 2];
    let titles = ["The Pragmatic Programmer", "Designing Data-Intensive Applications", "(missing)", "Rust in Action"];
    for (i, id) in ids.iter().enumerate() {
        let title = match id {
            1 => titles[0],
            2 => titles[1],
            4 => titles[3],
            _ => titles[2],
        };
        csv.push_str(&format!("{},{id},{title}\n", i + 1));
    }
    std::fs::write(dir.join("books.csv"), csv).unwrap();
}

/// The Package Library of Settings › Scripts: three modules of the
/// workspace and a lockfile with two pinned packages.
fn write_packages(root: &Path) {
    let dir = root.join("packages");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body) in [
        ("my-helpers", "module.exports = {\n  isoNow: () => new Date().toISOString(),\n};\n"),
        ("signing", "module.exports = {\n  sign: (body, key) => CryptoJS.HmacSHA256(body, key).toString(),\n};\n"),
        ("fixtures", "module.exports = {\n  book: { title: 'Bumi Manusia', price: 15.5 },\n};\n"),
    ] {
        std::fs::write(dir.join(format!("{name}.js")), body).unwrap();
    }
    let pin = |n: u8| format!("sha256-{}=", String::from_utf8(vec![b'A' + n; 43]).unwrap());
    let lock = json!({
        "lockfileVersion": 1,
        "packages": {
            "npm:lodash-es@4": "https://esm.sh/lodash-es@4.17.21",
            "jsr:@std/encoding": "https://esm.sh/jsr/@std/encoding@1.0.5",
        },
        "modules": {
            "https://esm.sh/lodash-es@4.17.21": pin(0),
            "https://esm.sh/lodash-es@4.17.21/es2022/lodash-es.mjs": pin(1),
            "https://esm.sh/jsr/@std/encoding@1.0.5": pin(2),
            "https://esm.sh/jsr/@std/encoding@1.0.5/es2022/base64.mjs": pin(3),
            "https://esm.sh/jsr/@std/encoding@1.0.5/es2022/hex.mjs": pin(4),
        },
    });
    std::fs::write(root.join("packages.lock.json"), serde_json::to_string_pretty(&lock).unwrap()).unwrap();
}

/// Unix milliseconds of `day` (days after REFERENCE_DAY, negative = before)
/// at `hh:mm` local time.
fn at(day: i64, hh: u32, mm: u32) -> i64 {
    use chrono::TimeZone;
    let d = chrono::NaiveDate::parse_from_str(REFERENCE_DAY, "%Y-%m-%d").unwrap() + chrono::Duration::days(day);
    chrono::Local.from_local_datetime(&d.and_hms_opt(hh, mm, 7).unwrap()).earliest().unwrap().timestamp_millis()
}

/// (day, hh, mm, method, url as edited, resolved url, status, ms, size, error)
type HistoryRow<'a> = (i64, u32, u32, &'a str, &'a str, &'a str, Option<u16>, f64, u64, Option<&'a str>);

fn write_history(root: &Path) {
    let store = HistoryStore::new(root.join(".ternarywire").join("history.jsonl"), 1000);
    // (day, hh, mm, method, url as edited, resolved url, status, ms, size, error)
    #[rustfmt::skip]
    let rows: Vec<HistoryRow> = vec![
        (-2, 10, 59, "GET", "{{baseUrl}}/health", "http://127.0.0.1:18795/health", Some(200), 2.0, 15, None),
        (-2, 11, 5, "POST", "https://api.openai.com/v1/responses", "https://api.openai.com/v1/responses", Some(200), 2411.0, 2190, None),
        (-1, 16, 40, "GET", "{{baseUrl}}/books?tag=rust", "http://127.0.0.1:18795/books?tag=rust", Some(200), 3.0, 410, None),
        (-1, 16, 44, "PATCH", "{{baseUrl}}/orders/14", "http://127.0.0.1:18795/orders/14", Some(200), 4.0, 156, None),
        (-1, 17, 2, "GET", "https://httpbin.org/delay/5", "https://httpbin.org/delay/5", None, 30001.0, 0, Some("timed out after 30 s")),
        (-1, 18, 20, "PUT", "{{baseUrl}}/books/2", "http://127.0.0.1:18795/books/2", Some(200), 5.0, 180, None),
        (0, 8, 57, "DELETE", "{{baseUrl}}/books/3", "http://127.0.0.1:18795/books/3", Some(204), 4.0, 0, None),
        (0, 9, 12, "GET", "https://api.staging.bookstore.dev/me", "https://api.staging.bookstore.dev/me", Some(401), 212.0, 61, None),
        (0, 9, 30, "POST", "{{baseUrl}}/auth/login", "http://127.0.0.1:18795/auth/login", Some(200), 31.0, 412, None),
        (0, 9, 36, "GET", "{{baseUrl}}/books/9", "http://127.0.0.1:18795/books/9", Some(404), 4.0, 64, None),
        (0, 9, 38, "POST", "{{baseUrl}}/books", "http://127.0.0.1:18795/books", Some(201), 31.0, 383, None),
        (0, 9, 41, "GET", "{{baseUrl}}/books?page=1&limit=20", "http://127.0.0.1:18795/books?page=1&limit=20", Some(200), 142.0, 956, None),
    ];
    for (i, (day, hh, mm, method, url, resolved, status, ms, size, error)) in rows.into_iter().enumerate() {
        let entry = HistoryEntry {
            id: format!("hist-{i:02}"),
            timestamp: at(day, hh, mm),
            request: HttpRequest::new(method, url),
            resolved_url: resolved.into(),
            status,
            elapsed_ms: ms,
            size,
            error: error.map(str::to_string),
            source_request_id: None,
            name: None,
        };
        store.append(&entry).unwrap();
    }
}

fn write_cookies(root: &Path) {
    let jar = ternarywire_http::CookieJar::open(root.join(".ternarywire").join("cookies.json"));
    for (domain, cookie) in [
        ("api.staging.bookstore.dev", "session=s%3Ab2f9c1e0d4a7f3b29a; Path=/; HttpOnly; SameSite=Lax"),
        ("api.staging.bookstore.dev", "csrftoken=Xk29fJq0aLm7Qe; Path=/; Expires=Fri, 11 Dec 2026 09:00:00 GMT"),
        ("api.staging.bookstore.dev", "_ga=GA1.2.1928374655.1762851667; Path=/; Expires=Thu, 11 Nov 2027 09:00:00 GMT"),
        ("127.0.0.1", "session=s%3Ab2f9c1e0d4a7f3b29a; Path=/; HttpOnly; SameSite=Lax"),
        ("127.0.0.1", "theme=dark; Path=/; Expires=Thu, 11 Nov 2027 09:00:00 GMT"),
        ("github.com", "_gh_sess=Zm9vYmFyYmF6; Path=/; Secure; HttpOnly"),
    ] {
        jar.set(domain, cookie).expect("a valid cookie");
    }
    jar.save().unwrap();
}

/// The prototype's OPEN_TABS (data.js), restored when the app starts.
fn write_session(root: &Path) {
    let tab =
        |kind: &str, c: &str, r: &str| json!({"kind": kind, "location": [c, r], "scratch": null, "edited": false});
    let session = json!({
        "tabs": [
            tab("request", C_BOOK, R_LIST),
            tab("request", C_BOOK, R_CREATE),
            tab("request", C_BOOK, R_ME),
            tab("websocket", C_RT, R_WS),
            tab("proto", C_RT, R_SIO),
        ],
        "active": 0,
    });
    std::fs::write(root.join(".ternarywire").join("session.json"), serde_json::to_string_pretty(&session).unwrap())
        .unwrap();
}
