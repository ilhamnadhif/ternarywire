// HTTP / GraphQL request screen: request on the left, response on the right.

const REQS = {
  list: { crumbs: ["Bookstore API", "Books"], name: "List books", m: "GET", url: "{{baseUrl}}/books?page=1&limit={{pageSize}}&sort=title" },
  create: { crumbs: ["Bookstore API", "Books"], name: "Create book", m: "POST", url: "{{baseUrl}}/books" },
  me: { crumbs: ["Bookstore API", "Auth"], name: "Current user", m: "GET", url: "{{baseUrl}}/me" },
  search: { crumbs: ["Bookstore API", "Books"], name: "Search catalog", m: "POST", url: "{{baseUrl}}/graphql", gql: true },
  scratch: { crumbs: [], name: "Untitled request", m: "GET", url: "", scratch: true },
};

// Side by side needs room for two usable panes (~440 px each); below that the response goes under the request.
function sideBySide() { return S.layout === "side" && benchW() >= 880; }

/* ------------------------------------------------------------------ head */
function reqTitleRow(r) {
  const crumbs = r.scratch
    ? `<span class="pill outline">Unsaved</span>`
    : r.crumbs.map((c) => `<span>${c}</span><span class="sep">${I("CARET_RIGHT")}</span>`).join("");
  return `<div class="row" style="height:46px;padding:0 12px 0 16px;border-bottom:1px solid var(--border);flex:none">
    <div class="crumbs grow">${crumbs}<span class="cur" title="Double-click to rename">${r.name}</span>
      ${r.dirty ? '<span class="dot" style="background:var(--accent);width:7px;height:7px;margin-left:4px" title="Unsaved changes"></span>' : ""}</div>
    ${ibtn("CODE", "Code snippet", { dlg: "snippet" })}
    ${btn(r.scratch ? "Save…" : "Save", { icon: "FLOPPY_DISK", size: "sm", dlg: r.scratch ? "save" : undefined, title: "Save (⌘S)" })}
    ${ibtn("DOTS_THREE", "More actions", { menu: "req-more" })}
  </div>`;
}
function urlRow(r) {
  const running = S.resState === "loading";
  return `<div class="row" style="height:56px;padding:0 12px;flex:none;gap:8px;border-bottom:1px solid var(--border)">
    <div class="urlbar ${S.urlFocus ? "focus" : ""}">
      <div class="mpick m-${r.m}" data-menu="method">${r.m}${I("CARET_DOWN", "car")}</div>
      <div class="url" title="${esc(r.url)}">${r.url ? urlHtml(r.url) : '<span class="faint" style="font-family:Inter">Enter a URL or paste a cURL command</span>'}</div>
    </div>
    ${running
      ? btn("Cancel", { icon: "STOP", size: "lg", title: "Cancel (⌘↩)" })
      : `<button class="btn primary lg" title="Send (⌘↩)" style="height:38px;padding:0 16px 0 14px;border-radius:9px">${I("PAPER_PLANE_RIGHT")}Send</button>`}
  </div>`;
}

/* ------------------------------------------------------------------ request tabs */
function reqTabs(r) {
  const isGql = r.gql || S.bodyMode === "graphql";
  const n = (v) => (r.scratch ? undefined : v); // a new request has nothing set yet
  return tabs("reqTab", S.reqTab, [
    ["params", "Params", { count: n(r.m === "GET" ? 3 : undefined) }],
    ["auth", "Auth", { dot: S.screen === "http-auth" }],
    ["headers", "Headers", { count: n(2) }],
    ["body", "Body", { dot: !r.scratch && (r.m !== "GET" || isGql) }],
    ["scripts", "Scripts", { count: n(1) }],
    ["settings", "Settings"],
    ["docs", "Docs", { count: n(2) }],
  ]);
}
const toolbar = (left, right = "") => `<div class="row" style="height:40px;flex:none;padding:0 12px;gap:8px">${left}<span class="spacer"></span>${right}</div>`;
const paneBody = (inner, o = {}) => `<div class="scroll grow pbody" style="padding:${o.pad ?? "4px 12px 16px"};display:flex;flex-direction:column;gap:${o.gap ?? 14}px">${inner}</div>`;

function reqContent(r) {
  switch (S.reqTab) {
    case "params": return paramsTab(r);
    case "auth": return authTab();
    case "headers": return headersTab();
    case "body": return bodyTab(r);
    case "scripts": return scriptsTab();
    case "settings": return reqSettingsTab();
    case "docs": return docsTab();
  }
  return "";
}

function paramsTab(r) {
  const rows = r.m === "GET" && r.url
    ? [[true, "page", "1", "Page number, starting at 1"], [true, "limit", "{{pageSize}}", ""], [true, "sort", "title", "title | price | author"], [false, "inStock", "true", "Only books in stock"]]
    : [];
  return toolbar(`<span class="section-title">Query parameters</span>`, btn(S.bulk ? "Key-value edit" : "Bulk edit", { kind: "ghost", size: "sm", icon: S.bulk ? "TABLE" : "TEXT_ALIGN_LEFT", set: "bulk=!" }))
    + paneBody((S.bulk
      ? code(["page:1", "limit:{{pageSize}}", "sort:title", '<span class="c">// inStock:true</span>'].map((l) => vars(l).replace("&lt;", "<")), { style: "border:1px solid var(--border);border-radius:8px" }) + hint("One entry per line as <span class=mono>key:value</span>. Prefix with <span class=mono>//</span> to disable.")
      : kv(rows))
      + `<div class="col" style="gap:8px;margin-top:6px"><span class="section-title">Path variables</span>${hint("Write <span class='mono'>:name</span> in the URL path to add one; its value is filled in here.")}</div>`, { pad: "0 12px 16px" });
}

function headersTab() {
  return toolbar(`<span class="section-title">Request headers</span>`, btn("Bulk edit", { kind: "ghost", size: "sm", icon: "TEXT_ALIGN_LEFT" }))
    + paneBody(kv([[true, "Accept", "application/json", ""], [true, "X-Client", "ternarywire/{{$appVersion}}", "Sent to the access log"]])
      + disclosure("Auto-generated headers", true, kv([
        [true, "User-Agent", "TernaryWire/0.2.3", "From Settings › Requests"],
        [true, "Cache-Control", "no-cache", "From Settings › Requests"],
        [true, "Host", '<span class="faint">calculated when sent</span>', "", { lock: true, html: true }],
        [true, "Accept-Encoding", "gzip, deflate, br", "", { lock: true }],
        [true, "Authorization", '<span class="faint">Bearer ••••••• (OAuth 2.0, inherited)</span>', "", { lock: true, html: true }],
      ], { noadd: true }) + hint("Headers you set above override these. Cookies from the jar are added automatically."), { count: 5 }), { pad: "0 12px 16px" });
}

