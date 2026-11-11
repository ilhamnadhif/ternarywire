// Components of the redesign as HTML-string helpers. Each one has a twin in
// crates/ternarywire-app/src/kit.rs or widgets.rs (named in TOKENS.md).

const I = (n, cls = "") => `<i class="i ${cls}">${ICON[n] || ""}</i>`;
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const attrs = (o = {}) =>
  Object.entries(o).filter(([, v]) => v !== undefined && v !== null && v !== false)
    .map(([k, v]) => (v === true ? k : `${k}="${String(v).replace(/"/g, "&quot;")}"`)).join(" ");

/* buttons ------------------------------------------------------------ */
function btn(label, o = {}) {
  const cls = ["btn", o.kind || "", o.size || "", o.on ? "on" : "", !label ? "ib" : ""].join(" ");
  return `<button class="${cls}" ${attrs({ "data-set": o.set, "data-go": o.go, "data-dlg": o.dlg, "data-menu": o.menu, title: o.title, disabled: o.disabled, style: o.style })}>${o.icon ? I(o.icon) : ""}${label ? `<span>${label}</span>` : ""}${o.kbd ? kbd(o.kbd) : ""}${o.caret ? I("CARET_DOWN", "car") : ""}</button>`;
}
const primary = (label, o = {}) => btn(label, { ...o, kind: "primary " + (o.kind || "") });
const ghost = (label, o = {}) => btn(label, { ...o, kind: "ghost " + (o.kind || "") });
function ibtn(icon, title, o = {}) {
  return `<button class="ibtn ${o.sm ? "sm" : ""} ${o.on ? "on" : ""}" ${attrs({ title, "aria-label": title, "data-set": o.set, "data-go": o.go, "data-dlg": o.dlg, "data-menu": o.menu })}>${I(icon)}${o.dot ? `<span class="ndot ${o.dot}"></span>` : ""}</button>`;
}
const kbd = (k) => `<span class="kbd">${k}</span>`;
const sw = (on) => `<span class="switch ${on ? "on" : ""}"></span>`;
const chk = (on, mixed) => `<span class="check ${on ? "on" : ""} ${mixed ? "mixed" : ""}"></span>`;
const radio = (on) => `<span class="radio ${on ? "on" : ""}"></span>`;

/* labels -------------------------------------------------------------- */
const meth = (m, cls = "") => `<span class="meth m-${m} ${cls}">${m}</span>`;
function pill(text, kind = "", o = {}) {
  return `<span class="pill ${kind} ${o.live ? "live" : ""} ${o.lg ? "lg" : ""} ${o.outline ? "outline" : ""}" ${attrs({ title: o.title })}>${o.dot ? '<span class="d"></span>' : ""}${o.icon ? I(o.icon) : ""}${text}</span>`;
}
function statusPill(code, reason) {
  const k = code < 300 ? "ok" : code < 400 ? "info" : code < 500 ? "warn" : "err";
  return pill(`${code} ${reason}`, k, { lg: true });
}
const eyebrow = (t) => `<div class="eyebrow">${t}</div>`;
const hint = (t) => `<div class="hint">${t}</div>`;

/* tabs + segmented --------------------------------------------------- */
// items: [id, label, {count, countKind, dot}]
function tabs(key, cur, items, end = "", o = {}) {
  const t = items.map(([id, label, x = {}]) =>
    `<button class="tb ${cur === id ? "on" : ""}" data-set="${key}=${id}">${x.icon ? I(x.icon) : ""}${label}${x.count !== undefined ? `<span class="cnt ${x.countKind || ""}">${x.count}</span>` : ""}${x.dot ? '<span class="sdot"></span>' : ""}</button>`).join("");
  return `<div class="tabs" ${attrs({ style: o.style })}>${t}${end ? `<div class="tend">${end}</div>` : ""}</div>`;
}
function seg(key, cur, items, o = {}) {
  return `<div class="seg ${o.sm ? "sm" : ""}">${items.map(([id, label, icon]) =>
    `<button class="${cur === id ? "on" : ""}" data-set="${key}=${id}">${icon ? I(icon) : ""}${label}</button>`).join("")}</div>`;
}

