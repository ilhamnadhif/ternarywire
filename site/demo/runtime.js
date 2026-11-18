// The live window in the website's hero. It is the redesign prototype (design/redesign), copied
// here by scripts/build-site.py, with this file in place of its app.js: no design harness, the
// window only, and a few behaviours the page asks for (Send, the theme, which screen is shown).
// Screens register themselves with screen({...}); dialogs with DIALOGS[id]; menus with MENUS[id].

const SCREENS = [];
const DIALOGS = {};
const MENUS = {};
const screen = (def) => SCREENS.push(def);

const QS = new URLSearchParams(location.search);
const SIZE = (QS.get("size") || "1280x820").split("x").map(Number);
const SIZES = [["demo", SIZE[0] || 1280, SIZE[1] || 820]];

const DEFAULTS = {
  screen: "http", theme: "dark", size: 0, notes: false, dlg: "", menu: "", menuPos: null,
  sidebar: true, console: false, layout: "side",
  // http
  reqTab: "params", resTab: "body", bodyView: "pretty", bodyMode: "raw", resState: "ok", authType: "oauth2",
  scriptEv: "post", filter: false, testFilter: "all", bulk: false,
  // generic sub-tabs used by other screens
  sub: "", sub2: "", seg: "", sel: "", page: "general", mode: "",
};
let S = { ...DEFAULTS, theme: QS.get("theme") === "light" ? "light" : "dark" };

const cur = () => SCREENS.find((s) => s.id === S.screen) || SCREENS[0];
const W = () => SIZES[0][1];
const H = () => SIZES[0][2];
const sidebarW = () => Math.round(Math.min(276, Math.max(224, W() * 0.21)));
const benchW = () => W() - 52 - (S.sidebar ? sidebarW() + 2 : 0) - 10;

const tell = (msg) => { try { parent.postMessage({ tw: true, ...msg }, "*"); } catch (e) { /* not framed */ } };

function go(id) {
  const s = SCREENS.find((x) => x.id === id);
  if (!s) return;
  S = { ...DEFAULTS, theme: S.theme, screen: id, ...(s.preset || {}), size: 0 };
  S.dlg = s.dlg || "";
  clearTimeout(sendTimer);
  render();
  tell({ screen: id });
}

window.__scale = 1;
function render() {
  const root = document.getElementById("root");
  root.innerHTML = `<div class="win theme-${S.theme} bare" id="win" style="width:${W()}px;height:${H()}px">${appShell(cur())}</div>`;
}

/* ------------------------------------------------------------------ Send */
// The request bar's Send button (and ⌘↩) sends: the response pane shows "Sending…" for a moment,
// then the response, as the app does.
let sendTimer = 0;
function send() {
  const s = cur();
  if (!s.id.startsWith("http") || s.id === "http-graphql") return false;
  if (s.id === "http-empty" || s.id === "http-error" || s.id === "http-stacked") { go("http"); }
  S.resState = "loading"; S.dlg = ""; S.menu = ""; render();
  tell({ sending: true });
  clearTimeout(sendTimer);
  sendTimer = setTimeout(() => { S.resState = "ok"; S.resTab = "body"; render(); tell({ sent: true }); }, 900);
  return true;
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
  const sendBtn = e.target.closest(".btn.primary.lg");
  if (sendBtn && /^Send/.test(sendBtn.getAttribute("title") || "")) { if (send()) return; }
  const cancel = e.target.closest(".btn");
  if (cancel && S.resState === "loading" && /^Cancel/.test(cancel.getAttribute("title") || cancel.textContent.trim())) {
    clearTimeout(sendTimer); S.resState = "ok"; render(); return;
  }
  let n = e.target;
  while (n && n.nodeType === 1 && n.id !== "win") {
    const d = n.dataset;
    if (d.menu !== undefined) {
      if (S.menu === d.menu) { S.menu = ""; render(); return; }
      const win = document.getElementById("win").getBoundingClientRect();
      const r = n.getBoundingClientRect();
      S.menu = d.menu;
      S.menuPos = { x: r.left - win.left, y: r.bottom - win.top + 4, r: win.right - r.right, w: r.width };
      render(); return;
    }
    if (d.go) { go(d.go); return; }
    if (d.dlg !== undefined) { S.dlg = d.dlg; S.menu = ""; render(); tell({ dialog: d.dlg }); return; }
    if (d.set !== undefined) { parseSet(d.set); S.menu = ""; render(); return; }
    if (d.stop) break;
    n = n.parentElement;
  }
  if (S.menu) { S.menu = ""; render(); }
});
document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (mod && e.key === "Enter") { e.preventDefault(); send(); return; }
  if (mod && e.key.toLowerCase() === "k") { e.preventDefault(); S.dlg = "palette"; render(); return; }
  if (mod && e.key === ",") { e.preventDefault(); S.dlg = "settings"; render(); return; }
  if (mod && e.key.toLowerCase() === "e") { e.preventDefault(); S.dlg = "env-switch"; render(); return; }
  if (mod && e.key.toLowerCase() === "b") { e.preventDefault(); S.sidebar = !S.sidebar; render(); return; }
  if (e.key === "Escape") { if (S.menu) S.menu = ""; else S.dlg = ""; render(); }
});

// The page drives the window: {go: id}, {theme: "dark"|"light"}, {send: true}.
window.addEventListener("message", (e) => {
  const m = e.data || {};
  if (m.go) go(m.go);
  if (m.theme) { S.theme = m.theme === "light" ? "light" : "dark"; render(); }
  if (m.send) send();
});

window.addEventListener("DOMContentLoaded", () => {
  const id = QS.get("screen");
  go(id && SCREENS.find((s) => s.id === id) ? id : "http");
  tell({ ready: true });
});