/* body ------------------------------------------------------------------ */
function bodyTab(r) {
  const mode = r.gql ? "graphql" : r.m === "GET" && S.screen === "http" && S.bodyMode === "raw" ? "none" : S.bodyMode;
  const modes = seg("bodyMode", mode, [["none", "None"], ["form", "Form"], ["urlenc", "URL-encoded"], ["raw", "Raw"], ["binary", "Binary"], ["graphql", "GraphQL"]], { sm: true });
  if (mode === "graphql") return graphqlBody(modes);
  let right = "", inner = "";
  if (mode === "raw") {
    right = `${btn("Beautify", { kind: "ghost", size: "sm", icon: "SPARKLE" })}${selectbox("JSON", { sm: true, icon: "BRACKETS_CURLY" })}`;
    inner = `<div class="well grow" style="overflow:hidden;display:flex;margin:0 12px 12px">${code(jsonLines({ title: "Bumi Manusia", author: "Pramoedya Ananta Toer", price: 15.5, inStock: true, tags: ["novel", "classic"], publisherId: "{{publisherId}}" }), { style: "flex:1;background:transparent", cur: 2 })}</div>`;
    inner = inner.replace('"{{publisherId}}"', '"<span class="var">{{publisherId}}</span>"');
    return toolbar(modes, right) + inner;
  }
  if (mode === "none") inner = empty("FILE", "This request does not have a body", "Choose a body type above to send data with the request.", "", { style: "flex:1" });
  if (mode === "form") {
    inner = paneBody(`<div class="kv"><div class="kr h" style="grid-template-columns:30px 1fr 74px 1.3fr 30px"><div>${chk(true)}</div><div>Key</div><div>Type</div><div>Value</div><div></div></div>
      ${[["title", "Text", "Bumi Manusia"], ["cover", "File", `${I("FILE", "faint")} cover.jpg · 182 KB`], ["author", "Text", "{{author}}"]].map(([k, t, v]) => `<div class="kr" style="grid-template-columns:30px 1fr 74px 1.3fr 30px"><div>${chk(true)}</div><div class="mono">${k}</div><div>${selectbox(t, { sm: true, quiet: true })}</div><div class="${t === "Text" ? "mono" : ""}">${t === "Text" ? vars(v) : v}</div><div>${I("TRASH")}</div></div>`).join("")}
      <div class="kr ph" style="grid-template-columns:30px 1fr 74px 1.3fr 30px"><div></div><div>Key</div><div></div><div>Value</div><div></div></div></div>
      ${hint("Sent as multipart/form-data. Switch a field to File to upload files.")}`, { pad: "0 12px 16px" });
  }
  if (mode === "urlenc") inner = paneBody(kv([[true, "grant_type", "password", ""], [true, "username", "{{user}}", ""], [true, "password", "{{vault:bookstore-pass}}", ""]]) + hint("Sent as application/x-www-form-urlencoded."), { pad: "0 12px 16px" });
  if (mode === "binary") inner = paneBody(card(`<div class="row" style="gap:12px"><div class="ei" style="width:40px;height:40px;border-radius:10px;display:grid;place-items:center;background:var(--accent-soft);color:var(--accent-text);font-size:20px">${I("FILE_ARROW_UP")}</div>
      <div class="grow"><div class="med">books-2026.csv</div><div class="hint">48.2 KB · the file's bytes are sent as the request body</div></div>${btn("Change…", { size: "sm" })}${ibtn("X", "Remove file", { sm: true })}</div>`) + input("~/Downloads/books-2026.csv", { mono: true, sm: true }), { pad: "0 12px 16px" });
  return toolbar(modes, right) + inner;
}

function graphqlBody(modes) {
  const q = [
    '<span class="b">query</span> <span class="syn-fn">Search</span>(<span class="k">$q</span>: <span class="tg">String</span>!, <span class="k">$first</span>: <span class="tg">Int</span> = <span class="n">10</span>) {',
    '  <span class="syn-fn">search</span>(query: <span class="k">$q</span>, first: <span class="k">$first</span>) {',
    "    totalCount",
    "    nodes {",
    "      id",
    "      title",
    "      author { name }",
    "      price",
    "    }",
    "  }",
    "}",
  ];
  const explorer = `<div class="col" style="width:224px;flex:none;border-left:1px solid var(--border);gap:0;background:var(--well)">
    <div class="row" style="height:36px;padding:0 10px;border-bottom:1px solid var(--border)"><span class="section-title grow">Schema</span>${pill("42 types", "ok")}${ibtn("X", "Close schema", { sm: true })}</div>
    <div style="padding:8px">${search("Filter fields", { sm: true })}</div>
    <div class="scroll grow" style="padding:0 6px 8px;font-size:12px">
      ${[["Query", ["search(query, first): SearchResult!", "book(id): Book", "books(page, limit): [Book!]!", "me: User"]], ["Mutation", ["createBook(input): Book!", "deleteBook(id): Boolean!"]], ["Subscription", ["bookAdded: Book!"]]]
        .map(([g, fs], gi) => `<div class="row" style="height:26px;padding:0 4px;gap:4px">${I(gi < 2 ? "CARET_DOWN" : "CARET_RIGHT", "faint")}<span class="med">${g}</span><span class="faint xs">${fs.length}</span></div>` + (gi < 2 ? fs.map((f) => {
          const [n, t] = f.split(": ");
          return `<div class="row" style="height:26px;padding:0 4px 0 18px;gap:6px">${ibtn("PLUS", "Insert an operation for this field", { sm: true })}<span class="mono ellipsis" style="font-size:11.5px">${n.split("(")[0]}</span><span class="faint mono ellipsis" style="font-size:11px">${t}</span></div>`;
        }).join("") : "")).join("")}
    </div></div>`;
  return toolbar(modes, `${btn("Prettify", { kind: "ghost", size: "sm", icon: "SPARKLE" })}${ibtn("BOOK_OPEN", "Schema explorer", { sm: true, on: true })}`)
    + `<div class="row" style="flex:1;min-height:0;align-items:stretch;gap:0;margin:0 12px 12px;border:1px solid var(--border);border-radius:8px;overflow:hidden">
      <div class="col grow" style="gap:0">
        <div class="row" style="height:30px;padding:0 10px;background:var(--well);border-bottom:1px solid var(--border)"><span class="eyebrow">Query</span><span class="spacer"></span><span class="faint xs">${I("CHECK_CIRCLE", "ok")} Valid against schema</span></div>
        ${code(q, { style: "flex:1.4" })}
        <div class="row" style="height:30px;padding:0 10px;background:var(--well);border-top:1px solid var(--border);border-bottom:1px solid var(--border)"><span class="eyebrow">Variables</span><span class="spacer"></span><span class="faint xs">JSON</span></div>
        ${code(['<span class="p">{</span> <span class="k">"q"</span><span class="p">:</span> <span class="s">"rust"</span><span class="p">,</span> <span class="k">"first"</span><span class="p">:</span> <span class="n">5</span> <span class="p">}</span>'], { style: "flex:.5" })}
      </div>${explorer}</div>`;
}

