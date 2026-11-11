//! Local webhook listener (FR-X10): records incoming requests and can replay
//! them to another URL. A public URL needs a tunnel set up by the user.

use std::cell::RefCell;

use egui::{Align, Layout, Rect, RichText, Ui, UiBuilder, pos2, vec2};

use ternarywire_mock::listener::{ListenerConfig, ReplayResult, WebhookEvent, WebhookListener};

use crate::app::{Msg, TernaryWireApp};
use crate::i18n::{tr, trf};
use crate::kit::{self, Tone};
use crate::server_ui::{self as sv, Cell, Col, State};
use crate::{icons, theme, widgets};

const COLS: [Col; 4] = [Col::Px(92.0), Col::Px(64.0), Col::Fr(1.0), Col::Px(62.0)];

pub enum WebhookMsg {
    Started(Result<WebhookListener, String>),
    Replayed(Result<ReplayResult, String>),
}

pub struct WebhookTab {
    listener: Option<WebhookListener>,
    starting: bool,
    port: u16,
    status: u16,
    body: String,
    selected: Option<u64>,
    replay_target: String,
    replay: Option<Result<ReplayResult, String>>,
    replaying: bool,
    /// Why the listener could not start.
    start_error: Option<String>,
    filter: String,
    settings_open: bool,
    /// Automation: `start`, `stop`, `settings`, `select[:n]` (the latest
    /// request, or the n-th row from the top), `hit:<METHOD> <path> [body]`,
    /// `replay:<url>` (replays the selected request to the URL).
    pub auto: Option<String>,
}

impl Default for WebhookTab {
    fn default() -> Self {
        WebhookTab {
            listener: None,
            starting: false,
            port: 0,
            status: 200,
            body: "{\"ok\":true}".into(),
            selected: None,
            replay_target: "http://localhost:3000".into(),
            replay: None,
            replaying: false,
            start_error: None,
            filter: String::new(),
            settings_open: true,
            auto: None,
        }
    }
}

impl WebhookTab {
    pub fn on_msg(&mut self, m: WebhookMsg) {
        match m {
            WebhookMsg::Started(r) => {
                self.starting = false;
                match r {
                    Ok(l) => self.listener = Some(l),
                    Err(e) => self.start_error = Some(e),
                }
            }
            WebhookMsg::Replayed(r) => {
                self.replaying = false;
                self.replay = Some(r)
            }
        }
    }

    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    pub fn is_running(&self) -> bool {
        self.listener.is_some()
    }

    /// The listener's address while it runs.
    pub fn address(&self) -> Option<std::net::SocketAddr> {
        self.listener.as_ref().map(|l| l.addr())
    }

    /// Requests recorded so far.
    pub fn request_count(&self) -> usize {
        self.listener.as_ref().map_or(0, |l| l.events().len())
    }

    pub fn stop(&mut self, rt: &tokio::runtime::Runtime) {
        if let Some(l) = self.listener.take() {
            rt.spawn(l.stop());
        }
    }

    fn state(&self) -> (State, &'static str) {
        if self.listener.is_some() {
            (State::Live, tr("Listening"))
        } else if self.starting {
            (State::Busy, tr("Starting…"))
        } else if self.start_error.is_some() {
            (State::Failed, tr("Not started"))
        } else {
            (State::Idle, tr("Stopped"))
        }
    }
}

fn start(app: &mut TernaryWireApp, tab_id: u64, tab: &mut WebhookTab) {
    tab.starting = true;
    tab.start_error = None;
    let cfg = ListenerConfig {
        port: tab.port,
        response_status: tab.status,
        response_body: tab.body.clone(),
        ..Default::default()
    };
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let r = WebhookListener::start(cfg).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::Webhook { tab: tab_id, event: WebhookMsg::Started(r) });
        ctx.request_repaint();
    });
}

