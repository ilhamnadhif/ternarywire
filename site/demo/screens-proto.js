// Realtime & RPC screens: WebSocket, Socket.IO, MQTT, gRPC, MCP and AI.
// Same grammar as the HTTP screen: compose on the left, traffic/result on the right.

document.head.insertAdjacentHTML("beforeend", `<style id="css-proto">
.pbar { display: flex; align-items: stretch; height: 38px; border: 1px solid var(--border-strong); border-radius: 9px; background: var(--surface); min-width: 0; flex: 1; }
.pbar .pbadge { display: flex; align-items: center; padding: 0 12px; border-right: 1px solid var(--border); flex: none; }
.pbar .pbadge .meth { font-size: 11px; }
.pbar .pfld { flex: 1 1 auto; display: flex; align-items: center; padding: 0 12px; font-family: "JBMono", monospace; font-size: 12.5px; white-space: nowrap; overflow: hidden; min-width: 0; }
.pbar .pfld.ph { color: var(--text-faint); }
.pbar .pfld + .pfld, .pbar .pcombo + .pfld, .pbar .pfld + .pcombo { border-left: 1px solid var(--border); }
.pbar .pcombo { display: flex; align-items: center; gap: 7px; padding: 0 10px 0 12px; font-size: 12.5px; flex: none; cursor: pointer; white-space: nowrap; min-width: 0; }
.pbar .pcombo:hover { background: var(--hover); }
.pbar .pcombo .car { color: var(--text-faint); font-size: 11px; margin-left: auto; }
.pbar .pcombo + .pcombo { border-left: 1px solid var(--border); }
.pbig { height: 38px !important; padding: 0 16px 0 14px !important; border-radius: 9px !important; font-size: 13px !important; }
.rbar { height: 36px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 10px 0 12px; border-bottom: 1px solid var(--border); }
.pdir { width: 20px; height: 20px; border-radius: 6px; display: inline-grid; place-items: center; font-size: 12px; flex: none; }
.pdir.in { background: var(--ok-soft); color: var(--ok); } .pdir.out { background: var(--info-soft); color: var(--info); }
.pdir.sys { background: var(--subtle); color: var(--text-muted); } .pdir.er { background: var(--err-soft); color: var(--err); }
.plog .lr { cursor: pointer; grid-template-columns: 22px 92px minmax(0, auto) 1fr auto; }
.plog .lr .sz { white-space: nowrap; min-width: 36px; }
.log .lr.lr-err { background: color-mix(in srgb, var(--err) 7%, transparent); }
.log .lr .body.sysl { font-family: "Inter"; color: var(--text-muted); font-size: 12px; }
.notice { border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent); background: var(--accent-soft); border-radius: 10px; padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
.notice .nt { display: flex; align-items: center; gap: 8px; font-weight: 600; }
.notice .nt .i { color: var(--accent-text); font-size: 16px; }
.argc { border: 1px solid var(--border); border-radius: 9px; background: var(--surface); overflow: hidden; }
.argc .ah { display: flex; align-items: center; gap: 8px; height: 36px; padding: 0 8px 0 10px; border-bottom: 1px solid var(--border); background: var(--well); }
.argc .num { width: 20px; height: 20px; border-radius: 50%; display: inline-grid; place-items: center; font-size: 11px; font-weight: 600; background: var(--subtle); color: var(--text-muted); }
.lrow { display: flex; align-items: center; gap: 8px; min-height: 40px; padding: 0 8px 0 12px; border-top: 1px solid var(--border); }
.lrow:first-child { border-top: 0; }
.lrow:hover { background: var(--well); }
.lrow .ra { display: flex; gap: 0; opacity: .55; } .lrow:hover .ra { opacity: 1; }
.tcard { padding: 8px 10px; border-radius: 8px; border: 1px solid transparent; cursor: pointer; display: flex; flex-direction: column; gap: 3px; }
.tcard:hover { background: var(--hover); }
.tcard.sel { background: var(--surface); border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 12%, transparent); }
.tcard .td { font-size: 11.5px; color: var(--text-muted); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; line-height: 1.4; }
.bubrow { display: flex; gap: 8px; align-items: flex-start; }
.bubrow.me { flex-direction: row-reverse; }
.av { width: 26px; height: 26px; border-radius: 8px; display: grid; place-items: center; font-size: 14px; flex: none; background: var(--subtle); color: var(--text-muted); }
.av.ai { background: color-mix(in srgb, var(--m-ai) 16%, transparent); color: var(--m-ai); }
.av.me { background: var(--accent-soft); color: var(--accent-text); }
.bub { max-width: 86%; border-radius: 12px; padding: 8px 12px 10px; border: 1px solid var(--border); background: var(--surface); line-height: 1.55; position: relative; }
.bub .bl { font-size: 10.5px; font-weight: 600; letter-spacing: .06em; text-transform: uppercase; color: var(--text-faint); margin-bottom: 2px; display: flex; align-items: center; gap: 6px; }
.bubrow.me .bub { background: var(--accent-soft); border-color: transparent; }
.bub .bact { position: absolute; top: 4px; right: 4px; display: flex; gap: 0; }
.bub.tool { background: var(--well); font-size: 12px; }
.bub.tool .bl { text-transform: none; letter-spacing: 0; font-size: 11.5px; color: var(--text-muted); }
.bub p { margin: 0 0 8px; } .bub p:last-child { margin: 0; }
.bub ol { margin: 0 0 8px; padding-left: 20px; } .bub li { margin: 3px 0; }
.typing span { display: inline-block; width: 5px; height: 5px; border-radius: 50%; background: var(--text-faint); margin-right: 3px; animation: pulse 1.2s infinite; }
.pslider { width: 150px; height: 4px; border-radius: 2px; background: var(--subtle); position: relative; }
.pslider > span { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 2px; background: var(--accent); }
.pslider > i { position: absolute; top: 50%; width: 14px; height: 14px; margin: -7px 0 0 -7px; border-radius: 50%; background: #fff; box-shadow: 0 1px 3px rgba(0,0,0,.3), 0 0 0 1px var(--border-strong); }
.mpop { position: absolute; z-index: 30; width: 380px; background: var(--surface); border: 1px solid var(--border); border-radius: 12px; box-shadow: var(--shadow-pop); padding: 6px; }
.mpop .mrow { display: flex; align-items: center; gap: 10px; height: 42px; padding: 0 10px; border-radius: 7px; }
.mpop .mrow:hover, .mpop .mrow.on { background: var(--hover); }
.mpop .mrow.on { box-shadow: inset 2px 0 0 var(--accent); }
.subchip { display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 4px 0 9px; border-radius: 12px; border: 1px solid var(--border); background: var(--surface); font-size: 11.5px; white-space: nowrap; }
.subchip .x { width: 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; color: var(--text-faint); font-size: 10px; }
.subchip .x:hover { background: var(--hover); color: var(--text); }
</style>`);

/* ------------------------------------------------------------------ building blocks */
const withVars = (lines) => lines.map((l) => {
  const fix = (s) => s.replace(/\{\{([^}]+)\}\}/g, '<span class="var">{{$1}}</span>');
  return typeof l === "string" ? fix(l) : { ...l, t: fix(l.t) };
});
const pfld = (html, o = {}) => `<div class="pfld ${o.ph ? "ph" : ""}" style="${o.style || ""}" title="${o.title || ""}">${html}</div>`;
const pcombo = (html, o = {}) => `<div class="pcombo" style="${o.style || ""}" title="${o.title || ""}" ${o.menu ? `data-menu="${o.menu}"` : ""}>${html}${I("CARET_DOWN", "car")}</div>`;
const pbar = (proto, parts) => `<div class="pbar"><div class="pbadge">${meth(proto)}</div>${parts}</div>`;
const bigBtn = (label, icon, kind, title) => `<button class="btn ${kind === undefined ? "primary" : kind} pbig" title="${title || ""}">${I(icon)}${label}</button>`;
const row56 = (inner, o = {}) => `<div class="row" style="height:56px;padding:0 12px;flex:none;gap:8px;border-bottom:1px solid var(--border);${o.rel ? "position:relative;z-index:20" : ""}">${inner}</div>`;
const rrow = (left, right) => `<div class="row" style="height:56px;padding:0 10px 0 14px;flex:none;gap:8px;border-bottom:1px solid var(--border)">${left}<span class="spacer"></span>${right}</div>`;
const rbar = (left, right = "") => `<div class="rbar">${left}<span class="spacer"></span>${right}</div>`;
const paneActs = () => `<div class="vdiv"></div>${ibtn("ARROWS_OUT", "Maximize", { sm: true })}${ibtn(sideBySide() ? "CARET_RIGHT" : "CARET_DOWN", "Minimize", { sm: true })}`;
const mt = (icon, text, title) => `<span class="mt" ${title ? `title="${title}"` : ""}>${icon ? I(icon) : ""}${text}</span>`;
const editorWell = (lines, o = {}) => `<div class="well" style="margin:${o.margin || "0 12px 12px"};display:flex;overflow:hidden;min-height:0;${o.h ? `height:${o.h}px;flex:none` : "flex:1"}">${code(lines, { style: "flex:1;background:transparent", cur: o.cur })}</div>`;

