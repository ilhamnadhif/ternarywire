//! MCP request tab (FR-X8): connect to a local (stdio) or remote (Streamable
//! HTTP) MCP server, browse tools/resources/prompts, call them, watch the
//! JSON-RPC traffic and export the server config for other MCP hosts.

use std::collections::VecDeque;
use std::time::Instant;

use egui::{RichText, Ui};

use ternarywire_core::model::{KeyValue, RequestKind};
use ternarywire_core::protocols::{McpCallKind, McpSavedCall, McpTransport};
use ternarywire_mcp::{
    Direction, ExportStyle, McpClient, McpConfig, McpEvent, Prompt, Resource, ResourceTemplate, ServerInfo, Tool,
};

use crate::app::{Msg, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tab;
use crate::proto_common::{self as pc, Conn, ConnAction, Dir, ItemSlot, Log, ScriptState, header, resolver};
use crate::widgets::{self, KvOptions};
use crate::{icons, kit, theme};
use ternarywire_core::script::ScriptEvent;
use ternarywire_core::script::protocol as proto;

pub struct Catalog {
    pub info: ServerInfo,
    pub tools: Vec<Tool>,
    pub resources: Vec<Resource>,
    pub templates: Vec<ResourceTemplate>,
    pub prompts: Vec<Prompt>,
}

pub enum McpMsg {
    Connected(Result<(McpClient, Box<Catalog>), String>),
    Event(McpEvent),
    Result {
        title: String,
        text: String,
        is_error: bool,
        raw: serde_json::Value,
        ms: f64,
    },
    /// The server asks the user's model for a completion (needs approval).
    Sampling(Box<ternarywire_mcp::SamplingRequest>, SamplingReply),
    /// The server asks the user for input.
    Elicit(Box<ternarywire_mcp::ElicitationRequest>, ElicitReply),
    /// The approved sampling request `id` was answered (or failed).
    SamplingDone(u64, Result<String, String>),
}

pub type SamplingReply =
    tokio::sync::oneshot::Sender<Result<ternarywire_mcp::SamplingResult, ternarywire_mcp::McpError>>;
pub type ElicitReply = tokio::sync::oneshot::Sender<ternarywire_mcp::ElicitationResponse>;

struct PendingSampling {
    id: u64,
    request: Box<ternarywire_mcp::SamplingRequest>,
    reply: Option<SamplingReply>,
    /// Saved AI request used to answer.
    model: Option<String>,
    running: bool,
}

struct PendingElicit {
    request: Box<ternarywire_mcp::ElicitationRequest>,
    reply: Option<ElicitReply>,
    values: serde_json::Map<String, serde_json::Value>,
    problems: Vec<String>,
}

/// OAuth tokens and registered clients of MCP servers, in the workspace's
/// secret store (Keychain).
struct SecretOAuth(std::sync::Arc<ternarywire_core::secrets::SecretStore>);

impl ternarywire_mcp::OAuthStorage for SecretOAuth {
    fn load_tokens(&self, server: &str) -> Option<ternarywire_mcp::OAuthTokens> {
        self.0.get(&format!("mcp-oauth:tokens:{server}")).and_then(|s| serde_json::from_str(&s).ok())
    }

    fn save_tokens(&self, server: &str, tokens: Option<&ternarywire_mcp::OAuthTokens>) {
        let key = format!("mcp-oauth:tokens:{server}");
        match tokens.and_then(|t| serde_json::to_string(t).ok()) {
            Some(json) => self.0.set(&key, &json),
            None => self.0.remove(&key),
        }
    }

    fn load_client(&self, issuer: &str) -> Option<ternarywire_mcp::RegisteredClient> {
        self.0.get(&format!("mcp-oauth:client:{issuer}")).and_then(|s| serde_json::from_str(&s).ok())
    }

    fn save_client(&self, issuer: &str, client: Option<&ternarywire_mcp::RegisteredClient>) {
        let key = format!("mcp-oauth:client:{issuer}");
        match client.and_then(|c| serde_json::to_string(c).ok()) {
            Some(json) => self.0.set(&key, &json),
            None => self.0.remove(&key),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Tools,
    Resources,
    Prompts,
    Saved,
    Scripts,
    Connection,
}

/// What the traffic pane shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResultView {
    Result,
    Traffic,
}

/// The result of the last call.
struct CallResult {
    title: String,
    text: String,
    is_error: bool,
    ms: Option<f64>,
    /// The result as the server sent it (`content`, `structuredContent`).
    raw: serde_json::Value,
}

pub struct McpTab {
    /// Number of the current connection (see `app::next_epoch`).
    pub epoch: u64,
    pub slot: ItemSlot,
    client: Option<McpClient>,
    connecting: bool,
    catalog: Option<Box<Catalog>>,
    view: View,
    result_view: ResultView,
    selected: Option<(McpCallKind, String)>,
    args: String,
    result: Option<CallResult>,
    busy: bool,
    log: Log,
    export_style: ExportStyle,
    /// Requests from the server waiting for the user, answered oldest first.
    /// Dropping one (closing the tab, a closed connection) rejects it.
    sampling: VecDeque<PendingSampling>,
    elicit: VecDeque<PendingElicit>,
    next_sampling: u64,
    login: Option<tokio_util::sync::CancellationToken>,
    pub scripts: ScriptState,
    /// A call waiting for its pre-request script, then its post-response input.
    pending_call: Option<(McpCallKind, String, serde_json::Value)>,
    post: Option<(McpCallKind, String, serde_json::Value, serde_json::Value)>,
    /// Automation: `connect` or `call:<tool>:<json args>`.
    pub auto: Option<String>,
    /// Filter of the tools/resources/prompts list.
    search: String,
    since: Option<Instant>,
    /// Why the last connection failed.
    error: Option<String>,
    /// The STDIO command line as typed, and the command it was read as
    /// (see [`command_line`]).
    typed_command: Option<(String, String)>,
}

impl McpTab {
    pub fn new(slot: ItemSlot) -> Self {
        let mut log =
            Log::with_empty("No traffic yet", "Connect to see the JSON-RPC messages exchanged with the server.");
        log.title = "";
        McpTab {
            epoch: 0,
            slot,
            client: None,
            connecting: false,
            catalog: None,
            view: View::Connection,
            result_view: ResultView::Result,
            selected: None,
            args: "{}".into(),
            result: None,
            busy: false,
            log,
            export_style: ExportStyle::default(),
            sampling: VecDeque::new(),
            elicit: VecDeque::new(),
            next_sampling: 0,
            login: None,
            scripts: ScriptState::default(),
            pending_call: None,
            post: None,
            auto: None,
            search: String::new(),
            since: None,
            error: None,
            typed_command: None,
        }
    }

    /// Cancel while connecting: ends the OAuth login and the handshake (a
    /// server that never answers would keep the tab connecting).
    fn cancel_connect(&mut self) {
        if let Some(l) = self.login.take() {
            l.cancel();
        }
        if self.connecting {
            self.connecting = false;
            self.epoch = crate::app::next_epoch();
            self.log.system(tr("Cancelled"));
        }
    }

    fn state(&self) -> Conn {
        if self.client.is_some() {
            Conn::Live
        } else if self.connecting {
            Conn::Busy
        } else if self.error.is_some() {
            Conn::Failed
        } else {
            Conn::Idle
        }
    }

    pub fn on_msg(&mut self, m: McpMsg) {
        match m {
            McpMsg::Connected(Ok((client, catalog))) => {
                self.connecting = false;
                self.error = None;
                self.since = Some(Instant::now());
                let info = &catalog.info;
                self.log.system(trf(
                    "Connected to {} {} (protocol {})",
                    &[&info.name, &info.version, &info.protocol_version],
                ));
                self.client = Some(client);
                if self.view == View::Connection {
                    self.view = View::Tools;
                }
                self.catalog = Some(catalog);
            }
            McpMsg::Connected(Err(e)) => {
                self.connecting = false;
                self.error = Some(e.clone());
                self.log.error(e);
            }
            McpMsg::Event(e) => match e {
                McpEvent::Log { level, message } => {
                    self.log.push(Dir::In, trf("log/{}", &[&level]), message, crate::proto_common::now_ms())
                }
                McpEvent::Stderr(line) => self.log.push(Dir::System, "stderr", line, crate::proto_common::now_ms()),
                McpEvent::Notification { method, params } => {
                    self.log.push(Dir::In, method, params.to_string(), crate::proto_common::now_ms())
                }
                McpEvent::Traffic { direction, json, at_ms } => {
                    let dir = if direction == Direction::Incoming { Dir::In } else { Dir::Out };
                    let title = serde_json::from_str::<serde_json::Value>(&json)
                        .ok()
                        .map(|v| traffic_title(&v))
                        .unwrap_or_default();
                    self.log.push(dir, title, json, at_ms);
                }
                McpEvent::Closed(reason) => {
                    self.log.system(trf("Closed: {}", &[&reason]));
                    self.client = None;
                    self.catalog = None;
                    self.since = None;
                    self.drop_pending();
                }
            },
            McpMsg::Result { title, text, is_error, raw, ms } => {
                self.busy = false;
                if let Some((kind, target, args)) = self.pending_call.take() {
                    self.post = Some((kind, target, args, proto::mcp_response(raw.clone(), is_error, ms)));
                }
                self.result = Some(CallResult { title, text, is_error, ms: Some(ms), raw });
            }
            McpMsg::Sampling(request, reply) => {
                self.log.system(trf("{} asks to use your model", &[&request.server]));
                self.next_sampling += 1;
                let id = self.next_sampling;
                self.sampling.push_back(PendingSampling {
                    id,
                    request,
                    reply: Some(reply),
                    model: None,
                    running: false,
                });
            }
            McpMsg::Elicit(request, reply) => {
                let values = request
                    .fields()
                    .into_iter()
                    .filter_map(|f| f.default.clone().map(|d| (f.name.clone(), d)))
                    .collect();
                self.elicit.push_back(PendingElicit { request, reply: Some(reply), values, problems: Vec::new() });
            }
            McpMsg::SamplingDone(id, r) => {
                match &r {
                    Ok(_) => self.log.system(tr("Sampling answered")),
                    Err(e) => self.log.error(trf("Sampling failed: {}", &[&e])),
                }
                self.sampling.retain(|p| p.id != id);
            }
        }
    }

    /// Declines the sampling request shown (the oldest waiting).
    fn decline_sampling(&mut self) {
        if let Some(mut p) = self.sampling.pop_front()
            && let Some(r) = p.reply.take()
        {
            let _ = r.send(Err(ternarywire_mcp::McpError::rejected("The user declined the request")));
        }
    }

    /// Answers the elicitation shown (the oldest waiting).
    fn answer_elicit(&mut self, answer: ternarywire_mcp::ElicitationResponse) {
        if let Some(mut p) = self.elicit.pop_front()
            && let Some(r) = p.reply.take()
        {
            let _ = r.send(answer);
        }
    }

    /// Forgets the requests of a connection that ended; their (dropped)
    /// replies reject them.
    fn drop_pending(&mut self) {
        self.sampling.clear();
        self.elicit.clear();
    }
}

/// The log title of a JSON-RPC message: its method, or which request a
/// response answers (`result #3`, `error #3`).
fn traffic_title(v: &serde_json::Value) -> String {
    if let Some(m) = v.get("method").and_then(|m| m.as_str()) {
        return m.to_string();
    }
    let id = v.get("id").map(|i| i.to_string()).unwrap_or_default();
    if v.get("error").is_some() {
        format!("error #{id}")
    } else if v.get("result").is_some() {
        format!("result #{id}")
    } else {
        String::new()
    }
}

fn connect(
    app: &mut TernaryWireApp,
    tab_id: u64,
    tab: &mut McpTab,
    item_id: &str,
    req: &ternarywire_core::protocols::McpRequest,
    collection: Option<&str>,
) {
    let r = resolver(app, collection);
    let cfg = McpConfig::from_request(&with_inherited_auth(app, collection, item_id, req), &r);
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    tab.connecting = true;
    tab.error = None;
    tab.drop_pending();
    tab.log.system(trf("Connecting to {}…", &[&connecting_target(req)]));
    let login = tokio_util::sync::CancellationToken::new();
    tab.login = Some(login.clone());
    let cancel = login.clone();
    let http = app.http.default_client().ok();
    let storage: std::sync::Arc<dyn ternarywire_mcp::OAuthStorage> =
        std::sync::Arc::new(SecretOAuth(app.ws.secrets.clone()));
    let roots: Vec<ternarywire_mcp::Root> = ternarywire_mcp::Root::from_path(app.ws.root()).into_iter().collect();
    tab.epoch = crate::app::next_epoch();
    let epoch = tab.epoch;
    app.rt.spawn(async move {
        let tx_events = tx.clone();
        let ctx_events = ctx.clone();
        let (tx_s, ctx_s, tx_e, ctx_e) = (tx.clone(), ctx.clone(), tx.clone(), ctx.clone());
        let result = async {
            let mut builder = McpClient::builder(cfg)
                .on_event(move |e| {
                    let _ = tx_events.send(Msg::Mcp { tab: tab_id, epoch, event: McpMsg::Event(e) });
                    ctx_events.request_repaint();
                })
                .oauth(
                    ternarywire_mcp::OAuthOptions::new(|url| {
                        crate::widgets::open_web_link(url);
                    })
                    .with_storage(storage)
                    .with_cancel(login),
                )
                .sampling(move |req: ternarywire_mcp::SamplingRequest| {
                    let (reply, wait) = tokio::sync::oneshot::channel();
                    let _ = tx_s.send(Msg::Mcp { tab: tab_id, epoch, event: McpMsg::Sampling(Box::new(req), reply) });
                    ctx_s.request_repaint();
                    async move {
                        wait.await
                            .unwrap_or_else(|_| Err(ternarywire_mcp::McpError::rejected("The request was dismissed")))
                    }
                })
                .elicitation(move |req: ternarywire_mcp::ElicitationRequest| {
                    let (reply, wait) = tokio::sync::oneshot::channel();
                    let _ = tx_e.send(Msg::Mcp { tab: tab_id, epoch, event: McpMsg::Elicit(Box::new(req), reply) });
                    ctx_e.request_repaint();
                    async move { Ok(wait.await.unwrap_or(ternarywire_mcp::ElicitationResponse::Cancel)) }
                })
                .roots(roots);
            if let Some(h) = http {
                builder = builder.http_client(h);
            }
            let client = builder.connect().await.map_err(|e| e.to_string())?;
            let info = client.server_info().clone();
            let tools =
                if info.has_capability("tools") { client.list_tools().await.unwrap_or_default() } else { Vec::new() };
            let (resources, templates) = if info.has_capability("resources") {
                (
                    client.list_resources().await.unwrap_or_default(),
                    client.list_resource_templates().await.unwrap_or_default(),
                )
            } else {
                (Vec::new(), Vec::new())
            };
            let prompts = if info.has_capability("prompts") {
                client.list_prompts().await.unwrap_or_default()
            } else {
                Vec::new()
            };
            Ok::<_, String>((client, Box::new(Catalog { info, tools, resources, templates, prompts })))
        };
        let result = tokio::select! {
            r = result => r,
            // Cancel: the half-open connection (a started server) is dropped.
            _ = cancel.cancelled() => return,
        };
        let _ = tx.send(Msg::Mcp { tab: tab_id, epoch, event: McpMsg::Connected(result) });
        ctx.request_repaint();
    });
}

fn run_call(
    app: &TernaryWireApp,
    tab_id: u64,
    client: McpClient,
    kind: McpCallKind,
    target: String,
    args: serde_json::Value,
) {
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let started = std::time::Instant::now();
        let mut raw = serde_json::Value::Null;
        let (title, text, is_error) = match kind {
            McpCallKind::Tool => match client.call_tool(&target, args).await {
                Ok(r) => {
                    raw = r.raw.clone();
                    let mut text = r.text();
                    if let Some(s) = &r.structured {
                        text.push_str("\n\nstructuredContent:\n");
                        text.push_str(&serde_json::to_string_pretty(s).unwrap_or_default());
                    }
                    if text.trim().is_empty() {
                        text = serde_json::to_string_pretty(&r.raw).unwrap_or_default();
                    }
                    (format!("tools/call {target}"), text, r.is_error)
                }
                Err(e) => (format!("tools/call {target}"), e.to_string(), true),
            },
            McpCallKind::Resource => match client.read_resource(&target).await {
                Ok(v) => {
                    raw = v.clone();
                    (format!("resources/read {target}"), serde_json::to_string_pretty(&v).unwrap_or_default(), false)
                }
                Err(e) => (format!("resources/read {target}"), e.to_string(), true),
            },
            McpCallKind::Prompt => {
                let map = args.as_object().cloned().unwrap_or_default();
                match client.get_prompt(&target, map).await {
                    Ok(v) => {
                        raw = v.clone();
                        (format!("prompts/get {target}"), serde_json::to_string_pretty(&v).unwrap_or_default(), false)
                    }
                    Err(e) => (format!("prompts/get {target}"), e.to_string(), true),
                }
            }
        };
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let _ = tx.send(Msg::Mcp { tab: tab_id, epoch: 0, event: McpMsg::Result { title, text, is_error, raw, ms } });
        ctx.request_repaint();
    });
}

