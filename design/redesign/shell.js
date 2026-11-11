// The app shell: title bar, activity rail, sidebars, tab strip, status bar,
// console drawer, menus and the dialog layer.

const RAIL = [
  ["collections", "STACK", "Collections"],
  ["environments", "GLOBE", "Environments"],
  ["history", "CLOCK_COUNTER_CLOCKWISE", "History"],
  ["apis", "BRACKETS_CURLY", "APIs"],
  ["flows", "FLOW_ARROW", "Flows"],
];

function appShell(s) {
  const rail = s.rail || "collections";
  return `
  ${titlebar()}
  <div class="body">
    ${railHtml(rail)}
    ${S.sidebar ? `<aside class="sidebar" style="width:${sidebarW()}px">${(SIDEBARS[rail] || SIDEBARS.collections)(s)}</aside>` : ""}
    <main class="bench">
      ${tabstrip(s)}
      <div class="view">${s.view ? s.view() : ""}</div>
      ${S.console || s.console ? consoleDrawer() : ""}
    </main>
  </div>
  ${statusbar()}
  ${S.menu && MENUS[S.menu] ? MENUS[S.menu]() : ""}
  ${S.dlg && DIALOGS[S.dlg] ? DIALOGS[S.dlg]() : ""}
  ${s.toast ? `<div class="toasts">${s.toast}</div>` : ""}`;
}

function titlebar() {
  const env = ENVS[0];
  return `<header class="titlebar">
    <div class="lights"><span></span><span></span><span></span></div>
    <div class="ws-switch" data-menu="workspace" title="~/Developer/bookstore">
      <img src="../../crates/ternarywire-app/assets/icon-256.png" alt=""><span class="name">Bookstore</span>${I("CARET_DOWN", "faint")}
    </div>
    <div class="vdiv"></div>
    ${btn("New", { kind: "ghost", icon: "PLUS", size: "sm", menu: "new", caret: true })}
    ${W() > 1000 ? btn("Import", { kind: "ghost", icon: "DOWNLOAD_SIMPLE", size: "sm", dlg: "import" }) : ""}
    <div class="cmdk" data-dlg="palette">${I("MAGNIFYING_GLASS")}<span class="ellipsis">Search requests, run a command…</span>${kbd("⌘K")}</div>
    <div class="row" style="gap:2px">
      <span class="selectbox sm" data-menu="env" title="Switch environment (⌘E)" style="min-width:132px"><span class="dot" style="background:${env.color}"></span><span class="grow">${env.name}</span>${I("CARET_DOWN", "car")}</span>
      ${ibtn("EYE", "Environment quick look", { dlg: "quicklook" })}
      <div class="vdiv" style="margin:0 4px"></div>
      ${ibtn("COOKIE", "Cookies", { dlg: "cookies" })}
      ${ibtn("LOCK_KEY", "Vault", { dlg: "vault" })}
    </div>
  </header>`;
}

function railHtml(active) {
  const item = ([id, icon, label]) => {
    const go = { collections: "http", environments: "env", history: "history", apis: "spec", flows: "flow", tools: "mock" }[id];
    const dot = id === "tools" ? "" : id === "flows" ? "" : "";
    return `<button class="rbtn ${active === id && S.sidebar ? "on" : ""}" title="${label}" aria-label="${label}" data-go="${go}">${I(icon)}${id === "tools" ? '<span class="ndot"></span>' : dot}</button>`;
  };
  return `<nav class="rail">
    ${RAIL.map(item).join("")}
    <div class="rsep"></div>
    ${item(["tools", "BROADCAST", "Local servers"])}
    <div class="spacer"></div>
    <button class="rbtn ${S.console ? "on" : ""}" title="Console (⌥⌘C)" data-set="console=!">${I("TERMINAL_WINDOW")}<span class="ndot err"></span></button>
    <button class="rbtn" title="Settings (⌘,)" data-dlg="settings">${I("GEAR_SIX")}</button>
  </nav>`;
}

/* tab strip ------------------------------------------------------------ */
function tabstrip(s) {
  if (s.noTabs) return `<div class="tabstrip"><button class="tplus" title="New request (⌘T)">${I("PLUS")}</button></div>`;
  const tabs = [...OPEN_TABS];
  if (s.tab && !tabs.find((t) => t.id === s.tab.id)) tabs.push(s.tab);
  const activeId = s.tab ? s.tab.id : s.tabId;
  const shown = tabs.slice(-6);
  return `<div class="tabstrip">
    ${shown.map((t) => `<div class="dtab ${t.id === activeId ? "on" : ""} ${t.dirty ? "dirty" : ""}" data-go="${t.go}" title="${t.t}">
      ${t.m ? `<span class="meth m-${t.m}">${t.m}</span>` : I(t.icon, "faint")}
      <span class="t">${t.t}</span>${t.dirty ? '<span class="udot"></span>' : ""}<span class="x">${I("X")}</span></div>`).join("")}
    <button class="tplus" title="New request (⌘T)" data-menu="new">${I("PLUS")}</button>
    <div class="tend">${ibtn("CARET_DOWN", "All open tabs", { sm: true, menu: "tabs" })}</div>
  </div>`;
}

