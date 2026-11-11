//! WebSocket request tab (FR-X3): connect with headers, send text/binary,
//! timestamped message log, saved messages.

use std::collections::VecDeque;
use std::time::Instant;

use base64::Engine;
use egui::{RichText, Ui};

use ternarywire_core::model::{RequestItem, RequestKind, WsMessageKind, WsSavedMessage};
use ternarywire_core::prepared::{PrepareContext, prepare};
use ternarywire_core::protocols::SessionRunner;
use ternarywire_core::variables::Resolver;
use ternarywire_http::ws::{WsConfig, WsDirection, WsEvent, WsPayload, WsSession};

use crate::app::{ItemRef, Msg, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tab;
use crate::proto_common::{self as pc, Conn, ConnAction, Dir, Log, LogLine, RowAction};
use crate::widgets::{self, KvOptions};
use crate::{icons, kit, theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Message,
    Params,
    Headers,
    Auth,
    Saved,
    Scripts,
}

#[derive(Clone, PartialEq)]
enum State {
    Disconnected,
    Connecting,
    Connected { protocol: Option<String>, status: u16 },
}

pub struct WsTab {
    /// Number of the current connection (see `app::next_epoch`).
    pub epoch: u64,
    pub location: Option<ItemRef>,
    pub scratch: Option<RequestItem>,
    session: Option<WsSession>,
    conn: State,
    log: Log,
    composer: String,
    binary: bool,
    section: Section,
    pub scripts: crate::proto_common::ScriptState,
    inbox: VecDeque<serde_json::Value>,
    /// When the connection opened, and how long the handshake took.
    since: Option<Instant>,
    handshake_ms: Option<f64>,
    /// Why the last connection failed (cleared when connecting again).
    error: Option<String>,
    /// Automation: `connect`, `send`, `select:<row>`, `section:<name>`, `composer:<text>`.
    pub auto: Option<String>,
}

impl WsTab {
    fn base(location: Option<ItemRef>, scratch: Option<RequestItem>) -> Self {
        WsTab {
            epoch: 0,
            location,
            scratch,
            session: None,
            conn: State::Disconnected,
            log: Log::with_empty("No messages yet", "Connect, then send a message to see the conversation here."),
            composer: String::new(),
            binary: false,
            section: Section::Message,
            scripts: Default::default(),
            inbox: VecDeque::new(),
            since: None,
            handshake_ms: None,
            error: None,
            auto: None,
        }
    }

    fn slot(&self) -> crate::proto_common::ItemSlot {
        crate::proto_common::ItemSlot { location: self.location.clone(), scratch: None }
    }

    pub fn scratch(item: RequestItem) -> Self {
        Self::base(None, Some(item))
    }

    pub fn saved(r: ItemRef) -> Self {
        Self::base(Some(r), None)
    }

    pub fn on_event(&mut self, e: WsEvent) {
        match e {
            WsEvent::Connecting => {
                self.conn = State::Connecting;
                self.error = None;
            }
            WsEvent::Connected { status, protocol, elapsed_ms, .. } => {
                self.conn = State::Connected { protocol: protocol.clone(), status };
                self.since = Some(Instant::now());
                self.handshake_ms = Some(elapsed_ms);
                self.error = None;
                let protocol = protocol.map(|p| trf(", protocol {}", &[&p])).unwrap_or_default();
                let took = ternarywire_core::util::human_ms(elapsed_ms);
                self.log.system(trf("Connected ({}) in {}{}", &[&status, &took, &protocol]));
            }
            WsEvent::Message { direction, payload, at_ms } => {
                let (text, binary) = match &payload {
                    WsPayload::Text(t) => (t.clone(), false),
                    WsPayload::Binary(_) => (payload.display(), true),
                };
                if direction == WsDirection::Incoming {
                    use ternarywire_core::script::protocol as proto;
                    let mut m = proto::message(proto::json_or_text(&text), proto::Direction::Incoming, at_ms);
                    m["binary"] = serde_json::json!(binary);
                    crate::proto_common::queue_message(&mut self.inbox, m);
                }
                let dir = if direction == WsDirection::Incoming { Dir::In } else { Dir::Out };
                let size = payload.len();
                self.log.push_line(LogLine {
                    dir,
                    title: String::new(),
                    text,
                    at_ms,
                    size,
                    error: false,
                    binary,
                    meta: String::new(),
                });
            }
            WsEvent::Error(msg) => {
                if !matches!(self.conn, State::Connected { .. }) {
                    self.error = Some(msg.clone());
                }
                self.log.error(trf("Error: {}", &[&msg]))
            }
            WsEvent::Closed { code, reason, by_peer } => {
                self.conn = State::Disconnected;
                self.session = None;
                self.since = None;
                let mut text = if by_peer { tr("Disconnected by the server") } else { tr("Disconnected") }.to_string();
                if let Some(c) = code {
                    text.push_str(&trf(" (code {})", &[&c]));
                }
                if !reason.is_empty() {
                    text.push_str(&format!(": {reason}"));
                }
                self.log.system(text);
            }
        }
    }

    fn state(&self) -> Conn {
        match self.conn {
            State::Connected { .. } => Conn::Live,
            // Waiting for the pre-request script counts as connecting.
            State::Connecting => Conn::Busy,
            State::Disconnected if self.scripts.is_waiting() => Conn::Busy,
            State::Disconnected if self.error.is_some() => Conn::Failed,
            State::Disconnected => Conn::Idle,
        }
    }
}

fn get_item(app: &TernaryWireApp, tab: &WsTab) -> Option<RequestItem> {
    match &tab.location {
        Some(loc) => app.ws.data.collection(&loc.collection)?.find_request(&loc.item).cloned(),
        None => tab.scratch.clone(),
    }
}

/// The "Runner" settings of a WebSocket, Socket.IO or MQTT request: when the
/// Collection Runner and `ternarywire run` end the session.
pub(crate) fn runner_section(ui: &mut Ui, id: &str, runner: &mut SessionRunner, until_hint: &str) {
    let customized = !runner.is_default();
    let open = pc::disclosure(ui, ui.make_persistent_id((id, "runner")), tr("Runner"), None, true, |ui| {
        if customized {
            kit::tone_pill(ui, tr("customized"), kit::Tone::Accent);
        }
    });
    if !open {
        return;
    }
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(kit::hint(
            ui,
            tr("Collection Runner and ternarywire run: connect, send the saved messages in order, then stop at the first of:"),
        ));
        kit::group(ui, "", "", |ui, rows| {
            rows.block(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut limit = runner.max_messages.is_some();
                    if kit::checkbox(ui, &mut limit, tr("Incoming messages")).changed() {
                        runner.max_messages = limit.then_some(1);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &mut runner.max_messages {
                        Some(n) => {
                            pc::number_input(ui, (id, "runner-max"), n, 90.0, tr("Incoming messages"));
                        }
                        None => {
                            ui.label(RichText::new(tr("no limit")).size(12.0).color(theme::pal(ui).text_faint));
                        }
                    });
                });
            });
            rows.row(ui, tr("Message that ends the session"), until_hint, |ui| {
                let mut until = runner.until_event.clone().unwrap_or_default();
                if pc::input_sm(ui, &mut until, tr("no limit"), 160.0, true).changed() {
                    runner.until_event = (!until.trim().is_empty()).then_some(until);
                }
            });
            rows.row(ui, tr("Silence after the last message"), tr("ms, 0 = off"), |ui| {
                pc::number_input(ui, (id, "runner-wait"), &mut runner.wait_ms, 90.0, tr("Silence after the last message"));
            });
            rows.row(ui, tr("Session timeout"), tr("ms, 0 = none"), |ui| {
                pc::number_input(ui, (id, "runner-timeout"), &mut runner.timeout_ms, 90.0, tr("Session timeout"));
            });
            if id == "sio" {
                rows.row(
                    ui,
                    tr("Answer server acknowledgements with"),
                    tr("JSON array of arguments; empty = no answer"),
                    |ui| {
                        let mut reply = runner.ack_reply.clone().unwrap_or_default();
                        if pc::input_sm(ui, &mut reply, "[\"ok\"]", 160.0, true).changed() {
                            runner.ack_reply = (!reply.trim().is_empty()).then_some(reply);
                        }
                    },
                );
            }
        });
        let reset = ui.add_enabled_ui(customized, |ui| {
            kit::ghost(ui, Some(icons::ARROW_COUNTER_CLOCKWISE), tr("Reset to defaults"))
        });
        if reset.inner.clicked() {
            *runner = SessionRunner::default();
        }
    });
}

