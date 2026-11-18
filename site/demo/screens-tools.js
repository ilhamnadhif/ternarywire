// Local servers: Mock server, Webhook listener and Capture proxy.
// All three share one header grammar: a 46 px title row (tile, name, what it
// does), a 56 px "address bar" (status + address + Copy, main action on the
// right), a collapsible settings strip, then list (left) | detail (right).

document.head.insertAdjacentHTML("beforeend", `<style id="css-tools">
.tw-tile { width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent-text); font-size: 15px; flex: none; }
.tw-st { display: flex; align-items: center; padding: 0 10px 0 8px; border-right: 1px solid var(--border); flex: none; }
.tw-strip { display: flex; align-items: center; gap: 10px; height: 40px; padding: 0 14px 0 16px; border-bottom: 1px solid var(--border); flex: none; font-size: 12.5px; cursor: pointer; }
.tw-strip:hover { background: var(--well); }
.tw-strip .sum { color: var(--text-muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tw-set { padding: 12px 16px 14px; border-bottom: 1px solid var(--border); background: var(--well); display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 12px 24px; flex: none; }
.tw-locked .tw-f { opacity: .55; pointer-events: none; }
.tw-grp { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.tw-grp .eyebrow { display: flex; align-items: center; gap: 6px; }
.tw-f { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-height: 26px; }
.tw-f > label { font-size: 12px; color: var(--text-muted); }
.tw-num { width: 70px; text-align: right; }
.tw-head { display: grid; align-items: center; gap: 10px; height: 28px; padding: 0 12px; background: var(--well); border-bottom: 1px solid var(--border); font-size: 11.5px; font-weight: 500; color: var(--text-muted); flex: none; }
.tw-row { display: grid; align-items: center; gap: 10px; height: 32px; padding: 0 12px; border-bottom: 1px solid var(--border); font-size: 12.5px; cursor: pointer; position: relative; }
.tw-row > * { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tw-head > .r, .tw-row > .r { text-align: right; }
.tw-row:hover { background: var(--well); }
.tw-row.sel { background: var(--accent-soft); }
.tw-row.sel::before { content: ""; position: absolute; left: 0; top: 6px; bottom: 6px; width: 3px; border-radius: 0 2px 2px 0; background: var(--accent); }
.tw-ts { font-family: "JBMono", monospace; font-size: 11.5px; color: var(--text-faint); }
.tw-path { font-family: "JBMono", monospace; font-size: 12px; }
.tw-path .q { color: var(--text-faint); }
.mp { display: inline-block; font-size: 10.5px; font-weight: 600; letter-spacing: .02em; padding: 2px 6px; border-radius: 5px; background: color-mix(in srgb, currentColor 13%, transparent); line-height: 1.3; flex: none; }
.tw-box { border: 1px solid var(--border); border-radius: 8px; overflow: hidden; background: var(--surface); }
.tw-cmd .btn { font-family: "Inter", sans-serif; }
.mp.m-CONNECT { color: var(--text-muted); }
.tw-cmd { display: flex; align-items: center; gap: 10px; padding: 10px 10px 10px 14px; border: 1px solid var(--border); border-radius: 9px; background: var(--well); font-family: "JBMono", monospace; font-size: 12px; width: min(620px, 100%); }
.tw-foot { border-top: 1px solid var(--border); padding: 10px 12px 12px; display: flex; flex-direction: column; gap: 8px; flex: none; background: var(--well); }
.tw-msg { display: grid; grid-template-columns: 22px 92px 1fr 54px; gap: 8px; align-items: center; height: 30px; padding: 0 10px; border-bottom: 1px solid var(--border); font-size: 12px; }
.tw-msg:last-child { border-bottom: 0; }
</style>`);

/* ------------------------------------------------------------------ helpers */
const REASON = { 101: "Switching Protocols", 200: "OK", 201: "Created", 204: "No Content", 304: "Not Modified", 404: "Not Found", 500: "Internal Server Error" };
const mp = (m) => `<span class="mp m-${m}">${m}</span>`;
function pathHtml(p) { const [a, q] = String(p).split("?"); return esc(a) + (q ? `<span class="q">?${esc(q)}</span>` : ""); }
function stCell(s) {
  if (typeof s === "number") {
    const k = s < 200 ? "info" : s < 300 ? "ok" : s < 400 ? "info" : s < 500 ? "warn" : "err";
    return `<span class="mono semi ${k}" style="font-size:12px">${s}</span>`;
  }
  if (s === "dropped" || s === "failed") return `<span class="err" style="font-size:11.5px">${s}</span>`;
  if (s === "tunnel") return `<span class="faint" style="font-size:11.5px">tunnel</span>`;
  return `<span class="faint pulse mono">···</span>`;
}
function srvPill(state, text) {
  if (state === "busy") return `<span class="pill info lg">${I("CIRCLE_NOTCH", "spin")}${text}</span>`;
  if (state === "live") return pill(text, "ok", { dot: true, live: true, lg: true });
  if (state === "failed") return pill(text, "err", { dot: true, lg: true });
  return `<span class="pill lg"><span style="width:8px;height:8px;border-radius:50%;border:1.5px solid currentColor;display:inline-block"></span>${text}</span>`;
}
function toolTitle(icon, title, sub) {
  return `<div class="row" style="height:46px;padding:0 12px 0 16px;border-bottom:1px solid var(--border);flex:none;gap:10px">
    <span class="tw-tile">${I(icon)}</span><span class="semi" style="font-size:14px;flex:none">${title}</span>
    <span class="muted ellipsis grow" style="font-size:12.5px" title="${esc(sub.replace(/<[^>]+>/g, ""))}">${sub}</span>
    ${ibtn("DOTS_THREE", "More actions", { sm: true })}</div>`;
}
// o: { state, label, addr, ph, mid, right, max }
function serverBar(o) {
  return `<div class="row" style="height:56px;padding:0 12px 0 16px;flex:none;gap:8px;border-bottom:1px solid var(--border)">
    <div class="urlbar" style="flex:1 1 auto;max-width:${o.max || 480}px">
      <div class="tw-st">${srvPill(o.state, o.label)}</div>
      <div class="url" style="-webkit-mask-image:none;mask-image:none">${o.addr ? esc(o.addr) : `<span class="faint" style="font-family:Inter">${o.ph}</span>`}</div>
      <div class="tail">${o.addr ? ibtn("COPY", "Copy address", { sm: true }) : ""}</div>
    </div>
    ${o.mid || ""}<span class="spacer"></span>${o.right || ""}</div>`;
}
function strip(key, open, label, summary, lock, body, o = {}) {
  return `<div class="tw-strip" data-set="${key}=!" title="${open ? "Hide" : "Show"} ${label.toLowerCase()}">${I("SLIDERS_HORIZONTAL", "muted")}<span class="med" style="flex:none">${label}</span>
      <span class="sum grow">${summary}</span>
      ${lock ? `<span class="row hint" style="gap:5px;flex:none">${I("LOCK_SIMPLE")}${lock}</span>` : ""}${I(open ? "CARET_UP" : "CARET_DOWN", "faint")}</div>
    ${open ? `<div class="tw-set" ${o.style ? `style="${o.style}"` : ""}>${body}</div>` : ""}`;
}
const grp = (title, inner, o = {}) => `<div class="tw-grp ${o.locked ? "tw-locked" : ""}" ${o.style ? `style="${o.style}"` : ""}><div class="eyebrow">${title}${o.locked ? I("LOCK_SIMPLE") : ""}</div>${inner}</div>`;
const fi = (label, ctrl, title) => `<div class="tw-f" ${title ? `title="${title}"` : ""}>${label ? `<label>${label}</label>` : ""}${ctrl}</div>`;
const num = (v, o = {}) => input(v, { sm: true, style: `width:${o.w || 70}px;text-align:right`, ph: o.ph });
const checkLine = (on, label, title) => `<div class="tw-f" ${title ? `title="${title}"` : ""}>${chk(on)}<span>${label}</span></div>`;
const switchLine = (on, label, title) => `<div class="tw-f" style="justify-content:space-between" ${title ? `title="${title}"` : ""}><span>${label}</span>${sw(on)}</div>`;

