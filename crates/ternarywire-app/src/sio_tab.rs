//! Socket.IO request tab (FR-X4): namespaces, event listeners, emitting
//! events with JSON, text and binary arguments and acknowledgements.

use std::time::Instant;

use egui::{RichText, Ui};

use ternarywire_core::model::RequestKind;
use ternarywire_core::protocols::{SocketIoArg, SocketIoArgKind, SocketIoListener, SocketIoMessage};
use ternarywire_socketio::{
    ArgError, Direction, EmitArg, SharedCookies, SocketIoConfig, SocketIoEvent, SocketIoSession, SocketIoTransport,
    SocketIoVersion,
};

use crate::app::{Msg, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tab;
use crate::proto_common::{self as pc, Conn, ConnAction, Dir, ItemSlot, Log, RowAction, ScriptState, header, resolver};
use crate::widgets::{self, KvOptions, VarCtx};
use crate::{icons, kit, theme};
use ternarywire_core::script::ScriptEvent;
use ternarywire_core::script::protocol as proto;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Emit,
    Listeners,
    Saved,
    Headers,
    Settings,
    Scripts,
}

pub struct SioTab {
    /// Number of the current connection (see `app::next_epoch`).
    pub epoch: u64,
    pub slot: ItemSlot,
    session: Option<SocketIoSession>,
    connected: bool,
    section: Section,
    event: String,
    args: Vec<SocketIoArg>,
    ack: bool,
    only_listened: bool,
    /// Reply per pending server ack id: a JSON argument list, or one text or
    /// binary argument.
    ack_replies: std::collections::HashMap<u64, SocketIoArg>,
    log: Log,
    /// Automation: `connect` or `emit`.
    pub auto: Option<String>,
    pub scripts: ScriptState,
    inbox: std::collections::VecDeque<serde_json::Value>,
    since: Option<Instant>,
    /// Session id of the connection.
    sid: Option<String>,
    /// Why the last connection failed.
    error: Option<String>,
}

/// An event to emit: name, arguments and whether to ask for an ack.
type Emit = (String, Vec<SocketIoArg>, bool);

impl SioTab {
    pub fn new(slot: ItemSlot) -> Self {
        SioTab {
            epoch: 0,
            slot,
            session: None,
            connected: false,
            section: Section::Emit,
            event: "message".into(),
            args: vec![SocketIoArg::new(SocketIoArgKind::Json, "\"hello\"")],
            ack: false,
            only_listened: false,
            ack_replies: Default::default(),
            log: {
                let mut log = Log::with_empty(
                    "No events yet",
                    "Connect, then emit an event; events from the server appear here.",
                );
                log.title = "Events";
                log
            },
            auto: None,
            scripts: ScriptState::default(),
            inbox: Default::default(),
            since: None,
            sid: None,
            error: None,
        }
    }

    fn state(&self) -> Conn {
        if self.connected {
            Conn::Live
        } else if self.session.is_some() || self.scripts.is_waiting() {
            Conn::Busy
        } else if self.error.is_some() {
            Conn::Failed
        } else {
            Conn::Idle
        }
    }