function protoTitle(name, pills = "") {
  return `<div class="row" style="height:46px;padding:0 12px 0 16px;border-bottom:1px solid var(--border);flex:none">
    <div class="crumbs grow"><span>Realtime &amp; AI</span><span class="sep">${I("CARET_RIGHT")}</span><span class="cur" title="Double-click to rename">${name}</span><span class="row" style="gap:6px;margin-left:8px">${pills}</span></div>
    ${btn("Save", { icon: "FLOPPY_DISK", size: "sm", title: "Save (⌘S)" })}${ibtn("DOTS_THREE", "More actions", { menu: "req-more" })}
  </div>`;
}

// o: { title, bar, ltabs, left, status, ractions, rtabs, right }
function protoView(o) {
  const side = sideBySide();
  const L = `<section class="pane" style="flex:1 1 0;min-height:0">${row56(o.bar, { rel: !!o.barOverlay })}${o.ltabs}<div class="col grow" style="gap:0;min-height:0">${o.left}</div></section>`;
  const R = `<section class="pane" style="flex:1 1 0;min-height:0;${side ? "" : "border-top:1px solid var(--border)"}">${rrow(o.status, o.ractions ?? paneActs())}${o.rtabs}<div class="col grow" style="gap:0;min-height:0">${o.right}</div></section>`;
  return `${o.title}<div class="panes" style="flex-direction:${side ? "row" : "column"}">${L}${side ? '<div class="gutter" title="Drag to resize · double-click to reset"></div>' : ""}${R}</div>`;
}

const DIR = { in: ["ARROW_DOWN", "Received"], out: ["ARROW_UP", "Sent"], sys: ["INFO", "Event"], er: ["WARNING", "Error"] };
// rows: [dir, time, title, meta, preview, size]
function logRows(rows, key = "logSel") {
  return `<div class="log plog">${rows.map(([d, t, title, meta, prev, size], i) => `<div class="lr ${String(i) === String(S[key]) ? "sel" : ""} ${d === "er" ? "lr-err" : ""}" data-set="${key}=${i}">
    <span class="pdir ${d}" title="${DIR[d][1]}">${I(DIR[d][0])}</span><span class="ts">${t}</span>
    <span>${title ? `<span class="ev" title="${esc(title)}">${esc(title)}</span>` : ""}</span>
    <span class="body ${d === "sys" ? "sysl" : ""}">${meta ? `<span class="faint" style="font-family:Inter;font-size:11.5px;margin-right:8px">${meta}</span>` : ""}${esc(prev)}</span>
    <span class="sz">${size || ""}</span></div>`).join("")}</div>`;
}
function logDetail(r, body, key = "logSel") {
  const [d, t, title, meta, , size] = r;
  const content = typeof body === "object" ? jsonCode(body, { gutter: false }) : `<div class="mono" style="padding:10px 12px;white-space:pre-wrap">${esc(body)}</div>`;
  return `<div class="hgutter"></div><div class="col" style="flex:0 0 42%;gap:0;min-height:0">
    <div class="row" style="height:34px;padding:0 10px 0 12px;flex:none;gap:8px"><span class="pdir ${d}">${I(DIR[d][0])}</span><span class="med">${DIR[d][1]}</span>
      <span class="mono faint" style="font-size:11.5px">${t}</span>${title ? `<span class="pill outline mono" style="font-size:11px">${esc(title)}</span>` : ""}${meta ? `<span class="faint xs">${meta}</span>` : ""}<span class="faint xs">${size}</span>
      <span class="spacer"></span>${ibtn("COPY", "Copy message", { sm: true })}${ibtn("X", "Close (Esc)", { sm: true, set: `${key}=` })}</div>
    <div class="scroll grow" style="margin:0 12px 12px;border:1px solid var(--border);border-radius:8px;background:var(--surface)">${content}</div></div>`;
}
function logPane(rows, bodies, emptyState) {
  const sel = S.logSel === "" || S.logSel === undefined ? -1 : +S.logSel;
  if (!rows.length) return emptyState;
  return `<div class="scroll grow" style="min-height:0">${logRows(rows)}</div>${sel >= 0 && rows[sel] ? logDetail(rows[sel], bodies[sel] ?? rows[sel][4]) : ""}`;
}
function logBar(title, count) {
  return rbar(`<span class="section-title">${title}</span>${pill(count)}`,
    `${seg("logF", S.logF || "all", [["all", "All"], ["out", "Sent", "ARROW_UP"], ["in", "Received", "ARROW_DOWN"]], { sm: true })}${search("Search messages", { sm: true, style: "width:150px" })}${ibtn("TRASH", "Clear the log", { sm: true })}`);
}

function savedList(items, sendLabel, o = {}) {
  return `<div class="tbl-wrap">${items.map(([name, tag, preview, extra]) => `<div class="lrow">
    <span class="med" style="flex:none">${name}</span>${tag || ""}${extra || ""}
    <span class="mono faint grow ellipsis" style="font-size:11.5px">${esc(preview)}</span>
    <span class="ra">${ibtn("ARROW_BEND_UP_LEFT", "Load into the composer", { sm: true })}${ibtn("PAPER_PLANE_RIGHT", o.connected === false ? "Connect first" : sendLabel, { sm: true })}${ibtn("TRASH", "Remove", { sm: true })}</span></div>`).join("")}</div>`;
}
function runnerSection(endHint, extra = "") {
  return disclosure("Runner", true, `${hint("Collection Runner and <span class=mono>ternarywire run</span>: connect, send the saved messages in order, then stop at the first of:")}
    <div class="group">
      ${frow(`<span class="row" style="gap:8px">${chk(true)}Incoming messages</span>`, "", input("10", { sm: true, style: "width:90px;text-align:right" }))}
      ${frow("Message that ends the session", endHint, input("", { sm: true, mono: true, ph: "no limit", style: "width:160px" }))}
      ${frow("Silence after the last message", "ms, 0 = off", input("1500", { sm: true, style: "width:90px;text-align:right" }))}
      ${frow("Session timeout", "ms, 0 = none", input("30000", { sm: true, style: "width:90px;text-align:right" }))}
      ${extra}
    </div>
    <div>${ghost("Reset to defaults", { size: "sm", icon: "ARROW_COUNTER_CLOCKWISE" })}</div>`, { end: pill("customized", "acc") });
}
function protoScripts(events, src) {
  const cur = S.sev || events[0][0];
  return toolbar(seg("sev", cur, events.map(([id, l, has]) => [id, has ? `${l} •` : l]), { sm: true }), `<span class="hint">JavaScript with <span class="mono">pm.request</span>, <span class="mono">pm.response</span> and <span class="mono">pm.message</span></span>`)
    + paneBody(`<div class="row"><span class="section-title">Test results</span>${pill("✓ 2 passed", "ok")}${pill("0 failed")}<span class="spacer"></span>${ghost("Clear", { size: "sm" })}</div>
      <div class="tbl-wrap">${["Echo returns the same payload", "Server answers within 200 ms"].map((t) => `<div class="row" style="height:32px;padding:0 12px;border-bottom:1px solid var(--border)">${I("CHECK_CIRCLE", "ok")}<span class="grow">${t}</span>${pill("PASS", "ok")}</div>`).join("")}</div>
      ${editorWell(jsHl(src), { h: 210, margin: "0" })}`, { pad: "0 12px 16px", gap: 10 });
}
function inheritAuth(note) {
  return toolbar(`<span class="section-title">Auth type</span>${selectbox("Inherit from parent", { sm: true, icon: "KEY", style: "min-width:190px" })}`)
    + paneBody(card(`<div class="row" style="gap:12px;align-items:flex-start"><div class="ptitle"><div class="pi">${I("SHIELD_CHECK")}</div></div>
      <div class="col" style="gap:4px"><div class="row"><span class="semi">Inherited from “Realtime &amp; AI”</span>${pill("Bearer Token", "acc")}</div>
      <div class="hint">This request uses Bearer Token from “Realtime &amp; AI”. Change it in the collection settings.</div></div></div>`) + (note ? hint(note) : ""), { pad: "0 12px 16px" });
}
const kvNoDesc = (rows, o = {}) => kv(rows, { ...o, desc: false });

/* ================================================================== WebSocket */
const WS_LOG = [
  ["sys", "09:40:02.114", "", "", "Connected (HTTP 101) in 12 ms, protocol graphql-transport-ws", ""],
  ["out", "09:40:02.320", "", "", '{"type":"subscribe","channel":"books","token":"eyJhbGci…"}', "61 B"],
  ["in", "09:40:02.322", "", "", '{"type":"subscribed","channel":"books"}', "38 B"],
  ["in", "09:40:09.871", "", "", '{"type":"book.added","id":6,"title":"Bumi Manusia","author":"Pramoedya Ananta Toer","price":15.5}', "96 B"],
  ["out", "09:40:15.002", "", "", "ping", "4 B"],
  ["in", "09:40:15.003", "", "", "pong", "4 B"],
  ["in", "09:40:31.540", "", "", '{"type":"stock","id":3,"inStock":true}', "37 B"],
  ["in", "09:40:44.019", "", "", "01 9f 3c 00 7a", "5 B · bin"],
  ["out", "09:41:02.716", "", "", '{"type":"subscribe","channel":"orders"}', "39 B"],
  ["er", "09:41:02.730", "", "", "Error: channel “orders” needs the orders:read scope", ""],
  ["in", "09:41:20.118", "", "", '{"type":"book.updated","id":2,"price":52}', "40 B"],
];
const WS_BODIES = { 3: { type: "book.added", id: 6, title: "Bumi Manusia", author: "Pramoedya Ananta Toer", price: 15.5 } };