/* auth ------------------------------------------------------------------ */
const AUTH_TYPES = [["inherit", "Inherit from parent"], ["none", "No auth"], "-", ["apikey", "API Key"], ["bearer", "Bearer Token"], ["basic", "Basic Auth"], ["digest", "Digest Auth"], ["jwt", "JWT Bearer"], ["oauth1", "OAuth 1.0"], ["oauth2", "OAuth 2.0"], ["hawk", "Hawk"], ["aws", "AWS Signature"], ["ntlm", "NTLM"], ["edgegrid", "Akamai EdgeGrid"], ["asap", "ASAP (Atlassian)"]];
function secretField(v, o = {}) {
  return `<div class="row" style="gap:0;position:relative">${input(v, { mono: true, sm: o.sm, style: "width:100%;padding-right:30px" })}<span style="position:absolute;right:4px">${ibtn("EYE", "Show value", { sm: true })}</span></div>`;
}
function authTab() {
  const t = S.authType;
  const label = (AUTH_TYPES.find((a) => a[0] === t) || [])[1];
  const typeRow = toolbar(`<span class="section-title">Auth type</span>${selectbox(label, { sm: true, icon: "KEY", menu: "auth-type", style: "min-width:190px" })}`, t === "oauth2" ? pill("Token active · 52 min", "ok", { dot: true }) : "");
  let form = "";
  if (t === "inherit") {
    form = card(`<div class="row" style="gap:12px;align-items:flex-start"><div class="ptitle"><div class="pi">${I("SHIELD_CHECK")}</div></div>
      <div class="col" style="gap:4px"><div class="row"><span class="semi">Inherited from “Bookstore API”</span>${pill("OAuth 2.0", "acc")}</div>
      <div class="hint">This request uses OAuth 2.0 from “Bookstore API”. Change it in the collection settings.</div>
      <div>${btn("Open collection auth", { size: "sm", icon: "ARROW_SQUARE_OUT", go: "container" })}</div></div></div>`);
  } else if (t === "bearer") {
    form = field("Token", secretField("{{vault:bookstore-token}}"), "Sent as <span class=mono>Authorization: Bearer &lt;token&gt;</span>.");
  } else if (t === "oauth2") {
    form = `${card(`<div class="row" style="gap:10px"><span class="section-title grow">Current token</span>${pill("Active", "ok", { dot: true })}</div>
        <div class="row mono" style="margin-top:10px;padding:8px 10px;border-radius:7px;background:var(--well);border:1px solid var(--border);font-size:12px"><span class="faint">Bearer</span><span class="grow ellipsis">eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1XzE5MiIsInNjb3Bl…</span>${ibtn("COPY", "Copy token", { sm: true })}</div>
        <div class="row small muted" style="margin-top:10px;gap:14px"><span>${I("CLOCK")} Expires in 52 min</span><span>${I("ARROWS_CLOCKWISE")} Renewed automatically</span></div>
        <div class="row" style="margin-top:8px;gap:6px">${["books:read", "books:write", "profile"].map((s) => pill(s, "", { outline: true })).join("")}</div>
        <div class="row" style="margin-top:12px">${btn("Get new access token", { size: "sm", icon: "ARROWS_CLOCKWISE" })}${btn("Clear token", { size: "sm", kind: "ghost", icon: "TRASH" })}<span class="spacer"></span><span class="faint xs">Kept in the Keychain</span></div>`)}
      <div class="grid2">
        ${field("Grant type", selectbox("Authorization Code (with PKCE)", { sm: true }))}
        ${field("Add token to", seg("o2to", "hdr", [["hdr", "Header"], ["url", "URL"]], { sm: true }))}
        ${field("Auth URL", input("https://auth.bookstore.dev/authorize", { mono: true, sm: true }))}
        ${field("Access token URL", input("https://auth.bookstore.dev/token", { mono: true, sm: true }))}
        ${field("Client ID", input("{{clientId}}", { mono: true, sm: true }))}
        ${field("Client secret", secretField("{{vault:bookstore-client}}", { sm: true }))}
        ${field("Scope", input("books:read books:write profile", { mono: true, sm: true }))}
        ${field("Callback URL", input("http://127.0.0.1:7777/callback", { mono: true, sm: true }))}
      </div>
      <div class="row" style="justify-content:space-between"><div><div class="med">Auto-refresh token</div><div class="hint">Uses the refresh token before the access token expires.</div></div>${sw(true)}</div>
      ${disclosure("Advanced", false, "", { count: 6, end: '<span class="faint xs">Most servers work with the defaults</span>' })}`;
  } else {
    form = `<div class="grid2">${field("Username", input("{{user}}", { mono: true, sm: true }))}${field("Password", secretField("{{vault:pass}}", { sm: true }))}</div>`;
  }
  return typeRow + paneBody(`${hint(t === "oauth2" ? "Gets a token from an authorization server and sends it with the request." : "")}${form}`, { pad: "0 12px 16px" });
}

/* scripts ---------------------------------------------------------------- */
function scriptsTab() {
  const pre = ["// Runs before the request is sent", "const ts = Date.now();", 'pm.variables.set("ts", ts);'];
  const post = [
    'pm.test("Status code is 200", () => {',
    "  pm.response.to.have.status(200);",
    "});",
    "",
    'pm.test("Every book has a title", () => {',
    "  const { data } = pm.response.json();",
    "  pm.expect(data.every((b) => b.title)).to.be.true;",
    "});",
    "",
    "const left = pm.response.json().total - pm.response.json().data.length;",
    'console.log("pages left:", Math.ceil(left / 20));',
  ];
  const src = S.scriptEv === "pre" ? pre : post;
  return toolbar(seg("scriptEv", S.scriptEv, [["pre", "Pre-request"], ["post", "Post-response"]], { sm: true }),
    `${S.scriptEv === "post" ? btn("Generate tests", { kind: "ghost", size: "sm", icon: "TEST_TUBE", dlg: "gen-tests" }) : ""}${btn("Snippets", { size: "sm", icon: "LIGHTNING", menu: "snippets", caret: true })}`)
    + `<div class="well grow" style="margin:0 12px 8px;display:flex;overflow:hidden">${code(jsHl(src.join("\n")), { style: "flex:1;background:transparent", cur: 5 })}</div>
    <div class="row" style="height:30px;flex:none;padding:0 14px 6px;gap:8px">${I("INFO", "faint")}<span class="hint">JavaScript with the Postman-compatible <span class="mono">pm</span> API · type <span class="mono">pm.</span> for completions</span></div>`;
}