function listHead(cols, labels, right = []) {
  return `<div class="tw-head" style="grid-template-columns:${cols}">${labels.map((l, i) => `<span class="${right.includes(i) ? "r" : ""}">${l}</span>`).join("")}</div>`;
}
function listRow(cols, cells, o = {}) {
  return `<div class="tw-row ${o.sel ? "sel" : ""}" style="grid-template-columns:${cols}" ${o.set ? `data-set="${o.set}"` : ""} ${o.title ? `title="${esc(o.title)}"` : ""}>${cells.map((c, i) => `<span class="${(o.right || []).includes(i) ? "r" : ""}">${c}</span>`).join("")}</div>`;
}
const paneTitle = (label, count, end = "") => `<div class="row" style="height:36px;padding:0 12px 0 14px;border-bottom:1px solid var(--border);flex:none;gap:8px"><span class="section-title">${label}</span>${count !== undefined ? pill(count) : ""}<span class="spacer"></span>${end}</div>`;
const detailHead = (left, right = "") => `<div class="row" style="height:36px;padding:0 10px 0 14px;border-bottom:1px solid var(--border);flex:none;gap:8px">${left}<span class="spacer"></span>${right}</div>`;
const detailMeta = (left, right = "") => `<div class="row" style="height:40px;padding:0 10px 0 12px;flex:none;gap:8px;border-bottom:1px solid var(--border)">${left}<span class="spacer"></span>${right}</div>`;
const metrics = (items) => `<span class="rmeta">${items.map(([ic, t, title]) => `<span class="mt" ${title ? `title="${title}"` : ""}>${ic ? I(ic) : ""}${t}</span>`).join("")}</span>`;
function section(title, count, inner, end = "") {
  return `<div class="col" style="gap:8px"><div class="row"><span class="section-title">${title}</span>${count !== undefined ? pill(count) : ""}<span class="spacer"></span>${end}</div>${inner}</div>`;
}
const headersTable = (rows) => table([{ label: "Name", w: "36%" }, { label: "Value" }], rows.map(([k, v]) => [`<span class="mono muted" style="font-size:12px">${esc(k)}</span>`, `<span class="mono" style="font-size:12px">${esc(v)}</span>`]));
const bodyBox = (obj) => `<div class="tw-box">${jsonCode(obj)}</div>`;
const noBody = (t = "No body") => `<div class="row hint" style="height:32px;padding:0 12px;border:1px dashed var(--border);border-radius:8px">${I("MINUS", "faint")}${t}</div>`;
function toolPanes(left, right, lw = "46%") {
  const side = benchW() >= 760;
  return `<div class="panes" style="flex-direction:${side ? "row" : "column"}">
    <section class="pane" style="flex:${side ? `0 0 ${lw}` : "0 0 44%"};min-height:0">${left}</section>
    ${side ? '<div class="gutter" title="Drag to resize · double-click to reset"></div>' : ""}
    <section class="pane grow" style="min-height:0;${side ? "" : "border-top:1px solid var(--border)"}">${right}</section></div>`;
}
const listScroll = (inner) => `<div class="scroll grow" style="min-height:0">${inner}</div>`;
const detailScroll = (inner) => `<div class="scroll grow" style="min-height:0;padding:12px 12px 16px;display:flex;flex-direction:column;gap:16px">${inner}</div>`;
const selectHint = (body) => `<div class="grow col" style="justify-content:center">${empty("CURSOR_CLICK" in ICON ? "CURSOR_CLICK" : "ARROW_LEFT", "Select a request", body)}</div>`;