function wsLeft() {
  switch (S.pt || "message") {
    case "message": {
      const bin = S.wsmode === "bin";
      return toolbar(seg("wsmode", bin ? "bin" : "text", [["text", "Text"], ["bin", "Binary"]], { sm: true }) + (bin ? `<span class="hint">hex or base64, e.g. 01 02 ff or AQL/</span>` : ""),
        `${ghost("Save message", { size: "sm", icon: "BOOKMARK_SIMPLE", title: "Keep this message with the request" })}${kbd("⇧⌘↩")}${primary("Send", { size: "sm", icon: "PAPER_PLANE_RIGHT", title: "Send (⇧⌘↩)" })}`)
        + (bin ? editorWell(["01 9f 3c 00 7a"]) : editorWell(withVars(jsonLines({ type: "subscribe", channel: "books", token: "{{wsToken}}" })), { cur: 2 }));
    }
    case "params": return toolbar(`<span class="section-title">Query parameters</span>`, ghost("Bulk edit", { size: "sm", icon: "TEXT_ALIGN_LEFT" })) + paneBody(kvNoDesc([[true, "room", "books"], [true, "v", "2"]]) + hint("Kept in sync with the URL."), { pad: "0 12px 16px" });
    case "headers": return toolbar(`<span class="section-title">Headers</span>`, ghost("Bulk edit", { size: "sm", icon: "TEXT_ALIGN_LEFT" }))
      + paneBody(kvNoDesc([[true, "Origin", "http://localhost:5173"], [true, "X-Client", "ternarywire"]])
        + `<div class="col" style="gap:6px"><span class="section-title">Subprotocols</span>${input("graphql-transport-ws", { mono: true, sm: true })}${hint("Sent as Sec-WebSocket-Protocol, comma separated.")}</div>`, { pad: "0 12px 16px" });
    case "auth": return inheritAuth();
    case "saved": return toolbar(`<span class="section-title">Saved messages</span>${pill("3")}`, `<span class="hint">Stored with the request</span>`)
      + paneBody(savedList([["Subscribe books", "", '{"type":"subscribe","channel":"books"}'], ["Ping", "", "ping"], ["Raw frame", pill("Binary", "info"), "01 9f 3c 00 7a"]], "Send now")
        + runnerSection("message text, or its JSON type/event"), { pad: "0 12px 16px" });
    case "scripts": return protoScripts([["connect", "Before connect", true], ["message", "On message", true]], 'pm.test("Echo returns the same payload", () => {\n  pm.expect(pm.message.json().type).to.not.equal("error");\n});');
  }
  return "";
}
function wsView() {
  const sel = S.logSel === "" ? -1 : +S.logSel;
  return protoView({
    title: protoTitle("Echo socket"),
    bar: pbar("WS", pfld(urlHtml("{{wsUrl}}/live?room=books&v=2"), { title: "Press ↩ to connect" })) + bigBtn("Disconnect", "PLUGS", "", "Disconnect"),
    ltabs: tabs("pt", S.pt || "message", [["message", "Message"], ["params", "Params", { count: 2 }], ["headers", "Headers", { count: 2 }], ["auth", "Auth", { dot: true }], ["saved", "Saved", { count: 3 }], ["scripts", "Scripts", { count: 2 }]]),
    left: wsLeft(),
    status: `${pill("Connected · 2:14", "ok", { dot: true, live: true, lg: true })}<span class="rmeta">${mt("PLUGS_CONNECTED", "101 · 12 ms", "HTTP 101 · handshake 12 ms · graphql-transport-ws")}${mt("ARROW_UP", "4 sent")}${mt("ARROW_DOWN", "6 received")}</span>`,
    rtabs: logBar("Messages", WS_LOG.length),
    right: logPane(WS_LOG, WS_BODIES, empty("CHATS", "No messages yet", "Connect, then send a message to see the conversation here.")),
  });
}

/* ================================================================== Socket.IO */
const SIO_LOG = [
  ["sys", "16:18:10.402", "", "", "Connected to http://127.0.0.1:18902/chat (sid Xk2pQ9…)", ""],
  ["in", "16:18:15.968", "tick", "", '[{"count":1}]', "13 B"],
  ["in", "16:18:17.969", "tick", "", '[{"count":2}]', "13 B"],
  ["out", "16:18:20.989", "message", "ack", '[{"room":"books","text":"hello"}]', "33 B"],
  ["in", "16:18:20.990", "ack #3", "", '["delivered"]', "13 B"],
  ["in", "16:18:21.402", "join", "ack requested", '[{"user":"u_192","room":"books"}]', "34 B"],
  ["in", "16:18:23.970", "tick", "", '[{"count":3}]', "13 B"],
  ["in", "16:18:24.811", "upload", "", '[{"name":"cover.jpg"},(binary 182 KB)]', "182 KB · bin"],
  ["in", "16:18:25.969", "tick", "", '[{"count":4}]', "13 B"],
];
function sioArg(n, type, body) {
  return `<div class="argc"><div class="ah"><span class="num">${n}</span>${seg(`sioa${n}`, type, [["json", "JSON"], ["text", "Text"], ["bin", "Binary"], ["file", "File"]], { sm: true })}<span class="spacer"></span>${ibtn("TRASH", "Remove argument", { sm: true })}</div>${body}</div>`;
}
function sioLeft() {
  switch (S.pt || "emit") {
    case "emit":
      return toolbar(`<span class="muted">Event</span>${input("message", { sm: true, mono: true, style: "width:150px" })}<label class="row" style="gap:6px" title="Ask the server to acknowledge; its answer appears in the log">${chk(true)}Expect ack</label>`,
        `${ibtn("BOOKMARK_SIMPLE", "Save message", { sm: true })}${kbd("⇧⌘↩")}${primary("Emit", { size: "sm", icon: "PAPER_PLANE_RIGHT", title: "Emit (⇧⌘↩)" })}`)
        + paneBody(`<div class="row"><span class="section-title">Arguments</span>${pill("2")}<span class="hint">JSON, text or binary (an attachment, a Buffer on the server).</span></div>
          ${sioArg(1, S.sioa1 || "json", code(withVars(['<span class="p">{</span> <span class="k">"room"</span><span class="p">:</span> <span class="s">"books"</span><span class="p">,</span> <span class="k">"text"</span><span class="p">:</span> <span class="s">"hello"</span><span class="p">,</span> <span class="k">"user"</span><span class="p">:</span> <span class="s">"{{userId}}"</span> <span class="p">}</span>']), { gutter: false }))}
          ${sioArg(2, S.sioa2 || "file", `<div class="row" style="padding:8px 10px;gap:8px">${btn("Choose file…", { size: "sm", icon: "FILE_ARROW_UP" })}${input("assets/cover.jpg", { sm: true, mono: true, style: "flex:1" })}<span class="faint xs">182 KB</span></div>`)}
          <div>${ghost("Add argument", { size: "sm", icon: "PLUS" })}</div>`, { pad: "0 12px 16px", gap: 10 });
    case "events":
      return toolbar(`<span class="section-title">Event listeners</span>`, `<label class="row" style="gap:6px">${chk(false)}<span class="small">Only show incoming events listed here</span></label>`)
        + paneBody(hint("Incoming events with these names are expected; the log can show only them.")
          + `<div class="tbl-wrap">${[["tick", true], ["join", true], ["upload", false]].map(([n, on]) => `<div class="lrow"><span title="Listen to this event">${chk(on)}</span>${input(n, { sm: true, mono: true, style: "flex:1;border-color:transparent;background:none" })}${ibtn("TRASH", "Remove event", { sm: true })}</div>`).join("")}</div>
          <div>${ghost("Add event", { size: "sm", icon: "PLUS" })}</div>`, { pad: "0 12px 16px", gap: 10 });
    case "saved":
      return toolbar(`<span class="section-title">Saved messages</span>${pill("3")}`, `<span class="hint">Stored with the request</span>`)
        + paneBody(savedList([["Say hello", `<span class="ev mono pill outline" style="font-size:11px">message</span>`, '{"room":"books","text":"hello"}', '<span class="faint xs">ack</span>'], ["Join room", `<span class="pill outline mono" style="font-size:11px">join</span>`, '{"room":"books"}'], ["Upload cover", `<span class="pill outline mono" style="font-size:11px">upload</span>`, "(binary) cover.jpg"]], "Emit now")
          + runnerSection("event name", frow("Answer server acknowledgements with", "JSON array of arguments; empty = no answer", input('["ok"]', { sm: true, mono: true, style: "width:160px" }))), { pad: "0 12px 16px" });
    case "headers":
      return toolbar(`<span class="section-title">Headers</span>`, ghost("Bulk edit", { size: "sm", icon: "TEXT_ALIGN_LEFT" }))
        + paneBody(kvNoDesc([[true, "Authorization", "Bearer {{vault:chat-token}}"]]) + `<span class="section-title">Query parameters</span>` + kvNoDesc([]), { pad: "0 12px 16px" });
    case "settings":
      return toolbar(`<span class="section-title">Connection</span>`) + paneBody(group("", [
        frow("Transport", "Auto: long-polling first, then upgrade to WebSocket, like the official client", seg("siot", "auto", [["auto", "Auto"], ["ws", "WebSocket"], ["poll", "Polling"]], { sm: true })),
        frow("Server version", "", seg("siov", "v4", [["v4", "v3 / v4"], ["v2", "v2"]], { sm: true })),
        frow("Handshake path", "", input("/socket.io/", { sm: true, mono: true, style: "width:150px" })),
        frow("Verify SSL certificates", "For https:// servers", chk(true)),
      ]) + `<div class="col" style="gap:6px"><span class="section-title">Handshake auth</span>${hint("A JSON object sent with the connection (socket.handshake.auth).")}${editorWell(withVars(['<span class="p">{</span> <span class="k">"token"</span><span class="p">:</span> <span class="s">"{{vault:chat-token}}"</span> <span class="p">}</span>']), { h: 70, margin: "0" })}</div>
        <div class="col" style="gap:6px"><span class="section-title">Authorization</span><div class="row">${selectbox("Inherit from parent", { sm: true, icon: "KEY", style: "min-width:190px" })}<span class="hint">Bearer Token from “Realtime &amp; AI”</span></div></div>`, { pad: "0 12px 16px" });
    case "scripts": return protoScripts([["connect", "Before connect", false], ["message", "On message", true]], 'pm.test("tick counts up", () => {\n  pm.expect(pm.message.args[0].count).to.be.above(0);\n});');
  }
  return "";
}
function sioView() {
  const notice = `<div style="padding:10px 12px 0;flex:none"><div class="notice">
      <div class="nt">${I("HOURGLASS")}The server is waiting for acknowledgements<span class="spacer"></span>${pill("1", "acc")}</div>
      <div class="row" style="gap:8px"><span class="pill outline mono">#7</span><span class="mono semi" style="font-size:12px">join</span>
        ${selectbox("JSON", { sm: true, style: "width:84px", title: "Arguments as a JSON array, a single JSON value, or plain text." })}
        ${input('["welcome", {"room":"books"}]', { sm: true, mono: true, style: "flex:1" })}${btn("Send ack", { size: "sm", icon: "CHECK" })}</div></div></div>`;
  return protoView({
    title: protoTitle("Chat events", pill("websocket", "", { outline: true, title: "Transport in use" })),
    bar: pbar("SIO", pfld(urlHtml("http://127.0.0.1:18902")) + pfld("/chat", { style: "flex:0 0 120px", title: "Namespace" })) + bigBtn("Disconnect", "PLUGS", "", "Disconnect"),
    ltabs: tabs("pt", S.pt || "emit", [["emit", "Emit"], ["events", "Events", { count: 2 }], ["saved", "Saved", { count: 3 }], ["headers", "Headers", { count: 1 }], ["settings", "Settings"], ["scripts", "Scripts", { count: 1 }]]),
    left: sioLeft(),
    status: `${pill("Connected · 0:15", "ok", { dot: true, live: true, lg: true })}<span class="rmeta">${mt("FINGERPRINT", "sid Xk2pQ9", "Session id")}${mt("ARROW_UP", "1 sent")}${mt("ARROW_DOWN", "7 received")}</span>`,
    rtabs: logBar("Events", SIO_LOG.length),
    right: notice + logPane(SIO_LOG, { 5: [{ user: "u_192", room: "books" }] }, empty("CHATS", "No events yet", "Connect, then emit an event; events from the server appear here.")),
  });
}

