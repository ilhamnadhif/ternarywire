//! Local mock server tab (FR-M1..M4).

use std::cell::RefCell;

use egui::{RichText, Ui};

use ternarywire_core::model::{Collection, Example, Id};
use ternarywire_mock::{
    FailureKind, FailureSimulation, MockConfig, MockLogEntry, MockRoute, MockServer, NetworkConditions,
};

use crate::app::{ItemRef, Msg, TernaryWireApp};
use crate::i18n::{tr, trf};
use crate::kit::{self, Tab, Tone};
use crate::server_ui::{self as sv, Cell, Col, Stat, State};
use crate::{icons, theme, widgets};

#[derive(Clone, Copy, PartialEq, Eq)]
enum FailKind {
    Status,
    Drop,
    Timeout,
}

/// The list on the left: the routes served, or the requests received.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sub {
    Routes,
    Logs,
}

const LOG_COLS: [Col; 5] = [Col::Px(86.0), Col::Px(64.0), Col::Px(58.0), Col::Fr(1.0), Col::Px(62.0)];
const ROUTE_COLS: [Col; 4] = [Col::Px(64.0), Col::Fr(1.0), Col::Fr(0.8), Col::Px(86.0)];

pub struct MockTab {
    pub collection: Id,
    pub server: Option<MockServer>,
    pub starting: bool,
    port: u16,
    delay_ms: u64,
    fail_rate: f32,
    fail_status: u16,
    fail_kind: FailKind,
    network: NetworkConditions,
    use_env: bool,
    /// The selected log entry and where it was last seen: once the server's
    /// log is full it drops the oldest entries, so an index alone drifts.
    selected_log: Option<(usize, MockLogEntry)>,
    sub: Sub,
    filter: String,
    selected_route: usize,
    settings_open: bool,
    /// Automation: `start`, `stop`, `routes`, `logs`, `settings`,
    /// `select[:n]` (n-th row from the top), `hit:<METHOD> <path> [body]`.
    pub auto: Option<String>,
}

impl MockTab {
    pub fn new(collection: Id) -> Self {
        MockTab {
            collection,
            server: None,
            starting: false,
            port: 3000,
            delay_ms: 0,
            fail_rate: 0.0,
            fail_status: 500,
            fail_kind: FailKind::Status,
            network: NetworkConditions::default(),
            use_env: true,
            selected_log: None,
            sub: Sub::Logs,
            filter: String::new(),
            selected_route: 0,
            settings_open: false,
            auto: None,
        }
    }

    pub fn started(&mut self, server: MockServer) {
        self.starting = false;
        self.server = Some(server);
    }

    pub fn stop(&self, _rt: &tokio::runtime::Runtime) {
        // `MockServer` shuts down when dropped; `show` drops it on Stop.
    }

    pub fn is_running(&self) -> bool {
        self.server.is_some()
    }

    /// The server's address while it runs.
    pub fn address(&self) -> Option<std::net::SocketAddr> {
        self.server.as_ref().map(|s| s.addr())
    }

    /// `Port 3000 · Delay 0 ms · No throttling · No failures`.
    fn summary(&self) -> String {
        let net = network_label(self.network);
        let fail = if self.fail_rate <= 0.0 {
            tr("No failures").to_string()
        } else {
            let kind = match self.fail_kind {
                FailKind::Status => format!("{} {}", tr("Error status"), self.fail_status),
                FailKind::Drop => tr("Drop connection").to_string(),
                FailKind::Timeout => tr("Timeout").to_string(),
            };
            format!("{} {}% · {kind}", tr("Failures"), (self.fail_rate * 100.0).round())
        };
        let port = self.address().map_or(self.port, |a| a.port());
        format!("{} {port} · {} {} ms · {net} · {fail}", tr("Port"), tr("Delay"), self.delay_ms)
    }
}

/// The name of a network preset, `No throttling`, or `Custom`.
fn network_label(n: NetworkConditions) -> String {
    if n == NetworkConditions::default() {
        return tr("No throttling").to_string();
    }
    match NetworkConditions::PRESETS.iter().find(|(_, p)| *p == n) {
        Some((name, _)) => name.to_string(),
        None => tr("Custom").to_string(),
    }
}

fn start(app: &mut TernaryWireApp, tab_id: u64, tab: &mut MockTab, collection: Collection) {
    tab.starting = true;
    let mut vars = app.ws.scopes_for(Some(&tab.collection));
    if !tab.use_env {
        vars.environment.clear();
    }
    let config = MockConfig {
        port: tab.port,
        delay_ms: tab.delay_ms,
        variables: vars,
        network: tab.network,
        failure: (tab.fail_rate > 0.0).then_some(FailureSimulation {
            rate: tab.fail_rate,
            kind: match tab.fail_kind {
                FailKind::Status => FailureKind::Status(tab.fail_status),
                FailKind::Drop => FailureKind::Drop,
                FailKind::Timeout => FailureKind::Timeout,
            },
        }),
        ..Default::default()
    };
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let result = MockServer::start(collection, config).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::Mock { tab: tab_id, result });
        ctx.request_repaint();
    });
}