    pub fn on_event(&mut self, e: SocketIoEvent, listened: &[String]) {
        match e {
            SocketIoEvent::Connecting => {
                self.error = None;
                self.log.system(tr("Connecting…"))
            }
            SocketIoEvent::Connected { sid, namespace } => {
                self.connected = true;
                self.since = Some(Instant::now());
                self.error = None;
                self.log.system(trf("Connected to {} (sid {})", &[&namespace, &sid]));
                self.sid = Some(sid);
            }
            SocketIoEvent::Event { name, args, direction, at_ms } => {
                if direction == Direction::Incoming && self.only_listened && !listened.iter().any(|l| l == &name) {
                    return;
                }
                let dir = if direction == Direction::Incoming { Dir::In } else { Dir::Out };
                if direction == Direction::Incoming {
                    let data = args.first().cloned().unwrap_or(serde_json::Value::Null);
                    let mut m = proto::message(data, proto::Direction::Incoming, at_ms);
                    m["event"] = serde_json::json!(name);
                    m["args"] = serde_json::json!(args);
                    crate::proto_common::queue_message(&mut self.inbox, m);
                }
                self.log.push(dir, name, serde_json::to_string(&args).unwrap_or_default(), at_ms);
            }
            SocketIoEvent::Ack { id, args, at_ms } => {
                self.log.push(Dir::In, trf("ack #{}", &[&id]), serde_json::to_string(&args).unwrap_or_default(), at_ms)
            }
            SocketIoEvent::Error(m) => {
                if !self.connected {
                    self.error = Some(m.clone());
                }
                self.log.error(m)
            }
            SocketIoEvent::Disconnected { reason } => {
                if !self.connected && self.error.is_none() && !reason.is_empty() {
                    self.error = Some(reason.clone());
                }
                self.connected = false;
                self.session = None;
                self.since = None;
                self.sid = None;
                self.log.system(trf("Disconnected: {}", &[&reason]));
            }
        }
    }
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut SioTab) {
    let Some(original) = tab.slot.load(app) else {
        ui.label(tr("This request no longer exists."));
        return;
    };
    let mut item = original.clone();
    // The rows of the view are edge to edge.
    ui.spacing_mut().item_spacing.y = 0.0;
    let collection = tab.slot.collection_id();
    let vars = app.var_ctx(collection.as_deref());
    let state = tab.state();
    let since = tab.since;
    let transport = tab.session.as_ref().and_then(|s| s.transport());
    let inherited = pc::inherited_auth(app, &tab.slot, &item);
    let root = app.ws.root().to_path_buf();
    header(ui, app, tab_id, &tab.slot, &mut item, |ui| {
        if let Some(t) = transport {
            kit::pill_ex(ui, t.label(), kit::Tone::Neutral, kit::PillOpts { outline: true, ..Default::default() })
                .on_hover_text(tr("Transport in use"));
        }
    });
    let RequestKind::SocketIo(req) = &mut item.request else { return };
    let mut action = None;
    let connected = state == Conn::Live;
    let can_emit = connected && !tab.event.trim().is_empty();
    let mut emit_now: Option<Emit> = None;
    if can_emit && pc::send_shortcut(ui) {
        emit_now = Some((tab.event.clone(), tab.args.clone(), tab.ack));
    }
    match tab.auto.as_deref() {
        Some("connect") => {
            if tab.session.is_none() {
                action = Some(ConnAction::Connect);
            }
            tab.auto = None;
        }
        Some("emit") if tab.connected => {
            emit_now = Some((tab.event.clone(), tab.args.clone(), tab.ack));
            tab.auto = None;
        }
        Some(a) if a.starts_with("select:") || a.starts_with("section:") => {
            if let Some(i) = a.strip_prefix("select:").and_then(|i| i.parse().ok()) {
                tab.log.selected = Some(i);
            } else if let Some(s) = a.strip_prefix("section:") {
                tab.section = match s {
                    "events" => Section::Listeners,
                    "saved" => Section::Saved,
                    "headers" => Section::Headers,
                    "settings" => Section::Settings,
                    "scripts" => Section::Scripts,
                    _ => Section::Emit,
                };
            }
            tab.auto = None;
        }
        _ => {}
    }
    // Server events waiting for our acknowledgement.
    let pending = tab.session.as_ref().map(|s| s.pending_acks()).unwrap_or_default();
    // Replies of acknowledgements that were sent (or whose connection closed).
    tab.ack_replies.retain(|id, _| pending.iter().any(|a| a.id == *id));
    let mut send_ack = None;
    let mut retry = false;
    let panes = ui.make_persistent_id("sio-panes");
    let item_id = item.id.clone();
    let scripts = &mut item.scripts;
    pc::view_panes(
        ui,
        panes,
        |ui| {
            let clicked = pc::connect_bar(
                ui,
                "SIO",
                124.0,
                |ui| {
                    let w = (ui.available_width() - 121.0).max(120.0);
                    let r = pc::bar_var(ui, "sio-url", &mut req.url, &vars, "http://localhost:3000", w);
                    pc::bar_divider(ui);
                    let ns = pc::bar_field(ui, "sio-ns", &mut req.namespace, tr("/namespace"), ui.available_width())
                        .on_hover_text(tr("Namespace"));
                    let enter = (r.lost_focus() || ns.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter && state != Conn::Live {
                        action = Some(ConnAction::Connect);
                    }
                    r.has_focus() || ns.has_focus()
                },
                |ui| pc::connect_button(ui, state),
            );
            if clicked.is_some() {
                action = clicked;
            }
            let listeners = req.listeners.iter().filter(|l| l.enabled).count();
            let headers =
                req.headers.iter().filter(|h| h.enabled).count() + req.params.iter().filter(|h| h.enabled).count();
            let tabs = [
                Tab::new(Section::Emit, tr("Emit")),
                Tab::new(Section::Listeners, tr("Events")).count(listeners),
                Tab::new(Section::Saved, tr("Saved")).count(req.messages.len()),
                Tab::new(Section::Headers, tr("Headers")).count(headers),
                Tab::new(Section::Settings, tr("Settings")),
                Tab::new(Section::Scripts, tr("Scripts")).count(pc::scripts_count(scripts)),
            ];
            kit::tabs(ui, &mut tab.section, &tabs);
            let event = &mut tab.event;
            let args = &mut tab.args;
            let ack = &mut tab.ack;
            match tab.section {
                Section::Emit => {
                    let p = theme::pal(ui);
                    let ack_now = *ack;
                    let event_now = event.clone();
                    pc::toolbar(
                        ui,
                        |ui| {
                            ui.label(RichText::new(tr("Event")).color(p.text_muted));
                            let w = (ui.available_width() - 130.0).clamp(90.0, 150.0);
                            pc::input_sm(ui, event, "message", w, true);
                            pc::check_label(ui, ack, tr("Expect ack"))
                                .on_hover_text(tr("Ask the server to acknowledge; its answer appears in the log"));
                        },
                        |ui| {
                            let hint = if connected { tr("Enter an event name first") } else { tr("Connect first") };
                            let ok = connected && !event_now.trim().is_empty();
                            if pc::toolbar_send(ui, tr("Emit"), ok, hint) {
                                emit_now = Some((event_now.clone(), args.clone(), ack_now));
                            }
                            if pc::save_message_button(ui, !event_now.trim().is_empty()) {
                                req.messages.push(SocketIoMessage {
                                    name: event_now.clone(),
                                    event: event_now.clone(),
                                    arguments: args.clone(),
                                    ack: ack_now,
                                    ..Default::default()
                                });
                            }
                        },
                    );
                    pc::body(ui, "sio-emit", 0.0, 10.0, |ui| args_editor(ui, args, &vars, &root));
                }
                Section::Listeners => {
                    let only = &mut tab.only_listened;
                    pc::toolbar(
                        ui,
                        |ui| pc::toolbar_title(ui, tr("Event listeners"), None),
                        |ui| {
                            pc::check_label(ui, only, tr("Only show incoming events listed here"));
                        },
                    );
                    pc::body(ui, "sio-listeners", 0.0, 10.0, |ui| {
                        ui.label(kit::hint(
                            ui,
                            tr("Incoming events with these names are expected; the log can show only them."),
                        ));
                        let mut remove = None;
                        if !req.listeners.is_empty() {
                            let n = req.listeners.len();
                            pc::list_box(ui, n, 40.0, |ui, i| {
                                let l = &mut req.listeners[i];
                                kit::check(ui, &mut l.enabled, tr("Listen to this event"))
                                    .on_hover_text(tr("Listen to this event"));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(8.0);
                                    if kit::icon_button_sm(ui, icons::TRASH, tr("Remove event")).clicked() {
                                        remove = Some(i);
                                    }
                                    pc::row_input(ui, &mut l.event, tr("event name"), ui.available_width(), true);
                                });
                            });
                        }
                        if let Some(i) = remove {
                            req.listeners.remove(i);
                        }
                        if kit::ghost(ui, Some(icons::PLUS), tr("Add event")).clicked() {
                            req.listeners.push(SocketIoListener { event: "message".into(), enabled: true });
                        }
                    });
                }
                Section::Saved => {
                    pc::toolbar(
                        ui,
                        |ui| pc::toolbar_title(ui, tr("Saved messages"), Some(req.messages.len())),
                        |ui| {
                            ui.label(kit::hint(ui, tr("Stored with the request")));
                        },
                    );
                    pc::body(ui, "sio-saved", 0.0, 14.0, |ui| {
                        if req.messages.is_empty() {
                            ui.label(kit::hint(
                                ui,
                                tr("Save events from the Emit section to reuse them; they are stored with the request."),
                            ));
                        } else {
                            let p = theme::pal(ui);
                            let acted = pc::saved_list(
                                ui,
                                "sio-saved-list",
                                &mut req.messages,
                                connected,
                                tr("Emit now"),
                                |ui, m| {
                                    pc::name_field(ui, &mut m.name, 120.0);
                                    pc::mono_tag(ui, &m.event, 140.0);
                                    if m.ack {
                                        ui.label(RichText::new(tr("ack")).size(11.0).color(p.text_faint));
                                    }
                                    pc::row_summary(ui, &args_summary(&m.argument_list()));
                                },
                            );
                            match acted {
                                Some((i, RowAction::Use)) => {
                                    let m = req.messages[i].clone();
                                    *args = m.argument_list();
                                    *event = m.event;
                                    *ack = m.ack;
                                    tab.section = Section::Emit;
                                }
                                Some((i, RowAction::Send)) => {
                                    let m = &req.messages[i];
                                    emit_now = Some((m.event.clone(), m.argument_list(), m.ack));
                                }
                                Some((i, RowAction::Remove)) => {
                                    req.messages.remove(i);
                                }
                                None => {}
                            }
                        }
                        crate::ws_tab::runner_section(ui, "sio", &mut req.runner, tr("event name"));
                    });
                }
                Section::Headers => {
                    pc::body(ui, "sio-headers", 0.0, 0.0, |ui| {
                        let mut h = req.headers.clone();
                        let opts = KvOptions {
                            title: Some(tr("Headers")),
                            allow_add: true,
                            key_suggestions: Some(widgets::COMMON_HEADERS),
                            key_hint: tr("Key"),
                            value_hint: tr("Value"),
                            ..Default::default()
                        };
                        if widgets::kv_table(ui, "sio-headers", &mut h, &vars, opts) {
                            req.headers = h;
                        }
                        ui.add_space(6.0);
                        let mut p = req.params.clone();
                        let opts = KvOptions {
                            title: Some(tr("Query parameters")),
                            allow_add: true,
                            key_hint: tr("Key"),
                            value_hint: tr("Value"),
                            ..Default::default()
                        };
                        if widgets::kv_table(ui, "sio-params", &mut p, &vars, opts) {
                            req.params = p;
                        }
                    });
                }
                Section::Scripts => pc::scripts_ui(
                    ui,
                    scripts,
                    &[(ScriptEvent::PreRequest, "Before connect"), (ScriptEvent::OnMessage, "On message")],
                    &mut tab.scripts,
                    &vars,
                ),
                Section::Settings => {
                    pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("Connection"), None), |_| {});
                    pc::body(ui, "sio-settings", 0.0, 14.0, |ui| {
                        kit::group(ui, "", "", |ui, rows| {
                            rows.row(
                                ui,
                                tr("Transport"),
                                tr("Auto: long-polling first, then upgrade to WebSocket, like the official client"),
                                |ui| {
                                    ui.push_id("transport", |ui| {
                                        kit::segmented_ex(
                                            ui,
                                            &mut req.transport,
                                            &[
                                                (SocketIoTransport::Auto, None, tr("Auto")),
                                                (SocketIoTransport::WebSocket, None, "WebSocket"),
                                                (SocketIoTransport::Polling, None, tr("Polling")),
                                            ],
                                            true,
                                        )
                                    });
                                },
                            );
                            rows.row(ui, tr("Server version"), "", |ui| {
                                ui.push_id("version", |ui| {
                                    kit::segmented_ex(
                                        ui,
                                        &mut req.version,
                                        &[(SocketIoVersion::V4, None, "v3 / v4"), (SocketIoVersion::V2, None, "v2")],
                                        true,
                                    )
                                });
                            });
                            rows.row(ui, tr("Handshake path"), "", |ui| {
                                pc::input_sm(ui, &mut req.path, "/socket.io/", 150.0, true);
                            });
                            rows.row(ui, tr("Verify SSL certificates"), tr("For https:// servers"), |ui| {
                                let mut v = req.verify_ssl.unwrap_or(true);
                                if kit::check(ui, &mut v, tr("Verify SSL certificates")).changed() {
                                    req.verify_ssl = Some(v);
                                }
                            });
                        });
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.label(kit::section_title(tr("Handshake auth")));
                            ui.label(kit::hint(
                                ui,
                                tr("A JSON object sent with the connection (socket.handshake.auth)."),
                            ));
                            let margin = egui::Margin::ZERO;
                            let r = pc::editor_well(
                                ui,
                                "sio-auth",
                                &mut req.auth_payload,
                                Lang::Json,
                                Some(&vars),
                                margin,
                                Some(70.0),
                                None,
                            );
                            pc::well_placeholder(ui, &r, &req.auth_payload, "{ \"token\": \"…\" }");
                        });
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.label(kit::section_title(tr("Authorization")));
                            crate::auth_ui::auth_editor(
                                ui,
                                &mut req.auth,
                                crate::auth_ui::AuthUiCtx {
                                    is_root: false,
                                    inherited: inherited.clone(),
                                    owner_id: &item_id,
                                    token: None,
                                    fetching: false,
                                    vars: &vars,
                                },
                            );
                        });
                    });
                }
            }
        },
        |ui| {
            pc::status_row(
                ui,
                |ui| {
                    let text = pc::conn_text(ui, state, since);
                    pc::status_pill(ui, state, &text);
                    let (sent, received) = tab.log.counts();
                    pc::metrics(ui, |ui| {
                        if let Some(sid) = tab.sid.as_deref().filter(|_| connected) {
                            let short: String = sid.chars().take(6).collect();
                            kit::metric(ui, icons::FINGERPRINT, &format!("sid {short}"))
                                .on_hover_text(format!("{}: {sid}", tr("Session id")));
                        }
                        if connected || sent + received > 0 {
                            kit::metric(ui, icons::ARROW_UP, &trf("{} sent", &[&sent]));
                            kit::metric(ui, icons::ARROW_DOWN, &trf("{} received", &[&received]));
                        }
                    });
                },
                |ui| pc::pane_actions(ui, panes),
            );
            tab.log.bar(ui, ("sio-log", tab_id));
            if let (Some(e), Conn::Failed) = (&tab.error, state) {
                let (r, dismiss) = pc::connection_error(ui, tr("Could not connect"), e);
                retry = r;
                if dismiss {
                    tab.error = None;
                }
            }
            if !pending.is_empty() {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 0 }).show(
                    ui,
                    |ui| {
                        pc::notice_card_ex(
                            ui,
                            icons::HOURGLASS,
                            tr("The server is waiting for acknowledgements"),
                            |ui| {
                                kit::tone_pill(ui, &pending.len().to_string(), kit::Tone::Accent);
                            },
                            |ui| {
                                for a in &pending {
                                    ui.push_id(("sio-ack", a.id), |ui| {
                                        let reply = tab
                                            .ack_replies
                                            .entry(a.id)
                                            .or_insert_with(|| SocketIoArg::new(SocketIoArgKind::Json, "[]"));
                                        if ack_row(ui, a.id, &a.event, reply) {
                                            send_ack = Some(a.id);
                                        }
                                    });
                                }
                            },
                        );
                    },
                );
            }
            tab.log.body(ui, ("sio-log", tab_id));
        },
    );
    if retry {
        action = Some(ConnAction::Connect);
    }
    if let Some(id) = send_ack {
        answer_ack(app, tab, id, collection.as_deref());
    }
    if let Some(e) = emit_now {
        emit(app, tab, collection.as_deref(), e);
    }
    let req_clone = req.clone();
    let item_now = item.clone();
    tab.slot.store(app, &original, item);
    match action {
        Some(ConnAction::Connect) => {
            tab.scripts.clear_results();
            tab.error = None;
            if let Some(run) = crate::proto_common::run_scripts(
                app,
                tab_id,
                &tab.slot,
                &item_now,
                ScriptEvent::PreRequest,
                proto::socketio(&req_clone),
            ) {
                tab.scripts.wait_for(run);
            } else {
                start(app, tab_id, tab, &req_clone, collection.as_deref());
            }
        }
        Some(ConnAction::Cancel) | Some(ConnAction::Disconnect) => {
            if let Some(s) = &tab.session {
                s.disconnect();
            }
            tab.scripts.stop_waiting();
        }
        None => {}
    }
    if let Some(pre) = tab.scripts.take_pre() {
        match pre {
            Some(v) => {
                let mut r = req_clone.clone();
                proto::apply_socketio(&mut r, &v);
                start(app, tab_id, tab, &r, collection.as_deref());
            }
            None => tab.log.system(tr("Skipped by pm.execution.skipRequest()")),
        }
    }
    for m in std::mem::take(&mut tab.inbox) {
        if crate::proto_common::run_scripts(
            app,
            tab_id,
            &tab.slot,
            &item_now,
            ScriptEvent::OnMessage,
            proto::socketio(&req_clone).with_message(m),
        )
        .is_none()
        {
            break;
        }
    }
}

