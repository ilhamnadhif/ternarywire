// Flows canvas and Collection Runner (functional + performance).

document.head.insertAdjacentHTML("beforeend", `<style id="css-flow">
.fwrap { position: relative; flex: 1; display: flex; min-height: 0; }
.fcanvas { position: relative; flex: 1; min-height: 0; overflow: hidden; }
.fworld { position: absolute; left: 12px; top: 64px; width: 960px; height: 540px; transform-origin: 0 0; }
.fworld svg { position: absolute; inset: 0; overflow: visible; pointer-events: none; }
.fn { position: absolute; width: 172px; border-radius: 10px; background: var(--surface); border: 1px solid var(--border-strong);
  box-shadow: 0 2px 8px rgba(20,20,30,.07); font-size: 11.5px; }
.theme-dark .fn { box-shadow: 0 4px 14px rgba(0,0,0,.35); }
.fn.sel { border-color: var(--accent); box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 20%, transparent), 0 6px 16px rgba(20,20,30,.1); }
.fn.warnb { border-color: var(--warn); }
.fn .fh { height: 30px; display: flex; align-items: center; gap: 6px; padding: 0 6px 0 7px; border-bottom: 1px solid var(--border);
  border-radius: 9px 9px 0 0; background: color-mix(in srgb, var(--kc) 10%, var(--surface)); font-weight: 600; }
.fn .ft { width: 20px; height: 20px; border-radius: 6px; display: grid; place-items: center; color: #fff; background: var(--kc); font-size: 12px; flex: none; }
.fn .fname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.fn .fb { display: inline-flex; align-items: center; gap: 3px; height: 17px; padding: 0 5px; border-radius: 9px; font-size: 10px; font-weight: 500;
  background: var(--ok-soft); color: var(--ok); white-space: nowrap; flex: none; }
.fn .fb.w { background: var(--warn-soft); color: var(--warn); }
.fn .fs { height: 24px; display: flex; align-items: center; padding: 0 8px; font-family: "JBMono", monospace; font-size: 10.5px; color: var(--text-muted);
  white-space: nowrap; overflow: hidden; border-bottom: 1px solid var(--border); }
.fn .fs span { overflow: hidden; text-overflow: ellipsis; }
.fn .fp { padding: 4px 0; }
.fn .pr { position: relative; height: 20px; display: flex; align-items: center; justify-content: space-between; padding: 0 11px; color: var(--text-muted); font-size: 10.5px; }
.fn .pd { position: absolute; top: 5px; width: 10px; height: 10px; border-radius: 50%; background: var(--surface); box-shadow: 0 0 0 1.5px var(--pc); }
.fn .pd.on { background: var(--pc); box-shadow: 0 0 0 2px var(--surface), 0 0 0 3px var(--pc); }
.fn .pd.l { left: -6px; } .fn .pd.r { right: -6px; }
.fzoom { position: absolute; right: 14px; bottom: 14px; display: flex; align-items: center; gap: 2px; padding: 3px; border-radius: 9px;
  background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-pop); }
.fzoom .z { min-width: 40px; text-align: center; font-size: 11.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
.insp { width: 324px; flex: none; border-left: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; background: var(--canvas); }
.insp .ih { display: flex; align-items: center; gap: 10px; padding: 14px 14px 12px 16px; }
.insp .itile { width: 36px; height: 36px; border-radius: 9px; display: grid; place-items: center; font-size: 18px; flex: none;
  background: color-mix(in srgb, var(--kc) 14%, transparent); color: var(--kc); }
.insp .sec { padding: 14px 16px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 10px; }
.insp .sec h4 { margin: 0; font-size: 12.5px; font-weight: 600; }
.reftag { display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 4px 0 9px; border-radius: 7px; background: var(--well); border: 1px solid var(--border);
  font-family: "JBMono", monospace; font-size: 11.5px; }
.flog { height: 196px; flex: none; display: flex; flex-direction: column; border-top: 1px solid var(--border); }
.flog .lb { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; justify-content: flex-end; padding: 4px 0 6px; -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 30px); mask-image: linear-gradient(to bottom, transparent 0, #000 30px); }
.flog .lr2 { display: flex; align-items: center; gap: 9px; height: 22px; padding: 0 14px; font-family: "JBMono", monospace; font-size: 11.5px; white-space: nowrap; }
.flog .lr2 .t { color: var(--text-faint); width: 72px; flex: none; }
.flog .lr2 .chipb { font-family: "Inter"; font-size: 11px; font-weight: 500; padding: 0 6px; height: 18px; display: inline-flex; align-items: center; border-radius: 5px;
  background: var(--subtle); color: var(--text); flex: none; }
.flog .lr2 .tx { overflow: hidden; text-overflow: ellipsis; }
.outchip { display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px; border-radius: 6px; background: var(--well); border: 1px solid var(--border);
  font-family: "JBMono", monospace; font-size: 11px; max-width: 260px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.addmenu .ar { display: flex; align-items: center; gap: 10px; padding: 5px 8px; border-radius: 7px; }
.addmenu .ar:hover { background: var(--hover); }
.addmenu .at { width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; color: #fff; font-size: 14px; flex: none; }
.addmenu .ad { font-size: 11px; color: var(--text-muted); line-height: 1.25; }
.rset { width: 336px; flex: none; border-right: 1px solid var(--border); display: flex; flex-direction: column; min-height: 0; }
.rorder .ro { display: flex; align-items: center; gap: 8px; height: 24px; padding: 0 12px 0 6px; border-radius: 6px; }
.rorder .ro:hover { background: var(--hover); }
.rorder .ro .h { color: var(--text-faint); font-size: 13px; cursor: grab; }
.rorder .ro .meth { width: 34px; }
.rorder .ro.off .nm { color: var(--text-faint); }
.rng { position: relative; display: inline-flex; align-items: center; width: 84px; height: 18px; }
.rng .rt { width: 100%; height: 4px; border-radius: 2px; background: var(--subtle); }
.rng .rk { position: absolute; left: -2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; border: 1px solid var(--border-strong); box-shadow: 0 1px 2px rgba(0,0,0,.2); }
.rset .frow { padding: 7px 0; }
.filechip { display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 4px 0 10px; border-radius: 8px; border: 1px solid var(--border); background: var(--well); }
.ptbl td, .ptbl th { padding: 0 8px !important; }
.pctl { display: grid; grid-template-columns: 34px 1fr 62px; gap: 10px 10px; align-items: center; font-size: 12px; }
.pctl .pb { height: 8px; border-radius: 4px; background: var(--subtle); overflow: hidden; }
.pctl .pb span { display: block; height: 100%; border-radius: 4px; }
</style>`);

