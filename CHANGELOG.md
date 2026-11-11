# Changelog

All notable changes to TernaryWire. The newest version comes first.

## 0.3.1 — 2026-11-12

Every screen of the new design checked against the design once more and matched to it, and
upgrades from 0.2 now open with the response on the right.

### Fixed
- **Upgrading from 0.2 opens side by side.** Settings from before 0.3 kept the old default
  (response below the request), so upgraded installs stayed stacked. That old setting is now
  ignored; choosing Stacked in the status bar still sticks.
- **Every screen matches the design** in light and dark: sizes, gaps, line heights, colors and
  fonts of all screens and dialogs. Among others: buttons count their border, placeholders are
  faint, scroll areas no longer fade at their edges, icons sit centered, text fields show a focus
  halo, menu rows touch, stacked panes are parted by a hairline, tabs shrink when they don't fit
  and Indonesian auth labels are translated.
- Code shows what was typed: `=>` no longer turns into an arrow.
- A tab opened from History is named by its path and isn't marked unsaved until you edit it.
- A refused connection says that nothing is listening on that host and port.
- The mock server log shows the client's address, and a request dropped by failure simulation
  shows the route it would have hit.
- gRPC: when the server hits the deadline first, the call shows DEADLINE_EXCEEDED instead of
  CANCELLED.

### Added
- Key/value tables have a menu: copy as text, remove disabled rows, remove all rows.
- Markdown colors in collection descriptions and shell colors in cURL, HTTPie and Wget snippets.
- The Spec Hub editor keeps long lines on one line and scrolls sideways.

## 0.3.0 — 2026-11-11

A new design for the whole app: request on the left, response on the right, and every screen redrawn
on one set of colors, sizes and components.

### Changed
- **Request left, response right.** The method, URL and Send sit at the top of the left pane; the
  response status, time, size and test results at the top of the right pane, so both panes' rows line
  up across the window. Side by side is now the default (narrow windows still stack the response
  under the request; the status bar switches between the two).
- **The window**: an integrated title bar (on macOS under the traffic lights) with the workspace,
  New, Import, a search that also runs commands, the environment and its quick look, cookies and the
  vault; the tabs and the open view sit on one card; the status bar shows the console, saving, updates,
  the workspace folder and the layout.
- **Every screen redrawn**: HTTP and GraphQL requests and responses (with Compare, the JSONPath
  filter and folded JSON), WebSocket, Socket.IO, MQTT, gRPC, MCP and AI in the same left/right layout,
  collection and environment settings, Spec Hub, Flows, the Collection Runner, the mock server,
  webhook listener and capture proxy, Settings, the Vault, cookies, import, code snippets and every
  other dialog, the console and the start page.
- Sidebars use one row style; collections and folders have hover actions (add, more) and a menu of
  every action.

### Added
- **Local servers** in the sidebar: mock servers, webhook listeners and capture proxies, with their
  address and whether they run.
- Performance runs export to HTML (with the chart), CSV or JSON.
- New › GraphQL request and New › API spec.
- A request opened from History says so, with the time and status it had.
- Filters for webhook requests and for the languages of code snippets.
- Markdown preview of a request's documentation.

## 0.2.3 — 2026-11-02

A release of fixes from a review of the whole code base.

### Security
- Scripts can no longer read files outside the workspace. A file a script puts into a request body
  or a `pm.sendRequest` body must be a relative path inside the working directory (the workspace,
  or `--working-dir` for the CLI). Files the saved request already uses keep working.
- Scripts that run on the same worker can no longer affect later scripts by patching built-ins such
  as iterators, or call the worker's internal hooks. The Package Library no longer tells whether a
  file outside it exists.
- A redirect to another origin no longer takes credentials along: API key headers, a `Cookie` header
  set by hand, `X-Amz-Security-Token` and `Authorization` are dropped (unless *Follow Authorization
  header* is on), and `Host` and `Proxy-Authorization` always are.
- Network errors no longer show query strings or passwords from URLs, and `ternarywire run --verbose`
  and `ternarywire flow run` mask credentials in URLs unless you pass `--show-secrets`.
