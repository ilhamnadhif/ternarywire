//! gRPC request tab (FR-X2): schema via server reflection or `.proto` files,
//! service/method picker, unary and streaming calls with JSON messages.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::{RichText, Ui};

use ternarywire_core::model::RequestKind;
use ternarywire_core::protocols::GrpcSchemaSource;
use ternarywire_grpc::{Direction, GrpcCall, GrpcEvent, GrpcSchema, MethodKind, Target};

use crate::app::{Msg, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tab;
use crate::proto_common::{self as pc, Conn, Dir, ItemSlot, Log, ScriptState, header, resolver};
use crate::widgets::{self, KvOptions};
use crate::{icons, kit, theme};
use ternarywire_core::script::ScriptEvent;
use ternarywire_core::script::protocol as proto;

pub enum GrpcMsg {
    Schema(Result<Arc<GrpcSchema>, String>),
    Event(GrpcEvent),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Message,
    Metadata,
    Auth,
    Schema,
    Scripts,
    Settings,
}

/// Script work waiting for the next frame (it needs the request).
enum Queued {
    Message(serde_json::Value),
    Response(serde_json::Value),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RespSection {
    Messages,
    Headers,
    Trailers,
}

pub struct GrpcTab {
    /// Number of the current call (see `app::next_epoch`).
    pub epoch: u64,
    pub slot: ItemSlot,
    schema: Option<Arc<GrpcSchema>>,
    /// Services and methods of the schema (listed once when it loads).
    methods: Vec<ternarywire_grpc::ServiceInfo>,
    schema_loading: bool,
    schema_error: Option<String>,
    call: Option<GrpcCall>,
    /// When the current call started.
    started: Option<Instant>,
    section: Section,
    resp_section: RespSection,
    log: Log,
    status: Option<(i32, String, String, f64)>,
    headers: Vec<(String, String)>,
    trailers: Vec<(String, String)>,
    /// Text typed in the method picker.
    method_search: String,
    /// Set by UI automation.
    pub auto_load: bool,
    pub auto_invoke: bool,
    /// Automation: `picker`, `select:<row>`, `section:<name>`, `resp:<name>`.
    pub auto: Option<String>,
    pub scripts: ScriptState,
    /// Messages of the current call for the post-response script, and the
    /// script inputs not run yet (a tab in the background is not drawn):
    /// the latest [`pc::MAX_INBOX`] messages of a long stream.
    received: VecDeque<serde_json::Value>,
    queue: VecDeque<Queued>,
    /// Input and output message types of the current call (log titles).
    msg_types: (String, String),
}

impl GrpcTab {
    pub fn new(slot: ItemSlot) -> Self {
        let mut log = Log::with_empty("No response yet", "Pick a method and press Invoke to call it.");
        log.title = "";
        GrpcTab {
            epoch: 0,
            slot,
            schema: None,
            methods: Vec::new(),
            schema_loading: false,
            schema_error: None,
            call: None,
            started: None,
            section: Section::Message,
            resp_section: RespSection::Messages,
            log,
            status: None,
            headers: Vec::new(),
            trailers: Vec::new(),
            method_search: String::new(),
            auto_load: false,
            auto_invoke: false,
            auto: None,
            scripts: ScriptState::default(),
            received: VecDeque::new(),
            queue: VecDeque::new(),
            msg_types: Default::default(),
        }
    }

    /// Automation: switch to the Scripts section.
    pub fn show_scripts(&mut self) {
        self.section = Section::Scripts;
    }

    /// The loaded schema (for automation).
    pub fn schema(&self) -> Option<&Arc<GrpcSchema>> {
        self.schema.as_ref()
    }

    pub fn on_msg(&mut self, m: GrpcMsg) {
        match m {
            GrpcMsg::Schema(r) => {
                self.schema_loading = false;
                match r {
                    Ok(s) => {
                        let n: usize = s.services().iter().map(|s| s.methods.len()).sum();
                        self.log.system(trf("Schema loaded: {} services, {} methods", &[&s.services().len(), &n]));
                        for w in s.warnings() {
                            self.log.system(trf("warning: {}", &[&w]));
                        }
                        self.methods = s.services();
                        self.schema = Some(s);
                        self.schema_error = None;
                    }
                    Err(e) => {
                        self.log.error(trf("Schema: {}", &[&e]));
                        self.schema_error = Some(e);
                    }
                }
            }
            GrpcMsg::Event(e) => match e {
                GrpcEvent::Connecting => self.log.system(tr("Connecting…")),
                GrpcEvent::ResponseHeaders(h) => self.headers = h,
                GrpcEvent::Message { direction, json, at_ms } => {
                    let dir = if direction == Direction::Received { Dir::In } else { Dir::Out };
                    if direction == Direction::Received {
                        let m = proto::message(proto::json_or_text(&json), proto::Direction::Incoming, at_ms);
                        pc::queue_message(&mut self.received, m.clone());
                        self.queue.push_back(Queued::Message(m));
                        if self.queue.len() > pc::MAX_INBOX
                            && let Some(i) = self.queue.iter().position(|q| matches!(q, Queued::Message(_)))
                        {
                            self.queue.remove(i);
                        }
                    }
                    let title = if dir == Dir::In { &self.msg_types.1 } else { &self.msg_types.0 };
                    self.log.push(dir, title.clone(), json, at_ms);
                }
                GrpcEvent::Trailers(t) => self.trailers = t,
                GrpcEvent::Finished { code, code_name, message, elapsed_ms } => {
                    let message_part = if message.is_empty() { String::new() } else { format!(": {message}") };
                    let took = ternarywire_core::util::human_ms(elapsed_ms);
                    self.log.system(trf("{} ({}){} in {}", &[&code_name, &code, &message_part, &took]));
                    let resp = proto::grpc_response(
                        code,
                        &message,
                        &self.headers,
                        &self.trailers,
                        std::mem::take(&mut self.received).into(),
                        elapsed_ms,
                    );
                    self.queue.push_back(Queued::Response(resp));
                    self.status = Some((code, code_name, message, elapsed_ms));
                    self.call = None;
                    self.started = None;
                }
                GrpcEvent::Error(m) => {
                    self.log.error(m);
                }
            },
        }
    }
}

fn target_for(app: &TernaryWireApp, tab: &GrpcTab, item: &ternarywire_core::model::RequestItem) -> Option<Target> {
    let RequestKind::Grpc(req) = &item.request else { return None };
    let collection = tab.slot.collection_id();
    let r = resolver(app, collection.as_deref());
    let mut target = Target::from_request(req, &r, app.settings.request.verify_ssl)
        .with_certificates(app.settings.certificates.clone(), Some(TernaryWireApp::vault_resolver(&app.ws)))
        .with_proxy(app.proxy_settings());
    if req.auth.is_inherit()
        && let Some(eff) =
            collection.as_deref().and_then(|c| app.ws.data.collection(c)).and_then(|c| c.effective_auth(&item.id))
    {
        for (k, v) in ternarywire_grpc::auth_metadata(eff.auth, &r) {
            if !target.metadata.iter().any(|(mk, _)| mk.eq_ignore_ascii_case(&k)) {
                target.metadata.push((k, v));
            }
        }
    }
    Some(target)
}

fn load_schema(app: &mut TernaryWireApp, tab_id: u64, tab: &mut GrpcTab, item: &ternarywire_core::model::RequestItem) {
    let RequestKind::Grpc(req) = &item.request else { return };
    let Some(target) = target_for(app, tab, item) else { return };
    tab.schema_loading = true;
    tab.schema_error = None;
    let source = req.schema.clone();
    let base = app.ws.root().to_path_buf();
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let result = match source {
            GrpcSchemaSource::Reflection => {
                ternarywire_grpc::load_via_reflection(&target).await.map_err(|e| e.to_string())
            }
            GrpcSchemaSource::ProtoFiles { files, import_paths } => {
                let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
                let imports: Vec<PathBuf> = import_paths.iter().map(PathBuf::from).collect();
                tokio::task::spawn_blocking(move || {
                    ternarywire_grpc::load_proto_files(&files, &imports, Some(&base)).map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            }
        };
        let _ = tx.send(Msg::Grpc { tab: tab_id, epoch: 0, event: GrpcMsg::Schema(result.map(Arc::new)) });
        ctx.request_repaint();
    });
}

/// Short label and tone of a method kind.
fn kind_look(kind: MethodKind) -> (&'static str, kit::Tone) {
    match kind {
        MethodKind::Unary => (tr("unary"), kit::Tone::Info),
        MethodKind::ServerStreaming => (tr("server stream"), kit::Tone::Ok),
        MethodKind::ClientStreaming => (tr("client stream"), kit::Tone::Warn),
        MethodKind::Bidi => (tr("bidi stream"), kit::Tone::Accent),
    }
}

/// `HelloRequest → stream HelloReply`: the messages of a method.
fn signature(m: &ternarywire_grpc::MethodInfo) -> String {
    let stream = |on: bool| if on { "stream " } else { "" };
    let server = matches!(m.kind, MethodKind::ServerStreaming | MethodKind::Bidi);
    format!(
        "{}{} → {}{}",
        stream(m.kind.is_client_streaming()),
        short(&m.input_type),
        stream(server),
        short(&m.output_type)
    )
}

/// What the method picker asks for.
enum Pick {
    Method(String, String),
    LoadSchema,
}

/// What the method picker shows.
struct Picker<'a> {
    loaded: bool,
    loading: bool,
    methods: &'a [ternarywire_grpc::ServiceInfo],
    search: &'a mut String,
    /// Where the schema comes from (`from server reflection`).
    source: &'a str,
    /// Open the list (automation).
    open: bool,
}

/// The method cell of the connect bar (`helloworld.Greeter / SayHello ▾`,
/// or Load schema), opening a searchable list of the services' methods with
/// their kinds.
fn method_picker(ui: &mut Ui, picker: Picker<'_>, service: &str, method: &str) -> Option<Pick> {
    let p = theme::pal(ui);
    let mut pick = None;
    let w = ui.available_width().max(120.0);
    let label = if picker.loading {
        tr("Loading schema…").to_string()
    } else if !picker.loaded {
        tr("Load schema").to_string()
    } else if method.is_empty() {
        tr("Select a method").to_string()
    } else {
        format!("{service} / {method}")
    };
    let resp = pc::bar_combo(ui, &label, w, |ui, rect| {
        let painter = ui.painter();
        let cy = rect.center().y;
        let mut x = rect.left();
        if picker.loading {
            let t = ui.input(|i| i.time) as f32;
            let c = egui::pos2(x + 7.0, cy);
            let pts: Vec<egui::Pos2> = (0..=14)
                .map(|i| {
                    let a = t * 6.0 + i as f32 / 14.0 * 4.5;
                    c + 5.0 * egui::vec2(a.cos(), a.sin())
                })
                .collect();
            painter.line(pts, egui::Stroke::new(1.6, p.accent));
            ui.ctx().request_repaint();
        } else {
            let icon = if picker.loaded { icons::TREE_STRUCTURE } else { icons::ARROWS_CLOCKWISE };
            painter.text(
                egui::pos2(x + 7.0, cy),
                egui::Align2::CENTER_CENTER,
                icon,
                theme::icon_font(14.0),
                p.text_faint,
            );
        }
        x += 14.0 + 7.0;
        if picker.loaded && !method.is_empty() {
            // `helloworld.Greeter /` gives way to the method name.
            let name = painter.layout_no_wrap(method.to_string(), theme::medium(12.5), p.text);
            let room = (rect.right() - x - name.size().x - 6.0).max(0.0);
            if room > 14.0 {
                let svc = kit::elide(ui, &format!("{service} /"), egui::FontId::proportional(12.5), p.text_muted, room);
                painter.galley(egui::pos2(x, cy - svc.size().y / 2.0), svc.clone(), p.text_muted);
                x += svc.size().x + 6.0;
            }
            let name = kit::elide(ui, method, theme::medium(12.5), p.text, rect.right() - x);
            painter.galley(egui::pos2(x, cy - name.size().y / 2.0), name, p.text);
        } else {
            let g = kit::elide(ui, &label, egui::FontId::proportional(12.5), p.text_muted, rect.right() - x);
            painter.galley(egui::pos2(x, cy - g.size().y / 2.0), g, p.text_muted);
        }
    });
    if !picker.loaded {
        if resp.clicked() && !picker.loading {
            pick = Some(Pick::LoadSchema);
        }
        resp.on_hover_text(tr("Server reflection or the .proto files in Schema"));
        return pick;
    }
    let resp = resp.on_hover_text(tr("Method"));
    let popup_id = egui::Popup::default_response_id(&resp);
    if picker.open {
        egui::Popup::open_id(ui.ctx(), popup_id);
    }
    let frame =
        egui::Frame::popup(ui.style()).corner_radius(egui::CornerRadius::same(12)).inner_margin(egui::Margin::same(6));
    let popup = egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(5.0)
        .frame(frame)
        .width(380.0);
    let search = picker.search;
    popup.show(|ui| {
        ui.set_min_width(368.0);
        ui.set_max_width(368.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::Frame::new().inner_margin(egui::Margin { left: 4, right: 4, top: 4, bottom: 8 }).show(ui, |ui| {
            let r = kit::search_field_sm(ui, "grpc-method-search", search, tr("Search methods"), ui.available_width());
            if !r.has_focus() && search.is_empty() {
                r.request_focus();
            }
        });
        let q = search.to_lowercase();
        egui::ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut any = false;
            for svc in picker.methods {
                let methods: Vec<_> = svc
                    .methods
                    .iter()
                    .filter(|m| {
                        q.is_empty() || m.name.to_lowercase().contains(&q) || svc.name.to_lowercase().contains(&q)
                    })
                    .collect();
                if methods.is_empty() {
                    continue;
                }
                any = true;
                let (head, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
                ui.painter().text(
                    egui::pos2(head.left() + 8.0, head.center().y),
                    egui::Align2::LEFT_CENTER,
                    icons::CUBE,
                    theme::icon_font(13.0),
                    p.text_faint,
                );
                ui.painter().text(
                    egui::pos2(head.left() + 8.0 + 13.0 + 6.0, head.center().y),
                    egui::Align2::LEFT_CENTER,
                    &svc.name,
                    theme::semibold(11.5),
                    p.text_muted,
                );
                for m in methods {
                    let selected = svc.name == service && m.name == method;
                    let (rect, r) =
                        ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
                    widgets::a11y_button(&r, &format!("{} {}", svc.name, m.name));
                    let painter = ui.painter();
                    if selected || r.hovered() {
                        painter.rect_filled(rect, egui::CornerRadius::same(7), p.hover);
                    }
                    if selected {
                        painter.rect_filled(
                            egui::Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height())),
                            egui::CornerRadius { nw: 7, sw: 7, ne: 0, se: 0 },
                            p.accent,
                        );
                    }
                    let (kind, tone) = kind_look(m.kind);
                    let (fg, bg) = p.tone(tone);
                    let kg = painter.layout_no_wrap(kind.to_string(), theme::medium(11.5), fg);
                    let pill = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 10.0 - kg.size().x - 14.0, rect.center().y - 10.0),
                        egui::vec2(kg.size().x + 14.0, 20.0),
                    );
                    painter.rect_filled(pill, egui::CornerRadius::same(10), bg);
                    painter.galley(pill.center() - kg.size() / 2.0, kg, fg);
                    let text_w = pill.left() - rect.left() - 20.0;
                    let ng = kit::elide(ui, &m.name, theme::medium(13.0), p.text, text_w);
                    let ig = kit::elide(ui, &signature(m), theme::mono(11.0), p.text_faint, text_w);
                    let top = rect.center().y - (ng.size().y + ig.size().y) / 2.0;
                    ui.painter().galley(egui::pos2(rect.left() + 10.0, top), ng.clone(), p.text);
                    ui.painter().galley(egui::pos2(rect.left() + 10.0, top + ng.size().y), ig, p.text_faint);
                    if r.clicked() {
                        pick = Some(Pick::Method(svc.name.clone(), m.name.clone()));
                        ui.close();
                    }
                }
            }
            if !any {
                ui.add_space(10.0);
                ui.label(RichText::new(tr("No methods match")).color(p.text_muted));
                ui.add_space(10.0);
            }
        });
        let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 7.0), egui::Sense::hover());
        ui.painter().hline(line.x_range(), line.bottom() - 0.5, egui::Stroke::new(1.0, p.border));
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 30.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(4.0);
                if kit::ghost(ui, Some(icons::ARROWS_CLOCKWISE), tr("Reload schema")).clicked() {
                    pick = Some(Pick::LoadSchema);
                    ui.close();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(picker.source).size(11.0).color(p.text_faint));
                });
            },
        );
    });
    pick
}
pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut GrpcTab) {
    let Some(original) = tab.slot.load(app) else {
        ui.label(tr("This request no longer exists."));
        return;
    };
    let mut item = original.clone();
    // The rows of the view are edge to edge.
    ui.spacing_mut().item_spacing.y = 0.0;
    let collection = tab.slot.collection_id();
    let vars = app.var_ctx(collection.as_deref());
    let inherited = pc::inherited_auth(app, &tab.slot, &item);
    let methods_n: usize = tab.methods.iter().map(|s| s.methods.len()).sum();
    let (schema_state, schema_text) = if tab.schema_loading {
        (Conn::Busy, tr("Loading schema…").to_string())
    } else if tab.schema.is_some() {
        (Conn::Live, trf("{} methods", &[&methods_n]))
    } else if tab.schema_error.is_some() {
        (Conn::Failed, tr("Schema error").to_string())
    } else {
        (Conn::Idle, tr("No schema").to_string())
    };
    let reflection = matches!(&item.request, RequestKind::Grpc(r) if matches!(r.schema, GrpcSchemaSource::Reflection));
    let source = if reflection { tr("server reflection") } else { tr(".proto files") };
    header(ui, app, tab_id, &tab.slot, &mut item, |ui| {
        let opts = kit::PillOpts { dot: true, live: false, ..Default::default() };
        let r = match schema_state {
            Conn::Live => kit::pill_ex(ui, &schema_text, kit::Tone::Ok, opts),
            Conn::Failed => kit::pill_ex(ui, &schema_text, kit::Tone::Err, opts),
            s => pc::conn_pill(ui, s, &schema_text, false),
        };
        r.on_hover_text(trf("Schema from {}", &[&source]));
    });
    let mut open_picker = false;
    match tab.auto.take().as_deref() {
        Some("picker") => open_picker = true,
        Some(a) => {
            if let Some(i) = a.strip_prefix("select:").and_then(|i| i.parse().ok()) {
                tab.log.selected = Some(i);
            } else if let Some(s) = a.strip_prefix("section:") {
                tab.section = match s {
                    "metadata" => Section::Metadata,
                    "auth" => Section::Auth,
                    "schema" => Section::Schema,
                    "scripts" => Section::Scripts,
                    "settings" => Section::Settings,
                    _ => Section::Message,
                };
            } else if let Some(s) = a.strip_prefix("resp:") {
                tab.resp_section = match s {
                    "headers" => RespSection::Headers,
                    "trailers" => RespSection::Trailers,
                    _ => RespSection::Messages,
                };
            }
        }
        None => {}
    }
    let mut do_load = false;
    let mut do_invoke = false;
    let mut stream_send = false;
    {
        let RequestKind::Grpc(req) = &mut item.request else { return };
        let method_kind = tab.schema.as_ref().and_then(|s| s.method(&req.service, &req.method)).map(|m| m.kind);
        // Waiting for the pre-invoke script counts as running (one call at a time).
        let running = tab.call.is_some() || tab.scripts.is_waiting();
        let ready = tab.schema.is_some() && !req.method.is_empty();
        let has_schema = tab.schema.is_some();
        let mut cancel = false;
        let mut invoke_clicked = false;
        let mut picked = None;
        let mut end_stream = false;
        let mut dismiss_error = false;
        let mut retry_load = false;
        let tests: Vec<_> = tab.scripts.tests.iter().filter(|t| !t.skipped).collect();
        let tests_run = (tests.iter().filter(|t| t.passed).count(), tests.len());
        let item_id = item.id.clone();
        let scripts = &mut item.scripts;
        let panes = ui.make_persistent_id("grpc-panes");
        let call = &tab.call;
        let schema = &tab.schema;
        let schema_loading = tab.schema_loading;
        let schema_error = &tab.schema_error;
        let methods = &tab.methods;
        let method_search = &mut tab.method_search;
        let section = &mut tab.section;
        let scripts_state = &mut tab.scripts;
        let log = &mut tab.log;
        let resp_section = &mut tab.resp_section;
        let status = &tab.status;
        let headers = &tab.headers;
        let trailers = &tab.trailers;
        let started = tab.started;
        let source_label = if reflection { tr("from server reflection") } else { tr("from .proto files") };
        pc::view_panes(
            ui,
            panes,
            |ui| {
                pc::connect_bar(
                    ui,
                    "gRPC",
                    112.0,
                    |ui| {
                        let url_w = (ui.available_width() * 0.3).clamp(150.0, 240.0);
                        let r = pc::bar_var(ui, "grpc-url", &mut req.url, &vars, "localhost:50051", url_w);
                        pc::bar_divider(ui);
                        let picker = Picker {
                            loaded: schema.is_some(),
                            loading: schema_loading,
                            methods,
                            search: &mut *method_search,
                            source: source_label,
                            open: open_picker,
                        };
                        let (service, method) = (req.service.clone(), req.method.clone());
                        picked = method_picker(ui, picker, &service, &method);
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if schema.is_none() {
                                do_load = true;
                            } else {
                                do_invoke = !running;
                            }
                        }
                        r.has_focus()
                    },
                    |ui| {
                        if running {
                            cancel = kit::action_button(ui, kit::Kind::Secondary, icons::STOP, tr("Cancel")).clicked();
                        } else {
                            let label = match method_kind {
                                Some(MethodKind::ClientStreaming) | Some(MethodKind::Bidi) => tr("Start stream"),
                                _ => tr("Invoke"),
                            };
                            let r = ui
                                .add_enabled_ui(ready, |ui| {
                                    kit::action_button(ui, kit::Kind::Primary, icons::PLAY, label)
                                })
                                .inner;
                            let hint = if has_schema { tr("Select a method") } else { tr("Load the schema first") };
                            if r.on_hover_text(format!("{label} (⇧⌘↩)")).on_disabled_hover_text(hint).clicked() {
                                invoke_clicked = true;
                            }
                        }
                    },
                );
                let metadata_n = req.metadata.iter().filter(|m| m.enabled).count();
                let tabs = [
                    Tab::new(Section::Message, tr("Message")),
                    Tab::new(Section::Metadata, tr("Metadata")).count(metadata_n),
                    Tab::new(Section::Auth, tr("Auth")).dot(pc::has_own_auth(&req.auth)),
                    Tab::new(Section::Schema, tr("Schema")),
                    Tab::new(Section::Scripts, tr("Scripts")).count(pc::scripts_count(scripts)),
                    Tab::new(Section::Settings, tr("Settings")),
                ];
                kit::tabs(ui, section, &tabs);
                match *section {
                    Section::Message => {
                        let p = theme::pal(ui);
                        let info = schema.as_ref().and_then(|s| s.method(&req.service, &req.method));
                        let template = info.as_ref().map(|m| m.input_template.clone());
                        pc::toolbar(
                            ui,
                            |ui| match &info {
                                Some(m) => {
                                    let (kind, tone) = kind_look(m.kind);
                                    kit::tone_pill(ui, kind, tone);
                                    let full = format!("{} → {}", m.input_type, m.output_type);
                                    ui.scope(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.label(
                                            RichText::new(short(&m.input_type)).font(theme::mono(12.0)).color(p.text),
                                        )
                                        .on_hover_text(&full);
                                        ui.label(
                                            RichText::new(icons::ARROW_RIGHT)
                                                .font(theme::icon_font(12.0))
                                                .color(p.text_faint),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(short(&m.output_type))
                                                    .font(theme::mono(12.0))
                                                    .color(p.text),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&full);
                                    });
                                }
                                None => {
                                    ui.add(
                                        egui::Label::new(kit::hint(
                                            ui,
                                            tr("Pick a method to get a template of its input message."),
                                        ))
                                        .truncate(),
                                    );
                                }
                            },
                            |ui| {
                                if let Some(c) = call {
                                    if c.can_send() && pc::toolbar_send(ui, tr("Send"), true, "") {
                                        stream_send = true;
                                    }
                                    if c.kind().is_client_streaming() {
                                        let opts = kit::ButtonOpts { icon: Some(icons::STOP), ..Default::default() };
                                        if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("End stream"), opts)
                                            .clicked()
                                        {
                                            end_stream = true;
                                        }
                                    }
                                } else if let Some(t) = &template
                                    && kit::ghost(ui, Some(icons::SPARKLE), tr("Use example"))
                                        .on_hover_text(tr("Fill with a template of the input message"))
                                        .clicked()
                                {
                                    req.message = t.clone();
                                }
                            },
                        );
                        let margin = egui::Margin { left: 12, right: 12, top: 0, bottom: 12 };
                        let r = pc::editor_well(
                            ui,
                            "grpc-msg",
                            &mut req.message,
                            Lang::Json,
                            Some(&vars),
                            margin,
                            None,
                            None,
                        );
                        pc::well_placeholder(ui, &r, &req.message, tr("The request message as JSON"));
                    }
                    Section::Metadata => {
                        pc::body(ui, "grpc-md-scroll", 0.0, 0.0, |ui| {
                            let mut md = req.metadata.clone();
                            let opts = KvOptions {
                                title: Some(tr("Metadata")),
                                allow_add: true,
                                show_description: true,
                                key_hint: tr("Key"),
                                value_hint: tr("Value"),
                                ..Default::default()
                            };
                            if widgets::kv_table(ui, "grpc-metadata", &mut md, &vars, opts) {
                                req.metadata = md;
                            }
                            ui.add_space(14.0);
                            ui.label(kit::hint(ui, tr("Keys ending in -bin take base64 values.")));
                        });
                    }
                    Section::Auth => pc::auth_pane(
                        ui,
                        "grpc-auth",
                        &mut req.auth,
                        &item_id,
                        inherited.clone(),
                        Some(tr("Bearer, Basic and API key (header) are sent as metadata.")),
                        &vars,
                    ),
                    Section::Schema => schema_section(ui, req, schema.is_some(), schema_loading, methods, &mut do_load),
                    Section::Scripts => pc::scripts_ui(
                        ui,
                        scripts,
                        &[
                            (ScriptEvent::PreRequest, "Before invoke"),
                            (ScriptEvent::OnMessage, "On message"),
                            (ScriptEvent::PostResponse, "After response"),
                        ],
                        scripts_state,
                        &vars,
                    ),
                    Section::Settings => {
                        pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("Connection"), None), |_| {});
                        pc::body(ui, "grpc-settings", 0.0, 14.0, |ui| {
                            kit::group(ui, "", "", |ui, rows| {
                                rows.row(ui, "TLS", "", |ui| {
                                    ui.push_id("tls", |ui| {
                                        kit::segmented_ex(
                                            ui,
                                            &mut req.tls,
                                            &[
                                                (None, None, tr("By scheme")),
                                                (Some(true), None, tr("On")),
                                                (Some(false), None, tr("Off")),
                                            ],
                                            true,
                                        )
                                    });
                                });
                                rows.row(ui, tr("Verify SSL certificates"), tr("When TLS is on"), |ui| {
                                    let mut v = req.verify_ssl.unwrap_or(true);
                                    if kit::check(ui, &mut v, tr("Verify SSL certificates")).changed() {
                                        req.verify_ssl = Some(v);
                                    }
                                });
                                rows.row(ui, tr("Deadline"), tr("ms, 0 = none"), |ui| {
                                    let mut d = req.deadline_ms.unwrap_or(0);
                                    if pc::number_input(ui, "grpc-deadline", &mut d, 90.0, tr("Deadline")).changed() {
                                        req.deadline_ms = (d > 0).then_some(d);
                                    }
                                });
                            });
                        });
                    }
                }
            },
            |ui| {
                let p = theme::pal(ui);
                let n = log.lines.iter().filter(|l| l.dir != Dir::System).count();
                let received = log.lines.iter().filter(|l| l.dir == Dir::In).count();
                pc::status_row(
                    ui,
                    |ui| {
                        if let Some(s) = started {
                            pc::status_pill(ui, Conn::Busy, tr("Calling…"));
                            let ms = s.elapsed().as_secs_f64() * 1000.0;
                            pc::metrics(ui, |ui| {
                                kit::metric(ui, icons::TIMER, &ternarywire_core::util::human_ms(ms));
                                if received > 0 {
                                    kit::metric(ui, icons::ARROW_DOWN, &messages_text(received));
                                }
                            });
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                        } else if let Some((code, name, message, elapsed)) = status {
                            let tone = if *code == 0 { kit::Tone::Ok } else { kit::Tone::Err };
                            let opts = kit::PillOpts { large: true, ..Default::default() };
                            let tip = if message.is_empty() { tr("(empty)") } else { message.as_str() };
                            kit::pill_ex(ui, &format!("{code} {name}"), tone, opts)
                                .on_hover_text(trf("Status message: {}", &[&tip]));
                            let (passed, total) = tests_run;
                            pc::metrics(ui, |ui| {
                                kit::metric(ui, icons::TIMER, &ternarywire_core::util::human_ms(*elapsed));
                                kit::metric(ui, icons::ARROW_DOWN, &messages_text(received));
                                if total > 0 {
                                    let color = (passed < total).then_some(p.err);
                                    kit::metric_colored(
                                        ui,
                                        icons::CHECK_CIRCLE,
                                        &trf("{}/{} tests", &[&passed, &total]),
                                        color,
                                    );
                                }
                            });
                        } else {
                            pc::status_pill(ui, Conn::Idle, tr("No response yet"));
                        }
                    },
                    |ui| pc::pane_actions(ui, panes),
                );
                let tabs = [
                    Tab::new(RespSection::Messages, tr("Messages")).count(n),
                    Tab::new(RespSection::Headers, tr("Headers")).count(headers.len()),
                    Tab::new(RespSection::Trailers, tr("Trailers")).count(trailers.len()),
                ];
                let messages = *resp_section == RespSection::Messages;
                kit::tabs_with(ui, resp_section, &tabs, |ui| {
                    if messages {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        // The search gives way to the tabs.
                        let w = (ui.available_width() - 320.0 - 34.0).clamp(90.0, 160.0);
                        log.tools(ui, ("grpc-log", tab_id), false, w);
                    }
                });
                if let Some(e) = schema_error {
                    let mut retry = false;
                    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 0 }).show(
                        ui,
                        |ui| {
                            pc::error_banner(ui, tr("Could not load the schema"), e, |ui| {
                                dismiss_error = kit::icon_button_sm(ui, icons::X, tr("Dismiss")).clicked();
                                let opts = kit::ButtonOpts { icon: Some(icons::ARROW_CLOCKWISE), ..Default::default() };
                                retry =
                                    kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Retry"), opts).clicked();
                            });
                        },
                    );
                    if retry {
                        retry_load = true;
                    }
                }
                match *resp_section {
                    RespSection::Messages => log.body(ui, ("grpc-log", tab_id)),
                    RespSection::Headers => pc::body(ui, "grpc-headers", 12.0, 0.0, |ui| {
                        pc::kv_table_view(
                            ui,
                            headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                            tr("No response headers yet."),
                        )
                    }),
                    RespSection::Trailers => pc::body(ui, "grpc-trailers", 12.0, 0.0, |ui| {
                        pc::kv_table_view(
                            ui,
                            trailers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                            tr("No trailers yet."),
                        )
                    }),
                }
            },
        );
        do_invoke |= invoke_clicked;
        do_load |= retry_load;
        if dismiss_error {
            tab.schema_error = None;
        }
        match picked {
            Some(Pick::Method(s, m)) => {
                if let Some(info) = tab.schema.as_ref().and_then(|sc| sc.method(&s, &m))
                    && (req.message.trim().is_empty() || req.method != m)
                {
                    req.message = info.input_template.clone();
                }
                req.service = s;
                req.method = m;
                tab.method_search.clear();
            }
            Some(Pick::LoadSchema) => do_load = true,
            None => {}
        }
        if cancel {
            tab.scripts.stop_waiting();
            if let Some(call) = &tab.call {
                call.cancel();
            }
        }
        if pc::send_shortcut(ui) {
            match &tab.call {
                Some(call) if call.can_send() => stream_send = true,
                None if tab.schema.is_some() && !req.method.is_empty() => do_invoke = true,
                _ => {}
            }
        }
        if end_stream && let Some(c) = &tab.call {
            c.end_stream();
        }
        if stream_send && let Some(c) = &tab.call {
            let r = resolver(app, collection.as_deref());
            if let Err(e) = c.send(&r(&req.message)) {
                tab.log.error(e.to_string());
            }
        }
    }
    do_load |= std::mem::take(&mut tab.auto_load);
    do_invoke |= std::mem::take(&mut tab.auto_invoke) && tab.schema.is_some();
    if do_load {
        load_schema(app, tab_id, tab, &item);
    }
    if do_invoke && tab.schema.is_some() && tab.call.is_none() && !tab.scripts.is_waiting() {
        tab.scripts.clear_results();
        let pre = match &item.request {
            RequestKind::Grpc(r) => {
                crate::proto_common::run_scripts(app, tab_id, &tab.slot, &item, ScriptEvent::PreRequest, proto::grpc(r))
            }
            _ => None,
        };
        if let Some(run) = pre {
            tab.scripts.wait_for(run);
            tab.log.system(tr("Running the pre-invoke script…"));
        } else {
            invoke(app, tab_id, tab, &item);
        }
    }
    // The pre-invoke script finished: send what it left in pm.request.
    if let Some(pre) = tab.scripts.take_pre() {
        match pre {
            None => tab.log.system(tr("Skipped by pm.execution.skipRequest()")),
            Some(v) => {
                let mut sent = item.clone();
                if let RequestKind::Grpc(r) = &mut sent.request {
                    proto::apply_grpc(r, &v);
                }
                invoke(app, tab_id, tab, &sent);
            }
        }
    }
    if let RequestKind::Grpc(r) = &item.request {
        for q in std::mem::take(&mut tab.queue) {
            let (event, ctx) = match q {
                Queued::Message(m) => (ScriptEvent::OnMessage, proto::grpc(r).with_message(m)),
                Queued::Response(v) => (ScriptEvent::PostResponse, proto::grpc(r).with_response(v)),
            };
            crate::proto_common::run_scripts(app, tab_id, &tab.slot, &item, event, ctx);
        }
    }
    tab.slot.store(app, &original, item);
}