/* ------------------------------------------------------------------ flows */
const FK = {
  start: ["Start", "PLAY", "--ok", "Where the flow begins"],
  request: ["Request", "GLOBE", "--accent", "Sends a saved or inline request"],
  condition: ["Condition", "GIT_BRANCH", "--warn", "Continues on true or false"],
  loop: ["Loop", "ARROWS_CLOCKWISE", "--m-patch", "Repeats for each item or a number of times"],
  parallel: ["Parallel", "GIT_FORK", "--m-ws", "Runs branches at the same time"],
  join: ["Join", "GIT_MERGE", "--m-ws", "Waits for every parallel branch"],
  script: ["Script", "CODE", "--m-put", "Runs JavaScript"],
  ai: ["AI", "SPARKLE", "--m-gql", "Asks an AI model"],
  delay: ["Delay", "HOURGLASS", "--text-muted", "Waits before continuing"],
  setvar: ["Set variable", "BRACKETS_CURLY", "--m-head", "Stores a value in a variable"],
  template: ["Template", "FILE_TEXT", "--m-options", "Builds text or JSON from a template"],
  output: ["Output", "EXPORT", "--ok", "Names a result of the flow"],
  log: ["Log", "NOTE", "--text-muted", "Writes a message to the run log"],
};
const PORT_C = { success: "--ok", true: "--ok", failure: "--err", false: "--err", item: "--info", break: "--warn", branches: "--m-ws" };
const portColor = (p) => PORT_C[p] || "--text-faint";

const FNODES = [
  { id: "start", k: "start", name: "Start", x: 14, y: 200, ins: [], outs: ["next"], sum: "inputs: user, aiModel", run: "0 ms" },
  { id: "par", k: "parallel", name: "In parallel", x: 204, y: 200, ins: ["in"], outs: ["branches"], sum: "3 branches at once", run: "0 ms" },
  { id: "books", k: "request", name: "Books", x: 394, y: 30, ins: ["in"], outs: ["success", "failure"], sum: "Bookstore API / List books", run: "78 ms · 3/3" },
  { id: "user", k: "request", name: "User", x: 394, y: 200, ins: ["in"], outs: ["success", "failure"], sum: "Bookstore API / Current user", run: "80 ms · 2/2" },
  { id: "book", k: "request", name: "Book", x: 394, y: 370, ins: ["in"], outs: ["success", "failure"], sum: "Bookstore API / Get book", run: "81 ms · 2/2" },
  { id: "join", k: "join", name: "Join", x: 584, y: 200, ins: ["in"], outs: ["next"], sum: "waits for 3 incoming", run: "0 ms" },
  { id: "sum", k: "ai", name: "Summarize", x: 774, y: 200, ins: ["in"], outs: ["success", "failure"], sum: "{{aiModel}} · Summarize the catalog", run: "909 ms" },
];
const FEDGES = [
  ["start", "next", "par", "in"], ["par", "branches", "books", "in"], ["par", "branches", "user", "in"], ["par", "branches", "book", "in"],
  ["books", "success", "join", "in"], ["user", "success", "join", "in"], ["book", "success", "join", "in"], ["join", "next", "sum", "in"],
];
const fnodeById = (id) => FNODES.find((n) => n.id === id);
// Port centre in world coordinates: border 1 + header 30 + summary 24 + padding 4 + row centre 10.
function portXY(n, side, port) {
  const i = (side === "in" ? n.ins : n.outs).indexOf(port);
  return [side === "in" ? n.x : n.x + 172, n.y + 69 + 20 * i];
}

function fnode(n, o) {
  const [, icon, kc] = FK[n.k];
  const rows = Math.max(n.ins.length, n.outs.length, 1);
  const used = (side, p) => FEDGES.some((e) => (side === "in" ? e[2] === n.id && e[3] === p : e[0] === n.id && e[1] === p));
  const dot = (side, p) => `<i class="pd ${side === "in" ? "l" : "r"} ${used(side, p) ? "on" : ""}" style="--pc:var(${portColor(p)})"></i>`;
  const pr = Array.from({ length: rows }, (_, i) => {
    const a = n.ins[i], b = n.outs[i];
    return `<div class="pr"><span>${a ? dot("in", a) + a : ""}</span><span>${b ? b + dot("out", b) : ""}</span></div>`;
  }).join("");
  let badge = "";
  if (o.warn === n.id) badge = `<span class="fb w" title="Uses {{aiModel}}, which is not defined in Local">${I("WARNING")}</span>`;
  else if (o.run) badge = `<span class="fb">${I("CHECK")}${n.run}</span>`;
  return `<div class="fn ${o.sel === n.id ? "sel" : ""} ${o.warn === n.id ? "warnb" : ""}" style="left:${n.x}px;top:${n.y}px;--kc:var(${kc})">
    <div class="fh"><span class="ft">${I(icon)}</span><span class="fname">${n.name}</span>${badge}</div>
    <div class="fs"><span>${vars(n.sum)}</span></div><div class="fp">${pr}</div></div>`;
}