/// `req` with `Inherit` auth replaced by its folder's or collection's, as
/// for HTTP requests.
fn with_inherited_auth(
    app: &TernaryWireApp,
    collection: Option<&str>,
    item_id: &str,
    req: &ternarywire_core::protocols::SocketIoRequest,
) -> ternarywire_core::protocols::SocketIoRequest {
    let mut req = req.clone();
    if req.auth.is_inherit() {
        req.auth = collection
            .and_then(|c| app.ws.data.collection(c))
            .and_then(|c| c.effective_auth(item_id).map(|e| e.auth.clone()))
            .unwrap_or_else(ternarywire_core::auth::Auth::none);
    }
    req
}

fn start(
    app: &mut TernaryWireApp,
    tab_id: u64,
    tab: &mut SioTab,
    req_clone: &ternarywire_core::protocols::SocketIoRequest,
    collection: Option<&str>,
) {
    {
        let r = resolver(app, collection);
        let item_id = tab.slot.location.as_ref().map(|l| l.item.clone()).unwrap_or_default();
        let req = with_inherited_auth(app, collection, &item_id, req_clone);
        let mut cfg = SocketIoConfig::from_request(&req, &r, app.settings.request.verify_ssl);
        cfg.cookies = Some(SharedCookies::new(app.ws.cookies.clone()));
        cfg.proxy = app.proxy_settings();
        match app.tls_setup(&cfg.url, cfg.verify_ssl) {
            Ok(tls) => cfg.tls = tls,
            Err(e) => return app.toast(e, true),
        }
        let tx = app.tx.clone();
        let ctx = app.ctx.clone();
        tab.epoch = crate::app::next_epoch();
        let epoch = tab.epoch;
        tab.session = Some(SocketIoSession::connect(app.rt.handle(), cfg, move |event| {
            let _ = tx.send(Msg::SocketIo { tab: tab_id, epoch, event });
            ctx.request_repaint();
        }));
    }
}