/* settings / docs ---------------------------------------------------------- */
function tri(cur, def) {
  return seg("tri", cur, [["def", `Default (${def})`], ["on", "On"], ["off", "Off"]], { sm: true });
}
function reqSettingsTab() {
  return toolbar(hint("These apply to this request only and override the app settings."), btn("Reset to defaults", { kind: "ghost", size: "sm", icon: "ARROW_COUNTER_CLOCKWISE" }))
    + paneBody(
      group("Redirects", [
        frow("Follow redirects", "Go on to the address a 3xx response points to.", tri("def", "on")),
        frow("Follow original HTTP method", "Keep POST on 301/302 instead of switching to GET.", tri("off", "off").replace('class=""', 'class=""')),
        frow("Keep Authorization header", "Send it again when redirected to another host.", tri("def", "off")),
        frow("Maximum redirects", "", input("10", { sm: true, style: "width:80px;text-align:right" })),
      ])
      + group("Connection", [
        frow("Verify SSL certificates", "", tri("def", "on")),
        frow("Request timeout", "0 waits forever.", `<div class="row" style="gap:6px">${input("", { sm: true, ph: "30000", style: "width:90px;text-align:right" })}<span class="faint">ms</span></div>`),
        frow("HTTP version", "", seg("hv", "auto", [["auto", "Auto"], ["h1", "HTTP/1.x"], ["h2", "HTTP/2"]], { sm: true })),
      ])
      + group("Request", [
        frow("Encode URL automatically", "", tri("def", "on")),
        frow("Disable cookie jar", "Don't send or store cookies for this request.", tri("def", "off")),
      ]), { pad: "0 12px 16px", gap: 18 });
}
function docsTab() {
  return toolbar(`<span class="section-title">Description</span>`, seg("docv", "edit", [["edit", "Write"], ["prev", "Preview"]], { sm: true }))
    + paneBody(textarea("Lists books, newest first. Supports paging with `page` and `limit`, and sorting by `title`, `price` or `author`.\n\nRequires the `books:read` scope.", { rows: 5 })
      + hint("Markdown. Shown in the generated collection documentation.")
      + `<div class="row"><span class="section-title">Examples</span>${pill("2")}<span class="spacer"></span><span class="hint">Also served by the mock server</span></div>`
      + [["200", "ok", "Page of books", "JSON", true], ["404", "warn", "Unknown page", "JSON", false]].map(([c, k, n, l, open]) => card(`<div class="row">${I(open ? "CARET_DOWN" : "CARET_RIGHT", "faint")}${pill(c, k)}<span class="med grow">${n}</span><span class="faint xs">${l}</span>${ibtn("TRASH", "Delete example", { sm: true })}</div>${open ? `<div style="margin-top:10px" class="well">${jsonCode({ data: [{ id: 1, title: "The Pragmatic Programmer" }], total: 1 }, { gutter: false })}</div>` : ""}`, { style: "padding:10px 12px" })).join(""), { pad: "0 12px 16px", gap: 10 });
}

