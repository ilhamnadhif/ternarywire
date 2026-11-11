// Dialogs, modals and the console panel. One anatomy for every dialog:
// header (icon tile, title, subtitle) · body · footer (hint left, Cancel, primary rightmost).
// Palette-style pickers (palette, env switcher) are anchored to the top and have no header.

document.head.insertAdjacentHTML("beforeend", `<style id="css-dialogs">
.modal { max-width: calc(100% - 40px); }
.modal .mb.col > * { flex-shrink: 0; }
.modal .field > .seg { align-self: flex-start; }
.dl-pal-row .lead > .i.ok { color: var(--ok); } .dl-pal-row .lead > .i.faint { color: var(--text-faint); }
.modal .mh h3 .dl-msub { display: block; font-size: 12px; font-weight: 400; color: var(--text-muted); margin-top: 2px; }
.modal .mb.dl-flush { padding: 0; display: flex; min-height: 0; overflow: hidden; }
.modal .mf .dl-keyhints { display: flex; align-items: center; gap: 5px; color: var(--text-muted); font-size: 12px; }
.modal .mf .dl-keyhints .sp { width: 8px; }
.dl-pal-in { display: flex; align-items: center; gap: 10px; height: 54px; padding: 0 16px; border-bottom: 1px solid var(--border); flex: none; }
.dl-pal-in > .i { font-size: 18px; color: var(--text-faint); }
.dl-pal-in input { flex: 1; border: 0; outline: 0; background: none; font-size: 15px; color: var(--text); min-width: 0; }
.dl-pal-in input::placeholder { color: var(--text-faint); }
.dl-pal-list { padding: 4px 8px 8px; overflow: auto; min-height: 0; flex: 1; }
.dl-pal-grp { padding: 10px 10px 4px; font-size: 10.5px; font-weight: 600; letter-spacing: .06em; text-transform: uppercase; color: var(--text-faint); }
.dl-pal-row { display: flex; align-items: center; gap: 10px; height: 36px; padding: 0 10px; border-radius: 7px; font-size: 13px; }
.dl-pal-row:hover { background: var(--hover); }
.dl-pal-row.on { background: var(--accent-soft); }
.dl-pal-row .lead { width: 38px; display: flex; justify-content: center; flex: none; }
.dl-pal-row .lead > .i { font-size: 16px; color: var(--text-muted); }
.dl-pal-row .det { color: var(--text-faint); font-size: 12px; }
.dl-pal-row b.mt { color: var(--accent-text); font-weight: 600; }
.dl-pal-row .ret { color: var(--accent-text); font-size: 12px; display: inline-flex; align-items: center; gap: 4px; }
.dl-set-nav { width: 196px; flex: none; border-right: 1px solid var(--border); background: var(--well); padding: 10px 8px; display: flex; flex-direction: column; gap: 2px; }
.dl-set-nav .ni { display: flex; align-items: center; gap: 9px; height: 32px; padding: 0 10px; border-radius: 7px; color: var(--text-muted); font-weight: 500; font-size: 12.5px; }
.dl-set-nav .ni:hover { background: var(--hover); color: var(--text); }
.dl-set-nav .ni.on { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--border), var(--shadow-card); }
.dl-set-nav .ni .i { font-size: 15px; }
.dl-set-nav .ni.on .i { color: var(--accent-text); }
.dl-set-page { flex: 1; overflow: auto; padding: 20px 24px 28px; display: flex; flex-direction: column; gap: 20px; min-width: 0; }
.dl-set-h h2 { margin: 0; font-size: 16px; font-weight: 600; }
.dl-set-h p { margin: 2px 0 0; color: var(--text-muted); font-size: 12.5px; }
.dl-slider { position: relative; width: 150px; height: 18px; display: inline-block; flex: none; }
.dl-slider .trk { position: absolute; left: 0; right: 0; top: 7px; height: 4px; border-radius: 2px; background: var(--subtle); }
.dl-slider .fil { position: absolute; left: 0; top: 7px; height: 4px; border-radius: 2px; background: var(--accent); }
.dl-slider .knb { position: absolute; top: 1px; width: 16px; height: 16px; margin-left: -8px; border-radius: 50%; background: #fff; box-shadow: 0 0 0 1px var(--border-strong), 0 1px 3px rgba(0,0,0,.2); }
.dl-dz { border: 1.5px dashed var(--border-strong); border-radius: 12px; height: 112px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; background: var(--well); text-align: center; flex: none; }
.dl-dz .i.big { font-size: 24px; color: var(--accent-text); margin-bottom: 2px; }
.dl-or-line { display: flex; align-items: center; gap: 10px; color: var(--text-faint); font-size: 11.5px; }
.dl-or-line::before, .dl-or-line::after { content: ""; flex: 1; height: 1px; background: var(--border); }
.dl-vlist { border: 1px solid var(--border); border-radius: 9px; overflow: hidden; background: var(--surface); }
.dl-vrow { display: flex; align-items: center; gap: 10px; min-height: 44px; padding: 0 10px 0 12px; border-top: 1px solid var(--border); }
.dl-vrow:first-child { border-top: 0; }
.dl-vrow .dl-ra { display: flex; gap: 2px; visibility: hidden; }
.dl-vrow:hover .dl-ra, .dl-vrow.hov .dl-ra { visibility: visible; }
.dl-vrow:hover, .dl-vrow.hov { background: var(--well); }
.dl-vrow.sel { background: var(--accent-soft); }
.dl-tile { width: 28px; height: 28px; border-radius: 7px; display: grid; place-items: center; background: var(--well); border: 1px solid var(--border); color: var(--text-muted); font-size: 15px; flex: none; }
.dl-tile.acc { background: var(--accent-soft); border-color: transparent; color: var(--accent-text); }
.dl-lpane { width: 230px; flex: none; border-right: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; background: var(--well); }
.dl-lpane .lh { height: 38px; display: flex; align-items: center; gap: 8px; padding: 0 12px; border-bottom: 1px solid var(--border); flex: none; }
.dl-lpane .ll { flex: 1; overflow: auto; padding: 6px; display: flex; flex-direction: column; gap: 1px; }
.dl-lrow { display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 10px; border-radius: 7px; font-size: 12.5px; flex: none; }
.dl-lrow:hover { background: var(--hover); }
.dl-lrow.on { background: var(--surface); box-shadow: 0 0 0 1px var(--border), var(--shadow-card); }
.dl-lrow .cnt { margin-left: auto; color: var(--text-faint); font-size: 11.5px; }
.dl-lrow .var2 { color: var(--text-faint); font-size: 11.5px; margin-left: auto; }
.dl-rpane { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
.dl-danger-circle { width: 44px; height: 44px; border-radius: 50%; display: grid; place-items: center; background: var(--err-soft); color: var(--err); font-size: 22px; flex: none; }
.dl-warn-circle { width: 44px; height: 44px; border-radius: 50%; display: grid; place-items: center; background: var(--warn-soft); color: var(--warn); font-size: 22px; flex: none; }
.dl-steps { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
.dl-steps .st { display: inline-flex; align-items: center; gap: 6px; color: var(--text-muted); }
.dl-steps .st.done { color: var(--ok); } .dl-steps .st.now { color: var(--text); font-weight: 500; }
.dl-relnotes { max-height: 210px; overflow: auto; padding: 12px 14px; font-size: 12.5px; }
.dl-relnotes h5 { margin: 10px 0 4px; font-size: 12px; font-weight: 600; } .dl-relnotes h5:first-child { margin-top: 0; }
.dl-relnotes ul { margin: 0; padding-left: 18px; } .dl-relnotes li { margin: 3px 0; color: var(--text-muted); }
.dl-diffv { font-family: "JBMono", monospace; font-size: 12px; }
.dl-diffv .dr { display: flex; height: 22px; align-items: center; white-space: pre; }
.dl-diffv .dr.add { background: var(--ok-soft); } .dl-diffv .dr.del { background: var(--err-soft); }
.dl-diffv .dr .sg { width: 26px; text-align: center; flex: none; }
.dl-diffv .dr .ln { width: 34px; text-align: right; color: var(--text-faint); flex: none; padding-right: 6px; }
.dl-qrow { display: grid; grid-template-columns: minmax(0, .9fr) minmax(0, 1.3fr); gap: 12px; align-items: center; min-height: 30px; padding: 0 14px; border-top: 1px solid var(--border); font-size: 12px; }
.dl-qrow:first-of-type { border-top: 0; }
.dl-pathbox { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px; border-radius: 7px; border: 1px solid var(--border); background: var(--well); font-family: "JBMono", monospace; font-size: 12px; min-width: 0; flex: 1; }
.dl-tree-pick .tp { display: flex; align-items: center; gap: 8px; height: 30px; padding-right: 10px; border-radius: 6px; font-size: 12.5px; }
.dl-tree-pick .tp:hover { background: var(--hover); }
.dl-tree-pick .tp.on { background: var(--accent-soft); color: var(--accent-text); font-weight: 500; }
.dl-tree-pick .tp .i { color: var(--text-muted); font-size: 15px; }
.dl-tree-pick .tp.on .i { color: var(--accent-text); }
.dl-codebox { border: 1px solid var(--border); border-radius: 8px; background: var(--well); overflow: hidden; }
</style>`);