- Unsaved tabs no longer write credentials (Authorization, Cookie and API key headers, sensitive auth
  fields, secret and vault values) to `.ternarywire/session.json`; they are kept in the secret store
  and come back after a restart.
- `.ternarywire/` now ignores itself in Git, also in workspaces that already had a `.gitignore`.
- The mock server allows credentials in CORS only for local origins (localhost, loopback and private
  addresses), so other web pages cannot read its responses with your cookies.
- Code snippets: HTTPie items are escaped, so a value starting with `@` no longer reads a local file;
  JavaScript, Java and Swift snippets quote file paths and multipart names.
- Generated Markdown documentation keeps raw HTML from descriptions inside code, the OpenAPI export
  and the documentation mask credentials in GraphQL variables, XML and form bodies, and the first
  word of a masked passphrase is no longer left visible.
- Pages opened in the browser (visualizer, HTML preview, docs) are written to private temporary
  files with random names.
- MCP sign-ins ignore callback requests that don't carry the sign-in's `state`.
- Requests saved from the capture proxy follow the cookie and header options in their examples too.
- The GitHub Action passes its inputs to its scripts through the environment, so an input can't run
  shell commands.

### Fixed
- **Requests and auth**
  - gRPC requests in the Collection Runner and the CLI send the auth they inherit from their
    collection or folder.
  - NTLM uses HTTP/1.1 and keeps its handshake on one connection, and servers that offer only
    `Negotiate` are answered too.
  - OAuth 2.0: requests that find the same expired token refresh it once, so rotating refresh tokens
    keep working; client credentials in the Basic header are form-encoded as RFC 6749 asks.
  - AWS Signature v4 signs file bodies for every service, not only S3, and leaves the multipart
    Content-Type out; Hawk includes file bodies in the payload hash.
  - Proxy auto-config: a failed or slow PAC call is tried again instead of sending that host direct
    until the app restarts, and changes to the system proxy reach requests at once.
  - Empty query values keep their `=` (`?b=`), GraphQL variables that aren't JSON are sent as typed,
    and headers whose name is empty are left out.
- **Scripts, Runner and CLI**
  - `pm.sendRequest` stops when the run is cancelled or the script times out, and binary responses
    keep their bytes.
  - A variable that can't be stored (a BigInt, a circular object) no longer loses the tests, console
    and variables of the whole script.
  - `pm.cookies` holds the cookies of the URL the request is sent to, also when re-running tests.
  - The Runner warns when its loop guard ends a `setNextRequest` loop, and performance tests say why
    nothing ran and use the workspace folder for files.
  - Sessions without a timeout give up connecting after 30 s; `packages.lock.json` is locked while
    several script workers update it.
- **Workspace**
  - Deleting a collection, folder or request also deletes its saved secrets and OAuth tokens.
  - Forks: a change you declined comes up again at the next pull, and removing a folder that the
    other side changed is a conflict instead of deleting those changes.
  - A symlink back to a parent folder no longer makes loading hang; rewritten files keep their
    permissions; adding to a long history no longer rereads it.
- **Protocols**
  - Socket.IO: acknowledgements given before the namespace connects are sent once it does, invalid
    event names are refused right away, and a slow last polling request no longer breaks the upgrade.
  - MQTT: messages published while connecting are checked against the broker's packet size limit.
  - gRPC: unary and server-streaming calls refuse extra request messages.
  - MCP: a sign-in or an open dialog no longer cuts off requests, an expired session closes the
    connection, resumed streams sign in again when needed, the last event of a stream is kept, and
    exported host configs include Basic auth and query API keys.
  - AI: the timeout limits the silences of a streamed answer and the whole of any other answer;
    servers that refuse `stream_options` are asked again without it; Gemini events with several
    responses keep them all; time to first token is shown only for streamed answers.