/// `1 message`, `3 messages`.
fn messages_text(n: usize) -> String {
    if n == 1 { tr("1 message").to_string() } else { trf("{} messages", &[&n]) }
}

/// Where the schema comes from: server reflection or `.proto` files, and
/// the services it has.
fn schema_section(
    ui: &mut Ui,
    req: &mut ternarywire_core::protocols::GrpcRequest,
    loaded: bool,
    loading: bool,
    methods: &[ternarywire_grpc::ServiceInfo],
    do_load: &mut bool,
) {
    let p = theme::pal(ui);
    let mut reflection = matches!(req.schema, GrpcSchemaSource::Reflection);
    pc::toolbar(
        ui,
        |ui| {
            let items = [(true, None, tr("Server reflection")), (false, None, tr(".proto files"))];
            if kit::segmented_ex(ui, &mut reflection, &items, true) {
                req.schema = if reflection {
                    GrpcSchemaSource::Reflection
                } else {
                    GrpcSchemaSource::ProtoFiles { files: Vec::new(), import_paths: Vec::new() }
                };
            }
        },
        |ui| {
            if loading {
                ui.add(egui::Spinner::new().size(16.0).color(p.accent));
            } else {
                let label = if loaded { tr("Reload schema") } else { tr("Load schema") };
                let opts = kit::ButtonOpts { icon: Some(icons::ARROWS_CLOCKWISE), ..Default::default() };
                if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, label, opts).clicked() {
                    *do_load = true;
                }
            }
        },
    );
    pc::body(ui, "grpc-schema", 0.0, 10.0, |ui| {
        match &mut req.schema {
            GrpcSchemaSource::Reflection => {
                ui.label(kit::hint(ui, tr("The server lists its services itself (grpc.reflection v1 or v1alpha).")));
            }
            GrpcSchemaSource::ProtoFiles { files, import_paths } => {
                let mut remove = None;
                let mut remove_import = None;
                let n = files.len() + import_paths.len();
                if n == 0 {
                    ui.label(kit::hint(ui, tr("No .proto files yet.")));
                } else {
                    pc::list_box(ui, n, 40.0, |ui, i| {
                        let (icon, path, import) = if i < files.len() {
                            (icons::FILE, files[i].as_str(), false)
                        } else {
                            (icons::FOLDER_SIMPLE, import_paths[i - files.len()].as_str(), true)
                        };
                        ui.label(RichText::new(icon).font(theme::icon_font(15.0)).color(p.text_faint));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(8.0);
                            let tip = if import { tr("Remove import path") } else { tr("Remove file") };
                            if kit::icon_button_sm(ui, icons::X, tip).clicked() {
                                if import {
                                    remove_import = Some(i - files.len());
                                } else {
                                    remove = Some(i);
                                }
                            }
                            if import {
                                kit::tone_pill(ui, tr("import path"), kit::Tone::Neutral);
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(path).font(theme::mono(12.0)).color(p.text))
                                        .truncate(),
                                )
                                .on_hover_text(path);
                            });
                        });
                    });
                }
                if let Some(i) = remove {
                    files.remove(i);
                }
                if let Some(i) = remove_import {
                    import_paths.remove(i);
                }
                ui.horizontal(|ui| {
                    if kit::ghost(ui, Some(icons::FILE_PLUS), tr("Add .proto files")).clicked()
                        && let Some(paths) =
                            rfd::FileDialog::new().add_filter("Protocol Buffers", &["proto"]).pick_files()
                    {
                        files.extend(paths.into_iter().map(|p| p.display().to_string()));
                    }
                    if kit::ghost(ui, Some(icons::FOLDER_PLUS), tr("Add import path")).clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        import_paths.push(dir.display().to_string());
                    }
                });
            }
        }
        if !methods.is_empty() {
            ui.label(kit::section_title(tr("Services")));
            pc::list_box(ui, methods.len(), 40.0, |ui, i| {
                let svc = &methods[i];
                ui.label(RichText::new(icons::CUBE).font(theme::icon_font(15.0)).color(p.text_faint));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new(trf("{} methods", &[&svc.methods.len()])).size(11.0).color(p.text_faint));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&svc.name).font(theme::mono(12.0)).color(p.text)).truncate(),
                        );
                    });
                });
            });
        }
    });
}