/* status bar ----------------------------------------------------------- */
function statusbar() {
  return `<footer class="statusbar">
    <button class="sbtn" data-set="sidebar=!" title="Toggle sidebar (⌘B)">${I("SIDEBAR_SIMPLE")}</button>
    <button class="sbtn" data-set="console=!" title="Console (⌥⌘C)">${I("TERMINAL_WINDOW")}Console<span class="pill err" style="height:16px;padding:0 5px;font-size:10.5px">1</span></button>
    <span class="sbtn" style="cursor:default">${I("CHECK", "ok")}All changes saved</span>
    <div class="spacer"></div>
    <button class="sbtn" data-dlg="update" style="color:var(--accent-text)">${I("ARROW_LINE_DOWN")}Update 0.3.0</button>
    <span class="sbtn" title="~/Developer/bookstore">${I("FOLDER_SIMPLE")}~/Developer/bookstore</span>
    <div class="vdiv" style="height:14px"></div>
    <button class="sbtn" data-set="layout=${S.layout === "side" ? "below" : "side"}" title="Response position: ${S.layout === "side" ? "beside" : "below"} the request">${I(S.layout === "side" ? "COLUMNS" : "ROWS")}${S.layout === "side" ? "Side by side" : "Stacked"}</button>
    <button class="sbtn" data-dlg="palette" title="Command palette (⇧⌘P)">${I("COMMAND")}</button>
  </footer>`;
}

/* console drawer ------------------------------------------------------- */
function consoleDrawer() {
  const icon = { net: ["GLOBE_SIMPLE", "ok"], log: ["TERMINAL", "muted"], warn: ["WARNING", "warn"], err: ["WARNING_CIRCLE", "err"] };
  return `<div class="hgutter" style="margin-top:-4px"></div>
  <section style="height:210px;flex:none;display:flex;flex-direction:column;border-top:0">
    <div class="row" style="height:36px;padding:0 12px;gap:10px;border-bottom:1px solid var(--border)">
      <span class="section-title row" style="gap:6px">${I("TERMINAL_WINDOW", "muted")}Console</span>
      ${seg("cf", S.cf || "all", [["all", "All"], ["logs", "Logs"], ["net", "Network"], ["err", "Errors 1"]], { sm: true })}
      <span class="spacer"></span>
      ${search("Filter", { sm: true, style: "width:200px" })}
      ${ibtn("COPY", "Copy lines", { sm: true })}${ibtn("BROOM", "Clear console", { sm: true })}${ibtn("X", "Close console (⌥⌘C)", { sm: true, set: "console=false" })}
    </div>
    <div class="scroll grow mono" style="font-size:12px">
      ${CONSOLE.map(([t, k, msg, src]) => `<div class="row" style="height:26px;padding:0 12px;gap:10px;${k === "err" ? "background:var(--err-soft)" : k === "warn" ? "background:var(--warn-soft)" : ""}">
        <span class="faint" style="width:86px">${t}</span>${I(icon[k][0], icon[k][1])}
        <span class="grow ellipsis">${esc(msg)}</span><span class="faint xs" style="font-family:Inter">${src}</span></div>`).join("")}
    </div>
  </section>`;
}