/* ================================================================== MQTT */
const MQTT_LOG = [
  ["sys", "10:02:11.004", "", "", "Connected (session present)", ""],
  ["sys", "10:02:11.010", "", "", "Subscribed to sensors/# (QoS 1)", ""],
  ["sys", "10:02:11.011", "", "", "Subscribed to status/+ (QoS 0)", ""],
  ["in", "10:02:11.040", "status/kitchen", "QoS 0 · retained", "online", "6 B"],
  ["in", "10:02:14.502", "sensors/kitchen/temp", "QoS 1", '{"c":21.4,"at":"10:02:14"}', "26 B"],
  ["in", "10:02:16.118", "sensors/garage/humidity", "QoS 1 · 2 props", '{"rh":63}', "9 B"],
  ["out", "10:02:19.733", "sensors/kitchen/target", "QoS 1 · retained", '{"c":22,"by":"{{clientId}}"}', "24 B"],
  ["in", "10:02:24.511", "sensors/kitchen/temp", "QoS 1", '{"c":21.6,"at":"10:02:24"}', "26 B"],
  ["in", "10:02:34.517", "sensors/kitchen/temp", "QoS 1", '{"c":21.9,"at":"10:02:34"}', "26 B"],
];
function mqttLeft() {
  switch (S.pt || "publish") {
    case "publish":
      return toolbar(`<span class="muted" style="width:40px">Topic</span>${input("sensors/kitchen/target", { sm: true, mono: true, style: "flex:1" })}`)
        + toolbar(`<span class="muted" style="width:40px">QoS</span>${seg("qos", S.qos || "1", [["0", "0"], ["1", "1"], ["2", "2"]], { sm: true })}<label class="row" style="gap:6px;margin-left:6px" title="The broker keeps the last retained message of a topic for new subscribers">${chk(true)}Retain</label>`,
          `${ibtn("BOOKMARK_SIMPLE", "Save message", { sm: true })}${kbd("⇧⌘↩")}${primary("Publish", { size: "sm", icon: "PAPER_PLANE_RIGHT", title: "Publish (⇧⌘↩)" })}`)
        + editorWell(withVars(jsonLines({ c: 22, by: "{{clientId}}" })), { margin: "4px 12px 10px" })
        + `<div style="padding:0 12px 12px;flex:none">${disclosure("User properties", true, kv([[true, "unit", "celsius"], [true, "source", "thermostat-2"]], { desc: false, k: "Name" }), { count: 2, end: '<span class="faint xs">MQTT 5</span>' })}</div>`;
    case "subs":
      return toolbar(`<span class="section-title">Subscriptions</span>${pill("3")}`, ghost("Add subscription", { size: "sm", icon: "PLUS" }))
        + paneBody(hint("Enabled subscriptions are made automatically after connecting. Wildcards + and # are allowed.")
          + `<div class="tbl-wrap">${[["sensors/#", "1", true, true], ["status/+", "0", true, true], ["alerts/critical", "2", false, false]].map(([t, q, on, live]) => `<div class="lrow"><span title="Subscribe after connecting">${chk(on)}</span>${input(t, { sm: true, mono: true, style: "flex:1;border-color:transparent;background:none" })}${seg("q" + t, q, [["0", "0"], ["1", "1"], ["2", "2"]], { sm: true })}${live ? ghost("", { size: "sm", icon: "MINUS", title: "Unsubscribe" }) : ghost("", { size: "sm", icon: "PLUS", title: "Subscribe" })}${ibtn("TRASH", "Remove subscription", { sm: true })}</div>`).join("")}</div>`, { pad: "0 12px 16px", gap: 10 });
    case "saved":
      return toolbar(`<span class="section-title">Saved messages</span>${pill("2")}`)
        + paneBody(savedList([["Target 22 °C", `<span class="pill outline mono" style="font-size:11px">sensors/kitchen/target</span>`, '{"c":22}', pill("QoS 1")], ["Reboot", `<span class="pill outline mono" style="font-size:11px">cmd/reboot</span>`, "now"]], "Publish now")
          + runnerSection("topic filter, e.g. status/+"), { pad: "0 12px 16px" });
    case "conn":
      return toolbar(`<span class="section-title">Connection</span>`) + paneBody(group("", [
        frow("Version", "", seg("mv", "5", [["3", "3.1.1"], ["5", "5.0"]], { sm: true })),
        frow("Client ID", "", input("", { sm: true, mono: true, ph: "generated when empty", style: "width:190px" })),
        frow("Keep alive (s)", "", input("60", { sm: true, style: "width:80px;text-align:right" })),
        frow("Clean session", "Start without stored subscriptions", chk(false)),
        frow("Verify SSL certificates", "For mqtts:// and wss:// brokers", chk(true)),
      ]) + group("Credentials", [
        frow(`<span class="row" style="gap:8px">${chk(true)}Use username and password</span>`, "", ""),
        frow("Username", "", input("{{mqttUser}}", { sm: true, mono: true, style: "width:190px" })),
        frow("Password", "", input("••••••••", { sm: true, style: "width:190px" })),
      ]) + group("Last will", [
        frow(`<span class="row" style="gap:8px">${chk(true)}Send a last will</span>`, "Published by the broker when this client disconnects unexpectedly.", ""),
        frow("Topic", "", input("clients/ternarywire/status", { sm: true, mono: true, style: "width:220px" })),
        frow("Payload", "", input("offline", { sm: true, mono: true, style: "width:220px" })),
        frow("QoS", "", `<span class="row">${seg("lwq", "1", [["0", "0"], ["1", "1"], ["2", "2"]], { sm: true })}<label class="row" style="gap:6px">${chk(true)}Retain</label></span>`),
      ]), { pad: "0 12px 16px", gap: 16 });
    case "scripts": return protoScripts([["connect", "Before connect", false], ["message", "On message", true]], 'const t = pm.message.json().c;\npm.test("temperature is plausible", () => pm.expect(t).to.be.within(-20, 60));');
  }
  return "";
}
function mqttView() {
  const chips = `<div class="row" style="padding:8px 12px;gap:6px;flex:none;border-bottom:1px solid var(--border);flex-wrap:wrap">
    <span class="eyebrow" style="margin-right:2px">Subscribed</span>
    ${[["sensors/#", 1], ["status/+", 0]].map(([t, q]) => `<span class="subchip"><span class="mono">${t}</span><span class="faint">QoS ${q}</span><span class="x" title="Unsubscribe">${I("X")}</span></span>`).join("")}
    ${ghost("Add", { size: "sm", icon: "PLUS", set: "pt=subs" })}</div>`;
  return protoView({
    title: protoTitle("Sensor stream", pill("MQTT 5", "", { outline: true })),
    bar: pbar("MQTT", pfld(urlHtml("mqtt://broker.local:1883"))) + bigBtn("Disconnect", "PLUGS", "", "Disconnect"),
    ltabs: tabs("pt", S.pt || "publish", [["publish", "Publish"], ["subs", "Subscriptions", { count: 2 }], ["saved", "Saved", { count: 2 }], ["conn", "Connection"], ["scripts", "Scripts", { count: 1 }]]),
    left: mqttLeft(),
    status: `${pill("Connected · 5:41", "ok", { dot: true, live: true, lg: true })}<span class="rmeta">${mt("CHECK", "session present")}${mt("ARROW_UP", "1 sent")}${mt("ARROW_DOWN", "5 received")}</span>`,
    rtabs: logBar("Messages", MQTT_LOG.length),
    right: chips + logPane(MQTT_LOG, { 4: { c: 21.4, at: "10:02:14" } }, empty("CHATS", "No messages yet", "Connect to the broker; messages on your subscriptions appear here.")),
  });
}