fn invoke(app: &mut TernaryWireApp, tab_id: u64, tab: &mut GrpcTab, item: &ternarywire_core::model::RequestItem) {
    let (Some(schema), Some(target)) = (tab.schema.clone(), target_for(app, tab, item)) else { return };
    let RequestKind::Grpc(req) = &item.request else { return };
    let collection = tab.slot.collection_id();
    let r = resolver(app, collection.as_deref());
    tab.log.clear();
    tab.status = None;
    tab.headers.clear();
    tab.trailers.clear();
    tab.received.clear();
    tab.msg_types = tab
        .schema
        .as_ref()
        .and_then(|s| s.method(&req.service, &req.method))
        .map(|m| (short(&m.input_type).to_string(), short(&m.output_type).to_string()))
        .unwrap_or_default();
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    let first = Some(r(&req.message)).filter(|m| !m.trim().is_empty());
    tab.epoch = crate::app::next_epoch();
    let epoch = tab.epoch;
    match GrpcCall::start(app.rt.handle(), target, schema, &req.service, &req.method, first, move |event| {
        let _ = tx.send(Msg::Grpc { tab: tab_id, epoch, event: GrpcMsg::Event(event) });
        ctx.request_repaint();
    }) {
        Ok(call) => {
            tab.call = Some(call);
            tab.started = Some(Instant::now());
            tab.resp_section = RespSection::Messages;
        }
        Err(e) => tab.log.error(e.to_string()),
    }
}