/* ------------------------------------------------------------------ mock server */
const MOCK_ROUTES = [
  ["GET", "/books", "List books", [[200, "Page of books"], [404, "Unknown page"]]],
  ["POST", "/books", "Create book", [[201, "Created"]]],
  ["GET", "/books/:id", "Get book", [[200, "One book"], [404, "Missing book"]]],
  ["PUT", "/books/:id", "Update book", [[200, "Updated"]]],
  ["DELETE", "/books/:id", "Delete book", [[204, "Deleted"]]],
  ["POST", "/auth/login", "Login", [[200, "Signed in"]]],
  ["GET", "/me", "Current user", [[200, "Profile"]]],
];
// time, method, status, path, duration, matched route ("METHOD /path") or null
const MOCK_LOGS = [
  ["09:41:07.214", "GET", 200, "/books?page=1&limit=20", "3 ms", "GET /books"],
  ["09:41:05.880", "POST", 201, "/books", "12 ms", "POST /books"],
  ["09:41:02.431", "GET", 404, "/books/99/reviews", "2 ms", null],
  ["09:40:58.102", "GET", "dropped", "/books", "—", "GET /books"],
  ["09:40:51.660", "DELETE", 204, "/books/3", "4 ms", "DELETE /books/:id"],
  ["09:40:47.019", "GET", 200, "/books/2", "3 ms", "GET /books/:id"],
  ["09:40:40.553", "POST", 200, "/auth/login", "251 ms", "POST /auth/login"],
  ["09:40:31.200", "GET", 500, "/me", "1 ms", "GET /me"],
];

function mockSettings(running) {
  return grp("Server", `<div class="row" style="gap:16px;flex-wrap:wrap">${fi("Port", num("3000"))}${fi("Delay (ms)", num("0"))}</div>${checkLine(true, "Resolve variables with the active environment")}`, { locked: running })
    + grp("Network", `${fi("Preset", selectbox("No throttling", { sm: true, style: "min-width:150px" }))}<div class="row" style="gap:16px;flex-wrap:wrap">${fi("Latency (ms)", num("0"))}${fi("Jitter (ms)", num("0"))}</div>${checkLine(false, "Bandwidth (KB/s)")}`, { locked: running })
    + grp("Failures", `<div class="row" style="gap:16px;flex-wrap:wrap">${fi("", `${num("0", { w: 56 })}<span class="faint">%</span>`, "Share of requests that fail")}${fi("Status", num("500", { w: 64 }))}</div>${seg("mfail", S.mfail || "status", [["status", "Error status"], ["drop", "Drop connection"], ["timeout", "Timeout"]], { sm: true })}`, { locked: running });
}

function mockLogDetail(i) {
  const [ts, m, st, path, dur, route] = MOCK_LOGS[i];
  const r = route ? MOCK_ROUTES.find((x) => `${x[0]} ${x[1]}` === route) : null;
  const ex = r ? (r[3].find((e) => e[0] === st) || r[3][0]) : null;
  const status = st === "dropped" ? pill("Connection dropped", "err", { lg: true }) : statusPill(st, REASON[st] || "");
  let match;
  if (!r) match = banner("warn", "WARNING", `<b>No match.</b> No request in “Bookstore API” has a saved example for <span class="mono">${m} ${esc(path.split("?")[0])}</span>, so the server answered 404.`);
  else {
    match = `<div class="card" style="padding:10px 12px"><div class="row">${I("CHECK_CIRCLE", "ok")}<span class="med">Matched</span>${mp(r[0])}<span class="tw-path">${r[1]}</span><span class="muted ellipsis">→ ${r[2]}</span><span class="spacer"></span>${btn("Open request", { kind: "ghost", size: "sm", icon: "ARROW_SQUARE_OUT" })}</div>
      <div class="hint" style="margin:6px 0 0 22px">${st === "dropped" ? "Dropped by failure simulation (Failures › Drop connection) before the example was sent." : `Served example “${ex[1]}” · ${ex[0]} · application/json${dur === "251 ms" ? " · after the 250 ms delay" : ""}`}</div></div>`;
  }
  const headers = [["host", "127.0.0.1:3000"], ["user-agent", "TernaryWire/0.2.3"], ["accept", "application/json"], ["x-request-id", "4c1f0a2e-77b1-4f5e-9d0e-3b1a2f8c5d10"]];
  if (m === "POST" || m === "PUT") headers.splice(3, 0, ["content-type", "application/json"], ["content-length", path === "/auth/login" ? "48" : "84"]);
  const body = path === "/books" && m === "POST" ? bodyBox({ title: "Bumi Manusia", author: "Pramoedya Ananta Toer", price: 15.5 })
    : path === "/auth/login" ? bodyBox({ username: "ilham", password: "••••••••" }) : noBody();
  return detailHead(`${mp(m)}<span class="tw-path semi ellipsis">${pathHtml(path)}</span>`, `${status}${ibtn("X", "Close (Esc)", { sm: true, set: "row=-1" })}`)
    + detailMeta(metrics([["CLOCK", ts, "Received"], ["TIMER", dur, "Duration"], ["DESKTOP", "from 127.0.0.1:52144"]]), btn("Copy as cURL", { kind: "ghost", size: "sm", icon: "TERMINAL" }))
    + detailScroll(match + section("Headers", headers.length, headersTable(headers)) + section("Body", undefined, body, body.includes("tw-box") ? ibtn("COPY", "Copy body", { sm: true }) : ""));
}
function mockRouteDetail(i) {
  const [m, path, name, exs] = MOCK_ROUTES[i];
  return detailHead(`${mp(m)}<span class="tw-path semi ellipsis">${esc(path)}</span><span class="muted ellipsis">${name}</span>`, btn("Open request", { kind: "ghost", size: "sm", icon: "ARROW_SQUARE_OUT" }))
    + detailMeta(`<span class="hint">Served when method and path match. Path variables like <span class="mono">:id</span> match any value.</span>`, "")
    + detailScroll(section("Examples", exs.length, exs.map(([c, n], k) => `<div class="card" style="padding:10px 12px"><div class="row">${I(k === 0 ? "CARET_DOWN" : "CARET_RIGHT", "faint")}${pill(c, c < 300 ? "ok" : c < 400 ? "info" : "warn")}<span class="med grow">${n}</span><span class="faint xs">JSON</span>${k === 0 ? pill("default", "acc") : ""}</div>
        ${k === 0 ? `<div style="margin-top:10px">${bodyBox({ data: [{ id: 1, title: "The Pragmatic Programmer", price: 42.5 }], page: 1, total: 1 })}</div>` : ""}</div>`).join(""),
      hint("Edit in the request’s Docs tab")) + banner("info", "INFO", "Add examples with “Save as example” on a response; they are stored with the request and committed with the collection."));
}

