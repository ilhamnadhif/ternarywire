# Visual comparison fixture

Screenshots of the real app next to the prototype, screen by screen, at the same size and in the same
state. Every prototype screen id (`index.html#<id>`, see `../README.md`) has a script in `steps/`.

```sh
design/redesign/fixture/compare.sh http light          # app + design PNGs, prints both paths
design/redesign/fixture/compare.sh home dark 1440x900
design/redesign/fixture/shoot.sh ws dark               # only the app
design/redesign/fixture/server.py status               # the local servers
```

PNGs go to `$TMPDIR/tw-shots/<id>-<theme>-app.png` and `…-design.png` (2× pixels: 1280×820 gives
2560×1640). Both scripts work from any worktree: `shoot.sh` builds and runs the app of the worktree you
call it from (`git rev-parse --show-toplevel` of the current directory).

## Files

| File | What it is |
| --- | --- |
| `crates/ternarywire-app/tests/design_fixture.rs` | Ignored test `make_fixture`: writes the "Bookstore" workspace with the `ternarywire_core` API |
| `server.py` | Local servers for every protocol (stdlib only) |
| `steps/<id>.json` | Automation script per screen (`automation.rs` steps plus fixture steps, below) |
| `shoot.sh` | One app screenshot (builds the app's test binary and app binary in the current worktree) |
| `compare.sh` | App screenshot + prototype screenshot (headless Chrome, bare mode) |
| `MISSING.md` | Screens the app cannot reach yet, with the automation step each one needs |

## The fixture workspace

```sh
TW_FIXTURE_OUT=/tmp/tw-fixture cargo test -p ternarywire-app --test design_fixture -- --ignored
```

writes

- `workspace/`: collections **Bookstore API** (OAuth 2.0, variables, scripts; Auth, Books, Orders with the
  prototype's requests, params, headers, scripts, docs and examples), **Realtime & AI** (WS, SIO, MQTT,
  gRPC, MCP, AI pointing at `server.py`) and **Payments**; environments Local (active), Staging,
  Production; 3 globals; `specs/` (5 files), `flows/` (Catalog summary, Checkout smoke test, Nightly
  sync), `data/books.csv`; and in `.ternarywire/`: current values, `secrets.json` (vault secrets, secret
  variables, OAuth tokens; read with `TERNARYWIRE_SECRETS=file`), history, cookies and `session.json` (the
  prototype's five open tabs).
- `config/TernaryWire/settings.json`: with `TERNARYWIRE_CONFIG_DIR=<copy of config>` the app opens that
  workspace (side-by-side layout, no system proxy).
- `fixture.json`: ids and the reference day of the sample data.

Ids are fixed (`coll-bookstore`, `req-list-books`, `env-local`, …; see the top of the test), so steps and
`_current` can name them.

`shoot.sh` generates it once into `$TMPDIR/tw-fixture-cache/<hash>/` and again whenever
`design_fixture.rs` is newer. Each run works on its own copy, where it also renews the OAuth tokens
("expires in 52 min") and moves the history to today.

## Servers

`server.py start` starts them in the background unless they already run (`shoot.sh` does this); one set
serves every worktree. State, log and lock: `$TMPDIR/tw-fixture-servers/`. `server.py stop` ends them.

| Port | Protocol | What |
| --- | --- | --- |
| 18795 | HTTP | Bookstore API: `/books`, `/books/<id>` (404 for unknown ids), `/auth/login`, `/me`, `/orders…`, `/graphql` (with introspection), `/v1/charges`, `/health`; realistic headers and `Set-Cookie`. `?snapshot=previous` gives the older `/books` (Compare), `/__slow/<path>` answers after 15 s (Sending…), `/__fixture/update.json` is an update feed |
| 18796 | HTTP | OpenAI-compatible `/v1/chat/completions` (streaming and not; calls the MCP tool `search_books` first when offered) and `/v1/models` |
| 8931 | HTTP | MCP Streamable HTTP at `/mcp`: initialize, tools (12), resources, templates, prompts, completion; `tools/call` streams a log message, `create_order` asks for input (elicitation) |
| 18900 | WS | WebSocket echo; `ping` → `pong`, `{"type":"subscribe","channel":"books"}` → `subscribed` + pushed events |
| 1883 | MQTT | MQTT 3.1.1 broker (QoS 0–2, retained, wildcards, will) with a sensor simulator on `sensors/#`, `status/+` |
| 18902 | Socket.IO | `crates/ternarywire-socketio/examples/echo_server.rs` (echo, acks, `tick` every 2 s) |
| 50051 | gRPC | forwarder to `crates/ternarywire-grpc/examples/greeter_server.rs` (reflection v1, `helloworld.Greeter`) |

The first `start` builds the two examples (`cargo build --example`, `CARGO_BUILD_JOBS=4`). Webhook (8787)
and capture (8888) screens open their own listeners in the app; `shoot.sh` runs those screens one at a
time and `server.py poke webhook|capture` sends them sample traffic.

## Steps

A step file is an automation script (`crates/ternarywire-app/src/automation.rs`, `Step`). Placeholders,
replaced by `shoot.sh`: `["@W", "@H"]` (window size, numbers), `"@THEME"`, `"@OUT"` (PNG path). Every
script starts with `{"window": ["@W", "@H"]}`, `{"theme": "@THEME"}` and ends with
`{"screenshot": "@OUT"}`, `{"quit": true}`.

Fixture steps (handled and removed by `shoot.sh`, never seen by the app):

| Step | Effect |
| --- | --- |
| `{"_size": "900x640"}` | default size of the screen (`http-stacked`) |
| `{"_settings": {"ui": {"sidebar_visible": false}}}` | merged into `settings.json` |
| `{"_current": {"env-local": {"baseUrl": "…"}}}` | current values in `state.json` (the URL bar keeps showing `{{baseUrl}}`) |
| `{"_session": false}` | no restored tabs |
| `{"_env": {"TERNARYWIRE_UPDATE_URL": "…"}}` | extra environment for the app |
| `{"_poke": "webhook"}` / `"capture"` | sample traffic once the listener or proxy is up |

When the UI work adds an automation step, use it in the screen's script and remove the screen from
`MISSING.md`. Screens that are only approximate are listed there with the step they need.

## Known limits

- **Script workers**: the app starts script workers from its own executable, which in the off-screen
  test is the test binary (libtest rejects `--ternarywire-script-worker`). `shoot.sh` therefore runs the
  test binary through a symlink and, once it runs, points the link at `target/debug/ternarywire-app` of the
  same build, so pre-request, post-response and test scripts work. `MISSING.md` has the app-side fix that
  would make this unnecessary.
- Secrets use `TERNARYWIRE_SECRETS=file`, so the Vault says "kept in file …" instead of the Keychain.
- WebSocket, MQTT and gRPC tabs take `{"auto": …}`: `connect`, `send`/`publish`, `composer:<text>` (WS), `compose:<topic>|<qos>|<retain>|<payload>` (MQTT), `select:<row>`, `section:<name>`, and `picker` / `{"grpc_picker": true}` (gRPC).
- Sidebar order follows file names (Payments before Realtime & AI; Production before Staging).
- `KEEP=1 shoot.sh …` keeps the scratch folder (`app.log`, `steps.json`, workspace) for debugging.
