//! MQTT request tab (FR-X5): connect to a broker (3.1.1 or 5), manage
//! subscriptions, publish and watch the message log.

use std::time::Instant;

use egui::{RichText, Ui};

use ternarywire_core::model::RequestKind;
use ternarywire_core::protocols::{MqttMessage, MqttSubscription, MqttVersion};
use ternarywire_mqtt::{Direction, MessageProperties, MqttConfig, MqttEvent, MqttSession, Publish};

use crate::app::{Msg, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tab;
use crate::proto_common::{
    self as pc, Conn, ConnAction, Dir, ItemSlot, Log, LogLine, RowAction, ScriptState, basic_credentials, header,
    payload_text, resolver,
};
use crate::{icons, kit, theme, widgets};
use ternarywire_core::script::ScriptEvent;
use ternarywire_core::script::protocol as proto;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Publish,
    Subscriptions,
    Saved,
    Connection,
    Scripts,
}

pub struct MqttTab {
    /// Number of the current connection (see `app::next_epoch`).
    pub epoch: u64,
    pub slot: ItemSlot,
    session: Option<MqttSession>,
    connected: bool,
    connecting: bool,
    section: Section,
    topic: String,
    payload: String,
    qos: u8,
    retain: bool,
    /// MQTT 5 user properties of the next publish.
    user_props: Vec<ternarywire_core::model::KeyValue>,
    log: Log,
    pub scripts: ScriptState,
    /// Incoming messages waiting for the on-message script.
    inbox: std::collections::VecDeque<serde_json::Value>,
    since: Option<Instant>,
    /// Why the last connection failed.
    error: Option<String>,
    /// Subscriptions the broker granted on this connection (topic, QoS).
    active: Vec<(String, u8)>,
    session_present: bool,
    /// Automation: `connect`, `publish`, `select:<row>`, `section:<name>`,
    /// `compose:<topic>|<qos>|<retain>|<payload>`.
    pub auto: Option<String>,
}

impl MqttTab {
    pub fn new(slot: ItemSlot) -> Self {
        MqttTab {
            epoch: 0,
            slot,
            session: None,
            connected: false,
            connecting: false,
            section: Section::Publish,
            topic: String::new(),
            payload: String::new(),
            qos: 0,
            retain: false,
            user_props: Vec::new(),
            log: Log::with_empty(
                "No messages yet",
                "Connect to the broker; messages on your subscriptions appear here.",
            ),
            scripts: ScriptState::default(),
            inbox: Default::default(),
            since: None,
            error: None,
            active: Vec::new(),
            session_present: false,
            auto: None,
        }
    }

    fn state(&self) -> Conn {
        if self.connected {
            Conn::Live
        } else if self.connecting || self.scripts.is_waiting() {
            Conn::Busy
        } else if self.error.is_some() {
            Conn::Failed
        } else {
            Conn::Idle
        }
    }