function mockView() {
  const running = S.srv !== "stopped";
  const sub = S.sub || "logs";
  const bar = serverBar({
    state: running ? "live" : "idle", label: running ? "Running" : "Stopped", addr: running ? "http://127.0.0.1:3000" : "", ph: "Starts on 127.0.0.1:3000",
    right: running
      ? `${btn('Use as <span class="mono" style="font-size:11.5px">{{baseUrl}}</span>', { icon: "CROSSHAIR", size: "sm", title: "Set {{baseUrl}} in “Local” to this URL" })}${btn("Stop", { icon: "STOP", set: "srv=stopped" })}`
      : primary("Start", { icon: "PLAY", set: "srv=running" }),
  });
  const settings = strip("mset", !!S.mset, "Settings", "Port 3000 · Delay 0 ms · No throttling · No failures", running ? "Stop the server to change these settings." : "", mockSettings(running));
  const LOGC = "86px 64px 58px minmax(0,1fr) 62px";
  const ROUTEC = "64px minmax(0,1fr) minmax(0,.8fr) 86px";
  let left = tabs("sub", sub, [["routes", "Routes", { count: MOCK_ROUTES.length }], ["logs", "Logs", { count: running ? MOCK_LOGS.length : undefined }]], "", {});
  let right;
  if (sub === "logs") {
    left += toolbar(search("Filter paths", { sm: true, style: "width:200px" }), running ? btn("Clear", { kind: "ghost", size: "sm", icon: "TRASH" }) : "")
      + listHead(LOGC, ["Time", "Method", "Status", "Path", "Duration"], [4])
      + (running
        ? listScroll(MOCK_LOGS.map(([ts, m, st, path, dur, route], i) => listRow(LOGC, [`<span class="tw-ts">${ts}</span>`, mp(m), stCell(st), route ? `<span class="tw-path">${pathHtml(path)}</span>` : `<span class="row" style="gap:6px;min-width:0"><span class="tw-path ellipsis">${pathHtml(path)}</span>${pill("no match", "warn")}</span>`, `<span class="tw-ts">${dur}</span>`], { sel: String(i) === String(S.row), set: `row=${i}`, title: route || "no match", right: [4] })).join(""))
        : `<div class="grow col" style="justify-content:center">${empty("LIST", "No requests yet", "Start the server, then send requests to it.")}</div>`);
    right = running && S.row !== "-1" && MOCK_LOGS[+S.row] ? mockLogDetail(+S.row) : selectHint(running ? "Its matched route, headers and body appear here." : "Requests to the mock server appear here.");
  } else {
    const rt = +(S.rt || 0);
    left += toolbar(hint("Requests with saved examples, matched by method and path"), "")
      + listHead(ROUTEC, ["Method", "Path", "Request", "Examples"], [3])
      + listScroll(MOCK_ROUTES.map(([m, path, name, exs], i) => listRow(ROUTEC, [mp(m), `<span class="tw-path">${esc(path)}</span>`, `<span class="muted">${name}</span>`, pill(`${exs.length} example${exs.length > 1 ? "s" : ""}`, "", { title: exs.map((e) => `${e[0]} ${e[1]}`).join(", ") })], { sel: i === rt, set: `rt=${i}`, title: name, right: [3] })).join(""));
    right = mockRouteDetail(rt);
  }
  return toolTitle("CUBE", "Mock server", "Bookstore API · examples saved on requests are served by method and path") + bar + settings + toolPanes(left, right);
}