function fcanvas(o) {
  const z = 0.78;
  const edges = FEDGES.map(([a, ap, b, bp]) => {
    const [x1, y1] = portXY(fnodeById(a), "out", ap);
    const [x2, y2] = portXY(fnodeById(b), "in", bp);
    const dx = Math.max(36, (x2 - x1) * 0.5);
    const col = o.run ? "var(--ok)" : "var(--text-faint)";
    return `<path d="M${x1} ${y1} C${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}" style="fill:none;stroke:${col};stroke-width:${o.run ? 2.2 : 1.6};${o.run ? "opacity:.85" : ""}"/>`;
  }).join("");
  return `<div class="fcanvas canvas-bg" aria-label="Flow canvas">
    <div class="fworld" style="transform:scale(${z})"><svg width="960" height="540">${edges}</svg>${FNODES.map((n) => fnode(n, o)).join("")}</div>
    <div class="fzoom">${ibtn("MINUS", "Zoom out", { sm: true })}<span class="z" title="Zoom level (click for 100 %)">78%</span>${ibtn("PLUS", "Zoom in", { sm: true })}<div class="vdiv" style="height:14px;margin:0 2px"></div>${ibtn("ARROWS_OUT", "Fit — show all blocks", { sm: true })}</div>
  </div>`;
}

function flowTitle(o) {
  return `<div class="row" style="height:46px;padding:0 12px 0 16px;border-bottom:1px solid var(--border);flex:none">
    <div class="crumbs grow">${I("FLOW_ARROW", "accent")}<span>Flows</span><span class="sep">${I("CARET_RIGHT")}</span><span class="cur" title="Double-click to rename">Catalog summary</span>
      ${pill("Saved", "ok", { icon: "CHECK", title: "flows/catalog-summary.flow.json" })}</div>
    <span class="pill outline" title="Zoom level (click for 100 %)">78%</span>
    ${ibtn("SIDEBAR", "Inspector", { sm: true, on: true })}
    <div class="vdiv"></div>
    ${btn("Fit", { size: "sm", kind: "ghost", icon: "ARROWS_OUT", title: "Show all blocks" })}
    ${btn("Validate", { size: "sm", icon: "SHIELD_CHECK", title: "Check the flow for problems" })}
    ${btn("Add block", { size: "sm", icon: "PLUS", caret: true, menu: "add-block" })}
    ${o.running ? btn("Stop", { size: "sm", kind: "danger", icon: "STOP" }) : primary("Run", { size: "sm", icon: "PLAY", title: "Run the flow (⌘↩)" })}
  </div>`;
}

function runLog(o) {
  const lines = [
    ["00:00.000", "CHECK", "ok", "Start", "→ next · 0 ms"],
    ["00:00.001", "CHECK", "ok", "In parallel", "→ branches · 0 ms"],
    ["00:00.077", "BUG", "faint", "Books", "GET http://127.0.0.1:18795/books → 200 OK (1 ms, 956 B)"],
    ["00:00.078", "CHECK", "ok", "Books", "→ success · 78 ms · 3/3 tests"],
    ["00:00.080", "CHECK", "ok", "User", "→ success · 80 ms · 2/2 tests"],
    ["00:00.081", "CHECK", "ok", "Book", "→ success · 81 ms · 2/2 tests"],
    ["00:00.082", "CHECK", "ok", "Join", "→ next · 0 ms"],
    ["00:00.991", "INFO", "info", "Summarize", "Ilham, the catalogue has 5 books and 4 are in stock. Laskar Pelangi is the cheapest at 12.00."],
    ["00:00.992", "CHECK", "ok", "Summarize", "→ success · 909 ms"],
    ["00:00.992", "CHECK_CIRCLE", "ok", "", "<span style='font-family:Inter;font-weight:600'>Succeeded · 7 steps · 992 ms</span>"],
  ];
  const head = o.run
    ? `${pill("Succeeded", "ok", { icon: "CHECK_CIRCLE" })}<span class="hint">7 steps · 992 ms</span>${pill("7/7 tests", "ok")}`
    : `<span class="hint">Run the flow to see each step, its timing and its messages here.</span>`;
  return `<div class="flog">
    <div class="row" style="height:36px;padding:0 10px 0 14px;border-bottom:1px solid var(--border);flex:none">
      <span class="section-title row" style="gap:6px">${I("TERMINAL_WINDOW", "muted")}Run log</span>${head}<span class="spacer"></span>
      ${o.run ? `<span class="eyebrow">Outputs</span><span class="outchip" title="Ilham, the catalogue has 5 books and 4 are in stock…"><span class="k">summary</span> = "Ilham, the catalogue has 5 b…"</span>` : ""}
      ${ibtn("BROOM", "Clear the log and block states", { sm: true })}
    </div>
    ${o.run ? `<div class="lb">${lines.map(([t, ic, c, b, tx]) => `<div class="lr2"><span class="t">${t}</span>${I(ic, c)}${b ? `<span class="chipb">${b}</span>` : ""}<span class="tx ${ic === "BUG" ? "faint" : ""}">${tx}</span></div>`).join("")}</div>`
      : `<div class="grow col" style="justify-content:center">${empty("TERMINAL_WINDOW", "No run yet", "", `${kbd("⌘")}${kbd("↩")}<span class="hint">runs the flow</span>`)}</div>`}
  </div>`;
}

function inspHead(icon, kc, eb, title, actions) {
  return `<div class="ih" style="--kc:var(${kc})"><div class="itile">${I(icon)}</div><div class="col grow" style="gap:1px">${eyebrow(eb)}<div class="semi ellipsis" style="font-size:14px">${title}</div></div>${actions}</div>`;
}

function blockInspector() {
  const out = { status: 200, time: 78, body: { data: "[4 items]", page: 1, total: 5 }, tests: { passed: 3, total: 3 } };
  return `<aside class="insp">
    <div class="scroll grow">
      ${inspHead("GLOBE", "--accent", "Request", "Books", ibtn("COPY", "Duplicate (⌘D)", { sm: true }) + ibtn("TRASH", "Delete block", { sm: true }) + ibtn("X", "Close", { sm: true }))}
      <div style="padding:0 16px 14px">${banner("ok", "CHECK_CIRCLE", "<b>Succeeded</b> → success · 78 ms · 3/3 tests")}</div>
      <div class="sec"><h4>Block</h4>
        ${field("Label", input("Books", { sm: true, ph: "Request" }))}
        ${field("Reference", `<div class="reftag"><span class="grow ellipsis"><span class="var">{{blocks.Books.output}}</span></span>${btn("Copy", { size: "sm", kind: "ghost", icon: "COPY" })}</div>`, "Later blocks read this block's output with this reference.")}
      </div>
      <div class="sec"><h4>Request</h4>
        ${seg("rsrc", "saved", [["saved", "Saved request"], ["inline", "Inline request"]], { sm: true })}
        ${field("Request", `<span class="selectbox sm" style="width:100%">${meth("GET")}<span class="grow ellipsis">Bookstore API / List books</span>${I("CARET_DOWN", "car")}</span>`, "Runs the saved request with its auth, scripts and tests.")}
        ${field("Success status", input("", { sm: true, mono: true, ph: "2xx" }), "Statuses that leave through <b>success</b>; any other status leaves through <b>failure</b>.")}
      </div>
      <div class="sec"><div class="row"><h4 class="grow">Output</h4>${ibtn("COPY", "Copy", { sm: true })}</div>
        ${hint("What the block produced in the last run.")}
        <div class="well" style="overflow:hidden">${jsonCode(out, { gutter: false })}</div>
      </div>
    </div></aside>`;
}