    /// An event with its MQTT 5 properties (user properties of messages).
    pub fn on_event(&mut self, e: MqttEvent, props: Option<MessageProperties>) {
        let user_props = props.map(|p| p.user_properties).unwrap_or_default();
        match e {
            MqttEvent::Connecting => {
                self.connecting = true;
                self.error = None;
                self.log.system(tr("Connecting…"));
            }
            MqttEvent::Connected { session_present } => {
                self.connecting = false;
                self.connected = true;
                self.since = Some(Instant::now());
                self.error = None;
                self.session_present = session_present;
                self.active.clear();
                self.log.system(trf("Connected{}", &[&if session_present { tr(" (session present)") } else { "" }]));
            }
            MqttEvent::Subscribed { topic, granted_qos } => {
                let qos = granted_qos.map(|q| format!(" (QoS {q})")).unwrap_or_default();
                self.log.system(trf("Subscribed to {}{}", &[&topic, &qos]));
                let granted = granted_qos.unwrap_or(0);
                match self.active.iter_mut().find(|(t, _)| *t == topic) {
                    Some(a) => a.1 = granted,
                    None => self.active.push((topic, granted)),
                }
            }
            MqttEvent::Unsubscribed { topic } => {
                self.log.system(trf("Unsubscribed from {}", &[&topic]));
                self.active.retain(|(t, _)| *t != topic);
            }
            MqttEvent::Message { topic, payload, qos, retain, direction, at_ms } => {
                let dir = if direction == Direction::Incoming { Dir::In } else { Dir::Out };
                let mut meta = vec![format!("QoS {qos}")];
                if retain {
                    meta.push(tr("retained").to_string());
                }
                if !user_props.is_empty() {
                    meta.push(trf("{} props", &[&user_props.len()]));
                }
                let size = payload.len();
                let mut text = payload_text(&payload);
                if direction == Direction::Incoming {
                    let mut m = proto::message(proto::json_or_text(&text), proto::Direction::Incoming, at_ms);
                    m["topic"] = serde_json::json!(topic);
                    m["qos"] = serde_json::json!(qos);
                    m["retain"] = serde_json::json!(retain);
                    m["userProperties"] = user_props
                        .iter()
                        .map(|(k, v)| serde_json::json!({"key": k, "value": v}))
                        .collect::<Vec<_>>()
                        .into();
                    crate::proto_common::queue_message(&mut self.inbox, m);
                }
                if !user_props.is_empty() {
                    text.push_str(&format!("\n\n{}", tr("User properties:")));
                    for (k, v) in &user_props {
                        text.push_str(&format!("\n  {k}: {v}"));
                    }
                }
                self.log.push_line(LogLine {
                    dir,
                    title: topic,
                    text,
                    at_ms,
                    size,
                    error: false,
                    binary: std::str::from_utf8(&payload).is_err(),
                    meta: meta.join(" · "),
                });
            }
            MqttEvent::Error(m) => {
                if !self.connected {
                    self.error = Some(m.clone());
                }
                self.log.error(m)
            }
            MqttEvent::Disconnected { reason } => {
                if self.connecting && !reason.is_empty() && self.error.is_none() {
                    self.error = Some(reason.clone());
                }
                self.connected = false;
                self.connecting = false;
                self.session = None;
                self.since = None;
                self.active.clear();
                self.log.system(if reason.is_empty() {
                    tr("Disconnected").to_string()
                } else {
                    trf("Disconnected: {}", &[&reason])
                });
            }
        }
    }
}

/// Enabled, named user properties with variables resolved.
fn publish_properties(
    rows: &[ternarywire_core::model::KeyValue],
    resolve: &dyn Fn(&str) -> String,
) -> Vec<(String, String)> {
    rows.iter()
        .filter(|p| p.enabled && !p.key.trim().is_empty())
        .map(|p| (resolve(&p.key), resolve(&p.value)))
        .collect()
}

/// What to publish: topic, payload, QoS, retain and user properties.
type Outgoing = (String, String, u8, bool, Vec<ternarywire_core::model::KeyValue>);

fn publish(app: &TernaryWireApp, tab: &MqttTab, collection: Option<&str>, out: Outgoing) {
    let (topic, payload, qos, retain, props) = out;
    let r = resolver(app, collection);
    let Some(s) = &tab.session else { return };
    let publish = Publish { topic: r(&topic), payload: r(&payload).into_bytes(), qos, retain };
    let user_properties = publish_properties(&props, &r);
    if user_properties.is_empty() {
        s.publish(publish);
    } else {
        s.publish_with_properties(publish, MessageProperties { user_properties });
    }
}