fn parse_binary(text: &str) -> Option<Vec<u8>> {
    let t = text.trim();
    let hex: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if hex.len().is_multiple_of(2) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect();
    }
    base64::engine::general_purpose::STANDARD.decode(t).ok()
}

fn connect(app: &mut TernaryWireApp, tab_id: u64, tab: &mut WsTab, item: &RequestItem) {
    let RequestKind::WebSocket(ws) = &item.request else { return };
    let collection_id = tab.location.as_ref().map(|l| l.collection.clone());
    let scopes = app.ws.scopes_for(collection_id.as_deref());
    let vault = app.ws.vault();
    let resolver = Resolver::new(&scopes).with_vault(vault.as_ref());
    // Reuse the HTTP preparation for variables, query params and auth headers.
    let auth = if ws.auth.is_inherit() {
        collection_id
            .as_deref()
            .and_then(|c| app.ws.data.collection(c))
            .and_then(|c| c.effective_auth(&item.id).map(|e| e.auth.clone()))
            .unwrap_or_else(ternarywire_core::auth::Auth::none)
    } else {
        ws.auth.clone()
    };
    let mut http = ternarywire_core::model::HttpRequest::new("GET", ws.url.clone());
    http.params = ws.params.clone();
    http.sync_url_from_params();
    http.headers = ws.headers.clone();
    let mut defaults = app.settings.request.clone();
    defaults.send_user_agent = true;
    let ctx =
        PrepareContext { resolver, auth: &auth, auth_owner_id: Some(&item.id), defaults: &defaults, base_dir: None };
    let mut prepared = prepare(&http, &ctx);
    if let Some(p) = ternarywire_auth::preview(&prepared) {
        prepared = p.request;
    }
    let mut url = prepared.url.clone();
    if let Some(rest) = url.strip_prefix("http://") {
        url = format!("ws://{rest}");
    } else if let Some(rest) = url.strip_prefix("https://") {
        url = format!("wss://{rest}");
    }
    let headers = prepared.headers.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("accept")).cloned().collect();
    let verify_ssl = ws.verify_ssl.unwrap_or(app.settings.request.verify_ssl);
    let tls = match app.tls_setup(&url, verify_ssl) {
        Ok(t) => t,
        Err(e) => {
            app.toast(e, true);
            return;
        }
    };
    let cfg = WsConfig {
        url,
        headers,
        subprotocols: ws.subprotocols.clone(),
        verify_ssl,
        tls,
        proxy: app.proxy_settings(),
        ..Default::default()
    };
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    tab.conn = State::Connecting;
    tab.error = None;
    tab.epoch = crate::app::next_epoch();
    let epoch = tab.epoch;
    tab.session = Some(WsSession::connect(app.rt.handle(), cfg, move |event| {
        let _ = tx.send(Msg::Ws { tab: tab_id, epoch, event });
        ctx.request_repaint();
    }));
}