/// Points `{{baseUrl}}` (of the active environment, else the collection) at `url`.
fn point_base_url(app: &mut TernaryWireApp, collection: &Id, url: &str) {
    match app.ws.data.active_environment.clone() {
        Some(env_id) => {
            if let Some(env) = app.ws.data.environments.iter_mut().find(|e| e.id == env_id) {
                env.set_current("baseUrl", url);
            }
            app.ws.mark_env(&env_id);
        }
        None => {
            if let Some(c) = app.ws.data.collection_mut(collection) {
                match c.variables.iter_mut().find(|v| v.key == "baseUrl") {
                    Some(v) => v.current = Some(url.to_string()),
                    None => {
                        let mut v = ternarywire_core::variables::Variable::new("baseUrl", "");
                        v.current = Some(url.to_string());
                        c.variables.push(v);
                    }
                }
            }
            let cid = collection.clone();
            app.ws.mark_collection(&cid);
        }
    }
    app.toast(tr("baseUrl now points at the mock server"), false);
}

/// Automation: sends `METHOD /path [body]` to `base`.
fn hit(app: &TernaryWireApp, base: &str, spec: &str) {
    let mut parts = spec.splitn(3, ' ');
    let method = parts.next().unwrap_or("GET").to_string();
    let url = format!("{base}{}", parts.next().unwrap_or("/"));
    let body = parts.next().map(str::to_string);
    let Ok(client) = app.http.default_client() else { return };
    app.rt.spawn(async move {
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
        let mut req = client.request(m, &url);
        if let Some(b) = body {
            req = req.header("content-type", "application/json").body(b);
        }
        let _ = req.send().await;
    });
}