const QOS: [(u8, Option<&str>, &str); 3] = [(0, None, "0"), (1, None, "1"), (2, None, "2")];

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut MqttTab) {
    let Some(original) = tab.slot.load(app) else {
        ui.label(tr("This request no longer exists."));
        return;
    };
    let mut item = original.clone();
    // The rows of the view are edge to edge.
    ui.spacing_mut().item_spacing.y = 0.0;
    let collection = tab.slot.collection_id();
    let vars = app.var_ctx(collection.as_deref());
    let resolve = resolver(app, collection.as_deref());
    let state = tab.state();
    let since = tab.since;
    let v5 = matches!(&item.request, RequestKind::Mqtt(r) if r.version == MqttVersion::V5);
    header(ui, app, tab_id, &tab.slot, &mut item, |ui| {
        let opts = kit::PillOpts { outline: true, ..Default::default() };
        kit::pill_ex(ui, if v5 { "MQTT 5" } else { "MQTT 3.1.1" }, kit::Tone::Neutral, opts);
    });
    let RequestKind::Mqtt(req) = &mut item.request else { return };
    let mut action = None;
    let connected = state == Conn::Live;
    let mut outgoing: Option<Outgoing> = None;
    let can_publish = connected && !tab.topic.trim().is_empty();
    if can_publish && pc::send_shortcut(ui) {
        outgoing = Some((tab.topic.clone(), tab.payload.clone(), tab.qos, tab.retain, tab.user_props.clone()));
    }
    match tab.auto.take().as_deref() {
        Some("connect") if state != Conn::Live => action = Some(ConnAction::Connect),
        Some("publish") if can_publish => {
            outgoing = Some((tab.topic.clone(), tab.payload.clone(), tab.qos, tab.retain, tab.user_props.clone()))
        }
        Some(a) => {
            if let Some(i) = a.strip_prefix("select:").and_then(|i| i.parse().ok()) {
                tab.log.selected = Some(i);
            } else if let Some(s) = a.strip_prefix("section:") {
                tab.section = match s {
                    "subs" | "subscriptions" => Section::Subscriptions,
                    "saved" => Section::Saved,
                    "conn" | "connection" => Section::Connection,
                    "scripts" => Section::Scripts,
                    _ => Section::Publish,
                };
            } else if let Some(rest) = a.strip_prefix("compose:") {
                // `compose:<topic>|<qos>|<retain>|<payload>`
                let mut parts = rest.splitn(4, '|');
                tab.topic = parts.next().unwrap_or_default().to_string();
                tab.qos = parts.next().and_then(|q| q.parse().ok()).unwrap_or(0);
                tab.retain = parts.next() == Some("true");
                tab.payload = parts.next().unwrap_or_default().to_string();
            }
        }
        None => {}
    }
    let mut sub_action: Option<(bool, String, u8)> = None;
    let mut unsubscribe: Option<String> = None;
    let mut go_subs = false;
    let mut retry = false;
    let panes = ui.make_persistent_id("mqtt-panes");
    let session_present = tab.session_present;
    let scripts = &mut item.scripts;
    pc::view_panes(
        ui,
        panes,
        |ui| {
            let clicked = pc::connect_bar(
                ui,
                "MQTT",
                124.0,
                |ui| {
                    let w = ui.available_width();
                    let r = pc::bar_var(ui, "mqtt-url", &mut req.url, &vars, "mqtt://localhost:1883", w);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && state != Conn::Live {
                        action = Some(ConnAction::Connect);
                    }
                    r.has_focus()
                },
                |ui| pc::connect_button(ui, state),
            );
            if clicked.is_some() {
                action = clicked;
            }
            let active_subs = req.subscriptions.iter().filter(|s| s.enabled).count();
            let tabs = [
                Tab::new(Section::Publish, tr("Publish")),
                Tab::new(Section::Subscriptions, tr("Subscriptions")).count(active_subs),
                Tab::new(Section::Saved, tr("Saved")).count(req.messages.len()),
                Tab::new(Section::Connection, tr("Connection")),
                Tab::new(Section::Scripts, tr("Scripts")).count(pc::scripts_count(scripts)),
            ];
            kit::tabs(ui, &mut tab.section, &tabs);
            let t = &mut tab.topic;
            let payload = &mut tab.payload;
            let qos = &mut tab.qos;
            let retain = &mut tab.retain;
            let user_props = &mut tab.user_props;
            let p = theme::pal(ui);
            let label = |ui: &mut Ui, text: &str| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::hover());
                ui.painter().text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    text,
                    egui::FontId::proportional(13.0),
                    p.text_muted,
                );
            };
            match tab.section {
                Section::Publish => {
                    pc::toolbar(
                        ui,
                        |ui| {
                            label(ui, tr("Topic"));
                            let w = ui.available_width();
                            pc::var_input_sm(ui, "mqtt-topic", t, &vars, "sensors/kitchen/temp", w);
                        },
                        |_| {},
                    );
                    let (topic_now, qos_now, retain_now) = (t.clone(), *qos, *retain);
                    pc::toolbar(
                        ui,
                        |ui| {
                            label(ui, "QoS");
                            ui.push_id("pub-qos", |ui| kit::segmented_ex(ui, qos, &QOS, true));
                            ui.add_space(6.0);
                            pc::check_label(ui, retain, tr("Retain")).on_hover_text(tr(
                                "The broker keeps the last retained message of a topic for new subscribers",
                            ));
                        },
                        |ui| {
                            let hint = if connected { tr("Enter a topic first") } else { tr("Connect first") };
                            if pc::toolbar_send(ui, tr("Publish"), connected && !topic_now.trim().is_empty(), hint) {
                                outgoing =
                                    Some((topic_now.clone(), payload.clone(), qos_now, retain_now, user_props.clone()));
                            }
                            if pc::save_message_button(ui, !topic_now.trim().is_empty()) {
                                req.messages.push(MqttMessage {
                                    name: topic_now.clone(),
                                    topic: topic_now.clone(),
                                    payload: payload.clone(),
                                    qos: qos_now,
                                    retain: retain_now,
                                    user_properties: user_props.clone(),
                                    ..Default::default()
                                });
                            }
                        },
                    );
                    // The user properties (MQTT 5) under the editor keep their
                    // height (measured on the previous frame).
                    let props_id = ui.make_persistent_id("mqtt-props-h");
                    let props_h: f32 = if req.version == MqttVersion::V5 {
                        ui.data(|d| d.get_temp(props_id).unwrap_or(120.0))
                    } else {
                        0.0
                    };
                    let margin = egui::Margin { left: 12, right: 12, top: 4, bottom: 10 };
                    let h = (ui.available_height() - 14.0 - props_h).max(80.0);
                    let r =
                        pc::editor_well(ui, "mqtt-payload", payload, Lang::Json, Some(&vars), margin, Some(h), None);
                    pc::well_placeholder(ui, &r, payload, tr("Payload: JSON or text, with {{variables}}"));
                    if req.version == MqttVersion::V5 {
                        let top = ui.cursor().top();
                        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 12 }).show(
                            ui,
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 8.0;
                                let id = ui.make_persistent_id("mqtt-user-props-open");
                                let n = user_props.iter().filter(|p| p.enabled && !p.key.is_empty()).count();
                                let open = pc::disclosure(ui, id, tr("User properties"), Some(n), true, |ui| {
                                    ui.label(RichText::new("MQTT 5").size(11.0).color(p.text_faint));
                                });
                                if open {
                                    egui::ScrollArea::vertical().id_salt("mqtt-props-scroll").max_height(200.0).show(
                                        ui,
                                        |ui| {
                                            let opts = widgets::KvOptions {
                                                title: None,
                                                key_suggestions: None,
                                                show_description: false,
                                                key_hint: tr("Name"),
                                                value_hint: tr("Value"),
                                                readonly_keys: false,
                                                allow_add: true,
                                            };
                                            widgets::kv_table(ui, "mqtt-user-props", user_props, &vars, opts);
                                        },
                                    );
                                }
                            },
                        );
                        let used = ui.cursor().top() - top;
                        if (used - props_h).abs() > 0.5 {
                            ui.data_mut(|d| d.insert_temp(props_id, used));
                            ui.ctx().request_repaint();
                        }
                    }
                }
                Section::Subscriptions => {
                    let n_subs = req.subscriptions.len();
                    pc::toolbar(
                        ui,
                        |ui| pc::toolbar_title(ui, tr("Subscriptions"), Some(n_subs)),
                        |ui| {
                            if kit::ghost(ui, Some(icons::PLUS), tr("Add subscription")).clicked() {
                                req.subscriptions.push(MqttSubscription { topic: "#".into(), qos: 0, enabled: true });
                            }
                        },
                    );
                    pc::body(ui, "mqtt-subs", 0.0, 10.0, |ui| {
                        ui.label(kit::hint(
                            ui,
                            tr("Enabled subscriptions are made automatically after connecting. Wildcards + and # are allowed."),
                        ));
                        if req.subscriptions.is_empty() {
                            ui.label(kit::hint(ui, tr("No subscriptions yet.")));
                            return;
                        }
                        let mut remove = None;
                        let n = req.subscriptions.len();
                        pc::list_box(ui, n, 40.0, |ui, i| {
                            let s = &mut req.subscriptions[i];
                            kit::check(ui, &mut s.enabled, tr("Subscribe after connecting"))
                                .on_hover_text(tr("Subscribe after connecting"));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(8.0);
                                if kit::icon_button_sm(ui, icons::TRASH, tr("Remove subscription")).clicked() {
                                    remove = Some(i);
                                }
                                if connected {
                                    let live = tab.active.iter().any(|(t, _)| *t == resolve(&s.topic));
                                    let (icon, tip) = if live {
                                        (icons::MINUS, tr("Unsubscribe"))
                                    } else {
                                        (icons::PLUS, tr("Subscribe"))
                                    };
                                    let opts = kit::ButtonOpts { icon: Some(icon), ..Default::default() };
                                    let r =
                                        kit::button(ui, kit::Kind::Ghost, kit::Size::Sm, "", opts).on_hover_text(tip);
                                    widgets::a11y_name(&r, tip);
                                    if r.clicked() {
                                        sub_action = Some((!live, s.topic.clone(), s.qos));
                                    }
                                }
                                ui.push_id("qos", |ui| kit::segmented_ex(ui, &mut s.qos, &QOS, true));
                                pc::row_input(ui, &mut s.topic, "sensors/#", ui.available_width(), true);
                            });
                        });
                        if let Some(i) = remove {
                            req.subscriptions.remove(i);
                        }
                    });
                }
                Section::Saved => {
                    pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("Saved messages"), Some(req.messages.len())), |_| {});
                    pc::body(ui, "mqtt-saved", 0.0, 14.0, |ui| {
                        if req.messages.is_empty() {
                            ui.label(kit::hint(ui, tr("Save messages from the Publish section to reuse them.")));
                        } else {
                            let acted = pc::saved_list(
                                ui,
                                "mqtt-saved-list",
                                &mut req.messages,
                                connected,
                                tr("Publish now"),
                                |ui, m| {
                                    pc::name_field(ui, &mut m.name, 120.0);
                                    pc::mono_tag(ui, &m.topic, 180.0);
                                    if m.qos > 0 {
                                        kit::tone_pill(ui, &format!("QoS {}", m.qos), kit::Tone::Neutral);
                                    }
                                    pc::row_summary(ui, &m.payload);
                                },
                            );
                            match acted {
                                Some((i, RowAction::Use)) => {
                                    let m = req.messages[i].clone();
                                    *t = m.topic;
                                    *payload = m.payload;
                                    *qos = m.qos;
                                    *retain = m.retain;
                                    *user_props = m.user_properties;
                                    tab.section = Section::Publish;
                                }
                                Some((i, RowAction::Send)) => {
                                    let m = &req.messages[i];
                                    outgoing = Some((
                                        m.topic.clone(),
                                        m.payload.clone(),
                                        m.qos,
                                        m.retain,
                                        m.user_properties.clone(),
                                    ));
                                }
                                Some((i, RowAction::Remove)) => {
                                    req.messages.remove(i);
                                }
                                None => {}
                            }
                        }
                        crate::ws_tab::runner_section(ui, "mqtt", &mut req.runner, tr("topic filter, e.g. status/+"));
                    });
                }
                Section::Scripts => pc::scripts_ui(
                    ui,
                    scripts,
                    &[(ScriptEvent::PreRequest, "Before connect"), (ScriptEvent::OnMessage, "On message")],
                    &mut tab.scripts,
                    &vars,
                ),
                Section::Connection => {
                    pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("Connection"), None), |_| {});
                    pc::body(ui, "mqtt-conn", 0.0, 16.0, |ui| {
                        kit::group(ui, "", "", |ui, rows| {
                            rows.row(ui, tr("Version"), "", |ui| {
                                ui.push_id("version", |ui| {
                                    kit::segmented_ex(
                                        ui,
                                        &mut req.version,
                                        &[(MqttVersion::V311, None, "3.1.1"), (MqttVersion::V5, None, "5.0")],
                                        true,
                                    )
                                });
                            });
                            rows.row(ui, tr("Client ID"), "", |ui| {
                                pc::input_sm(ui, &mut req.client_id, tr("generated when empty"), 190.0, true);
                            });
                            rows.row(ui, tr("Keep alive (s)"), "", |ui| {
                                pc::number_input(
                                    ui,
                                    "mqtt-keep-alive",
                                    &mut req.keep_alive_secs,
                                    80.0,
                                    tr("Keep alive (s)"),
                                );
                            });
                            rows.row(ui, tr("Clean session"), tr("Start without stored subscriptions"), |ui| {
                                kit::check(ui, &mut req.clean_session, tr("Clean session"));
                            });
                            rows.row(ui, tr("Verify SSL certificates"), tr("For mqtts:// and wss:// brokers"), |ui| {
                                let mut verify = req.verify_ssl.unwrap_or(true);
                                if kit::check(ui, &mut verify, tr("Verify SSL certificates")).changed() {
                                    req.verify_ssl = Some(verify);
                                }
                            });
                        });
                        basic_credentials(ui, &mut req.auth, &vars);
                        kit::group(ui, tr("Last will"), "", |ui, rows| {
                            rows.block(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let mut has_will = req.last_will.is_some();
                                if kit::checkbox(ui, &mut has_will, tr("Send a last will")).changed() {
                                    req.last_will = if has_will { Some(MqttMessage::default()) } else { None };
                                }
                                ui.label(kit::hint(
                                    ui,
                                    tr("Published by the broker when this client disconnects unexpectedly."),
                                ));
                            });
                            if let Some(w) = &mut req.last_will {
                                rows.row(ui, tr("Topic"), "", |ui| {
                                    pc::input_sm(ui, &mut w.topic, "clients/ternarywire/status", 220.0, true);
                                });
                                rows.row(ui, tr("Payload"), "", |ui| {
                                    pc::input_sm(ui, &mut w.payload, "offline", 220.0, true);
                                });
                                rows.row(ui, "QoS", "", |ui| {
                                    pc::check_label(ui, &mut w.retain, tr("Retain"));
                                    ui.push_id("will-qos", |ui| kit::segmented_ex(ui, &mut w.qos, &QOS, true));
                                });
                            }
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
                        if connected && session_present {
                            kit::metric(ui, icons::CHECK, tr("session present"));
                        }
                        if connected || sent + received > 0 {
                            kit::metric(ui, icons::ARROW_UP, &trf("{} sent", &[&sent]));
                            kit::metric(ui, icons::ARROW_DOWN, &trf("{} received", &[&received]));
                        }
                    });
                },
                |ui| pc::pane_actions(ui, panes),
            );
            tab.log.bar(ui, ("mqtt-log", tab_id));
            if let (Some(e), Conn::Failed) = (&tab.error, state) {
                let (r, dismiss) = pc::connection_error(ui, tr("Could not connect to the broker"), e);
                retry = r;
                if dismiss {
                    tab.error = None;
                }
            }
            if connected {
                let (u, add) = subscribed_chips(ui, &tab.active);
                unsubscribe = u;
                go_subs = add;
            }
            tab.log.body(ui, ("mqtt-log", tab_id));
        },
    );
    if retry {
        action = Some(ConnAction::Connect);
    }
    if go_subs {
        tab.section = Section::Subscriptions;
    }
    if let Some(out) = outgoing {
        publish(app, tab, collection.as_deref(), out);
    }
    if let (Some(topic), Some(sess)) = (unsubscribe, &tab.session) {
        // Already resolved (as the broker granted it).
        sess.unsubscribe(&topic);
    }
    if let (Some((subscribe, topic, qos)), Some(sess)) = (sub_action, &tab.session) {
        // Topics may hold variables, as when connecting.
        let topic = resolver(app, collection.as_deref())(&topic);
        if subscribe {
            sess.subscribe(&topic, qos);
        } else {
            sess.unsubscribe(&topic);
        }
    }
    let req_clone = req.clone();
    let item_now = item.clone();
    tab.slot.store(app, &original, item);
    match action {
        Some(ConnAction::Connect) => {
            tab.scripts.clear_results();
            if let Some(run) = crate::proto_common::run_scripts(
                app,
                tab_id,
                &tab.slot,
                &item_now,
                ScriptEvent::PreRequest,
                proto::mqtt(&req_clone),
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
            if action == Some(ConnAction::Cancel) {
                tab.session = None;
                tab.connecting = false;
                tab.scripts.stop_waiting();
            }
        }
        None => {}
    }
    if let Some(pre) = tab.scripts.take_pre() {
        match pre {
            Some(v) => {
                let mut r = req_clone.clone();
                proto::apply_mqtt(&mut r, &v);
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
            proto::mqtt(&req_clone).with_message(m),
        )
        .is_none()
        {
            break;
        }
    }
}

/// The subscriptions the broker granted, as chips above the log (× to
/// unsubscribe), and Add (opens the Subscriptions section). Returns the
/// topic to unsubscribe and whether Add was clicked.
fn subscribed_chips(ui: &mut Ui, active: &[(String, u8)]) -> (Option<String>, bool) {
    let p = theme::pal(ui);
    let mut unsubscribe = None;
    let mut add = false;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Rows of 24 (`.subchip`, the ghost button), 6 apart.
        ui.spacing_mut().interact_size.y = 24.0;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            ui.add_sized(egui::vec2(0.0, 24.0), egui::Label::new(kit::eyebrow(ui, tr("Subscribed"))).selectable(false));
            ui.add_space(2.0);
            for (topic, qos) in active {
                let tg = kit::elide(ui, topic, theme::mono(12.5), p.text, 220.0);
                let qg =
                    ui.painter().layout_no_wrap(format!("QoS {qos}"), egui::FontId::proportional(11.5), p.text_faint);
                // A 1 px border, 9 in, the topic, its QoS and × 6 apart, 4 out.
                let w = 1.0 + 9.0 + tg.size().x + 6.0 + qg.size().x + 6.0 + 18.0 + 4.0 + 1.0;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 24.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(12),
                    p.surface,
                    egui::Stroke::new(1.0, p.border),
                    egui::StrokeKind::Inside,
                );
                let mut x = rect.left() + 10.0;
                ui.painter().galley(egui::pos2(x, rect.center().y - tg.size().y / 2.0), tg.clone(), p.text);
                x += tg.size().x + 6.0;
                ui.painter().galley(egui::pos2(x, rect.center().y - qg.size().y / 2.0), qg.clone(), p.text_faint);
                let xr = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 14.0, rect.center().y),
                    egui::vec2(18.0, 18.0),
                );
                let r = ui.interact(xr, ui.make_persistent_id(("unsubscribe", topic)), egui::Sense::click());
                let r = r.on_hover_text(tr("Unsubscribe"));
                widgets::a11y_button(&r, &format!("{} {topic}", tr("Unsubscribe")));
                if r.hovered() {
                    ui.painter().circle_filled(xr.center(), 9.0, p.hover);
                }
                let c = if r.hovered() { p.text } else { p.text_faint };
                ui.painter().text(xr.center(), egui::Align2::CENTER_CENTER, icons::X, theme::icon_font(10.0), c);
                if r.clicked() {
                    unsubscribe = Some(topic.clone());
                }
            }
            if kit::ghost(ui, Some(icons::PLUS), tr("Add")).on_hover_text(tr("Add subscription")).clicked() {
                add = true;
            }
        });
    });
    let bottom = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), bottom - 0.5, egui::Stroke::new(1.0, p.border));
    (unsubscribe, add)
}