/// What the panes asked for this frame.
#[derive(Default)]
struct Acts {
    clear: bool,
    close: bool,
    replay: bool,
    copy: Option<String>,
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut WebhookTab) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let id = ui.make_persistent_id("webhook-view");
    let auto = tab.auto.take();
    let mut do_start = auto.as_deref() == Some("start") && tab.listener.is_none() && !tab.starting;
    let mut do_stop = auto.as_deref() == Some("stop");
    let mut auto_replay = false;
    match auto.as_deref() {
        Some("settings") => tab.settings_open = !tab.settings_open,
        Some(a) if a.starts_with("replay:") => {
            tab.replay_target = a[7..].to_string();
            auto_replay = true;
        }
        Some(a) if a.starts_with("select") => {
            if let Some(l) = &tab.listener {
                let events = l.events();
                let n: usize = a.strip_prefix("select:").and_then(|n| n.parse().ok()).unwrap_or(0);
                tab.selected = events.len().checked_sub(n + 1).map(|i| events[i].id);
            }
        }
        Some(a) if a.starts_with("hit:") => {
            if let Some(l) = &tab.listener {
                hit(app, &l.url(), &a[4..]);
            }
        }
        _ => {}
    }
    let (state, label) = tab.state();
    let url = tab.listener.as_ref().map(|l| l.url());
    let running = tab.listener.is_some();
    let events = tab.listener.as_ref().map(|l| l.events()).unwrap_or_default();
    if running {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }

    // Title and address bar.
    let mut acts = Acts { replay: auto_replay, ..Default::default() };
    let mut copy_addr = false;
    sv::tool_title(
        ui,
        icons::WEBHOOKS_LOGO,
        tr("Webhook listener"),
        tr(
            "Records every request sent to the local URL. For a public URL, point a tunnel (ngrok, cloudflared, ...) at it.",
        ),
        |ui| {
            if running {
                copy_addr = kit::menu_item(ui, Some(icons::COPY), tr("Copy address"), None).clicked();
                ui.add_enabled_ui(tab.request_count() > 0, |ui| {
                    acts.clear |= kit::menu_item(ui, Some(icons::TRASH), tr("Clear"), None).clicked();
                });
                do_stop |= kit::menu_item(ui, Some(icons::STOP), tr("Stop"), None).clicked();
            } else {
                do_start |= ui
                    .add_enabled_ui(!tab.starting, |ui| kit::menu_item(ui, Some(icons::PLAY), tr("Start"), None))
                    .inner
                    .clicked();
            }
        },
    );
    let placeholder = if tab.port == 0 {
        tr("Starts on a free port").to_string()
    } else {
        trf("Starts on {}", &[&format!("127.0.0.1:{}", tab.port)])
    };
    let copied = sv::server_bar(
        ui,
        id.with("bar"),
        sv::Bar { state, label, addr: url.as_deref(), placeholder: &placeholder, max_w: 480.0 },
        |_| {},
        |ui| match state {
            State::Live => do_stop |= kit::secondary(ui, Some(icons::STOP), tr("Stop")).clicked(),
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
    if let Some(e) = tab.start_error.clone() {
        let text = format!("{}: {e}", tr("Could not start the listener"));
        sv::error_row(ui, &text, |ui| {
            do_start |=
                kit::button_ex(ui, kit::Kind::Secondary, Some(icons::ARROW_CLOCKWISE), tr("Retry"), true).clicked();
        });
    }

    // Settings.
    let port_now = tab.address().map_or(tab.port, |a| a.port());
    let summary = vec![
        (format!("{} {} · {} ", tr("Port"), port_now, tr("responds")), false),
        (format!("{} {}", tab.status, tab.body), true),
    ];
    let tip = if tab.settings_open { tr("Hide settings") } else { tr("Show settings") };
    sv::strip(ui, id.with("strip"), &mut tab.settings_open, tr("Settings"), &summary, None, tip);
    if tab.settings_open {
        sv::settings_body(ui, |ui| settings(ui, id, tab));
    }

    if !running || events.is_empty() {
        waiting(ui, tab, url.as_deref(), &mut do_start, &mut acts);
    } else {
        let dialog_open = app.dialog.is_some();
        let base = url.clone().unwrap_or_default();
        let cell = RefCell::new(&mut *tab);
        let mut clear = false;
        sv::panes(
            ui,
            id.with("panes"),
            0.42,
            |ui| clear = list(ui, &mut cell.borrow_mut(), &events),
            |ui| {
                let mut t = cell.borrow_mut();
                match t.selected.and_then(|sel| events.iter().find(|e| e.id == sel)) {
                    Some(e) => {
                        detail(ui, &mut t, e, &base, &mut acts);
                        acts.close |= !dialog_open && sv::esc_closes(ui);
                    }
                    None => sv::select_hint(ui, tr("Its headers and body appear here.")),
                }
            },
        );
        acts.clear |= clear;
    }
    if acts.clear
        && let Some(l) = &tab.listener
    {
        l.clear();
        tab.selected = None;
    }
    if acts.close {
        tab.selected = None;
    }
    if let Some(text) = acts.copy {
        ui.ctx().copy_text(text);
        app.toast(tr("Copied"), false);
    }
    if acts.replay
        && let Some(e) = tab.selected.and_then(|sel| events.iter().find(|e| e.id == sel))
    {
        tab.replaying = true;
        tab.replay = None;
        let event = e.clone();
        let target = tab.replay_target.clone();
        let http = app.http.default_client();
        let tx = app.tx.clone();
        let ctx = app.ctx.clone();
        app.rt.spawn(async move {
            let r = match http {
                Ok(client) => ternarywire_mock::listener::replay(&event, &target, &client).await,
                Err(e) => Err(e.message),
            };
            let _ = tx.send(Msg::Webhook { tab: tab_id, event: WebhookMsg::Replayed(r) });
            ctx.request_repaint();
        });
    }
    run(app, tab_id, tab, do_start, do_stop);
}

/// Automation: sends `METHOD /path [body]` to `base`.
fn hit(app: &TernaryWireApp, base: &str, spec: &str) {
    let mut parts = spec.splitn(3, ' ');
    let method = parts.next().unwrap_or("POST").to_string();
    let url = format!("{base}{}", parts.next().unwrap_or("/"));
    let body = parts.next().map(str::to_string);
    let Ok(client) = app.http.default_client() else { return };
    app.rt.spawn(async move {
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::POST);
        let mut req = client.request(m, &url);
        if let Some(b) = body {
            req = req.header("content-type", "application/json").body(b);
        }
        let _ = req.send().await;
    });
}