- **The app**
  - Stopping a flow and running it again no longer leaves the new run impossible to stop.
  - Re-running tests while a request is being sent no longer hides its progress and Cancel.
  - Results of requests started in a workspace you have since closed are dropped instead of being
    written into the new one.
  - Tabs restored after a restart remember unsaved edits and ask before closing.
  - Connecting twice quickly runs the pre-request script and connects once; Stop on an AI request
    ignores what it still streams; Cancel ends a hanging MCP handshake.
  - Opening an AI request's Authorization section keeps its auth; the MCP command line keeps spaces
    and quotes as you type them; a flow input's default stays as typed.
  - A request's settings show the defaults from Settings; pending flow edits are saved when the tab
    closes or the app quits; the Vault dialog reports a provider error once.
  - Large response bodies are indexed and searched without blocking the window, image previews are
    read once, and long gRPC streams no longer grow memory without bound.
  - Updates: an AppImage is replaced without freezing the window, a skipped version no longer shows
    its badge after a restart, and downloads never overwrite a file of yours.
  - Eight Indonesian translations were missing.
  - On Windows the window's thread gets the same 8 MiB stack as on macOS and Linux.
- **Visualizer**
  - A template whose partials call each other no longer crashes the app; template output, grids and
    inline images have size limits.
  - `font: 700 12px …`, `visibility: hidden`, `display: inline-table`, attribute selectors with `i`,
    comments inside strings and `text-decoration` on children render as in browsers.
- **Import and export**
  - cURL: every option that takes a value is read as such, and unsupported input (`-b file`,
    `-H @file`, extra URLs, data with `-F`) gives a warning.
  - Insomnia v5 environments are detected and partial exports keep their order; OpenAPI path
    parameters inside a segment get their variables; SOAP types keep their own namespace; spaces
    around XML entities stay; Smithy reads prelude traits only from `smithy.api` and rejects broken
    escapes and out-of-range status codes; Thunder tests keep only real number literals; GraphQL
    suggestions work after a `#` inside a string.
- **Flows**
  - A loop that its own *done* path starts again is reported as a cycle before the run.
  - Vault secrets are fetched before the run, so parallel branches don't wait on each other.
  - Scripts find blocks by name the way templates do, flow references inside variables reach
    requests, and `await` in a comment no longer changes a script's result.
- **Capture proxy and mock server**
  - Intercepted requests are recorded under the host they reached, and a client that just aborts a
    TLS handshake no longer turns that host to passthrough.
  - Clients that check the certificate only after the TLS handshake and then hang up (such as curl
    8.18 on Linux) show up in the list as failed, with the likely cause, instead of not at all.
  - The mock server answers 413 to bodies over 16 MiB instead of matching them as empty, and the
    webhook listener numbers events in the order it stores them.
- **Secret managers**
  - 1Password names starting with `-` are no longer read as options, and Test connection warns when
    credentials would go over plain `http://` to another machine.

### Changed
- The directory of a collection or folder is renamed on the next save when its name only starts
  with the new slug (for example `api-v2` for a collection now called "API").
- Visualizer documents nested deeper than 128 levels are flattened (was 256).

## 0.2.2 — 2026-11-02

### Changed
- Merpati is now TernaryWire, one of the Ternary family with TernaryGrid (SQL client) and TernaryShell (SSH and SFTP client). The app, its files and its downloads carry the new name, and it has a new icon: a route through the 3 × 3 lattice the three apps share.
- The command-line tool is `ternarywire` (`ternarywire run` replaces `merpati run`), the GitHub Action is `ilhamnadhif/ternarywire`, and a workspace is a folder with a `ternarywire.json` file (local state in `.ternarywire/`). Settings and saved secrets are kept under the new name; nothing is read from Merpati's folders or Keychain entries.

## 0.2.1 — 2026-10-31

### Fixed
- The collection sidebar left the bottom of a long list empty, and rows that should have shown
  there (such as the requests of an open folder) were missing. The same slip affected the
  Collection Runner results, the JSON tree, the Compare view and the capture list.
- Chinese, Japanese, Korean, Thai, Arabic and other scripts the bundled fonts lack showed as boxes;
  they now come from the system fonts.
- Side-by-side layout in a narrow window squeezed the response over the request; below 680 points
  the response now sits under the request.
- Section tabs (Params … Docs) that don't fit move closer together, then scroll with a fade on the
  hidden side instead of being cut off.
- The new-tab button stays in view when there are many tabs.
- Runner results keep the request name visible when its folder names are long.
- A run without tests says "25 requests sent, no tests ran." instead of "All 0 tests passed"; a
  single iteration no longer reads "× 1 iterations".