/// One entry of the tools/resources/prompts list.
struct Entry {
    kind: McpCallKind,
    /// Tool or prompt name, or resource URI (template).
    target: String,
    name: String,
    detail: Option<String>,
    tag: Option<String>,
}

fn entries(c: &Catalog, view: View) -> Vec<Entry> {
    match view {
        View::Tools => c
            .tools
            .iter()
            .map(|t| Entry {
                kind: McpCallKind::Tool,
                target: t.name.clone(),
                name: t.display_name().to_string(),
                detail: t.description.clone(),
                tag: t.title.as_ref().map(|_| t.name.clone()),
            })
            .collect(),
        View::Resources => c
            .resources
            .iter()
            .map(|r| Entry {
                kind: McpCallKind::Resource,
                target: r.uri.clone(),
                name: r.title.clone().unwrap_or_else(|| r.name.clone()),
                detail: Some(r.description.clone().unwrap_or_else(|| r.uri.clone())),
                tag: r.mime_type.clone(),
            })
            .chain(c.templates.iter().map(|t| Entry {
                kind: McpCallKind::Resource,
                target: t.uri_template.clone(),
                name: t.title.clone().unwrap_or_else(|| t.name.clone()),
                detail: Some(t.description.clone().unwrap_or_else(|| t.uri_template.clone())),
                tag: Some(tr("template").to_string()),
            }))
            .collect(),
        View::Prompts => c
            .prompts
            .iter()
            .map(|p| Entry {
                kind: McpCallKind::Prompt,
                target: p.name.clone(),
                name: p.title.clone().unwrap_or_else(|| p.name.clone()),
                detail: p.description.clone(),
                tag: (!p.arguments.is_empty()).then(|| trf("{} args", &[&p.arguments.len()])),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A card of the catalog list (`.tcard`): name, a monospace tag and two
/// lines of description.
fn entry_card(ui: &mut Ui, e: &Entry, selected: bool) -> egui::Response {
    let p = theme::pal(ui);
    let w = ui.available_width();
    let inner_w = (w - 20.0).max(20.0);
    let name = kit::elide(ui, &e.name, theme::medium(12.5), p.text, inner_w);
    let tag = e.tag.as_ref().map(|t| kit::elide(ui, t, theme::mono(10.5), p.text_faint, inner_w));
    let detail = e.detail.as_ref().map(|d| {
        let mut job = egui::text::LayoutJob::single_section(
            d.replace('\n', " "),
            egui::TextFormat {
                line_height: Some(16.0),
                ..egui::TextFormat::simple(egui::FontId::proportional(11.5), p.text_muted)
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: inner_w,
            max_rows: 2,
            break_anywhere: false,
            overflow_character: Some('…'),
        };
        ui.painter().layout_job(job)
    });
    let h = 16.0
        + name.size().y
        + tag.as_ref().map_or(0.0, |t| t.size().y + 3.0)
        + detail.as_ref().map_or(0.0, |d| d.size().y + 3.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    widgets::a11y_button(&resp, &e.name);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = egui::CornerRadius::same(8);
        if selected {
            painter.rect_stroke(
                rect,
                r,
                egui::Stroke::new(3.0, theme::soft(p.accent, 0.12)),
                egui::StrokeKind::Outside,
            );
            painter.rect(rect, r, p.surface, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            painter.rect_filled(rect, r, p.hover);
        }
        let mut y = rect.top() + 8.0;
        painter.galley(egui::pos2(rect.left() + 10.0, y), name.clone(), p.text);
        y += name.size().y + 3.0;
        if let Some(t) = tag {
            painter.galley(egui::pos2(rect.left() + 10.0, y), t.clone(), p.text_faint);
            y += t.size().y + 3.0;
        }
        if let Some(d) = detail {
            painter.galley(egui::pos2(rect.left() + 10.0, y), d, p.text_muted);
        }
    }
    resp
}

/// What the compose pane of the MCP view works on.
struct Browser<'a> {
    view: View,
    catalog: Option<&'a Catalog>,
    selected: &'a mut Option<(McpCallKind, String)>,
    args: &'a mut String,
    search: &'a mut String,
    connected: bool,
    busy: bool,
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut McpTab) {
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
    // Where an inherited auth comes from (applied when connecting).
    let inherited = pc::inherited_auth(app, &tab.slot, &item);
    let state = tab.state();
    let since = tab.since;
    let server = tab.catalog.as_ref().map(|c| {
        (
            format!("{} {}", c.info.title.clone().unwrap_or(c.info.name.clone()), c.info.version),
            c.info.protocol_version.clone(),
        )
    });
    // Saved AI requests that can answer a sampling request.
    let ai: Vec<(String, String)> = if tab.sampling.is_empty() {
        Vec::new()
    } else {
        app.ws
            .data
            .collections
            .iter()
            .flat_map(|c| {
                c.requests()
                    .into_iter()
                    .filter(|r| matches!(r.request, RequestKind::Ai(_)))
                    .map(move |r| (r.id.clone(), format!("{} / {}", c.name, r.name)))
            })
            .collect()
    };
    header(ui, app, tab_id, &tab.slot, &mut item, |ui| {
        let text = pc::conn_text(ui, state, since);
        if state != Conn::Idle {
            pc::conn_pill(ui, state, &text, false);
        }
        if let Some((name, protocol)) = &server {
            let opts = kit::PillOpts { outline: true, ..Default::default() };
            kit::pill_ex(ui, name, kit::Tone::Neutral, opts).on_hover_text(trf("protocol {}", &[protocol]));
        }
    });
    let item_name = item.name.clone();
    let item_id = item.id.clone();
    let scripts = &mut item.scripts;
    let RequestKind::Mcp(req) = &mut item.request else { return };
    let mut action = None;
    let mut call: Option<(McpCallKind, String, serde_json::Value)> = None;
    let can_call = tab.client.is_some() && !tab.busy;
    let mut call_now = can_call && tab.selected.is_some() && pc::send_shortcut(ui);
    if let Some(a) = tab.auto.clone() {
        if a == "connect" {
            if tab.client.is_none() && !tab.connecting {
                action = Some(ConnAction::Connect);
            }
            tab.auto = None;
        } else if let Some(rest) = a.strip_prefix("call:")
            && tab.catalog.is_some()
            && !tab.busy
        {
            let (name, args) = rest.split_once(':').unwrap_or((rest, "{}"));
            tab.view = View::Tools;
            tab.selected = Some((McpCallKind::Tool, name.to_string()));
            tab.args = serde_json::from_str::<serde_json::Value>(args)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or_else(|| args.to_string());
            call = Some((McpCallKind::Tool, name.to_string(), serde_json::from_str(args).unwrap_or_default()));
            tab.auto = None;
        } else if let Some(s) = a.strip_prefix("section:") {
            tab.view = match s {
                "resources" => View::Resources,
                "prompts" => View::Prompts,
                "saved" => View::Saved,
                "conn" | "connection" => View::Connection,
                "scripts" => View::Scripts,
                _ => View::Tools,
            };
            tab.auto = None;
        } else if let Some(s) = a.strip_prefix("result:") {
            tab.result_view = if s == "traffic" { ResultView::Traffic } else { ResultView::Result };
            tab.auto = None;
        } else if let Some(i) = a.strip_prefix("select:").and_then(|i| i.parse().ok()) {
            tab.log.selected = Some(i);
            tab.auto = None;
        }
    }
    // Automation: approve with the first saved AI request.
    let auto_approve = tab.auto.as_deref() == Some("approve") && tab.sampling.front().is_some_and(|p| !p.running);
    if auto_approve {
        tab.auto = None;
    }
    let mut new_result: Option<Option<CallResult>> = None;
    let mut copied_config = false;
    let mut retry = false;
    let mut pending: Option<Pending> = None;
    let panes = ui.make_persistent_id("mcp-panes");
    let typed_command = &mut tab.typed_command;
    let export_style = &mut tab.export_style;
    let counts = tab
        .catalog
        .as_ref()
        .map(|c| (c.tools.len(), c.resources.len() + c.templates.len(), c.prompts.len()))
        .unwrap_or_default();
    let traffic_n = tab.log.lines.len();
    let busy = tab.busy;
    let result = &tab.result;
    let log = &mut tab.log;
    let result_view = &mut tab.result_view;
    let sampling = &mut tab.sampling;
    let elicit = &mut tab.elicit;
    let error = &mut tab.error;
    let view = &mut tab.view;
    let browser = Browser {
        view: *view,
        catalog: tab.catalog.as_deref(),
        selected: &mut tab.selected,
        args: &mut tab.args,
        search: &mut tab.search,
        connected: tab.client.is_some(),
        busy,
    };
    let catalog = tab.catalog.as_deref();
    let scripts_state = &mut tab.scripts;
    pc::view_panes(
        ui,
        panes,
        |ui| {
            let clicked = pc::connect_bar(
                ui,
                "MCP",
                124.0,
                |ui| {
                    let is_http = matches!(req.transport, McpTransport::Http { .. });
                    let label = if is_http { "HTTP" } else { "STDIO" };
                    let w = pc::bar_combo_width(ui, label);
                    let r = pc::bar_combo(ui, tr("Transport"), w, |ui, rect| {
                        let p = theme::pal(ui);
                        ui.painter().text(
                            rect.left_center(),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(12.5),
                            p.text,
                        );
                    });
                    let r = r.on_hover_text(tr("Transport"));
                    egui::Popup::menu(&r).width(200.0).show(|ui| {
                        let http =
                            kit::MenuItem { checked: Some(is_http), label: tr("HTTP (remote)"), ..Default::default() };
                        if kit::menu_item_ex(ui, http).clicked() && !is_http {
                            req.transport = McpTransport::Http { url: String::new(), headers: Vec::new() };
                        }
                        let stdio = kit::MenuItem {
                            checked: Some(!is_http),
                            label: tr("STDIO (local command)"),
                            ..Default::default()
                        };
                        if kit::menu_item_ex(ui, stdio).clicked() && is_http {
                            req.transport = McpTransport::Stdio {
                                command: String::new(),
                                args: Vec::new(),
                                env: Vec::new(),
                                cwd: String::new(),
                            };
                        }
                    });
                    pc::bar_divider(ui);
                    let w = ui.available_width();
                    let r = match &mut req.transport {
                        McpTransport::Http { url, .. } => {
                            pc::bar_var(ui, "mcp-url", url, &vars, "https://example.com/mcp", w)
                        }
                        McpTransport::Stdio { command, args, .. } => {
                            let mut line = command_line(typed_command.take(), command, args);
                            let r = pc::bar_var(
                                ui,
                                "mcp-cmd",
                                &mut line,
                                &vars,
                                "npx -y @modelcontextprotocol/server-everything",
                                w,
                            );
                            if r.changed() {
                                let mut parts = shell_split(&line).into_iter();
                                *command = parts.next().unwrap_or_default();
                                *args = parts.collect();
                            }
                            *typed_command = Some((line, shell_join(command, args)));
                            r
                        }
                    };
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
            let tabs = [
                Tab::new(View::Tools, tr("Tools")).count(counts.0),
                Tab::new(View::Resources, tr("Resources")).count(counts.1),
                Tab::new(View::Prompts, tr("Prompts")).count(counts.2),
                Tab::new(View::Saved, tr("Saved")).count(req.saved_calls.len()),
                Tab::new(View::Connection, tr("Connection")),
                Tab::new(View::Scripts, tr("Scripts")).count(pc::scripts_count(scripts)),
            ];
            kit::tabs(ui, view, &tabs);
            let browser = Browser { view: *view, ..browser };
            match *view {
                View::Scripts => pc::scripts_ui(
                    ui,
                    scripts,
                    &[(ScriptEvent::PreRequest, "Before call"), (ScriptEvent::PostResponse, "After result")],
                    scripts_state,
                    &vars,
                ),
                View::Connection => {
                    copied_config =
                        connection_view(ui, req, catalog, export_style, &item_name, &item_id, inherited.clone(), &vars);
                }
                v => {
                    if v != View::Saved && browser.catalog.is_none() {
                        let (title, body) = if state == Conn::Busy {
                            (tr("Connecting…"), tr("Starting the server and listing what it offers."))
                        } else {
                            (tr("Not connected"), tr("Connect to list the server's tools, resources and prompts."))
                        };
                        kit::empty_state(ui, icons::PLUGS, title, body, |ui| {
                            if state == Conn::Busy {
                                ui.add(egui::Spinner::new().size(18.0).color(theme::pal(ui).accent));
                            }
                        });
                    } else {
                        browse(ui, browser, req, &vars, &resolve, &mut call, &mut call_now, &mut new_result);
                    }
                }
            }
        },
        |ui| {
            let p = theme::pal(ui);
            pc::status_row(
                ui,
                |ui| {
                    if busy {
                        pc::status_pill(ui, Conn::Busy, tr("Calling…"));
                    } else if let Some(res) = result {
                        let (tone, icon, text) = if res.is_error {
                            (kit::Tone::Err, icons::WARNING_CIRCLE, tr("Error"))
                        } else {
                            (kit::Tone::Ok, icons::CHECK_CIRCLE, tr("Success"))
                        };
                        let opts = kit::PillOpts { large: true, icon: Some(icon), ..Default::default() };
                        kit::pill_ex(ui, text, tone, opts);
                        pc::metrics(ui, |ui| {
                            if let Some(ms) = res.ms {
                                kit::metric(ui, icons::TIMER, &ternarywire_core::util::human_ms(ms));
                            }
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(RichText::new(&res.title).font(theme::mono(11.5)).color(p.text_muted))
                                    .truncate(),
                            );
                        });
                    } else {
                        pc::status_pill(ui, Conn::Idle, tr("No result yet"));
                    }
                },
                |ui| {
                    pc::pane_actions(ui, panes);
                    let copy = ui
                        .add_enabled_ui(result.is_some(), |ui| kit::icon_button_sm(ui, icons::COPY, tr("Copy result")));
                    if copy.inner.clicked()
                        && let Some(res) = result
                    {
                        ui.ctx().copy_text(res.text.clone());
                    }
                },
            );
            let tabs = [
                Tab::new(ResultView::Result, tr("Result")),
                Tab::new(ResultView::Traffic, tr("Traffic")).count(traffic_n),
            ];
            let traffic = *result_view == ResultView::Traffic;
            kit::tabs_with(ui, result_view, &tabs, |ui| {
                if traffic {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    log.tools(ui, ("mcp-log", tab_id), true, 120.0);
                }
            });
            if let (Some(e), Conn::Failed) = (error.as_ref(), state) {
                let (r, dismiss) = pc::connection_error(ui, tr("Could not connect to the server"), e);
                retry = r;
                if dismiss {
                    *error = None;
                }
            }
            pending = pending_ui(ui, tab_id, sampling, elicit, &ai, auto_approve);
            match *result_view {
                ResultView::Traffic => log.body(ui, ("mcp-log", tab_id)),
                ResultView::Result => result_ui(ui, result.as_ref(), busy),
            }
        },
    );
    if retry {
        action = Some(ConnAction::Connect);
    }
    if copied_config {
        app.toast(tr("Copied"), false);
    }
    if let Some(r) = new_result {
        tab.result = r;
    }
    match pending {
        Some(Pending::Decline) => tab.decline_sampling(),
        Some(Pending::Approve) => answer_sampling(app, tab_id, tab),
        Some(Pending::Elicit(o)) => tab.answer_elicit(o),
        None => {}
    }
    if call_now && call.is_none() {
        // ⇧⌘↩ outside the form (another section): call what is selected.
        if let Some((kind, target)) = tab.selected.clone() {
            match serde_json::from_str::<serde_json::Value>(&resolve(&tab.args)) {
                Ok(v) => call = Some((kind, target, v)),
                Err(e) => {
                    tab.result = Some(CallResult {
                        title: tr("Invalid arguments").into(),
                        text: e.to_string(),
                        is_error: true,
                        ms: None,
                        raw: serde_json::Value::Null,
                    })
                }
            }
        }
    }
    let req_clone = req.clone();
    let item_now = item.clone();
    tab.slot.store(app, &original, item);
    match action {
        Some(ConnAction::Connect) => connect(app, tab_id, tab, &item_now.id, &req_clone, collection.as_deref()),
        Some(ConnAction::Cancel) => tab.cancel_connect(),
        Some(ConnAction::Disconnect) => {
            if let Some(c) = tab.client.take() {
                app.rt.spawn(async move { c.close().await });
                tab.catalog = None;
                tab.since = None;
                tab.drop_pending();
                tab.log.system(tr("Disconnected"));
            }
        }
        None => {}
    }
    let saved = |kind: McpCallKind, target: &str, args: &serde_json::Value| ternarywire_core::protocols::McpSavedCall {
        id: String::new(),
        name: target.to_string(),
        kind,
        target: target.to_string(),
        arguments: args.to_string(),
    };
    if let (Some((kind, target, args)), Some(client)) = (call, tab.client.clone()) {
        tab.busy = true;
        tab.scripts.clear_results();
        let ctx = proto::mcp(&req_clone, Some(&saved(kind, &target, &args)));
        tab.pending_call = Some((kind, target.clone(), args.clone()));
        if let Some(run) =
            crate::proto_common::run_scripts(app, tab_id, &tab.slot, &item_now, ScriptEvent::PreRequest, ctx)
        {
            tab.scripts.wait_for(run);
        } else {
            run_call(app, tab_id, client, kind, target, args);
        }
    }
    if let Some(pre) = tab.scripts.take_pre() {
        match (pre, tab.pending_call.clone(), tab.client.clone()) {
            (Some(v), Some((kind, target, args)), Some(client)) => {
                let mut r = req_clone.clone();
                let mut c = saved(kind, &target, &args);
                proto::apply_mcp(&mut r, Some(&mut c), &v);
                let args = serde_json::from_str(&c.arguments).unwrap_or(args);
                tab.pending_call = Some((kind, c.target.clone(), args.clone()));
                run_call(app, tab_id, client, kind, c.target, args);
            }
            _ => {
                tab.busy = false;
                tab.pending_call = None;
                tab.log.system(tr("Skipped by pm.execution.skipRequest()"));
            }
        }
    }
    if let Some((kind, target, args, resp)) = tab.post.take() {
        let ctx = proto::mcp(&req_clone, Some(&saved(kind, &target, &args))).with_response(resp);
        crate::proto_common::run_scripts(app, tab_id, &tab.slot, &item_now, ScriptEvent::PostResponse, ctx);
    }
}

/// The arguments template of a tool or prompt.
fn default_args(c: &Catalog, kind: McpCallKind, target: &str) -> String {
    match kind {
        McpCallKind::Tool => c
            .tools
            .iter()
            .find(|t| t.name == target)
            .and_then(|t| serde_json::to_string_pretty(&ternarywire_mcp::args_template(&t.input_schema)).ok())
            .unwrap_or_else(|| "{}".into()),
        McpCallKind::Prompt => {
            let args: serde_json::Map<String, serde_json::Value> = c
                .prompts
                .iter()
                .find(|p| p.name == target)
                .map(|p| {
                    p.arguments.iter().map(|a| (a.name.clone(), serde_json::Value::String(String::new()))).collect()
                })
                .unwrap_or_default();
            serde_json::to_string_pretty(&serde_json::Value::Object(args)).unwrap_or_else(|_| "{}".into())
        }
        McpCallKind::Resource => "{}".into(),
    }
}

/// The catalog list (filter and cards) beside the form of the selected item.
#[allow(clippy::too_many_arguments)]
fn browse(
    ui: &mut Ui,
    b: Browser<'_>,
    req: &mut ternarywire_core::protocols::McpRequest,
    vars: &widgets::VarCtx,
    resolve: &dyn Fn(&str) -> String,
    call: &mut Option<(McpCallKind, String, serde_json::Value)>,
    call_now: &mut bool,
    new_result: &mut Option<Option<CallResult>>,
) {
    let p = theme::pal(ui);
    let rect = ui.available_rect_before_wrap();
    let list_w = 200.0_f32.min((rect.width() * 0.45).round());
    let list = egui::Rect::from_min_size(rect.min, egui::vec2(list_w, rect.height()));
    let form = egui::Rect::from_min_max(egui::pos2(rect.left() + list_w + 1.0, rect.top()), rect.max);
    ui.painter().vline(list.right() + 0.5, rect.y_range(), egui::Stroke::new(1.0, p.border));
    let Browser { view, catalog, selected, args, search, connected, busy } = b;
    pc::inside(ui, egui::UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if view == View::Saved {
            saved_list(ui, req, selected, args, new_result);
            return;
        }
        let Some(c) = catalog else { return };
        let list = entries(c, view);
        if list.is_empty() {
            let what = match view {
                View::Tools => tr("This server has no tools."),
                View::Resources => tr("This server has no resources."),
                _ => tr("This server has no prompts."),
            };
            egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.add(egui::Label::new(kit::hint(ui, what)).wrap());
            });
            return;
        }
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 10, bottom: 6 }).show(ui, |ui| {
            kit::search_field_sm(ui, "mcp-search", search, tr("Filter"), ui.available_width());
        });
        let q = search.to_lowercase();
        egui::ScrollArea::vertical().id_salt(("mcp-list", view == View::Tools)).auto_shrink([false; 2]).show(
            ui,
            |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 6, top: 0, bottom: 8 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for e in list.iter().filter(|e| {
                        q.is_empty()
                            || e.name.to_lowercase().contains(&q)
                            || e.target.to_lowercase().contains(&q)
                            || e.detail.as_ref().is_some_and(|d| d.to_lowercase().contains(&q))
                    }) {
                        let sel = selected.as_ref() == Some(&(e.kind, e.target.clone()));
                        if entry_card(ui, e, sel).clicked() && !sel {
                            *selected = Some((e.kind, e.target.clone()));
                            *args = default_args(c, e.kind, &e.target);
                            *new_result = Some(None);
                        }
                    }
                });
            },
        );
    });
    pc::inside(ui, egui::UiBuilder::new().max_rect(form).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        call_form(ui, view, catalog, selected, args, connected, busy, req, vars, resolve, call, call_now, new_result);
    });
    ui.advance_cursor_after_rect(rect);
}