/// Listener (port, locked while listening) | Respond with (status, body, Apply).
fn settings(ui: &mut Ui, id: egui::Id, tab: &mut WebhookTab) {
    let running = tab.listener.is_some();
    let p = theme::pal(ui);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        let port_tip = if running {
            format!("{} · {}", tr("Locked while listening"), tr("0 picks a free port"))
        } else {
            tr("0 picks a free port").to_string()
        };
        ui.scope(|ui| {
            sv::group(ui, tr("Listener"), running, None, |ui| {
                sv::field(ui, tr("Port"), |ui| {
                    let mut v = tab.port as u64;
                    sv::num_input(ui, id.with("port"), &mut v, 0..=65535, 70.0, tr("Port")).on_hover_text(&port_tip);
                    tab.port = v as u16;
                });
            });
        });
        let (rect, _) = ui.allocate_exact_size(vec2(1.0, 52.0), egui::Sense::hover());
        ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(1.0, p.border));
        sv::group(ui, tr("Respond with"), false, None, |ui| {
            sv::field(ui, tr("Status"), |ui| {
                let mut v = tab.status as u64;
                sv::num_input(ui, id.with("status"), &mut v, 100..=599, 60.0, tr("Status code"))
                    .on_hover_text(tr("Status code"));
                tab.status = v as u16;
                let apply_w = if running {
                    let g = ui.painter().layout_no_wrap(tr("Apply").into(), theme::medium(12.0), p.text);
                    8.0 + 13.0 + 5.0 + g.size().x + 8.0 + 8.0
                } else {
                    0.0
                };
                let w = (ui.available_width() - apply_w).max(120.0);
                let r = sv::input(ui, id.with("body"), &mut tab.body, tr("Response body"), w, true);
                widgets::a11y_name(&r, tr("Response body"));
                if let Some(l) = &tab.listener
                    && kit::ghost(ui, Some(icons::CHECK), tr("Apply"))
                        .on_hover_text(tr("Use this response from now on"))
                        .clicked()
                {
                    l.set_response(
                        tab.status,
                        vec![("Content-Type".into(), "application/json".into())],
                        tab.body.clone(),
                    );
                }
            });
        });
    });
}