const KINDS: [SocketIoArgKind; 4] =
    [SocketIoArgKind::Json, SocketIoArgKind::Text, SocketIoArgKind::Binary, SocketIoArgKind::File];

fn kind_label(kind: SocketIoArgKind) -> &'static str {
    match kind {
        SocketIoArgKind::Json => "JSON",
        SocketIoArgKind::Text => tr("Text"),
        SocketIoArgKind::Binary => tr("Binary"),
        SocketIoArgKind::File => tr("File"),
    }
}

/// The kind of an acknowledgement reply, as a small select box.
fn kind_combo(ui: &mut Ui, kind: &mut SocketIoArgKind) -> egui::Response {
    let r = kit::select_box(ui, None, kind_label(*kind), 84.0, true);
    widgets::a11y_name(&r, tr("Type"));
    egui::Popup::menu(&r).width(120.0).show(|ui| {
        for k in KINDS {
            let item = kit::MenuItem { checked: Some(*kind == k), label: kind_label(k), ..Default::default() };
            if kit::menu_item_ex(ui, item).clicked() {
                *kind = k;
            }
        }
    });
    r
}

fn choose_file(ui: &mut Ui, path: &mut String) {
    let opts = kit::ButtonOpts { icon: Some(icons::FILE_ARROW_UP), ..Default::default() };
    if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Choose file…"), opts).clicked()
        && let Some(p) = rfd::FileDialog::new().pick_file()
    {
        *path = p.display().to_string();
    }
}