/* sidebars ------------------------------------------------------------- */
function sbHead(title, actions) {
  return `<div class="sb-head"><h2>${title}</h2>${actions}</div>`;
}
function treeRows(items, depth, sel) {
  return items.map((it) => {
    const pad = 4 + depth * 14;
    if (it.t === "req") {
      return `<div class="tr ${sel === it.id ? "sel" : ""}" style="padding-left:${pad}px" ${it.go ? `data-go="${it.go}"` : ""}>
        <span class="car"></span>${meth(it.m)}<span class="nm">${it.name}</span>
        <span class="acts">${ibtn("DOTS_THREE", "More actions", { sm: true, menu: "req-ctx" })}</span></div>`;
    }
    const icon = it.t === "coll" ? "STACK" : it.open ? "FOLDER_OPEN" : "FOLDER_SIMPLE";
    const row = `<div class="tr ${it.t === "coll" ? "coll" : ""}" style="padding-left:${pad}px" ${it.t === "coll" ? `data-go="container"` : ""}>
      <span class="car">${I(it.open ? "CARET_DOWN" : "CARET_RIGHT")}</span>${I(icon, "ic")}<span class="nm">${it.name}</span>
      ${it.fork ? pill("fork", "info", { title: "Forked from " + it.fork }) : ""}
      ${it.t === "coll" ? `<span class="meta">${countReqs(it)}</span>` : ""}
      <span class="acts">${ibtn("PLUS", "Add request", { sm: true, menu: "add" })}${ibtn("DOTS_THREE", "More actions", { sm: true, menu: it.t === "coll" ? "coll-ctx" : "folder-ctx" })}</span></div>`;
    return row + (it.open && it.children.length ? `<div class="guide" style="--gx:${pad + 7}px">${treeRows(it.children, depth + 1, sel)}</div>` : "");
  }).join("");
}
function countReqs(n) { return n.children ? n.children.reduce((a, c) => a + (c.t === "req" ? 1 : countReqs(c)), 0) || "" : ""; }

const SIDEBARS = {
  collections: (s) => `${sbHead("Collections", ibtn("PLUS", "New collection", { sm: true, menu: "new" }) + ibtn("DOWNLOAD_SIMPLE", "Import", { sm: true, dlg: "import" }) + ibtn("DOTS_THREE", "More", { sm: true }))}
    <div class="sb-search">${search("Filter requests", { sm: true })}</div>
    <div class="sb-list scroll tree">${treeRows(TREE, 0, s.sel)}</div>`,

  environments: (s) => `${sbHead("Environments", ibtn("PLUS", "New environment", { sm: true }))}
    <div class="sb-search">${search("Filter environments", { sm: true })}</div>
    <div class="sb-list scroll tree">
      <div class="tr ${s.sel === "globals" ? "sel" : ""}" style="padding-left:8px">${I("GLOBE_SIMPLE", "ic")}<span class="nm">Globals</span><span class="meta">3</span></div>
      <div class="grp">Environments</div>
      ${ENVS.map((e, i) => `<div class="tr ${s.sel === e.id ? "sel" : ""}" style="padding-left:8px" data-go="env">
        <span title="${i === 0 ? "Active — click to deactivate" : "Set active"}" style="display:grid;place-items:center;width:16px">${i === 0 ? I("CHECK_CIRCLE", "ok") : `<span class="dot" style="background:${e.color};opacity:.9"></span>`}</span>
        <span class="nm">${e.name}</span>${i === 0 ? pill("Active", "ok") : ""}<span class="meta">${e.vars}</span>
        <span class="acts">${ibtn("DOTS_THREE", "More actions", { sm: true, menu: "env-ctx" })}</span></div>`).join("")}
    </div>
    <div class="sb-foot">Double-click an environment to make it active. Initial values are committed; current values stay on this Mac.</div>`,

  history: (s) => `${sbHead("History", ibtn("BROOM", "Clear history", { sm: true }))}
    <div class="sb-search">${search("Search history", { sm: true })}</div>
    <div class="sb-list scroll tree">${HISTORY.map(([day, rows]) => `<div class="grp">${day}</div>` + rows.map(([m, u, st, t], i) => {
      const k = st === "Error" ? "err" : st < 300 ? "ok" : st < 400 ? "info" : st < 500 ? "warn" : "err";
      return `<div class="tr ${s.sel === "h" + i && day === "Today" ? "sel" : ""}" style="padding-left:6px" title="${u}">
        ${meth(m)}<span class="nm mono" style="font-size:11.5px">${u.replace(/^[^/]+/, "")}</span><span class="meta mono" style="color:var(--${k});font-size:11px">${st}</span>
        <span class="acts">${ibtn("ARROW_SQUARE_OUT", "Open in a new tab", { sm: true })}</span></div>`;
    }).join("")).join("")}</div>`,

  apis: (s) => `${sbHead("API specs", ibtn("PLUS", "New spec", { sm: true, menu: "new-spec" }))}
    <div class="sb-search">${search("Filter specs", { sm: true })}</div>
    <div class="sb-list scroll tree">
      ${[["bookstore.yaml", "OpenAPI", "FILE_CODE", "--m-get", true], ["users.smithy", "Smithy", "TREE_STRUCTURE", "--m-post"], ["chat.asyncapi.yaml", "AsyncAPI", "BROADCAST", "--m-ws"], ["greeter.proto", "Protobuf", "CUBE", "--m-grpc"], ["schema.graphql", "GraphQL", "GRAPH", "--m-gql"]]
        .map(([n, k, ic, c, on]) => `<div class="tr ${on && s.rail === "apis" ? "sel" : ""}" style="padding-left:8px" data-go="spec"><span style="color:var(${c});font-size:15px">${I(ic)}</span><span class="nm">${n}</span><span class="meta">${k}</span><span class="acts">${ibtn("DOTS_THREE", "More actions", { sm: true })}</span></div>`).join("")}
    </div>
    <div class="sb-foot">Specs live in <span class="mono" style="font-size:11px">specs/</span> inside the workspace and can be committed to Git.</div>`,

  flows: (s) => `${sbHead("Flows", ibtn("PLUS", "New flow", { sm: true }))}
    <div class="sb-search">${search("Filter flows", { sm: true })}</div>
    <div class="sb-list scroll tree">
      <div class="tr ${s.rail === "flows" ? "sel" : ""}" style="padding-left:8px" data-go="flow">${I("FLOW_ARROW", "accent")}<span class="nm">Catalog summary</span><span class="meta">7 blocks</span><span class="acts">${ibtn("PLAY", "Run", { sm: true })}${ibtn("DOTS_THREE", "More actions", { sm: true })}</span></div>
      <div class="tr" style="padding-left:8px">${I("CIRCLE_NOTCH", "spin accent")}<span class="nm">Checkout smoke test</span><span class="meta">12 blocks</span><span class="acts">${ibtn("STOP", "Stop", { sm: true })}${ibtn("DOTS_THREE", "More actions", { sm: true })}</span></div>
      <div class="tr" style="padding-left:8px">${I("FLOW_ARROW", "ic")}<span class="nm">Nightly sync</span><span class="udot" style="width:6px;height:6px;border-radius:50%;background:var(--accent)"></span><span class="meta">5 blocks</span><span class="acts">${ibtn("PLAY", "Run", { sm: true })}${ibtn("DOTS_THREE", "More actions", { sm: true })}</span></div>
    </div>
    <div class="sb-foot">Flows are saved in <span class="mono" style="font-size:11px">flows/</span> and also run with <span class="mono" style="font-size:11px">ternarywire flow run</span>.</div>`,

  tools: (s) => `${sbHead("Local servers", ibtn("PLUS", "New server", { sm: true, menu: "new-tool" }))}
    <div class="sb-list scroll tree">
      <div class="grp">Mock servers</div>
      <div class="tr ${s.id === "mock" ? "sel" : ""}" style="padding-left:8px" data-go="mock">${I("CUBE", "ic")}<span class="nm">Bookstore API</span>${pill(":3000", "ok", { dot: true, live: true })}</div>
      <div class="tr" style="padding-left:8px">${I("CUBE", "ic")}<span class="nm">Payments</span><span class="meta">stopped</span></div>
      <div class="grp">Webhook listeners</div>
      <div class="tr ${s.id === "webhook" ? "sel" : ""}" style="padding-left:8px" data-go="webhook">${I("WEBHOOKS_LOGO", "ic")}<span class="nm">Stripe events</span>${pill(":8787 · 3", "ok", { dot: true, live: true })}</div>
      <div class="grp">Capture proxy</div>
      <div class="tr ${s.id === "capture" ? "sel" : ""}" style="padding-left:8px" data-go="capture">${I("BROADCAST", "ic")}<span class="nm">Capture traffic</span>${pill(":8888", "ok", { dot: true, live: true, title: "Recording" })}</div>
    </div>
    <div class="sb-foot">Servers run on this machine only. Stopped servers keep their settings.</div>`,
};