- A response without a body (204 No Content, HEAD) says so instead of showing an empty editor.
- The note about where environment values are saved wraps instead of being cut off.
- Screen readers: drop-downs, code editors, number fields and sliders have names (for example
  "Provider", "Request timeout", "Port").
- Keyboard selection in the message logs of the protocol tabs scrolls the right row into view.

## 0.2.0 — 2026-10-30

### Added
- **Native macOS menu bar**: TernaryWire, File, Edit, View, Window and Help menus with the usual
  shortcuts, **Check for Updates…**, Settings…, links to the repository, release notes and issues.
  On Linux and Windows, **Check for updates…** is in the workspace menu.
- **Linux packages**: `.deb` (Ubuntu, Debian, Mint, Pop!_OS), `.rpm` (Fedora, RHEL, Rocky, Alma,
  openSUSE), AppImage (Arch, Manjaro and others) and `.tar.gz`, for x86_64 and ARM64, built
  against glibc 2.28. The release workflow installs them on Ubuntu 20.04/24.04, Debian 11/12,
  Fedora 41, Rocky Linux 8/9 and openSUSE Leap 15.6 before publishing.
- **Windows installer** (`TernaryWire-Setup-<version>-x64.exe`): per-user or all-users install, Start
  menu entry, optional `ternarywire` on `PATH`, updates in place; the portable `.zip` stays.
- **Updates per kind of install**: an AppImage replaces itself and restarts like the macOS app; a
  `.deb`/`.rpm` install gets the new package, Windows the new installer (or `.zip`).
- **Response folding**: line numbers and fold carets in the Pretty view for JSON, XML/HTML and
  YAML (`"address": { … } 3 keys`), Expand all / Collapse all and collapse to level 1 or 2.
- **JSONPath filter** on JSON responses (`$.data[?(@.price < 10)].title`) with a match count, in
  the Pretty and Tree views; the evaluator lives in `ternarywire-core` and is tested against
  Goessner's examples.
- **Value actions**: right-click a value to copy it or its path, **Set as variable…** (environment,
  collection or globals; optionally also set on every run by a script line) or **Add test for this
  value**.
- **Compare with the previous response**: status, time, size and header changes and a line diff of
  the bodies (JSON compared with sorted keys), with a pin to keep a baseline.
- **Generate tests** from a response: status, response time, Content-Type and a JSON schema
  check, previewed before they are added.
- **Maximize and minimize the response panel**, in both layouts.
- **Copy as cURL (as sent)** in the response menu.
- New request tabs put the caret in the URL field; the active tab scrolls into view.
- If TernaryWire ever closes unexpectedly, the details are written to `crash.log` in its settings
  folder (only on this computer) and the next start says so.

### Fixed
- The app closed when an HTML or XML response contained non-ASCII text (for example the "★" on
  jsonplaceholder.typicode.com); highlighted text is never cut inside a character any more.
- Response bodies no longer shake while scrolling: trackpad gestures stick to one axis, and large
  bodies keep the width of their longest line.
- The text cursor of an empty field was short and sat below the middle; focused inputs now show a
  calmer accent outline.
- An https:// PAC (proxy auto-config) file could fail to download when nothing had set up TLS yet.
- History rows with long URLs no longer overlap the status code or widen the sidebar.

## 0.1.0 — 2026-10-25

First public release: a native API client for macOS (Apple Silicon and Intel), with builds for
Linux and Windows.

### Requests and responses
- HTTP requests with every method, params and `:path` variables synced with the URL, headers with
  autocomplete, and form-data, urlencoded, raw, binary and GraphQL bodies.
- Responses as Pretty, Raw, Preview or Tree, with headers, cookies, test results, timings and
  search; large responses are streamed to disk and shown virtually.
- `pm.visualizer` rendered inside the app (HTML and a CSS subset with grid, flexbox, floats and
  positioning).
- Cookie jar, extra CA and per-host client certificates (PEM, PKCS#12), proxies (HTTP, HTTPS,
  SOCKS, system settings, PAC, WPAD), HTTP/1.1 and HTTP/2, Unix sockets and Windows named pipes.