/// One server acknowledgement waiting for a reply: its id and event, the
/// kind and text of the reply, and Send ack. Returns true when sent.
fn ack_row(ui: &mut Ui, id: u64, event: &str, reply: &mut SocketIoArg) -> bool {
    let mut send = false;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 26.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let opts = kit::ButtonOpts { icon: Some(icons::CHECK), ..Default::default() };
            if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Send ack"), opts).clicked() {
                send = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                pc::mono_tag(ui, &format!("#{id}"), 80.0);
                ui.label(RichText::new(event).font(theme::mono(12.0)).strong());
                let tip = match reply.kind {
                    SocketIoArgKind::Json => tr("Arguments as a JSON array, a single JSON value, or plain text."),
                    _ => tr("Sent as one argument"),
                };
                kind_combo(ui, &mut reply.kind).on_hover_text(tip);
                if reply.kind == SocketIoArgKind::File {
                    choose_file(ui, &mut reply.value);
                }
                pc::input_sm(ui, &mut reply.value, "[]", ui.available_width(), true);
            });
        },
    );
    send
}

/// What is wrong with an argument as typed; values with variables are
/// checked when sent, files when they are read.
fn arg_problem(a: &SocketIoArg) -> Option<&'static str> {
    if a.value.contains("{{") {
        return None;
    }
    match a.kind {
        SocketIoArgKind::Json if serde_json::from_str::<serde_json::Value>(&a.value).is_err() => {
            Some(tr("Invalid JSON"))
        }
        SocketIoArgKind::Binary if SocketIoArg::decode_binary(&a.value).is_none() => {
            Some(tr("Binary messages must be hex or base64"))
        }
        _ => None,
    }
}