/* menus ---------------------------------------------------------------- */
function placeMenu(html, o = {}) {
  const p = S.menuPos || { x: 100, y: 100 };
  const style = o.right ? `right:${p.r}px;top:${p.y}px` : `left:${p.x}px;top:${p.y}px`;
  return html.replace('<div class="menu" style="', `<div class="menu" data-stop="1" style="${style};${o.w ? `min-width:${o.w}px;` : ""}`);
}
Object.assign(MENUS, {
  new: () => placeMenu(menu([
    "Requests",
    { icon: "GLOBE", label: "HTTP request", kbd: "⌘N", go: "http" },
    { icon: "GRAPH", label: "GraphQL request", go: "http-graphql" },
    { icon: "PLUGS_CONNECTED", label: "WebSocket", go: "ws" },
    { icon: "PLUGS", label: "Socket.IO", go: "sio" },
    { icon: "WIFI_HIGH", label: "MQTT", go: "mqtt" },
    { icon: "LIGHTNING", label: "gRPC", go: "grpc" },
    { icon: "PUZZLE_PIECE", label: "MCP", go: "mcp" },
    { icon: "SPARKLE", label: "AI request", go: "ai" },
    "-", "Local servers",
    { icon: "CUBE", label: "Mock server…", go: "mock" },
    { icon: "WEBHOOKS_LOGO", label: "Webhook listener", go: "webhook" },
    { icon: "BROADCAST", label: "Capture traffic (proxy)", go: "capture" },
    "-", "Organize",
    { icon: "STACK", label: "Collection", go: "container" },
    { icon: "GLOBE_SIMPLE", label: "Environment", go: "env" },
    { icon: "FLOW_ARROW", label: "Flow", go: "flow" },
    { icon: "FILE_CODE", label: "API spec", go: "spec" },
  ]), { w: 240 }),
  workspace: () => placeMenu(menu([
    "Workspace",
    { icon: "CHECK", label: "Bookstore", sub: "~/Developer/bookstore" },
    { icon: "FOLDER_SIMPLE", label: "Payments platform", sub: "~/work/payments" },
    { icon: "FOLDER_SIMPLE", label: "My Workspace", sub: "Application Support" },
    "-",
    { icon: "FOLDER_OPEN", label: "Open workspace…", kbd: "⇧⌘O" },
    { icon: "FOLDER_PLUS", label: "New workspace…", dlg: "new-ws" },
    { icon: "ARROW_SQUARE_OUT", label: "Reveal in Finder" },
  ]), { w: 300 }),
  env: () => placeMenu(menu([
    { label: "No environment", check: false },
    ...ENVS.map((e, i) => ({ dot: e.color, label: e.name, sub: `${e.vars} variables`, on: i === 0 })),
    "-",
    { icon: "EYE", label: "Quick look", dlg: "quicklook" },
    { icon: "SLIDERS_HORIZONTAL", label: "Manage environments", go: "env" },
  ]), { right: true, w: 240 }),
  tabs: () => placeMenu(menu([
    "Open tabs",
    ...OPEN_TABS.map((t) => ({ meth: t.m, label: t.t, go: t.go })),
    "-",
    { icon: "X_CIRCLE", label: "Close other tabs" },
    { icon: "X", label: "Close all tabs" },
  ]), { right: true, w: 240 }),
  "coll-ctx": () => placeMenu(menu([
    { icon: "GLOBE", label: "Add request" }, { icon: "PLUS", label: "Add other request", more: true }, { icon: "FOLDER_PLUS", label: "Add folder" },
    "-",
    { icon: "PLAY", label: "Run collection", go: "runner" }, { icon: "CUBE", label: "Mock server", go: "mock" }, { icon: "SLIDERS_HORIZONTAL", label: "Settings", go: "container" },
    "-",
    { icon: "PENCIL_SIMPLE", label: "Rename" }, { icon: "COPY", label: "Duplicate" }, { icon: "GIT_FORK", label: "Fork…" },
    { icon: "EXPORT", label: "Export", more: true }, { icon: "BOOK_OPEN", label: "Documentation", more: true },
    "-",
    { icon: "TRASH", label: "Delete…", danger: true, dlg: "confirm" },
  ]), { w: 220 }),
  "folder-ctx": () => MENUS["coll-ctx"](),
  "req-ctx": () => placeMenu(menu([
    { icon: "ARROW_SQUARE_OUT", label: "Open in new tab" }, { icon: "PENCIL_SIMPLE", label: "Rename" }, { icon: "COPY", label: "Duplicate", kbd: "⌘D" },
    { icon: "TERMINAL", label: "Copy as cURL" }, "-", { icon: "TRASH", label: "Delete…", danger: true, dlg: "confirm" },
  ]), { w: 210 }),
  add: () => MENUS.new(),
  "env-ctx": () => placeMenu(menu([
    { icon: "CHECK_CIRCLE", label: "Set active" }, { icon: "PENCIL_SIMPLE", label: "Rename" }, { icon: "COPY", label: "Duplicate" },
    { icon: "EXPORT", label: "Export (Postman)" }, "-", { icon: "TRASH", label: "Delete…", danger: true },
  ]), { w: 200 }),
  "new-spec": () => placeMenu(menu([{ icon: "FILE_CODE", label: "New OpenAPI spec" }, { icon: "TREE_STRUCTURE", label: "New Smithy model" }, "-", { icon: "FOLDER_OPEN", label: "Open file…" }]), { w: 210 }),
  "new-tool": () => placeMenu(menu([{ icon: "CUBE", label: "Mock server for…", more: true }, { icon: "WEBHOOKS_LOGO", label: "Webhook listener" }, { icon: "BROADCAST", label: "Capture traffic (proxy)" }]), { w: 230 }),
});