/// What the detail pane asked for this frame.
#[derive(Default)]
struct Acts {
    close: bool,
    open_request: Option<String>,
    copy: Option<String>,
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut MockTab) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let Some(collection) = app.ws.data.collection(&tab.collection).cloned() else {
        kit::empty_state(
            ui,
            icons::CUBE,
            tr("This collection was deleted."),
            tr("Close this tab, or open a mock server from another collection."),
            |_| {},
        );
        return;
    };
    // Keep the running server in sync with edits to the collection.
    if let Some(s) = &tab.server {
        s.set_collection(collection.clone());
    }
    let id = ui.make_persistent_id("mock-view");
    let mut do_start = false;
    let mut do_stop = false;
    let mut clear_logs = false;
    let mut select_row: Option<usize> = None;
    match tab.auto.take().as_deref() {
        Some("start") => do_start = true,
        Some("stop") => do_stop = true,
        Some("routes") => tab.sub = Sub::Routes,
        Some("logs") => tab.sub = Sub::Logs,
        Some("settings") => tab.settings_open = !tab.settings_open,
        Some(a) if a.starts_with("select") => {
            select_row = Some(a.strip_prefix("select:").and_then(|n| n.parse().ok()).unwrap_or(0));
        }
        Some(a) if a.starts_with("hit:") => {
            if let Some(s) = &tab.server {
                hit(app, &s.url(), &a[4..]);
            }
        }
        _ => {}
    }
    let running = tab.is_running();
    let (state, label) = if running {
        (State::Live, tr("Running"))
    } else if tab.starting {
        (State::Busy, tr("Starting…"))
    } else {
        (State::Idle, tr("Stopped"))
    };
    let url = tab.server.as_ref().map(|s| s.url());
    if running {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }

    // Title and address bar.
    let subtitle = trf("{} · examples saved on requests are served by method and path", &[&collection.name]);
    let mut copy_addr = false;
    sv::tool_title(ui, icons::CUBE, tr("Mock server"), &subtitle, |ui| {
        if running {
            copy_addr = kit::menu_item(ui, Some(icons::COPY), tr("Copy address"), None).clicked();
            clear_logs |= kit::menu_item(ui, Some(icons::TRASH), tr("Clear"), None).clicked();
            do_stop |= kit::menu_item(ui, Some(icons::STOP), tr("Stop"), None).clicked();
        } else {
            do_start |= ui
                .add_enabled_ui(!tab.starting, |ui| kit::menu_item(ui, Some(icons::PLAY), tr("Start"), None))
                .inner
                .clicked();
        }
    });
    let placeholder = trf("Starts on {}", &[&format!("127.0.0.1:{}", tab.port)]);
    let env_name = app.ws.data.active_environment().map(|e| e.name.clone());
    let mut point = false;
    let copied = sv::server_bar(
        ui,
        id.with("bar"),
        sv::Bar { state, label, addr: url.as_deref(), placeholder: &placeholder, max_w: 480.0 },
        |_| {},
        |ui| match state {
            State::Live => {
                do_stop |= kit::secondary(ui, Some(icons::STOP), tr("Stop")).clicked();
                let hint = match &env_name {
                    Some(n) => trf("Set {{baseUrl}} in \"{}\" to this URL", &[n]),
                    None => tr("Set collection {{baseUrl}} to this URL").to_string(),
                };
                point = sv::rich_button(ui, icons::CROSSHAIR, tr("Use as {{baseUrl}}"), "{{baseUrl}}")
                    .on_hover_text(hint)
                    .clicked();
            }
            State::Busy => {
                ui.add_enabled_ui(false, |ui| kit::primary_icon(ui, icons::PLAY, tr("Start")));
            }
            _ => do_start |= kit::primary_icon(ui, icons::PLAY, tr("Start")).clicked(),
        },
    );
    if copied || copy_addr {
        if let Some(u) = &url {
            ui.ctx().copy_text(u.clone());
        }
        app.toast(tr("Copied"), false);
    }
    if point && let Some(u) = &url {
        point_base_url(app, &tab.collection, u);
    }

    // Settings.
    let summary = vec![(tab.summary(), false)];
    let tip = if tab.settings_open { tr("Hide settings") } else { tr("Show settings") };
    let lock = running.then(|| tr("Stop the server to change these settings."));
    sv::strip(ui, id.with("strip"), &mut tab.settings_open, tr("Settings"), &summary, lock, tip);
    if tab.settings_open {
        sv::settings_body(ui, |ui| settings(ui, id, tab, running));
    }

    // Routes | logs on the left, the selected one on the right.
    let routes = match &tab.server {
        Some(s) => s.routes(),
        None => ternarywire_mock::routes(&collection),
    };
    let logs = tab.server.as_ref().map(|s| s.logs()).unwrap_or_default();
    if let Some(n) = select_row {
        // Rows are newest first.
        if let Some(i) = logs.len().checked_sub(n + 1) {
            tab.selected_log = Some((i, logs[i].clone()));
            tab.sub = Sub::Logs;
        }
    }
    let selected = locate(&logs, &mut tab.selected_log);
    let dialog_open = app.dialog.is_some();
    let mut acts = Acts::default();
    let cell = RefCell::new(&mut *tab);
    sv::panes(
        ui,
        id.with("panes"),
        0.46,
        |ui| {
            let mut t = cell.borrow_mut();
            clear_logs |= left_pane(ui, &mut t, running, &routes, &logs, selected);
        },
        |ui| {
            let t = cell.borrow();
            match t.sub {
                Sub::Logs => match selected.and_then(|i| logs.get(i)).filter(|_| running) {
                    Some(l) => {
                        log_detail(ui, l, &routes, &collection, url.as_deref().unwrap_or(""), &mut acts);
                        acts.close |= !dialog_open && sv::esc_closes(ui);
                    }
                    None => sv::select_hint(
                        ui,
                        if running {
                            tr("Its matched route, headers and body appear here.")
                        } else {
                            tr("Requests to the mock server appear here.")
                        },
                    ),
                },
                Sub::Routes => match routes.get(t.selected_route).or(routes.first()) {
                    Some(r) => route_detail(ui, r, &collection, &mut acts),
                    None => kit::empty_state(
                        ui,
                        icons::BOOKMARK_SIMPLE,
                        tr("No examples yet"),
                        tr("Send a request and use “Save as example” on the response."),
                        |_| {},
                    ),
                },
            }
        },
    );
    if acts.close {
        tab.selected_log = None;
    }
    if let Some(item) = acts.open_request {
        app.open_item(ItemRef { collection: tab.collection.clone(), item });
    }
    if let Some(text) = acts.copy {
        ui.ctx().copy_text(text);
        app.toast(tr("Copied"), false);
    }
    if clear_logs && let Some(s) = &tab.server {
        s.clear_logs();
        tab.selected_log = None;
    }
    if do_stop && let Some(s) = tab.server.take() {
        app.rt.spawn(s.stop());
    }
    if do_start && tab.server.is_none() && !tab.starting {
        start(app, tab_id, tab, collection);
    }
}