/* ================================================================== gRPC */
const GRPC_LOG = [
  ["sys", "11:20:03.101", "", "", "Schema loaded: 2 services, 6 methods", ""],
  ["sys", "11:20:05.448", "", "", "Connecting…", ""],
  ["in", "11:20:05.469", "HelloReply", "", '{"message":"Hello, Ada! Nice to see you again.","sent_at":"2026-11-11T11:20:05Z"}', "58 B"],
  ["sys", "11:20:05.471", "", "", "OK (0) in 23 ms", ""],
];
const KIND = { unary: ["unary", "info"], server: ["server stream", "ok"], client: ["client stream", "warn"], bidi: ["bidi stream", "acc"] };
function grpcPicker() {
  const svc = [["helloworld.Greeter", [["SayHello", "HelloRequest → HelloReply", "unary", true], ["SayHelloStream", "HelloRequest → stream HelloReply", "server"]]],
    ["routeguide.RouteGuide", [["GetFeature", "Point → Feature", "unary"], ["ListFeatures", "Rectangle → stream Feature", "server"], ["RecordRoute", "stream Point → RouteSummary", "client"], ["RouteChat", "stream RouteNote → stream RouteNote", "bidi"]]]];
  return `<div class="mpop" style="top:52px;right:124px" data-stop="1">
    <div style="padding:4px 4px 8px">${search("Search methods", { sm: true }).replace('class="input sm"', 'class="input sm focus"')}</div>
    <div class="scroll" style="max-height:330px">${svc.map(([s, ms]) => `<div class="row" style="height:28px;padding:0 8px;gap:6px">${I("CUBE", "faint")}<span class="eyebrow" style="text-transform:none;letter-spacing:0;font-size:11.5px;color:var(--text-muted)">${s}</span></div>`
      + ms.map(([n, sig, k, on]) => `<div class="mrow ${on ? "on" : ""}"><div class="col grow" style="gap:0"><span class="med">${n}</span><span class="mono faint ellipsis" style="font-size:11px">${sig}</span></div>${pill(KIND[k][0], KIND[k][1])}</div>`).join("")).join("")}</div>
    <div class="row" style="border-top:1px solid var(--border);margin-top:6px;padding:6px 4px 0">${ghost("Reload schema", { size: "sm", icon: "ARROWS_CLOCKWISE" })}<span class="spacer"></span><span class="faint xs">from server reflection</span></div></div>`;
}
function grpcLeft() {
  switch (S.pt || "message") {
    case "message":
      return toolbar(`${pill("unary", "info")}<span class="mono small" title=".helloworld.HelloRequest → .helloworld.HelloReply">HelloRequest ${I("ARROW_RIGHT", "faint")} HelloReply</span>`,
        `${ghost("Use example", { size: "sm", icon: "SPARKLE", title: "Fill with a template of the input message" })}`)
        + editorWell(withVars(jsonLines({ name: "{{userName}}", greeting_style: "FORMAL", tags: ["vip"] })), { cur: 1 });
    case "metadata":
      return toolbar(`<span class="section-title">Metadata</span>`, ghost("Bulk edit", { size: "sm", icon: "TEXT_ALIGN_LEFT" }))
        + paneBody(kv([[true, "x-request-id", "{{$guid}}", "Traced in the gateway"], [true, "trace-bin", "AAECAw==", "base64"]]) + hint("Keys ending in <span class=mono>-bin</span> take base64 values."), { pad: "0 12px 16px" });
    case "auth": return inheritAuth("Bearer, Basic and API key (header) are sent as metadata.");
    case "schema":
      return toolbar(seg("gsrc", S.gsrc || "refl", [["refl", "Server reflection"], ["proto", ".proto files"]], { sm: true }), btn("Reload schema", { size: "sm", icon: "ARROWS_CLOCKWISE" }))
        + paneBody((S.gsrc === "proto"
          ? `<div class="tbl-wrap">${[["FILE", "protos/helloworld.proto", ""], ["FILE", "protos/route_guide.proto", ""], ["FOLDER_SIMPLE", "third_party/googleapis", pill("import path")]].map(([ic, p, x]) => `<div class="lrow">${I(ic, "faint")}<span class="mono grow" style="font-size:12px">${p}</span>${x}${ibtn("X", "Remove file", { sm: true })}</div>`).join("")}</div><div class="row">${ghost("Add .proto files", { size: "sm", icon: "FILE_PLUS" })}${ghost("Add import path", { size: "sm", icon: "FOLDER_PLUS" })}</div>`
          : hint("The server lists its services itself (grpc.reflection v1 or v1alpha)."))
          + `<span class="section-title">Services</span><div class="tbl-wrap">${[["helloworld.Greeter", 2], ["routeguide.RouteGuide", 4]].map(([n, c]) => `<div class="lrow">${I("CUBE", "faint")}<span class="mono grow" style="font-size:12px">${n}</span><span class="faint xs">${c} methods</span></div>`).join("")}</div>`, { pad: "0 12px 16px", gap: 10 });
    case "scripts": return protoScripts([["before", "Before invoke", false], ["message", "On message", false], ["after", "After response", true]], 'pm.test("status is OK", () => pm.expect(pm.response.code).to.equal(0));');
    case "settings":
      return toolbar(`<span class="section-title">Connection</span>`) + paneBody(group("", [
        frow("TLS", "", seg("gtls", "scheme", [["scheme", "By scheme"], ["on", "On"], ["off", "Off"]], { sm: true })),
        frow("Verify SSL certificates", "When TLS is on", chk(true)),
        frow("Deadline", "ms, 0 = none", input("5000", { sm: true, style: "width:90px;text-align:right" })),
      ]), { pad: "0 12px 16px" });
  }
  return "";
}
function grpcView(picker) {
  const rt = S.rt || "messages";
  let right;
  if (rt === "messages") right = logPane(GRPC_LOG, { 2: { message: "Hello, Ada! Nice to see you again.", sent_at: "2026-11-11T11:20:05Z" } }, empty("LIGHTNING", "No response yet", "Pick a method and press Invoke to call it."));
  else if (rt === "headers") right = paneBody(table([{ label: "Key", w: "40%" }, { label: "Value" }], [["content-type", "application/grpc"], ["grpc-accept-encoding", "identity, deflate, gzip"], ["date", "Tue, 11 Nov 2026 04:20:05 GMT"]].map(([a, b]) => [`<span class="mono muted">${a}</span>`, `<span class="mono">${b}</span>`])), { pad: "12px 12px 16px" });
  else right = paneBody(table([{ label: "Key", w: "40%" }, { label: "Value" }], [["grpc-status", "0"], ["grpc-message", ""]].map(([a, b]) => [`<span class="mono muted">${a}</span>`, `<span class="mono">${b || '<span class="faint">(empty)</span>'}</span>`])), { pad: "12px 12px 16px" });
  return protoView({
    title: protoTitle("Greeter", pill("6 methods", "ok", { dot: true, title: "Schema from server reflection" })),
    bar: pbar("gRPC", pfld(urlHtml("localhost:50051"), { style: "flex:0 1 150px" }) + pcombo(`${I("TREE_STRUCTURE", "faint")}<span class="muted ellipsis" style="min-width:0">helloworld.Greeter /</span><span class="med" style="flex:none">SayHello</span>`, { style: "flex:1 1 0", title: "Method" }))
      + bigBtn("Invoke", "PLAY", "primary", "Invoke (⇧⌘↩)") + (picker ? grpcPicker() : ""),
    barOverlay: picker,
    ltabs: tabs("pt", S.pt || "message", [["message", "Message"], ["metadata", "Metadata", { count: 2 }], ["auth", "Auth", { dot: true }], ["schema", "Schema"], ["scripts", "Scripts", { count: 1 }], ["settings", "Settings"]]),
    left: grpcLeft(),
    status: `${pill("0 OK", "ok", { lg: true, title: "Status message: (empty)" })}<span class="rmeta">${mt("TIMER", "23 ms")}${mt("ARROW_DOWN", "1 message")}${mt("CHECK_CIRCLE", "1/1 tests")}</span>`,
    rtabs: tabs("rt", rt, [["messages", "Messages", { count: 1 }], ["headers", "Headers", { count: 3 }], ["trailers", "Trailers", { count: 2 }]], `${search("Search messages", { sm: true, style: "width:160px" })}${ibtn("TRASH", "Clear the log", { sm: true })}`),
    right,
  });
}