function flowSettingsInspector() {
  const inputCard = (name, def, req) => `<div class="card" style="padding:10px 12px;display:flex;flex-direction:column;gap:8px">
    <div class="row">${input(name, { sm: true, mono: true, style: "flex:1;width:0" })}<label class="row" style="gap:6px;font-size:12px">${chk(req)}Required</label>${ibtn("TRASH", "Remove input", { sm: true })}</div>
    <div style="display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:8px">${field("Default", input(def, { sm: true, mono: true, ph: "JSON value", style: "width:100%" }))}${field("Next run", input("", { sm: true, mono: true, ph: "next run only", style: "width:100%" }))}</div></div>`;
  const keys = [["⌫", "Delete the selection"], ["⌘D", "Duplicate the block"], ["← ↑ → ↓", "Move the block (⇧ for bigger steps)"], ["Esc", "Clear the selection"], ["⌘ + scroll", "Zoom around the pointer"]];
  return `<aside class="insp"><div class="scroll grow">
    ${inspHead("FLOW_ARROW", "--accent", "7 blocks · 8 connections", "Flow settings", "")}
    <div class="sec"><h4>Description</h4>${textarea("Fetches the books, the signed-in user and one book at the same time, then asks a local model for a short summary.", { rows: 3, ph: "What the flow does and what it needs" })}</div>
    <div class="sec"><h4>Inputs</h4>${hint("Values the flow starts with; blocks read them as <span class='var'>{{inputs.name}}</span>.")}
      ${inputCard("user", '"Ilham"', true)}${inputCard("aiModel", '"llama3.2"', false)}
      <div>${btn("Add input", { size: "sm", kind: "ghost", icon: "PLUS" })}</div></div>
    <div class="sec"><h4>Run settings</h4>
      <div class="row"><div class="grow"><div class="med">Limit steps</div><div class="hint">Stops a run after this many block runs (protects against endless loops).</div></div>${chk(true)}${input("1000", { sm: true, style: "width:72px;text-align:right" })}</div>
      <div class="row"><div class="grow"><div class="med">Parallel branches at once</div><div class="hint">Branches of a Parallel block that run at the same time; 0 runs them all at once.</div></div>${input("0", { sm: true, style: "width:72px;text-align:right" })}</div>
      <label class="row" style="align-items:flex-start">${chk(false)}<span>Continue after a failed block without a failure connection</span></label>
    </div>
    <div class="sec"><div class="row"><h4>Problems</h4>${pill("1", "warn")}</div>${hint("Click a problem to select its block.")}
      <div class="row" style="align-items:flex-start;padding:8px 10px;border-radius:8px;background:var(--warn-soft);cursor:pointer">${I("WARNING", "warn")}<div><b>Summarize</b> <span class="muted">uses <span class="var bad">{{aiModel}}</span>, which is not defined in Local.</span></div></div>
    </div>
    <div class="sec"><h4>Keyboard</h4>${keys.map(([k, d]) => `<div class="row" style="height:24px">${kbd(k)}<span class="muted small">${d}</span></div>`).join("")}</div>
  </div></aside>`;
}

function flowView(o) {
  return flowTitle(o) + `<div class="fwrap"><div class="col grow" style="gap:0;min-width:0">${fcanvas(o)}${runLog(o)}</div>${o.sel ? blockInspector() : flowSettingsInspector()}</div>`;
}

MENUS["add-block"] = () => {
  const groups = [["Actions", ["request", "script", "ai"]], ["Logic", ["condition", "loop", "parallel", "join", "delay"]], ["Data", ["setvar", "template", "output", "log"]]];
  const html = `<div class="menu" style="width:268px;padding:6px"><div class="addmenu">${groups.map(([g, ks]) => `<div class="mh">${g}</div>` + ks.map((k) => {
    const [label, icon, c, d] = FK[k];
    return `<div class="ar" title="${d}"><span class="at" style="background:var(${c})">${I(icon)}</span><div class="col" style="gap:0"><span class="med">${label}</span><span class="ad">${d}</span></div></div>`;
  }).join("")).join("")}</div></div>`;
  return placeMenu(html, { right: true });
};

/* ------------------------------------------------------------------ runner */
const RUN_REQS = [
  ["POST", "Login", "Auth"], ["GET", "Current user", "Auth"], ["GET", "List books", "Books"], ["GET", "Get book", "Books"], ["POST", "Create book", "Books"],
  ["PUT", "Update book", "Books"], ["DEL", "Delete book", "Books"], ["GQL", "Search catalog", "Books"], ["POST", "Place order", "Orders"], ["PATCH", "Order status", "Orders"],
];

function runnerTitle(mode) {
  return `<div class="row" style="height:46px;padding:0 12px 0 14px;border-bottom:1px solid var(--border);flex:none;gap:10px">
    <span style="width:28px;height:28px;border-radius:8px;display:grid;place-items:center;background:var(--accent-soft);color:var(--accent-text);font-size:16px">${I("PLAY_CIRCLE")}</span>
    <span class="semi" style="font-size:14px">Collection Runner</span><span class="faint">·</span><span class="muted">Bookstore API</span>
    <span class="spacer"></span>${seg("rmode", mode, [["functional", "Functional", "LIST_CHECKS"], ["perf", "Performance", "GAUGE"]], { sm: true })}
  </div>`.replace(`data-set="rmode=functional"`, `data-go="runner"`).replace(`data-set="rmode=perf"`, `data-go="runner-perf"`);
}