/// The Server, Network and Failures groups (read-only while running).
fn settings(ui: &mut Ui, id: egui::Id, tab: &mut MockTab, running: bool) {
    let p = theme::pal(ui);
    let net_now = tab.network;
    let mut preset_pick = None;
    let MockTab { port, delay_ms, use_env, network, fail_rate, fail_status, fail_kind, .. } = tab;
    sv::settings_grid(
        ui,
        vec![
            Box::new(|ui: &mut Ui| {
                sv::group(ui, tr("Server"), running, None, |ui| {
                    let need = sv::label_w(ui, tr("Port")) + 70.0 + 16.0 + sv::label_w(ui, tr("Delay (ms)")) + 70.0;
                    sv::fields(ui, need, |ui| {
                        sv::field(ui, tr("Port"), |ui| {
                            let mut v = *port as u64;
                            sv::num_input(ui, id.with("port"), &mut v, 0..=65535, 70.0, tr("Port"));
                            *port = v as u16;
                        });
                        sv::field(ui, tr("Delay (ms)"), |ui| {
                            sv::num_input(ui, id.with("delay"), delay_ms, 0..=60_000, 70.0, tr("Delay (ms)"));
                        });
                    });
                    sv::check_line(ui, use_env, tr("Resolve variables with the active environment"));
                });
            }),
            Box::new(|ui: &mut Ui| {
                sv::group(ui, tr("Network"), running, None, |ui| {
                    sv::field(ui, tr("Preset"), |ui| {
                        let r = kit::select_box_ex(ui, None, None, &network_label(net_now), 150.0, true, false);
                        widgets::a11y_name(&r, tr("Network"));
                        egui::Popup::menu(&r).width(200.0).show(|ui| {
                            let none = NetworkConditions::default();
                            let item = |ui: &mut Ui, on: bool, label: &str| {
                                kit::menu_item_ex(ui, kit::MenuItem { checked: Some(on), label, ..Default::default() })
                                    .clicked()
                            };
                            if item(ui, net_now == none, tr("No throttling")) {
                                preset_pick = Some(none);
                            }
                            for (name, n) in NetworkConditions::PRESETS {
                                if item(ui, net_now == n, name) {
                                    preset_pick = Some(n);
                                }
                            }
                        });
                    });
                    let need =
                        sv::label_w(ui, tr("Latency (ms)")) + 70.0 + 16.0 + sv::label_w(ui, tr("Jitter (ms)")) + 70.0;
                    sv::fields(ui, need, |ui| {
                        sv::field(ui, tr("Latency (ms)"), |ui| {
                            sv::num_input(
                                ui,
                                id.with("latency"),
                                &mut network.latency_ms,
                                0..=60_000,
                                70.0,
                                tr("Latency (ms)"),
                            );
                        });
                        sv::field(ui, tr("Jitter (ms)"), |ui| {
                            sv::num_input(
                                ui,
                                id.with("jitter"),
                                &mut network.jitter_ms,
                                0..=10_000,
                                70.0,
                                tr("Jitter (ms)"),
                            );
                        });
                    });
                    sv::hrow(ui, 26.0, |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let bps = &mut network.bandwidth_bytes_per_sec;
                        let mut limited = bps.is_some();
                        if sv::check_line(ui, &mut limited, tr("Bandwidth (KB/s)")).changed() {
                            *bps = limited.then_some(100_000);
                        }
                        if let Some(bps) = bps {
                            let mut kb = *bps / 1000;
                            sv::num_input(ui, id.with("kbps"), &mut kb, 1..=1_000_000, 70.0, tr("Bandwidth (KB/s)"));
                            *bps = kb * 1000;
                        }
                    });
                });
            }),
            Box::new(|ui: &mut Ui| {
                sv::group(ui, tr("Failures"), running, None, |ui| {
                    let pct_w = ui
                        .painter()
                        .layout_no_wrap("%".into(), egui::FontId::proportional(12.0), p.text_faint)
                        .size()
                        .x;
                    let need = 56.0 + 8.0 + pct_w + 16.0 + sv::label_w(ui, tr("Status")) + 64.0;
                    sv::fields(ui, need, |ui| {
                        sv::field(ui, "", |ui| {
                            let mut pct = (*fail_rate * 100.0).round() as u64;
                            sv::num_input(ui, id.with("fail-rate"), &mut pct, 0..=100, 56.0, tr("Failures"))
                                .on_hover_text(tr("Share of requests that fail"));
                            *fail_rate = pct as f32 / 100.0;
                            ui.label(RichText::new("%").size(12.0).color(p.text_faint));
                        });
                        sv::field(ui, tr("Status"), |ui| {
                            ui.add_enabled_ui(*fail_kind == FailKind::Status, |ui| {
                                let mut v = *fail_status as u64;
                                sv::num_input(ui, id.with("fail-status"), &mut v, 100..=599, 64.0, tr("Status code"))
                                    .on_hover_text(tr("Status code"));
                                *fail_status = v as u16;
                            });
                        });
                    });
                    sv::seg_track(ui, 26.0);
                    ui.push_id("fail-kind", |ui| {
                        kit::segmented_sm(
                            ui,
                            fail_kind,
                            &[
                                (FailKind::Status, tr("Error status")),
                                (FailKind::Drop, tr("Drop connection")),
                                (FailKind::Timeout, tr("Timeout")),
                            ],
                        )
                    });
                });
            }),
        ],
    );
    if let Some(n) = preset_pick {
        tab.network = n;
    }
}