/* ================================================================== MCP */
const MCP_TOOLS = [
  ["search_books", "Search books", "Search the catalogue by title, author or tag and return matching books."],
  ["get_book", "Get book", "Fetch one book with its stock and price history."],
  ["create_order", "Create order", "Place an order for one or more books for the signed-in user."],
  ["list_orders", "List orders", "Orders of the signed-in user, newest first."],
  ["recommend", "Recommend", "Suggest books similar to the given ones."],
  ["update_stock", "Update stock", "Change the stock count of a book (admin only)."],
];
const MCP_LOG = [
  ["out", "12:01:44.210", "initialize", "", '{"protocolVersion":"2025-06-18","capabilities":{"sampling":{},"elicitation":{}}}', "142 B"],
  ["in", "12:01:44.262", "result #0", "", '{"serverInfo":{"name":"bookstore-tools","version":"1.4.0"},"capabilities":{"tools":{}}}', "318 B"],
  ["out", "12:01:44.263", "notifications/initialized", "", "{}", "52 B"],
  ["out", "12:01:44.270", "tools/list", "", "{}", "48 B"],
  ["in", "12:01:44.301", "result #1", "", '{"tools":[{"name":"search_books",…}, …12 tools]}', "4.1 KB"],
  ["out", "12:03:10.022", "tools/call", "", '{"name":"search_books","arguments":{"query":"rust","limit":5}}', "96 B"],
  ["in", "12:03:10.101", "log/info", "", "querying index books_v3 (2 shards)", "61 B"],
  ["in", "12:03:10.204", "result #2", "", '{"content":[{"type":"text","text":"Found 2 books"}],"structuredContent":{…}}', "612 B"],
  ["in", "12:03:10.205", "stderr", "", "warn: cache miss for query \"rust\"", "34 B"],
];
function mcpBrowser(kind) {
  const lists = {
    tools: MCP_TOOLS.map(([n, t, d]) => [t, n, d]),
    resources: [["Catalogue", "application/json", "book://catalog"], ["Stock levels", "text/csv", "book://stock.csv"], ["Book by id", "template", "book://books/{id}"], ["Readme", "text/markdown", "file:///srv/README.md"]],
    prompts: [["Summarize catalogue", "1 args", "Short summary of what is in stock."], ["Write blurb", "2 args", "Back-cover blurb for a book."], ["Compare books", "2 args", "Compare two books for a reader."]],
    saved: [["Search rust", "tool", '{"query":"rust","limit":5}'], ["Catalogue", "resource", "book://catalog"]],
  }[kind];
  const sel = lists[0];
  const icon = { tools: "PUZZLE_PIECE", resources: "FILE", prompts: "CHAT_CIRCLE", saved: "BOOKMARK_SIMPLE" }[kind];
  const listCol = `<div class="col" style="width:200px;flex:none;gap:0;border-right:1px solid var(--border);min-height:0">
    <div style="padding:10px 10px 6px">${search("Filter", { sm: true })}</div>
    <div class="scroll grow" style="padding:0 6px 8px;display:flex;flex-direction:column;gap:2px">${lists.map(([t, tag, d], i) => `<div class="tcard ${i === 0 ? "sel" : ""}"><div class="row" style="gap:6px"><span class="med ellipsis grow">${t}</span></div>${tag ? `<span class="mono faint ellipsis" style="font-size:10.5px">${tag}</span>` : ""}<div class="td">${d}</div></div>`).join("")}
    ${kind === "saved" ? `<div class="hint" style="padding:8px 6px">Right-click a saved call to remove it.</div>` : ""}</div></div>`;
  const isRes = kind === "resources";
  const args = kind === "prompts" ? { topic: "rust", audience: "beginner" } : kind === "tools" || kind === "saved" ? { query: "rust", limit: 5 } : null;
  const action = { tools: "Call tool", resources: "Read", prompts: "Get prompt", saved: "Call tool" }[kind];
  const form = `<div class="scroll grow" style="padding:12px 12px 16px;display:flex;flex-direction:column;gap:12px;min-width:0">
    <div class="row" style="gap:10px;align-items:flex-start"><div class="ptitle"><div class="pi" style="width:30px;height:30px;font-size:16px;border-radius:8px">${I(icon)}</div></div>
      <div class="col" style="gap:2px;min-width:0"><div class="row" style="gap:6px"><span class="semi" style="font-size:14px">${sel[0]}</span><span class="pill outline mono" style="font-size:10.5px">${kind === "resources" ? sel[2] : kind === "saved" ? "search_books" : sel[1]}</span></div><div class="hint">${kind === "resources" ? "Catalogue as JSON, refreshed every minute." : sel[2]}</div></div></div>
    ${isRes ? field("URI", input("book://catalog", { sm: true, mono: true }))
      : `<div class="col" style="gap:6px"><div class="row"><span class="section-title">Arguments</span><span class="hint">JSON object</span><span class="spacer"></span>${ghost("Reset", { size: "sm", icon: "ARROW_COUNTER_CLOCKWISE", title: "Fill with a template of the arguments" })}</div>${editorWell(jsonLines(args), { h: 132, margin: "0" })}</div>`}
    ${kind === "tools" || kind === "saved" ? disclosure("Input schema", false, "", { end: '<span class="faint xs mono">2 properties · query required</span>' }) : ""}
    <div class="row">${primary(action, { size: "sm", icon: "PLAY" })}${kind !== "saved" ? ghost("Save call", { size: "sm", icon: "BOOKMARK_SIMPLE" }) : ""}</div></div>`;
  return `<div class="row grow" style="gap:0;align-items:stretch;min-height:0">${listCol}${form}</div>`;
}
function mcpLeft() {
  const pt = S.pt || "tools";
  if (["tools", "resources", "prompts", "saved"].includes(pt)) return mcpBrowser(pt);
  if (pt === "conn") {
    return paneBody(card(`<div class="row" style="gap:10px"><div class="ptitle"><div class="pi">${I("PUZZLE_PIECE")}</div></div><div class="col" style="gap:4px"><div class="row"><span class="semi">bookstore-tools</span>${pill("1.4.0", "", { outline: true })}${pill("protocol 2025-06-18", "", { outline: true })}</div>
        <div class="row" style="gap:4px;flex-wrap:wrap">${["tools", "resources", "prompts", "logging", "completions"].map((c) => pill("✓ " + c, "ok")).join("")}</div></div></div>
        <div class="hint" style="margin-top:10px">Instructions: “Use search_books before recommending; prices are in USD.”</div>`)
      + group("HTTP", [
        frow("Transport", "Auto: Streamable HTTP, falling back to HTTP+SSE", seg("mtr", "auto", [["auto", "Auto"], ["sh", "Streamable HTTP"], ["sse", "HTTP+SSE (legacy)"]], { sm: true })),
      ], { hint: "Servers that require OAuth open your browser to sign in; tokens are kept in the Keychain." })
      + `<div class="col" style="gap:6px"><span class="section-title">Headers</span>${kvNoDesc([[true, "Authorization", "Bearer {{vault:mcp-token}}"]])}${hint("A Bearer token in Authorization is sent as the Authorization header.")}</div>`
      + `<div class="col" style="gap:6px"><span class="section-title">Authorization</span><div class="row">${selectbox("Inherit from parent", { sm: true, icon: "KEY", style: "min-width:190px" })}<span class="hint">No Auth from “Realtime &amp; AI”</span></div></div>`
      + `<div class="col" style="gap:6px"><span class="section-title">Export config</span>${hint("Use this server in other MCP hosts.")}<div class="row">${selectbox("Cursor / Claude Code (mcpServers)", { sm: true, style: "flex:1" })}${btn("Copy config", { size: "sm", icon: "COPY" })}</div></div>`, { pad: "12px 12px 16px" });
  }
  return protoScripts([["before", "Before call", false], ["after", "After result", true]], 'pm.test("found books", () => {\n  pm.expect(pm.response.json().structuredContent.books.length).to.be.above(0);\n});');
}
function mcpElicit() {
  return `<div style="padding:10px 12px 0;flex:none"><div class="notice">
    <div class="nt">${I("QUESTION")}bookstore-tools asks for input<span class="spacer"></span><span class="faint xs" style="font-weight:400">1 more request waiting</span></div>
    <div class="small">Which warehouse should the order ship from? <a class="accent">Shipping policy</a></div>
    <div class="grid2" style="gap:8px 12px">
      ${field("Warehouse *", selectbox("Jakarta (JKT-2)", { sm: true }))}
      ${field("Quantity *", input("2", { sm: true }))}
      ${field("Gift note", input("", { sm: true, ph: "optional" }))}
      ${field("Gift wrap", `<label class="row" style="gap:6px;height:26px">${chk(true)}<span class="small">Wrap each book</span></label>`)}
    </div>
    <div class="row">${primary("Submit", { size: "sm" })}${btn("Decline", { size: "sm" })}${ghost("Cancel", { size: "sm" })}</div></div></div>`;
}
function mcpView(elicit) {
  const rt = S.rt || "result";
  const result = `<div class="scroll grow" style="padding:12px;display:flex;flex-direction:column;gap:10px;min-height:0">
    <div class="eyebrow">content · text</div>
    <div class="well" style="padding:10px 12px;font-size:12.5px">Found 2 books for “rust”: Rust in Action (39.99, in stock) and Programming Rust (49.00, 3 left).</div>
    <div class="eyebrow">structuredContent</div>
    <div style="border:1px solid var(--border);border-radius:8px;background:var(--surface)">${jsonCode({ books: [{ id: 4, title: "Rust in Action", price: 39.99, inStock: true }, { id: 7, title: "Programming Rust", price: 49, stock: 3 }], total: 2 })}</div></div>`;
  return protoView({
    title: protoTitle("Tools server", pill("Connected · 4:02", "ok", { dot: true, live: true }) + pill("bookstore-tools 1.4.0", "", { outline: true, title: "protocol 2025-06-18" })),
    bar: pbar("MCP", pcombo("HTTP", { title: "Transport" }) + pfld(urlHtml("http://127.0.0.1:8931/mcp"))) + bigBtn("Disconnect", "PLUGS", "", "Disconnect"),
    ltabs: tabs("pt", S.pt || "tools", [["tools", "Tools", { count: 12 }], ["resources", "Resources", { count: 4 }], ["prompts", "Prompts", { count: 3 }], ["saved", "Saved", { count: 2 }], ["conn", "Connection"], ["scripts", "Scripts", { count: 1 }]]),
    left: mcpLeft(),
    status: `${pill("Success", "ok", { lg: true, icon: "CHECK_CIRCLE" })}<span class="rmeta">${mt("TIMER", "182 ms")}<span class="mt mono" style="font-size:11.5px">tools/call search_books</span></span>`,
    ractions: `${ibtn("COPY", "Copy result", { sm: true })}${paneActs()}`,
    rtabs: tabs("rt", rt, [["result", "Result"], ["traffic", "Traffic", { count: MCP_LOG.length }]], rt === "traffic" ? `${seg("logF", "all", [["all", "All"], ["out", "Sent"], ["in", "Received"]], { sm: true })}${ibtn("TRASH", "Clear the log", { sm: true })}` : ""),
    right: (elicit ? mcpElicit() : "") + (rt === "traffic" ? logPane(MCP_LOG, { 5: { name: "search_books", arguments: { query: "rust", limit: 5 } } }, empty("CHATS", "No traffic yet", "Connect to see the JSON-RPC messages exchanged with the server.")) : result),
  });
}