fn short(service: &str) -> &str {
    service.rsplit('.').next().unwrap_or(service)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long server stream (or one in a tab in the background, whose script
    /// queue is not drained) keeps the latest messages, like the other
    /// protocol tabs; the response still reaches the post-response script.
    #[test]
    fn long_streams_are_bounded() {
        let mut tab = GrpcTab::new(ItemSlot::default());
        for i in 0..pc::MAX_INBOX + 100 {
            let json = format!("{{\"n\": {i}}}");
            tab.on_msg(GrpcMsg::Event(GrpcEvent::Message { direction: Direction::Received, json, at_ms: 0 }));
        }
        assert_eq!(tab.received.len(), pc::MAX_INBOX);
        assert_eq!(tab.queue.len(), pc::MAX_INBOX);
        let finished = GrpcEvent::Finished { code: 0, code_name: "OK".into(), message: String::new(), elapsed_ms: 1.0 };
        tab.on_msg(GrpcMsg::Event(finished));
        assert!(tab.received.is_empty());
        assert!(matches!(tab.queue.back(), Some(Queued::Response(_))));
        let oldest = proto::message(proto::json_or_text("{\"n\": 100}"), proto::Direction::Incoming, 0);
        assert!(matches!(tab.queue.front(), Some(Queued::Message(m)) if *m == oldest), "the oldest are dropped");
    }
}