/// Routes | Logs: the tabs, a toolbar and the list. Returns true when the
/// log was cleared.
fn left_pane(
    ui: &mut Ui,
    tab: &mut MockTab,
    running: bool,
    routes: &[MockRoute],
    logs: &[MockLogEntry],
    selected: Option<usize>,
) -> bool {
    let mut clear = false;
    let mut logs_tab = Tab::new(Sub::Logs, tr("Logs"));
    if running {
        logs_tab = logs_tab.badge(logs.len().to_string(), Tone::Neutral);
    }
    ui.scope(|ui| {
        // kit::tabs lays its 35 px tabs in a `ui.horizontal`, which is only
        // as high as the interact size: make it fit them.
        ui.spacing_mut().interact_size.y = kit::size::TABS - 1.0;
        kit::tabs(
            ui,
            &mut tab.sub,
            &[Tab::new(Sub::Routes, tr("Routes")).badge(routes.len().to_string(), Tone::Neutral), logs_tab],
        );
    });
    match tab.sub {
        Sub::Logs => {
            kit::toolbar(ui, |ui| {
                let r = kit::search_field_sm(ui, "mock-filter", &mut tab.filter, tr("Filter paths"), 200.0);
                widgets::a11y_name(&r, tr("Filter paths"));
                if running {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let r =
                            ui.add_enabled_ui(!logs.is_empty(), |ui| kit::ghost(ui, Some(icons::TRASH), tr("Clear")));
                        clear = r.inner.clicked();
                    });
                }
            });
            sv::list_head(
                ui,
                &LOG_COLS,
                &[tr("Time"), tr("Method"), tr("Status"), tr("Path"), tr("Duration")],
                &[4],
                None,
            );
            if !running || logs.is_empty() {
                let body = if running {
                    tr("Requests to the mock server appear here.")
                } else {
                    tr("Start the server, then send requests to it.")
                };
                kit::empty_state(ui, icons::LIST, tr("No requests yet"), body, |_| {});
                return clear;
            }
            let q = tab.filter.trim().to_lowercase();
            // Newest first.
            let shown: Vec<usize> = (0..logs.len())
                .rev()
                .filter(|&i| {
                    q.is_empty() || logs[i].path.to_lowercase().contains(&q) || logs[i].method.eq_ignore_ascii_case(&q)
                })
                .collect();
            if shown.is_empty() {
                kit::empty_state(ui, icons::FUNNEL, tr("No requests match"), "", |_| {});
                return clear;
            }
            let dropped = tr("dropped");
            let no_match = tr("no match");
            sv::list_scroll(ui, "mock-logs", shown.len(), false, |ui, range| {
                for &i in &shown[range] {
                    let l = &logs[i];
                    let time = sv::clock_ms(l.timestamp_ms);
                    let dur =
                        if l.status == 0 { "—".to_string() } else { ternarywire_core::util::human_ms(l.duration_ms) };
                    let status = if l.status == 0 { Stat::Error(dropped) } else { Stat::Code(l.status) };
                    let unmatched = l.matched.is_none() && l.status == 404;
                    let cells = [
                        Cell::Ts(&time),
                        Cell::Method(&l.method),
                        Cell::Status(status),
                        Cell::Path {
                            host: "",
                            path: &l.path,
                            query: &l.query,
                            pill: unmatched.then_some((no_match, Tone::Warn)),
                        },
                        Cell::Ts(&dur),
                    ];
                    let name = format!("{} {}", l.method, l.path);
                    let r = sv::list_row(ui, &LOG_COLS, &cells, &[4], selected == Some(i), &name, None);
                    let r = r.on_hover_text(l.matched.clone().unwrap_or_else(|| no_match.to_string()));
                    if r.clicked() {
                        tab.selected_log = Some((i, l.clone()));
                    }
                }
            });
        }
        Sub::Routes => {
            kit::toolbar(ui, |ui| {
                ui.label(kit::hint(ui, tr("Requests with saved examples, matched by method and path")));
            });
            sv::list_head(ui, &ROUTE_COLS, &[tr("Method"), tr("Path"), tr("Request"), tr("Examples")], &[3], None);
            if routes.is_empty() {
                kit::empty_state(
                    ui,
                    icons::BOOKMARK_SIMPLE,
                    tr("No examples yet"),
                    tr("Send a request and use “Save as example” on the response."),
                    |_| {},
                );
                return clear;
            }
            let sel = tab.selected_route.min(routes.len() - 1);
            sv::list_scroll(ui, "mock-routes", routes.len(), false, |ui, range| {
                for i in range {
                    let r = &routes[i];
                    let n = r.examples.len();
                    let count = if n == 1 { tr("1 example").to_string() } else { trf("{} examples", &[&n]) };
                    let cells = [
                        Cell::Method(&r.method),
                        Cell::Path { host: "", path: &r.path, query: "", pill: None },
                        Cell::Text { text: &r.request_name, muted: true, mono: false },
                        Cell::Pill(&count, Tone::Neutral),
                    ];
                    let name = format!("{} {}", r.method, r.path);
                    let resp = sv::list_row(ui, &ROUTE_COLS, &cells, &[3], i == sel, &name, None);
                    if resp.on_hover_text(&r.request_name).clicked() {
                        tab.selected_route = i;
                    }
                }
            });
        }
    }
    clear
}