/* ------------------------------------------------------------------ response */
function resStatusRow() {
  const st = S.resState;
  let left, right = "";
  if (st === "empty") left = `<span class="semi">Response</span><span class="faint">·</span><span class="hint">Not sent yet</span>`;
  else if (st === "loading") left = `<span class="pill info lg">${I("CIRCLE_NOTCH", "spin")}Sending…</span><span class="mono muted" style="font-size:12px">0.84 s</span>`;
  else if (st === "error") left = `<span class="pill err lg">${I("WARNING_CIRCLE")}Connection refused</span><span class="rmeta"><span class="mt">${I("TIMER")}3 ms</span></span>`;
  else {
    left = `${statusPill(200, "OK")}<span class="rmeta">
      <span class="mt" data-menu="timing" title="Timing">${I("TIMER")}142 ms</span>
      <span class="mt" title="Headers 263 B · Body 693 B">${I("ARROW_LINE_DOWN")}956 B</span>
      <span class="mt" data-set="resTab=tests" style="color:var(--ok)">${I("CHECK_CIRCLE")}3/3 tests</span></span>`;
  }
  right = `${st === "ok" ? ibtn("BOOKMARK_SIMPLE", "Save as example", { sm: true }) : ""}${ibtn("DOTS_THREE", "More actions", { sm: true, menu: "res-more" })}<div class="vdiv"></div>${ibtn("ARROWS_OUT", "Maximize response", { sm: true })}${ibtn(sideBySide() ? "CARET_RIGHT" : "CARET_DOWN", "Minimize response", { sm: true })}`;
  return `<div class="row" style="height:56px;padding:0 10px 0 14px;flex:none;gap:8px;border-bottom:1px solid var(--border)">${left}<span class="spacer"></span>${right}</div>`;
}
function resTabs() {
  const has = S.resState === "ok";
  return tabs("resTab", S.resTab, [
    ["body", "Body"],
    ["headers", "Headers", { count: has ? 7 : undefined }],
    ["cookies", "Cookies", { count: has ? 1 : undefined }],
    ["tests", "Tests", { count: has ? "3/3" : undefined, countKind: "ok" }],
    ["details", "Details"],
    ...(has ? [["compare", "Compare", { icon: "GIT_DIFF" }]] : []),
  ]);
}
function resContent() {
  if (S.resState === "empty") return `<div class="grow col" style="justify-content:center">${empty("PAPER_PLANE_RIGHT", "No response yet", "Enter a URL and press Send to see the response here.", `<span class="row hint">${kbd("⌘")}${kbd("↩")} to send</span>`)}
    <div style="margin:18px auto 0;width:min(360px,80%)"><div class="eyebrow" style="margin-bottom:6px">Last sent from History</div>
    ${[["GET", "/books?page=1&limit=20", 200, "09:41"], ["GET", "/books?tag=rust", 200, "Yesterday"]].map(([m, u, s, t]) => `<div class="row" style="height:30px;padding:0 8px;border-radius:6px;background:var(--well);margin-bottom:4px">${meth(m)}<span class="mono grow ellipsis" style="font-size:11.5px">${u}</span><span class="ok xs">${s}</span><span class="faint xs">${t}</span></div>`).join("")}</div></div>`;
  if (S.resState === "loading") return `<div class="grow col" style="justify-content:center">${empty("CIRCLE_NOTCH", "Sending request…", "Waiting for the first byte from 127.0.0.1:18795", btn("Cancel", { icon: "STOP", size: "sm" }), {})}</div>`.replace('<div class="ei">', '<div class="ei"><span class="spin" style="display:inline-grid">').replace(`${ICON.CIRCLE_NOTCH}</i></div>`, `${ICON.CIRCLE_NOTCH}</i></span></div>`);
  if (S.resState === "error") return `<div class="grow col" style="justify-content:center;padding:20px">
      <div class="card" style="max-width:440px;margin:0 auto;padding:18px">
        <div class="row" style="gap:12px;align-items:flex-start"><div style="width:40px;height:40px;border-radius:50%;display:grid;place-items:center;background:var(--err-soft);color:var(--err);font-size:20px;flex:none">${I("PLUG")}</div>
        <div class="col" style="gap:4px"><div class="row"><span class="semi" style="font-size:14px">Could not send request</span>${pill("Connection", "err")}</div>
        <div class="muted">Nothing is listening on <span class="mono">127.0.0.1:18795</span> — the connection was refused.</div></div></div>
        <div class="col" style="gap:6px;margin-top:14px;font-size:12.5px">
          <div class="row">${I("CHECK", "faint")}<span>Is the server running? Start it, or check the port in <span class="var">{{baseUrl}}</span>.</span></div>
          <div class="row">${I("CHECK", "faint")}<span>Using a proxy? Check Settings › Proxy.</span></div>
          <div class="row">${I("CHECK", "faint")}<span>Switch environment with ${kbd("⌘E")} if this should go to Staging.</span></div>
        </div>
        <div class="row" style="margin-top:16px">${primary("Try again", { size: "sm", icon: "ARROW_CLOCKWISE" })}${btn("Show details", { size: "sm", kind: "ghost", icon: "CARET_DOWN" })}<span class="spacer"></span>${btn("Open console", { size: "sm", kind: "ghost", icon: "TERMINAL_WINDOW", set: "console=true" })}</div>
      </div></div>`;
  switch (S.resTab) {
    case "body": return resBody();
    case "headers": return toolbar(`<span class="hint">7 headers</span>`, btn("Copy all", { kind: "ghost", size: "sm", icon: "COPY" })) + paneBody(table([{ label: "Name", w: "38%" }, { label: "Value" }], RES_HEADERS.map(([k, v]) => [`<span class="mono muted">${k}</span>`, `<span class="mono">${esc(v)}</span>`])), { pad: "0 12px 16px" });
    case "cookies": return toolbar(`<span class="hint">1 cookie set by this response</span>`, btn("Open cookie jar", { kind: "ghost", size: "sm", icon: "COOKIE", dlg: "cookies" })) + paneBody(table([{ label: "Name" }, { label: "Value" }, { label: "Attributes" }], [[`<span class="mono">session</span>`, `<span class="mono">s%3Ab2f…9a</span>`, `<span class="muted">Path=/ · HttpOnly · SameSite=Lax · Session</span>`]]), { pad: "0 12px 16px" });
    case "tests": return resTests();
    case "details": return resDetails();
    case "compare": return resCompare();
  }
  return "";
}