fn start(
    app: &mut TernaryWireApp,
    tab_id: u64,
    tab: &mut MqttTab,
    req_clone: &ternarywire_core::protocols::MqttRequest,
    collection: Option<&str>,
) {
    {
        let r = resolver(app, collection);
        let mut cfg = MqttConfig::from_request(req_clone, &r, app.settings.request.verify_ssl);
        cfg.proxy = app.proxy_settings();
        match app.tls_setup(&cfg.url, cfg.verify_ssl) {
            Ok(tls) => cfg.tls = tls,
            Err(e) => return app.toast(e, true),
        }
        let tx = app.tx.clone();
        let ctx = app.ctx.clone();
        tab.epoch = crate::app::next_epoch();
        let epoch = tab.epoch;
        let session = MqttSession::connect_with_properties(app.rt.handle(), cfg, move |event, props| {
            let _ = tx.send(Msg::Mqtt { tab: tab_id, epoch, event, props });
            ctx.request_repaint();
        });
        for s in req_clone.subscriptions.iter().filter(|s| s.enabled && !s.topic.trim().is_empty()) {
            session.subscribe(&r(&s.topic), s.qos);
        }
        tab.session = Some(session);
        tab.connecting = true;
        tab.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ternarywire_core::model::KeyValue;

    #[test]
    fn mqtt5_user_properties_are_shown_and_given_to_scripts() {
        let mut tab = MqttTab::new(ItemSlot::default());
        let props =
            MessageProperties { user_properties: vec![("trace".into(), "abc".into()), ("k".into(), "v".into())] };
        let message = MqttEvent::Message {
            topic: "t/1".into(),
            payload: b"{\"n\":1}".to_vec(),
            qos: 1,
            retain: false,
            direction: Direction::Incoming,
            at_ms: 0,
        };
        tab.on_event(message, Some(props));
        let line = tab.log.lines.back().unwrap();
        assert!(line.text.contains("trace: abc") && line.text.contains("k: v"), "{}", line.text);
        assert_eq!(line.title, "t/1");
        assert!(line.meta.contains("QoS 1"), "{}", line.meta);
        let m = tab.inbox.back().unwrap();
        assert_eq!(m["userProperties"][0], serde_json::json!({"key": "trace", "value": "abc"}));

        let rows = vec![KeyValue::new("a", "{{x}}"), KeyValue::new("", "skip"), KeyValue::new("off", "1").disabled()];
        let resolve = |s: &str| s.replace("{{x}}", "42");
        assert_eq!(publish_properties(&rows, &resolve), vec![("a".to_string(), "42".to_string())]);
    }
}