/// `2 properties · query required`: a summary of a tool's input schema.
fn schema_summary(schema: &serde_json::Value) -> String {
    let n = schema.get("properties").and_then(|p| p.as_object()).map_or(0, |p| p.len());
    let required: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let mut s = trf("{} properties", &[&n]);
    if !required.is_empty() {
        s.push_str(&format!(" · {} {}", required.join(", "), tr("required")));
    }
    s
}

/// The form of the selected tool, resource or prompt: what it is, its
/// arguments (or URI), and Call / Save call.
#[allow(clippy::too_many_arguments)]
fn call_form(
    ui: &mut Ui,
    view: View,
    catalog: Option<&Catalog>,
    selected: &mut Option<(McpCallKind, String)>,
    args: &mut String,
    connected: bool,
    busy: bool,
    req: &mut ternarywire_core::protocols::McpRequest,
    vars: &widgets::VarCtx,
    resolve: &dyn Fn(&str) -> String,
    call: &mut Option<(McpCallKind, String, serde_json::Value)>,
    call_now: &mut bool,
    new_result: &mut Option<Option<CallResult>>,
) {
    let p = theme::pal(ui);
    let Some((kind, target)) = selected.clone() else {
        let body = if view == View::Saved {
            tr("Pick a saved call to load it.")
        } else {
            tr("Pick an item on the left to see it and call it.")
        };
        kit::empty_state(ui, icons::ARROW_LEFT, tr("Nothing selected"), body, |_| {});
        return;
    };
    pc::body(ui, "mcp-call", 12.0, 12.0, |ui| {
        let (icon, verb) = match kind {
            McpCallKind::Tool => (icons::PUZZLE_PIECE, tr("Call tool")),
            McpCallKind::Resource => (icons::FILE, tr("Read")),
            McpCallKind::Prompt => (icons::CHAT_CIRCLE, tr("Get prompt")),
        };
        let mut description = None;
        let mut schema = None;
        let mut title = target.clone();
        if let Some(c) = catalog {
            match kind {
                McpCallKind::Tool => {
                    if let Some(t) = c.tools.iter().find(|t| t.name == target) {
                        description = t.description.clone();
                        schema = Some(t.input_schema.clone());
                        title = t.display_name().to_string();
                    }
                }
                McpCallKind::Prompt => {
                    if let Some(pr) = c.prompts.iter().find(|pr| pr.name == target) {
                        description = pr.description.clone();
                        title = pr.title.clone().unwrap_or(pr.name.clone());
                    }
                }
                McpCallKind::Resource => {
                    if let Some(r) = c.resources.iter().find(|r| r.uri == target) {
                        description = r.description.clone();
                        title = r.title.clone().unwrap_or(r.name.clone());
                    }
                }
            }
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (tile, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
            ui.painter().rect_filled(tile, egui::CornerRadius::same(8), p.accent_soft);
            ui.painter().text(tile.center(), egui::Align2::CENTER_CENTER, icon, theme::icon_font(16.0), p.accent_text);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let tag_w = if view == View::Saved || title != target {
                        ui.painter().layout_no_wrap(target.clone(), theme::mono(11.0), p.text_muted).size().x + 20.0
                    } else {
                        0.0
                    };
                    let w = (ui.available_width() - tag_w).max(60.0);
                    let g = kit::elide(ui, &title, theme::semibold(14.0), p.text, w);
                    let (r, _) = ui.allocate_exact_size(g.size(), egui::Sense::hover());
                    ui.painter().galley(r.min, g, p.text);
                    let tag = if view == View::Saved || title != target { Some(target.as_str()) } else { None };
                    if let Some(t) = tag {
                        pc::mono_tag(ui, t, ui.available_width().max(40.0));
                    }
                });
                if let Some(d) = &description {
                    ui.add(egui::Label::new(kit::hint(ui, d)).wrap());
                }
            });
        });
        let mut target_edit = target.clone();
        if kind == McpCallKind::Resource {
            kit::field(ui, "URI", "", |ui| {
                pc::input_sm(ui, &mut target_edit, "", ui.available_width(), true);
            });
            if target_edit != target {
                *selected = Some((kind, target_edit.clone()));
            }
        } else {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.set_height(24.0);
                    ui.label(kit::section_title(tr("Arguments")));
                    ui.label(kit::hint(ui, tr("JSON object")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(c) = catalog
                            && kit::ghost(ui, Some(icons::ARROW_COUNTER_CLOCKWISE), tr("Reset"))
                                .on_hover_text(tr("Fill with a template of the arguments"))
                                .clicked()
                        {
                            *args = default_args(c, kind, &target);
                        }
                    });
                });
                pc::editor_well(ui, "mcp-args", args, Lang::Json, Some(vars), egui::Margin::ZERO, Some(132.0), None);
            });
            if let Some(s) = schema {
                let id = ui.make_persistent_id("mcp-input-schema");
                let summary = schema_summary(&s);
                let open = pc::disclosure(ui, id, tr("Input schema"), None, false, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&summary).font(theme::mono(11.0)).color(p.text_faint))
                            .truncate(),
                    );
                });
                if open {
                    pc::code_block(ui, "mcp-schema-text", &serde_json::to_string_pretty(&s).unwrap_or_default());
                }
            }
        }
        ui.horizontal(|ui| {
            let ready = connected && !busy;
            let hint = if !connected { tr("Connect first") } else { tr("A call is running") };
            let opts = kit::ButtonOpts { icon: Some(icons::PLAY), ..Default::default() };
            let r = ui.add_enabled_ui(ready, |ui| kit::button(ui, kit::Kind::Primary, kit::Size::Sm, verb, opts)).inner;
            if r.on_hover_text(format!("{verb} (⇧⌘↩)")).on_disabled_hover_text(hint).clicked() {
                *call_now = true;
            }
            if view != View::Saved && kit::ghost(ui, Some(icons::BOOKMARK_SIMPLE), tr("Save call")).clicked() {
                req.saved_calls.push(McpSavedCall {
                    id: ternarywire_core::util::new_id(),
                    name: target.clone(),
                    kind,
                    target: target_edit.clone(),
                    arguments: args.clone(),
                });
            }
        });
        if *call_now {
            match serde_json::from_str::<serde_json::Value>(&resolve(args)) {
                Ok(v) => *call = Some((kind, target_edit.clone(), v)),
                Err(e) => {
                    *new_result = Some(Some(CallResult {
                        title: tr("Invalid arguments").into(),
                        text: e.to_string(),
                        is_error: true,
                        ms: None,
                        raw: serde_json::Value::Null,
                    }))
                }
            }
        }
    });
}