function runOrder(selected) {
  const n = selected.length;
  return toolbar(`<span class="section-title">Run order</span>${pill(`${n}/10`, n ? "" : "err")}<span title="Drag the handles to change the order; right-click a request for more.">${I("INFO", "faint")}</span>`, ghost("All", { size: "sm", title: "Select all" }) + ghost("None", { size: "sm", title: "Deselect all" }))
    + `<div class="rorder" style="padding:0 6px">${RUN_REQS.map(([m, name, f], i) => {
      const on = selected.includes(i);
      return `<div class="ro ${on ? "" : "off"}" title="Right-click: Move up · Move down · Run only this"><span class="h">${I("DOTS_SIX_VERTICAL")}</span>${chk(on)}${meth(m)}<span class="nm grow ellipsis">${name}</span><span class="faint xs">${f}</span></div>`;
    }).join("")}</div>`;
}

const numUnit = (v, unit, w = 64) => `<div class="row" style="gap:6px">${input(v, { sm: true, style: `width:${w}px;text-align:right` })}${unit ? `<span class="faint small">${unit}</span>` : ""}</div>`;

function runnerSetup(mode) {
  let opts;
  if (mode === "functional") {
    opts = `<div class="row" style="height:30px;padding:0 12px"><span class="section-title">Options</span></div>
      <div style="padding:0 12px">
        <div class="frow" style="display:grid;grid-template-columns:1fr 1fr;gap:12px;align-items:start">
          ${field("Iterations", numUnit("12", "", 80), "Times to run the requests.")}
          ${field("Delay", numUnit("0", "ms", 80), "Pause between requests.")}</div>
        ${frow("Data file", "CSV or JSON; one iteration per row.", btn("Change…", { size: "sm", icon: "FILE_ARROW_UP" }))}
        <div class="filechip" title="~/Developer/bookstore/data/books.csv" style="margin-bottom:2px">${I("FILE_TEXT", "muted")}<span class="med grow ellipsis">books.csv</span><span class="faint xs">12 rows</span>${btn("Preview data", { size: "sm", kind: "ghost", icon: "TABLE", title: "Show the first 10 rows" })}${ibtn("X", "Remove data file", { sm: true })}</div>
        ${frow("Keep variable values", "Keep what scripts set, after the run.", sw(true))}
        ${frow("Save cookies", "Off runs against a copy of the cookie jar.", sw(true))}
        ${frow("Stop at the first failure", "", sw(false))}
        ${frow("Simulate network failures", "Requests that fail on purpose.", `<div class="row" style="gap:8px"><span class="rng"><span class="rt"></span><span class="rk"></span></span><span class="mono small muted" style="width:26px;text-align:right">0%</span></div>`)}
      </div>`;
  } else {
    opts = `<div class="row" style="height:36px;padding:0 12px"><span class="section-title">Load profile</span></div>
      <div style="padding:0 12px">
        ${frow("Virtual users", "Users running at the same time (1–1000).", numUnit("25", ""))}
        ${frow("Duration", "", numUnit("30", "s"))}
        ${frow("Ramp-up", "Time to reach all users.", numUnit("5", "s"))}
        <div class="hint" style="padding:6px 0 10px">Each virtual user loops over the selected requests with its own copy of the variables.</div>
      </div>`;
  }
  const sel = mode === "functional" ? [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] : [2, 3, 4, 7];
  const foot = mode === "functional" ? "10 requests × 12 iterations" : "25 users · 30 s";
  return `<section class="rset">
    <div class="scroll grow">${runOrder(sel)}<div class="divider" style="margin:4px 0 0"></div>${opts}</div>
    <div class="row" style="height:56px;flex:none;padding:0 12px;border-top:1px solid var(--border);background:var(--well)"><span class="hint grow">${foot}</span>
      ${primary(mode === "functional" ? "Run" : "Start load test", { icon: "PLAY" })}</div>
  </section>`;
}

