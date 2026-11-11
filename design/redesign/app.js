// Prototype runtime: state, screen registry, design harness and events.
// Screens register themselves with screen({...}); dialogs with DIALOGS[id].

const SCREENS = [];
const DIALOGS = {};
const MENUS = {};
const screen = (def) => SCREENS.push(def);

const SIZES = [
  ["1440×900", 1440, 900],
  ["1280×820 (default)", 1280, 820],
  ["1100×720", 1100, 720],
  ["900×640 (narrow)", 900, 640],
];

const DEFAULTS = {
  screen: "http", theme: "light", size: 0, notes: true, dlg: "", menu: "", menuPos: null,
  sidebar: true, console: false, layout: "side",
  // http
  reqTab: "params", resTab: "body", bodyView: "pretty", bodyMode: "raw", resState: "ok", authType: "oauth2",
  scriptEv: "post", filter: false, testFilter: "all", bulk: false,
  // generic sub-tabs used by other screens
  sub: "", sub2: "", seg: "", sel: "", page: "general", mode: "",
};
let S = { ...DEFAULTS };
try {
  const saved = JSON.parse(localStorage.getItem("tw-redesign") || "{}");
  S.theme = saved.theme || S.theme; S.size = saved.size ?? S.size; S.notes = saved.notes ?? S.notes;
} catch (e) { /* storage unavailable */ }
// ?theme=dark&notes=0 in the URL (screenshots, links)
const QS = new URLSearchParams(location.search);
if (QS.get("theme")) S.theme = QS.get("theme");
if (QS.get("notes")) S.notes = QS.get("notes") !== "0";

const cur = () => SCREENS.find((s) => s.id === S.screen) || SCREENS[0];
const W = () => SIZES[S.size][1];
const H = () => SIZES[S.size][2];
const sidebarW = () => Math.round(Math.min(276, Math.max(224, W() * 0.21)));
const benchW = () => W() - 52 - (S.sidebar ? sidebarW() + 2 : 0) - 10;

function go(id) {
  const s = SCREENS.find((x) => x.id === id);
  if (!s) return;
  const keep = { theme: S.theme, userSize: S.userSize ?? S.size, notes: S.notes };
  S = { ...DEFAULTS, ...keep, screen: id, ...(s.preset || {}) };
  S.size = s.preset && s.preset.size !== undefined ? s.preset.size : S.userSize;
  S.dlg = s.dlg || "";
  if (location.hash !== "#" + id) history.replaceState(null, "", "#" + id);
  render();
}

/* ------------------------------------------------------------------ harness */
function harness() {
  const groups = [...new Set(SCREENS.map((s) => s.group))];
  const opts = groups.map((g) => `<optgroup label="${g}">${SCREENS.filter((s) => s.group === g)
    .map((s) => `<option value="${s.id}" ${s.id === S.screen ? "selected" : ""}>${s.label}</option>`).join("")}</optgroup>`).join("");
  const idx = SCREENS.indexOf(cur());
  return `<div class="harness-bar">
    <div class="brand"><img src="../../crates/ternarywire-app/assets/icon-256.png" alt="">TernaryWire · Redesign</div>
    <div class="sep"></div>
    <button data-h="prev" title="Previous screen ([)">‹</button>
    <select id="h-screen" style="min-width:300px">${opts}</select>
    <button data-h="next" title="Next screen (])">›</button>
    <span style="color:#8a8b93">${idx + 1} / ${SCREENS.length}</span>
    <div class="sep"></div>
    <button data-h="theme" class="${S.theme === "dark" ? "on" : ""}">${S.theme === "dark" ? "Dark" : "Light"}</button>
    <select id="h-size">${SIZES.map((s, i) => `<option value="${i}" ${i === S.size ? "selected" : ""}>${s[0]}</option>`).join("")}</select>
    <div class="spacer"></div>
    <button data-h="notes" class="${S.notes ? "on" : ""}">Catatan desain</button>
  </div>`;
}
function notesPanel() {
  const n = cur().notes;
  if (!S.notes || !n) return "";
  return `<aside class="harness-notes"><h3>${cur().label}</h3><div style="color:#8a8b93;margin-bottom:6px">${cur().group}</div>${n}</aside>`;
}

/* ------------------------------------------------------------------ render */
// ?bare=1&size=1280x820: only the app window at 1:1, no harness (for pixel comparison with the app).
const BARE = QS.get("bare") === "1";
const BARE_SIZE = (QS.get("size") || "").split("x").map(Number);
function render() {
  const root = document.getElementById("root");
  const s = cur();
  if (BARE) {
    const [bw, bh] = BARE_SIZE.length === 2 && BARE_SIZE[0] ? BARE_SIZE : [W(), H()];
    SIZES.push(["bare", bw, bh]); S.size = SIZES.length - 1; S.userSize = S.size;
    root.innerHTML = `<div class="win theme-${S.theme} bare" id="win" style="width:${bw}px;height:${bh}px">${appShell(s)}</div>`;
    SIZES.pop();
    window.__scale = 1;
    return;
  }
  const stageW = window.innerWidth - (S.notes && s.notes ? 392 : 0);
  const stageH = window.innerHeight - 44;
  const scale = Math.min(1, (stageW - 40) / W(), (stageH - 40) / H());
  root.innerHTML = `<div class="harness">${harness()}
    <div class="harness-stage" style="background:${S.theme === "dark" ? "#08080a" : "#cfd0d6"};${S.notes && s.notes ? "padding-right:392px" : ""}">
      <div class="win theme-${S.theme}" id="win" style="width:${W()}px;height:${H()}px;transform:scale(${scale})">${appShell(s)}</div>
      ${notesPanel()}
    </div></div>`;
  window.__scale = scale;
  try { localStorage.setItem("tw-redesign", JSON.stringify({ theme: S.theme, size: S.userSize ?? S.size, notes: S.notes })); } catch (e) { /* ignore */ }
}