/* inputs -------------------------------------------------------------- */
function input(value, o = {}) {
  return `<input class="input ${o.sm ? "sm" : ""} ${o.mono ? "mono" : ""} ${o.focus ? "focus" : ""}" ${attrs({ value, placeholder: o.ph, style: o.style, type: o.type })} />`;
}
function selectbox(label, o = {}) {
  return `<span class="selectbox ${o.sm ? "sm" : ""} ${o.quiet ? "quiet" : ""}" ${attrs({ style: o.style, "data-menu": o.menu })}>${o.icon ? I(o.icon) : ""}${o.dot ? `<span class="dot" style="background:${o.dot}"></span>` : ""}<span class="grow ellipsis">${label}</span>${I("CARET_DOWN", "car")}</span>`;
}
function search(ph, o = {}) {
  return `<label class="search" ${attrs({ style: o.style })}>${I("MAGNIFYING_GLASS")}<input class="input ${o.sm ? "sm" : ""}" placeholder="${ph}" ${attrs({ value: o.value })}/>${o.kbd ? kbd(o.kbd) : ""}</label>`;
}
function textarea(value, o = {}) {
  return `<textarea class="textarea ${o.mono ? "mono" : ""}" rows="${o.rows || 3}" ${attrs({ placeholder: o.ph, style: o.style })}>${esc(value || "")}</textarea>`;
}
const field = (label, control, help) => `<div class="field"><label>${label}</label>${control}${help ? `<div class="help">${help}</div>` : ""}</div>`;
const frow = (t, d, control) => `<div class="frow"><div class="fl"><div class="t">${t}</div>${d ? `<div class="d">${d}</div>` : ""}</div>${control}</div>`;
const group = (title, rows, o = {}) => `<div class="col" style="gap:8px">${title ? `<div class="row"><h4 class="section-title">${title}</h4>${o.end ? `<span class="spacer"></span>${o.end}` : ""}</div>` : ""}${o.hint ? hint(o.hint) : ""}<div class="group">${rows.join("")}</div></div>`;

/* variables in text --------------------------------------------------- */
function vars(s, o = {}) {
  return esc(s).replace(/\{\{([^}]+)\}\}/g, (m, name) => {
    const bad = (o.undefined || []).includes(name);
    const sec = name.startsWith("vault:") || (o.secret || []).includes(name);
    return `<span class="var ${bad ? "bad" : ""} ${sec ? "sec" : ""}">{{${name}}}</span>`;
  });
}
function urlHtml(u) {
  const [base, q] = u.split("?");
  return vars(base) + (q ? `<span class="q">?</span>` + q.split("&").map((p) => {
    const [k, v] = p.split("=");
    return `<span>${esc(k)}</span><span class="q">=</span>${vars(v || "")}`;
  }).join('<span class="q">&amp;</span>') : "");
}

/* key-value table ----------------------------------------------------- */
// rows: [on, key, value, desc, {lock, ph}]
function kv(rows, o = {}) {
  const desc = o.desc !== false;
  const head = `<div class="kr h"><div>${chk(rows.length > 0, rows.some((r) => !r[0]))}</div><div>${o.k || "Key"}</div><div>${o.v || "Value"}</div>${desc ? `<div>${o.d || "Description"}</div>` : ""}<div>${I("DOTS_THREE")}</div></div>`;
  const body = rows.map(([on, k, v, d, x = {}]) =>
    `<div class="kr ${on ? "" : "off"}"><div>${chk(on)}</div><div class="mono">${esc(k)}${x.lock ? `<span class="lock" title="Set by TernaryWire">${I("LOCK_SIMPLE")}</span>` : ""}</div><div class="mono">${x.html ? v : vars(v)}</div>${desc ? `<div class="muted">${d || ""}</div>` : ""}<div>${I("TRASH")}</div></div>`).join("");
  const add = `<div class="kr ph"><div></div><div>${o.k || "Key"}</div><div>${o.v || "Value"}</div>${desc ? "<div>Description</div>" : ""}<div></div></div>`;
  return `<div class="kv ${desc ? "" : "nodesc"}">${head}${body}${o.noadd ? "" : add}</div>`;
}