/// The saved calls as cards; right-click one to remove it.
fn saved_list(
    ui: &mut Ui,
    req: &mut ternarywire_core::protocols::McpRequest,
    selected: &mut Option<(McpCallKind, String)>,
    args: &mut String,
    new_result: &mut Option<Option<CallResult>>,
) {
    if req.saved_calls.is_empty() {
        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.add(
                egui::Label::new(kit::hint(ui, tr("Use “Save call” to keep a tool call with its arguments."))).wrap(),
            );
        });
        return;
    }
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt("mcp-saved").auto_shrink([false; 2]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 6, top: 10, bottom: 8 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, c) in req.saved_calls.iter().enumerate() {
                let kind = match c.kind {
                    McpCallKind::Tool => tr("tool"),
                    McpCallKind::Prompt => tr("prompt"),
                    McpCallKind::Resource => tr("resource"),
                };
                let e = Entry {
                    kind: c.kind,
                    target: c.target.clone(),
                    name: c.name.clone(),
                    detail: (!c.arguments.trim().is_empty() && c.arguments.trim() != "{}")
                        .then(|| pc::preview_text(&c.arguments)),
                    tag: Some(kind.to_string()),
                };
                let sel = selected.as_ref() == Some(&(c.kind, c.target.clone()));
                let r = entry_card(ui, &e, sel);
                if r.clicked() {
                    *selected = Some((c.kind, c.target.clone()));
                    *args = if c.arguments.is_empty() { "{}".into() } else { c.arguments.clone() };
                    *new_result = Some(None);
                }
                r.context_menu(|ui| {
                    if kit::menu_item(ui, Some(icons::TRASH), tr("Remove"), None).clicked() {
                        remove = Some(i);
                    }
                });
            }
            ui.add_space(6.0);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                ui.add(egui::Label::new(kit::hint(ui, tr("Right-click a saved call to remove it."))).wrap());
            });
        });
    });
    if let Some(i) = remove {
        req.saved_calls.remove(i);
    }
}