/* ------------------------------------------------------------------ events */
function parseSet(str) {
  str.split(";").forEach((pair) => {
    if (!pair) return;
    const i = pair.indexOf("=");
    const k = pair.slice(0, i), v = pair.slice(i + 1);
    if (v === "!") S[k] = !S[k];
    else S[k] = v === "true" ? true : v === "false" ? false : v;
  });
}
document.addEventListener("click", (e) => {
  const h = e.target.closest("[data-h]");
  if (h) {
    const a = h.dataset.h;
    const i = SCREENS.indexOf(cur());
    if (a === "prev") go(SCREENS[(i - 1 + SCREENS.length) % SCREENS.length].id);
    if (a === "next") go(SCREENS[(i + 1) % SCREENS.length].id);
    if (a === "theme") { S.theme = S.theme === "dark" ? "light" : "dark"; render(); }
    if (a === "notes") { S.notes = !S.notes; render(); }
    return;
  }
  let n = e.target;
  while (n && n.nodeType === 1 && n.id !== "win") {
    const d = n.dataset;
    if (d.menu !== undefined) {
      if (S.menu === d.menu) { S.menu = ""; render(); return; }
      const win = document.getElementById("win").getBoundingClientRect();
      const r = n.getBoundingClientRect();
      const sc = window.__scale || 1;
      S.menu = d.menu;
      S.menuPos = { x: (r.left - win.left) / sc, y: (r.bottom - win.top) / sc + 4, r: (win.right - r.right) / sc, w: r.width / sc };
      render(); return;
    }
    if (d.go) { go(d.go); return; }
    if (d.dlg !== undefined) { S.dlg = d.dlg; S.menu = ""; render(); return; }
    if (d.set !== undefined) { parseSet(d.set); S.menu = ""; render(); return; }
    if (d.stop) break;
    n = n.parentElement;
  }
  if (S.menu) { S.menu = ""; render(); }
});
document.addEventListener("change", (e) => {
  if (e.target.id === "h-screen") go(e.target.value);
  if (e.target.id === "h-size") { S.size = S.userSize = +e.target.value; render(); }
});
document.addEventListener("keydown", (e) => {
  const typing = /INPUT|TEXTAREA|SELECT/.test(document.activeElement?.tagName || "");
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); S.dlg = "palette"; render(); return; }
  if ((e.metaKey || e.ctrlKey) && e.key === ",") { e.preventDefault(); S.dlg = "settings"; render(); return; }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "e") { e.preventDefault(); S.dlg = "env-switch"; render(); return; }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") { e.preventDefault(); S.sidebar = !S.sidebar; render(); return; }
  if (e.key === "Escape") { if (S.menu) S.menu = ""; else S.dlg = ""; render(); return; }
  if (typing) return;
  const i = SCREENS.indexOf(cur());
  if (e.key === "]") go(SCREENS[(i + 1) % SCREENS.length].id);
  if (e.key === "[") go(SCREENS[(i - 1 + SCREENS.length) % SCREENS.length].id);
});
window.addEventListener("resize", () => render());
window.addEventListener("hashchange", () => { const id = location.hash.slice(1); if (id && id !== S.screen) go(id); });
const GROUP_ORDER = ["Start", "HTTP request", "Realtime & RPC", "Workspace", "Local servers", "Dialogs & panels"];
window.addEventListener("DOMContentLoaded", () => {
  const rank = (g) => { const i = GROUP_ORDER.indexOf(g); return i < 0 ? 99 : i; };
  SCREENS.sort((a, b) => rank(a.group) - rank(b.group)); // stable: file order within a group
  const id = location.hash.slice(1);
  if (id && SCREENS.find((s) => s.id === id)) go(id); else go(SCREENS[0].id);
});

// ?audit=1: renders every screen in both themes and lists JS errors and content
// that spills out of its pane (used to check the prototype headlessly).
if (QS.get("audit")) {
  window.addEventListener("load", () => {
    const out = [];
    const errs = [];
    window.addEventListener("error", (e) => errs.push(e.message));
    for (const theme of ["light", "dark"]) {
      for (const s of SCREENS) {
        errs.length = 0;
        try { S.theme = theme; go(s.id); } catch (e) { errs.push(e.message); }
        const win = document.getElementById("win");
        const view = win && win.querySelector(".view");
        const issues = [...errs];
        if (!view || !view.innerHTML.trim()) issues.push("empty view");
        const wr = win.getBoundingClientRect();
        win.querySelectorAll(".view *, .titlebar *, .statusbar *, .sidebar *, .modal *").forEach((el) => {
          const r = el.getBoundingClientRect();
          if (r.width && r.right > wr.right + 2 && getComputedStyle(el).position !== "fixed") issues.push("past window: " + (el.className || el.tagName).toString().slice(0, 40));
        });
        if (issues.length) out.push(`${theme} ${s.id}: ${[...new Set(issues)].slice(0, 6).join(" | ")}`);
      }
    }
    document.body.innerHTML = `<pre id="audit">SCREENS ${SCREENS.length}\n${out.join("\n") || "no issues"}</pre>`;
  });
}