// Everything below is scoped so helper names never clash with other screen files.
(() => {
/* ------------------------------------------------------------------ helpers */
const sub = (title, subtitle) => `${title}${subtitle ? `<span class="dl-msub">${subtitle}</span>` : ""}`;
const foot = (left, right) => `${left || ""}<span class="spacer"></span>${right}`;
const keys = (...ks) => ks.map(kbd).join("");
const hints = (pairs) => `<div class="dl-keyhints">${pairs.map(([k, l]) => `${k}<span>${l}</span>`).join('<span class="sp"></span>')}</div>`;
const numUnit = (v, unit, w = 96) => `<div class="row" style="gap:6px">${input(v, { sm: true, style: `width:${w}px;text-align:right` })}${unit ? `<span class="faint">${unit}</span>` : ""}</div>`;
const slider = (pct) => `<span class="dl-slider"><span class="trk"></span><span class="fil" style="width:${pct}%"></span><span class="knb" style="left:${pct}%"></span></span>`;
const dim = (html) => html.replace('class="frow"', 'class="frow" style="opacity:.45"');
function hl(name, q) {
  if (!q) return esc(name);
  const i = name.toLowerCase().indexOf(q.toLowerCase());
  if (i < 0) return esc(name);
  return esc(name.slice(0, i)) + `<b class="mt">${esc(name.slice(i, i + q.length))}</b>` + esc(name.slice(i + q.length));
}
const masked = (o = {}) => `<div class="row" style="gap:0;position:relative;${o.style || ""}">${input(o.ph ? "" : "••••••••••••", { sm: true, ph: o.ph, style: "width:100%;padding-right:30px" })}<span style="position:absolute;right:4px">${ibtn("EYE", "Show value", { sm: true })}</span></div>`;
const stored = (label = "Stored in the Keychain") => `<div class="row" style="gap:8px"><span class="pill outline">${I("LOCK_SIMPLE")}${label}</span>${btn("Change", { size: "sm", kind: "ghost" })}</div>`;

/* ------------------------------------------------------------------ command palette */
const PAL_CREATE = [
  ["GLOBE", "New Request", "", "⌘N"], ["PLUGS_CONNECTED", "New WebSocket"], ["LIGHTNING", "New gRPC"], ["WIFI_HIGH", "New MQTT"],
  ["PLUGS", "New Socket.IO"], ["PUZZLE_PIECE", "New MCP"], ["SPARKLE", "New AI request"], ["WEBHOOKS_LOGO", "New webhook listener"],
  ["BROADCAST", "Capture traffic (proxy)"], ["FLOW_ARROW", "New flow"], ["STACK", "New Collection"], ["GLOBE_SIMPLE", "New Environment"], ["DOWNLOAD_SIMPLE", "Import", "", "⌘O"],
];
const PAL_ACTIONS = [
  ["GLOBE", "Switch environment", "", "⌘E"], ["EYE", "Quick look"], ["SIDEBAR_SIMPLE", "Toggle sidebar", "", "⌘B"], ["TERMINAL_WINDOW", "Toggle console", "", "⌥⌘C"],
  ["COOKIE", "Manage cookies"], ["LOCK_KEY", "Manage vault"], ["FOLDER_OPEN", "Open workspace…"], ["FOLDER_PLUS", "New workspace…"], ["GEAR_SIX", "Settings", "", "⌘,"], ["ARROWS_CLOCKWISE", "Check for updates…"],
];
const PAL_REQS = [
  ["GET", "List books", "Bookstore API › Books"], ["GET", "Get book", "Bookstore API › Books"], ["POST", "Create book", "Bookstore API › Books"],
  ["PUT", "Update book", "Bookstore API › Books"], ["DEL", "Delete book", "Bookstore API › Books"], ["GQL", "Search catalog", "Bookstore API › Books"],
  ["POST", "Login", "Bookstore API › Auth"], ["GET", "Current user", "Bookstore API › Auth"], ["WS", "Echo socket", "Realtime & AI"], ["AI", "Assistant", "Realtime & AI"],
];
function palRow(lead, name, det, right, on) {
  return `<div class="dl-pal-row ${on ? "on" : ""}"><span class="lead">${lead}</span><span class="ellipsis">${name}</span>${det ? `<span class="det ellipsis">${det}</span>` : ""}<span class="spacer"></span>${on ? `<span class="ret">${I("ARROW_BEND_UP_LEFT")}Open</span>` : right || ""}</div>`;
}
DIALOGS.palette = () => {
  const q = S.sub === "q" ? "book" : "";
  let list, count;
  if (q) {
    const rows = PAL_REQS.filter((r) => r[1].toLowerCase().includes(q));
    count = rows.length;
    list = rows.map(([m, n, p], i) => palRow(meth(m), hl(n, q), p, "", i === 0)).join("");
  } else {
    let first = true;
    const row = ([icon, n, det, k]) => { const r = palRow(I(icon), n, det, k ? kbd(k) : "", first); first = false; return r; };
    list = `<div class="dl-pal-grp">Create</div>${PAL_CREATE.slice(0, 4).map(row).join("")}
      <div class="dl-pal-grp">Actions</div>${PAL_ACTIONS.slice(0, 4).map(row).join("")}
      <div class="dl-pal-grp">Requests</div>${PAL_REQS.slice(0, 4).map(([m, n, p]) => palRow(meth(m), n, p)).join("")}
      <div class="dl-pal-grp">Environments</div>${palRow(I("CIRCLE"), "No environment", "")}${ENVS.map((e, i) => palRow(`<span class="dot" style="background:${e.color}"></span>`, e.name, `${e.vars} variables`, i === 0 ? I("CHECK", "accent") : "")).join("")}
      <div class="dl-pal-grp">Run collection</div>${palRow(I("PLAY"), "Bookstore API", "Collection runner")}${palRow(I("PLAY"), "Realtime & AI", "Collection runner")}
      <div class="dl-pal-grp">Preferences</div>${palRow(I("CIRCLE_HALF"), "Theme: System", "", I("CHECK", "accent"))}${palRow(I("SUN"), "Theme: Light")}${palRow(I("MOON"), "Theme: Dark")}${palRow(I("TRANSLATE"), "Language: English", "", I("CHECK", "accent"))}${palRow(I("TRANSLATE"), "Language: Bahasa Indonesia")}`;
    count = PAL_CREATE.length + PAL_ACTIONS.length + PAL_REQS.length + 4 + 2 + 5;
  }
  const body = `<div class="dl-pal-in">${I("MAGNIFYING_GLASS")}<input placeholder="${q ? "Search requests…" : "Search requests and commands…"}" value="${q}" />${kbd("esc")}</div>
    <div class="dl-pal-list" style="max-height:${q ? 300 : 440}px">${list}</div>`;
  return modal(null, body, foot(hints([[keys("↑", "↓"), "Navigate"], [kbd("↩"), "Open"], [kbd("esc"), "Close"]]), `<span class="faint small">${count} results</span>`), { w: 660 });
};

/* ------------------------------------------------------------------ environment switcher + quick look */
DIALOGS["env-switch"] = () => {
  const rows = [palRow(I("CIRCLE", "faint"), "No environment", "")].concat(ENVS.map((e, i) =>
    palRow(i === 0 ? I("CHECK_CIRCLE", "ok") : `<span class="dot" style="background:${e.color}"></span>`, e.name, `${e.vars} variables`, i === 0 ? pill("Active", "ok") : "", i === 1)));
  const body = `<div class="dl-pal-in">${I("GLOBE")}<input placeholder="Switch environment…" />${kbd("esc")}</div><div class="dl-pal-list">${rows.join("").replace('<span class="ret">' + I("ARROW_BEND_UP_LEFT") + "Open", '<span class="ret">' + I("ARROW_BEND_UP_LEFT") + "Activate")}</div>`;
  return modal(null, body, foot(hints([[keys("↑", "↓"), "Navigate"], [kbd("↩"), "Activate"]]), btn("Edit variables", { kind: "ghost", size: "sm", icon: "SLIDERS_HORIZONTAL", go: "env" })), { w: 500 });
};

DIALOGS.quicklook = () => {
  const qcard = (head, rows, edit) => `<div class="card" style="overflow:hidden">
      <div class="row" style="height:42px;padding:0 10px 0 14px;border-bottom:1px solid var(--border)">${head}<span class="spacer"></span>${btn("Edit", { kind: "ghost", size: "sm", icon: "PENCIL_SIMPLE", go: edit })}</div>
      ${rows.map(([k, v, sec]) => `<div class="dl-qrow"><span class="mono muted ellipsis">${k}</span><span class="mono ellipsis">${sec ? `<span class="faint">${I("LOCK_SIMPLE")} ••••••••</span>` : vars(v)}</span></div>`).join("")}</div>`;
  const body = `<div class="mb col" style="gap:12px">
    ${qcard(`<span class="dot" style="background:${ENVS[0].color}"></span><span class="semi">Local</span>${pill("Active", "ok")}<span class="faint small">6 variables</span>`, [["baseUrl", "http://127.0.0.1:18795"], ["pageSize", "20"], ["clientId", "tw-desktop"], ["user", "reader@bookstore.dev"], ["token", "", true], ["publisherId", "pub_7Q2"]], "env")}
    ${qcard(`${I("GLOBE_SIMPLE", "muted")}<span class="semi">Globals</span><span class="faint small">3 variables</span>`, [["timeout", "30000"], ["region", "ap-southeast-1"], ["webhookSecret", "", true]], "env")}
  </div>`;
  return modal(sub("Quick look", "Variables in scope right now"), body, foot(`<span class="row hint">${I("LOCK_SIMPLE")}Secret values stay hidden.</span>`, btn("Close", { set: "dlg=" })), { w: 620, icon: "EYE" });
};

/* ------------------------------------------------------------------ settings */
const SET_PAGES = [
  ["general", "SLIDERS_HORIZONTAL", "General", "Appearance, language, updates and history."],
  ["requests", "PAPER_PLANE_RIGHT", "Requests", "Defaults for every request you send."],
  ["scripts", "CODE", "Scripts", "Limits for scripts and the JavaScript packages they can load."],
  ["proxy", "SHARE", "Proxy", "Send requests through a proxy server."],
  ["certs", "SHIELD_CHECK", "Certificates", "Trust extra certificate authorities and send client certificates."],
  ["shortcuts", "KEYBOARD", "Shortcuts", "Keys for the things you do most."],
  ["about", "INFO", "About", ""],
];
function setPage(p) {
  switch (p) {
    case "general": return group("Appearance", [
        frow("Theme", "Follow the system, or always use light or dark.", seg("st", "system", [["system", "System", "MONITOR"], ["light", "Light", "SUN"], ["dark", "Dark", "MOON"]], { sm: true })),
        frow("Language", "The language of menus and messages.", seg("sl", "en", [["en", "English"], ["id", "Bahasa Indonesia"]], { sm: true })),
        frow("Font size", "Text size of the whole interface.", `<div class="row" style="gap:12px"><span class="mono small" style="width:44px;text-align:right">13 px</span>${slider(30)}</div>`),
        frow("Response panel", "Where the response appears next to the request.", seg("sr", "side", [["below", "Below", "ROWS"], ["side", "Side by side", "COLUMNS"]], { sm: true })),
      ])
      + group("Updates", [
        frow("Check for updates on startup", "TernaryWire asks the release feed for the latest version and shows what is new. Nothing about you or your work is sent.", sw(true)),
        frow("TernaryWire 0.2.3", "The version installed on this computer.", btn("Check now", { size: "sm", icon: "ARROWS_CLOCKWISE" })),
      ])
      + group("History", [frow("History entries", "The oldest entries are removed when History is full.", numUnit("1000", ""))]);
    case "requests": return group("Sending", [
        frow("Request timeout", "How long to wait for a response; 0 waits forever.", numUnit("30000", "ms")),
        frow("HTTP version", "Auto lets client and server agree on one.", seg("hv2", "auto", [["auto", "Auto"], ["h1", "HTTP/1.1"], ["h2", "HTTP/2"]], { sm: true })),
        frow("Encode URL automatically", "Percent-encode characters that are not allowed in a URL.", sw(true)),
      ])
      + group("Redirects", [frow("Follow redirects", "Go on to the address a 3xx response points to.", sw(true)), frow("Maximum redirects", "", numUnit("10", "", 72))])
      + group("Security", [frow("Verify SSL certificates", "Turn this off only for local servers with self-signed certificates.", sw(true))])
      + group("Default headers", [
        frow("Send User-Agent header", '<span class="mono">User-Agent: TernaryWire/0.2.3</span>', sw(true)),
        frow("Send no-cache header", '<span class="mono">Cache-Control: no-cache</span>', sw(true)),
      ])
      + group("Responses", [
        frow("Keep bodies in memory up to", "Larger bodies are kept in a temporary file.", numUnit("64", "MB", 72)),
        frow("Pretty-print bodies up to", "Larger bodies are shown as plain text.", numUnit("10", "MB", 72)),
      ]);
    case "scripts": return group("Limits", [
        frow("Script timeout", "Scripts that run longer are stopped.", numUnit("5000", "ms")),
        frow("Script memory limit", "Scripts run in a separate process that is stopped when it uses more memory than this; 0 means no limit.", numUnit("512", "MB", 72)),
        frow("Script engine", "", `<span class="muted small">Boa · separate worker process</span>`),
      ])
      + group("Package registry", [frow("Registry URL", "esm.sh-compatible; serves <span class=mono>pm.require('npm:…')</span> and <span class=mono>pm.require('jsr:…')</span>.", input("https://esm.sh", { sm: true, mono: true, style: "width:240px" }))])
      + `<div class="col" style="gap:8px"><div class="row"><h4 class="section-title">Package Library</h4><span class="spacer"></span>${btn("Open folder", { size: "sm", kind: "ghost", icon: "FOLDER_OPEN" })}${btn("New package", { size: "sm", icon: "PLUS" })}</div>
        ${hint("Your own modules in <span class=mono>packages/</span>, shared by every script in the workspace.")}
        <div class="dl-vlist">${["my-helpers", "signing", "fixtures"].map((n, i) => `<div class="dl-vrow ${i === 0 ? "hov" : ""}" style="min-height:38px"><span class="dl-tile">${I("PACKAGE")}</span><span class="mono small grow">pm.require('${n}')</span><span class="faint xs">packages/${n}.js</span><span class="dl-ra">${btn("Open", { size: "sm", kind: "ghost" })}</span></div>`).join("")}</div>
        <div class="well mono" style="padding:10px 12px;font-size:11.5px;line-height:1.7"><div class="muted" style="font-family:Inter;font-size:12px;margin-bottom:2px">packages.lock.json: 2 packages · 5 pinned modules</div>npm:lodash-es@4 <span class="faint">→ https://esm.sh/lodash-es@4.17.21</span><br>jsr:@std/encoding <span class="faint">→ https://esm.sh/jsr/@std/encoding@1.0.5</span></div></div>`;
    case "proxy": return group("System proxy", [frow("Use system proxy", "Honours HTTP_PROXY / HTTPS_PROXY / NO_PROXY and the OS proxy settings, including a proxy auto-config (PAC) script.", sw(false))])
      + group("Custom proxy", [
        frow("Use custom proxy", "Send requests through a proxy you set here.", sw(true)),
        frow("Proxy URL", "", input("http://proxy.corp:3128", { sm: true, mono: true, style: "width:260px" })),
        frow("Username", "", input("svc-api", { sm: true, style: "width:260px" })),
        frow("Password", "Stored in the Keychain, not in the settings file.", stored()),
        frow("Bypass", "Hosts reached directly, separated by commas.", input("localhost, *.internal", { sm: true, mono: true, style: "width:260px" })),
        frow("Auto-config (PAC) URL", "A PAC script picks the proxy for each request instead of the proxy URL.", input("", { sm: true, mono: true, ph: "http://wpad.example/proxy.pac", style: "width:260px" })),
      ]);
    case "certs": return group("Certificate authorities", [
        frow("Trust an extra CA certificate (PEM)", "Trusted in addition to the system certificate store.", sw(true)),
        frow("CA file", "", `<div class="row" style="gap:6px;width:300px"><span class="dl-pathbox">~/certs/corp-root-ca.pem</span>${ibtn("FOLDER_OPEN", "Choose file…", { sm: true })}</div>`),
      ])
      + `<div class="col" style="gap:8px"><div class="row"><h4 class="section-title">Client certificates</h4><span class="spacer"></span>${btn("Add certificate", { size: "sm", kind: "ghost", icon: "PLUS" })}</div>
        <div class="card" style="padding:12px 14px"><div class="row" style="gap:10px">${sw(true)}${input("api.bookstore.dev:443", { sm: true, mono: true, ph: "api.example.com:443, *.example.com or *", style: "flex:1" })}${ibtn("TRASH", "Remove certificate", { sm: true })}</div>
          <div class="col" style="gap:8px;margin-top:12px">${[["CRT (PEM)", "~/certs/client.crt"], ["KEY (PEM)", "~/certs/client.key"], ["PFX / P12", ""]].map(([l, v]) => `<div class="row"><span class="muted small" style="width:96px">${l}</span><span class="dl-pathbox">${v || '<span class="faint" style="font-family:Inter">No file</span>'}</span>${ibtn("FOLDER_OPEN", "Choose file…", { sm: true })}</div>`).join("")}
          <div class="row"><span class="muted small" style="width:96px">Passphrase</span>${stored()}</div></div>
          <div class="hint" style="margin-top:10px">A PKCS#12 bundle replaces the PEM files. The passphrase is stored in the Keychain, not in the settings file.</div></div></div>`;
    case "shortcuts": {
      const sc = (title, rows) => group(title, rows.map(([l, k]) => frow(l, "", `<span class="row" style="gap:3px">${k.split(" ").map(kbd).join("")}</span>`)));
      return sc("General", [["New request", "⌘ N"], ["New request (tab)", "⌘ T"], ["Send request · run or stop a flow", "⌘ ↩"], ["Send a message (WebSocket, gRPC, AI…)", "⇧ ⌘ ↩"], ["Save", "⌘ S"], ["Close tab", "⌘ W"]])
        + sc("Navigation", [["Search requests", "⌘ K"], ["Command palette", "⇧ ⌘ P"], ["Switch environment", "⌘ E"], ["Toggle sidebar", "⌘ B"], ["Toggle console", "⌥ ⌘ C"], ["Settings", "⌘ ,"], ["Previous / next tab", "⇧ ⌘ [ ⇧ ⌘ ]"]])
        + sc("Editing", [["Accept a variable suggestion", "Tab"], ["Close a dialog or filter", "esc"]])
        + hint("⌘ is Ctrl on Windows and Linux.");
    }
    case "about": return `<div class="col" style="align-items:center;gap:8px;padding:12px 0 4px;text-align:center">
        <img src="../../crates/ternarywire-app/assets/icon-256.png" alt="" style="width:76px;height:76px;border-radius:18px">
        <div style="font-size:20px;font-weight:600">TernaryWire</div>${pill("Version 0.2.3", "acc")}
        <div class="muted" style="max-width:420px">A lightweight API client written in Rust. No account, no telemetry: data stays in your workspace folder.</div>
        <div class="row" style="margin-top:6px">${btn("Source code", { size: "sm", icon: "GIT_BRANCH" })}${btn("Release notes", { size: "sm", icon: "NOTE" })}${btn("Report an issue", { size: "sm", icon: "BUG" })}</div></div>`
      + group("System", [
        frow("Settings file", '<span class="mono">~/Library/Application Support/TernaryWire/settings.json</span>', btn("Copy path", { size: "sm", kind: "ghost", icon: "COPY" })),
        frow("Secrets", "", '<span class="muted small">macOS Keychain</span>'),
        frow("Script engine", "", '<span class="muted small">Boa · separate worker process</span>'),
        frow("License", "", '<span class="muted small">MIT or Apache-2.0</span>'),
      ]);
  }
  return "";
}
DIALOGS.settings = () => {
  const p = SET_PAGES.find((x) => x[0] === S.page) || SET_PAGES[0];
  const nav = `<nav class="dl-set-nav">${SET_PAGES.map(([id, icon, label]) => `<div class="ni ${id === p[0] ? "on" : ""}" data-set="page=${id}">${I(icon)}${label}</div>`).join("")}
    <span class="spacer"></span><div class="hint" style="padding:8px 10px;font-size:11.5px">Settings are per user and apply to every workspace.</div></nav>`;
  const page = `<div class="dl-set-page">${p[0] === "about" ? "" : `<div class="dl-set-h"><h2>${p[2]}</h2><p>${p[3]}</p></div>`}${setPage(p[0])}</div>`;
  const dirty = p[0] === "proxy";
  return modal(sub("Settings", ""), `<div class="mb dl-flush" style="flex:1">${nav}${page}</div>`,
    foot(dirty ? `<span class="row small" style="gap:6px"><span class="dot" style="background:var(--accent)"></span>Unsaved changes</span>` : `<span class="hint">Changes apply when you save.</span>`, `${btn("Cancel", { set: "dlg=" })}${primary("Save")}`),
    { w: 860, h: 640, icon: "GEAR_SIX", center: true });
};

/* ------------------------------------------------------------------ vault */
const SECRETS = [
  ["bookstore-token", "local"], ["bookstore-client", "local"], ["bookstore-pass", "local"], ["openai", "local"],
  ["db_password", "corp-vault", "cached · 42 s", "path/to/db#password"], ["stripe_key", "aws-prod", "not fetched yet", "prod/stripe#secret"], ["github_pat", "team-1p", "op: you are not signed in", "op://Dev/GitHub/token", true],
];
const PROVIDERS = [["corp-vault", "HashiCorp Vault", "VAULT"], ["aws-prod", "AWS Secrets Manager", "CLOUD_ARROW_DOWN"], ["team-1p", "1Password", "KEY"]];
function vaultSecrets() {
  const link = S.sel === "link";
  const rows = SECRETS.map(([n, prov, st, ref, bad], i) => `<div class="dl-vrow ${i === 4 && !link ? "hov" : ""}">
      <span class="dl-tile">${I("KEY")}</span><span class="mono small">{{vault:${n}}}</span>
      ${prov === "local" ? pill("Local") : pill(`${I("LINK_SIMPLE")}${prov}`, "info", { title: ref })}
      ${st ? `<span class="small ${bad ? "err" : "faint"} ellipsis">${st}</span>` : ""}<span class="spacer"></span>
      <span class="dl-ra">${ibtn("COPY", "Copy the reference", { sm: true })}${prov === "local" ? ibtn("LINK", "Link to an external secret", { sm: true, set: "sel=link" }) + ibtn("TRASH", "Delete", { sm: true }) : ibtn("X", "Unlink", { sm: true })}</span></div>`).join("");
  const add = `<div class="card" style="padding:12px 14px"><div class="section-title" style="margin-bottom:8px">Add a secret</div>
      <div class="row">${input("", { sm: true, mono: true, ph: "Name", style: "width:200px" })}${masked({ style: "flex:1", ph: "Value" })}${btn("Add", { size: "sm", icon: "PLUS" })}</div></div>`;
  const linkForm = `<div class="card" style="padding:14px 16px;border-color:var(--accent)">
      <div class="row" style="margin-bottom:10px">${I("LINK", "accent")}<span class="section-title grow">Link a vault name to an external secret</span>${ibtn("X", "Cancel", { sm: true, set: "sel=" })}</div>
      <div class="grid2">${field("Name", input("db_password", { sm: true, mono: true }))}${field("Provider", selectbox("corp-vault (HashiCorp Vault)", { sm: true, icon: "VAULT" }))}</div>
      <div class="grid2" style="grid-template-columns:1fr 150px;margin-top:10px">${field("Reference", input("apps/bookstore/db#password", { sm: true, mono: true, ph: "path/to/secret#field (KV v2: path?version=3#field)" }))}${field("Cache", numUnit("300", "s", 80), "0 fetches on every use")}</div>
      <div class="well" style="margin-top:10px;padding:4px">
        ${[["ARROW_BEND_UP_LEFT", "Up", ""], ["FOLDER_SIMPLE", "apps/bookstore/", ""], ["FOLDER_SIMPLE", "apps/payments/", ""], ["KEY", "apps/bookstore/db", "use"]].map(([ic, n, u]) => `<div class="dl-lrow">${I(ic, "muted")}<span class="mono small grow">${n}</span>${u ? btn("Use", { size: "sm" }) : ""}</div>`).join("")}</div>
      <div class="row" style="margin-top:12px">${btn("Browse…", { size: "sm", icon: "FOLDER_OPEN" })}${btn("Preview", { size: "sm", icon: "EYE", title: "Fetch once without showing the value" })}<span class="ok small row" style="gap:4px">${I("CHECK_CIRCLE")}Resolved (24 characters)</span><span class="spacer"></span>${btn("Cancel", { size: "sm", set: "sel=" })}${primary("Save link", { size: "sm" })}</div></div>`;
  return `<div class="row">${search("Filter secrets", { sm: true, style: "width:240px" })}<span class="spacer"></span>${btn("Link external secret…", { size: "sm", icon: "LINK", set: "sel=link" })}</div>
    <div class="dl-vlist">${rows}</div>${link ? linkForm : add}`;
}
function vaultProviders() {
  const list = PROVIDERS.map(([n, k, ic], i) => `<div class="dl-vrow ${i === 0 ? "sel" : ""}"><span class="dl-tile acc">${I(ic)}</span><div class="col grow" style="gap:0"><span class="med">${n}</span><span class="faint xs">${k}</span></div><span class="dl-ra" style="visibility:visible">${ibtn("PENCIL_SIMPLE", "Edit", { sm: true })}${ibtn("TRASH", "Remove", { sm: true })}</span></div>`).join("");
  const form = `<div class="card" style="padding:14px 16px;flex:1;min-width:0">
      <div class="row" style="margin-bottom:12px"><span class="dl-tile acc">${I("VAULT")}</span><div class="col" style="gap:0"><span class="semi">corp-vault</span><span class="faint xs">HashiCorp Vault</span></div></div>
      <div class="col" style="gap:10px">
        ${field("Name", input("corp-vault", { sm: true }))}
        <div class="grid2">${field("Address", input("https://vault.corp:8200", { sm: true, mono: true }))}${field("Namespace", input("", { sm: true, ph: "optional (Vault Enterprise)" }))}</div>
        <div class="grid2">${field("Authentication", seg("va", "token", [["token", "Token"], ["approle", "AppRole"]], { sm: true }))}${field("KV version", seg("kv", "auto", [["auto", "Auto"], ["v1", "v1"], ["v2", "v2"]], { sm: true }))}</div>
        ${field("KV mount", input("secret", { sm: true, mono: true, ph: "optional: first path segment" }))}
        ${field("Token *", `<div class="row" style="gap:0;position:relative">${input("", { sm: true, ph: "set — type to replace", style: "width:100%" })}</div>`)}
      </div>
      <div class="row" style="margin-top:14px">${btn("Test connection", { size: "sm", icon: "PLUGS_CONNECTED" })}<span class="ok small row" style="gap:4px">${I("CHECK_CIRCLE")}Connected · KV v2 at secret/</span><span class="spacer"></span>${btn("Cancel", { size: "sm" })}${primary("Save", { size: "sm" })}</div></div>`;
  return `${hint("Providers keep their settings in <span class=mono>.ternarywire/external-secrets.json</span>; tokens and keys go to the secret store.")}
    <div class="row" style="align-items:flex-start;gap:14px"><div class="col" style="width:270px;flex:none;gap:8px"><div class="dl-vlist">${list}</div>${btn("Add provider", { size: "sm", icon: "PLUS", caret: true, menu: "add-provider" })}</div>${form}</div>`;
}
DIALOGS.vault = () => {
  const t = S.sub === "providers" ? "providers" : "secrets";
  const body = `${tabs("sub", t, [["secrets", "Secrets", { count: 7 }], ["providers", "External providers", { count: 3 }]], "", { style: "padding:0 18px" })}
    <div class="mb col" style="gap:12px;padding-top:14px">${t === "providers" ? vaultProviders() : vaultSecrets()}</div>`;
  return modal(sub("Vault", "Secrets are kept in the macOS Keychain, never in workspace files."), body,
    foot(`<span class="row hint" style="gap:6px">Use a secret in any field as <span class="var sec mono" style="font-size:11.5px">{{vault:name}}</span></span>`, btn("Close", { set: "dlg=" })), { w: 820, icon: "LOCK_KEY" });
};
MENUS["add-provider"] = () => placeMenu(menu([{ icon: "VAULT", label: "HashiCorp Vault" }, { icon: "CLOUD_ARROW_DOWN", label: "AWS Secrets Manager" }, { icon: "KEY", label: "Azure Key Vault" }, { icon: "LOCK_KEY", label: "1Password" }]), { w: 220 });

/* ------------------------------------------------------------------ cookies */
DIALOGS.cookies = () => {
  const domains = [["api.staging.bookstore.dev", 3], ["127.0.0.1", 2], ["github.com", 1]];
  const cookies = [["session", "s%3Ab2f9c1e0…9a", "/", "Session"], ["csrftoken", "Xk29fJq0aLm…", "/", "11 Dec 2026"], ["_ga", "GA1.2.1928…", "/", "11 Nov 2027"]];
  const left = `<div class="dl-lpane"><div class="lh"><span class="section-title grow">Domains</span><span class="faint xs">3</span></div>
      <div class="ll">${domains.map(([d, n], i) => `<div class="dl-lrow ${i === 0 ? "on" : ""}">${I("GLOBE_SIMPLE", "muted")}<span class="ellipsis">${d}</span><span class="cnt">${n}</span></div>`).join("")}</div>
      <div style="padding:8px;border-top:1px solid var(--border)">${input("", { sm: true, ph: "Add a domain…", style: "width:100%" })}</div></div>`;
  const right = `<div class="dl-rpane"><div class="row" style="height:48px;padding:0 16px;border-bottom:1px solid var(--border);flex:none"><span class="semi">api.staging.bookstore.dev</span><span class="faint small">3 cookies</span><span class="spacer"></span>${btn("Clear all", { size: "sm", kind: "ghost danger", icon: "TRASH" })}</div>
      <div class="scroll grow" style="padding:14px 16px;display:flex;flex-direction:column;gap:14px">
        ${table([{ label: "Name", w: "22%" }, { label: "Value" }, { label: "Path", w: "12%" }, { label: "Expires", w: "22%" }, { label: "", w: "64px" }],
          cookies.map(([n, v, p, e], i) => [`<span class="mono">${n}</span>`, `<span class="mono muted">${v}</span>`, `<span class="mono">${p}</span>`, e === "Session" ? pill("Session") : `<span class="muted">${e}</span>`, `<span class="row" style="gap:0;${i === 0 ? "" : "visibility:hidden"}">${ibtn("PENCIL_SIMPLE", "Edit cookie", { sm: true })}${ibtn("TRASH", "Delete cookie", { sm: true })}</span>`]), { sel: 0 })}
        <div class="col" style="gap:6px"><span class="section-title">Add or replace a cookie</span>
          <div class="row">${input("", { sm: true, mono: true, ph: "name=value; Path=/; Expires=…", style: "flex:1" })}${btn("Add", { size: "sm", icon: "PLUS" })}</div>
          ${hint("Paste a Set-Cookie value. Hover a row to see its Set-Cookie text.")}</div></div></div>`;
  return modal(sub("Cookies", "6 cookies · 3 domains"), `<div class="mb dl-flush" style="height:440px">${left}${right}</div>`,
    foot(`<span class="row hint">${I("LOCK_SIMPLE")}Stored in this workspace, never committed.</span>`, btn("Close", { set: "dlg=" })), { w: 880, icon: "COOKIE", center: true });
};

/* ------------------------------------------------------------------ import */
DIALOGS.import = () => {
  const ready = S.sub === "ready";
  const body = `<div class="mb col" style="gap:14px">
    ${ready
      ? `<div class="row" style="padding:10px 12px;border:1px solid var(--border);border-radius:9px;background:var(--well)"><span class="dl-tile acc">${I("FILE_TEXT")}</span><div class="col grow" style="gap:0"><span class="med">Bookstore.postman_collection.json</span><span class="faint xs mono">~/Downloads</span></div>${ibtn("X", "Clear", { sm: true, set: "sub=" })}</div>`
      : `<div class="dl-dz">${I("FILE_ARROW_DOWN", "big")}<div class="med">Drop a file here, or click to choose one</div><div class="faint small">Postman · OpenAPI / Swagger · Insomnia · HAR · Hoppscotch · Thunder Client · WSDL · cURL</div></div>`}
    <div class="dl-or-line">or paste the content</div>
    <div class="dl-codebox" style="height:${ready ? 96 : 150}px">${ready ? code(['<span class="faint">Clear the file to paste content instead.</span>'], { gutter: false }) : `<div style="padding:10px 14px" class="mono faint">A Postman or OpenAPI file, a HAR export or a cURL command…</div>`}</div>
    ${ready ? `<div class="card" style="padding:12px 14px;border-color:color-mix(in srgb, var(--ok) 40%, var(--border))">
        <div class="row">${I("CHECK_CIRCLE", "ok")}<span class="semi">Ready to import</span>${pill("Postman v2.1", "ok")}</div>
        <div class="row muted small" style="margin-top:6px;gap:14px"><span>${I("STACK")} 1 collection</span><span>${I("GLOBE")} 14 requests</span><span>${I("GLOBE_SIMPLE")} 2 environments</span><span>${I("HASH")} 3 global variables</span></div>
        <div class="col" style="gap:4px;margin-top:10px">${["Auth type “hawk” on “Legacy upload” is imported without its ext value.", "2 requests use pm.sendRequest with a callback; they run unchanged."].map((w) => `<div class="row small" style="gap:6px;align-items:flex-start">${I("WARNING", "warn")}<span class="muted">${w}</span></div>`).join("")}</div></div>`
      : hint("The format is detected as you paste. A file dropped anywhere on the window is imported too.")}
  </div>`;
  return modal(sub("Import", "Bring in collections, environments and requests from other tools."), body,
    foot(`<span class="hint">Imported items are added to this workspace.</span>`, `${btn("Cancel", { set: "dlg=" })}${primary("Import", { icon: "DOWNLOAD_SIMPLE", disabled: !ready })}`), { w: 700, icon: "DOWNLOAD_SIMPLE" });
};

/* ------------------------------------------------------------------ code snippet */
const LANGS = [["cURL", ""], ["HTTP", ""], ["HTTPie", ""], ["Wget", ""], ["JavaScript", "Fetch"], ["NodeJs", "Axios"], ["Python", "Requests"], ["Go", "Native"], ["Rust", "reqwest"], ["Java", "OkHttp"], ["Java", "HttpClient"], ["Kotlin", "OkHttp"], ["C#", "HttpClient"], ["PHP", "cURL"], ["Swift", "URLSession"], ["Ruby", "Net::HTTP"], ["Dart", "http"], ["PowerShell", "RestMethod"]];
DIALOGS.snippet = () => {
  const curl = [
    '<span class="b">curl</span> <span class="k">--request</span> GET \\',
    "  <span class=\"k\">--url</span> <span class=\"s\">'http://127.0.0.1:18795/books?page=1&amp;limit=20&amp;sort=title'</span> \\",
    "  <span class=\"k\">--header</span> <span class=\"s\">'Accept: application/json'</span> \\",
    "  <span class=\"k\">--header</span> <span class=\"s\">'Authorization: Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9…'</span> \\",
    "  <span class=\"k\">--header</span> <span class=\"s\">'X-Client: ternarywire/0.2.3'</span> \\",
    "  <span class=\"k\">--header</span> <span class=\"s\">'User-Agent: TernaryWire/0.2.3'</span>",
  ];
  const left = `<div class="dl-lpane" style="width:220px"><div style="padding:8px">${search("Filter languages", { sm: true })}</div>
      <div class="ll" style="padding-top:0">${LANGS.map(([n, v], i) => `<div class="dl-lrow ${i === 0 ? "on" : ""}"><span>${n}</span>${v ? `<span class="var2">${v}</span>` : ""}</div>`).join("")}</div></div>`;
  const right = `<div class="dl-rpane"><div class="row" style="height:42px;padding:0 12px 0 16px;border-bottom:1px solid var(--border);flex:none"><span class="semi">cURL</span><span class="faint small">Shell</span><span class="spacer"></span>${ibtn("TEXT_ALIGN_LEFT", "Wrap lines", { sm: true, on: true })}</div>
      <div class="scroll grow">${code(curl, { cls: "wrap", style: "min-height:100%" })}</div></div>`;
  return modal(sub("Code snippet", "List books"), `<div class="mb dl-flush" style="height:460px">${left}${right}</div>`,
    foot(`<span class="row small" style="gap:6px;color:var(--warn)">${I("WARNING")}<span class="muted">Variables are resolved with the active environment; secrets appear in clear text.</span></span>`, `${btn("Close", { set: "dlg=" })}${primary("Copy", { icon: "COPY" })}`), { w: 920, icon: "CODE", center: true });
};

/* ------------------------------------------------------------------ save request */
DIALOGS.save = () => {
  const tp = (depth, icon, name, on) => `<div class="tp ${on ? "on" : ""}" style="padding-left:${10 + depth * 18}px">${I(icon)}<span class="grow">${name}</span>${on ? I("CHECK") : ""}</div>`;
  const body = `<div class="mb col" style="gap:14px">
    ${field("Name", input("Untitled request", { focus: true }))}
    <div class="field"><label>Save to</label>
      <div class="well dl-tree-pick scroll" style="padding:4px;max-height:220px">
        ${tp(0, "STACK", "Bookstore API")}${tp(1, "FOLDER_SIMPLE", "Auth")}${tp(1, "FOLDER_OPEN", "Books", true)}${tp(1, "FOLDER_SIMPLE", "Orders")}
        ${tp(0, "STACK", "Realtime & AI")}${tp(0, "STACK", "Payments (fork)")}</div></div>
    <div class="row">${input("", { sm: true, ph: "New collection name", style: "flex:1" })}${btn("Create collection", { size: "sm", icon: "PLUS" })}</div>
  </div>`;
  return modal(sub("Save request", ""), body, foot(`<span class="row hint" style="gap:4px">${I("FOLDER_OPEN")}Bookstore API › Books</span>`, `${btn("Cancel", { set: "dlg=" })}${primary("Save", { icon: "FLOPPY_DISK" })}`), { w: 520, icon: "FLOPPY_DISK" });
};

/* ------------------------------------------------------------------ generate tests */
DIALOGS["gen-tests"] = () => {
  const src = [
    'pm.test("Status code is 200", () => {', "  pm.response.to.have.status(200);", "});", "",
    'pm.test("Response time is below 500 ms", () => {', "  pm.expect(pm.response.responseTime).to.be.below(500);", "});", "",
    'pm.test("Content-Type is application/json", () => {', '  pm.expect(pm.response.headers.get("Content-Type")).to.include("application/json");', "});", "",
    'pm.test("Body matches the JSON schema", () => {', '  const schema = { type: "object", required: ["data", "page", "limit", "total"] };', "  pm.response.to.have.jsonSchema(schema);", "});",
  ].join("\n");
  const body = `<div class="mb col" style="gap:10px">${hint("Review or edit the tests, then add them to the Post-response script.")}
    <div class="dl-codebox scroll" style="max-height:380px">${code(jsHl(src), { cls: "wrap" })}</div></div>`;
  return modal(sub("Generate tests", "From the 200 OK response · 4 tests"), body,
    foot(`<span class="hint">The tests run on this response right after they are added.</span>`, `${btn("Cancel", { set: "dlg=" })}${primary("Add tests", { icon: "PLUS" })}`), { w: 720, icon: "TEST_TUBE" });
};

/* ------------------------------------------------------------------ set as variable */
DIALOGS["set-var"] = () => {
  const body = `<div class="mb col" style="gap:14px">
    ${field("Name", input("firstTitle", { mono: true, focus: true }))}
    ${field("Scope", seg("vs", "env", [["env", "Local", "GLOBE"], ["coll", "Collection", "STACK"], ["glob", "Globals", "GLOBE_SIMPLE"]], { sm: true }))}
    <div class="field"><label>Value</label><div class="row" style="padding:8px 10px;border-radius:7px;background:var(--well);border:1px solid var(--border)"><span class="mono s grow ellipsis">"The Pragmatic Programmer"</span>${pill("string")}</div></div>
    <div class="group">${frow("Secret", "Masked in the app, kept in the secret store", sw(false))}${frow("Also set it on every run", "Adds this line to the Post-response script:", sw(true))}</div>
    <div class="dl-codebox" style="background:var(--ok-soft);border-color:color-mix(in srgb, var(--ok) 30%, var(--border))">${code(jsHl('pm.environment.set("firstTitle", pm.response.json().data[0].title);'), { gutter: false, cls: "wrap" })}</div>
  </div>`;
  return modal(sub("Set as variable", '<span class="mono">$.data[0].title</span>'), body,
    foot(`<span class="hint">Saved as the current value, which stays on this computer.</span>`, `${btn("Cancel", { set: "dlg=" })}${primary("Set variable")}`), { w: 540, icon: "BRACKETS_CURLY" });
};

/* ------------------------------------------------------------------ confirm / unsaved / name prompt */
DIALOGS.confirm = () => modal(null, `<div class="mb row" style="gap:14px;align-items:flex-start;padding:22px 20px 18px">
    <div class="dl-danger-circle">${I("TRASH")}</div>
    <div class="col" style="gap:4px"><div class="semi" style="font-size:15px">Delete request “List books”?</div><div class="muted">The request is removed from the workspace folder.</div></div></div>`,
  foot("", `${btn("Cancel", { set: "dlg=" })}${btn("Delete", { kind: "danger solid", icon: "TRASH" })}`), { w: 460, center: true });

DIALOGS.unsaved = () => modal(null, `<div class="mb row" style="gap:14px;align-items:flex-start;padding:22px 20px 18px">
    <div class="dl-warn-circle">${I("WARNING")}</div>
    <div class="col" style="gap:4px"><div class="semi" style="font-size:15px">Save changes to “Create book”?</div><div class="muted">Your changes will be lost if you close the tab without saving them.</div></div></div>`,
  foot(btn("Don't save", { kind: "ghost danger" }), `${btn("Cancel", { set: "dlg=", kbd: "esc" })}${primary("Save…", { kbd: "↩" })}`), { w: 460, center: true });

DIALOGS["name-prompt"] = () => modal(sub("New Collection", ""), `<div class="mb col" style="gap:8px">${field("Name", input("New Collection", { focus: true }))}</div>`,
  foot("", `${btn("Cancel", { set: "dlg=" })}${primary("Create")}`), { w: 440, icon: "STACK", center: true });

/* ------------------------------------------------------------------ new workspace */
DIALOGS["new-ws"] = () => {
  const body = `<div class="mb col" style="gap:14px">
    ${field("Name", input("Payments platform", { focus: true }))}
    <div class="field"><label>Location</label><div class="row"><span class="dl-pathbox">~/work/payments-platform</span>${btn("Choose folder…", { size: "sm", icon: "FOLDER_OPEN" })}</div></div>
    ${hint("A workspace is a folder of plain JSON files; put it in a Git repository to share it.")}
  </div>`;
  return modal(sub("New workspace", "A folder of plain files you can keep in Git."), body, foot("", `${btn("Cancel", { set: "dlg=" })}${primary("Create")}`), { w: 520, icon: "FOLDER_PLUS" });
};

/* ------------------------------------------------------------------ update */
DIALOGS.update = () => {
  const prog = S.sub === "progress";
  const body = `<div class="mb col" style="gap:14px;padding-top:20px">
    <div class="row" style="gap:14px"><img src="../../crates/ternarywire-app/assets/icon-256.png" alt="" style="width:52px;height:52px;border-radius:12px">
      <div class="col" style="gap:2px"><div class="semi" style="font-size:15px">A new version of TernaryWire is available</div><div class="muted small">TernaryWire 0.3.0 is ready — you have 0.2.3.  ·  2026-11-10</div></div></div>
    <div class="row"><span class="section-title grow">What's new</span>${btn("Release notes", { size: "sm", kind: "ghost", icon: "ARROW_SQUARE_OUT" })}</div>
    <div class="card dl-relnotes"><h5>Workbench</h5><ul><li>Request on the left, response on the right, with a status row that always shows code, time, size and tests.</li><li>Local servers (mock, webhooks, capture) get their own sidebar section.</li></ul>
      <h5>Fixes</h5><ul><li>Large responses no longer freeze the tree view.</li><li>OAuth 2.0 tokens refresh before a collection run starts.</li><li>gRPC reflection works with servers that only offer v1alpha.</li></ul></div>
    ${prog ? `<div class="col" style="gap:8px"><div class="dl-steps"><span class="st done">${I("CHECK_CIRCLE")}Download</span>${I("CARET_RIGHT", "faint")}<span class="st now">${I("CIRCLE_NOTCH", "spin")}Verify</span>${I("CARET_RIGHT", "faint")}<span class="st">${I("CIRCLE")}Install</span></div>
        <div class="bar" style="height:8px;border-radius:4px"><span style="width:82%"></span></div><div class="hint">Checking the checksum and the signature…</div></div>`
      : hint("TernaryWire quits, installs the new version and opens again with your tabs.")}
  </div>`;
  return modal(null, body, foot(btn("Skip this version", { kind: "ghost", disabled: prog }), `${btn("Remind me later", { set: "dlg=" })}${prog ? btn("Installing…", { kind: "primary", disabled: true }) : primary("Install and restart", { icon: "ARROW_LINE_DOWN" })}`), { w: 560, center: true });
};

/* ------------------------------------------------------------------ fork pull / merge */
DIALOGS["fork-merge"] = () => {
  const items = [
    ["changed", "info", "FILE_TEXT", "Create charge", true, true],
    ["added", "ok", "FILE_TEXT", "Refund charge", true],
    ["conflict", "warn", "FILE_TEXT", "List customers", true],
    ["changed", "info", "SLIDERS", "Collection settings", true],
    ["removed", "err", "FILE_TEXT", "Legacy webhook", false],
  ];
  const left = `<div class="dl-lpane" style="width:380px"><div class="lh"><span class="section-title grow">Changes</span><span class="faint xs">4 of 5 selected</span></div><div class="ll">
    ${items.map(([k, c, ic, n, on, sel]) => `<div class="dl-lrow ${sel ? "on" : ""}" style="height:auto;min-height:36px;padding:6px 10px;flex-wrap:wrap;gap:8px">${chk(on)}${pill(k, c)}${I(ic, "muted")}<span class="${on ? "" : "faint"} grow">${n}</span>
      ${k === "conflict" ? `<div style="width:100%;padding-left:23px">${seg("side", "src", [["src", "Use Payments"], ["dst", "Keep Payments (fork)"]], { sm: true })}</div>` : ""}</div>`).join("")}</div></div>`;
  const dl = [["", 1, 1, "{"], ["", 2, 2, '  "method": "POST",'], ["del", 3, "", '  "url": "{{baseUrl}}/v1/charges",'], ["add", "", 3, '  "url": "{{baseUrl}}/v2/charges",'], ["", 4, 4, '  "headers": ['], ["add", "", 5, '    { "key": "Idempotency-Key", "value": "{{$guid}}" },'], ["", 5, 6, '    { "key": "Content-Type", "value": "application/json" }'], ["", 6, 7, "  ],"], ["del", 7, "", '  "body": { "amount": 1000 }'], ["add", "", 8, '  "body": { "amount": 1000, "currency": "idr" }'], ["", 8, 9, "}"]];
  const right = `<div class="dl-rpane"><div class="row" style="height:48px;padding:0 16px;border-bottom:1px solid var(--border);flex:none"><span class="semi">Create charge</span>${pill("changed", "info")}<span class="spacer"></span><span class="ok small">+ Payments</span><span class="err small">− Payments (fork)</span></div>
      <div class="scroll grow dl-diffv" style="padding:8px 0">${dl.map(([k, a, b, t]) => `<div class="dr ${k}"><span class="ln">${a}</span><span class="ln">${b}</span><span class="sg ${k === "add" ? "ok" : k === "del" ? "err" : "faint"}">${k === "add" ? "+" : k === "del" ? "−" : ""}</span><span>${esc(t)}</span></div>`).join("")}</div></div>`;
  return modal(sub("Pull changes from “Payments”", "5 changes from “Payments” to “Payments (fork)”, 1 conflict: choose a side to include it"), `<div class="mb dl-flush" style="height:440px">${left}${right}</div>`,
    foot(`<span class="small muted">4 of 5 selected</span>${btn("Select all", { size: "sm", kind: "ghost" })}${btn("Select none", { size: "sm", kind: "ghost" })}`, `${btn("Cancel", { set: "dlg=" })}${primary("Apply (4)", { icon: "GIT_MERGE" })}`), { w: 1000, icon: "GIT_FORK", center: true });
};

/* ------------------------------------------------------------------ screens */
const dlgScreen = (id, dlg, label, notes, preset = {}) => screen({ id, group: "Dialogs & panels", label, rail: "collections", sel: "list", tab: OPEN_TABS[0], view: () => httpView("list"), dlg, preset, notes });

dlgScreen("dlg-palette", "palette", "Command palette (⇧⌘P)", N([
  "Tanpa header: baris pencarian besar 54 px di atas, hasil berkelompok (Create, Actions, Requests, Environments, Run collection, Preferences), footer berisi petunjuk tombol dan jumlah hasil.",
  "Baris terpilih memakai fill aksen lembut + label ‘↩ Open’ di kanan, menggantikan chip ‘↩’ kecil. Shortcut tiap aksi tampil sebagai kbd di kanan.",
  "Kotak ‘Search requests, run a command…’ di title bar membuka palette ini; ⌘K langsung ke mode requests.",
]) + NE(["<code>dialogs.rs</code> palette (:1998): ganti row 30 px jadi 36 px dengan kolom lead 38 px (method badge / ikon). Header grup hanya bila &gt;1 grup terlihat (tetap seperti sekarang)."]));
dlgScreen("dlg-palette-search", "palette", "Search requests (⌘K) · hasil fuzzy", N([
  "Mode ⌘K: hanya requests, tanpa header grup. Huruf yang cocok ditebalkan dengan warna aksen; path ‘Collection › Folder’ muted di samping nama.",
]) + NE(["Sorot huruf cocok dengan <code>LayoutJob</code> dua format (text / accent_text semibold)."]), { sub: "q" });
dlgScreen("dlg-env-switch", "env-switch", "Environment switcher (⌘E)", N([
  "Satu anatomi dengan palette: pencarian di atas, daftar, footer petunjuk tombol. Environment aktif memakai ✓ hijau + pill ‘Active’; yang lain titik warna environment.",
  "‘Edit variables’ di footer membuka tab environment aktif (atau Globals).",
]) + NE(["<code>dialogs.rs</code> :2095 — reuse komponen row palette; tambahkan warna environment (field baru, opsional) untuk titik."]));
dlgScreen("dlg-quicklook", "quicklook", "Quick look variabel", N([
  "Dua kartu (environment aktif, Globals) dengan baris key/value bergrid; secret tampil sebagai gembok + ••••. Tombol Edit per kartu.",
  "Footer: ‘Secret values stay hidden.’ di kiri, Close di kanan.",
]) + NE(["<code>dialogs.rs</code> :2225 — kartu memakai <code>kit::card</code>, baris 30 px dengan hairline seperti <code>read_table</code>."]));

dlgScreen("dlg-settings", "settings", "Settings · General", N([
  "Dialog 860×640 tetap: nav kiri (ikon + label) di latar ‘well’, halaman di kanan dengan judul + deskripsi lalu grup berbingkai berisi baris label/help/kontrol.",
  "Tinggi tetap supaya pindah halaman tidak membuat dialog melompat. Footer: status ‘Unsaved changes’ (titik aksen) atau ‘Changes apply when you save.’ + Cancel/Save.",
  "Theme memakai segmented berikon (System/Light/Dark); font size memakai slider dengan nilai di kiri.",
]) + NE(["<code>dialogs.rs</code> settings (:2290): nav jadi <code>list_row</code> 32 px dengan ikon; halaman memakai <code>dialogs::group</code> + <code>Rows::row</code> yang sudah ada."]), { page: "general" });
dlgScreen("dlg-settings-requests", "settings", "Settings · Requests", N(["Lima grup: Sending, Redirects, Security, Default headers, Responses. Help untuk header default menampilkan header aslinya dalam mono."]), { page: "requests" });
dlgScreen("dlg-settings-scripts", "settings", "Settings · Scripts", N(["Limits, Package registry, lalu Package Library sebagai daftar <code>pm.require('…')</code> dengan tombol Open saat hover, plus ringkasan <code>packages.lock.json</code> di kotak mono."]), { page: "scripts" });
dlgScreen("dlg-settings-proxy", "settings", "Settings · Proxy", N(["Password yang sudah tersimpan tampil sebagai pill ‘Stored in the Keychain’ + Change, bukan field kosong. Baris custom proxy diredupkan saat toggle mati.", "Contoh state ‘Unsaved changes’ di footer."]), { page: "proxy" });
dlgScreen("dlg-settings-certs", "settings", "Settings · Certificates", N(["Setiap client certificate adalah kartu: toggle + host di baris pertama, tiga path (CRT, KEY, PFX/P12) dengan tombol folder, passphrase tersimpan di Keychain."]), { page: "certs" });
dlgScreen("dlg-settings-shortcuts", "settings", "Settings · Shortcuts", N(["Daftar shortcut dikelompokkan (General, Navigation, Editing) dalam grup yang sama dengan halaman lain; tetap tampilan saja (belum bisa diubah)."]), { page: "shortcuts" });
dlgScreen("dlg-settings-about", "settings", "Settings · About", N(["Identitas app terpusat (ikon 76 px, nama, versi), tiga tautan, lalu grup System: lokasi settings file, backend secret, engine script, lisensi."]), { page: "about" });

dlgScreen("dlg-vault", "vault", "Vault · Secrets", N([
  "Satu daftar untuk secret lokal dan yang di-link: referensi <code>{{vault:name}}</code> dalam mono, pill Local / 🔗 provider, status cache atau error (merah).",
  "Aksi baris (copy reference, link/unlink, delete) muncul saat hover agar daftar tenang. ‘Add a secret’ jadi kartu di bawah daftar.",
  "Footer selalu mengingatkan cara memakai secret: chip <code>{{vault:name}}</code>.",
]) + NE(["<code>vault_ui.rs</code> (:114, :180): baris 44 px dengan <code>icon_tile</code>; tombol aksi digambar hanya bila <code>response.hovered()</code>."]));
dlgScreen("dlg-vault-link", "vault", "Vault · Link external secret", N(["Form link muncul sebagai kartu ber-border aksen menggantikan ‘Add a secret’: Name, Provider, Reference (placeholder per provider), Cache, browser folder, Preview → ‘Resolved (n characters)’, Save link."]), { sel: "link" });
dlgScreen("dlg-vault-providers", "vault", "Vault · External providers", N(["Master–detail: daftar provider di kiri (tile ikon jenis, nama, jenis), form di kanan. ‘Add provider ▾’ menawarkan HashiCorp Vault, AWS Secrets Manager, Azure Key Vault, 1Password.", "Hasil ‘Test connection’ tampil inline di sebelah tombol, bukan banner terpisah."]) + NE(["<code>vault_ui.rs</code> :509 / :609 — layout dua kolom di dalam modal 820 px."]), { sub: "providers" });

dlgScreen("dlg-cookies", "cookies", "Cookie manager", N([
  "Master–detail: domain di kiri (dengan jumlah, input ‘Add a domain…’ di bawah), tabel cookie di kanan (Name · Value · Path · Expires). ‘Session’ jadi pill.",
  "Edit/Delete per baris saat hover; ‘Add or replace a cookie’ di bawah tabel menerima teks Set-Cookie.",
]) + NE(["<code>dialogs.rs</code> :2900 — tabel memakai <code>read_table</code> dengan kolom aksi."]));
dlgScreen("dlg-import", "import", "Import · kosong", N([
  "Drop zone besar di atas, garis ‘or paste the content’, editor tempel. Tombol Import nonaktif sampai format terdeteksi.",
]) + NE(["<code>dialogs.rs</code> :3190 — tetap tanpa opsi; deteksi format tetap live."]));
dlgScreen("dlg-import-ready", "import", "Import · siap diimpor", N([
  "File terpilih tampil sebagai chip (ikon, nama, folder, ✕). Kartu hijau ‘Ready to import’ + pill format, hitungan dengan ikon, lalu peringatan (maks. 6).",
]), { sub: "ready" });
dlgScreen("dlg-snippet", "snippet", "Code snippet", N([
  "Daftar 18 bahasa di kiri dengan filter dan varian (Fetch, Requests, …) di kanan baris; kode di kanan dengan header bahasa dan toggle wrap.",
  "Peringatan ‘secrets appear in clear text’ tetap di footer kiri, Copy sebagai tombol primer.",
]) + NE(["<code>dialogs.rs</code> :3461 — tambah <code>search_field</code> di atas daftar (filter lokal, tanpa perubahan data)."]));
dlgScreen("dlg-save", "save", "Save request", N(["Pohon collection/folder dalam ‘well’ dengan ✓ pada target; footer kiri menampilkan path tujuan ‘Collection › Folder’. Buat collection baru inline."]) + NE(["<code>dialogs.rs</code> :3584."]));
dlgScreen("dlg-gen-tests", "gen-tests", "Generate tests", N(["Editor JS yang bisa diedit berisi test hasil generate (status, waktu, Content-Type, JSON schema); ‘Add tests’ menambahkannya ke Post-response."]) + NE(["<code>response_actions.rs</code> :221, <code>test_gen.rs</code> :338."]));
dlgScreen("dlg-set-var", "set-var", "Set as variable (dari body response)", N([
  "Dibuka dari klik kanan nilai di body. Path JSON jadi subjudul; scope segmented berikon (environment aktif, Collection, Globals); preview nilai dengan pill tipe.",
  "Dua switch dikelompokkan; saat ‘Also set it on every run’ aktif, baris kode yang akan ditambahkan tampil di kotak hijau.",
]) + NE(["<code>response_actions.rs</code> :280."]));
dlgScreen("dlg-fork-merge", "fork-merge", "Fork · pull / merge changes", N([
  "Master–detail 1000 px: daftar perubahan (checkbox, pill jenis, ikon, nama) di kiri; konflik mendapat segmented ‘Use … / Keep …’ langsung di barisnya. Diff baris di kanan.",
  "Footer kiri: hitungan terpilih + Select all/none; kanan: Cancel + ‘Apply (n)’.",
]) + NE(["<code>fork_ui.rs</code> — tetap memakai data yang sama; hanya tata letak dan pill warna jenis."]));
dlgScreen("dlg-confirm", "confirm", "Konfirmasi hapus", N(["Tanpa header: lingkaran merah berikon tempat sampah, judul berisi nama item, kalimat akibatnya. Tombol Delete merah solid di paling kanan."]) + NE(["<code>dialogs.rs</code> :3832; varian collection/folder/request/environment hanya mengganti teks."]));
dlgScreen("dlg-unsaved", "unsaved", "Tutup tab belum disimpan", N(["‘Don't save’ dipisah ke kiri (ghost merah) agar tidak tertekan tak sengaja; Cancel (esc) dan Save… (↩, default) di kanan dengan petunjuk tombol."]) + NE(["<code>dialogs.rs</code> :3773."]));
dlgScreen("dlg-name-prompt", "name-prompt", "Name prompt (New Collection)", N(["Satu field nama yang langsung fokus dan terseleksi. Varian: New Environment, Rename environment (‘Rename’), Fork collection (‘Fork’ + hint pull/merge)."]) + NE(["<code>dialogs.rs</code> :3978."]));
dlgScreen("dlg-new-ws", "new-ws", "New workspace", N(["Name + Location (path read-only + ‘Choose folder…’). Bila folder sudah berisi workspace, hint diganti banner info dan tombol jadi ‘Open’."]) + NE(["<code>dialogs.rs</code> :3893."]));
dlgScreen("dlg-update", "update", "Update tersedia", N([
  "Tanpa header: ikon app 52 px + judul + versi/tanggal; catatan rilis dalam kartu yang bisa di-scroll; ‘Skip this version’ di kiri, ‘Remind me later’ + ‘Install and restart’ di kanan.",
  "Status bar menampilkan pill ‘Update 0.3.0’ selama pop-up ditutup.",
]) + NE(["<code>update.rs</code> :1091 (<code>update::dialog</code>), pill di :991."]));
dlgScreen("dlg-update-progress", "update", "Update · sedang memasang", N(["Langkah Download › Verify › Install dengan ✓ / spinner / ○, progress bar 8 px, teks tahap. Tombol utama jadi ‘Installing…’ nonaktif."]), { sub: "progress" });

screen({ id: "console", group: "Dialogs & panels", label: "Console panel", rail: "collections", sel: "list", tab: OPEN_TABS[0], view: () => httpView("list"), preset: { console: true },
  notes: N([
    "Console pindah ke dalam kartu kerja sebagai laci bawah (bisa di-resize), bukan panel selebar jendela — sidebar tetap utuh.",
    "Header: judul, segmented All / Logs / Network / Errors (jumlah), filter, Copy lines, Clear, Close. Baris: waktu mono, ikon jenis (network berwarna status), pesan, sumber (request · fase) di kanan.",
    "Baris error dan warning diberi tint lembut. Rail dan status bar menampilkan titik/angka merah bila ada error.",
  ]) + NE(["<code>dialogs::console_ui</code> (:616): pindahkan pemanggilan dari <code>Panel::bottom(\"console\")</code> ke dalam frame kartu kerja di <code>app.rs ui()</code>, di bawah <code>central()</code>."]) });
})();