/// The emit arguments: a numbered card each (`.argc`) with its kind.
fn args_editor(ui: &mut Ui, args: &mut Vec<SocketIoArg>, vars: &VarCtx, root: &std::path::Path) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        ui.set_height(24.0);
        ui.label(kit::section_title(tr("Arguments")));
        if !args.is_empty() {
            kit::tone_pill(ui, &args.len().to_string(), kit::Tone::Neutral);
        }
        ui.add(
            egui::Label::new(kit::hint(ui, tr("JSON, text or binary (an attachment, a Buffer on the server).")))
                .truncate(),
        );
    });
    let mut remove = None;
    for (i, a) in args.iter_mut().enumerate() {
        ui.push_id(("sio-arg", i), |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS_CARD))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // Header: number, kind, problem and Remove on the well fill.
                    let w = ui.available_width();
                    let (head, _) = ui.allocate_exact_size(egui::vec2(w, 36.0), egui::Sense::hover());
                    let r = egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
                    ui.painter().rect_filled(head, r, p.well);
                    ui.painter().hline(head.x_range(), head.bottom() - 0.5, egui::Stroke::new(1.0, p.border));
                    let inner =
                        egui::Rect::from_min_max(head.min + egui::vec2(10.0, 0.0), head.max - egui::vec2(8.0, 0.0));
                    pc::inside(
                        ui,
                        egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let (c, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                            ui.painter().circle_filled(c.center(), 10.0, p.subtle);
                            ui.painter().text(
                                c.center(),
                                egui::Align2::CENTER_CENTER,
                                (i + 1).to_string(),
                                theme::semibold(11.0),
                                p.text_muted,
                            );
                            let items: Vec<(SocketIoArgKind, Option<&str>, &str)> =
                                KINDS.iter().map(|k| (*k, None, kind_label(*k))).collect();
                            kit::segmented_ex(ui, &mut a.kind, &items, true);
                            if let Some(problem) = arg_problem(a) {
                                kit::pill_ex(
                                    ui,
                                    problem,
                                    kit::Tone::Err,
                                    kit::PillOpts { icon: Some(icons::WARNING_CIRCLE), ..Default::default() },
                                );
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if kit::icon_button_sm(ui, icons::TRASH, tr("Remove argument")).clicked() {
                                    remove = Some(i);
                                }
                            });
                        },
                    );
                    match a.kind {
                        SocketIoArgKind::Json | SocketIoArgKind::Text => {
                            let lang = if a.kind == SocketIoArgKind::Json { Lang::Json } else { Lang::Text };
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                pc::plain_editor(ui, "value", &mut a.value, lang, Some(vars), 2);
                            });
                        }
                        SocketIoArgKind::Binary => {
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                                let w = ui.available_width();
                                pc::var_input_sm(
                                    ui,
                                    "value",
                                    &mut a.value,
                                    vars,
                                    tr("hex or base64, e.g. 01 02 ff or AQL/"),
                                    w,
                                );
                            });
                        }
                        SocketIoArgKind::File => {
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    choose_file(ui, &mut a.value);
                                    let size = (!a.value.trim().is_empty() && !a.value.contains("{{"))
                                        .then(|| std::fs::metadata(root.join(a.value.trim())).ok())
                                        .flatten()
                                        .map(|m| ternarywire_core::util::human_bytes(m.len()));
                                    let size_w = if size.is_some() { 60.0 } else { 0.0 };
                                    let hint = tr("File path (absolute or relative to the workspace folder)");
                                    let w = (ui.available_width() - size_w).max(80.0);
                                    pc::var_input_sm(ui, "value", &mut a.value, vars, hint, w);
                                    if let Some(s) = size {
                                        ui.label(RichText::new(s).size(11.0).color(p.text_faint));
                                    }
                                });
                            });
                        }
                    }
                });
        });
    }
    if let Some(i) = remove {
        args.remove(i);
    }
    ui.horizontal(|ui| {
        if kit::ghost(ui, Some(icons::PLUS), tr("Add argument")).clicked() {
            args.push(SocketIoArg::default());
        }
        if args.is_empty() {
            ui.label(kit::hint(ui, tr("No arguments: the event is emitted without data.")));
        }
    });
}