function resBody() {
  const views = seg("bodyView", S.bodyView, [["pretty", "Pretty"], ["raw", "Raw"], ["preview", "Preview"], ["tree", "Tree"]], { sm: true });
  const tools = `${search("Search", { sm: true, style: "width:130px" })}${ibtn("FUNNEL", "Filter with JSONPath", { sm: true, on: S.filter, set: "filter=!" })}${ibtn("ARROWS_IN_LINE_VERTICAL", "Collapse all", { sm: true })}${ibtn("TEXT_ALIGN_LEFT", "Wrap lines", { sm: true })}${ibtn("COPY", "Copy", { sm: true })}${ibtn("DOWNLOAD_SIMPLE", "Save response to file", { sm: true })}`;
  const filterBar = S.filter ? `<div class="row" style="padding:0 12px 8px;gap:8px;flex:none"><label class="search grow">${I("FUNNEL")}<input class="input sm mono focus" value="$.data[?(@.inStock)].title"/></label>${pill("3 matches", "ok")}${ibtn("X", "Clear", { sm: true, set: "filter=false" })}</div>` : "";
  let content;
  if (S.filter) {
    content = jsonCode(["The Pragmatic Programmer", "Designing Data-Intensive Applications", "Rust in Action"]);
  } else if (S.bodyView === "tree") {
    content = `<div style="padding:8px 10px;font-size:12.5px" class="col">${treeView()}</div>`;
  } else if (S.bodyView === "raw") {
    content = code([`<span class="s" style="color:var(--syn-text)">${esc(JSON.stringify(BOOKS))}</span>`], { cls: "wrap" }).replace('white-space: pre', "");
  } else if (S.bodyView === "preview") {
    content = `<div style="padding:12px">${banner("info", "INFO", "JSON has no visual preview. Tree shows it as an outline; <a class='accent'>Visualize</a> renders a <span class=mono>pm.visualizer</span> template.")}</div>`;
  } else if (S.screen === "http-graphql") {
    content = jsonCode({ data: { search: { totalCount: 2, nodes: [{ id: "4", title: "Rust in Action", author: { name: "Tim McNamara" }, price: 39.99 }, { id: "7", title: "Programming Rust", author: { name: "Jim Blandy" }, price: 49.5 }] } } }, { cur: 3 });
  } else {
    const lines = foldJson(jsonLines(BOOKS), 3, "6 keys"); // fold the 3rd book
    content = code(lines, { cur: 3 });
  }
  return toolbar(views, tools) + filterBar + `<div class="scroll grow" style="margin:0 12px 12px;border:1px solid var(--border);border-radius:8px;background:var(--surface)">${content}</div>`;
}
// Folds the n-th object that opens at depth 2 (a book) into one line with a chip.
function foldJson(lines, n, label) {
  const txt = (l) => (typeof l === "string" ? l : l.t);
  const ind = (l) => txt(l).match(/^ */)[0].length;
  let seen = 0;
  for (let i = 0; i < lines.length; i++) {
    if (typeof lines[i] !== "string" && ind(lines[i]) === 4 && /\{<\/span>$/.test(txt(lines[i])) && ++seen === n) {
      let j = i + 1;
      while (j < lines.length && !(ind(lines[j]) === 4 && /^ *<span class="p">\}/.test(txt(lines[j])))) j++;
      const comma = /,<\/span>$/.test(txt(lines[j])) ? '<span class="p">,</span>' : "";
      lines.splice(i, j - i + 1, { t: txt(lines[i]) + `<span class="fold-badge">…</span><span class="p">}</span>${comma} <span class="c" style="font-style:normal">${label}</span>`, fold: true });
      break;
    }
  }
  return lines;
}
function treeView() {
  const row = (d, caret, k, v, cls = "") => `<div class="row" style="height:24px;gap:6px;padding-left:${d * 16}px">${caret ? I(caret, "faint") : '<span style="width:13px"></span>'}<span class="mono k" style="font-size:12px">${k}</span>${v ? `<span class="mono ${cls}" style="font-size:12px">${v}</span>` : ""}</div>`;
  return row(0, "CARET_DOWN", "root", '<span class="faint">{ data, page, limit, total }</span>')
    + row(1, "CARET_DOWN", "data", '<span class="faint">[4 items]</span>')
    + row(2, "CARET_DOWN", "0", '<span class="faint">{ id, title, author, price, … }</span>')
    + row(3, "", "id", "1", "n") + row(3, "", "title", '"The Pragmatic Programmer"', "s") + row(3, "", "author", '"Andrew Hunt"', "s")
    + row(3, "", "price", "42.5", "n") + row(3, "", "inStock", "true", "b") + row(3, "CARET_RIGHT", "tags", '<span class="faint">[2 items]</span>')
    + row(2, "CARET_RIGHT", "1", '<span class="faint">{ id, title, author, price, … }</span>') + row(2, "CARET_RIGHT", "2", '<span class="faint">{ id, title, author, price, … }</span>')
    + row(2, "CARET_RIGHT", "3", '<span class="faint">{ id, title, author, price, … }</span>')
    + row(1, "", "page", "1", "n") + row(1, "", "limit", "20", "n") + row(1, "", "total", "5", "n");
}
function resTests() {
  const tests = [["ok", "Status code is 200", ""], ["ok", "Every book has a title", ""], ["ok", "Response time is below 500 ms", ""]];
  return toolbar(`${pill("3 passed", "ok", { icon: "CHECK" })}${pill("0 failed")}${pill("0 skipped")}`, btn("Re-run tests", { kind: "ghost", size: "sm", icon: "ARROW_CLOCKWISE" }))
    + paneBody(`<div class="bar" style="height:4px"><span style="width:100%;background:var(--ok)"></span></div>
      <div>${seg("testFilter", S.testFilter, [["all", "All 3"], ["pass", "Passed 3"], ["fail", "Failed 0"], ["skip", "Skipped 0"]], { sm: true })}</div>
      <div class="tbl-wrap">${tests.map(([k, n]) => `<div class="row" style="height:38px;padding:0 12px;border-bottom:1px solid var(--border)">${I("CHECK_CIRCLE", "ok")}<span class="grow">${n}</span>${pill("PASS", "ok")}</div>`).join("")}
      <div class="row" style="height:38px;padding:0 12px">${I("PLUS", "faint")}<span class="hint">Add tests in Scripts › Post-response, or right-click a value in the body.</span></div></div>`, { pad: "0 12px 16px", gap: 12 });
}
function resDetails() {
  const wf = [["DNS lookup", 0, 1, "1 ms", "var(--info)"], ["TCP connect", 1, 2, "1 ms", "var(--m-head)"], ["Waiting (TTFB)", 2, 82, "118 ms", "var(--accent)"], ["Content download", 82, 100, "22 ms", "var(--ok)"]];
  return paneBody(`${banner("warn", "WARNING", "Undefined variable <span class='var bad'>{{pageSize}}</span> was sent as typed. Define it in Local or the collection.")}
    ${card(`<div class="row"><span class="section-title grow">Timing</span><span class="mono semi">142 ms</span></div>
      <div class="waterfall" style="margin-top:12px">${wf.map(([n, a, b, t, c]) => `<span class="muted">${n}</span><div class="wb"><span style="left:${a}%;width:${Math.max(b - a, 1)}%;background:${c}"></span></div><span class="mono num" style="text-align:right">${t}</span>`).join("")}</div>`)}
    ${card(`<div class="section-title" style="margin-bottom:8px">Response</div>${[["Status", statusPill(200, "OK")], ["HTTP version", "HTTP/1.1"], ["Remote address", '<span class="mono">127.0.0.1:18795</span>'], ["Security", `${I("LOCK_SIMPLE", "faint")} Not encrypted (plain HTTP)`], ["Size", "956 B · headers 263 B · body 693 B"], ["URL", '<span class="mono">http://127.0.0.1:18795/books?page=1&amp;limit={{pageSize}}&amp;sort=title</span>']].map(([k, v]) => `<div class="row" style="min-height:28px"><span class="muted" style="width:120px;flex:none">${k}</span><span class="grow ellipsis">${v}</span></div>`).join("")}`)}
    ${card(`<div class="row" style="margin-bottom:8px">${meth("GET")}<span class="section-title grow">Request sent</span>${btn("Copy as cURL", { size: "sm", kind: "ghost", icon: "TERMINAL" })}</div>${table([{ label: "Header", w: "38%" }, { label: "Value" }], [["Accept", "application/json"], ["Authorization", "Bearer ••••••••"], ["User-Agent", "TernaryWire/0.2.3"]].map(([a, b]) => [`<span class="mono muted">${a}</span>`, `<span class="mono">${b}</span>`]))}`)}`, { pad: "4px 12px 16px", gap: 12 });
}
function resCompare() {
  const rows = [
    [" ", 12, 12, '    "price": 39.99,'],
    ["-", 13, "", '    "inStock": false,'],
    ["+", "", 13, '    "inStock": true,'],
    [" ", 14, 14, '    "tags": ["rust"]'],
    ["⋯", "", "", "18 unchanged lines"],
    ["-", 33, "", '  "total": 4'],
    ["+", "", 33, '  "total": 5'],
  ];
  return toolbar(`${seg("cmp", "body", [["body", "Body"], ["hdr", "Headers · 1"]], { sm: true })}<span class="ok xs semi">+2</span><span class="err xs semi">−2</span>`, ibtn("PUSH_PIN", "Keep this baseline", { sm: true }))
    + paneBody(`<div class="row" style="gap:18px;flex-wrap:wrap;padding:10px 12px;border:1px solid var(--border);border-radius:8px;font-size:12px">
        <span><span class="muted">Status</span> 200 → 200</span><span><span class="muted">Time</span> 118 → 142 ms <span class="warn">+24 ms</span></span><span><span class="muted">Size</span> 902 → 956 B <span class="muted">+54 B</span></span><span><span class="muted">Headers</span> 1 changed</span></div>
      <div class="tbl-wrap mono" style="font-size:12px">
        <div class="row" style="height:28px;padding:0 10px;background:var(--well);border-bottom:1px solid var(--border);font-family:Inter;font-size:11.5px"><span class="err">− Previous · 09:38:02</span><span class="spacer"></span><span class="ok">+ Current · 09:41:07</span></div>
        ${rows.map(([s, a, b, t]) => `<div class="row" style="height:22px;gap:0;${s === "-" ? "background:var(--err-soft)" : s === "+" ? "background:var(--ok-soft)" : ""}"><span class="faint" style="width:34px;text-align:right">${a}</span><span class="faint" style="width:34px;text-align:right">${b}</span><span style="width:22px;text-align:center" class="${s === "-" ? "err" : s === "+" ? "ok" : "faint"}">${s === "⋯" ? I("DOTS_THREE") : s}</span><span class="${s === "⋯" ? "faint" : ""}" style="white-space:pre">${s === "⋯" ? `<span style="font-family:Inter">${t}</span>` : esc(t)}</span></div>`).join("")}
      </div>`, { pad: "0 12px 16px", gap: 10 });
}