function functionalResults() {
  const rows = [
    [3, "POST", "Auth", "Login", "✓ 2", 200, "OK", "1.1 ms"],
    [3, "GET", "Auth", "Current user", "✓ 2", 200, "OK", "0.6 ms"],
    [3, "GET", "Books", "List books", "✓ 3", 200, "OK", "0.6 ms"],
    [3, "GET", "Books", "Get book", "✗ 1/2", 404, "Not Found", "0.6 ms"],
    [3, "POST", "Books", "Create book", "✓ 2", 201, "Created", "0.7 ms"],
  ];
  const kindOf = (c) => (c < 300 ? "ok" : c < 400 ? "info" : c < 500 ? "warn" : "err");
  const tbl = table([{ label: "Request" }, { label: "Tests", w: "84px" }, { label: "Status", w: "92px" }, { label: "Time", w: "78px", num: true }],
    rows.map(([it, m, f, n, t, c, , ms]) => [
      `<span class="row" style="gap:8px"><span class="faint mono" style="font-size:11px;width:22px">#${it}</span><span style="width:34px">${meth(m)}</span><span class="muted">${f} /</span><b class="med">${n}</b></span>`,
      `<span class="${t.startsWith("✗") ? "err" : "ok"} med">${t}</span>`,
      pill(String(c), kindOf(c)),
      `<span class="mono">${ms}</span>`,
    ]), { sel: 3 });
  const detail = `<div class="card" style="overflow:hidden">
    <div class="row" style="height:44px;padding:0 10px 0 14px;border-bottom:1px solid var(--border)">${meth("GET")}<span class="semi">Get book</span><span class="faint xs">iteration 3</span>${statusPill(404, "Not Found")}
      <span class="rmeta"><span class="mt">${I("TIMER")}0.6 ms</span><span class="mt">${I("ARROW_LINE_DOWN")}98 B</span></span><span class="spacer"></span><span class="faint xs">↑↓ move · Esc closes</span>${ibtn("X", "Close details", { sm: true })}</div>
    <div class="col" style="padding:12px 14px;gap:10px">
      <div class="mono small" style="padding:6px 10px;border-radius:7px;background:var(--well);border:1px solid var(--border)">http://127.0.0.1:18795/books/9</div>
      <div class="tbl-wrap">
        <div class="row" style="min-height:36px;padding:6px 12px;border-bottom:1px solid var(--border);align-items:flex-start">${I("X_CIRCLE", "err")}<div class="grow"><div>Book exists</div><div class="mono err" style="font-size:11.5px">AssertionError: expected response to have status code 200 but got 404</div></div>${pill("FAIL", "err")}</div>
        <div class="row" style="height:36px;padding:0 12px">${I("CHECK_CIRCLE", "ok")}<span class="grow">Response time is below 500 ms</span>${pill("PASS", "ok")}</div>
      </div>
      ${disclosure("Headers", false, "", { count: 5 })}
      ${disclosure("Body", true, `<div class="well">${jsonCode({ error: "not_found", message: "No book with id 9" }, { gutter: false })}</div>`)}
    </div></div>`;
  return toolbar(`${pill("Finished", "", { icon: "CHECK" })}<span class="hint">12 iterations · 4.1 s · started 09:52</span>`, btn("Export", { size: "sm", icon: "EXPORT", caret: true, menu: "run-export" }))
    + paneBody(`<div style="display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:10px">
        ${stat("Requests", "120", "requests")}${stat("Passed", "239", "tests", { color: "ok" })}${stat("Failed", "1", "tests and scripts", { color: "err" })}
        ${stat("Errors", "0", "network")}${stat("Duration", "4.1 s", "total")}${stat("Avg response", "0.9 ms", "per request")}</div>
      ${banner("err", "WARNING_CIRCLE", "<b>1 failure in 120 requests.</b> <span class='muted'>Get book (iteration 3) answered 404.</span>")}
      <div class="row">${seg("rf", "all", [["all", "All 120"], ["pass", "Passed 119"], ["fail", "Failed 1"]], { sm: true })}<span class="spacer"></span><span class="hint">Click a row for details</span></div>
      ${tbl}${detail}`, { pad: "4px 12px 16px", gap: 12 });
}

/* performance chart ------------------------------------------------------ */
const PERF = (() => {
  const noise = [3, -8, 12, -4, 7, -11, 5, 9, -6, 2, -3, 10, -9, 4, 6, -7, 1, 8, -5, 11, -2, -10, 6, 3, -8, 9, -4, 2, 7, -6];
  const rps = [], lat = [];
  for (let s = 1; s <= 30; s++) {
    const n = noise[s - 1];
    rps.push(s <= 5 ? 118 * s + n * 2 : 612 + 22 * Math.sin(s * 1.7) + n * 2);
    lat.push(31 + 5 * Math.sin(s * 0.9) + n * 0.4 + (s === 17 ? 18 : s === 18 ? 26 : s === 19 ? 9 : 0));
  }
  return { rps, lat };
})();

function perfChart() {
  const W = 700, Hh = 226, L = 46, R = 46, T = 14, B = 30;
  const pw = W - L - R, ph = Hh - T - B;
  const x = (i) => L + (i / 29) * pw;
  const yr = (v) => T + ph - (v / 800) * ph;
  const yl = (v) => T + ph - (v / 80) * ph;
  const grid = [0, 1, 2, 3, 4].map((g) => {
    const y = T + ph - (g / 4) * ph;
    return `<line x1="${L}" x2="${W - R}" y1="${y}" y2="${y}" style="stroke:var(--border);stroke-width:1"/>
      <text x="${L - 8}" y="${y + 4}" text-anchor="end" style="fill:var(--text-faint);font:11px Inter">${g * 200}</text>
      <text x="${W - R + 8}" y="${y + 4}" style="fill:var(--text-faint);font:11px Inter">${g * 20} ms</text>`;
  }).join("");
  const rpsPts = PERF.rps.map((v, i) => `${x(i).toFixed(1)},${yr(v).toFixed(1)}`);
  const area = `M${x(0)},${T + ph} L${rpsPts.join(" L")} L${x(29)},${T + ph} Z`;
  const latPts = PERF.lat.map((v, i) => `${x(i).toFixed(1)},${yl(v).toFixed(1)}`).join(" ");
  const hi = 17; // 18 s
  const hx = x(hi);
  const tipX = hx + 12;
  return `<svg viewBox="0 0 ${W} ${Hh}" width="100%" style="display:block">
    ${grid}
    <path d="${area}" style="fill:color-mix(in srgb, var(--accent) 16%, transparent);stroke:none"/>
    <polyline points="${rpsPts.join(" ")}" style="fill:none;stroke:var(--accent);stroke-width:2;stroke-linejoin:round"/>
    <polyline points="${latPts}" style="fill:none;stroke:var(--info);stroke-width:2;stroke-linejoin:round"/>
    <line x1="${hx}" x2="${hx}" y1="${T}" y2="${T + ph}" style="stroke:var(--text-faint);stroke-width:1;stroke-dasharray:3 3"/>
    <circle cx="${hx}" cy="${yr(PERF.rps[hi])}" r="4" style="fill:var(--accent);stroke:var(--surface);stroke-width:2"/>
    <circle cx="${hx}" cy="${yl(PERF.lat[hi])}" r="4" style="fill:var(--info);stroke:var(--surface);stroke-width:2"/>
    <g transform="translate(${tipX},${T + 6})">
      <rect width="128" height="74" rx="7" style="fill:var(--surface);stroke:var(--border-strong)"/>
      <text x="10" y="18" style="fill:var(--text);font:600 11.5px Inter">18 s</text>
      <circle cx="13" cy="32" r="3.5" style="fill:var(--accent)"/><text x="22" y="36" style="fill:var(--text);font:11.5px Inter">${Math.round(PERF.rps[hi])} req/s</text>
      <circle cx="13" cy="48" r="3.5" style="fill:var(--info)"/><text x="22" y="52" style="fill:var(--text);font:11.5px Inter">${Math.round(PERF.lat[hi])} ms avg</text>
      <text x="10" y="67" style="fill:var(--text-muted);font:11px Inter">25 users</text>
    </g>
    <text x="${L}" y="${Hh - 8}" style="fill:var(--text-faint);font:11px Inter">1 s</text>
    <text x="${W - R}" y="${Hh - 8}" text-anchor="end" style="fill:var(--text-faint);font:11px Inter">30 s</text>
  </svg>`;
}