/* ================================================================== AI */
function bubble(role, text, o = {}) {
  const me = role === "user";
  return `<div class="bubrow ${me ? "me" : ""}"><span class="av ${me ? "me" : "ai"}">${I(me ? "USER" : "SPARKLE")}</span>
    <div class="bub ${o.tool ? "tool" : ""}" style="${o.style || ""}"><div class="bl">${o.label || (me ? "User" : "Assistant")}</div>${text}
    ${o.acts ? `<div class="bact">${ibtn("ARROWS_DOWN_UP", me ? "Make assistant" : "Make user", { sm: true })}${ibtn("TRASH", "Remove message", { sm: true })}</div>` : ""}</div></div>`;
}
function aiLeft() {
  switch (S.pt || "conv") {
    case "conv":
      return paneBody(`${card(`<div class="row" style="gap:6px;margin-bottom:6px">${I("GEAR", "faint")}<span class="section-title">System prompt</span></div>${textarea("You are the Bookstore assistant. Use the tools to look up real stock and prices. Answer in short paragraphs.", { rows: 2, style: "width:100%;border-color:transparent;background:var(--well)" })}`, { style: "padding:10px 12px" })}
        ${bubble("user", "What do you have for someone learning Rust?")}
        ${bubble("assistant", "I can check the catalogue. Do you prefer hands-on projects or a reference book?")}
        ${bubble("user", "Hands-on, and keep it under $45. Recommend two and say if they are in stock.", { acts: true })}
        <div class="row">${ghost("User message", { size: "sm", icon: "PLUS" })}${ghost("Assistant message", { size: "sm", icon: "PLUS" })}<span class="spacer"></span><span class="hint row" style="gap:4px">${kbd("⇧⌘↩")} to send</span></div>`, { pad: "12px 12px 16px", gap: 12 });
    case "settings":
      return toolbar(`<span class="section-title">Model settings</span>`) + paneBody(group("", [
        frow("Base URL", "", input("", { sm: true, mono: true, ph: "https://api.openai.com/v1", style: "width:220px" })),
        frow("Stream", "Stream the response as it is generated", chk(true)),
        frow(`<span class="row" style="gap:8px">${chk(true)}Temperature</span>`, "", `<span class="row" style="gap:10px"><span class="pslider"><span style="width:35%"></span><i style="left:35%"></i></span><span class="mono small">0.70</span></span>`),
        frow(`<span class="row" style="gap:8px">${chk(false)}Max tokens</span>`, "", '<span class="faint small">provider default</span>'),
      ]) + `<div class="col" style="gap:6px"><span class="section-title">Extra body</span>${hint("JSON merged into the provider request")}${editorWell(['<span class="p">{</span> <span class="k">"reasoning"</span><span class="p">:</span> <span class="p">{</span> <span class="k">"effort"</span><span class="p">:</span> <span class="s">"low"</span> <span class="p">}</span> <span class="p">}</span>'], { h: 80, margin: "0" })}</div>`, { pad: "0 12px 16px" });
    case "auth":
      return toolbar(`<span class="section-title">API key</span>`) + paneBody(hint("API key, sent as the provider expects. Keep it in the vault, e.g. <span class='var sec'>{{vault:openai}}</span>.")
        + field("API key", `<div class="row" style="position:relative">${input("{{vault:openai}}", { mono: true, sm: true, style: "width:100%" })}</div>`), { pad: "0 12px 16px" });
    case "mcp":
      return toolbar(`<span class="section-title">MCP tools</span>`) + paneBody(hint("Tools of the selected MCP requests are offered to the model; calls run through those servers.")
        + `<div class="tbl-wrap">${[["Tools server", "Realtime & AI", true], ["Docs search", "Internal", false]].map(([n, c, on]) => `<div class="lrow">${chk(on)}${I("PUZZLE_PIECE", "faint")}<span class="med">${n}</span><span class="faint xs grow">${c}</span></div>`).join("")}</div>`, { pad: "0 12px 16px" });
    case "scripts": return protoScripts([["before", "Before send", false], ["after", "After response", true]], 'pm.test("mentions a price", () => {\n  pm.expect(pm.response.text()).to.match(/\\$\\d+/);\n});');
  }
  return "";
}
function aiView() {
  const rt = S.rt || "answer";
  const answer = paneBody(`
    ${bubble("assistant", `<div class="mono" style="font-size:11.5px">search_books({ "query": "rust", "maxPrice": 45 })</div>${disclosure("Details", false, "")}`, { tool: true, label: `${I("PUZZLE_PIECE")} Tool call · search_books` })}
    ${bubble("assistant", `<div class="mono" style="font-size:11.5px">{"books":[{"id":4,"title":"Rust in Action","price":39.99,"inStock":true}, …2 more]}</div>${disclosure("Details", false, "")}`, { tool: true, label: `${I("CHECK_CIRCLE")} Tool result · search_books` })}
    ${bubble("assistant", `<p>Two hands-on picks under $45:</p><ol><li><b>Rust in Action</b> — Tim McNamara, $39.99, <span class="ok">in stock</span>. Builds real tools (a CPU emulator, a key-value store) chapter by chapter.</li><li><b>Hands-on Rust</b> — Herbert Wolverson, $42.00, <span class="warn">2 left</span>. Teaches the language by writing a small game.</li></ol><p>Both assume you know another language. Want me to place an order?</p>`, { style: "max-width:92%" })}
    <div class="row" style="padding-left:34px">${btn("Add to conversation", { size: "sm", icon: "PLUS", title: "Append the answer as an assistant message" })}${ibtn("COPY", "Copy answer", { sm: true })}</div>`, { pad: "12px 12px 16px", gap: 12 });
  const raw = `<div class="scroll grow" style="margin:12px;border:1px solid var(--border);border-radius:8px;background:var(--surface)">${jsonCode({ id: "resp_68f2a9c1", model: "gpt-5-mini", status: "completed", output: [{ type: "function_call", name: "search_books", arguments: '{"query":"rust","maxPrice":45}' }, { type: "message", role: "assistant", content: [{ type: "output_text", text: "Two hands-on picks under $45: …" }] }], usage: { input_tokens: 96, output_tokens: 412 } })}</div>`;
  return protoView({
    title: protoTitle("Assistant"),
    bar: pbar("AI", pcombo("OpenAI", { title: "Provider" }) + pfld("gpt-5-mini", { title: "Model" }) + `<div class="pcombo" title="Choose a model" style="padding:0 8px;border-left:1px solid var(--border)">${I("CARET_DOWN", "car")}</div>`) + bigBtn("Send", "PAPER_PLANE_RIGHT", "primary", "Send (⇧⌘↩)"),
    ltabs: tabs("pt", S.pt || "conv", [["conv", "Conversation", { count: 3 }], ["settings", "Settings"], ["auth", "Auth"], ["mcp", "MCP tools", { count: 1 }], ["scripts", "Scripts", { count: 1 }]]),
    left: aiLeft(),
    status: `${pill("Completed", "ok", { dot: true, lg: true })}<span class="rmeta">${mt("TIMER", "2.4 s", "First token 380 ms · Total 2.4 s")}<span class="mt" title="Time to first token">TTFT 380 ms</span></span>${pill("412 out", "", { title: "Output tokens" })}${pill("96 in", "", { title: "Input tokens" })}${pill("stop", "ok", { title: "Finish reason" })}`,
    ractions: paneActs(),
    rtabs: tabs("rt", rt, [["answer", "Answer"], ["raw", "Raw response"]]),
    right: rt === "raw" ? raw : answer,
  });
}