/// The whole area under the strip while nothing was recorded: how to send
/// a request (running), or Start (stopped).
fn waiting(ui: &mut Ui, tab: &WebhookTab, url: Option<&str>, do_start: &mut bool, acts: &mut Acts) {
    let p = theme::pal(ui);
    match url {
        Some(url) => {
            let curl = format!(
                "curl -X POST {url}/hook \\\n  -H \"Content-Type: application/json\" \\\n  -d '{{\"hello\":\"world\"}}'"
            );
            let port = tab.address().map_or(tab.port, |a| a.port());
            // The prototype centers the empty state's 24 px top padding with
            // the rest (its bottom padding is the gap above the command).
            ui.add_space(24.0);
            kit::empty_state(
                ui,
                icons::HOURGLASS,
                tr("Waiting for requests"),
                tr("Requests sent to the address above appear here. Try:"),
                |ui| {
                    // 24 under the body (the `.empty` padding), less the actions' 16.
                    ui.add_space(8.0);
                    let w = ui.available_width().min(620.0);
                    ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                        egui::Frame::new()
                            .fill(p.well)
                            .stroke(egui::Stroke::new(1.0, p.border))
                            .corner_radius(egui::CornerRadius::same(9))
                            .inner_margin(egui::Margin { left: 14, right: 10, top: 10, bottom: 10 })
                            .show(ui, |ui| {
                                // 620 wide with its 14 + 10 padding and 1 px border.
                                ui.set_width(w - 26.0);
                                // `$`, the command (12 px on 1.45 lines) and Copy, centered on
                                // one another; Copy's width is measured on the previous frame.
                                let copy_w_id = ui.scope_id().with("copy-w");
                                let copy_w: f32 = ui.data(|d| d.get_temp(copy_w_id).unwrap_or(120.0));
                                let dollar = ui.painter().layout_no_wrap("$".into(), theme::mono(12.0), p.text_faint);
                                let text_w = (ui.available_width() - dollar.size().x - copy_w - 20.0).max(80.0);
                                let mut job = egui::text::LayoutJob::single_section(
                                    curl.clone(),
                                    egui::TextFormat {
                                        line_height: Some(12.0 * 1.45),
                                        ..egui::TextFormat::simple(theme::mono(12.0), p.text)
                                    },
                                );
                                job.wrap.max_width = text_w;
                                job.wrap.break_anywhere = true;
                                let g = ui.painter().layout_job(job);
                                sv::hrow(ui, g.size().y.max(24.0), |ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;
                                    let (r, _) = ui.allocate_exact_size(dollar.size(), egui::Sense::hover());
                                    ui.painter().galley(r.min, dollar, p.text_faint);
                                    let (r, _) = ui.allocate_exact_size(vec2(text_w, g.size().y), egui::Sense::hover());
                                    ui.painter().galley(r.min, g, p.text);
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let b = kit::button_ex(
                                            ui,
                                            kit::Kind::Secondary,
                                            Some(icons::COPY),
                                            tr("Copy command"),
                                            true,
                                        );
                                        if (b.rect.width() - copy_w).abs() > 0.5 {
                                            ui.data_mut(|d| d.insert_temp(copy_w_id, b.rect.width()));
                                            ui.ctx().request_repaint();
                                        }
                                        if b.clicked() {
                                            acts.copy = Some(curl.clone());
                                        }
                                    });
                                });
                            });
                    });
                    ui.add_space(14.0);
                    let cmd = format!("ngrok http {port}");
                    let text = trf(
                        "For Stripe, GitHub or Slack, start a tunnel ({}) and paste its URL in the provider.",
                        &[&cmd],
                    );
                    let mut job = egui::text::LayoutJob::default();
                    let line = 12.0 * 1.45;
                    let plain = egui::TextFormat {
                        line_height: Some(line),
                        ..egui::TextFormat::simple(egui::FontId::proportional(12.0), p.text_muted)
                    };
                    let code = egui::TextFormat {
                        line_height: Some(line),
                        ..egui::TextFormat::simple(theme::mono(12.0), p.text_muted)
                    };
                    match text.find(&cmd) {
                        Some(i) => {
                            job.append(&text[..i], 0.0, plain.clone());
                            job.append(&cmd, 0.0, code);
                            job.append(&text[i + cmd.len()..], 0.0, plain);
                        }
                        None => job.append(&text, 0.0, plain),
                    }
                    job.wrap.max_width = ui.available_width() - 30.0;
                    let g = ui.painter().layout_job(job);
                    let ig = ui.painter().layout_no_wrap(icons::INFO.into(), theme::icon_font(12.0), p.text_faint);
                    let size = vec2(ig.size().x + 6.0 + g.size().x, g.size().y.max(line));
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().galley(
                        pos2(rect.left(), rect.top() + line / 2.0 - ig.size().y / 2.0),
                        ig.clone(),
                        p.text_faint,
                    );
                    ui.painter().galley(pos2(rect.left() + ig.size().x + 6.0, rect.top()), g, p.text_muted);
                },
            );
        }
        None => kit::empty_state(
            ui,
            icons::WEBHOOKS_LOGO,
            tr("The listener is stopped"),
            tr("Start it to get a local URL that records every request sent to it."),
            |ui| {
                ui.add_enabled_ui(!tab.starting, |ui| {
                    *do_start |= kit::button_ex(ui, kit::Kind::Primary, Some(icons::PLAY), tr("Start"), true).clicked();
                });
            },
        ),
    }
}