function perfResults() {
  const pct = [["p50", 31, "--ok"], ["p90", 58, "--info"], ["p95", 74, "--info"], ["p99", 132, "--warn"], ["Max", 418, "--err"]];
  const percentiles = card(`<div class="section-title" style="margin-bottom:12px">Latency percentiles</div>
    <div class="pctl">${pct.map(([l, v, c]) => `<span class="muted med">${l}</span><div class="pb"><span style="width:${Math.max(4, (v / 418) * 100)}%;background:var(${c})"></span></div><span class="mono" style="text-align:right">${v} ms</span>`).join("")}</div>`);
  const perReq = table([{ label: "Request" }, { label: "Total", num: true, w: "60px" }, { label: "Errors", num: true, w: "52px" }, { label: "Avg", num: true, w: "60px" }, { label: "p95", num: true, w: "60px" }, { label: "Max", num: true, w: "66px" }],
    [["GET", "List books", "4,602", "0", "29 ms", "61 ms", "212 ms"], ["GET", "Get book", "4,598", "73", "24 ms", "52 ms", "190 ms"], ["POST", "Create book", "4,571", "0", "46 ms", "96 ms", "418 ms"], ["GQL", "Search catalog", "4,571", "0", "52 ms", "88 ms", "301 ms"]]
      .map(([m, n, t, e, a, p, mx]) => [`<span class="row" style="gap:8px"><span style="width:34px">${meth(m)}</span><span class="ellipsis">${n}</span></span>`, `<span class="mono">${t}</span>`, `<span class="mono ${e !== "0" ? "err" : ""}">${e}</span>`, `<span class="mono">${a}</span>`, `<span class="mono">${p}</span>`, `<span class="mono">${mx}</span>`]), { style: "font-size:12px" }).replace('<table class="tbl">', '<table class="tbl ptbl">');
  return toolbar(`${pill("Finished", "", { icon: "CHECK" })}<span class="hint">30 s · 25 virtual users · 5 s ramp-up</span>`,
    `<span class="pill acc" title="Proposed: the current app has no performance export">New</span>${btn("Export", { size: "sm", icon: "EXPORT", caret: true, menu: "perf-export" })}`)
    + paneBody(`<div class="grid4">${stat("Requests", "18,342", "25 virtual users")}${stat("Throughput", "611", "requests per second", { color: "accent-text" })}${stat("Error rate", "0.4%", "73 errors", { color: "err" })}${stat("Avg latency", "38 ms", "min 2 ms")}</div>
      ${card(`<div class="row" style="margin-bottom:8px"><span class="section-title grow">Throughput and latency</span>
        <span class="row small muted" style="gap:6px"><span class="dot" style="background:var(--accent)"></span>Requests per second</span>
        <span class="row small muted" style="gap:6px;margin-left:10px"><span class="dot" style="background:var(--info)"></span>Average latency</span></div>${perfChart()}`)}
      <div style="display:grid;grid-template-columns:minmax(0,34fr) minmax(0,66fr);gap:12px;align-items:start">${percentiles}
        <div class="col" style="gap:8px"><span class="section-title">Per request</span>${perReq}</div></div>`, { pad: "4px 12px 16px", gap: 12 });
}

function runnerView(mode) {
  return runnerTitle(mode) + `<div class="row" style="flex:1;min-height:0;align-items:stretch;gap:0">${runnerSetup(mode)}<section class="pane grow">${mode === "functional" ? functionalResults() : perfResults()}</section></div>`;
}

Object.assign(MENUS, {
  "run-export": () => placeMenu(menu(["Export every result", { icon: "BRACKETS_CURLY", label: "JSON", sub: ".json" }, { icon: "CODE", label: "JUnit XML", sub: ".xml" }, { icon: "FILE_TEXT", label: "HTML report", sub: ".html" }]), { right: true, w: 220 }),
  "perf-export": () => placeMenu(menu(["Export load test (new)", { icon: "FILE_TEXT", label: "HTML report", sub: "chart + tables" }, { icon: "TABLE", label: "CSV per second", sub: ".csv" }, { icon: "BRACKETS_CURLY", label: "JSON samples", sub: ".json" }]), { right: true, w: 250 }),
});

/* ------------------------------------------------------------------ screens */
const FLOW_TAB = { id: "flow", icon: "FLOW_ARROW", t: "Catalog summary", go: "flow" };
const RUN_TAB = { id: "runner", icon: "PLAY_CIRCLE", t: "Bookstore API", go: "runner" };

screen({ id: "flow", group: "Workspace", label: "Flows: canvas + inspector blok", rail: "flows", tab: FLOW_TAB, view: () => flowView({ sel: "books", run: true }),
  notes: N([
    "Tata letak tiga zona yang tetap: kanvas (tengah), Inspector (kanan, 324 px, setinggi view), Run log (bawah kanvas, 196 px). Garis judul 46 px sejajar dengan layar HTTP.",
    "Kartu blok: header berwarna sesuai jenis (tile ikon + nama + badge hasil run), satu baris ringkasan mono, lalu baris port. Port terhubung = titik penuh; port kosong = cincin berwarna (failure merah) sehingga cabang yang belum ditangani langsung terlihat.",
    "Edge yang dilewati run terakhir hijau tebal; selebihnya abu tipis. Blok terpilih diberi halo aksen 4 px.",
    "Kontrol zoom dipindah ke kapsul melayang kanan-bawah (−, 78%, +, Fit). Pill zoom di judul tetap ada (klik = 100%).",
    "Inspector blok: banner hasil run di atas, lalu seksi Block (Label, Reference + Copy), konfigurasi per jenis (Request: Saved/Inline, picker, Success status), dan Output JSON terakhir. Tombol Duplicate/Delete/Close jadi ikon di header.",
    "Run log: header ringkas (pill hasil, langkah · durasi, tes, chip Outputs, sapu) dan baris jam · ikon · chip blok · teks; menempel ke bawah.",
  ]) + NE([
    "<code>flow_tab::show</code>: Inspector sebagai <code>Panel::right</code> di dalam tab (bukan hanya ≥820 px), run log <code>Panel::bottom</code> di kolom kanvas saja.",
    "<code>draw_card</code>: header 30 px dengan fill <code>soft(kind_color, .10)</code>, ringkasan 24 px, port 20 px; titik port tak terhubung digambar <code>circle_stroke</code> warna port.",
    "<code>canvas</code>: edge pakai warna <code>ok</code> bila ada di jejak run terakhir; zoom −/+ (<code>:1483</code>) digabung dengan Fit dalam satu <code>Frame</code> popup.",
    "<code>block_inspector</code> / <code>inspector_head</code> / <code>banner</code> tetap; hanya urutan seksi dan tombol header berubah.",
  ]) });