/// The Connection section: the server (when connected), the transport,
/// headers or environment, authorization and Export config. Returns true
/// when the config was copied.
#[allow(clippy::too_many_arguments)]
fn connection_view(
    ui: &mut Ui,
    req: &mut ternarywire_core::protocols::McpRequest,
    catalog: Option<&Catalog>,
    export_style: &mut ExportStyle,
    item_name: &str,
    item_id: &str,
    inherited: Option<(String, ternarywire_core::auth::AuthType)>,
    vars: &widgets::VarCtx,
) -> bool {
    let p = theme::pal(ui);
    let mut copied = false;
    pc::body(ui, "mcp-conn", 12.0, 14.0, |ui| {
        if let Some(c) = catalog {
            kit::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    kit::icon_tile(ui, icons::PUZZLE_PIECE, p.accent, 34.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(
                                RichText::new(c.info.title.clone().unwrap_or(c.info.name.clone()))
                                    .font(theme::semibold(13.0))
                                    .color(p.text),
                            );
                            let outline = kit::PillOpts { outline: true, ..Default::default() };
                            kit::pill_ex(ui, &c.info.version, kit::Tone::Neutral, outline);
                            kit::pill_ex(
                                ui,
                                &trf("protocol {}", &[&c.info.protocol_version]),
                                kit::Tone::Neutral,
                                outline,
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            for cap in ["tools", "resources", "prompts", "logging", "completions"] {
                                if c.info.has_capability(cap) {
                                    kit::tone_pill(ui, &format!("✓ {cap}"), kit::Tone::Ok);
                                }
                            }
                        });
                    });
                });
                if let Some(i) = &c.info.instructions {
                    ui.add_space(10.0);
                    ui.add(egui::Label::new(kit::hint(ui, &trf("Instructions: “{}”", &[i]))).wrap());
                }
            });
        }
        match &mut req.transport {
            McpTransport::Http { headers, .. } => {
                use ternarywire_core::protocols::McpHttpTransport as T;
                kit::group(
                    ui,
                    "HTTP",
                    tr(
                        "Servers that require OAuth open your browser to sign in; tokens are kept in the Keychain. Set the auth type to OAuth 2.0 to use a pre-registered client.",
                    ),
                    |ui, rows| {
                        rows.row(ui, tr("Transport"), tr("Auto: Streamable HTTP, falling back to HTTP+SSE"), |ui| {
                            ui.push_id("http-transport", |ui| {
                                kit::segmented_ex(
                                    ui,
                                    &mut req.http_transport,
                                    &[
                                        (T::Auto, None, tr("Auto")),
                                        (T::Streamable, None, "Streamable HTTP"),
                                        (T::Sse, None, tr("HTTP+SSE (legacy)")),
                                    ],
                                    true,
                                )
                            });
                        });
                    },
                );
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut h = headers.clone();
                    let opts = KvOptions {
                        title: Some(tr("Headers")),
                        allow_add: true,
                        key_hint: tr("Key"),
                        value_hint: tr("Value"),
                        ..Default::default()
                    };
                    if widgets::kv_table(ui, "mcp-headers", &mut h, vars, opts) {
                        *headers = h;
                    }
                    ui.add_space(6.0);
                    ui.label(kit::hint(ui, tr("A Bearer token in Authorization is sent as the Authorization header.")));
                });
            }
            McpTransport::Stdio { env, cwd, .. } => {
                kit::group(ui, "STDIO", "", |ui, rows| {
                    rows.row(ui, tr("Working directory"), "", |ui| {
                        pc::input_sm(ui, cwd, tr("the workspace folder"), 220.0, true);
                    });
                });
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let mut e: Vec<KeyValue> = env.clone();
                    let opts = KvOptions {
                        title: Some(tr("Environment")),
                        allow_add: true,
                        key_hint: tr("Key"),
                        value_hint: tr("Value"),
                        ..Default::default()
                    };
                    if widgets::kv_table(ui, "mcp-env", &mut e, vars, opts) {
                        *env = e;
                    }
                });
            }
        }
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(kit::section_title(tr("Authorization")));
            crate::auth_ui::auth_editor(
                ui,
                &mut req.auth,
                crate::auth_ui::AuthUiCtx {
                    is_root: false,
                    inherited,
                    owner_id: item_id,
                    token: None,
                    fetching: false,
                    vars,
                },
            );
        });
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(kit::section_title(tr("Export config")));
            ui.label(kit::hint(ui, tr("Use this server in other MCP hosts.")));
            ui.horizontal(|ui| {
                let opts = kit::ButtonOpts { icon: Some(icons::COPY), ..Default::default() };
                let w = (ui.available_width() - 110.0).max(140.0);
                let r = kit::select_box(ui, None, export_style.label(), w, true);
                widgets::a11y_name(&r, tr("Export config"));
                egui::Popup::menu(&r).width(w).show(|ui| {
                    for s in ExportStyle::ALL {
                        let item =
                            kit::MenuItem { checked: Some(*export_style == s), label: s.label(), ..Default::default() };
                        if kit::menu_item_ex(ui, item).clicked() {
                            *export_style = s;
                        }
                    }
                });
                if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Copy config"), opts).clicked() {
                    let v = ternarywire_mcp::export_config(item_name, req, *export_style);
                    ui.ctx().copy_text(serde_json::to_string_pretty(&v).unwrap_or_default());
                    copied = true;
                }
            });
        });
    });
    copied
}

