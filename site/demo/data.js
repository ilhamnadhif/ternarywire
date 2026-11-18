// Sample workspace used by every screen of the prototype.

const ENVS = [
  { id: "local", name: "Local", color: "#22a660", vars: 6 },
  { id: "staging", name: "Staging", color: "#e0a020", vars: 6 },
  { id: "prod", name: "Production", color: "#e14d2a", vars: 7 },
];

// tree: [type, id, name, extra]
const TREE = [
  { t: "coll", id: "c-book", name: "Bookstore API", open: true, children: [
    { t: "folder", id: "f-auth", name: "Auth", open: true, children: [
      { t: "req", id: "login", m: "POST", name: "Login" },
      { t: "req", id: "me", m: "GET", name: "Current user", go: "http-auth" },
    ] },
    { t: "folder", id: "f-books", name: "Books", open: true, children: [
      { t: "req", id: "list", m: "GET", name: "List books", go: "http" },
      { t: "req", id: "get", m: "GET", name: "Get book" },
      { t: "req", id: "create", m: "POST", name: "Create book", go: "http-body" },
      { t: "req", id: "update", m: "PUT", name: "Update book" },
      { t: "req", id: "delete", m: "DEL", name: "Delete book" },
      { t: "req", id: "search", m: "GQL", name: "Search catalog", go: "http-graphql" },
    ] },
    { t: "folder", id: "f-orders", name: "Orders", open: false, children: [] },
  ] },
  { t: "coll", id: "c-rt", name: "Realtime & AI", open: true, children: [
    { t: "req", id: "ws", m: "WS", name: "Echo socket", go: "ws" },
    { t: "req", id: "sio", m: "SIO", name: "Chat events", go: "sio" },
    { t: "req", id: "mqtt", m: "MQTT", name: "Sensor stream", go: "mqtt" },
    { t: "req", id: "grpc", m: "gRPC", name: "Greeter", go: "grpc" },
    { t: "req", id: "mcp", m: "MCP", name: "Tools server", go: "mcp" },
    { t: "req", id: "ai", m: "AI", name: "Assistant", go: "ai" },
  ] },
  { t: "coll", id: "c-pay", name: "Payments (fork)", fork: "Payments", open: false, children: [] },
];

// open tabs in the tab strip: id -> screen
const OPEN_TABS = [
  { id: "list", m: "GET", t: "List books", go: "http" },
  { id: "create", m: "POST", t: "Create book", go: "http-body", dirty: true },
  { id: "me", m: "GET", t: "Current user", go: "http-auth" },
  { id: "ws", m: "WS", t: "Echo socket", go: "ws" },
  { id: "sio", m: "SIO", t: "Chat events", go: "sio" },
];

const BOOKS = {
  data: [
    { id: 1, title: "The Pragmatic Programmer", author: "Andrew Hunt", price: 42.5, inStock: true, tags: ["craft", "classic"] },
    { id: 2, title: "Designing Data-Intensive Applications", author: "Martin Kleppmann", price: 55, inStock: true, tags: ["data"] },
    { id: 3, title: "Laskar Pelangi", author: "Andrea Hirata", price: 12, inStock: false, tags: ["novel"] },
    { id: 4, title: "Rust in Action", author: "Tim McNamara", price: 39.99, inStock: true, tags: ["rust"] },
  ],
  page: 1,
  limit: 20,
  total: 5,
};

const RES_HEADERS = [
  ["content-type", "application/json; charset=utf-8"],
  ["content-length", "956"],
  ["cache-control", "no-store"],
  ["etag", 'W/"3bc-9fkq2"'],
  ["x-request-id", "7f2e9c1a-3d44-4b1e-a2f0-91c0f2d7b6aa"],
  ["x-ratelimit-remaining", "58"],
  ["date", "Tue, 11 Nov 2026 09:41:07 GMT"],
];

const HISTORY = [
  ["Today", [
    ["GET", "127.0.0.1:18795/books?page=1&limit=20", 200, "09:41"],
    ["POST", "127.0.0.1:18795/books", 201, "09:38"],
    ["GET", "127.0.0.1:18795/books/9", 404, "09:36"],
    ["POST", "127.0.0.1:18795/auth/login", 200, "09:30"],
    ["GET", "api.staging.bookstore.dev/me", 401, "09:12"],
    ["DEL", "127.0.0.1:18795/books/3", 204, "08:57"],
  ]],
  ["Yesterday", [
    ["PUT", "127.0.0.1:18795/books/2", 200, "18:20"],
    ["GET", "httpbin.org/delay/5", "Error", "17:02"],
    ["PATCH", "127.0.0.1:18795/orders/14", 200, "16:44"],
    ["GET", "127.0.0.1:18795/books?tag=rust", 200, "16:40"],
  ]],
  ["9 Nov 2026", [
    ["POST", "api.openai.com/v1/responses", 200, "11:05"],
    ["GET", "127.0.0.1:18795/health", 200, "10:59"],
  ]],
];

const CONSOLE = [
  ["09:41:07.214", "net", "GET http://127.0.0.1:18795/books?page=1&limit=20 → 200 OK · 142 ms · 956 B", "List books"],
  ["09:41:07.352", "log", "pages left: 0", "List books · post-response"],
  ["09:41:07.353", "log", "cheapest: Laskar Pelangi (12)", "List books · post-response"],
  ["09:38:11.020", "net", "POST http://127.0.0.1:18795/books → 201 Created · 31 ms · 383 B", "Create book"],
  ["09:36:02.877", "warn", "Undefined variable {{bookId}} — sent as typed", "Get book"],
  ["09:36:02.910", "net", "GET http://127.0.0.1:18795/books/9 → 404 Not Found · 4 ms", "Get book"],
  ["09:12:44.100", "err", "TypeError: Cannot read properties of undefined (reading 'token') at post-response:3:28", "Login"],
];