/// Requests (newest first) with a filter. Returns true when Clear was clicked.
fn list(ui: &mut Ui, tab: &mut WebhookTab, events: &[WebhookEvent]) -> bool {
    let mut clear = false;
    sv::pane_title(ui, tr("Requests"), Some(&events.len().to_string()), |ui| {
        clear = kit::ghost(ui, Some(icons::TRASH), tr("Clear")).clicked();
    });
    sv::row_ex(
        ui,
        kit::size::TOOLBAR,
        (12.0, 12.0),
        8.0,
        false,
        |ui| {
            let r = kit::search_field_sm(
                ui,
                "wh-filter",
                &mut tab.filter,
                tr("Filter requests"),
                220.0_f32.min(ui.available_width()),
            );
            widgets::a11y_name(&r, tr("Filter requests"));
        },
        |ui| {
            ui.label(kit::hint(ui, tr("Newest first")));
        },
    );
    sv::list_head(ui, &COLS, &[tr("Time"), tr("Method"), tr("Path"), tr("Size")], &[3], None);
    let q = tab.filter.trim().to_lowercase();
    let shown: Vec<&WebhookEvent> = events
        .iter()
        .rev()
        .filter(|e| {
            q.is_empty()
                || e.path.to_lowercase().contains(&q)
                || e.query.to_lowercase().contains(&q)
                || e.method.eq_ignore_ascii_case(&q)
        })
        .collect();
    if shown.is_empty() {
        kit::empty_state(ui, icons::FUNNEL, tr("No requests match"), "", |_| {});
        return clear;
    }
    sv::list_scroll(ui, "wh-events", shown.len(), false, |ui, range| {
        for e in &shown[range] {
            let time = sv::clock_ms(e.timestamp_ms);
            let size = ternarywire_core::util::human_bytes(e.body.len() as u64);
            let cells = [
                Cell::Ts(&time),
                Cell::Method(&e.method),
                Cell::Path { host: "", path: &e.path, query: &e.query, pill: None },
                Cell::Ts(&size),
            ];
            let name = format!("{} {}", e.method, e.path);
            let r = sv::list_row(ui, &COLS, &cells, &[3], tab.selected == Some(e.id), &name, None);
            let r = r.on_hover_ui(|ui| {
                ui.label(sv::event_type(&e.body).unwrap_or_else(|| format!("{} {}", e.method, e.path)));
            });
            if r.clicked() {
                tab.selected = Some(e.id);
                tab.replay = None;
            }
        }
    });
    clear
}