/// The route and example a log entry was answered with.
fn matched_route<'a>(l: &MockLogEntry, routes: &'a [MockRoute]) -> Option<(&'a MockRoute, &'a str)> {
    let label = l.matched.as_deref()?;
    let mut fallback = None;
    for r in routes {
        for ex in &r.examples {
            if format!("{} / {ex}", r.request_name) == label {
                if r.method.eq_ignore_ascii_case(&l.method) {
                    return Some((r, ex.as_str()));
                }
                fallback.get_or_insert((r, ex.as_str()));
            }
        }
    }
    fallback
}

fn example<'a>(c: &'a Collection, request: &str, name: &str) -> Option<&'a Example> {
    c.find_request(request)?.examples.iter().find(|e| e.name == name)
}

fn content_type(e: &Example) -> String {
    match e.headers.iter().find(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type")) {
        Some(h) => h.value.clone(),
        None => match e.language.as_deref() {
            Some("json") => "application/json".into(),
            Some("xml") => "application/xml".into(),
            Some("html") => "text/html".into(),
            None if looks_json(&e.body) => "application/json".into(),
            _ => "text/plain".into(),
        },
    }
}

fn looks_json(s: &str) -> bool {
    let s = s.trim_start();
    s.starts_with('{') || s.starts_with('[')
}