/* ================================================================== menus + screens */
const P_TAB = (id, m, t) => OPEN_TABS.find((x) => x.id === id) || { id, m, t, go: id };
const PN = (keputusan, egui) => N(keputusan) + NE(egui);

screen({ id: "ws", group: "Realtime & RPC", label: "WebSocket · terhubung + detail pesan", rail: "collections", sel: "ws", tab: P_TAB("ws", "WS", "Echo socket"), preset: { logSel: "3" }, view: wsView,
  notes: PN([
    "Komposer di kiri (URL + Disconnect, tab Message/Params/Headers/Auth/Saved/Scripts), log percakapan di kanan — dulu atas/bawah lewat <code>split_view</code>, kini kiri/kanan seperti HTTP.",
    "Status koneksi pindah dari header ke baris status panel kanan (56 px), sejajar dengan baris URL. Hover ‘101 · 12 ms’ = info handshake + subprotocol.",
    "Toolbar log (judul + jumlah, All/Sent/Received, cari, Clear ikon) sejajar dengan tab strip kiri. Klik baris → panel detail di bawah log (JSON rapi, Copy, Close/Esc).",
    "Baris error diberi tint merah; pesan biner diberi akhiran ‘· bin’. ‘Saved messages’ disingkat ‘Saved’ di tab (judul lengkap di dalam).",
  ], [
    "<code>ws_tab::show</code>: ganti <code>proto_common::split_view</code> (vertikal) dengan pembagi horizontal yang sama dengan <code>request_tab</code> (fraksi disimpan per tab).",
    "<code>Log::ui_with</code> tetap; toolbar log dipindah ke baris 36 px kolom kanan, detail pane tetap 50%→42% tinggi.",
    "<code>connect_bar</code> sama tinggi dengan URL bar HTTP (38 px, radius 9) dan tombol 38 px.",
  ]) });

screen({ id: "sio", group: "Realtime & RPC", label: "Socket.IO · ack tertunda + argumen", rail: "collections", sel: "sio", tab: P_TAB("sio", "SIO", "Chat events"), preset: { logSel: "5" }, view: sioView,
  notes: PN([
    "Kartu ‘The server is waiting for acknowledgements’ pindah ke atas panel kanan (di bawah toolbar log) karena terkait trafik masuk; baris toolbar tetap sejajar.",
    "Event + Expect ack + Emit dalam satu toolbar; argumen sebagai kartu bernomor dengan segmented JSON/Text/Binary/File.",
    "Pill transport (‘websocket’) tetap di judul; status koneksi + sid di baris status kanan.",
  ], [
    "<code>sio_tab::show</code>: <code>pending_acks_ui</code> dirender di kolom kanan sebelum <code>Log::ui_with</code>.",
    "Kartu argumen memakai <code>argument_editor</code> yang ada, dengan header 36 px ber-fill <code>well</code>.",
  ]) });

screen({ id: "mqtt", group: "Realtime & RPC", label: "MQTT · publish + langganan aktif", rail: "collections", sel: "mqtt", tab: P_TAB("mqtt", "MQTT", "Sensor stream"), preset: { logSel: "4" }, view: mqttView,
  notes: PN([
    "Tab Subscriptions tetap di kiri (konfigurasi), tetapi langganan yang <b>aktif</b> ditampilkan sebagai chip di atas log kanan, dengan × untuk unsubscribe dan ‘Add’ yang membuka tab Subscriptions.",
    "Publish: Topic satu baris penuh (topik sering panjang), baris kedua QoS + Retain + Publish; User properties (MQTT 5) di disclosure bawah editor.",
    "Meta pesan (‘QoS 1 · retained’, ‘2 props’) tampil sebelum preview di log.",
  ], [
    "<code>mqtt_tab::show</code>: chip dari <code>subscriptions.iter().filter(active)</code>, × memanggil unsubscribe yang sama dengan ghost ‘Unsubscribe’.",
    "Tab Connection memakai <code>dialogs::group</code> + <code>Rows::row</code> agar sama dengan Settings.",
  ]) });

screen({ id: "grpc", group: "Realtime & RPC", label: "gRPC · unary + respons", rail: "collections", sel: "grpc", tab: P_TAB("grpc", "gRPC", "Greeter"), preset: { logSel: "2" }, view: () => grpcView(false),
  notes: PN([
    "Bar koneksi: URL | pemilih method (Service / Method) | Invoke — sama seperti method+URL+Send di HTTP.",
    "Pill status skema (‘6 methods’, tooltip sumber skema) tetap di judul; status panggilan (‘0 OK’, waktu, jumlah pesan, test) di baris status kanan.",
    "Respons: tab Messages / Headers / Trailers di kanan dengan cari + clear di ujung tab strip.",
  ], [
    "<code>grpc_tab::show</code>: panel respons (<code>:671</code>) menjadi kolom kanan; <code>method_picker</code> tetap di <code>connect_bar</code>.",
  ]) });

screen({ id: "grpc-picker", group: "Realtime & RPC", label: "gRPC · pemilih method terbuka", rail: "collections", sel: "grpc", tab: P_TAB("grpc", "gRPC", "Greeter"), preset: { logSel: "2" }, view: () => grpcView(true),
  notes: PN([
    "Popover 380 px: cari (fokus otomatis), dikelompokkan per service, tiap baris nama + signature mono + pill jenis (unary / server stream / client stream / bidi stream) dengan warna konsisten.",
    "Footer ‘Reload schema’ + sumber skema.",
  ], ["<code>grpc_tab::method_picker</code> (<code>:252</code>) — tampilan sama, baris 42 px, aksen kiri untuk method terpilih."]) });

screen({ id: "mcp", group: "Realtime & RPC", label: "MCP · tools + hasil", rail: "collections", sel: "mcp", tab: P_TAB("mcp", "MCP", "Tools server"), view: () => mcpView(false),
  notes: PN([
    "Kiri = browser kapabilitas: daftar (filter + kartu) dan form argumen untuk item terpilih. Kanan = hasil panggilan terakhir + tab Traffic (log JSON-RPC, dulu tab terpisah di kiri).",
    "Baris status kanan: Success/Error, durasi, method JSON-RPC, Copy. Status koneksi + versi server di judul.",
    "Tab Connection berisi kartu server (kapabilitas sebagai pill), transport, headers, auth, Export config.",
  ], [
    "<code>mcp_tab::show</code>: <code>list_panel</code> + <code>call_form</code> di kolom kiri; area hasil di <code>call_form</code> dipindah ke kolom kanan bersama <code>Log</code> (Traffic).",
  ]) });

screen({ id: "mcp-elicit", group: "Realtime & RPC", label: "MCP · server meminta input", rail: "collections", sel: "mcp", tab: P_TAB("mcp", "MCP", "Tools server"), view: () => mcpView(true),
  notes: PN([
    "Permintaan server (elicitation/sampling) muncul sebagai kartu aksen di atas panel kanan, tidak menutupi form kiri; ‘1 more request waiting’ untuk antrean.",
    "Form dibuat dari skema: wajib diberi *, tipe boolean = checkbox, enum = dropdown.",
  ], ["<code>mcp_tab::pending_ui</code> (<code>:1269</code>) memakai <code>notice_card</code>, dirender di kolom kanan."]) });

screen({ id: "ai", group: "Realtime & RPC", label: "AI · percakapan + jawaban", rail: "collections", sel: "ai", tab: P_TAB("ai", "AI", "Assistant"), view: aiView,
  notes: PN([
    "Kiri = percakapan yang akan dikirim (system prompt, gelembung user/assistant yang bisa diedit/ditukar/dihapus). Kanan = jawaban model (tool call, tool result, jawaban streaming) — dulu satu kolom bergulir.",
    "Bar: provider | model | pilih model | Send. Statistik (Completed, waktu, TTFT, token in/out, finish reason, model) di baris status kanan.",
    "‘Add to conversation’ memindahkan jawaban ke kiri sebagai pesan assistant. Raw response sebagai tab kanan (dulu segmented Chat/Raw).",
  ], [
    "<code>ai_tab::conversation</code> dipecah: <code>bubble</code> pesan input di kiri, jawaban + tool bubbles di kanan; <code>split</code> sama dengan HTTP.",
  ]) });
