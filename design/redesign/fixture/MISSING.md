# Screens the app cannot reach yet

Generated with the steps in `steps/`. For each approximate screen: the automation step that
`crates/ternarywire-app/src/automation.rs` would need (a proposal; the name is free), and what it
should do. Add the step, then replace the stand-in steps of that screen.

**Exact** (61): `http`, `http-auth`, `http-scripts`, `http-filter`, `http-compare`, `http-empty`, `home`, `http-graphql`, `http-loading`, `http-stacked`, `mcp`, `mcp-elicit`, `ai`, `container`, `spec`, `webhook`, `webhook-empty`, `capture`, `dlg-palette`, `dlg-quicklook`, `dlg-settings`, `dlg-cookies`, `dlg-import`, `dlg-gen-tests`, `dlg-set-var`, `dlg-update`, `console`, `ws`, `mqtt`, `grpc`, `grpc-picker`, `dlg-palette-search`, `dlg-env-switch`, `dlg-settings-requests`, `dlg-settings-scripts`, `dlg-settings-proxy`, `dlg-settings-certs`, `dlg-settings-shortcuts`, `dlg-settings-about`, `dlg-vault`, `dlg-vault-link`, `dlg-vault-providers`, `dlg-import-ready`, `dlg-snippet`, `dlg-save`, `dlg-fork-merge`, `dlg-confirm`, `dlg-unsaved`, `dlg-name-prompt`, `dlg-new-ws`, `dlg-update-progress`, `mock`, `mock-routes`, `capture-cert`, `env`, `history`, `spec-sync`, `flow`, `flow-settings`, `runner`, `runner-perf`

**Approximate** (3): `http-body`, `http-error`, `sio`

| Screen | Needed step | What it should do |
| --- | --- | --- |
| `http-body` | `{"edit": {"request": "Create book", "body_raw": "…"}}` | the tab of Create book is unsaved (dot) in the prototype |
| `http-error` | (none) | the refused port is 18794 instead of 18795, since server.py listens on 18795 |
| `sio` | (server) | the pending-ack card needs a server event that asks for an ack, which server.py never sends; the prototype's second argument (a file) is not in the saved message that `{"auto": "load:Say hello"}` puts in the composer; `{"auto": "select:5"}` selects the row |

## States the fixture cannot produce

- **Vault** (`dlg-vault`, `dlg-vault-link`, `dlg-vault-providers`): the providers point at no real
  secret manager, so linked secrets stay "not fetched yet" (no "cached · 42 s" or sign-in error),
  the link form has no browse results or "Resolved (24 characters)", and Test connection has no
  result. The fixture also has seven local secrets where the prototype has four.

## Everywhere

- **Scripts in the off-screen test.** `TernaryWireApp::script_engine` starts script workers from `std::env::current_exe()`, which in `automation::tests::offscreen` is the test binary; libtest answers `--ternarywire-script-worker` with "Unrecognized option", so no script would run. `shoot.sh` works around it (it starts the test binary through a symlink and then points the link at the app binary of the same build). A fix in app code would make that unnecessary: under `cfg(test)` use `ternarywire_script::BoaEngine::new()` in `script_engine`, or call `ternarywire_script::maybe_run_worker()` first in the test binary.
- `{"expand": "Bookstore API"}` opens every folder; the prototype keeps *Orders* closed. Needed: `{"collapse": "Bookstore API/Orders"}` (or `expand` taking a folder path).
- Collections and environments are listed in file-name order (Bookstore API, Payments, Realtime & AI; Local, Production, Staging); the prototype lists Realtime & AI and Staging first.
- The *Create book* tab is not marked unsaved, and no tab can be closed (`home`).
- Mouse hover states (row actions, menus, tooltips) are not reproduced.