/// A received request: what answered it, its headers and body.
fn log_detail(ui: &mut Ui, l: &MockLogEntry, routes: &[MockRoute], c: &Collection, base: &str, acts: &mut Acts) {
    let p = theme::pal(ui);
    sv::detail_head(
        ui,
        |ui| {
            sv::method_tag(ui, &l.method);
            sv::path_label(ui, "", &l.path, &l.query);
        },
        |ui| {
            acts.close |= kit::icon_button_sm(ui, icons::X, tr("Close (Esc)")).clicked();
            if l.status == 0 {
                let tone = sv::code_tone(color_status(l.status));
                kit::pill_ex(ui, tr("Connection dropped"), tone, kit::PillOpts { large: true, ..Default::default() });
            } else {
                kit::status_pill(ui, l.status, sv::reason(l.status));
            }
        },
    );
    sv::detail_meta(
        ui,
        |ui| {
            sv::metrics(ui, |ui| {
                sv::metric(ui, Some(icons::CLOCK), &sv::clock_ms(l.timestamp_ms), Some(tr("Received")));
                if l.status != 0 {
                    sv::metric(
                        ui,
                        Some(icons::TIMER),
                        &ternarywire_core::util::human_ms(l.duration_ms),
                        Some(tr("Duration")),
                    );
                }
            });
        },
        |ui| {
            if kit::ghost(ui, Some(icons::TERMINAL), tr("Copy as cURL")).clicked() {
                let q = if l.query.is_empty() { String::new() } else { format!("?{}", l.query) };
                acts.copy = Some(sv::curl(&l.method, &format!("{base}{}{q}", l.path), &l.headers, &l.body));
            }
        },
    );
    sv::detail_scroll(ui, "mock-log-detail", |ui| {
        match matched_route(l, routes) {
            Some((r, ex_name)) => {
                kit::card(ui).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.0;
                    sv::hrow(ui, 24.0, |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(RichText::new(icons::CHECK_CIRCLE).font(theme::icon_font(14.0)).color(p.ok));
                        ui.label(RichText::new(tr("Matched")).font(theme::medium(13.0)).color(p.text));
                        sv::method_tag(ui, &r.method);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if kit::ghost(ui, Some(icons::ARROW_SQUARE_OUT), tr("Open request")).clicked() {
                                acts.open_request = Some(r.request_id.clone());
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.label(RichText::new(&r.path).font(theme::mono(12.0)).color(p.text));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("→ {}", r.request_name)).color(p.text_muted),
                                    )
                                    .truncate(),
                                );
                            });
                        });
                    });
                    let detail = match example(c, &r.request_id, ex_name) {
                        Some(e) => {
                            format!("{} · {} · {}", trf("Served example “{}”", &[&e.name]), l.status, content_type(e))
                        }
                        None => trf("Served example “{}”", &[&ex_name]),
                    };
                    sv::hrow(ui, 18.0, |ui| {
                        ui.add_space(22.0);
                        ui.add(egui::Label::new(kit::hint(ui, &detail)).wrap());
                    });
                });
            }
            None if l.status == 0 => {
                kit::banner(
                    ui,
                    Tone::Err,
                    icons::WARNING_CIRCLE,
                    tr("Dropped by failure simulation (Failures › Drop connection) before the example was sent."),
                );
            }
            None if l.status == 404 => {
                let what = format!("{} {}", l.method, l.path);
                kit::banner(
                    ui,
                    Tone::Warn,
                    icons::WARNING,
                    &trf(
                        "No match. No request in “{}” has a saved example for {}, so the server answered 404.",
                        &[&c.name, &what],
                    ),
                );
            }
            None => {
                kit::banner(
                    ui,
                    Tone::Info,
                    icons::INFO,
                    tr("Answered by failure simulation (Failures › Error status)."),
                );
            }
        }
        let rows: Vec<(&str, &str)> = l.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        sv::section(
            ui,
            tr("Headers"),
            Some(rows.len()),
            |_| {},
            |ui| {
                if rows.is_empty() {
                    sv::no_body(ui, tr("No headers"));
                } else {
                    sv::headers_table(ui, ui.make_persistent_id("mock-log-headers"), &rows);
                }
            },
        );
        let mut copy_body = false;
        sv::section(
            ui,
            tr("Body"),
            None,
            |ui| {
                if !l.body.is_empty() {
                    copy_body = kit::icon_button_sm(ui, icons::COPY, tr("Copy body")).clicked();
                }
            },
            |ui| {
                if l.body.is_empty() {
                    sv::no_body(ui, tr("No body"));
                } else {
                    let text = crate::proto_common::payload_text(l.body.as_bytes());
                    sv::body_box(ui, ui.make_persistent_id(("mock-log-body", l.timestamp_ms)), &text);
                }
            },
        );
        if copy_body {
            acts.copy = Some(l.body.clone());
        }
    });
}

/// A route: the request it comes from and the examples it serves.
fn route_detail(ui: &mut Ui, r: &MockRoute, c: &Collection, acts: &mut Acts) {
    let p = theme::pal(ui);
    sv::detail_head(
        ui,
        |ui| {
            sv::method_tag(ui, &r.method);
            sv::path_label(ui, "", &r.path, "");
            ui.add(egui::Label::new(RichText::new(&r.request_name).color(p.text_muted)).truncate());
        },
        |ui| {
            if kit::ghost(ui, Some(icons::ARROW_SQUARE_OUT), tr("Open request")).clicked() {
                acts.open_request = Some(r.request_id.clone());
            }
        },
    );
    sv::detail_meta(
        ui,
        |ui| {
            let text = tr("Served when method and path match. Path variables like :id match any value.");
            let mut job = egui::text::LayoutJob::default();
            let plain = egui::TextFormat::simple(egui::FontId::proportional(12.0), p.text_muted);
            let code = egui::TextFormat::simple(theme::mono(12.0), p.text_muted);
            match text.find(":id") {
                Some(i) => {
                    job.append(&text[..i], 0.0, plain.clone());
                    job.append(":id", 0.0, code);
                    job.append(&text[i + 3..], 0.0, plain);
                }
                None => job.append(text, 0.0, plain),
            }
            job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width());
            ui.add(egui::Label::new(job).selectable(false));
        },
        |_| {},
    );
    let examples: Vec<&Example> = r.examples.iter().filter_map(|n| example(c, &r.request_id, n)).collect();
    let default = examples.iter().position(|e| (200..300).contains(&e.status)).unwrap_or(0);
    sv::detail_scroll(ui, ("mock-route", &r.request_id, &r.method, &r.path), |ui| {
        sv::section(
            ui,
            tr("Examples"),
            Some(examples.len()),
            |ui| {
                ui.label(kit::hint(ui, tr("Edit in the request’s Docs tab")));
            },
            |ui| {
                for (i, e) in examples.iter().enumerate() {
                    example_card(ui, e, i == default, i == 0);
                }
            },
        );
        kit::banner(
            ui,
            Tone::Info,
            icons::INFO,
            tr(
                "Add examples with “Save as example” on a response; they are stored with the request and committed with the collection.",
            ),
        );
    });
}