/* code ---------------------------------------------------------------- */
function jsonLines(v, ind = 0, key = null, last = true, out = []) {
  const pad = "  ".repeat(ind);
  const k = key !== null ? `<span class="k">"${esc(key)}"</span><span class="p">: </span>` : "";
  const comma = last ? "" : '<span class="p">,</span>';
  if (Array.isArray(v)) {
    if (!v.length) { out.push(`${pad}${k}<span class="p">[]</span>${comma}`); return out; }
    out.push({ t: `${pad}${k}<span class="p">[</span>`, fold: true });
    v.forEach((x, i) => jsonLines(x, ind + 1, null, i === v.length - 1, out));
    out.push(`${pad}<span class="p">]</span>${comma}`);
  } else if (v && typeof v === "object") {
    const ks = Object.keys(v);
    if (!ks.length) { out.push(`${pad}${k}<span class="p">{}</span>${comma}`); return out; }
    out.push({ t: `${pad}${k}<span class="p">{</span>`, fold: true });
    ks.forEach((kk, i) => jsonLines(v[kk], ind + 1, kk, i === ks.length - 1, out));
    out.push(`${pad}<span class="p">}</span>${comma}`);
  } else {
    const cls = typeof v === "string" ? "s" : typeof v === "number" ? "n" : "b";
    const val = typeof v === "string" ? `"${esc(v)}"` : String(v);
    out.push(`${pad}${k}<span class="${cls}">${val}</span>${comma}`);
  }
  return out;
}
function code(lines, o = {}) {
  const ls = lines.map((l) => (typeof l === "string" ? { t: l } : l));
  const max = o.max || ls.length;
  const shown = ls.slice(0, max);
  const gut = o.gutter === false ? "" : `<div class="gut">${shown.map((l, i) => `<div>${i + 1 + (o.start || 0)}<span class="fold">${l.fold ? ICON.CARET_DOWN : ""}</span></div>`).join("")}</div>`;
  return `<div class="code ${o.cls || ""}" ${attrs({ style: o.style })}>${gut}<div class="src">${shown.map((l, i) => `<div class="${l.cls || ""} ${o.cur === i ? "cur" : ""}">${l.t || " "}</div>`).join("")}</div></div>`;
}
const jsonCode = (v, o) => code(jsonLines(v), o);
// light-weight JS highlighter for script editors
function jsHl(src) {
  return src.split("\n").map((line) => {
    let s = esc(line);
    if (/^\s*\/\//.test(line)) return `<span class="c">${s}</span>`;
    s = s.replace(/(&quot;|"|')(.*?)\1/g, '<span class="s">$1$2$1</span>');
    s = s.replace(/\b(const|let|var|function|return|if|else|await|async|new|true|false|null|for|of)\b/g, '<span class="b">$1</span>');
    s = s.replace(/\b(pm|tw|console|JSON)\b/g, '<span class="k">$1</span>');
    s = s.replace(/\b(\d+)\b/g, '<span class="n">$1</span>');
    return s;
  });
}
function yamlHl(src) {
  return src.split("\n").map((line) => {
    const m = line.match(/^(\s*-?\s*)([\w$\/{}."-]+)(:)(.*)$/);
    if (!m) return `<span class="s">${esc(line)}</span>`;
    let rest = esc(m[4]);
    if (/^\s*\d+(\.\d+)*\s*$/.test(m[4])) rest = `<span class="n">${rest}</span>`;
    else if (rest.trim()) rest = `<span class="s">${rest}</span>`;
    return `${esc(m[1])}<span class="k">${esc(m[2])}</span><span class="p">:</span>${rest}`;
  });
}

/* containers ---------------------------------------------------------- */
function empty(icon, title, body, acts = "", o = {}) {
  return `<div class="empty" ${attrs({ style: o.style })}><div class="ei">${I(icon)}</div><h4>${title}</h4>${body ? `<p>${body}</p>` : ""}${acts ? `<div class="acts">${acts}</div>` : ""}</div>`;
}
const banner = (kind, icon, html) => `<div class="banner ${kind}">${I(icon)}<div class="grow">${html}</div></div>`;
const card = (inner, o = {}) => `<div class="card ${o.pad === false ? "" : "pad"}" ${attrs({ style: o.style })}>${inner}</div>`;
const stat = (label, value, sub, o = {}) => `<div class="stat"><div class="l">${o.icon ? I(o.icon) : ""}${label}</div><div class="v" ${o.color ? `style="color:var(--${o.color})"` : ""}>${value}</div><div class="s">${sub || ""}</div></div>`;
function ptitle(icon, title, sub, o = {}) {
  return `<div class="ptitle"><div class="pi" ${o.tint ? `style="background:color-mix(in srgb, var(${o.tint}) 14%, transparent);color:var(${o.tint})"` : ""}>${I(icon)}</div><div class="col" style="gap:0"><div class="row" style="gap:8px"><h1>${title}</h1>${o.after || ""}</div>${sub ? `<div class="sub">${sub}</div>` : ""}</div></div>`;
}
function menu(items, o = {}) {
  const body = items.map((it) => {
    if (it === "-") return '<div class="ms"></div>';
    if (typeof it === "string") return `<div class="mh">${it}</div>`;
    return `<div class="mi ${it.danger ? "danger" : ""} ${it.on ? "on" : ""}" ${attrs({ "data-go": it.go, "data-dlg": it.dlg, "data-set": it.set })}>${it.icon ? I(it.icon) : it.meth ? `<span style="width:16px">${meth(it.meth)}</span>` : it.check !== undefined ? `<span style="width:16px;color:var(--accent-text)">${it.check ? I("CHECK") : ""}</span>` : it.dot ? `<span class="dot" style="background:${it.dot}"></span>` : ""}<span class="grow ellipsis">${it.label}</span>${it.sub ? `<span class="sub">${it.sub}</span>` : ""}${it.kbd ? kbd(it.kbd) : ""}${it.more ? I("CARET_RIGHT", "faint") : ""}</div>`;
  }).join("");
  return `<div class="menu" style="${o.style || ""}">${body}</div>`;
}
function modal(title, body, foot, o = {}) {
  return `<div class="scrim ${o.center ? "center" : ""}" data-set="dlg=">
    <div class="modal" style="width:${o.w || 560}px;${o.h ? `height:${o.h}px;` : ""}" data-stop="1">
      ${title !== null ? `<div class="mh">${o.icon ? `<div class="pi" style="width:30px;height:30px;border-radius:8px;display:grid;place-items:center;background:var(--accent-soft);color:var(--accent-text);font-size:16px">${I(o.icon)}</div>` : ""}<h3>${title}</h3>${o.head || ""}${ibtn("X", "Close", { set: "dlg=" })}</div>` : ""}
      ${body}
      ${foot ? `<div class="mf">${foot}</div>` : ""}
    </div></div>`;
}
function table(cols, rows, o = {}) {
  const th = cols.map((c) => `<th class="${c.num ? "num" : ""}" style="${c.w ? `width:${c.w}` : ""}">${c.label}</th>`).join("");
  const tr = rows.map((r, i) => `<tr class="${o.sel === i ? "sel" : ""}">${r.map((c, j) => `<td class="${cols[j].num ? "num" : ""} ${cols[j].mono ? "mono" : ""}">${c}</td>`).join("")}</tr>`).join("");
  return `<div class="tbl-wrap" ${attrs({ style: o.style })}><table class="tbl"><thead><tr>${th}</tr></thead><tbody>${tr}</tbody></table></div>`;
}
function disclosure(title, open, inner, o = {}) {
  return `<div class="col" style="gap:8px"><div class="row" style="gap:6px;cursor:pointer">${I(open ? "CARET_DOWN" : "CARET_RIGHT", "faint")}<span class="section-title">${title}</span>${o.count !== undefined ? pill(o.count) : ""}${o.end ? `<span class="spacer"></span>${o.end}` : ""}</div>${open ? inner : ""}</div>`;
}