/* ------------------------------------------------------------------ webhook listener */
const WH_REQS = [
  ["09:42:10.118", "POST", "/stripe/webhook", "3.1 KB", "payment_intent.succeeded", "evt_3QK9vLJ2eZvKYlo21Ab4Xc7d"],
  ["09:41:55.402", "POST", "/stripe/webhook", "2.8 KB", "charge.succeeded", "evt_3QK9v7J2eZvKYlo20pQ1mN2s"],
  ["09:40:12.007", "POST", "/stripe/webhook?attempt=2", "1.2 KB", "customer.created", "evt_1QK9u2J2eZvKYlo2Lq8wE4tB"],
];
function webhookDetail(i) {
  const [ts, m, path, size, type, id] = WH_REQS[i];
  const headers = [["host", "127.0.0.1:8787"], ["user-agent", "Stripe/1.0 (+https://stripe.com/docs/webhooks)"], ["content-type", "application/json; charset=utf-8"], ["content-length", String(Math.round(parseFloat(size) * 1024))],
    ["stripe-signature", "t=1762854130,v1=5257a869e7ecebeda32affa62cdca3fa51cad7e77a0e56ff536d0ce8e108d8bd"], ["accept", "*/*; q=0.5, application/xml"], ["cache-control", "no-cache"], ["x-forwarded-for", "54.187.174.169"], ["x-forwarded-proto", "https"]];
  const body = { id, object: "event", type, created: 1762854130, livemode: false, data: { object: { id: "pi_3QK9vLJ2eZvKYlo21x9Yb0Qe", object: "payment_intent", amount: 425000, currency: "idr", status: "succeeded", customer: "cus_R2bF7kQmZ1aXo9" } }, pending_webhooks: 1 };
  return detailHead(`${mp(m)}<span class="tw-path semi ellipsis">${pathHtml(path)}</span>`, `${pill(type, "", { outline: true })}${ibtn("X", "Close (Esc)", { sm: true, set: "row=-1" })}`)
    + detailMeta(metrics([["CLOCK", ts, "Received"], ["DESKTOP", "from 127.0.0.1:61022", "Through the tunnel"], ["ARROW_LINE_DOWN", size, "Body size"]]), btn("Copy as cURL", { kind: "ghost", size: "sm", icon: "TERMINAL" }))
    + detailScroll(disclosure("Headers", true, headersTable(headers), { count: headers.length }) + section("Body", undefined, bodyBox(body), ibtn("COPY", "Copy body", { sm: true })))
    + `<div class="tw-foot">
        <div class="row"><span class="section-title">${I("ARROW_CLOCKWISE", "muted")} Replay</span><span class="hint ellipsis">Send this request again to another URL, e.g. your local app.</span></div>
        <div class="row">${input("http://localhost:3000/api/stripe/webhook", { sm: true, mono: true, style: "flex:1" })}${btn("Replay", { size: "sm", icon: "ARROW_CLOCKWISE" })}</div>
        <div class="row" style="gap:8px">${pill("200 OK", "ok")}${metrics([["TIMER", "18 ms"], ["ARROW_LINE_DOWN", "17 B"]])}<span class="mono ellipsis grow" style="font-size:12px"><span class="p">{</span><span class="k">"received"</span><span class="p">:</span> <span class="b">true</span><span class="p">}</span></span></div>
      </div>`;
}
function webhookView(emptyMode) {
  const running = S.srv !== "stopped";
  const has = running && !emptyMode;
  const bar = serverBar({
    state: running ? "live" : "idle", label: running ? "Listening" : "Stopped", addr: running ? "http://127.0.0.1:8787" : "", ph: "Starts on a free port",
    right: running ? btn("Stop", { icon: "STOP", set: "srv=stopped" }) : primary("Start", { icon: "PLAY", set: "srv=running" }),
  });
  const settingsBody = grp("Listener", fi("Port", `${num("8787")}`, running ? "Locked while listening · 0 picks a free port" : "0 picks a free port"), { locked: running, style: "flex:none" })
    + '<div class="vdiv" style="height:52px;align-self:center"></div>'
    + grp("Respond with", `<div class="tw-f" style="flex-wrap:nowrap">${fi("Status", num("200", { w: 60 }))}${input('{"ok":true}', { sm: true, mono: true, ph: "Response body", style: "flex:1;min-width:120px" })}${running ? btn("Apply", { kind: "ghost", size: "sm", icon: "CHECK", title: "Use this response from now on" }) : ""}</div>`, { style: "flex:1" });
  const settings = strip("wclosed", !S.wclosed, "Settings", 'Port 8787 · responds <span class="mono" style="font-size:11.5px">200 {"ok":true}</span>', "", settingsBody, { style: "display:flex;gap:12px 20px;align-items:stretch" });
  const head = toolTitle("WEBHOOKS_LOGO", "Webhook listener", "Records every request sent to the local URL. For a public URL, point a tunnel (ngrok, cloudflared, ...) at it.") + bar + settings;
  if (!has) {
    const body = running
      ? `${empty("HOURGLASS", "Waiting for requests", "Requests sent to the address above appear here. Try:")}
        <div class="tw-cmd"><span class="faint">$</span><span class="grow" style="white-space:pre-wrap;word-break:break-all">curl -X POST http://127.0.0.1:8787/hook \\<br>  -H "Content-Type: application/json" \\<br>  -d '{"hello":"world"}'</span>${btn("Copy command", { size: "sm", icon: "COPY" })}</div>
        <div class="hint row" style="margin-top:14px;gap:6px">${I("INFO", "faint")}<span>For Stripe, GitHub or Slack, start a tunnel (<span class="mono">ngrok http 8787</span>) and paste its URL in the provider.</span></div>`
      : empty("WEBHOOKS_LOGO", "The listener is stopped", "Start it to get a local URL that records every request sent to it.", primary("Start", { icon: "PLAY", size: "sm", set: "srv=running" }));
    return head + `<div class="grow col" style="justify-content:center;align-items:center;padding:24px;gap:0;min-height:0">${body}</div>`;
  }
  const C = "92px 64px minmax(0,1fr) 62px";
  const left = paneTitle("Requests", WH_REQS.length, btn("Clear", { kind: "ghost", size: "sm", icon: "TRASH" }))
    + toolbar(search("Filter requests", { sm: true, style: "width:220px" }), hint("Newest first"))
    + listHead(C, ["Time", "Method", "Path", "Size"], [3])
    + listScroll(WH_REQS.map(([ts, m, path, size, type], i) => listRow(C, [`<span class="tw-ts">${ts}</span>`, mp(m), `<span class="tw-path">${pathHtml(path)}</span>`, `<span class="tw-ts">${size}</span>`], { sel: String(i) === String(S.row), set: `row=${i}`, title: type, right: [3] })).join(""));
  const right = S.row !== "-1" && WH_REQS[+S.row] ? webhookDetail(+S.row) : selectHint("Its headers and body appear here.");
  return head + toolPanes(left, right, "42%");
}