/// A recorded request: head, metrics, headers, body, and the Replay card
/// fixed at the bottom.
fn detail(ui: &mut Ui, tab: &mut WebhookTab, e: &WebhookEvent, base: &str, acts: &mut Acts) {
    let p = theme::pal(ui);
    let has_result = tab.replay.is_some();
    let foot_h = 10.0 + 18.0 + 8.0 + 26.0 + if has_result { 8.0 + 22.0 } else { 0.0 } + 12.0;
    let full = ui.available_rect_before_wrap();
    let split = (full.bottom() - foot_h).max(full.top() + 80.0);
    let top = Rect::from_min_max(full.min, pos2(full.right(), split));
    let foot = Rect::from_min_max(pos2(full.left(), split), full.max);
    ui.new_child(UiBuilder::new().id_salt("wh-detail-top").max_rect(top).layout(Layout::top_down(Align::Min))).scope(
        |ui| {
            ui.set_clip_rect(top.intersect(ui.clip_rect()));
            let kind = sv::event_type(&e.body);
            sv::detail_head(
                ui,
                |ui| {
                    sv::method_tag(ui, &e.method);
                    sv::path_label(ui, "", &e.path, &e.query);
                },
                |ui| {
                    acts.close |= kit::icon_button_sm(ui, icons::X, tr("Close (Esc)")).clicked();
                    if let Some(k) = &kind {
                        kit::pill_ex(ui, k, Tone::Neutral, kit::PillOpts { outline: true, ..Default::default() });
                    }
                },
            );
            sv::detail_meta(
                ui,
                |ui| {
                    sv::metrics(ui, |ui| {
                        sv::metric(ui, Some(icons::CLOCK), &sv::clock_ms(e.timestamp_ms), Some(tr("Received")));
                        if let Some(a) = &e.remote_addr {
                            sv::metric(ui, Some(icons::DESKTOP), &trf("from {}", &[a]), None);
                        }
                        let size = ternarywire_core::util::human_bytes(e.body.len() as u64);
                        sv::metric(ui, Some(icons::ARROW_LINE_DOWN), &size, Some(tr("Body size")));
                    });
                },
                |ui| {
                    if kit::ghost(ui, Some(icons::TERMINAL), tr("Copy as cURL")).clicked() {
                        let q = if e.query.is_empty() { String::new() } else { format!("?{}", e.query) };
                        acts.copy =
                            Some(sv::curl(&e.method, &format!("{base}{}{q}", e.path), &e.headers, &e.body_text()));
                    }
                },
            );
            sv::detail_scroll(ui, ("wh-detail", e.id), |ui| {
                let rows: Vec<(&str, &str)> = e.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                sv::disclosure(
                    ui,
                    ui.make_persistent_id("wh-headers-open"),
                    tr("Headers"),
                    Some(rows.len()),
                    true,
                    |ui| {
                        if rows.is_empty() {
                            sv::no_body(ui, tr("No headers"));
                        } else {
                            sv::headers_table(ui, ui.make_persistent_id(("wh-headers", e.id)), &rows);
                        }
                    },
                );
                let mut copy = false;
                sv::section(
                    ui,
                    tr("Body"),
                    None,
                    |ui| {
                        if !e.body.is_empty() {
                            copy = kit::icon_button_sm(ui, icons::COPY, tr("Copy body")).clicked();
                        }
                        if e.body_truncated {
                            kit::tone_pill(ui, tr("truncated"), Tone::Warn);
                        }
                    },
                    |ui| {
                        if e.body.is_empty() {
                            sv::no_body(ui, tr("No body"));
                        } else {
                            sv::body_box(
                                ui,
                                ui.make_persistent_id(("wh-body", e.id)),
                                &crate::proto_common::payload_text(&e.body),
                            );
                        }
                    },
                );
                if copy {
                    acts.copy = Some(e.body_text().into_owned());
                }
            });
        },
    );
    // Replay, always in view under the scrolling detail.
    ui.new_child(UiBuilder::new().id_salt("wh-replay").max_rect(foot).layout(Layout::top_down(Align::Min))).scope(
        |ui| {
            ui.painter().rect_filled(foot, 0.0, p.well);
            ui.painter().hline(foot.x_range(), foot.top() + 0.5, egui::Stroke::new(1.0, p.border));
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 12 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                sv::hrow(ui, 18.0, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(RichText::new(icons::ARROW_CLOCKWISE).font(theme::icon_font(12.5)).color(p.text_muted));
                    // A space of the 12.5 px title apart, like the prototype's text.
                    ui.add_space(-4.5);
                    ui.label(kit::section_title(tr("Replay")).color(p.text));
                    ui.add(
                        egui::Label::new(kit::hint(
                            ui,
                            tr("Send this request again to another URL, e.g. your local app."),
                        ))
                        .truncate(),
                    );
                });
                sv::hrow(ui, 26.0, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let busy = tab.replaying;
                        let r = ui.add_enabled_ui(!busy, |ui| {
                            kit::button_ex(ui, kit::Kind::Secondary, Some(icons::ARROW_CLOCKWISE), tr("Replay"), true)
                        });
                        if busy {
                            sv::spinner_icon(
                                ui,
                                pos2(r.response.rect.left() + 15.0, r.response.rect.center().y),
                                13.0,
                                p.info,
                            );
                        }
                        acts.replay |= r.inner.clicked();
                        let w = ui.available_width();
                        let r = sv::input(
                            ui,
                            ui.make_persistent_id("wh-replay-target"),
                            &mut tab.replay_target,
                            "http://localhost:3000",
                            w,
                            true,
                        );
                        widgets::a11y_name(&r, tr("Replay"));
                    });
                });
                match &tab.replay {
                    Some(Ok(r)) => {
                        sv::hrow(ui, 22.0, |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let text = format!("{} {}", r.status, sv::reason(r.status));
                            kit::tone_pill(ui, text.trim_end(), sv::code_tone(r.status));
                            sv::metrics(ui, |ui| {
                                sv::metric(
                                    ui,
                                    Some(icons::TIMER),
                                    &ternarywire_core::util::human_ms(r.elapsed_ms),
                                    None,
                                );
                                sv::metric(
                                    ui,
                                    Some(icons::ARROW_LINE_DOWN),
                                    &ternarywire_core::util::human_bytes(r.body.len() as u64),
                                    None,
                                );
                            });
                            one_line_body(ui, &r.body);
                        });
                    }
                    Some(Err(err)) => {
                        sv::hrow(ui, 22.0, |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(RichText::new(icons::WARNING_CIRCLE).font(theme::icon_font(14.0)).color(p.err));
                            ui.add(egui::Label::new(RichText::new(err).size(12.0).color(p.err)).truncate());
                        });
                    }
                    None => {}
                }
            });
        },
    );
    ui.advance_cursor_after_rect(full);
}