/// One line per saved message: JSON as written, text quoted, binary marked.
fn args_summary(args: &[SocketIoArg]) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| match a.kind {
            SocketIoArgKind::Json => a.value.trim().replace('\n', " "),
            SocketIoArgKind::Text => serde_json::Value::String(a.value.clone()).to_string(),
            SocketIoArgKind::Binary => format!("{} {}", tr("(binary)"), a.value.trim()),
            SocketIoArgKind::File => {
                let path = a.value.trim();
                format!("{} {}", tr("(binary)"), path.rsplit(['/', '\\']).next().unwrap_or(path))
            }
        })
        .collect();
    parts.join(", ")
}

/// A translated message for an argument that cannot be sent.
fn arg_error_text(e: &ArgError) -> String {
    let n = e.index() + 1;
    match e {
        ArgError::InvalidJson { message, .. } => {
            trf("Argument {}: {}", &[&n, &format!("{} ({message})", tr("Invalid JSON"))])
        }
        ArgError::InvalidBinary { .. } => trf("Argument {}: {}", &[&n, &tr("Binary messages must be hex or base64")]),
        ArgError::File { path, message, .. } => trf("Argument {}: cannot read {}: {}", &[&n, path, message]),
    }
}

/// Emits an event; arguments that cannot be sent are reported.
fn emit(app: &mut TernaryWireApp, tab: &SioTab, collection: Option<&str>, (event, args, ack): Emit) {
    let Some(s) = &tab.session else { return };
    let r = resolver(app, collection);
    match ternarywire_socketio::resolve_args(&args, &r, Some(app.ws.root())) {
        Ok(args) => {
            s.emit_args(&event, args, ack);
        }
        Err(e) => app.toast(arg_error_text(&e), true),
    }
}

