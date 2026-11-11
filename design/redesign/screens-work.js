// Start page, collection/folder settings, environments, history and Spec Hub.
// Wrapped in an IIFE so helper names cannot clash with the other screen files.
(() => {
document.head.insertAdjacentHTML("beforeend", `<style id="css-work">
  .w-title { height: 46px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 12px 0 16px; border-bottom: 1px solid var(--border); }
  .w-tile { width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; font-size: 15px; flex: none; background: var(--accent-soft); color: var(--accent-text); }
  .w-title h1 { margin: 0; font-size: 14px; font-weight: 600; white-space: nowrap; }
  .w-title .sub { color: var(--text-muted); font-size: 12px; white-space: nowrap; }
  .w-page { padding: 18px 20px 28px; display: flex; flex-direction: column; gap: 18px; max-width: 980px; }
  /* variables table */
  .vt { border: 1px solid var(--border); border-radius: 9px; background: var(--surface); position: relative; }
  .vt .vr { display: grid; grid-template-columns: 34px minmax(0,1fr) 128px minmax(0,1.25fr) minmax(0,1.25fr) 64px; min-height: 36px; border-top: 1px solid var(--border); }
  .vt .vr:first-child { border-top: 0; }
  .vt .vr > div { display: flex; align-items: center; gap: 6px; padding: 0 10px; min-width: 0; overflow: hidden; white-space: nowrap; border-left: 1px solid var(--border); }
  .vt .vr > div:first-child { border-left: 0; justify-content: center; padding: 0; }
  .vt .vr > div:last-child { border-left: 0; justify-content: flex-end; padding: 0 6px; gap: 0; visibility: hidden; }
  .vt .vr:hover > div:last-child, .vt .vr.hov > div:last-child { visibility: visible; }
  .vt .vr.h { min-height: 30px; background: var(--well); color: var(--text-muted); font-size: 11.5px; font-weight: 500; border-radius: 9px 9px 0 0; }
  .vt .vr.h > div:last-child { visibility: visible; }
  .vt .vr:not(.h):hover, .vt .vr.hov { background: var(--well); }
  .vt .vr.off { color: var(--text-faint); }
  .vt .vr.off .var, .vt .vr.off .mono { opacity: .6; }
  .vt .vr.ghost > div { color: var(--text-faint); }
  .vt .vr .cell-edit { box-shadow: inset 0 0 0 1.5px var(--accent); border-radius: 4px; margin: 3px -4px; padding: 0 6px; height: 28px; display: flex; align-items: center; width: calc(100% + 8px); background: var(--surface); }
  .vt .hdrtag { font-size: 10px; font-weight: 600; padding: 1px 5px; border-radius: 4px; background: var(--subtle); color: var(--text-faint); letter-spacing: .02em; }
  .vt .diff { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); flex: none; }
  .vt .mask { letter-spacing: 2px; color: var(--text-muted); }
  /* home */
  .qs { display: grid; gap: 10px; }
  .qs .qc { display: flex; align-items: center; gap: 12px; padding: 12px 14px; border: 1px solid var(--border); border-radius: 11px; background: var(--surface); cursor: pointer; min-width: 0; transition: border-color .1s, background .1s; }
  .qs .qc:hover { border-color: var(--border-strong); background: var(--hover); }
  .qs .qc .qi { width: 36px; height: 36px; border-radius: 9px; display: grid; place-items: center; font-size: 18px; flex: none; background: var(--accent-soft); color: var(--accent-text); }
  .qs .qc .qt { font-weight: 500; font-size: 13px; }
  .qs .qc .qd { color: var(--text-muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .qs .qc.big { flex-direction: column; align-items: flex-start; gap: 10px; padding: 14px; }
  .qs .pc { display: flex; align-items: center; gap: 10px; height: 46px; padding: 0 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); cursor: pointer; min-width: 0; }
  .qs .pc:hover { border-color: var(--border-strong); background: var(--hover); }
  .qs .pc .badge { min-width: 40px; height: 22px; border-radius: 6px; display: grid; place-items: center; font-size: 10.5px; font-weight: 600; background: color-mix(in srgb, currentColor 13%, transparent); }
  .home-rec .row:hover { background: var(--hover); }
  /* docs preview */
  .docs { padding: 22px 26px 40px; display: flex; flex-direction: column; gap: 14px; }
  .docs h2 { margin: 0; font-size: 21px; font-weight: 600; letter-spacing: -.01em; }
  .docs h3 { margin: 6px 0 0; font-size: 17px; font-weight: 600; }
  .docs .ep { display: grid; grid-template-columns: 64px minmax(0,1fr) auto; align-items: center; gap: 8px; height: 32px; padding: 0 12px; border-top: 1px solid var(--border); font-size: 12.5px; cursor: pointer; }
  .docs .ep:first-child { border-top: 0; }
  .docs .ep:hover { background: var(--well); }
  .docs .pathp { color: var(--accent-text); }
  .docs .tbl td { height: auto; padding: 8px 12px; vertical-align: top; white-space: normal; }
  .w-prob { flex: none; border-top: 1px solid var(--border); background: var(--well); }
  .w-prob .ph { height: 32px; display: flex; align-items: center; gap: 8px; padding: 0 12px; font-size: 12px; }
  .w-prob .pr { height: 28px; display: flex; align-items: center; gap: 8px; padding: 0 12px; font-size: 12px; border-top: 1px solid var(--border); cursor: pointer; }
  .w-prob .pr:hover { background: var(--hover); }
  .w-pop { position: absolute; z-index: 30; width: 312px; padding: 12px; border-radius: 11px; background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-pop); }
  .w-pop .opt { display: flex; align-items: center; gap: 9px; height: 32px; padding: 0 8px; border-radius: 7px; font-size: 12.5px; }
  .w-pop .opt:hover, .w-pop .opt.on { background: var(--hover); }
  .w-pop .act { display: flex; gap: 10px; padding: 9px 10px; border-radius: 8px; border: 1px solid var(--border); align-items: flex-start; }
  .w-pop .act:hover { border-color: var(--border-strong); background: var(--well); }
  .w-pop .act .ai { width: 28px; height: 28px; border-radius: 7px; display: grid; place-items: center; background: var(--accent-soft); color: var(--accent-text); font-size: 15px; flex: none; }
</style>`);

/* ------------------------------------------------------------------ shared bits */
function titleRow(icon, title, sub, right = "", o = {}) {
  const tile = `<div class="w-tile" ${o.tint ? `style="background:color-mix(in srgb, ${o.tint} 15%, transparent);color:${o.tint}"` : ""}>${I(icon)}</div>`;
  return `<div class="w-title">${tile}<h1 title="Double-click to rename">${title}</h1>${o.after || ""}${sub ? `<span class="sub">· ${sub}</span>` : ""}<span class="spacer"></span>${right}</div>`;
}
const keys = (s) => [...s].map((k) => kbd(k)).join("");

// rows: {on, name, type: "default"|"secret", init, cur, hov, edit, menu}
function varTable(rows, o = {}) {
  const head = `<div class="vr h">
    <div>${chk(true, rows.some((r) => !r.on))}</div>
    <div>Variable</div>
    <div title="Secret values are masked and never written to the workspace files">Type</div>
    <div title="Shared: saved to the workspace files (secrets excepted)">Initial value <span class="hdrtag">${I("SHARE")} SHARED</span></div>
    <div title="Local: used when sending, never committed">Current value <span class="hdrtag">${I("DESKTOP")} LOCAL</span></div>
    <div>${ibtn("DOTS_THREE", "Table options", { sm: true })}</div></div>`;
  const val = (r, v, isCur) => {
    if (r.type === "secret") return `<span class="mask mono">••••••••</span>${I("LOCK_SIMPLE", "faint")}<span class="spacer"></span>${ibtn("EYE", "Show value", { sm: true })}`;
    if (isCur && !v) return `<span class="mono faint" style="font-size:12px">${esc(r.init)}</span>`;
    const inner = `<span class="mono" style="font-size:12px">${vars(v)}</span>`;
    return r.edit && isCur ? `<span class="cell-edit">${inner}<span style="width:1.5px;height:15px;background:var(--accent);margin-left:1px"></span></span>` : inner;
  };
  const body = rows.map((r) => `<div class="vr ${r.on ? "" : "off"} ${r.hov ? "hov" : ""}">
    <div>${chk(r.on)}</div>
    <div class="mono" style="font-size:12px">${esc(r.name)}</div>
    <div>${selectbox(r.type, { sm: true, quiet: true, icon: r.type === "secret" ? "LOCK_SIMPLE" : "TEXT_AA", style: "width:100%" })}</div>
    <div>${val(r, r.init, false)}</div>
    <div>${val(r, r.cur, true)}${r.cur && r.cur !== r.init && r.type !== "secret" ? '<span class="spacer"></span><span class="diff" title="Differs from the initial value"></span>' : ""}</div>
    <div>${ibtn("DOTS_THREE", "More actions", { sm: true })}${ibtn("TRASH", "Delete", { sm: true })}</div></div>`).join("");
  const ghost = `<div class="vr ghost"><div>${I("PLUS")}</div><div>Add a new variable</div><div></div><div></div><div></div><div></div></div>`;
  return `<div class="vt">${head}${body}${ghost}${o.extra || ""}</div>`;
}

/* ------------------------------------------------------------------ home */
function homeView() {
  const big = [
    ["GLOBE", "HTTP request", "REST, GraphQL, SOAP", "http-empty", "⌘N"],
    ["DOWNLOAD_SIMPLE", "Import", "Postman, OpenAPI, cURL, …", "", "", "import"],
    ["STACK", "New Collection", "Organize requests", "container"],
    ["FLOW_ARROW", "New flow", "Chain requests visually", "flow"],
  ];
  const protos = [
    ["WS", "WebSocket", "Messages both ways", "ws"],
    ["SIO", "Socket.IO", "Emit and listen to events", "sio"],
    ["MQTT", "MQTT", "Publish and subscribe", "mqtt"],
    ["gRPC", "gRPC", "Unary and streaming calls", "grpc"],
    ["MCP", "MCP", "Call a server's tools", "mcp"],
    ["AI", "AI request", "Chat with a model", "ai"],
  ];
  const shortcuts = [["⌘N", "New request"], ["⌘↩", "Send request"], ["⌘K", "Search requests"], ["⌘E", "Switch environment"], ["⇧⌘P", "Command palette"], ["⌘B", "Toggle sidebar"]];
  const recent = HISTORY[0][1].slice(0, 6);
  const sk = (st) => (st === "Error" ? "err" : st < 300 ? "ok" : st < 400 ? "info" : st < 500 ? "warn" : "err");
  return `<div class="scroll grow"><div style="max-width:780px;margin:0 auto;padding:${H() < 760 ? 28 : 54}px 24px 40px;display:flex;flex-direction:column;gap:26px">
    <div class="col" style="align-items:center;gap:6px;text-align:center">
      <img src="../../crates/ternarywire-app/assets/icon-256.png" alt="" style="width:56px;height:56px;border-radius:13px;box-shadow:0 6px 18px rgba(225,77,42,.25)">
      <div style="font-size:24px;font-weight:600;letter-spacing:-.015em;margin-top:8px">TernaryWire</div>
      <div class="muted">A fast, local-first API client. Start with one of these:</div>
    </div>
    <div class="col" style="gap:10px">
      <div class="qs" style="grid-template-columns:repeat(4,minmax(0,1fr))">${big.map(([ic, t, d, go, k, dlg]) => `<div class="qc big" ${go ? `data-go="${go}"` : `data-dlg="${dlg}"`}>
        <div class="row" style="width:100%"><div class="qi">${I(ic)}</div><span class="spacer"></span>${k ? keys(k) : ""}</div>
        <div style="min-width:0;width:100%"><div class="qt">${t}</div><div class="qd">${d}</div></div></div>`).join("")}</div>
      <div class="row" style="margin-top:8px"><span class="eyebrow">Connect with another protocol</span><span class="spacer"></span><span class="hint">Every request type has its own tab, message log and scripts</span></div>
      <div class="qs" style="grid-template-columns:repeat(3,minmax(0,1fr))">${protos.map(([m, t, d, go]) => `<div class="pc" data-go="${go}">
        <span class="badge m-${m}">${m}</span><div style="min-width:0"><div class="med" style="font-size:12.5px">${t}</div><div class="qd hint" style="font-size:11.5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${d}</div></div></div>`).join("")}</div>
    </div>
    <div class="grid2" style="gap:28px;align-items:start">
      <div class="col" style="gap:4px">
        <div class="row" style="height:28px"><span class="section-title">Keyboard shortcuts</span><span class="spacer"></span><a class="hint accent" data-dlg="settings" style="cursor:pointer">All shortcuts</a></div>
        ${shortcuts.map(([k, w]) => `<div class="row" style="height:30px"><span class="muted grow">${w}</span><span class="row" style="gap:2px">${keys(k)}</span></div>`).join("")}
      </div>
      <div class="col home-rec" style="gap:4px">
        <div class="row" style="height:28px"><span class="section-title">Recent requests</span><span class="spacer"></span><a class="hint accent" data-go="history" style="cursor:pointer">History</a></div>
        ${recent.map(([m, u, st, t]) => `<div class="row" style="height:30px;padding:0 6px;margin:0 -6px;border-radius:6px;cursor:pointer" data-go="history" title="${u}">
          <span style="width:40px;text-align:right">${meth(m)}</span><span class="mono grow ellipsis" style="font-size:11.5px">${u.replace(/^[^/]+/, "")}</span>
          <span class="mono xs" style="color:var(--${sk(st)})">${st}</span><span class="faint xs" style="width:36px;text-align:right">${t}</span></div>`).join("")}
      </div>
    </div>
    <div class="row hint" style="justify-content:center;gap:6px;margin-top:4px">${I("FOLDER_SIMPLE")} Workspace <b class="med" style="color:var(--text)">Bookstore</b> · ~/Developer/bookstore · 3 collections · plain JSON files you can commit</div>
  </div></div>`;
}

/* ------------------------------------------------------------------ container */
const COLL_VARS = [
  { on: true, name: "baseUrl", type: "default", init: "http://127.0.0.1:18795", cur: "" },
  { on: true, name: "pageSize", type: "default", init: "20", cur: "50" },
  { on: true, name: "clientId", type: "default", init: "bookstore-desktop", cur: "" },
  { on: true, name: "clientSecret", type: "secret", init: "x", cur: "x" },
];
function containerView() {
  const sub = S.sub || "overview";
  const right = `${btn("Mock server", { size: "sm", icon: "CUBE", go: "mock" })}${btn("Run", { kind: "primary", size: "sm", icon: "PLAY", go: "runner", title: "Open the Collection Runner" })}${ibtn("DOTS_THREE", "More actions", { menu: "coll-ctx" })}`;
  const head = titleRow("STACK", "Bookstore API", "Collection · 8 requests", right);
  const strip = tabs("sub", sub, [["overview", "Overview"], ["auth", "Authorization", { dot: true }], ["vars", "Variables", { count: 4 }], ["scripts", "Scripts", { count: 2 }]]);
  let body = "";
  if (sub === "overview") {
    body = `<div class="w-page">
      <div class="grid4">${stat("Requests", "8", "in 3 folders", { icon: "GLOBE" })}${stat("Folders", "3", "Auth · Books · Orders", { icon: "FOLDER_SIMPLE" })}${stat("Variables", "4", "1 secret", { icon: "BRACKETS_CURLY" })}${stat("Authorization", '<span style="font-size:17px">OAuth 2.0</span>', "inherited by 7 requests", { icon: "KEY" })}</div>
      <div class="row" style="align-items:flex-start;gap:20px">
        <div class="col grow" style="gap:18px">
          ${group("General", [frow("Name", "Also the name of the collection file.", input("Bookstore API", { sm: true, style: "width:280px" }))])}
          <div class="col" style="gap:8px"><div class="row"><h4 class="section-title">Description</h4><span class="spacer"></span>${seg("docv", "edit", [["edit", "Write"], ["prev", "Preview"]], { sm: true })}</div>
            <div class="well" style="overflow:hidden">${code(["# Bookstore API", "", "Browse books, place orders and manage your account.", "", "## Authentication", "Requests inherit **OAuth 2.0** (Authorization Code with PKCE).", "Run *Auth › Login* once to get a token; it is renewed automatically.", "", "## Environments", "- **Local**: `http://127.0.0.1:18795`", "- **Staging**: `https://api.staging.bookstore.dev`"].map((l) => (l.startsWith("#") ? `<span class="k">${esc(l)}</span>` : l.startsWith("-") ? `<span class="p">-</span>${esc(l.slice(1))}` : esc(l))), { style: "background:transparent" })}</div>
            ${hint("Markdown. Shown in the generated collection documentation.")}</div>
        </div>
        <div class="col" style="width:250px;flex:none;gap:8px">
          <h4 class="section-title">Actions</h4>
          <div class="card" style="padding:4px">${[["PLAY", "Run collection", "runner"], ["CUBE", "Mock server", "mock"], ["BOOK_OPEN", "Preview documentation"], ["EXPORT", "Export (Postman v2.1)"], ["EXPORT", "Export (OpenAPI 3)"], ["FILE_TEXT", "Export documentation (HTML)"], ["GIT_FORK", "Fork…"]].map(([ic, l, go]) => `<div class="row" style="height:30px;padding:0 9px;border-radius:6px;cursor:pointer" ${go ? `data-go="${go}"` : ""}>${I(ic, "faint")}<span class="grow">${l}</span></div>`).join("")}</div>
          <div class="hint" style="padding:0 4px">${I("FOLDER_SIMPLE")} <span class="mono" style="font-size:11px">collections/bookstore-api/</span> · one file per request</div>
        </div>
      </div></div>`;
  } else if (sub === "auth") {
    body = `<div class="col" style="max-width:820px;gap:0;flex:1;min-height:0"><div style="padding:14px 12px 0">${banner("info", "INFO", "Requests that use “Inherit auth from parent” get this auth. <b class='med'>7 of 8</b> requests inherit it; <span class='mono'>Auth / Login</span> uses No Auth.")}</div>${authTab()}</div>`;
  } else if (sub === "vars") {
    body = toolbar(`${search("Filter variables", { sm: true, style: "width:260px" })}<span class="hint">Collection variables are available to every request in the collection.</span>`, btn("Bulk edit", { kind: "ghost", size: "sm", icon: "TEXT_ALIGN_LEFT" }))
      + `<div style="padding:4px 12px 16px">${varTable(COLL_VARS)}</div>
      <div class="row hint" style="padding:0 14px;gap:14px">${I("INFO")}Hover a <span class="var">{{variable}}</span> anywhere to see its value and the scope it comes from.</div>`;
  } else {
    const ev = S.sub2 || "pre";
    const src = ev === "pre"
      ? ["// Runs before every request in “Bookstore API”", 'if (!pm.environment.get("token")) {', '  console.log("no token yet — run Auth › Login first");', "}", 'pm.request.headers.upsert({ key: "X-Request-Start", value: String(Date.now()) });']
      : ['pm.test("No server errors", () => {', "  pm.expect(pm.response.code).to.be.below(500);", "});"];
    const dot = '<span class="dot" style="width:6px;height:6px;background:var(--ok);margin-left:2px"></span>';
    body = toolbar(seg("sub2", ev, [["pre", "Pre-request" + dot], ["post", "Post-response" + dot]], { sm: true }) + `<span class="hint">Runs for every request inside, before the request's own script.</span>`, btn("Snippets", { size: "sm", icon: "LIGHTNING", menu: "snippets", caret: true }))
      + `<div class="well grow" style="margin:0 12px 12px;display:flex;overflow:hidden">${code(jsHl(src.join("\n")), { style: "flex:1;background:transparent", cur: 1 })}</div>`;
  }
  return head + strip + `<div class="scroll grow col" style="gap:0">${body}</div>`;
}

/* ------------------------------------------------------------------ environment */
function envView() {
  const env = ENVS[0];
  const right = `${pill("Active", "ok", { icon: "CHECK", title: "Requests resolve variables from this environment" })}${ibtn("DOTS_THREE", "More actions", { menu: "env-ctx" })}`;
  const rows = [
    { on: true, name: "baseUrl", type: "default", init: "http://127.0.0.1:18795", cur: "", edit: false },
    { on: true, name: "pageSize", type: "default", init: "20", cur: "5", hov: true },
    { on: true, name: "user", type: "default", init: "ilham@example.com", cur: "" },
    { on: true, name: "token", type: "secret", init: "x", cur: "x" },
    { on: false, name: "legacyHost", type: "default", init: "http://old.bookstore.dev", cur: "" },
    { on: true, name: "publisherId", type: "default", init: "pub_{{$randomInt}}", cur: "pub_812", edit: true },
  ];
  const hoverMenu = `<div class="menu" style="right:8px;top:${30 + 36 * 2 + 2}px;min-width:250px">
      <div class="mh">pageSize</div>
      <div class="mi">${I("ARROW_LINE_DOWN")}<span class="grow">Persist current value as initial</span></div>
      <div class="mi">${I("ARROW_COUNTER_CLOCKWISE")}<span class="grow">Reset current value</span><span class="sub">back to 20</span></div>
      <div class="ms"></div><div class="mi">${I("COPY")}<span class="grow">Copy <span class="mono" style="font-size:11.5px">{{pageSize}}</span></span></div></div>`;
  return titleRow("GLOBE_SIMPLE", env.name, "Environment · 6 variables", right, { tint: env.color })
    + toolbar(`${search("Filter variables", { sm: true, style: "width:280px" })}`, `<span class="hint row" style="gap:6px">${I("INFO")}Initial values are saved in the workspace and can be committed to Git; current values and secrets stay on this machine.</span>`)
    + `<div class="scroll grow" style="padding:4px 12px 16px">${varTable(rows, { extra: hoverMenu })}
      <div class="row" style="margin-top:14px;gap:10px">
        ${card(`<div class="row" style="gap:10px">${I("LOCK_KEY", "muted")}<div class="grow"><div class="med">Secrets live in the Keychain</div><div class="hint">Use <span class="var sec">{{vault:name}}</span> for values shared across environments.</div></div>${btn("Open vault", { size: "sm", dlg: "vault" })}</div>`, { style: "flex:1;padding:12px 14px" })}
        ${card(`<div class="row" style="gap:10px">${I("SWAP" in ICON ? "SWAP" : "ARROWS_LEFT_RIGHT", "muted")}<div class="grow"><div class="med">Switch quickly</div><div class="hint">${kbd("⌘")}${kbd("E")} anywhere, or double-click an environment in the sidebar.</div></div></div>`, { style: "flex:1;padding:12px 14px" })}
      </div></div>`;
}

/* ------------------------------------------------------------------ history */
REQS.hist = { crumbs: [], name: "{{baseUrl}}/books?page=1&limit=20", m: "GET", url: "{{baseUrl}}/books?page=1&limit=20", scratch: true };
function historyView() {
  const html = httpView("hist");
  const note = `<div class="row" style="flex:none;height:36px;padding:0 16px;gap:8px;background:var(--info-soft);border-bottom:1px solid var(--border);font-size:12.5px">
    ${I("CLOCK_COUNTER_CLOCKWISE", "info")}<span>Opened from History · <b class="med">Today 09:41</b></span><span class="faint">·</span>
    <span class="muted">last answer</span>${pill("200 OK", "ok")}<span class="muted mono" style="font-size:11.5px">142 ms · 956 B</span><span class="faint">·</span><span class="muted">Save to keep it</span>
    <span class="spacer"></span>${btn("Save to collection…", { size: "sm", kind: "ghost", icon: "FLOPPY_DISK", dlg: "save" })}${ibtn("X", "Dismiss", { sm: true })}</div>`;
  const i = html.indexOf('<div class="panes"');
  return html.slice(0, i) + note + html.slice(i);
}

/* ------------------------------------------------------------------ spec hub */
const SPEC = `openapi: 3.0.3
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
      summary: Get a book`;

function specDocs() {
  const eps = [["GET", "/books", "List books"], ["POST", "/books", "Create a book"], ["GET", "/books/{id}", "Get a book"], ["DELETE", "/books/{id}", "Delete a book"], ["POST", "/auth/login", "Log in"]];
  const path = (p) => esc(p).replace(/\{(\w+)\}/g, '<span class="pathp">{$1}</span>');
  return `<div class="docs">
    <div class="col" style="gap:6px"><h2>Bookstore API</h2><div class="muted">Browse books, place orders and manage your account.</div></div>
    <div class="row" style="gap:6px;flex-wrap:wrap">${pill("5 operations", "info", { icon: "LIST" })}${pill("v1.2.0", "", { outline: true })}<span class="chip mono" style="font-size:11.5px;height:22px"><span class="faint" style="font-family:Inter">baseUrl</span>http://127.0.0.1:18795</span></div>
    <div class="col" style="gap:8px"><div class="eyebrow">Endpoints</div>
      <div class="card" style="overflow:hidden">${eps.map(([m, p, n]) => `<div class="ep"><span class="meth m-${m === "DELETE" ? "DEL" : m}" style="font-size:11px">${m}</span><span class="mono ellipsis" style="font-size:12px">${path(p)}</span><span class="muted">${n}</span></div>`).join("")}</div></div>
    <div class="row" style="margin-top:6px;gap:8px"><h3>Books</h3>${pill("4")}</div><div class="divider"></div>
    ${card(`<div class="col" style="gap:10px">
      <div class="semi" style="font-size:14px">List books</div>
      <div class="row">${pill("GET", "ok")}<span class="mono" style="font-size:12.5px">/books</span></div>
      <div class="col" style="gap:6px"><div class="eyebrow">Parameters</div>
        ${table([{ label: "Name", w: "34%" }, { label: "In", w: "18%" }, { label: "Example" }], [
          ['<span class="mono">page</span><div class="hint" style="white-space:normal">Page number, starting at 1</div>', '<span class="muted">query</span>', '<span class="mono">1</span>'],
          ['<span class="mono">tag</span><div class="hint" style="white-space:normal">Only books with this tag</div>', '<span class="muted">query</span>', '<span class="mono">classic</span>'],
          ['<span class="mono">Accept</span>', '<span class="muted">header</span>', '<span class="mono">application/json</span>'],
        ]).replace(/height: 34px/g, "")}
      </div>
      <div class="col" style="gap:6px"><div class="eyebrow">Responses</div>
        <div class="row" style="gap:8px">${pill("200", "ok")}<span>A page of books</span><span class="spacer"></span>${btn("Example", { size: "sm", kind: "ghost", icon: "CARET_RIGHT" })}</div></div></div>`)}
    ${card(`<div class="col" style="gap:10px">
      <div class="semi" style="font-size:14px">Create a book</div>
      <div class="row">${pill("POST", "warn")}<span class="mono" style="font-size:12.5px">/books</span></div>
      <div class="col" style="gap:6px"><div class="eyebrow">Request body</div><div class="well">${jsonCode({ title: "Rust in Action", price: 39.99 }, { gutter: false })}</div></div>
      <div class="col" style="gap:6px"><div class="eyebrow">Responses</div><div class="row" style="gap:8px">${pill("201", "ok")}<span>Created</span></div></div></div>`)}
  </div>`;
}

function specView(o = {}) {
  const mode = S.seg || "split";
  const narrow = benchW() < 760;
  const head = `<div class="w-title"><div class="crumbs grow" style="gap:6px">${I("BRACKETS_CURLY", "faint")}<span>specs</span><span class="sep">${I("CARET_RIGHT")}</span>
      <span class="cur" style="font-size:14px" title="specs/bookstore.yaml">bookstore.yaml</span>
      ${o.dirty ? '<span class="dot" style="width:7px;height:7px;background:var(--accent)" title="Unsaved changes"></span>' : ""}
      ${pill("OpenAPI", "info", { icon: "BRACKETS_CURLY", title: "OpenAPI 3.0.3" })}
      ${o.warn ? pill("1 warning", "warn", { icon: "WARNING", title: "Valid Bookstore API: 5 operations, 1 warning" }) : pill("Valid", "ok", { icon: "CHECK", title: "Valid Bookstore API: 5 operations" })}</div>
    ${btn("Save", { size: "sm", icon: "FLOPPY_DISK", title: "Save (⌘S)" })}${ibtn("DOTS_THREE", "More actions", { menu: "w-spec-more" })}</div>`;
  const bar = `<div class="row" style="height:46px;flex:none;padding:0 12px;gap:8px;border-bottom:1px solid var(--border)">
    ${seg("seg", mode, [["editor", "Editor", "CODE"], ["split", "Split", "COLUMNS"], ["docs", "Docs", "BOOK_OPEN"]], { sm: true })}
    <span class="spacer"></span>
    ${btn("Validate", { size: "sm", icon: "SHIELD_CHECK", title: "Check the spec again, with the files it uses" })}
    ${btn("Sync", { size: "sm", icon: "ARROWS_LEFT_RIGHT", caret: true, on: o.sync, title: "Update a collection from this spec, or the spec from a collection" })}
    ${btn("Generate collection", { kind: "primary", size: "sm", icon: "STACK", title: "Create a collection with a request for every operation" })}</div>`;
  const syncNote = o.sync ? `<div class="row" style="flex:none;height:36px;padding:0 12px 0 16px;gap:8px;background:var(--info-soft);border-bottom:1px solid var(--border);font-size:12.5px">${I("CHECK_CIRCLE", "info")}
    <span class="ellipsis" style="min-width:0">Synced with <b class="med">Bookstore API</b>: 3 updated, 1 added, 1 in the collection but not in the spec (left untouched)</span><span class="spacer"></span>${btn("Open collection", { size: "sm", kind: "ghost", go: "container" })}${ibtn("X", "Dismiss", { sm: true })}</div>` : "";
  const problems = o.warn
    ? `<div class="w-prob"><div class="ph">${I("WARNING", "warn")}<span class="med">Problems</span>${pill("0 errors")}${pill("1 warning", "warn")}<span class="spacer"></span><span class="faint mono" style="font-size:11px">Ln 34, Col 9 · OpenAPI · 1.8 KB</span></div>
        <div class="pr">${I("WARNING", "warn")}<span class="pill outline mono" style="font-size:11px">34:9</span><span class="grow ellipsis">GET /books/{id} has no 404 response; clients cannot tell a missing book from an error</span></div></div>`
    : `<div class="w-prob"><div class="ph">${I("CHECK_CIRCLE", "ok")}<span class="med">No problems</span><span class="spacer"></span><span class="faint mono" style="font-size:11px">Ln 12, Col 8 · OpenAPI · 1.8 KB</span></div></div>`;
  const editor = `<section class="pane grow" style="min-width:0;min-height:0">
      <div class="scroll grow" style="background:var(--surface)">${code(yamlHl(SPEC), { cur: 11 })}</div>${problems}</section>`;
  const docs = `<section class="pane grow scroll" style="min-width:0;min-height:0;background:${mode === "docs" ? "var(--canvas)" : "var(--well)"}">${mode === "docs" ? `<div style="max-width:780px;margin:0 auto;width:100%">${specDocs()}</div>` : specDocs()}</section>`;
  let body;
  if (mode === "editor") body = editor;
  else if (mode === "docs") body = docs;
  else body = `<div class="panes" style="flex-direction:${narrow ? "column" : "row"}">
    <div class="pane" style="flex:0 0 54%;min-height:0;display:flex">${editor}</div>${narrow ? '<div class="hgutter"></div>' : '<div class="gutter" title="Drag to resize · double-click to reset"></div>'}
    <div class="pane grow" style="min-height:0;display:flex;${narrow ? "" : "background:var(--well)"}">${docs}</div></div>`;
  const pop = o.sync ? `<div class="w-pop" style="top:${46 + 46 - 4}px;right:168px" data-stop="1">
      <div class="semi" style="font-size:13px">Sync with a collection</div>
      <div class="hint" style="margin:2px 0 10px">Operations match by method and path. Your scripts, auth and examples are kept.</div>
      <div class="eyebrow" style="margin-bottom:4px">Collection</div>
      ${[["Bookstore API", true, "8 requests"], ["Realtime & AI", false, "6 requests"], ["Payments (fork)", false, "fork"]].map(([n, on, d]) => `<div class="opt ${on ? "on" : ""}"><span style="width:16px;color:var(--accent-text)">${on ? I("CHECK") : ""}</span>${I("STACK", "faint")}<span class="grow">${n}</span><span class="faint xs">${d}</span></div>`).join("")}
      <div class="divider" style="margin:10px 0"></div>
      <div class="col" style="gap:6px">
        <div class="act"><div class="ai">${I("ARROW_RIGHT")}</div><div><div class="med">Spec → collection</div><div class="hint">Update the collection from this spec</div></div></div>
        <div class="act"><div class="ai" style="background:var(--subtle);color:var(--text-muted)">${I("ARROW_LEFT")}</div><div><div class="med">Collection → spec</div><div class="hint">Replace this spec with one generated from the collection (not saved until you press Save)</div></div></div>
      </div></div>` : "";
  return `<div class="col grow" style="gap:0;position:relative;min-height:0">${head}${bar}${syncNote}${body}${pop}</div>`;
}

Object.assign(MENUS, {
  "w-spec-more": () => placeMenu(menu([{ icon: "ARROW_SQUARE_OUT", label: "Open docs in browser" }, { icon: "COPY", label: "Copy file path" }, "-", { icon: "TRASH", label: "Delete…", danger: true }]), { right: true, w: 220 }),
});

/* ------------------------------------------------------------------ screens */
screen({ id: "home", group: "Start", label: "Start page (tanpa tab)", rail: "collections", noTabs: true, view: homeView,
  notes: N([
    "Halaman awal saat tidak ada tab: tenang, berpusat, lebar maks 780 px. Empat aksi utama sebagai kartu besar (HTTP, Import, Collection, Flow) — HTTP menampilkan shortcut ⌘N di kartunya.",
    "Protokol lain dikelompokkan di bawah ‘Connect with another protocol’ dengan badge berwarna yang sama seperti di sidebar dan tab (WS, SIO, MQTT, gRPC, MCP, AI), masing-masing dengan deskripsi spesifik (bukan ‘Connect and test’ generik).",
    "Dua kolom bawah: shortcut (tombol per tuts, bukan satu string) dan request terakhir dari History dengan status berwarna. Tautan ‘All shortcuts’ membuka Settings › Shortcuts.",
    "Baris workspace di paling bawah menegaskan model ‘file biasa di folder Anda’.",
  ]) + NE([
    "<code>app.rs</code> <code>home()</code>: ganti grid seragam 3 kolom dengan dua grid: 4 kartu 96 px + 6 kartu protokol 46 px; warna badge dari <code>theme::method_color</code>.",
    "Shortcut: helper baru <code>kit::keys(ui, \"⇧⌘P\")</code> yang memanggil <code>kit::kbd</code> per karakter.",
    "Recent requests: tambahkan status (warna <code>theme::status_color</code>) dan jam dari <code>HistoryEntry</code>.",
  ]) });

screen({ id: "container", group: "Workspace", label: "Collection settings (Overview · Auth · Variables · Scripts)", rail: "collections", sel: "", tab: { id: "c-book", icon: "STACK", t: "Bookstore API", go: "container" }, view: containerView,
  notes: N([
    "Baris judul 46 px sama seperti request: ikon koleksi, nama (double-click rename), ‘Collection · 8 requests’; aksi Mock server (sekunder) + Run (primer) + ⋯ (menu konteks koleksi yang sama dengan sidebar).",
    "Overview: 4 stat card, grup General (Name + hint nama file), Description markdown dengan Write/Preview. <b>Baru:</b> kartu ‘Actions’ di kanan memunculkan item yang dulu hanya ada di klik-kanan sidebar (Export, Documentation, Fork).",
    "Authorization memakai editor auth yang sama dengan request, diawali banner berapa request yang mewarisinya.",
    "Variables: tabel 6 kolom dengan label kolom ‘SHARED’ / ‘LOCAL’ agar beda initial vs current terbaca tanpa tooltip; titik aksen menandai current value yang berbeda. Di bawah tabel: pengingat bahwa hover variabel menunjukkan nilai + scope asalnya.",
    "Folder memakai layar yang sama: subtitle ‘Folder in Bookstore API · 6 requests’, tanpa Mock server, hint Variables ‘Folder variables override collection variables…’. State terhapus: empty state ‘This collection was deleted.’.",
  ]) + NE([
    "<code>container_tab::show</code>: ganti <code>view_header</code> (36 px tile) dengan baris judul 46 px yang sama dengan <code>request_tab::header</code>; tab tetap <code>kit::tab_strip_ex</code> (dot di Authorization).",
    "Tabel variabel: <code>env_tab</code> variables table dipakai ulang; header kolom mendapat tag kecil (<code>kit::pill</code> 10 px) SHARED/LOCAL.",
    "Kartu Actions: <code>kit::card</code> berisi <code>request_tab::list_row</code> yang memanggil aksi menu <code>sidebar::container_menu</code>.",
  ]) });

screen({ id: "env", group: "Workspace", label: "Environment: variabel", rail: "environments", sel: "local", tab: { id: "env-local", icon: "GLOBE_SIMPLE", t: "Local", go: "env" }, view: envView,
  notes: N([
    "Header 46 px: tile berwarna environment, nama, ‘Environment · 6 variables’, pill ‘Active’ (tooltip penjelasan) dan ⋯ (Set active/Rename/Duplicate/Export/Delete). Environment tidak aktif menampilkan tombol ‘Set active’ di tempat pill.",
    "Toolbar: filter nama + penjelasan initial (di-commit) vs current (lokal) — satu kalimat, selalu terlihat.",
    "Tabel memperlihatkan semua state: secret ter-mask + gembok + mata, baris nonaktif pudar, current kosong menampilkan initial sebagai placeholder, sel yang sedang diedit dengan cincin aksen, baris berbeda dengan titik aksen dan menu hover ‘Persist current value as initial’ / ‘Reset current value’.",
    "Dua kartu bantuan di bawah: Vault untuk secret lintas environment, dan cara cepat berpindah (⌘E / double-click).",
    "Globals memakai layar yang sama dengan subtitle ‘Global variables · 3 variables’ tanpa pill Active.",
  ]) + NE([
    "<code>env_tab::show</code> + variables table (<code>env_tab.rs:243</code>): grid kolom tetap 34 / 1fr / 112 / 1.25fr / 1.25fr / 64; aksi baris hanya digambar saat hover (<code>ui.rect_contains_pointer</code>).",
    "Penanda beda nilai: <code>painter.circle_filled</code> 3 px warna <code>accent</code> di ujung kanan sel current.",
    "Menu hover memakai <code>egui::Popup::menu</code> dari tombol ⋯ baris; item ‘Reset current value’ menampilkan nilai awal sebagai teks sekunder.",
  ]) });

screen({ id: "history", group: "Workspace", label: "History: entri dibuka sebagai tab", rail: "history", sel: "h0", tab: { id: "hist", m: "GET", t: "/books?page=1&limit=20", go: "history" }, preset: { resState: "empty" }, view: historyView,
  notes: N([
    "Sidebar History dikelompokkan per hari (TODAY / YESTERDAY / tanggal) dengan method, path tanpa host, dan status berwarna; hover menampilkan tombol ‘Open in a new tab’.",
    "Entri dibuka sebagai request baru yang belum disimpan. Bar info di bawah judul menjelaskan asalnya, status & waktu jawaban terakhir, dan aksi ‘Save to collection…’.",
    "History hanya menyimpan request + ringkasan (status, waktu), bukan body — karena itu panel response kosong sampai dikirim ulang; ringkasan jawaban lama tetap terlihat di bar info.",
  ]) + NE([
    "<code>sidebar::history</code> + <code>history_row</code>: tambahkan kolom jam (faint) dan aksi hover.",
    "<code>app.rs</code> <code>open_history</code>: simpan <code>HistoryEntry</code> asal di <code>RequestTab</code> (field baru <code>from_history</code>) agar <code>request_tab::show</code> bisa menggambar bar info 36 px (<code>request_tab::banner</code> varian Info) di bawah header.",
  ]) });

screen({ id: "spec", group: "Workspace", label: "Spec Hub: editor + docs (split)", rail: "apis", tab: { id: "spec", icon: "FILE_CODE", t: "bookstore.yaml", go: "spec" }, view: () => specView({}),
  notes: N([
    "Baris judul 46 px: specs › bookstore.yaml, pill jenis (OpenAPI) dan pill validitas; Save + ⋯ (Open docs in browser, Copy file path). Titik aksen untuk perubahan belum disimpan.",
    "Toolbar 46 px: Editor / Split / Docs di kiri; Validate, Sync ▾ dan satu aksi primer ‘Generate collection’ di kanan.",
    "Kiri: editor YAML dengan gutter; panel Problems menempel di bawah editor dengan posisi kursor, jenis dan ukuran — bertambah tinggi sesuai jumlah masalah (maks 5 baris).",
    "Kanan: pratinjau dokumentasi di latar ‘well’ agar terbedakan dari editor: judul, deskripsi, pill operasi + variabel, indeks Endpoints (klik = scroll), lalu kartu per operasi (Parameters, Request body, Responses).",
    "Format non-OpenAPI/Smithy (AsyncAPI, Protobuf, GraphQL) menyembunyikan segmented dan Generate/Sync, menampilkan ringkasan validasi di toolbar.",
  ]) + NE([
    "<code>spec_tab::header</code> / <code>toolbar</code>: header jadi 46 px seperti request; tombol tetap, urutan kanan → kiri: Generate (primer), Sync, Validate.",
    "<code>spec_tab::problems_panel</code>: fill <code>well</code>, baris 28 px, pill lokasi <code>kit::pill</code> outline.",
    "<code>spec_tab::docs_view</code> / <code>docs_page</code>: latar <code>well</code> di mode Split, lebar konten maks 780 di mode Docs.",
  ]) });

screen({ id: "spec-sync", group: "Workspace", label: "Spec Hub: Sync popover + catatan sync", rail: "apis", tab: { id: "spec", icon: "FILE_CODE", t: "bookstore.yaml", go: "spec" }, view: () => specView({ sync: true, warn: true, dirty: true }),
  notes: N([
    "Popover Sync (312 px) di bawah tombolnya: pilih koleksi (centang), lalu dua aksi sebagai kartu dengan penjelasan akibatnya — ‘Spec → collection’ dan ‘Collection → spec’ (hanya OpenAPI).",
    "Setelah sync, bar catatan info di bawah toolbar merangkum hasil (updated / added / dibiarkan) dengan ‘Open collection’ dan tombol tutup.",
    "Contoh state lain di layar ini: file belum disimpan (titik aksen), pill ‘1 warning’, dan panel Problems berisi satu peringatan yang bisa diklik untuk lompat ke baris.",
  ]) + NE([
    "<code>spec_tab::sync_menu</code>: lebar 312, opsi koleksi sebagai <code>list_row</code> dengan centang; aksi sebagai kartu 2 baris (judul + hint) alih-alih tombol polos.",
    "<code>spec_tab::sync_note</code>: bar 36 px fill <code>info_soft</code>, tombol ‘Open collection’ memanggil <code>app.open_container</code>.",
  ]) });
})();