/// One example of a route: status, name, kind; opens to show its body.
fn example_card(ui: &mut Ui, e: &Example, is_default: bool, open_at_first: bool) {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id(("mock-example", &e.id));
    let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(open_at_first));
    kit::card(ui).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let head = ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 20.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
                ui.label(RichText::new(caret).font(theme::icon_font(12.5)).color(p.text_faint));
                kit::tone_pill(ui, &e.status.to_string(), sv::code_tone(e.status));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_default {
                        kit::tone_pill(ui, tr("default"), Tone::Accent);
                    }
                    let kind = match e.language.as_deref() {
                        Some(l) => l.to_uppercase(),
                        None if looks_json(&e.body) => "JSON".into(),
                        None => "TEXT".into(),
                    };
                    ui.label(RichText::new(kind).size(11.0).color(p.text_faint));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&e.name).font(theme::medium(13.0)).color(p.text)).truncate(),
                        );
                    });
                });
            },
        );
        let r = ui.interact(head.response.rect, id.with("toggle"), egui::Sense::click());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &e.name));
        r.ctx.accesskit_node_builder(r.id, |n| n.set_expanded(open));
        if r.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(id, open));
        }
        if open {
            ui.add_space(10.0);
            if e.body.is_empty() {
                sv::no_body(ui, tr("No body"));
            } else {
                let text = crate::proto_common::payload_text(e.body.as_bytes());
                sv::body_box(ui, id.with("body"), &text);
            }
        }
    });
}

/// Where the selected entry is now. Entries only move towards the front (the
/// oldest are dropped); the selection is cleared once its entry is gone.
fn locate(logs: &[MockLogEntry], selected: &mut Option<(usize, MockLogEntry)>) -> Option<usize> {
    let (hint, entry) = selected.as_mut()?;
    match (0..=(*hint).min(logs.len().saturating_sub(1))).rev().find(|&i| logs.get(i) == Some(&*entry)) {
        Some(i) => {
            *hint = i;
            Some(i)
        }
        None => {
            *selected = None;
            None
        }
    }
}

/// Status used to colour a log entry (0 = dropped, shown like a server error).
fn color_status(status: u16) -> u16 {
    if status == 0 { 500 } else { status }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: i64) -> MockLogEntry {
        MockLogEntry {
            timestamp_ms: n,
            method: "GET".into(),
            path: format!("/{n}"),
            query: String::new(),
            headers: Vec::new(),
            body: String::new(),
            matched: None,
            status: 200,
            duration_ms: 1.0,
        }
    }

    /// The selected request stays selected while a full log drops its
    /// oldest entries (the index used to point at a newer request), and the
    /// selection goes once its entry is dropped too.
    #[test]
    fn selection_follows_its_entry_in_a_full_log() {
        let window = |from: i64| (from..from + 500).map(entry).collect::<Vec<_>>();
        let mut selected = Some((5, entry(5)));
        assert_eq!(locate(&window(0), &mut selected), Some(5));
        assert_eq!(locate(&window(3), &mut selected), Some(2));
        assert_eq!(selected.as_ref().map(|(_, e)| e.path.as_str()), Some("/5"));
        assert_eq!(locate(&window(5), &mut selected), Some(0));
        assert_eq!(locate(&window(6), &mut selected), None);
        assert!(selected.is_none());
        let mut selected = Some((0, entry(0)));
        assert_eq!(locate(&[], &mut selected), None);
    }

    #[test]
    fn log_rows_are_coloured_by_their_status() {
        assert_eq!(color_status(0), 500);
        assert_eq!(color_status(200), 200);
        assert_eq!(color_status(404), 404);
    }

    /// A log entry finds the route and example that answered it, preferring
    /// the route with its method.
    #[test]
    fn log_entries_find_their_route() {
        let route = |m: &str| MockRoute {
            method: m.into(),
            path: "/books".into(),
            request_id: m.into(),
            request_name: "Books".into(),
            examples: vec!["Page".into()],
        };
        let routes = [route("GET"), route("POST")];
        let mut l = entry(1);
        l.method = "POST".into();
        l.matched = Some("Books / Page".into());
        assert_eq!(matched_route(&l, &routes).map(|(r, e)| (r.request_id.as_str(), e)), Some(("POST", "Page")));
        l.matched = Some("Other / Page".into());
        assert!(matched_route(&l, &routes).is_none());
    }
}