/* ------------------------------------------------------------------ capture proxy */
// ticked (null = tunnel, no checkbox), time, method, status, host, path, size
const CAP = [
  [true, "09:44:01", "GET", 200, "api.bookstore.dev", "/books?page=1&limit=20", "2.1 KB"],
  [true, "09:44:01", "POST", 201, "api.bookstore.dev", "/books", "412 B"],
  [false, "09:44:02", "GET", 304, "cdn.bookstore.dev", "/assets/app.4f2a.js", "0 B"],
  [null, "09:44:02", "CONNECT", "tunnel", "accounts.google.com", ":443", "9.6 KB"],
  [false, "09:44:02", "GET", 200, "api.bookstore.dev", "/me", "318 B"],
  [true, "09:44:03", "PATCH", 200, "api.bookstore.dev", "/orders/14", "156 B"],
  [false, "09:44:04", "GET", 404, "api.bookstore.dev", "/books/99", "64 B"],
  [false, "09:44:04", "POST", "failed", "telemetry.example.com", "/v1/events", "—"],
  [false, "09:44:05", "GET", 101, "api.bookstore.dev", "/live", "16 msgs"],
  [false, "09:44:06", "GET", "···", "api.bookstore.dev", "/books/export.csv", "—"],
];
function captureDetail(i) {
  const [, ts, m, st, host, path, size] = CAP[i];
  const url = st === "tunnel" ? `${host}${path}` : `https://${host}${path}`;
  const status = typeof st === "number" ? statusPill(st, REASON[st] || "") : st === "tunnel" ? pill("tunnel", "", { lg: true }) : st === "failed" ? pill("failed", "err", { lg: true }) : `<span class="pill info lg">${I("CIRCLE_NOTCH", "spin")}Waiting…</span>`;
  const head = detailHead(`${mp(m)}<span class="tw-path semi ellipsis" title="${url}">${pathHtml(url)}</span>`, `${status}${ibtn("X", "Close (Esc)", { sm: true, set: "row=-1" })}`);
  if (st === "tunnel") {
    return head + detailMeta(`${pill("passed through", "", { outline: true, icon: "LOCK_SIMPLE" })}`, metrics([["ARROW_UP", "1.2 KB", "Sent by the client"], ["ARROW_DOWN", "8.4 KB", "Sent by the server"], ["TIMER", "4.2 s"]]))
      + detailScroll(banner("warn", "WARNING", "<b>The client rejected the TernaryWire certificate</b> for this host, so its HTTPS was passed through without decrypting it. Trust the certificate on that device, or add the host to <i>Never decrypt</i> to silence this.") + section("Connection", undefined, headersTable([["Host", "accounts.google.com:443"], ["Client", "127.0.0.1:61540 (Safari)"], ["Opened", ts + ".118"], ["Closed", ts.replace("02", "06") + ".327"]])));
  }
  if (st === "failed") return head + detailMeta(pill("HTTP/1.1", "", { outline: true }), metrics([["TIMER", "5.0 s"]])) + detailScroll(banner("err", "WARNING_CIRCLE", "<b>Could not reach telemetry.example.com.</b> DNS lookup failed: no such host.") + section("Request headers", 4, headersTable([["host", host], ["content-type", "application/json"], ["user-agent", "BookstoreApp/3.2 (iOS 18)"], ["content-length", "211"]])));
  if (st === "···") return head + detailMeta(pill("HTTP/2", "", { outline: true }), metrics([["TIMER", "3.1 s"]])) + `<div class="grow col" style="justify-content:center">${empty("HOURGLASS", "No response yet", "The server is still sending the response. It appears here when it is complete.")}</div>`;
  const reqH = [["host", host], ["accept", m === "GET" ? "application/json" : "*/*"], ["authorization", "Bearer ••••••••"], ["user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_6) Safari/605.1.15"], ["accept-encoding", "gzip, deflate, br"], ["cookie", "session=s%3Ab2f…9a"]];
  const resH = [["content-type", "application/json; charset=utf-8"], ["content-encoding", "br"], ["cache-control", "no-store"], ["date", "Tue, 11 Nov 2026 09:44:01 GMT"], ["x-request-id", "a91c22e0-5b1e-4d4f-a1e2-0c7d9b3e6f12"], ["vary", "Accept-Encoding"]];
  const ws = st === 101;
  const msgs = [["out", "09:44:05.210", '{"subscribe":"orders"}', "22 B"], ["in", "09:44:05.244", '{"ok":true}', "11 B"], ["in", "09:44:07.002", '{"order":14,"status":"shipped"}', "31 B"], ["in", "09:44:09.551", '{"order":15,"status":"paid"}', "28 B"]];
  return head
    + detailMeta(`${pill(ws ? "HTTP/1.1" : "HTTP/2", "", { outline: true })}${pill("decrypted", "ok", { icon: "LOCK_SIMPLE" })}`, metrics(ws ? [["ARROW_UP", "2 msgs · 22 B"], ["ARROW_DOWN", "14 msgs · 1.4 KB"]] : [["TIMER", "142 ms", "Total"], ["", "TTFB 118 ms", "Time to first byte"], ["ARROW_LINE_DOWN", size, "Response size"]]))
    + detailScroll(disclosure("Request headers", true, headersTable(reqH), { count: reqH.length })
      + section("Request body", undefined, m === "GET" ? noBody() : bodyBox({ title: "Bumi Manusia", author: "Pramoedya Ananta Toer", price: 15.5 }))
      + disclosure("Response headers", !ws, headersTable(resH), { count: resH.length })
      + (ws
        ? section("Messages", 16, `<div class="tw-box">${msgs.map(([d, t, b, s]) => `<div class="tw-msg"><span class="${d === "out" ? "info" : "ok"}">${I(d === "out" ? "ARROW_UP" : "ARROW_DOWN")}</span><span class="tw-ts">${t}</span><span class="mono ellipsis">${esc(b)}</span><span class="tw-ts" style="text-align:right">${s}</span></div>`).join("")}</div>`)
        : section("Response body", undefined, st === 304 ? noBody("No body (304 Not Modified)") : bodyBox(path.startsWith("/books?") ? { data: BOOKS.data.slice(0, 2).map(({ id, title, price }) => ({ id, title, price })), page: 1, total: 5 } : { ok: true }), ibtn("COPY", "Copy body", { sm: true }))));
}
function certPanel() {
  return `<div style="padding:12px 16px;border-bottom:1px solid var(--border);flex:none;background:var(--well)">
    <div class="card" style="padding:14px 16px;display:flex;gap:14px;align-items:flex-start">
      <span class="tw-tile" style="width:34px;height:34px;font-size:18px;background:var(--warn-soft);color:var(--warn)">${I("SHIELD_CHECK")}</span>
      <div class="col grow" style="gap:10px">
        <div class="row"><span class="semi" style="font-size:13.5px">TernaryWire capture certificate</span>${pill("Not trusted yet", "warn", { icon: "WARNING" })}<span class="spacer"></span>${ibtn("X", "Close", { sm: true, set: "cert=false" })}</div>
        <div style="display:grid;grid-template-columns:96px minmax(0,1fr);gap:6px 12px;font-size:12.5px;align-items:center">
          <span class="muted">SHA-256</span><span class="row" style="gap:4px;min-width:0"><span class="mono ellipsis" style="font-size:11.5px">4F:9A:21:C7:0B:3E:88:D2:5A:17:E0:94:6C:B3:1F:72:AD:05:39:E6:C8:4B:90:12:7D:F3:66:A8:0E:5C:B1:C2</span>${ibtn("COPY", "Copy fingerprint", { sm: true })}</span>
          <span class="muted">Valid until</span><span>11 Nov 2027</span>
        </div>
        <div class="row" style="flex-wrap:wrap">${btn("Save certificate…", { size: "sm", icon: "DOWNLOAD_SIMPLE" })}${btn("Check again", { size: "sm", kind: "ghost", icon: "ARROWS_CLOCKWISE" })}<span class="hint">Saving also copies the steps to trust it to the clipboard.</span></div>
        <div class="hint row" style="gap:6px">${I("INFO", "faint")}<span>Devices on the network can also download it from <span class="mono">http://ternarywire.proxy/</span> through the proxy.</span></div>
      </div>
    </div></div>`;
}
function captureView() {
  const running = S.srv !== "stopped";
  const bar = serverBar({
    state: running ? "live" : "idle", label: running ? "Recording" : "Stopped", addr: running ? "127.0.0.1:8888" : "", ph: "Starts on port 8888", max: 400,
    mid: running ? `${btn("Pause", { icon: "PAUSE", size: "sm", title: "Stop recording; traffic still flows" })}${btn("Certificate", { icon: "SHIELD_CHECK", size: "sm", on: !!S.cert, set: "cert=!", title: "Show the HTTPS certificate" })}${pill("Certificate rejected", "warn", { icon: "WARNING", title: "Rejected by: accounts.google.com, *.apple.com" })}` : "",
    right: running ? btn("Stop", { icon: "STOP", set: "srv=stopped" }) : primary("Start proxy", { icon: "PLAY", set: "srv=running" }),
  });
  const optBody = grp("Proxy", `<div class="row" style="gap:16px">${fi("Port", num("8888"))}</div>${switchLine(true, "Decrypt HTTPS")}${switchLine(true, "Verify server certificates")}${switchLine(false, "Allow other devices on the network", "Anyone on the network can use the proxy")}${running ? `<div class="hint row" style="gap:5px">${I("LOCK_SIMPLE")}Stop the proxy to change its port and HTTPS options.</div>` : ""}`, { locked: running })
    + grp("Record", `${fi("Only hosts", input("api.bookstore.dev", { sm: true, mono: true, ph: "*.example.com", style: "flex:1" }))}${fi("Skip hosts", input("*.apple.com", { sm: true, mono: true, ph: "*.apple.com", style: "flex:1" }))}${fi("URL contains", input("", { sm: true, mono: true, ph: "/api/", style: "flex:1" }))}${running ? `<div>${btn("Apply filters", { kind: "ghost", size: "sm", icon: "CHECK" })}</div>` : ""}`)
    + grp("Never decrypt", `${input("bank.example.com, *.icloud.com", { sm: true, mono: true, style: "width:100%" })}<div class="hint">Hosts whose HTTPS is passed through untouched.</div>`);
  const optOpen = running ? !!S.copt : !S.coptClosed;
  const options = strip(running ? "copt" : "coptClosed", optOpen, "Options", "Port 8888 · Decrypt HTTPS · 2 filters", "", optBody);
  const head = toolTitle("BROADCAST", "Capture traffic", "Set this proxy in a browser, app or device to record its requests. HTTPS needs the TernaryWire certificate to be trusted.") + bar + (running && S.cert ? certPanel() : "") + options;
  const C = "18px 62px 66px 50px minmax(0,1fr) 58px";
  const ticked = CAP.filter((r) => r[0]).length;
  const left = paneTitle("Traffic", running ? `${CAP.length} requests` : undefined, running ? `${btn("Export HAR", { kind: "ghost", size: "sm", icon: "EXPORT" })}${btn("Clear", { kind: "ghost", size: "sm", icon: "TRASH" })}` : "")
    + toolbar(`${search("Search URLs", { sm: true, style: "flex:1;min-width:110px" })}${running ? `${selectbox("Bookstore API", { sm: true, icon: "STACK", style: "max-width:150px" })}${primary(`Save ${ticked} to collection`, { size: "sm", icon: "FOLDER_PLUS", title: "Save the ticked requests" })}` : ""}`).replace('<span class="spacer"></span>', "")
    + listHead(C, [chk(true, true), "Time", "Method", "Status", "URL", "Size"], [5])
    + (running
      ? listScroll(CAP.map(([t, ts, m, st, host, path, size], i) => listRow(C, [t === null ? "" : chk(t), `<span class="tw-ts">${ts}</span>`, mp(m), stCell(st), `<span class="tw-path"><span class="muted">${host}</span>${pathHtml(path)}</span>`, `<span class="tw-ts">${size}</span>`], { sel: String(i) === String(S.row), set: `row=${i}`, title: `${host}${path}`, right: [5] })).join(""))
      : `<div class="grow col" style="justify-content:center">${empty("BROADCAST", "The proxy is stopped", "Start it, then point a browser, app or device at it to record their requests.")}</div>`);
  const right = running && S.row !== "-1" && CAP[+S.row] ? captureDetail(+S.row) : selectHint("Its headers, bodies and timings appear here.");
  return head + toolPanes(left, right, "50%");
}