/// The result of the last call: its content (texts and other parts) and
/// structured content, or the text of an error.
fn result_ui(ui: &mut Ui, result: Option<&CallResult>, busy: bool) {
    let p = theme::pal(ui);
    if busy {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new().size(18.0).color(p.accent));
            ui.label(RichText::new(tr("Waiting for the server…")).color(p.text_muted));
        });
        return;
    }
    let Some(res) = result else {
        kit::empty_state(
            ui,
            icons::LIGHTNING,
            tr("No result yet"),
            tr("Pick a tool, resource or prompt and call it."),
            |_| {},
        );
        return;
    };
    pc::body(ui, "mcp-result", 12.0, 10.0, |ui| {
        let content = res.raw.get("content").and_then(|c| c.as_array());
        let structured = res.raw.get("structuredContent").filter(|v| !v.is_null());
        if content.is_none() && structured.is_none() {
            ui.label(kit::eyebrow(ui, if res.is_error { tr("error") } else { &res.title }));
            pc::code_block(ui, "mcp-result-text", &res.text);
            return;
        }
        for (i, part) in content.into_iter().flatten().enumerate() {
            let kind = part.get("type").and_then(|t| t.as_str()).unwrap_or("content");
            ui.label(kit::eyebrow(ui, &format!("content · {kind}")));
            match part.get("text").and_then(|t| t.as_str()) {
                Some(text) => {
                    egui::Frame::new()
                        .fill(p.well)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let color = if res.is_error { p.err } else { p.text };
                            ui.add(
                                egui::Label::new(RichText::new(text).size(12.5).color(color)).wrap().selectable(true),
                            );
                        });
                }
                None => pc::code_block(ui, ("mcp-part", i), &serde_json::to_string_pretty(part).unwrap_or_default()),
            }
        }
        if let Some(s) = structured {
            ui.label(kit::eyebrow(ui, "structuredContent"));
            pc::code_block_gutter(ui, "mcp-structured", &serde_json::to_string_pretty(s).unwrap_or_default());
        }
    });
}

/// What the user answered to a server request.
enum Pending {
    Decline,
    Approve,
    Elicit(ternarywire_mcp::ElicitationResponse),
}