screen({ id: "flow-settings", group: "Workspace", label: "Flows: pengaturan flow + menu Add block", rail: "flows", tab: FLOW_TAB, preset: { menu: "add-block", menuPos: { r: 86, y: 128 } }, view: () => flowView({ run: false, warn: "sum" }),
  notes: N([
    "Tanpa seleksi, Inspector menampilkan Flow settings: Description, Inputs (kartu per input: nama, Required, Default, Next run), Run settings, Problems, Keyboard.",
    "Problems tampil sebagai baris kuning yang bisa diklik (memilih blok). Blok bermasalah di kanvas mendapat border kuning + badge peringatan.",
    "Menu Add block dikelompokkan Actions / Logic / Data dengan tile ikon berwarna sesuai jenis. Deskripsi jenis (dulu hanya tooltip) kini tampil sebagai baris kedua agar mudah dipindai.",
    "Sebelum run: edge abu-abu, tanpa badge waktu; Run log menampilkan state kosong dengan shortcut ⌘↩.",
  ]) + NE([
    "<code>flow_settings</code>: kartu input memakai <code>kit::card</code>; Default/Next run dalam <code>form_grid</code> 2 kolom.",
    "Menu Add block (<code>:794</code>): baris 40 px = tile 26 px + label <code>medium</code> + deskripsi <code>small</code> muted.",
    "Validasi: border <code>warn</code> di <code>draw_card</code> sudah ada; tambahkan badge ikon di header.",
  ]) });

screen({ id: "runner", group: "Workspace", label: "Collection Runner: fungsional (1 gagal)", rail: "collections", tab: RUN_TAB, view: () => runnerView("functional"),
  notes: N([
    "Dua panel seperti layar HTTP: setup di kiri (336 px, footer Run menempel di bawah), hasil di kanan. Garis toolbar 40 px kiri–kanan sejajar.",
    "Setup: Run order (handle seret, checkbox, badge method, folder samar di kanan, pill 10/10, All/None) lalu Options sebagai baris label + hint + kontrol. Data file tampil sebagai chip berkas (nama, jumlah baris, Preview data, hapus). Hint ‘Drag the handles…’ pindah ke ikon info di header Run order supaya semua opsi muat tanpa scroll.",
    "Hasil: toolbar status run + Export ▾; 6 kartu statistik; banner hasil; filter All/Passed/Failed; tabel hasil (#iterasi, method, folder / nama, tes, status, waktu).",
    "Baris gagal yang dipilih membuka kartu detail di bawah tabel: URL, daftar tes dengan pesan error, Headers dan Body sebagai disclosure. ↑↓ pindah, Esc tutup.",
    "Di bawah 820 px setup dan hasil jadi segmented Setup | Results (seperti sekarang).",
  ]) + NE([
    "<code>runner_tab::show</code>: ganti kartu setup mengambang dengan panel ber-hairline (<code>Panel::left</code> 336 px), footer di <code>Panel::bottom</code> dalam panel itu.",
    "<code>setup_panel</code> / <code>run_order</code> / <code>functional_options</code>: chip data file baru = <code>Frame</code> well + ikon + nama + jumlah baris + <code>icon_button</code> X.",
    "<code>functional_results</code>: toolbar 40 px baru berisi status run dan Export; <code>stat_cards</code> 6 kolom; <code>results_table</code> + <code>result_detail</code> tetap.",
  ]) });

screen({ id: "runner-perf", group: "Workspace", label: "Collection Runner: performance", rail: "collections", tab: { ...RUN_TAB, go: "runner-perf" }, view: () => runnerView("perf"),
  notes: N([
    "Mode Performance memakai kerangka yang sama: Run order (4/10 dipilih) + Load profile (Virtual users, Duration, Ramp-up) di kiri; footer ‘25 users · 30 s’ + Start load test.",
    "Hasil: 4 kartu statistik (Requests, Throughput beraksen, Error rate merah, Avg latency), grafik ‘Throughput and latency’ dua sumbu dengan area req/s + garis latency, penanda hover dan tooltip (detik, req/s, avg, users).",
    "Latency percentiles (p50 hijau, p90/p95 biru, p99 kuning, Max merah) kini berdampingan dengan tabel Per request (34/66), bukan di samping grafik — grafik mendapat lebar penuh.",
    "<b>Baru (usulan):</b> tombol Export ▾ (HTML report, CSV per detik, JSON) — aplikasi sekarang belum punya export untuk mode performance; ini celah yang perlu ditutup.",
  ]) + NE([
    "<code>perf_results</code>: <code>timeline_chart</code> tetap 180–200 px dengan dua sumbu; tooltip hover sudah ada.",
    "<code>percentiles</code> dipindah ke baris bawah bersama tabel Per request (<code>ui.columns</code> 34/66), lebar &lt;760 px → bertumpuk.",
    "Export perf: perlu writer baru di <code>ternarywire-runner</code> (HTML/CSV/JSON dari sampel per detik) + menu seperti Export fungsional.",
  ]) });