- Authentication: API key, Basic, Bearer, Digest, JWT, OAuth 1.0, OAuth 2.0 (with PKCE and
  refresh), Hawk, AWS SigV4, NTLM, Akamai EdgeGrid and ASAP.
- Searchable history and code snippets in 18 languages and tools.

### Collections and environments
- Workspaces as folders of pretty-printed JSON, one file per request, ready for Git.
- Import from Postman (collections v2.0/v2.1, environments, globals), cURL, OpenAPI 3, Swagger 2,
  Insomnia, HAR, Hoppscotch, Thunder Client, WSDL and Smithy; export to Postman v2.1 and OpenAPI.
- Five variable scopes, initial and current values, secret variables, 118 dynamic variables and
  `{{vault:name}}`, backed by the Keychain or by HashiCorp Vault, AWS Secrets Manager, Azure Key
  Vault and 1Password.
- Secrets and OAuth tokens are kept in the Keychain, never in workspace files, exports or reports.
- Local forks of a collection with three-way pull and merge.

### Scripts and tests
- Postman-compatible `pm` API on an embedded JavaScript engine (boa) with Chai, lodash, moment,
  crypto-js and tv4; pre-request and post-response scripts on collections, folders and requests.
- Scripts for gRPC, WebSocket, Socket.IO, MQTT, MCP and AI requests.
- npm and JSR packages through `pm.require`, cached and locked by SHA-256, a local Package Library,
  `Intl` and Web Streams.
- Scripts run in a separate worker process with time and memory limits.

### Runner and CLI
- Collection Runner with environments, iterations, delays, CSV/JSON data, `setNextRequest`,
  custom order, failure simulation and JSON/JUnit/HTML export; every protocol can run in it.
- Performance tests with virtual users, ramp-up and latency percentiles.
- `ternarywire` CLI: `run` with newman-compatible options, `schedule`, `mock`, `import`, `export`,
  `convert`, `validate`, `codegen`, `perf`, `docs`, `flow` and `init`; exit codes 0/1/2 for CI.
- A Dockerfile for the CLI and a GitHub Action that runs a collection in CI.

### Protocols
- GraphQL with subscriptions, introspection and schema autocomplete.
- gRPC from `.proto` files or server reflection, all four call types, TLS and mTLS.
- WebSocket, Socket.IO v2–v4 and MQTT 3.1.1/5 with saved messages and virtualized message logs.
- SOAP requests from WSDL.
- MCP clients over stdio, Streamable HTTP and SSE, with OAuth, sampling and elicitation.
- AI requests to OpenAI, Anthropic, Gemini and OpenAI-compatible servers, with streaming and MCP
  tools.

### Flows
- A visual canvas with Start, Request, Condition, Loop, Parallel, Join, Script, AI, Delay, Set
  Variable, Template, Output and Log blocks; parallel branches, streaming AI output, validation.
- Flows are saved in `flows/` and run from the app or with `ternarywire flow run`.

### API design
- Spec Hub for OpenAPI, Swagger, AsyncAPI, Protobuf, GraphQL SDL and Smithy with live validation
  and a live documentation preview.
- Generate or update a collection from an OpenAPI or Smithy spec; export a collection as OpenAPI.
- Documentation as Markdown or a static HTML site, from the app or `ternarywire docs`.

### Mock and capture
- Mock server from examples with Postman-style matching, templating, network and failure
  simulation.
- Local webhook listener with replay.
- Capture proxy for HTTP and HTTPS with a local CA and HAR export.

### App
- Design system: Inter, JetBrains Mono and Phosphor icons, light and dark themes, browser-like
  tabs, command palette and quick search; tabs are restored after a restart or crash.
- Updates: on start TernaryWire checks GitHub for a new release (it can be turned off in Settings ›
  General › Updates); on macOS **Install and restart** downloads, verifies and installs it;
  elsewhere the archive is downloaded and checked against the release checksums.
- One-line installer for macOS (`scripts/install.sh`), a universal DMG, an app archive, a
  Homebrew cask, a CLI tarball, and Linux and Windows archives.
- English and Indonesian interface.
- Accessibility: keyboard navigation and screen-reader names for every control (AccessKit).