/// JSON on one line with a space after `:` and `,` (`{"received": true}`).
fn one_line_json(v: &serde_json::Value) -> String {
    struct Spaced;
    impl serde_json::ser::Formatter for Spaced {
        fn begin_array_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W, first: bool) -> std::io::Result<()> {
            if first { Ok(()) } else { w.write_all(b", ") }
        }
        fn begin_object_key<W: ?Sized + std::io::Write>(&mut self, w: &mut W, first: bool) -> std::io::Result<()> {
            if first { Ok(()) } else { w.write_all(b", ") }
        }
        fn begin_object_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
            w.write_all(b": ")
        }
    }
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, Spaced);
    match serde::Serialize::serialize(v, &mut ser) {
        Ok(()) => String::from_utf8(out).unwrap_or_default(),
        Err(_) => v.to_string(),
    }
}

/// A response body on one line (JSON compacted and highlighted), elided.
fn one_line_body(ui: &mut Ui, body: &[u8]) {
    let text = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(v) => one_line_json(&v),
        Err(_) => String::from_utf8_lossy(body).replace(['\n', '\r'], " "),
    };
    let dark = ui.visuals().dark_mode;
    let mut job = crate::highlight::cached(
        ui.ctx(),
        crate::highlight::HlKey {
            text: &text,
            lang: crate::highlight::Lang::Json,
            dark,
            search: "",
            vars: None,
            font_size: 12.0,
            wrap: false,
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width().max(10.0));
    ui.add(egui::Label::new(job).selectable(false)).on_hover_text(text);
}

/// Starts or stops the listener as asked this frame.
fn run(app: &mut TernaryWireApp, tab_id: u64, tab: &mut WebhookTab, start_now: bool, stop_now: bool) {
    if stop_now {
        tab.stop(&app.rt);
    } else if start_now && tab.listener.is_none() && !tab.starting {
        start(app, tab_id, tab);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_line_json_spaces_separators() {
        let v = serde_json::json!({"received": true, "ids": [1, 2]});
        assert_eq!(super::one_line_json(&v), r#"{"received": true, "ids": [1, 2]}"#);
    }
}