/// Sends `text` (resolved) as a text or binary message.
fn send_message(
    app: &mut TernaryWireApp,
    tab: &WsTab,
    scopes: &ternarywire_core::variables::VariableScopes,
    text: &str,
    binary: bool,
) {
    let resolved = resolve_composer(app, scopes, text);
    let Some(s) = &tab.session else { return };
    if binary {
        match parse_binary(&resolved) {
            Some(b) => {
                s.send_binary(b);
            }
            None => app.toast(tr("Binary messages must be hex or base64"), true),
        }
    } else {
        s.send_text(resolved);
    }
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut WsTab) {
    let Some(original) = get_item(app, tab) else {
        ui.label(tr("This request no longer exists."));
        return;
    };
    let mut item = original.clone();
    // The rows of the view are edge to edge.
    ui.spacing_mut().item_spacing.y = 0.0;
    let collection_id = tab.location.as_ref().map(|l| l.collection.clone());
    let vars = app.var_ctx(collection_id.as_deref());
    let state = tab.state();
    let slot = tab.slot();
    let since = tab.since;
    let inherited = pc::inherited_auth(app, &slot, &item);
    // Handshake facts for the status row: `101 · 12 ms`, and in full.
    let handshake = match &tab.conn {
        State::Connected { protocol, status } => {
            let mut short = status.to_string();
            let mut full = format!("HTTP {status}");
            if let Some(ms) = tab.handshake_ms {
                let ms = ternarywire_core::util::human_ms(ms);
                short.push_str(&format!(" · {ms}"));
                full.push_str(&format!(" · {}", trf("handshake {}", &[&ms])));
            }
            if let Some(p) = protocol {
                full.push_str(&format!(" · {p}"));
            }
            Some((short, full))
        }
        _ => None,
    };
    pc::title_row(ui, app, tab_id, &slot, &mut item, |_| {});
    let RequestKind::WebSocket(ws) = &mut item.request else { return };
    let mut action = None;
    let connected = state == Conn::Live;
    let mut send_now: Option<(String, bool)> = None;
    if connected && pc::send_shortcut(ui) {
        send_now = Some((tab.composer.clone(), tab.binary));
    }
    match tab.auto.take().as_deref() {
        Some("connect") if state != Conn::Live => action = Some(ConnAction::Connect),
        Some("send") if connected => send_now = Some((tab.composer.clone(), tab.binary)),
        Some(a) => {
            if let Some(i) = a.strip_prefix("select:").and_then(|i| i.parse().ok()) {
                tab.log.selected = Some(i);
            } else if let Some(s) = a.strip_prefix("section:") {
                tab.section = match s {
                    "params" => Section::Params,
                    "headers" => Section::Headers,
                    "auth" => Section::Auth,
                    "saved" => Section::Saved,
                    "scripts" => Section::Scripts,
                    _ => Section::Message,
                };
            } else if let Some(t) = a.strip_prefix("composer:") {
                tab.composer = t.to_string();
            }
        }
        None => {}
    }
    let item_id = item.id.clone();
    let scripts = &mut item.scripts;
    let panes = ui.make_persistent_id("ws-panes");
    let mut retry = false;
    pc::view_panes(
        ui,
        panes,
        |ui| {
            let clicked = pc::connect_bar(
                ui,
                "WS",
                124.0,
                |ui| {
                    let mut url = ws.url.clone();
                    let w = ui.available_width();
                    let r = pc::bar_var(ui, "ws-url", &mut url, &vars, "ws://localhost:8080/socket", w);
                    if r.changed() {
                        ws.params = ternarywire_core::urlutil::merge_params_from_url(&url, &ws.params);
                        ws.url = url;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && state != Conn::Live {
                        action = Some(ConnAction::Connect);
                    }
                    let focused = r.has_focus();
                    r.on_hover_text(tr("Press ↩ to connect"));
                    focused
                },
                |ui| pc::connect_button(ui, state),
            );
            if clicked.is_some() {
                action = clicked;
            }
            let tabs = [
                Tab::new(Section::Message, tr("Message")),
                Tab::new(Section::Params, tr("Params")).count(ws.params.iter().filter(|p| p.enabled).count()),
                Tab::new(Section::Headers, tr("Headers")).count(ws.headers.iter().filter(|p| p.enabled).count()),
                Tab::new(Section::Auth, tr("Auth")).dot(pc::has_own_auth(&ws.auth)),
                Tab::new(Section::Saved, tr("Saved")).count(ws.messages.len()),
                Tab::new(Section::Scripts, tr("Scripts")).count(pc::scripts_count(scripts)),
            ];
            kit::tabs(ui, &mut tab.section, &tabs);
            let composer = &mut tab.composer;
            let binary = &mut tab.binary;
            match tab.section {
                Section::Message => {
                    let is_binary = *binary;
                    pc::toolbar(
                        ui,
                        |ui| {
                            ui.push_id("ws-mode", |ui| {
                                kit::segmented_ex(
                                    ui,
                                    binary,
                                    &[(false, None, tr("Text")), (true, None, tr("Binary"))],
                                    true,
                                )
                            });
                            if *binary {
                                ui.add(
                                    egui::Label::new(kit::hint(ui, tr("hex or base64, e.g. 01 02 ff or AQL/")))
                                        .truncate(),
                                );
                            }
                        },
                        |ui| {
                            if pc::toolbar_send(ui, tr("Send"), connected, tr("Connect first")) {
                                send_now = Some((composer.clone(), is_binary));
                            }
                            let wide = ui.available_width() > 300.0;
                            if pc::save_message_button_ex(ui, !composer.trim().is_empty(), wide) {
                                ws.messages.push(WsSavedMessage {
                                    id: ternarywire_core::util::new_id(),
                                    name: composer.lines().next().unwrap_or("").chars().take(30).collect(),
                                    body: composer.clone(),
                                    kind: if is_binary { WsMessageKind::Binary } else { WsMessageKind::Text },
                                });
                            }
                        },
                    );
                    let lang = if *binary { Lang::Text } else { Lang::Json };
                    let margin = egui::Margin { left: 12, right: 12, top: 0, bottom: 12 };
                    let r = pc::editor_well(ui, "ws-composer", composer, lang, Some(&vars), margin, None, None);
                    let hint = if *binary {
                        tr("Bytes as hex (01 02 ff) or base64")
                    } else {
                        tr("Type a message: JSON or text, with {{variables}}")
                    };
                    pc::well_placeholder(ui, &r, composer, hint);
                }
                Section::Params => {
                    pc::body(ui, "ws-params-scroll", 0.0, 0.0, |ui| {
                        let mut params = ws.params.clone();
                        let opts = KvOptions {
                            title: Some(tr("Query parameters")),
                            allow_add: true,
                            key_hint: tr("Key"),
                            value_hint: tr("Value"),
                            ..Default::default()
                        };
                        if widgets::kv_table(ui, "ws-params", &mut params, &vars, opts) {
                            ws.params = params;
                            ws.url = ternarywire_core::urlutil::replace_query(&ws.url, &ws.params);
                        }
                        ui.add_space(14.0);
                        ui.label(kit::hint(ui, tr("Kept in sync with the URL.")));
                    });
                }
                Section::Headers => {
                    pc::body(ui, "ws-headers-scroll", 0.0, 0.0, |ui| {
                        let mut headers = ws.headers.clone();
                        let opts = KvOptions {
                            title: Some(tr("Headers")),
                            key_suggestions: Some(widgets::COMMON_HEADERS),
                            allow_add: true,
                            key_hint: tr("Key"),
                            value_hint: tr("Value"),
                            ..Default::default()
                        };
                        if widgets::kv_table(ui, "ws-headers", &mut headers, &vars, opts) {
                            ws.headers = headers;
                        }
                        ui.add_space(14.0);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.label(kit::section_title(tr("Subprotocols")));
                        let mut text = ws.subprotocols.join(", ");
                        if pc::input_sm(ui, &mut text, tr("graphql-transport-ws, mqtt"), ui.available_width(), true)
                            .changed()
                        {
                            ws.subprotocols =
                                text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                        }
                        ui.label(kit::hint(ui, tr("Sent as Sec-WebSocket-Protocol, comma separated.")));
                    });
                }
                Section::Auth => pc::auth_pane(ui, "ws-auth-scroll", &mut ws.auth, &item_id, inherited, None, &vars),
                Section::Scripts => pc::scripts_ui(
                    ui,
                    scripts,
                    &[
                        (ternarywire_core::script::ScriptEvent::PreRequest, "Before connect"),
                        (ternarywire_core::script::ScriptEvent::OnMessage, "On message"),
                    ],
                    &mut tab.scripts,
                    &vars,
                ),
                Section::Saved => {
                    pc::toolbar(
                        ui,
                        |ui| pc::toolbar_title(ui, tr("Saved messages"), Some(ws.messages.len())),
                        |ui| {
                            ui.label(kit::hint(ui, tr("Stored with the request")));
                        },
                    );
                    pc::body(ui, "ws-saved", 0.0, 14.0, |ui| {
                        if ws.messages.is_empty() {
                            ui.label(kit::hint(
                                ui,
                                tr("Save messages from the composer to reuse them; they are stored with the request."),
                            ));
                        } else {
                            let acted = pc::saved_list(
                                ui,
                                "ws-saved-list",
                                &mut ws.messages,
                                connected,
                                tr("Send now"),
                                |ui, m| {
                                    pc::name_field(ui, &mut m.name, 130.0);
                                    if m.kind == WsMessageKind::Binary {
                                        kit::tone_pill(ui, tr("Binary"), kit::Tone::Info);
                                    }
                                    pc::row_summary(ui, &m.body);
                                },
                            );
                            match acted {
                                Some((i, RowAction::Use)) => {
                                    *composer = ws.messages[i].body.clone();
                                    *binary = ws.messages[i].kind == WsMessageKind::Binary;
                                    tab.section = Section::Message;
                                }
                                Some((i, RowAction::Send)) => {
                                    send_now = Some((
                                        ws.messages[i].body.clone(),
                                        ws.messages[i].kind == WsMessageKind::Binary,
                                    ));
                                }
                                Some((i, RowAction::Remove)) => {
                                    ws.messages.remove(i);
                                }
                                None => {}
                            }
                        }
                        runner_section(ui, "ws", &mut ws.runner, tr("message text, or its JSON type/event"));
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
                        if let Some((short, full)) = &handshake {
                            kit::metric(ui, icons::PLUGS_CONNECTED, short).on_hover_text(full);
                        }
                        if connected || sent + received > 0 {
                            kit::metric(ui, icons::ARROW_UP, &trf("{} sent", &[&sent]));
                            kit::metric(ui, icons::ARROW_DOWN, &trf("{} received", &[&received]));
                        }
                    });
                },
                |ui| pc::pane_actions(ui, panes),
            );
            tab.log.bar(ui, ("ws-log", tab_id));
            if let (Some(e), Conn::Failed) = (&tab.error, state) {
                let (r, dismiss) = pc::connection_error(ui, tr("Could not connect"), e);
                retry = r;
                if dismiss {
                    tab.error = None;
                }
            }
            tab.log.body(ui, ("ws-log", tab_id));
        },
    );
    if retry {
        action = Some(ConnAction::Connect);
    }
    if let Some((text, binary)) = send_now {
        send_message(app, tab, &vars.scopes, &text, binary);
    }
    if item != original {
        match &tab.location {
            Some(loc) => {
                let cid = loc.collection.clone();
                if let Some(target) =
                    app.ws.data.collection_mut(&loc.collection).and_then(|c| c.find_request_mut(&loc.item))
                {
                    *target = item.clone();
                    app.ws.mark_collection(&cid);
                }
            }
            None => tab.scratch = Some(item.clone()),
        }
    }
    use ternarywire_core::script::{ScriptEvent, protocol as proto};
    match action {
        Some(ConnAction::Connect) => {
            tab.scripts.clear_results();
            let pre = match &item.request {
                RequestKind::WebSocket(w) => crate::proto_common::run_scripts(
                    app,
                    tab_id,
                    &slot,
                    &item,
                    ScriptEvent::PreRequest,
                    proto::websocket(w),
                ),
                _ => None,
            };
            if let Some(run) = pre {
                tab.scripts.wait_for(run);
            } else {
                connect(app, tab_id, tab, &item);
            }
        }
        Some(ConnAction::Cancel) => {
            if let Some(s) = &tab.session {
                s.close(1000, "");
            }
            tab.session = None;
            tab.conn = State::Disconnected;
            tab.scripts.stop_waiting();
        }
        Some(ConnAction::Disconnect) => {
            if let Some(s) = &tab.session {
                s.close(1000, "");
            }
        }
        None => {}
    }
    // Pre-request finished (a skipped request just doesn't connect).
    if let Some(Some(v)) = tab.scripts.take_pre() {
        let mut sent = item.clone();
        if let RequestKind::WebSocket(w) = &mut sent.request {
            proto::apply_websocket(w, &v);
        }
        connect(app, tab_id, tab, &sent);
    }
    if let RequestKind::WebSocket(w) = &item.request {
        for m in std::mem::take(&mut tab.inbox) {
            if crate::proto_common::run_scripts(
                app,
                tab_id,
                &slot,
                &item,
                ScriptEvent::OnMessage,
                proto::websocket(w).with_message(m),
            )
            .is_none()
            {
                break;
            }
        }
    }
}

/// The message to send, with `{{variables}}` resolved.
fn resolve_composer(app: &TernaryWireApp, scopes: &ternarywire_core::variables::VariableScopes, text: &str) -> String {
    let vault = app.ws.vault();
    Resolver::new(scopes).with_vault(vault.as_ref()).resolve(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{{vault:…}}` in a message resolves like it does in the URL and
    /// headers instead of being sent literally.
    #[test]
    fn composer_resolves_vault_references() {
        let dir = tempfile::tempdir().unwrap();
        let app = TernaryWireApp::for_test(dir.path());
        app.ws.secrets.vault_set("ws-key", "s3cret");
        let scopes = app.ws.scopes_for(None);
        assert_eq!(resolve_composer(&app, &scopes, "{\"key\": \"{{vault:ws-key}}\"}"), "{\"key\": \"s3cret\"}");
    }

    /// While the pre-request script runs the tab counts as connecting, so
    /// Connect turns into Cancel (a second click used to start a second
    /// script and a second connection).
    #[test]
    fn waiting_for_the_pre_request_script_is_busy() {
        let mut tab = WsTab::scratch(RequestItem::new("ws", ternarywire_core::model::HttpRequest::default()));
        tab.scripts.wait_for(crate::app::next_epoch());
        assert_eq!(tab.state(), Conn::Busy);
        tab.scripts.stop_waiting();
        assert_eq!(tab.state(), Conn::Idle);
    }

    fn incoming(tab: &mut WsTab, text: String) {
        tab.on_event(WsEvent::Message { direction: WsDirection::Incoming, payload: WsPayload::Text(text), at_ms: 0 });
    }

    /// The log keeps the latest messages; the selected one stays selected as
    /// older ones are dropped, and messages waiting for on-message scripts
    /// (a tab in the background is not drawn) do not pile up without limit.
    #[test]
    fn full_log_keeps_the_selection_and_bounded_inbox() {
        let mut tab = WsTab::scratch(RequestItem::new("ws", ternarywire_core::model::HttpRequest::default()));
        let cap = tab.log.cap;
        for i in 0..cap {
            incoming(&mut tab, format!("m{i}"));
        }
        tab.log.selected = Some(5);
        incoming(&mut tab, "next".into());
        assert_eq!(tab.log.selected.and_then(|i| tab.log.lines.get(i)).map(|l| l.text.as_str()), Some("m5"));
        for i in 0..cap {
            incoming(&mut tab, format!("more{i}"));
        }
        assert_eq!(tab.log.lines.len(), cap);
        assert!(tab.inbox.len() <= crate::proto_common::MAX_INBOX);
    }

    /// A failed connection is shown as such (with its cause) until the next
    /// attempt; errors on an open connection do not mark it failed.
    #[test]
    fn failed_connection_state() {
        let mut tab = WsTab::scratch(RequestItem::new("ws", ternarywire_core::model::HttpRequest::default()));
        assert_eq!(tab.state(), Conn::Idle);
        tab.on_event(WsEvent::Connecting);
        assert_eq!(tab.state(), Conn::Busy);
        tab.on_event(WsEvent::Error("connection refused".into()));
        tab.on_event(WsEvent::Closed { code: None, reason: String::new(), by_peer: false });
        assert_eq!(tab.state(), Conn::Failed);
        assert_eq!(tab.error.as_deref(), Some("connection refused"));
        tab.on_event(WsEvent::Connecting);
        assert_eq!(tab.state(), Conn::Busy);
    }

    /// A log of 20,000 messages (some long, multi-line JSON) draws only the
    /// rows in view: a frame takes milliseconds, not one widget per message.
    #[test]
    fn big_log_draws_only_visible_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let mut item = RequestItem::new("ws", ternarywire_core::model::HttpRequest::default());
        item.request = RequestKind::WebSocket(Default::default());
        let id = app.add_tab(crate::app::TabKind::WebSocket(Box::new(WsTab::scratch(item))));
        let crate::app::TabKind::WebSocket(tab) = &mut app.tabs[0].kind else { panic!("ws tab") };
        tab.log.cap = 20_000;
        let json = serde_json::to_string_pretty(&serde_json::json!({"items": vec![1; 200]})).unwrap();
        for i in 0..20_000 {
            incoming(tab, if i % 10 == 0 { json.clone() } else { format!("message {i}") });
        }
        tab.log.selected = Some(19_990);
        app.draw_frame(Vec::new());
        let started = std::time::Instant::now();
        let frame = app.draw_frame(Vec::new());
        let took = started.elapsed();
        assert!(frame.labels.len() < 1_000, "{} widgets", frame.labels.len());
        assert!(frame.shapes < 2_000, "{} shapes", frame.shapes);
        assert!(took < std::time::Duration::from_millis(500), "{took:?}");
        // The newest rows are in view (the list sticks to the bottom), and
        // the selected message is shown in full.
        assert!(frame.labels.iter().any(|l| l == "message 19999"), "{:?}", frame.labels);
        assert!(app.tab_index(id).is_some());

        // Searching a big log is fine too.
        let crate::app::TabKind::WebSocket(tab) = &mut app.tabs[0].kind else { panic!("ws tab") };
        tab.log.search = "message 1999".into();
        app.draw_frame(Vec::new());
        let frame = app.draw_frame(Vec::new());
        assert!(frame.labels.iter().any(|l| l == "message 19998"), "{:?}", frame.labels);
        assert!(!frame.labels.iter().any(|l| l == "message 19989"));
    }
}