/// Answers the server's acknowledgement request `id` with its reply.
fn answer_ack(app: &mut TernaryWireApp, tab: &mut SioTab, id: u64, collection: Option<&str>) {
    let (Some(s), Some(reply)) = (&tab.session, tab.ack_replies.get(&id)) else { return };
    let r = resolver(app, collection);
    let value = r(&reply.value);
    let args = match reply.kind {
        SocketIoArgKind::Json => Ok(ternarywire_socketio::parse_args(&value).into_iter().map(EmitArg::Json).collect()),
        kind => ternarywire_socketio::resolve_arg(0, kind, &value, Some(app.ws.root())).map(|a| vec![a]),
    };
    match args {
        Ok(args) => {
            let shown: Vec<serde_json::Value> = args.iter().map(EmitArg::to_json).collect();
            if s.ack_args(id, args) {
                let text = serde_json::to_string(&shown).unwrap_or_default();
                tab.log.push(Dir::Out, trf("ack #{}", &[&id]), text, crate::proto_common::now_ms());
            }
        }
        Err(e) => app.toast(arg_error_text(&e), true),
    }
}

/// Event names the request listens to, when the tab shows only those
/// (looked up for every incoming event: without cloning the request).
pub fn listened(app: &TernaryWireApp, tab: &SioTab) -> Vec<String> {
    if !tab.only_listened {
        return Vec::new();
    }
    let item = match &tab.slot.location {
        Some(loc) => app.ws.data.collection(&loc.collection).and_then(|c| c.find_request(&loc.item)),
        None => tab.slot.scratch.as_ref(),
    };
    match item.map(|i| &i.request) {
        Some(RequestKind::SocketIo(r)) => r.listeners.iter().filter(|l| l.enabled).map(|l| l.event.clone()).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Incoming events look the listeners up only when the tab shows just
    /// those (it used to clone the request for every event).
    #[test]
    fn listeners_are_looked_up_only_to_filter() {
        use ternarywire_core::model::RequestItem;
        use ternarywire_core::protocols::{SocketIoListener, SocketIoRequest};
        let dir = tempfile::tempdir().unwrap();
        let app = TernaryWireApp::for_test(dir.path());
        let mut item = RequestItem::new("socket", ternarywire_core::model::HttpRequest::default());
        let listeners = vec![
            SocketIoListener { event: "a".into(), enabled: true },
            SocketIoListener { event: "b".into(), enabled: false },
        ];
        item.request = RequestKind::SocketIo(SocketIoRequest { listeners, ..Default::default() });
        let mut tab = SioTab::new(crate::proto_common::ItemSlot { location: None, scratch: Some(item) });
        assert!(listened(&app, &tab).is_empty());
        tab.only_listened = true;
        assert_eq!(listened(&app, &tab), ["a"]);
    }

    #[test]
    fn inherited_auth_reaches_the_handshake() {
        use ternarywire_core::model::{Item, RequestItem, RequestKind};
        use ternarywire_core::protocols::SocketIoRequest;
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let cid = app.new_collection("C");
        let req = SocketIoRequest { url: "http://sio.test".into(), ..Default::default() };
        assert!(req.auth.is_inherit());
        let mut item = RequestItem::new("socket", ternarywire_core::model::HttpRequest::default());
        item.request = RequestKind::SocketIo(req.clone());
        let item_id = item.id.clone();
        let c = app.ws.data.collection_mut(&cid).unwrap();
        c.auth = ternarywire_core::auth::Auth::bearer("{{token}}");
        c.variables.push(ternarywire_core::variables::Variable::new("token", "t0k"));
        c.items.push(Item::Request(item));
        let r = resolver(&app, Some(&cid));
        let cfg = SocketIoConfig::from_request(&with_inherited_auth(&app, Some(&cid), &item_id, &req), &r, true);
        assert!(cfg.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer t0k"), "{:?}", cfg.headers);
    }

    /// Connecting, a failure with its cause, and a new attempt.
    #[test]
    fn connection_states() {
        let mut tab = SioTab::new(ItemSlot::default());
        assert_eq!(tab.state(), Conn::Idle);
        tab.on_event(SocketIoEvent::Error("connection refused".into()), &[]);
        assert_eq!(tab.state(), Conn::Failed);
        tab.on_event(SocketIoEvent::Connected { sid: "s1".into(), namespace: "/".into() }, &[]);
        assert_eq!(tab.state(), Conn::Live);
        assert!(tab.error.is_none());
    }
}