/* ------------------------------------------------------------------ screens */
const SERVER_NOTES = [
  "Pola header server yang sama untuk Mock, Webhook dan Capture: baris judul 46 px (tile ikon, nama, penjelasan satu baris) lalu ‘address bar’ 56 px yang meniru URL bar HTTP — status (Running/Listening/Recording · Starting… · Stopped · Failed) + alamat + Copy di kiri, aksi server di kanan.",
  "Pengaturan dilipat menjadi satu strip 40 px dengan ringkasan nilai; klik untuk membuka. Bagian yang terkunci saat server jalan ditandai gembok + petunjuk.",
  "Di bawahnya selalu list (kiri) | detail (kanan) dengan baris 36/40 px yang sejajar kiri-kanan, sama seperti panel request/response.",
];

screen({ id: "mock", group: "Local servers", label: "Mock server · logs + detail", rail: "tools", tab: { id: "mock", icon: "CUBE", t: "Mock · Bookstore API", go: "mock" }, preset: { sub: "logs", row: "1", mset: true }, view: mockView,
  notes: N([
    ...SERVER_NOTES,
    "Routes dan Logs tidak lagi dua kolom + detail di bawah: keduanya jadi tab di panel kiri (Routes 7 · Logs 8), panel kanan selalu detail yang dipilih — log (Matched, Headers, Body) atau route beserta contoh-contohnya. List tetap setinggi penuh.",
    "‘Use as {{baseUrl}}’ berada di address bar karena ia aksi terhadap alamat server.",
    "Request tanpa route ditandai pill ‘no match’ langsung di baris (dulu hanya tooltip). Kolom durasi berjudul ‘Duration’ agar tidak ada dua kolom ‘Time’ (string baru → id.tsv).",
    "Klik Stop untuk melihat state Stopped: Start jadi tombol primer, pengaturan bisa diubah, Logs menampilkan empty state.",
  ]) + NE([
    "<code>mock_tab::show</code>: ganti <code>proto_common::view_title</code> + kartu pengaturan dengan <code>tool_header()</code> (46) dan <code>server_bar()</code> (56) baru di <code>proto_common</code>; <code>address_pill</code> pindah ke dalam bar sebagai segmen kanan.",
    "Strip pengaturan = <code>proto_common::collapsible</code> dengan ringkasan; isinya di <code>ui.add_enabled_ui(!running, …)</code> per grup.",
    "Kiri: <code>kit::tab_strip</code> (Routes/Logs) + <code>request_header</code>/<code>request_row</code> (86/64/58/*/62) dengan <code>kit::show_rows</code>; kanan memakai <code>split_view</code> horizontal, bukan vertikal 45 %.",
  ]) });