/// Sampling and elicitation requests of the server, oldest first, as
/// accent cards at the top of the traffic pane.
fn pending_ui(
    ui: &mut Ui,
    tab_id: u64,
    sampling: &mut VecDeque<PendingSampling>,
    elicit: &mut VecDeque<PendingElicit>,
    ai: &[(String, String)],
    auto_approve: bool,
) -> Option<Pending> {
    let p = theme::pal(ui);
    let mut out = None;
    if sampling.is_empty() && elicit.is_empty() {
        return None;
    }
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 0 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        let waiting = sampling.len().saturating_sub(1);
        if let Some(s) = sampling.front_mut() {
            if auto_approve {
                s.model = ai.first().map(|(id, _)| id.clone());
                if s.model.is_some() {
                    out = Some(Pending::Approve);
                }
            }
            let title = format!("{} {}", s.request.server, tr("asks to use your AI model"));
            pc::notice_card_ex(
                ui,
                icons::ROBOT,
                &title,
                |ui| {
                    if waiting > 0 {
                        ui.label(
                            RichText::new(trf("{} more requests waiting", &[&waiting])).size(11.0).color(p.text_faint),
                        );
                    }
                },
                |ui| {
                    if let Some(sys) = &s.request.system_prompt {
                        ui.label(
                            RichText::new(format!("{}: {}", tr("System"), crate::dialogs::ellipsize(sys, 300)))
                                .size(12.0)
                                .color(p.text_muted),
                        );
                    }
                    for m in &s.request.messages {
                        let text: Vec<String> = m
                            .content
                            .iter()
                            .map(|c| match c {
                                ternarywire_mcp::Content::Text { text } => text.clone(),
                                _ => format!("[{}]", tr("non-text content")),
                            })
                            .collect();
                        ui.horizontal_wrapped(|ui| {
                            pc::mono_tag(ui, &m.role, 100.0);
                            ui.label(
                                RichText::new(crate::dialogs::ellipsize(&text.join(" "), 500)).font(theme::mono(12.0)),
                            );
                        });
                    }
                    if !s.request.model_hints.is_empty() {
                        ui.label(kit::hint(
                            ui,
                            &format!("{}: {}", tr("Model hints"), s.request.model_hints.join(", ")),
                        ));
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("Answer with")).size(12.0).color(p.text_muted));
                        let current = s
                            .model
                            .as_ref()
                            .and_then(|id| ai.iter().find(|(i, _)| i == id))
                            .map(|(_, n)| n.clone())
                            .unwrap_or_else(|| tr("choose an AI request").into());
                        let r = kit::select_box(ui, None, &current, 220.0, true);
                        widgets::a11y_name(&r, tr("Model"));
                        egui::Popup::menu(&r).id(egui::Id::unique(("mcp-sampling-model", tab_id))).width(240.0).show(
                            |ui| {
                                for (id, name) in ai {
                                    let item = kit::MenuItem {
                                        checked: Some(s.model.as_ref() == Some(id)),
                                        label: name,
                                        ..Default::default()
                                    };
                                    if kit::menu_item_ex(ui, item).clicked() {
                                        s.model = Some(id.clone());
                                    }
                                }
                            },
                        );
                        if ai.is_empty() {
                            ui.label(kit::hint(ui, tr("Save an AI request first")));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if s.running {
                                ui.add(egui::Spinner::new().size(16.0).color(p.accent));
                                ui.label(RichText::new(tr("Answering…")).color(p.text_muted));
                            } else {
                                let x = kit::ButtonOpts { icon: Some(icons::X), ..Default::default() };
                                if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Decline"), x).clicked() {
                                    out = Some(Pending::Decline);
                                }
                                let ok = kit::ButtonOpts { icon: Some(icons::CHECK), ..Default::default() };
                                let r = ui.add_enabled_ui(s.model.is_some(), |ui| {
                                    kit::button(ui, kit::Kind::Primary, kit::Size::Sm, tr("Approve"), ok)
                                });
                                if r.inner.clicked() {
                                    out = Some(Pending::Approve);
                                }
                            }
                        });
                    });
                },
            );
        }
        let waiting = elicit.len().saturating_sub(1);
        if let Some(e) = elicit.front_mut() {
            let title = format!("{} {}", e.request.server, tr("asks for input"));
            pc::notice_card_ex(
                ui,
                icons::QUESTION,
                &title,
                |ui| {
                    if waiting > 0 {
                        let text = if waiting == 1 {
                            tr("1 more request waiting").to_string()
                        } else {
                            trf("{} more requests waiting", &[&waiting])
                        };
                        ui.label(RichText::new(text).size(11.0).color(p.text_faint));
                    }
                },
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&e.request.message).size(12.0).color(p.text));
                        if let Some(url) = &e.request.url
                            && ui.link(RichText::new(url).size(12.0).color(p.accent_text)).clicked()
                        {
                            crate::widgets::open_web_link(url);
                        }
                    });
                    elicit_fields(ui, tab_id, e);
                    for problem in &e.problems {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icons::WARNING_CIRCLE).font(theme::icon_font(14.0)).color(p.err));
                            ui.label(RichText::new(problem).size(12.0).color(p.err));
                        });
                    }
                    ui.horizontal(|ui| {
                        if kit::button(ui, kit::Kind::Primary, kit::Size::Sm, tr("Submit"), Default::default())
                            .clicked()
                        {
                            let content: serde_json::Map<String, serde_json::Value> = e
                                .values
                                .iter()
                                .filter(|(_, v)| !v.is_null())
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            e.problems = e.request.validate(&content);
                            if e.problems.is_empty() {
                                out = Some(Pending::Elicit(ternarywire_mcp::ElicitationResponse::Accept(content)));
                            }
                        }
                        if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Decline"), Default::default())
                            .clicked()
                        {
                            out = Some(Pending::Elicit(ternarywire_mcp::ElicitationResponse::Decline));
                        }
                        if kit::ghost(ui, None, tr("Cancel")).clicked() {
                            out = Some(Pending::Elicit(ternarywire_mcp::ElicitationResponse::Cancel));
                        }
                    });
                },
            );
        }
    });
    out
}

/// The form of an elicitation, two fields a row: required fields have a
/// `*`, booleans are checkboxes, choices drop-downs.
fn elicit_fields(ui: &mut Ui, tab_id: u64, e: &mut PendingElicit) {
    let p = theme::pal(ui);
    let fields = e.request.fields();
    let w = ((ui.available_width() - 12.0) / 2.0).floor().max(120.0);
    for (row, pair) in fields.chunks(2).enumerate() {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for f in pair {
                ui.allocate_ui_with_layout(egui::vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    let label = format!("{}{}", f.label(), if f.required { " *" } else { "" });
                    kit::field(ui, &label, "", |ui| {
                        let v = e.values.entry(f.name.clone()).or_insert(serde_json::Value::Null);
                        match &f.kind {
                            ternarywire_mcp::ElicitFieldKind::Boolean => {
                                let mut b = v.as_bool().unwrap_or(false);
                                ui.horizontal(|ui| {
                                    ui.set_height(26.0);
                                    let text = f.description.clone().unwrap_or_else(|| f.label().to_string());
                                    if kit::checkbox(ui, &mut b, &text).changed() {
                                        *v = serde_json::Value::Bool(b);
                                    }
                                });
                            }
                            ternarywire_mcp::ElicitFieldKind::Number { integer, .. } => {
                                let mut text = if v.is_null() { String::new() } else { v.to_string() };
                                if pc::input_sm(ui, &mut text, "", w, false).changed() {
                                    *v = if *integer {
                                        text.trim()
                                            .parse::<i64>()
                                            .map(serde_json::Value::from)
                                            .unwrap_or(serde_json::Value::Null)
                                    } else {
                                        text.trim()
                                            .parse::<f64>()
                                            .ok()
                                            .and_then(serde_json::Number::from_f64)
                                            .map(serde_json::Value::Number)
                                            .unwrap_or(serde_json::Value::Null)
                                    };
                                }
                            }
                            ternarywire_mcp::ElicitFieldKind::Choice { values, labels } => {
                                let current = v.as_str().unwrap_or("").to_string();
                                let shown = values
                                    .iter()
                                    .position(|x| *x == current)
                                    .and_then(|i| labels.get(i))
                                    .cloned()
                                    .unwrap_or(current.clone());
                                let r = kit::select_box(ui, None, &shown, w, true);
                                widgets::a11y_name(&r, f.label());
                                egui::Popup::menu(&r).id(egui::Id::unique(("elicit", tab_id, &f.name))).width(w).show(
                                    |ui| {
                                        for (i, val) in values.iter().enumerate() {
                                            let label = labels.get(i).unwrap_or(val);
                                            let item = kit::MenuItem {
                                                checked: Some(current == *val),
                                                label,
                                                ..Default::default()
                                            };
                                            if kit::menu_item_ex(ui, item).clicked() {
                                                *v = serde_json::Value::String(val.clone());
                                            }
                                        }
                                    },
                                );
                            }
                            ternarywire_mcp::ElicitFieldKind::MultiChoice { values, labels, .. } => {
                                ui.horizontal_wrapped(|ui| {
                                    let mut chosen: Vec<String> = v
                                        .as_array()
                                        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                                        .unwrap_or_default();
                                    for (i, val) in values.iter().enumerate() {
                                        let mut on = chosen.contains(val);
                                        if kit::checkbox(ui, &mut on, labels.get(i).unwrap_or(val)).changed() {
                                            if on {
                                                chosen.push(val.clone())
                                            } else {
                                                chosen.retain(|c| c != val)
                                            }
                                            *v = serde_json::json!(chosen);
                                        }
                                    }
                                });
                            }
                            _ => {
                                let mut text = v.as_str().unwrap_or("").to_string();
                                let hint = if f.required { "" } else { tr("optional") };
                                if pc::input_sm(ui, &mut text, hint, w, false).changed() {
                                    *v = serde_json::Value::String(text);
                                }
                            }
                        }
                    });
                    if let Some(d) = &f.description
                        && !matches!(f.kind, ternarywire_mcp::ElicitFieldKind::Boolean)
                    {
                        ui.label(RichText::new(d).size(11.5).color(p.text_muted));
                    }
                });
            }
        });
        let _ = row;
    }
}

/// Joins a command and its arguments into a line that [`shell_split`]
/// reads back the same (arguments with spaces or quotes are quoted).
pub fn shell_join(command: &str, args: &[String]) -> String {
    if command.is_empty() && args.is_empty() {
        return String::new();
    }
    let quote = |a: &str| {
        if !a.is_empty() && !a.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            a.to_string()
        } else if a.contains('\'') {
            format!("\"{a}\"")
        } else {
            format!("'{a}'")
        }
    };
    std::iter::once(command).chain(args.iter().map(String::as_str)).map(quote).collect::<Vec<_>>().join(" ")
}

/// The STDIO command line to show: the text as typed (`typed`: the text
/// and the command line it was read as) while it still stands for the
/// command, so a trailing space or quotes are not lost while typing.
fn command_line(typed: Option<(String, String)>, command: &str, args: &[String]) -> String {
    let joined = shell_join(command, args);
    match typed {
        Some((text, read_as)) if read_as == joined => text,
        _ => joined,
    }
}

/// Splits a command line on whitespace, honouring single and double quotes.
pub fn shell_split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for ch in line.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                has = true;
            }
            None if ch.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => cur.push(ch),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Runs the approved sampling request through the chosen AI request.
fn answer_sampling(app: &mut TernaryWireApp, tab_id: u64, tab: &mut McpTab) {
    let Some(p) = tab.sampling.front_mut() else { return };
    let Some(item) = p.model.as_ref().and_then(|id| {
        app.ws.data.collections.iter().find_map(|c| c.find_request(id).map(|i| (c.id.clone(), i.clone())))
    }) else {
        return;
    };
    let (cid, item) = item;
    let RequestKind::Ai(mut ai) = item.request else { return };
    ai.system = p.request.system_prompt.clone().unwrap_or_default();
    ai.messages = p
        .request
        .messages
        .iter()
        .map(|m| ternarywire_core::protocols::AiMessage {
            role: m.role.clone(),
            content: m
                .content
                .iter()
                .filter_map(|c| if let ternarywire_mcp::Content::Text { text } = c { Some(text.clone()) } else { None })
                .collect::<Vec<_>>()
                .join("\n"),
        })
        .collect();
    ai.max_tokens = Some(p.request.max_tokens.min(u64::from(u32::MAX)) as u32);
    if let Some(t) = p.request.temperature {
        ai.temperature = Some(t as f32);
    }
    ai.mcp_servers.clear();
    let resolve = resolver(app, Some(&cid));
    for key in ["token", "value"] {
        let v = ai.auth.get_string(key);
        if !v.is_empty() {
            ai.auth.set(key, resolve(&v));
        }
    }
    let cfg = match ternarywire_ai::AiConfig::from_request(&ai, &resolve) {
        Ok(c) => c,
        Err(e) => {
            tab.log.error(trf("Sampling failed: {}", &[&e]));
            return;
        }
    };
    let Ok(http) = app.http.default_client() else { return };
    let Some(reply) = p.reply.take() else { return };
    p.running = true;
    let id = p.id;
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let r = ternarywire_ai::run(cfg, &http, Vec::new(), |_| {}, ternarywire_ai::CancellationToken::new()).await;
        let done = match r {
            Ok(res) => {
                let _ = reply.send(Ok(ternarywire_mcp::SamplingResult::text(&res.model, &res.text)));
                Ok(res.text)
            }
            Err(e) => {
                let _ = reply.send(Err(ternarywire_mcp::McpError::rejected(e.to_string())));
                Err(e.to_string())
            }
        };
        let _ = tx.send(Msg::Mcp { tab: tab_id, epoch: 0, event: McpMsg::SamplingDone(id, done) });
        ctx.request_repaint();
    });
}

