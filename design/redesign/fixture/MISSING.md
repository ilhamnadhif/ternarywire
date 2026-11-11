# Screens the app cannot reach yet

Generated with the steps in `steps/`. For each approximate screen: the automation step that
`crates/ternarywire-app/src/automation.rs` would need (a proposal; the name is free), and what it
should do. Add the step, then replace the stand-in steps of that screen.

**Exact** (29): `http`, `http-auth`, `http-scripts`, `http-filter`, `http-compare`, `http-empty`, `http-loading`, `http-stacked`, `mcp`, `mcp-elicit`, `ai`, `container`, `spec`, `webhook`, `webhook-empty`, `capture`, `dlg-palette`, `dlg-quicklook`, `dlg-settings`, `dlg-cookies`, `dlg-import`, `dlg-gen-tests`, `dlg-set-var`, `dlg-update`, `console`, `ws`, `mqtt`, `grpc`, `grpc-picker`

**Approximate** (35): `home`, `http-body`, `http-graphql`, `http-error`, `sio`, `env`, `history`, `spec-sync`, `flow`, `flow-settings`, `runner`, `runner-perf`, `mock`, `mock-routes`, `capture-cert`, `dlg-palette-search`, `dlg-env-switch`, `dlg-settings-requests`, `dlg-settings-scripts`, `dlg-settings-proxy`, `dlg-settings-certs`, `dlg-settings-shortcuts`, `dlg-settings-about`, `dlg-vault`, `dlg-vault-link`, `dlg-vault-providers`, `dlg-import-ready`, `dlg-snippet`, `dlg-save`, `dlg-fork-merge`, `dlg-confirm`, `dlg-unsaved`, `dlg-name-prompt`, `dlg-new-ws`, `dlg-update-progress`

| Screen | Needed step | What it should do |
| --- | --- | --- |
| `home` | `{"close_all_tabs": true}` | the start page shows only without tabs; the app opens an empty request tab when none is restored |
| `http-body` | `{"edit": {"request": "Create book", "body_raw": "…"}}` | the tab of Create book is unsaved (dot) in the prototype |
| `http-graphql` | `{"graphql_schema": "fetch"}` | fetch the schema (server.py answers introspection) and open the schema explorer |
| `http-error` | (none) | the refused port is 18794 instead of 18795, since server.py listens on 18795 |
| `sio` | (server) | the pending-ack card needs a server event that asks for an ack, which echo_server.rs never sends; `{"auto": "select:5"}` selects the row |
| `env` | `{"env_row_menu": "pageSize"}` | the hover menu of a variable row (Persist / Reset current value) |
| `history` | `{"open_history": "hist-11"}` | open a history entry as a tab (with its "Opened from History" bar); a scratch tab with the same request stands in |
| `spec-sync` | `{"spec_sync": "Bookstore API"}` | run Sync spec → collection and open the Sync popover; also an unsaved edit and a warning |
| `flow` | `{"flow_select": "Books"}` | select a block so the inspector shows it |
| `flow-settings` | `{"flow_menu": "add-block"}` | open the Add block menu |
| `runner` | `{"runner_options": {"iterations": 12, "data_file": "data/books.csv"}}` | 12 iterations from the data file (iteration 3 asks for book 9 → the 404 failure); then {"runner_select": "Get book#3"} |
| `runner-perf` | `{"runner_mode": "performance"}` | switch the runner to Performance (25 users, 30 s), then runner_start |
| `mock` | `{"auto": "start"}` | start the mock server (port 3000); its log then needs traffic, e.g. {"_poke": "mock"}, and {"auto": "select:1"} |
| `mock-routes` | `{"mock_section": "routes"}` | show the Routes tab (and {"auto": "start"}) |
| `capture-cert` | `{"capture_cert": true}` | open the certificate panel of the capture proxy |
| `dlg-palette-search` | `{"palette_query": "books"}` | type a query so the fuzzy results show |
| `dlg-env-switch` | `{"dialog": "env-switch"}` | the environment switcher (⌘E); the command palette stands in |
| `dlg-settings-requests` | `{"dialog": "settings:requests"}` | open Settings on the Requests page (opens on General) |
| `dlg-settings-scripts` | `{"dialog": "settings:scripts"}` | open Settings on the Scripts page (opens on General) |
| `dlg-settings-proxy` | `{"dialog": "settings:proxy"}` | open Settings on the Proxy page (opens on General) |
| `dlg-settings-certs` | `{"dialog": "settings:certs"}` | open Settings on the Certificates page (opens on General) |
| `dlg-settings-shortcuts` | `{"dialog": "settings:shortcuts"}` | open Settings on the Shortcuts page (opens on General) |
| `dlg-settings-about` | `{"dialog": "settings:about"}` | open Settings on the About page (opens on General) |
| `dlg-vault` | (fixture) | no external providers or linked secrets: linking them would make the app fetch them at start |
| `dlg-vault-link` | `{"dialog": "vault:link"}` | the Vault with the Link external secret form open |
| `dlg-vault-providers` | `{"dialog": "vault:providers"}` | the Vault on its External providers tab (and providers in the fixture) |
| `dlg-import-ready` | `{"dialog": "import:<path>"}` | the Import dialog with a file already chosen and detected |
| `dlg-snippet` | `{"dialog": "snippet"}` | the Code snippet dialog of the active request |
| `dlg-save` | `{"dialog": "save"}` | the Save request dialog of the active (unsaved) request |
| `dlg-fork-merge` | `{"open": {"collection": "Payments (fork)", "request": "List customers"}}` | open (and then edit with {"url": …}) a request of the fork, for a conflict; `open` finds the first request of that name, the parent's, so only 1 change shows instead of 5 |
| `dlg-confirm` | `{"dialog": "confirm-delete:Get book"}` | the delete confirmation of a request |
| `dlg-unsaved` | `{"close_tab": "Create book"}` | close an unsaved tab, which asks to save |
| `dlg-name-prompt` | `{"dialog": "new-collection"}` | the name prompt for a new collection |
| `dlg-new-ws` | `{"dialog": "new-workspace"}` | the New workspace dialog |
| `dlg-update-progress` | `{"update_stage": "verify"}` | the pop-up while installing; update_install would download into ~/Downloads or open the browser |

## Everywhere

- **Scripts in the off-screen test.** `TernaryWireApp::script_engine` starts script workers from `std::env::current_exe()`, which in `automation::tests::offscreen` is the test binary; libtest answers `--ternarywire-script-worker` with "Unrecognized option", so no script would run. `shoot.sh` works around it (it starts the test binary through a symlink and then points the link at the app binary of the same build). A fix in app code would make that unnecessary: under `cfg(test)` use `ternarywire_script::BoaEngine::new()` in `script_engine`, or call `ternarywire_script::maybe_run_worker()` first in the test binary.
- `{"expand": "Bookstore API"}` opens every folder; the prototype keeps *Orders* closed. Needed: `{"collapse": "Bookstore API/Orders"}` (or `expand` taking a folder path).
- Collections and environments are listed in file-name order (Bookstore API, Payments, Realtime & AI; Local, Production, Staging); the prototype lists Realtime & AI and Staging first.
- The *Create book* tab is not marked unsaved, and no tab can be closed (`home`).
- Mouse hover states (row actions, menus, tooltips) are not reproduced.