screen({ id: "mock-routes", group: "Local servers", label: "Mock server · routes", rail: "tools", tab: { id: "mock", icon: "CUBE", t: "Mock · Bookstore API", go: "mock" }, preset: { sub: "routes", rt: "0" }, view: mockView,
  notes: N(["Tab Routes: tiap route (method, path, nama request, jumlah contoh) dan di kanan contoh-contoh yang disajikan beserta body contoh default.", "Strip pengaturan tertutup menampilkan ringkasan ‘Port 3000 · Delay 0 ms · No throttling · No failures’."]) });

screen({ id: "webhook", group: "Local servers", label: "Webhook listener · request + replay", rail: "tools", tab: { id: "webhook", icon: "WEBHOOKS_LOGO", t: "Stripe events", go: "webhook" }, preset: { row: "0" }, view: () => webhookView(false),
  notes: N([
    ...SERVER_NOTES,
    "‘Respond with’ (status + body + Apply) ada di strip Settings bersama Port, bukan di baris judul.",
    "Jumlah request + Clear pindah ke judul panel list (‘Requests 3’). Ditambah filter path kecil agar baris kedua sejajar dengan meta detail.",
    "Replay menjadi footer tetap di panel detail: URL + Replay selalu terlihat walau body panjang di-scroll; hasil (status, waktu, ukuran, body) tampil satu baris di bawahnya.",
    "Tipe event (mis. payment_intent.succeeded) tampil sebagai pill di header detail dan tooltip baris.",
  ]) + NE([
    "<code>webhook_tab::show</code>: list kiri 42 % (280–520 px) seperti sekarang, detail kanan; kartu Replay jadi <code>egui::Panel::bottom</code> di dalam panel detail.",
    "Settings strip memakai helper yang sama dengan mock (<code>proto_common::collapsible</code>), Port dikunci saat Listening.",
  ]) });

screen({ id: "webhook-empty", group: "Local servers", label: "Webhook listener · menunggu request", rail: "tools", tab: { id: "webhook", icon: "WEBHOOKS_LOGO", t: "Stripe events", go: "webhook-empty" }, view: () => webhookView(true),
  notes: N(["Saat belum ada request, seluruh area di bawah strip menjadi empty state dengan perintah curl siap pakai + Copy command, ditambah petunjuk tunnel untuk Stripe/GitHub/Slack.", "Klik Stop untuk state ‘The listener is stopped’ dengan tombol Start."]) });

screen({ id: "capture", group: "Local servers", label: "Capture proxy · traffic + detail", rail: "tools", tab: { id: "capture", icon: "BROADCAST", t: "Capture traffic", go: "capture" }, preset: { row: "0" }, view: captureView,
  notes: N([
    ...SERVER_NOTES,
    "Address bar memuat Pause, toggle Certificate dan pill peringatan ‘Certificate rejected’ (tooltip: host yang menolak) — semua kontrol sesi di satu baris.",
    "Options tertutup dengan ringkasan ‘Port 8888 · Decrypt HTTPS · 2 filters’; terbuka otomatis saat proxy berhenti. Hanya grup Proxy yang terkunci saat merekam — filter Record tetap bisa di-Apply.",
    "Action bar dipecah: judul panel (‘Traffic · 10 requests’, Export HAR, Clear) dan toolbar (Search URLs, koleksi, Save n to collection). Di layar sempit toolbar tetap satu baris karena search mengecil.",
    "Baris tunnel tanpa checkbox, pending ‘···’ berdenyut, failed merah, 101 WebSocket menampilkan daftar Messages ↑/↓ di detail. Klik baris lain untuk melihat detailnya.",
  ]) + NE([
    "<code>capture_tab::show</code>: header/opsi lewat <code>tool_header</code> + <code>server_bar</code> + <code>collapsible</code>; list 50 % (320–640 px) dengan <code>request_header</code>/<code>request_row</code> + kolom checkbox 18 px.",
    "Detail: <code>disclosure</code> untuk headers (terbuka), <code>text_view</code> untuk body; metrik memakai <code>kit::metric</code>.",
  ]) });

screen({ id: "capture-cert", group: "Local servers", label: "Capture proxy · panel sertifikat", rail: "tools", tab: { id: "capture", icon: "BROADCAST", t: "Capture traffic", go: "capture" }, preset: { row: "0", cert: true }, view: captureView,
  notes: N([
    "Panel sertifikat muncul sebagai kartu tepat di bawah address bar (toggle ‘Certificate’ aktif), bukan menggeser list: status trust (‘Not trusted yet’ / ‘Trusted by this Mac’ / ‘Checking trust…’), SHA-256 + Copy, Valid until, Save certificate…, Check again.",
    "Petunjuk http://ternarywire.proxy/ untuk perangkat lain di jaringan tetap di dalam kartu.",
  ]) + NE(["Kartu = <code>kit::card</code> di <code>capture_tab</code> (fungsi panel sertifikat yang ada, :531) dengan tile <code>warn_soft</code>; tutup lewat X mengembalikan toggle."]) });