/* ------------------------------------------------------------------ view */
function httpView(key) {
  const r = REQS[key];
  const side = sideBySide();
  const reqPane = `<section class="pane" style="flex:${side ? "0 0 50%" : "1 1 50%"};${side ? "" : "min-height:0"}">${urlRow(r)}${reqTabs(r)}<div class="col grow" style="gap:0;min-height:0">${reqContent(r)}</div></section>`;
  const resPane = `<section class="pane grow" style="${side ? "" : "flex:1 1 50%;border-top:1px solid var(--border)"}">${resStatusRow()}${resTabs()}<div class="col grow" style="gap:0;min-height:0">${resContent()}</div></section>`;
  return reqTitleRow(r) + `<div class="panes" style="flex-direction:${side ? "row" : "column"}">${reqPane}${side ? '<div class="gutter" title="Drag to resize · double-click to reset"></div>' : ""}${resPane}</div>`;
}

Object.assign(MENUS, {
  method: () => placeMenu(menu([...["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map((m) => ({ meth: m === "DELETE" ? "DEL" : m === "OPTIONS" ? "OPT" : m, label: m, on: m === "GET" })), "-", { icon: "PENCIL_SIMPLE", label: "Custom…", sub: "PROPFIND" }]), { w: 180 }),
  "req-more": () => placeMenu(menu([{ icon: "CODE", label: "Code snippet", dlg: "snippet" }, { icon: "COPY", label: "Duplicate", kbd: "⌘D" }, { icon: "TERMINAL", label: "Copy as cURL" }, { icon: "FLOPPY_DISK", label: "Save as…", dlg: "save" }, "-", { icon: "TRASH", label: "Delete…", danger: true, dlg: "confirm" }]), { right: true, w: 210 }),
  "res-more": () => placeMenu(menu([{ icon: "BOOKMARK_SIMPLE", label: "Save as example" }, { icon: "TEST_TUBE", label: "Generate tests", dlg: "gen-tests" }, { icon: "ARROW_CLOCKWISE", label: "Re-run tests" }, "-", { icon: "PUSH_PIN", label: "Keep as compare baseline" }, { icon: "TERMINAL", label: "Copy as cURL (as sent)" }, { icon: "DOWNLOAD_SIMPLE", label: "Save response to file" }, { icon: "ARROW_SQUARE_OUT", label: "Open visualization in browser" }]), { right: true, w: 250 }),
  timing: () => placeMenu(`<div class="menu" style="padding:12px 14px;width:280px"><div class="row"><span class="section-title grow">Timing</span><span class="mono semi">142 ms</span></div>
    <div class="waterfall" style="margin-top:10px;grid-template-columns:100px 1fr 50px;font-size:11.5px">${[["DNS", 0, 1, "1 ms", "var(--info)"], ["Connect", 1, 2, "1 ms", "var(--m-head)"], ["TTFB", 2, 82, "118 ms", "var(--accent)"], ["Download", 82, 100, "22 ms", "var(--ok)"]].map(([n, a, b, t, c]) => `<span class="muted">${n}</span><div class="wb"><span style="left:${a}%;width:${Math.max(b - a, 1)}%;background:${c}"></span></div><span class="mono" style="text-align:right">${t}</span>`).join("")}</div></div>`),
  "auth-type": () => placeMenu(menu(AUTH_TYPES.map((a) => (a === "-" ? "-" : { label: a[1], check: a[0] === S.authType, set: `authType=${a[0]}` }))), { w: 220 }),
  snippets: () => placeMenu(menu(["Tests", { icon: "PLUS", label: "Status code is 200" }, { icon: "PLUS", label: "Response time is below 500 ms" }, { icon: "PLUS", label: "Response body: JSON value check" }, { icon: "PLUS", label: "Content-Type header is present" }, "Variables", { icon: "PLUS", label: "Set an environment variable" }, { icon: "PLUS", label: "Save token from response" }, "Flow", { icon: "PLUS", label: "Skip this request" }, { icon: "PLUS", label: "Set next request (runner)" }, "-", { icon: "BOOK_OPEN", label: "pm API reference", more: true }]), { right: true, w: 260 }),
});

/* ------------------------------------------------------------------ screens */
const N = (items) => `<h4>Keputusan desain</h4><ul>${items.map((i) => `<li>${i}</li>`).join("")}</ul>`;
const NE = (items) => `<h4>Implementasi egui</h4><ul>${items.map((i) => `<li>${i}</li>`).join("")}</ul>`;

screen({ id: "http", group: "HTTP request", label: "Request kiri · Response kanan", rail: "collections", sel: "list", tab: OPEN_TABS[0], view: () => httpView("list"),
  notes: N([
    "Request <b>kiri</b>, response <b>kanan</b>. Masing-masing panel berdiri sendiri: kiri berisi method + URL + Send lalu tab request; kanan berisi baris status lalu tab response.",
    "Baris URL (kiri) dan baris status (kanan) sama tinggi (56 px), tab strip kiri dan kanan juga sejajar (36 px) → garis horizontal menyambung di seluruh lebar, terlihat rapi seperti grid.",
    "Status, waktu, ukuran dan ringkasan test selalu terlihat di atas response. Klik waktu → popover timing; klik ‘3/3 tests’ → tab Tests.",
    "Area kerja (tab + view) adalah satu kartu yang mengambang di atas chrome. Sidebar dan rail tidak bersaing dengan konten.",
    "Breadcrumb + nama di baris judul penuh; aksi sekunder (Code, Save, ⋯) dikelompokkan di kanan. Tidak ada tombol primer lain selain Send.",
    "Pembagi bisa digeser (klik-dua-kali = reset 50/50). URL panjang memudar di ujung; fokus pada URL membuka editor URL penuh di atas panel.",
  ]) + NE([
    "<code>request_tab::show</code>: ganti layout jadi dua <code>Ui</code> kolom; baris URL pindah ke kolom kiri, <code>response_panel</code> mendapat baris status sendiri setinggi 56.",
    "Kartu kerja: <code>Frame::new().fill(canvas).stroke(border).corner_radius(10)</code> dengan margin 0/8/0/0 dari panel chrome.",
    "Variabel di URL: chip dengan fill <code>var_ok_bg</code> (token baru) agar terbaca sebagai token, bukan hanya warna teks.",
  ]) });

screen({ id: "http-body", group: "HTTP request", label: "Body raw JSON + Headers response", rail: "collections", sel: "create", tab: OPEN_TABS[1], preset: { reqTab: "body", resTab: "headers" }, view: () => httpView("create"),
  notes: N([
    "Mode body pakai segmented kecil (None · Form · URL-encoded · Raw · Binary · GraphQL). Label ‘x-www-form-urlencoded’ dipendekkan; nama lengkap di tooltip.",
    "Bahasa raw (JSON ▾) dan Beautify ada di toolbar yang sama, sejajar dengan toolbar body response di kanan.",
    "Editor berada di ‘well’ (latar sedikit lebih gelap) dengan gutter nomor baris — konsisten dengan viewer response.",
    "Klik segmen lain untuk melihat Form, URL-encoded, Binary dan None.",
  ]) });

screen({ id: "http-auth", group: "HTTP request", label: "Auth OAuth 2.0 + Tests response", rail: "collections", sel: "me", tab: OPEN_TABS[2], preset: { reqTab: "auth", resTab: "tests", authType: "oauth2" }, view: () => httpView("me"),
  notes: N([
    "Pemilih tipe auth jadi dropdown di toolbar (bukan kolom kiri 250 px) supaya formulir mendapat lebar penuh panel kiri.",
    "Kartu ‘Current token’ di atas: status, token ter-mask, kedaluwarsa, scope, Get new / Clear. Konfigurasi di grid 2 kolom, ‘Advanced (6)’ tertutup.",
    "Tests di response: ringkasan pass/fail/skip + bar rasio + filter + daftar. Baris kosong mengarahkan cara menambah test.",
    "Coba ganti tipe auth lewat dropdown untuk melihat Inherit, Bearer dan Basic.",
  ]) });

screen({ id: "http-scripts", group: "HTTP request", label: "Scripts + Details/timing response", rail: "collections", sel: "list", tab: OPEN_TABS[0], preset: { reqTab: "scripts", resTab: "details" }, view: () => httpView("list"),
  notes: N([
    "Pre-request / Post-response sebagai segmented; Snippets jadi menu (panel samping memakan lebar saat side-by-side).",
    "‘Generate tests’ ada di toolbar Post-response; mati dengan tooltip bila belum ada response.",
    "Details: banner variabel undefined di paling atas, lalu waterfall timing (DNS, connect, TTFB, download), info response, dan request yang benar-benar terkirim.",
  ]) });

screen({ id: "http-graphql", group: "HTTP request", label: "GraphQL + schema explorer", rail: "collections", sel: "search", tab: { id: "search", m: "GQL", t: "Search catalog", go: "http-graphql" }, preset: { reqTab: "body", bodyMode: "graphql" }, view: () => httpView("search"),
  notes: N([
    "Query di atas, Variables di bawah dalam satu bingkai; schema explorer sebagai laci di sisi kanan panel request (bisa ditutup).",
    "Tombol + di setiap field menyisipkan operasi beserta variabelnya.",
    "Status validasi query (‘Valid against schema’) di header editor.",
  ]) });

screen({ id: "http-filter", group: "HTTP request", label: "Response: JSONPath filter", rail: "collections", sel: "list", tab: OPEN_TABS[0], preset: { filter: true }, view: () => httpView("list"),
  notes: N(["Filter JSONPath muncul sebagai baris di bawah toolbar body dengan jumlah match; Esc menutup. View lain: klik Tree / Raw / Preview."]) });

screen({ id: "http-compare", group: "HTTP request", label: "Response: Compare dengan sebelumnya", rail: "collections", sel: "list", tab: OPEN_TABS[0], preset: { resTab: "compare" }, view: () => httpView("list"),
  notes: N(["Compare tetap tab tersendiri (muncul bila ada response sebelumnya). Ringkasan delta status/waktu/ukuran/headers di atas, diff baris dengan konteks yang dilipat."]) });

screen({ id: "http-empty", group: "HTTP request", label: "State: belum dikirim (scratch)", rail: "collections", tab: { id: "scratch", icon: "GLOBE", t: "Untitled request", go: "http-empty", dirty: true }, preset: { resState: "empty" }, view: () => httpView("scratch"),
  notes: N(["Response kosong menampilkan shortcut ⌘↩ dan dua request terakhir dari History agar langsung bisa dibuka.", "Request belum disimpan: pill ‘Unsaved’ menggantikan breadcrumb, tombol jadi ‘Save…’."]) });

screen({ id: "http-loading", group: "HTTP request", label: "State: sedang mengirim", rail: "collections", sel: "list", tab: OPEN_TABS[0], preset: { resState: "loading" }, view: () => httpView("list"),
  notes: N(["Saat mengirim, Send berubah jadi Cancel di tempat yang sama; baris status menampilkan spinner + waktu berjalan."]) });

screen({ id: "http-error", group: "HTTP request", label: "State: gagal kirim", rail: "collections", sel: "list", tab: OPEN_TABS[0], preset: { resState: "error" }, view: () => httpView("list"),
  notes: N(["Error dijelaskan dengan bahasa manusia + daftar periksa sesuai jenis error (11 jenis di <code>error_hint</code>), aksi Try again / Show details / Open console."]) });

screen({ id: "http-stacked", group: "HTTP request", label: "Jendela sempit: response di bawah", rail: "collections", sel: "list", tab: OPEN_TABS[0], preset: { size: 3, sidebar: false }, view: () => httpView("list"),
  notes: N(["Di bawah ±880 px lebar area kerja (dua panel ±440 px), response otomatis pindah ke bawah; struktur panel tetap sama (URL → tab | status → tab).", "Toggle manual Side by side / Stacked ada di status bar."]) });