/// `req` with an inherited auth replaced by the collection's or folder's.
pub fn with_inherited_auth(
    app: &TernaryWireApp,
    collection: Option<&str>,
    item_id: &str,
    req: &ternarywire_core::protocols::McpRequest,
) -> ternarywire_core::protocols::McpRequest {
    let mut req = req.clone();
    if req.auth.is_inherit() {
        req.auth = collection
            .and_then(|c| app.ws.data.collection(c))
            .and_then(|c| c.effective_auth(item_id).map(|e| e.auth.clone()))
            .unwrap_or_else(ternarywire_core::auth::Auth::none);
    }
    req
}

/// The server shown in the traffic log while connecting: as configured,
/// with `{{variables}}` unresolved so secrets passed to it stay out of the log.
fn connecting_target(req: &ternarywire_core::protocols::McpRequest) -> String {
    McpConfig::from_request(req, &|s: &str| s.to_string()).describe()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An MCP request set to "Inherit auth from parent" (the default) uses
    /// the collection's auth, like HTTP requests do.
    #[test]
    fn inherited_auth_reaches_the_server() {
        use ternarywire_core::model::{Item, RequestItem};
        use ternarywire_core::protocols::{McpRequest, McpTransport};
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let cid = app.new_collection("C");
        let req = McpRequest {
            transport: McpTransport::Http { url: "https://mcp.test/mcp".into(), headers: Vec::new() },
            ..Default::default()
        };
        assert!(req.auth.is_inherit());
        let mut item = RequestItem::new("server", ternarywire_core::model::HttpRequest::default());
        item.request = RequestKind::Mcp(req.clone());
        let item_id = item.id.clone();
        let c = app.ws.data.collection_mut(&cid).unwrap();
        c.auth = ternarywire_core::auth::Auth::bearer("{{token}}");
        c.variables.push(ternarywire_core::variables::Variable::new("token", "t0k"));
        c.items.push(Item::Request(item));
        let r = resolver(&app, Some(&cid));
        let cfg = McpConfig::from_request(&with_inherited_auth(&app, Some(&cid), &item_id, &req), &r);
        let ternarywire_mcp::Transport::Http { headers, .. } = &cfg.transport else { panic!("http") };
        assert!(headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer t0k"), "{headers:?}");
    }

    /// Secrets passed to a server (`--api-key {{vault:…}}`, `?token=…`) do
    /// not appear resolved in the traffic log.
    #[test]
    fn connecting_log_keeps_secrets_out() {
        use ternarywire_core::protocols::{McpRequest, McpTransport};
        let stdio = McpRequest {
            transport: McpTransport::Stdio {
                command: "npx".into(),
                args: vec!["server".into(), "--api-key".into(), "{{vault:key}}".into()],
                env: Vec::new(),
                cwd: String::new(),
            },
            ..Default::default()
        };
        assert_eq!(connecting_target(&stdio), "npx server --api-key {{vault:key}}");
    }

    fn sampling(server: &str) -> Box<ternarywire_mcp::SamplingRequest> {
        Box::new(ternarywire_mcp::SamplingRequest {
            server: server.into(),
            messages: Vec::new(),
            system_prompt: None,
            max_tokens: 100,
            temperature: None,
            stop_sequences: Vec::new(),
            model_hints: Vec::new(),
            model_preferences: None,
            include_context: None,
            metadata: None,
            raw: serde_json::Value::Null,
        })
    }

    fn elicitation(message: &str) -> Box<ternarywire_mcp::ElicitationRequest> {
        Box::new(ternarywire_mcp::ElicitationRequest {
            server: "s".into(),
            message: message.into(),
            requested_schema: serde_json::json!({"type": "object", "properties": {}}),
            mode: "form".into(),
            url: None,
            elicitation_id: None,
            raw: serde_json::Value::Null,
        })
    }

    /// A second sampling or elicitation request waits behind the first
    /// instead of replacing it (which rejected the first one unseen); they
    /// are answered in order, and closing the tab rejects what still waits.
    #[test]
    fn server_requests_queue_up_and_closing_rejects_them() {
        use tokio::sync::oneshot::{self, error::TryRecvError};
        let mut tab = McpTab::new(ItemSlot::default());
        let (r1, mut w1) = oneshot::channel();
        let (r2, mut w2) = oneshot::channel();
        tab.on_msg(McpMsg::Sampling(sampling("first"), r1));
        tab.on_msg(McpMsg::Sampling(sampling("second"), r2));
        let (e1, mut v1) = oneshot::channel();
        let (e2, mut v2) = oneshot::channel();
        tab.on_msg(McpMsg::Elicit(elicitation("one"), e1));
        tab.on_msg(McpMsg::Elicit(elicitation("two"), e2));
        assert!(matches!(w1.try_recv(), Err(TryRecvError::Empty)));
        assert_eq!(tab.sampling.iter().map(|p| p.request.server.as_str()).collect::<Vec<_>>(), ["first", "second"]);
        tab.decline_sampling();
        assert!(matches!(w1.try_recv(), Ok(Err(_))));
        assert!(matches!(w2.try_recv(), Err(TryRecvError::Empty)));
        tab.answer_elicit(ternarywire_mcp::ElicitationResponse::Decline);
        assert_eq!(v1.try_recv().ok(), Some(ternarywire_mcp::ElicitationResponse::Decline));
        assert_eq!(tab.elicit.front().map(|p| p.request.message.as_str()), Some("two"));
        // A finished answer removes its own request, not the one shown.
        tab.on_msg(McpMsg::SamplingDone(99, Ok(String::new())));
        assert_eq!(tab.sampling.len(), 1);

        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let id = app.add_tab(crate::app::TabKind::Mcp(Box::new(tab)));
        app.close_tab(app.tab_index(id).unwrap());
        assert!(matches!(w2.try_recv(), Err(TryRecvError::Closed)));
        assert!(matches!(v2.try_recv(), Err(TryRecvError::Closed)));

        // So does a connection that closes.
        let mut tab = McpTab::new(ItemSlot::default());
        let (r3, mut w3) = oneshot::channel();
        tab.on_msg(McpMsg::Sampling(sampling("third"), r3));
        tab.on_msg(McpMsg::Event(McpEvent::Closed("gone".into())));
        assert!(tab.sampling.is_empty());
        assert!(matches!(w3.try_recv(), Err(TryRecvError::Closed)));
    }

    #[test]
    fn splits_commands() {
        assert_eq!(shell_split("npx -y \"@scope/pkg name\" 'a b'"), vec!["npx", "-y", "@scope/pkg name", "a b"]);
        assert!(shell_split("   ").is_empty());
        let args: Vec<String> = ["-y", "a b", "it's", ""].map(String::from).into();
        let line = shell_join("npx", &args);
        assert_eq!(shell_split(&line), [vec!["npx".to_string()], args].concat(), "{line}");
    }

    /// Cancel ends a handshake that does not finish (a STDIO server that
    /// never answers); it used to stop only an OAuth login.
    #[test]
    fn cancel_ends_a_hanging_handshake() {
        use ternarywire_core::protocols::{McpRequest, McpTransport};
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let transport = McpTransport::Stdio {
            command: "sleep".into(),
            args: vec!["30".into()],
            env: Vec::new(),
            cwd: String::new(),
        };
        let req = McpRequest { transport, ..Default::default() };
        let mut tab = McpTab::new(ItemSlot::default());
        connect(&mut app, 1, &mut tab, "server", &req, None);
        assert_eq!(tab.state(), Conn::Busy);
        tab.cancel_connect();
        assert_eq!(tab.state(), Conn::Idle);
        assert!(tab.login.is_none());
    }

    /// The STDIO command line keeps what is typed: the space after `npx`
    /// used to vanish on the next frame (the line was joined again from the
    /// command and its arguments), so `npx -y` could not be typed.
    #[test]
    fn typing_a_command_line_keeps_spaces_and_quotes() {
        let mut typed = None;
        let (mut command, mut args) = (String::new(), Vec::new());
        for c in "npx -y \"a b\" x".chars() {
            // What the field shows, then one key press.
            let mut line = command_line(typed.take(), &command, &args);
            line.push(c);
            let mut parts = shell_split(&line).into_iter();
            command = parts.next().unwrap_or_default();
            args = parts.collect();
            typed = Some((line, shell_join(&command, &args)));
        }
        assert_eq!(command_line(typed.clone(), &command, &args), "npx -y \"a b\" x");
        assert_eq!((command.as_str(), args.as_slice()), ("npx", ["-y", "a b", "x"].map(String::from).as_slice()));
        // Changed elsewhere (another transport, an undo): the command wins.
        assert_eq!(command_line(typed, "uvx", &[]), "uvx");
    }
}
