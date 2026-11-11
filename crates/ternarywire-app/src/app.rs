//! Application shell: workspace, tabs, background tasks, shortcuts and the
//! top-level layout (PRD section 11).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use egui::{Key, KeyboardShortcut, Modifiers};

use ternarywire_core::environment::Environment;
use ternarywire_core::history::HistoryEntry;
use ternarywire_core::model::{Collection, Folder, HttpRequest, Id, Item, RequestItem, RequestKind, WsRequest};
use ternarywire_core::script::ScriptEngine;
use ternarywire_core::settings::AppSettings;
use ternarywire_core::tokens::{OAuth2Token, TokenStore};
use ternarywire_core::variables::VariableScopes;
use ternarywire_http::{CancellationToken, EngineConfig, HttpEngine};
use ternarywire_runner::{ExecuteInput, Execution, RunEvent, Services};

use crate::dialogs::Dialog;
use crate::i18n::{tr, trf};
use crate::request_tab::RequestTab;
use crate::state::WsState;
use crate::widgets::VarCtx;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemRef {
    pub collection: Id,
    pub item: Id,
}

pub enum TabKind {
    Request(Box<RequestTab>),
    WebSocket(Box<crate::ws_tab::WsTab>),
    Container(crate::container_tab::ContainerTab),
    Environment(crate::env_tab::EnvTab),
    Runner(Box<crate::runner_tab::RunnerTab>),
    Mock(Box<crate::mock_tab::MockTab>),
    Spec(Box<crate::spec_tab::SpecTab>),
    Mqtt(Box<crate::mqtt_tab::MqttTab>),
    Grpc(Box<crate::grpc_tab::GrpcTab>),
    SocketIo(Box<crate::sio_tab::SioTab>),
    Mcp(Box<crate::mcp_tab::McpTab>),
    Ai(Box<crate::ai_tab::AiTab>),
    Webhook(Box<crate::webhook_tab::WebhookTab>),
    Flow(Box<crate::flow_tab::FlowTab>),
    Capture(Box<crate::capture_tab::CaptureTab>),
}

impl TabKind {
    /// The saved request a tab shows, if any.
    pub fn location(&self) -> Option<&ItemRef> {
        match self {
            TabKind::Request(t) => t.location.as_ref(),
            TabKind::WebSocket(t) => t.location.as_ref(),
            TabKind::Mqtt(t) => t.slot.location.as_ref(),
            TabKind::Grpc(t) => t.slot.location.as_ref(),
            TabKind::SocketIo(t) => t.slot.location.as_ref(),
            TabKind::Mcp(t) => t.slot.location.as_ref(),
            TabKind::Ai(t) => t.slot.location.as_ref(),
            _ => None,
        }
    }

    pub fn location_mut(&mut self) -> Option<&mut Option<ItemRef>> {
        match self {
            TabKind::Request(t) => Some(&mut t.location),
            TabKind::WebSocket(t) => Some(&mut t.location),
            TabKind::Mqtt(t) => Some(&mut t.slot.location),
            TabKind::Grpc(t) => Some(&mut t.slot.location),
            TabKind::SocketIo(t) => Some(&mut t.slot.location),
            TabKind::Mcp(t) => Some(&mut t.slot.location),
            TabKind::Ai(t) => Some(&mut t.slot.location),
            _ => None,
        }
    }

    /// The unsaved request a tab holds, if any.
    pub fn scratch(&self) -> Option<&RequestItem> {
        match self {
            TabKind::Request(t) => t.scratch.as_ref(),
            TabKind::WebSocket(t) => t.scratch.as_ref(),
            TabKind::Mqtt(t) => t.slot.scratch.as_ref(),
            TabKind::Grpc(t) => t.slot.scratch.as_ref(),
            TabKind::SocketIo(t) => t.slot.scratch.as_ref(),
            TabKind::Mcp(t) => t.slot.scratch.as_ref(),
            TabKind::Ai(t) => t.slot.scratch.as_ref(),
            _ => None,
        }
    }

    pub fn scratch_mut(&mut self) -> Option<&mut Option<RequestItem>> {
        match self {
            TabKind::Request(t) => Some(&mut t.scratch),
            TabKind::WebSocket(t) => Some(&mut t.scratch),
            TabKind::Mqtt(t) => Some(&mut t.slot.scratch),
            TabKind::Grpc(t) => Some(&mut t.slot.scratch),
            TabKind::SocketIo(t) => Some(&mut t.slot.scratch),
            TabKind::Mcp(t) => Some(&mut t.slot.scratch),
            TabKind::Ai(t) => Some(&mut t.slot.scratch),
            _ => None,
        }
    }

    /// Script state of protocol tabs (FR-S7).
    pub fn script_state_mut(&mut self) -> Option<&mut crate::proto_common::ScriptState> {
        match self {
            TabKind::Grpc(t) => Some(&mut t.scripts),
            TabKind::Mqtt(t) => Some(&mut t.scripts),
            TabKind::SocketIo(t) => Some(&mut t.scripts),
            TabKind::Mcp(t) => Some(&mut t.scripts),
            TabKind::Ai(t) => Some(&mut t.scripts),
            TabKind::WebSocket(t) => Some(&mut t.scripts),
            _ => None,
        }
    }

    /// The tab type for a request of this protocol.
    pub fn for_item(location: Option<ItemRef>, scratch: Option<RequestItem>, kind: &RequestKind) -> Option<TabKind> {
        use crate::proto_common::ItemSlot;
        let slot = ItemSlot { location, scratch };
        Some(match kind {
            RequestKind::Http(_) => TabKind::Request(Box::new(match slot.location {
                Some(l) => RequestTab::saved(l),
                None => RequestTab::scratch(slot.scratch?),
            })),
            RequestKind::WebSocket(_) => TabKind::WebSocket(Box::new(match slot.location {
                Some(l) => crate::ws_tab::WsTab::saved(l),
                None => crate::ws_tab::WsTab::scratch(slot.scratch?),
            })),
            RequestKind::Mqtt(_) => TabKind::Mqtt(Box::new(crate::mqtt_tab::MqttTab::new(slot))),
            RequestKind::Grpc(_) => TabKind::Grpc(Box::new(crate::grpc_tab::GrpcTab::new(slot))),
            RequestKind::SocketIo(_) => TabKind::SocketIo(Box::new(crate::sio_tab::SioTab::new(slot))),
            RequestKind::Mcp(_) => TabKind::Mcp(Box::new(crate::mcp_tab::McpTab::new(slot))),
            RequestKind::Ai(_) => TabKind::Ai(Box::new(crate::ai_tab::AiTab::new(slot))),
        })
    }
}

pub struct Tab {
    pub id: u64,
    pub kind: TabKind,
}

/// Console lines kept (the latest).
pub const MAX_CONSOLE: usize = 5000;

/// Messages from background work. Those that change the workspace (variables,
/// history, tokens) carry `ws`, the [`TernaryWireApp::ws_gen`] they started
/// in: after a workspace switch they belong to a workspace that is closed.
pub enum Msg {
    Request {
        tab: u64,
        ws: u64,
        exec: Box<Execution>,
        before: VariableScopes,
        after: VariableScopes,
        collection: Option<Id>,
        /// Environment active when the request was sent.
        env: Option<Id>,
        rerun: bool,
    },
    Run {
        tab: u64,
        event: Box<RunEvent>,
    },
    RunFinished {
        tab: u64,
        ws: u64,
        before: VariableScopes,
        after: VariableScopes,
        collection: Id,
        /// Environment active when the run started.
        env: Option<Id>,
        persist: bool,
    },
    Token {
        ws: u64,
        owner: String,
        result: Result<OAuth2Token, String>,
    },
    Ws {
        tab: u64,
        epoch: u64,
        event: ternarywire_http::ws::WsEvent,
    },
    Schema {
        tab: u64,
        result: Result<ternarywire_convert::graphql::Schema, String>,
    },
    Mock {
        tab: u64,
        result: Result<ternarywire_mock::MockServer, String>,
    },
    Perf {
        tab: u64,
        event: crate::runner_tab::PerfMsg,
    },
    Mqtt {
        tab: u64,
        epoch: u64,
        event: ternarywire_mqtt::MqttEvent,
        /// MQTT 5 properties of a message event.
        props: Option<ternarywire_mqtt::MessageProperties>,
    },
    Grpc {
        tab: u64,
        epoch: u64,
        event: crate::grpc_tab::GrpcMsg,
    },
    SocketIo {
        tab: u64,
        epoch: u64,
        event: ternarywire_socketio::SocketIoEvent,
    },
    Mcp {
        tab: u64,
        epoch: u64,
        event: crate::mcp_tab::McpMsg,
    },
    Ai {
        tab: u64,
        epoch: u64,
        event: crate::ai_tab::AiMsg,
    },
    Webhook {
        tab: u64,
        event: crate::webhook_tab::WebhookMsg,
    },
    Flow {
        tab: u64,
        epoch: u64,
        event: crate::flow_tab::FlowMsg,
    },
    Capture {
        tab: u64,
        event: crate::capture_tab::CaptureMsg,
    },
    /// A protocol tab's script finished (FR-S7).
    ProtoScript {
        tab: u64,
        ws: u64,
        run: Box<crate::proto_common::ScriptRun>,
    },
    Toast(String, bool),
    Update(crate::update::UpdateMsg),
}

/// What a click in the tab bar asks for.
enum TabBarAct {
    Activate(usize),
    CloseOthers(usize),
    New,
}

/// Actions requested from inside a tab while it is being shown (the tab is
/// temporarily out of `tabs`, so they run after it is put back).
pub enum Deferred {
    Send(u64),
    RerunTests(u64),
}

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Collections,
    Environments,
    History,
    Apis,
    Flows,
    /// Mock servers, webhook listeners and capture proxies (the open tool tabs).
    Servers,
}

pub struct TernaryWireApp {
    pub rt: tokio::runtime::Runtime,
    pub settings: AppSettings,
    pub settings_path: PathBuf,
    pub ws: WsState,
    /// Counts workspace switches (see [`Msg`]).
    pub ws_gen: u64,
    pub http: Arc<HttpEngine>,
    pub scripts: Arc<dyn ScriptEngine>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    next_tab_id: u64,
    pub sidebar_tab: SidebarTab,
    pub sidebar: crate::sidebar::SidebarState,
    pub console: Vec<ternarywire_runner::ConsoleLine>,
    pub console_open: bool,
    pub console_filter: crate::dialogs::ConsoleFilter,
    pub tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    pub dialog: Option<Dialog>,
    pub toasts: Vec<Toast>,
    pub ctx: egui::Context,
    pub pending_tokens: std::collections::HashSet<String>,
    last_settings_saved: AppSettings,
    pub automation: Option<crate::automation::Automation>,
    pub pending_runner_start: Option<u64>,
    pub deferred: Vec<Deferred>,
    /// Last session file contents, and when input last changed something.
    pub last_session: Vec<u8>,
    session_dirty: Option<Instant>,
    /// Content hash of each unsaved request tab when it was opened, to tell
    /// whether closing it loses edits.
    scratch_baseline: std::collections::HashMap<u64, u64>,
    /// Tab to close once the Save As dialog saved it (closing asked to save).
    pub close_after_save: Option<u64>,
    /// Update check and download state.
    pub update: crate::update::UpdateState,
    /// The tab the tab bar last scrolled into view, and the bar's width then.
    tab_in_view: Option<(u64, u32)>,
    /// Keeps trackpad scrolls on one axis per gesture.
    axis_lock: crate::scroll_lock::AxisLock,
    /// The native menu bar (installed by `new`, not in tests).
    #[cfg(target_os = "macos")]
    menu: Option<crate::menu::AppMenu>,
}

/// A new connection or call number. Session events carry the number of the
/// session that sent them (0: not tied to one), so events a closed session
/// sends late are ignored once the tab has connected again.
pub fn next_epoch() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Whether an event of session `epoch` is for the tab's current session.
fn current(tab_epoch: u64, epoch: u64) -> bool {
    epoch == 0 || epoch == tab_epoch
}

/// Hash of a request's content (name and settings included).
fn content_hash(item: &RequestItem) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(item).unwrap_or_default().hash(&mut h);
    h.finish()
}

fn default_workspace_dir() -> PathBuf {
    AppSettings::config_dir().join("workspaces").join("My Workspace")
}

/// "New …" menu entries for the non-HTTP protocols.
/// Icon for a protocol template name (see [`protocol_templates`]).
pub fn protocol_icon(name: &str) -> &'static str {
    use crate::icons;
    match name {
        "gRPC" => icons::LIGHTNING,
        "MQTT" => icons::WIFI_HIGH,
        "Socket.IO" => icons::PLUGS,
        "MCP" => icons::PUZZLE_PIECE,
        "AI request" => icons::SPARKLE,
        _ => icons::GLOBE,
    }
}

/// What the New menus create (the toolbar's New and, on macOS, File › New).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewItem {
    Request,
    /// A request with a GraphQL body.
    GraphQl,
    WebSocket,
    /// The n-th entry of [`protocol_templates`].
    Protocol(usize),
    Webhook,
    Capture,
    Flow,
    Collection,
    Environment,
    /// An OpenAPI spec in `specs/`.
    Spec,
}

impl NewItem {
    /// Every item in menu order, with its icon and (untranslated) label.
    // Only the macOS menu bar (File › New) lists them all.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn all() -> Vec<(NewItem, &'static str, &'static str)> {
        use crate::icons;
        let mut items = vec![
            (NewItem::Request, icons::GLOBE, "New Request"),
            (NewItem::GraphQl, icons::GRAPH, "New GraphQL request"),
            (NewItem::WebSocket, icons::PLUGS_CONNECTED, "New WebSocket"),
        ];
        for (i, (label, name, _)) in protocol_templates().into_iter().enumerate() {
            items.push((NewItem::Protocol(i), protocol_icon(name), label));
        }
        items.extend([
            (NewItem::Webhook, icons::WEBHOOKS_LOGO, "New webhook listener"),
            (NewItem::Capture, icons::BROADCAST, "Capture traffic (proxy)"),
            (NewItem::Flow, icons::FLOW_ARROW, "New flow"),
            (NewItem::Collection, icons::STACK, "New Collection"),
            (NewItem::Environment, icons::GLOBE_SIMPLE, "New Environment"),
            (NewItem::Spec, icons::FILE_CODE, "New API spec"),
        ]);
        items
    }
}

pub fn protocol_templates() -> Vec<(&'static str, &'static str, RequestKind)> {
    use ternarywire_core::protocols::*;
    vec![
        ("New gRPC", "gRPC", RequestKind::Grpc(GrpcRequest { url: "localhost:50051".into(), ..Default::default() })),
        (
            "New MQTT",
            "MQTT",
            RequestKind::Mqtt(MqttRequest { url: "mqtt://localhost:1883".into(), ..Default::default() }),
        ),
        (
            "New Socket.IO",
            "Socket.IO",
            RequestKind::SocketIo(SocketIoRequest {
                url: "http://localhost:3000".into(),
                listeners: vec![SocketIoListener { event: "message".into(), enabled: true }],
                ..Default::default()
            }),
        ),
        ("New MCP", "MCP", RequestKind::Mcp(McpRequest::default())),
        (
            "New AI request",
            "AI request",
            RequestKind::Ai(AiRequest {
                model: "gpt-5-mini".into(),
                stream: true,
                messages: vec![AiMessage { role: "user".into(), content: "Hello!".into() }],
                auth: ternarywire_core::auth::Auth::bearer("{{vault:openai}}"),
                ..Default::default()
            }),
        ),
    ]
}

/// Replaces every occurrence of a secret (raw or percent-encoded) with `•••`.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for s in secrets {
        out = out.replace(s.as_str(), "•••");
        let encoded = url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
        if encoded != *s {
            out = out.replace(&encoded, "•••");
        }
    }
    out
}

/// [`redact`] on every part of a request that may hold a literal secret
/// (pre-request scripts often copy one into a header).
pub fn redact_request(req: &mut HttpRequest, secrets: &[String]) {
    if secrets.is_empty() {
        return;
    }
    req.url = redact(&req.url, secrets);
    let body = &mut req.body;
    for kv in req.params.iter_mut().chain(&mut req.path_vars).chain(&mut req.headers).chain(&mut body.urlencoded) {
        kv.value = redact(&kv.value, secrets);
    }
    for f in &mut body.form_data {
        f.value = redact(&f.value, secrets);
    }
    body.raw = redact(&body.raw, secrets);
    body.graphql.query = redact(&body.graphql.query, secrets);
    body.graphql.variables = redact(&body.graphql.variables, secrets);
}

/// A copy of the collection containing only what one request needs (its
/// ancestors' auth, scripts and variables), so sending never clones a
/// collection with thousands of requests.
pub fn slim_collection(c: &Collection, item_id: &str) -> Collection {
    fn walk(items: &[Item], id: &str) -> Option<Vec<Item>> {
        for item in items {
            match item {
                Item::Request(r) if r.id == id => return Some(vec![Item::Request(r.clone())]),
                Item::Folder(f) => {
                    if let Some(children) = walk(&f.items, id) {
                        let mut copy = Folder { items: children, ..f.clone() };
                        copy.items.shrink_to_fit();
                        return Some(vec![Item::Folder(copy)]);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let items = walk(&c.items, item_id).unwrap_or_default();
    Collection { items, ..Collection { items: Vec::new(), ..c.clone() } }
}

impl TernaryWireApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut settings: AppSettings,
        settings_path: PathBuf,
        open_path: Option<PathBuf>,
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        crate::theme::install_fonts(&cc.egui_ctx);
        crate::i18n::set_language(settings.ui.language);
        cc.egui_ctx.set_theme(match settings.ui.theme {
            ternarywire_core::settings::ThemePreference::System => egui::ThemePreference::System,
            ternarywire_core::settings::ThemePreference::Light => egui::ThemePreference::Light,
            ternarywire_core::settings::ThemePreference::Dark => egui::ThemePreference::Dark,
        });
        crate::theme::apply(&cc.egui_ctx, settings.ui.font_size);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(32)
            .thread_name("ternarywire-rt")
            .enable_all()
            .build()
            .expect("tokio runtime");

        let candidates: Vec<PathBuf> = open_path
            .into_iter()
            .chain(settings.recent_workspaces.iter().cloned())
            .chain(std::iter::once(default_workspace_dir()))
            .collect();
        let mut ws = None;
        let mut startup_error = None;
        for path in candidates {
            if ternarywire_core::storage::WorkspaceStore::is_workspace(&path) {
                match WsState::open(&path, settings.history_limit) {
                    Ok(w) => {
                        ws = Some(w);
                        break;
                    }
                    Err(e) => startup_error = Some(format!("Could not open {}: {e}", path.display())),
                }
            }
        }
        let ws = ws.unwrap_or_else(|| {
            let dir = default_workspace_dir();
            WsState::create(&dir, "My Workspace", settings.history_limit)
                .or_else(|_| WsState::open(&dir, settings.history_limit))
                .expect("could not create the default workspace")
        });
        settings.push_recent(ws.root());
        // Settings from older versions may hold passwords in plain text.
        let moved_secrets = Self::vault_settings_secrets(&mut settings);
        let http = Arc::new(Self::engine(&settings, &ws));
        let scripts = Self::script_engine(&settings);
        let (tx, rx) = mpsc::channel();
        let mut app = TernaryWireApp {
            rt,
            last_settings_saved: settings.clone(),
            settings,
            settings_path,
            ws,
            ws_gen: 0,
            http,
            scripts,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            sidebar_tab: SidebarTab::Collections,
            sidebar: Default::default(),
            console: Vec::new(),
            console_open: false,
            console_filter: Default::default(),
            tx,
            rx,
            dialog: None,
            toasts: Vec::new(),
            ctx: cc.egui_ctx.clone(),
            pending_tokens: Default::default(),
            automation: crate::automation::Automation::from_env(),
            pending_runner_start: None,
            last_session: Vec::new(),
            session_dirty: None,
            deferred: Vec::new(),
            scratch_baseline: Default::default(),
            close_after_save: None,
            update: Default::default(),
            tab_in_view: None,
            axis_lock: Default::default(),
            #[cfg(target_os = "macos")]
            menu: None,
        };
        if crate::update::check_on_startup(&app) {
            crate::update::check(&mut app, false);
        }
        #[cfg(target_os = "macos")]
        {
            // Tests run off the main thread, where AppKit has no menu bar.
            if !cfg!(test) {
                app.menu = Some(crate::menu::AppMenu::install(&cc.egui_ctx));
            }
            // ⌘Q is the menu's Quit, which closes like the window's button.
            cc.egui_ctx.options_mut(|o| o.quit_shortcuts.clear());
        }
        if let Some(log) = crate::crash::take_pending(&AppSettings::config_dir()) {
            app.toast(trf("TernaryWire closed unexpectedly last time. Details: {}", &[&log.display()]), true);
        }
        if moved_secrets && let Err(e) = app.settings.save(&app.settings_path) {
            tracing::warn!("could not save settings: {e}");
        }
        if let Some(e) = startup_error {
            app.toast(e, true);
        }
        for e in app.ws.data.load_errors.clone() {
            app.toast(e, true);
        }
        app.attach_external_secrets(false);
        crate::dialogs::restore_session(&mut app);
        if app.tabs.is_empty() {
            app.new_request_tab();
        }
        app
    }

    /// Opens the workspace's external secret links and warms their cache in
    /// the background, so `{{vault:…}}` lookups rarely wait for a provider.
    /// Without `force` nothing happens for workspaces that never configured a
    /// provider (building the client loads the system certificates).
    pub fn attach_external_secrets(&mut self, force: bool) {
        if self.ws.external.is_some()
            || (!force && !ternarywire_secrets::SecretsConfig::path(&self.ws.local_dir()).exists())
        {
            return;
        }
        let client = match ternarywire_secrets::client_for(&self.proxy_settings(), &self.settings.certificates) {
            Ok(c) => c,
            Err(e) => {
                self.toast(trf("External secrets: {}", &[&e.message]), true);
                return;
            }
        };
        match ternarywire_secrets::ExternalSecrets::open(&self.ws.local_dir(), self.ws.secrets.clone(), client) {
            Ok(ext) => {
                let names = ext.linked_names();
                if !names.is_empty() {
                    let warm = ext.clone();
                    let tx = self.tx.clone();
                    self.rt.spawn(async move {
                        for (name, e) in warm.prefetch(&names).await {
                            let _ = tx.send(Msg::Toast(format!("vault:{name}: {}", e.message), true));
                        }
                    });
                }
                self.ws.external = Some(ext);
            }
            Err(e) => self.toast(trf("External secrets: {}", &[&e.message]), true),
        }
    }

    /// Scripts run in worker processes started from this executable, so a
    /// script that crashes, hangs in native code or uses too much memory
    /// cannot take the app down.
    fn script_engine(settings: &AppSettings) -> Arc<dyn ScriptEngine> {
        // The test binary can't serve as a script worker (libtest rejects the
        // worker flag): tests, including the off-screen screenshots, run
        // scripts in process.
        if cfg!(test) {
            return Arc::new(ternarywire_script::BoaEngine::new());
        }
        let config = ternarywire_script::IsolationConfig {
            memory_limit_bytes: settings.script_memory_mb.saturating_mul(1024 * 1024),
            ..Default::default()
        };
        match ternarywire_script::IsolatedEngine::for_current_exe(config) {
            Ok(e) => Arc::new(e),
            Err(e) => {
                tracing::warn!("script worker processes unavailable ({e}); running scripts in the app");
                Arc::new(ternarywire_script::BoaEngine::new())
            }
        }
    }

    fn engine(settings: &AppSettings, ws: &WsState) -> HttpEngine {
        HttpEngine::new(Self::engine_config(settings, ws), ws.cookies.clone())
    }

    fn engine_config(settings: &AppSettings, ws: &WsState) -> EngineConfig {
        let mut cfg = EngineConfig::from_settings(settings);
        cfg.temp_dir = Some(std::env::temp_dir().join("ternarywire"));
        cfg.secret_resolver = Some(Self::vault_resolver(ws));
        cfg
    }

    /// Extra CA and the client certificate from Settings > Certificates for
    /// a WebSocket, Socket.IO or MQTT connection to `url` (the proxy comes
    /// from `settings.proxy`).
    pub fn tls_setup(&self, url: &str, verify: bool) -> Result<ternarywire_http::tls::TlsSetup, String> {
        let secrets = Self::vault_resolver(&self.ws);
        ternarywire_http::tls::TlsSetup::for_url(&self.settings.certificates, Some(&secrets), verify, url)
    }

    /// Resolves `{{vault:name}}` in the settings (client certificate
    /// passphrases, the proxy password): the settings' own secrets for
    /// `settings.` names, the workspace vault otherwise.
    pub fn vault_resolver(ws: &WsState) -> ternarywire_http::SecretResolver {
        let vault = crate::state::settings_vault(ws.vault());
        ternarywire_http::SecretResolver::new(move |s| {
            let scopes = ternarywire_core::variables::VariableScopes::default();
            ternarywire_core::variables::Resolver::new(&scopes).with_vault(&vault).resolve(s)
        })
    }

    /// Moves literal secrets of `settings` (proxy password, client
    /// certificate passphrases) to the settings' Keychain entry and leaves
    /// `{{vault:settings.<name>}}` references, so `settings.json` never
    /// holds them. Returns whether anything moved.
    pub fn vault_settings_secrets(settings: &mut AppSettings) -> bool {
        let store = crate::state::settings_secrets();
        let mut moved = false;
        let mut keep = |value: &mut String, name: &str| {
            if !value.is_empty() && !ternarywire_core::variables::is_pure_reference(value) {
                store.vault_set(name, value);
                *value = format!("{{{{vault:{}{name}}}}}", crate::state::SETTINGS_VAULT_PREFIX);
                moved = true;
            }
        };
        keep(&mut settings.proxy.password, "proxy-password");
        for c in &mut settings.certificates.client_certs {
            let host: String =
                c.host.chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '.' { ch } else { '-' }).collect();
            keep(&mut c.passphrase, &format!("client-cert-{}", host.trim_matches('-')));
        }
        moved
    }

    /// The proxy settings with the password itself, for the protocol clients.
    pub fn proxy_settings(&self) -> ternarywire_core::settings::ProxySettings {
        self.http.config().proxy
    }

    pub fn services(&self) -> Services {
        let mut defaults = self.settings.request.clone();
        defaults.timeout_ms = self.settings.request.timeout_ms;
        Services {
            http: self.http.clone(),
            scripts: self.scripts.clone(),
            tokens: self.ws.secrets.clone(),
            vault: Some(self.ws.vault()),
            defaults,
            script_timeout: Duration::from_millis(self.settings.script_timeout_ms.max(100)),
            runtime: self.rt.handle().clone(),
            packages: Some(ternarywire_core::script::PackageContext::for_workspace(
                self.ws.root(),
                &self.settings.package_registry,
            )),
        }
    }

    /// Something changed that the session file should reflect soon.
    pub fn mark_session_dirty(&mut self) {
        self.session_dirty.get_or_insert_with(Instant::now);
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast { text: text.into(), error, at: Instant::now() });
    }

    pub fn open_workspace(&mut self, path: PathBuf, create_name: Option<String>) {
        // New flows get their file (and a place in the session) first.
        self.flush_flows();
        crate::dialogs::save_session(self);
        self.ws.flush_blocking();
        let result = match create_name {
            Some(name) => WsState::create(&path, &name, self.settings.history_limit),
            None => WsState::open(&path, self.settings.history_limit),
        };
        match result {
            Ok(ws) => self.switch_workspace(ws, &path),
            Err(e) => self.toast(trf("Could not open workspace: {}", &[&e]), true),
        }
    }

    fn switch_workspace(&mut self, ws: WsState, path: &std::path::Path) {
        // Close the old tabs properly: running requests, runs, mock servers,
        // listeners and proxies would otherwise keep going (and write their
        // results into the new workspace).
        while !self.tabs.is_empty() {
            self.close_tab(self.tabs.len() - 1);
        }
        self.ws = ws;
        // Work the old tabs started may still send results; they are dropped.
        self.ws_gen += 1;
        self.pending_tokens.clear();
        self.http = Arc::new(Self::engine(&self.settings, &self.ws));
        self.attach_external_secrets(false);
        self.active = 0;
        self.settings.push_recent(path);
        crate::dialogs::restore_session(self);
        if self.tabs.is_empty() {
            self.new_request_tab();
        }
        for e in self.ws.data.load_errors.clone() {
            self.toast(e, true);
        }
    }

    pub fn add_tab(&mut self, kind: TabKind) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let at = if self.tabs.is_empty() { 0 } else { (self.active + 1).min(self.tabs.len()) };
        if let Some(item) = kind.scratch() {
            self.scratch_baseline.insert(id, content_hash(item));
        }
        self.tabs.insert(at, Tab { id, kind });
        self.active = at;
        id
    }

    /// The unsaved request of tab `id` counts as edited (a restored tab that
    /// was edited before the restart): closing it asks.
    pub fn mark_scratch_edited(&mut self, id: u64) {
        self.scratch_baseline.remove(&id);
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Whether closing tab `idx` loses work: a request that was never saved
    /// and was edited since its tab opened, or a spec with unsaved changes.
    /// Saved requests and flows are written automatically.
    pub fn tab_has_unsaved_work(&self, idx: usize) -> bool {
        let Some(tab) = self.tabs.get(idx) else { return false };
        if let TabKind::Spec(t) = &tab.kind {
            return t.dirty();
        }
        match tab.kind.scratch() {
            Some(item) if tab.kind.location().is_none() => {
                self.scratch_baseline.get(&tab.id) != Some(&content_hash(item))
            }
            _ => false,
        }
    }

    /// Closes tab `idx`, asking first when that would lose unsaved work.
    pub fn request_close_tab(&mut self, idx: usize) {
        if self.tab_has_unsaved_work(idx) {
            self.dialog = Some(Dialog::CloseUnsaved { tab: self.tabs[idx].id });
        } else {
            self.close_tab(idx);
        }
    }

    pub fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(idx);
        self.scratch_baseline.remove(&tab.id);
        match tab.kind {
            TabKind::Request(t) => {
                if let Some(r) = &t.running {
                    r.cancel.cancel();
                }
            }
            TabKind::Runner(t) => t.cancel_all(),
            TabKind::Mock(t) => t.stop(&self.rt),
            TabKind::Webhook(mut t) => t.stop(&self.rt),
            TabKind::Ai(mut t) => t.cancel(),
            TabKind::Flow(mut t) => {
                if t.flush(self.ws.root()) {
                    self.sidebar.flows_dirty = true;
                }
                t.stop();
            }
            TabKind::Capture(mut t) => t.stop(&self.rt),
            TabKind::WebSocket(t) => drop(t),
            _ => {}
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if idx < self.active {
            self.active -= 1;
        }
    }

    /// Asks for a workspace folder and opens it.
    pub fn pick_workspace(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title(tr("Open workspace folder")).pick_folder() else { return };
        if ternarywire_core::storage::WorkspaceStore::is_workspace(&dir) {
            self.open_workspace(dir, None);
        } else {
            self.toast(trf("{} is not a TernaryWire workspace (no ternarywire.json)", &[&dir.display()]), true);
        }
    }

    pub fn create_new(&mut self, what: NewItem) {
        match what {
            NewItem::Request => self.new_request_tab(),
            NewItem::GraphQl => self.new_graphql_tab(),
            NewItem::WebSocket => self.new_ws_tab(),
            NewItem::Protocol(i) => {
                if let Some((_, name, kind)) = protocol_templates().into_iter().nth(i) {
                    self.new_proto_tab(name, kind);
                }
            }
            NewItem::Webhook => {
                self.add_tab(TabKind::Webhook(Box::default()));
            }
            NewItem::Capture => {
                self.add_tab(TabKind::Capture(Box::default()));
            }
            NewItem::Flow => {
                let n = self.sidebar.flows.as_ref().map(|f| f.len()).unwrap_or(0) + 1;
                let name = format!("{} {n}", tr("Flow"));
                self.add_tab(TabKind::Flow(Box::new(crate::flow_tab::FlowTab::blank(&name))));
            }
            NewItem::Collection => self.dialog = Some(Dialog::prompt_new_collection()),
            NewItem::Environment => self.dialog = Some(Dialog::prompt_new_environment()),
            NewItem::Spec => crate::spec_tab::new_spec(self, false),
        }
    }

    pub fn new_request_tab(&mut self) {
        let item = RequestItem::new(tr("Untitled request"), HttpRequest::default());
        let mut tab = RequestTab::scratch(item);
        tab.focus_url = true;
        self.add_tab(TabKind::Request(Box::new(tab)));
    }

    /// Opens an unsaved request of another protocol in a new tab.
    pub fn new_proto_tab(&mut self, name: &str, kind: RequestKind) {
        let mut item = RequestItem::new(name, HttpRequest::default());
        item.request = kind;
        match TabKind::for_item(None, Some(item.clone()), &item.request) {
            Some(k) => {
                self.add_tab(k);
            }
            None => self
                .toast(trf("{} requests are not supported in this build yet", &[&item.request.protocol_label()]), true),
        }
    }

    pub fn new_ws_tab(&mut self) {
        let mut item = RequestItem::new("WebSocket", HttpRequest::default());
        item.request = RequestKind::WebSocket(WsRequest::default());
        self.add_tab(TabKind::WebSocket(Box::new(crate::ws_tab::WsTab::scratch(item))));
    }

    /// Opens (or focuses) the tab for a saved request.
    pub fn open_item(&mut self, r: ItemRef) {
        let existing = self.tabs.iter().position(|t| t.kind.location() == Some(&r));
        if let Some(i) = existing {
            self.active = i;
            return;
        }
        let Some(c) = self.ws.data.collection(&r.collection) else { return };
        match c.find(&r.item) {
            Some(Item::Request(item)) => {
                let label = item.request.protocol_label();
                match TabKind::for_item(Some(r), None, &item.request) {
                    Some(kind) => {
                        self.add_tab(kind);
                    }
                    None => self.toast(trf("{} requests are not supported in this build yet", &[&label]), true),
                }
            }
            Some(Item::Folder(_)) => self.open_container(r.collection.clone(), Some(r.item.clone())),
            None => {}
        }
    }

    pub fn open_container(&mut self, collection: Id, folder: Option<Id>) {
        let existing = self
            .tabs
            .iter()
            .position(|t| matches!(&t.kind, TabKind::Container(c) if c.collection == collection && c.folder == folder));
        match existing {
            Some(i) => self.active = i,
            None => {
                self.add_tab(TabKind::Container(crate::container_tab::ContainerTab::new(collection, folder)));
            }
        }
    }

    pub fn open_environment(&mut self, env: Option<Id>) {
        let existing = self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Environment(e) if e.env == env));
        match existing {
            Some(i) => self.active = i,
            None => {
                self.add_tab(TabKind::Environment(crate::env_tab::EnvTab { env, search: String::new() }));
            }
        }
    }

    pub fn open_runner(&mut self, collection: Id, folder: Option<Id>) {
        let tab = crate::runner_tab::RunnerTab::new(&self.ws, collection, folder);
        self.add_tab(TabKind::Runner(Box::new(tab)));
    }

    pub fn open_mock(&mut self, collection: Id) {
        let existing = self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Mock(m) if m.collection == collection));
        match existing {
            Some(i) => self.active = i,
            None => {
                self.add_tab(TabKind::Mock(Box::new(crate::mock_tab::MockTab::new(collection))));
            }
        }
    }

    /// The variable context for a request in `collection`.
    pub fn var_ctx(&self, collection: Option<&str>) -> VarCtx {
        VarCtx::new(self.ws.scopes_for(collection), self.ws.secret_keys(collection), self.ws.vault_names())
    }

    /// Starts sending the request of tab `id`.
    pub fn send(&mut self, id: u64) {
        let Some(idx) = self.tab_index(id) else { return };
        let services = self.services();
        let TabKind::Request(tab) = &mut self.tabs[idx].kind else { return };
        if tab.running.is_some() || tab.gql_sub.as_ref().map(|s| !s.done).unwrap_or(false) {
            return;
        }
        let item = match &tab.location {
            Some(loc) => self.ws.data.collection(&loc.collection).and_then(|c| c.find_request(&loc.item)).cloned(),
            None => tab.scratch.clone(),
        };
        if item.as_ref().map(crate::request_tab::is_subscription).unwrap_or(false) {
            // Subscriptions run over WebSocket from the tab itself.
            tab.click_send = true;
            return;
        }
        let (collection, request_id, collection_id, request) = match &tab.location {
            Some(loc) => {
                let Some(c) = self.ws.data.collection(&loc.collection) else { return };
                let Some(item) = c.find_request(&loc.item) else { return };
                let Some(http) = item.http() else { return };
                (slim_collection(c, &loc.item), loc.item.clone(), Some(loc.collection.clone()), http.clone())
            }
            None => {
                let Some(item) = &tab.scratch else { return };
                let Some(http) = item.http() else { return };
                let mut c = Collection::new("Scratch");
                c.items.push(Item::Request(item.clone()));
                (c, item.id.clone(), None, http.clone())
            }
        };
        if request.url.trim().is_empty() {
            return;
        }
        let before = self.ws.scopes_for(collection_id.as_deref());
        let env = self.ws.data.active_environment.clone();
        let cancel = CancellationToken::new();
        tab.start_running(cancel.clone());
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let base = self.ws.root().to_path_buf();
        let ws = self.ws_gen;
        self.rt.spawn(async move {
            let mut vars = before.clone();
            let input = ExecuteInput {
                collection: &collection,
                request_id: &request_id,
                request_override: Some(&request),
                iteration: 0,
                iteration_count: 1,
                base_dir: Some(&base),
            };
            let exec = ternarywire_runner::execute(&services, input, &mut vars, &cancel).await;
            let _ = tx.send(Msg::Request {
                tab: id,
                ws,
                exec: Box::new(exec),
                before,
                after: vars,
                collection: collection_id,
                env,
                rerun: false,
            });
            ctx.request_repaint();
        });
    }

    /// Re-runs post-response scripts on the existing response (FR-S6).
    pub fn rerun_tests(&mut self, id: u64) {
        let Some(idx) = self.tab_index(id) else { return };
        let TabKind::Request(tab) = &self.tabs[idx].kind else { return };
        // The response is about to be replaced (and the scripts' results
        // would end the send in progress).
        if tab.running.is_some() {
            return;
        }
        let Some(result) = &tab.result else { return };
        let (Some(prepared), Some(response)) = (result.exec.prepared.clone(), result.exec.response.clone()) else {
            return;
        };
        let (collection, request_id, collection_id) = match &tab.location {
            Some(loc) => {
                let Some(c) = self.ws.data.collection(&loc.collection) else { return };
                (slim_collection(c, &loc.item), loc.item.clone(), Some(loc.collection.clone()))
            }
            None => {
                let Some(item) = &tab.scratch else { return };
                let mut c = Collection::new("Scratch");
                c.items.push(Item::Request(item.clone()));
                (c, item.id.clone(), None)
            }
        };
        let before = self.ws.scopes_for(collection_id.as_deref());
        let env = self.ws.data.active_environment.clone();
        let services = self.services();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let ws = self.ws_gen;
        let base = self.ws.root().to_path_buf();
        self.rt.spawn(async move {
            let mut vars = before.clone();
            let cancel = CancellationToken::new();
            let mut exec = ternarywire_runner::pipeline::rerun_post_response(
                &services,
                &collection,
                &request_id,
                &prepared,
                &response,
                &mut vars,
                Some(&base),
                &cancel,
            )
            .await;
            exec.prepared = Some(prepared);
            exec.response = Some(response);
            let _ = tx.send(Msg::Request {
                tab: id,
                ws,
                exec: Box::new(exec),
                before,
                after: vars,
                collection: collection_id,
                env,
                rerun: true,
            });
            ctx.request_repaint();
        });
    }

    /// Gets a new OAuth 2.0 token for `auth`, whose variables are resolved
    /// with `scopes` and the workspace vault in the background (vault names
    /// linked to a secret manager may need the network).
    pub fn fetch_token(
        &mut self,
        auth: ternarywire_core::auth::Auth,
        scopes: ternarywire_core::variables::VariableScopes,
        owner: String,
    ) {
        if self.pending_tokens.contains(&owner) {
            return;
        }
        self.pending_tokens.insert(owner.clone());
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let http = self.http.clone();
        let vault = self.ws.vault();
        let ws = self.ws_gen;
        self.rt.spawn(async move {
            let resolved = tokio::task::spawn_blocking(move || {
                let resolver = ternarywire_core::variables::Resolver::new(&scopes).with_vault(vault.as_ref());
                auth.map_strings(|s| resolver.resolve(s))
            })
            .await;
            let result = match resolved {
                Err(e) => Err(e.to_string()),
                Ok(auth) => {
                    let opener = |url: &str| {
                        crate::widgets::open_web_link(url);
                    };
                    // The engine presents the token endpoint's client certificate (mutual TLS).
                    ternarywire_auth::oauth2::get_new_token(&auth, http.as_ref(), &opener)
                        .await
                        .map_err(|e| e.to_string())
                }
            };
            let _ = tx.send(Msg::Token { ws, owner, result });
            ctx.request_repaint();
        });
    }

    /// Keeps the latest [`MAX_CONSOLE`] console lines.
    fn trim_console(&mut self) {
        let extra = self.console.len().saturating_sub(MAX_CONSOLE);
        self.console.drain(..extra);
    }

    /// Handles pending background messages (tests drive the app with it).
    #[cfg(test)]
    pub fn pump_messages(&mut self) {
        self.handle_msgs();
    }

    fn handle_msgs(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                // The workspace they were for is closed (its tabs too).
                Msg::Request { ws, .. }
                | Msg::RunFinished { ws, .. }
                | Msg::Token { ws, .. }
                | Msg::ProtoScript { ws, .. }
                    if ws != self.ws_gen => {}
                Msg::Request { tab, exec, before, after, collection, env, rerun, .. } => {
                    self.ws.apply_var_changes(collection.as_deref(), env.as_deref(), &before, &after);
                    self.console.extend(exec.console.iter().cloned());
                    self.trim_console();
                    if exec.response.is_some() {
                        self.ws.mark_cookies();
                    }
                    if !rerun && let Some(req) = &exec.request {
                        let secrets =
                            self.secret_values(collection.as_deref(), env.as_deref(), &before, exec.prepared.as_ref());
                        let mut request = req.clone();
                        redact_request(&mut request, &secrets);
                        let resolved = exec
                            .sent
                            .as_ref()
                            .map(|s| s.url.clone())
                            .or_else(|| exec.prepared.as_ref().map(|p| p.url.clone()))
                            .unwrap_or_default();
                        let entry = HistoryEntry {
                            id: ternarywire_core::util::new_id(),
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            request,
                            resolved_url: redact(&resolved, &secrets),
                            status: exec.response.as_ref().map(|r| r.status),
                            elapsed_ms: exec.response.as_ref().map(|r| r.timings.total_ms).unwrap_or(0.0),
                            size: exec.response.as_ref().map(|r| r.total_size()).unwrap_or(0),
                            error: exec.error.as_ref().map(|e| redact(&e.message, &secrets)),
                            source_request_id: collection.as_ref().map(|_| exec.request_id.clone()),
                            name: Some(exec.request_name.clone()),
                        };
                        self.ws.add_history(entry);
                    }
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Request(t) = &mut self.tabs[i].kind
                    {
                        t.finish(*exec, rerun);
                    }
                }
                Msg::Run { tab, event } => {
                    if let RunEvent::Execution(exec) = event.as_ref() {
                        self.console.extend(exec.console.iter().cloned());
                        self.trim_console();
                    }
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Runner(t) = &mut self.tabs[i].kind
                    {
                        t.on_event(*event);
                    }
                }
                Msg::RunFinished { tab, before, after, collection, env, persist, .. } => {
                    if persist {
                        self.ws.apply_var_changes(Some(&collection), env.as_deref(), &before, &after);
                    }
                    self.ws.mark_cookies();
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Runner(t) = &mut self.tabs[i].kind
                    {
                        t.finished();
                    }
                }
                Msg::Token { owner, result, .. } => {
                    self.pending_tokens.remove(&owner);
                    match result {
                        Ok(token) => {
                            TokenStore::set(self.ws.secrets.as_ref(), &owner, Some(token));
                            self.toast(tr("New access token stored"), false);
                        }
                        Err(e) => self.toast(trf("OAuth 2.0: {}", &[&e]), true),
                    }
                }
                Msg::Ws { tab, epoch, event } => {
                    if let Some(i) = self.tab_index(tab) {
                        match &mut self.tabs[i].kind {
                            TabKind::WebSocket(t) if current(t.epoch, epoch) => t.on_event(event),
                            TabKind::Request(t) if current(t.ws_epoch, epoch) => t.on_ws_event(event),
                            _ => {}
                        }
                    }
                }
                Msg::Schema { tab, result } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Request(t) = &mut self.tabs[i].kind
                    {
                        t.graphql.loading = false;
                        match result {
                            Ok(s) => {
                                t.graphql.schema = Some(Arc::new(s));
                                t.graphql.error = None;
                                t.graphql.explorer_open = true;
                            }
                            Err(e) => t.graphql.error = Some(e),
                        }
                    }
                }
                Msg::Mock { tab, result } => {
                    let mut err = None;
                    if let Some(i) = self.tab_index(tab) {
                        if let TabKind::Mock(t) = &mut self.tabs[i].kind {
                            match result {
                                Ok(server) => t.started(server),
                                Err(e) => {
                                    t.starting = false;
                                    err = Some(e);
                                }
                            }
                        } else if let Ok(server) = result {
                            self.rt.spawn(server.stop());
                        }
                    }
                    if let Some(e) = err {
                        self.toast(trf("Mock server: {}", &[&e]), true);
                    }
                }
                Msg::Perf { tab, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Runner(t) = &mut self.tabs[i].kind
                    {
                        t.on_perf(event);
                    }
                }
                Msg::Mqtt { tab, epoch, event, props } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Mqtt(t) = &mut self.tabs[i].kind
                        && current(t.epoch, epoch)
                    {
                        t.on_event(event, props);
                    }
                }
                Msg::Grpc { tab, epoch, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Grpc(t) = &mut self.tabs[i].kind
                        && current(t.epoch, epoch)
                    {
                        t.on_msg(event);
                    }
                }
                Msg::SocketIo { tab, epoch, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::SocketIo(t) = &self.tabs[i].kind
                        && current(t.epoch, epoch)
                    {
                        let listened = crate::sio_tab::listened(self, t);
                        if let TabKind::SocketIo(t) = &mut self.tabs[i].kind {
                            t.on_event(event, &listened);
                        }
                    }
                }
                Msg::Mcp { tab, epoch, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Mcp(t) = &mut self.tabs[i].kind
                        && current(t.epoch, epoch)
                    {
                        t.on_msg(event);
                    }
                }
                Msg::Ai { tab, epoch, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Ai(t) = &mut self.tabs[i].kind
                        && current(t.epoch, epoch)
                    {
                        t.on_msg(event);
                    }
                }
                Msg::Capture { tab, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Capture(t) = &mut self.tabs[i].kind
                    {
                        t.on_msg(event);
                    }
                }
                Msg::Update(m) => crate::update::on_msg(self, m),
                Msg::Flow { tab, epoch, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Flow(t) = &mut self.tabs[i].kind
                        && current(t.run_epoch(), epoch)
                    {
                        t.on_msg(event);
                    }
                }
                Msg::Webhook { tab, event } => {
                    if let Some(i) = self.tab_index(tab)
                        && let TabKind::Webhook(t) = &mut self.tabs[i].kind
                    {
                        t.on_msg(event);
                    }
                }
                Msg::ProtoScript { tab, run, .. } => {
                    self.ws.apply_var_changes(
                        run.collection.as_deref(),
                        run.env.as_deref(),
                        &run.before,
                        &run.outcome.variables,
                    );
                    let at = chrono::Utc::now().timestamp_millis();
                    for c in &run.outcome.console {
                        self.console.push(ternarywire_runner::ConsoleLine {
                            kind: c.level.into(),
                            message: c.message.clone(),
                            source: run.name.clone(),
                            phase: run.event.label().into(),
                            at_ms: at,
                        });
                    }
                    for e in &run.outcome.errors {
                        self.console.push(ternarywire_runner::ConsoleLine {
                            kind: ternarywire_runner::ConsoleKind::Error,
                            message: format!("{}: {}", e.location, e.message),
                            source: run.name.clone(),
                            phase: run.event.label().into(),
                            at_ms: at,
                        });
                    }
                    self.trim_console();
                    if let Some(i) = self.tab_index(tab)
                        && let Some(s) = self.tabs[i].kind.script_state_mut()
                    {
                        s.on_run(&run);
                    }
                }
                Msg::Toast(t, e) => self.toast(t, e),
            }
        }
    }

    /// Values that must not be written to local files such as the history:
    /// secret variables, vault entries and sensitive auth parameters.
    pub fn secret_values(
        &self,
        collection: Option<&str>,
        env: Option<&str>,
        scopes: &VariableScopes,
        prepared: Option<&ternarywire_core::prepared::PreparedRequest>,
    ) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for key in self.ws.secret_keys_in(collection, env) {
            if let Some((v, _)) = scopes.get(&key) {
                out.push(v.into_owned());
            }
        }
        for name in self.ws.secrets.vault_names() {
            if let Some(v) = self.ws.secrets.vault_get(&name) {
                out.push(v);
            }
        }
        if let Some(ext) = &self.ws.external {
            out.extend(ext.linked_names().iter().filter_map(|n| ext.get_cached(n)));
        }
        if let Some(p) = prepared {
            for param in p.auth.kind.sensitive_params() {
                out.push(p.auth.get_string(param));
            }
        }
        out.retain(|v| v.len() >= 4);
        out.sort_by_key(|v| std::cmp::Reverse(v.len()));
        out
    }

    /// Carries out the menu bar items chosen since the last frame. Shortcut
    /// items become the key press they stand for, so `shortcuts` and text
    /// editing handle them as usual.
    #[cfg(target_os = "macos")]
    fn run_menu_commands(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        use crate::menu::Command;
        let Some(menu) = &self.menu else { return };
        for command in menu.take_commands() {
            if let Some(event) = crate::menu::as_input(command) {
                raw_input.events.push(event);
                continue;
            }
            match command {
                Command::New(what) => self.create_new(what),
                Command::Import => self.dialog = Some(Dialog::import()),
                Command::OpenWorkspace => self.pick_workspace(),
                Command::NewWorkspace => self.dialog = Some(Dialog::new_workspace()),
                Command::CheckForUpdates => crate::update::check(self, true),
                Command::Settings(section) => {
                    self.dialog = Some(Dialog::Settings { draft: Box::new(self.settings.clone()), section });
                }
                Command::Url(url) => {
                    crate::widgets::open_web_link(url);
                }
                Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Command::Key(..) | Command::Copy | Command::Cut | Command::Paste => {}
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let sc = |m: Modifiers, k: Key| KeyboardShortcut::new(m, k);
        let pressed = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        // egui ignores extra Shift/Alt when matching a shortcut; ⇧⌘↩ belongs
        // to the tab (it sends a WebSocket message), so ⌘↩ must match exactly.
        let exact = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| {
                let hit = i.events.iter().any(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => *key == k && modifiers.matches_exact(m),
                    _ => false,
                });
                hit && i.consume_shortcut(&KeyboardShortcut::new(m, k))
            })
        };
        if exact(cmd, Key::Enter)
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            let id = tab.id;
            match &mut tab.kind {
                TabKind::Request(t) if t.running.is_some() => t.cancel(),
                TabKind::Request(_) => self.send(id),
                // ⌘↩ runs a flow too (or stops the running one).
                TabKind::Flow(t) if t.is_running() => t.stop(),
                TabKind::Flow(t) => t.auto_run = true,
                _ => {}
            }
        }
        if pressed(sc(cmd | Modifiers::SHIFT, Key::P)) {
            self.dialog = Some(Dialog::palette(false));
        }
        if pressed(sc(cmd, Key::K)) {
            self.dialog = Some(Dialog::palette(true));
        }
        if pressed(sc(cmd, Key::N)) || pressed(sc(cmd, Key::T)) {
            self.new_request_tab();
        }
        if pressed(sc(cmd, Key::W)) {
            let a = self.active;
            self.request_close_tab(a);
        }
        if pressed(sc(cmd, Key::B)) {
            self.settings.ui.sidebar_visible = !self.settings.ui.sidebar_visible;
        }
        if pressed(sc(cmd, Key::E)) {
            self.dialog = Some(Dialog::env_switcher());
        }
        if pressed(sc(cmd | Modifiers::ALT, Key::C)) {
            self.console_open = !self.console_open;
        }
        if pressed(sc(cmd, Key::S)) {
            self.save_active();
        }
        if pressed(sc(cmd, Key::Comma)) {
            self.dialog = Some(Dialog::settings(&self.settings));
        }
        // With Shift held, `[` and `]` usually arrive as `{` and `}`.
        let prev = pressed(sc(cmd | Modifiers::SHIFT, Key::OpenBracket))
            || pressed(sc(cmd | Modifiers::SHIFT, Key::OpenCurlyBracket));
        if prev && !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
        let next = pressed(sc(cmd | Modifiers::SHIFT, Key::CloseBracket))
            || pressed(sc(cmd | Modifiers::SHIFT, Key::CloseCurlyBracket));
        if next && !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// ⌘S: scratch requests ask where to save; saved ones flush to disk.
    pub fn save_active(&mut self) {
        if let Some(tab) = self.tabs.get(self.active) {
            let scratch = tab.kind.location().is_none() && tab.kind.scratch().is_some();
            if scratch {
                self.dialog = Some(Dialog::save_as(tab.id, &self.ws));
                return;
            }
        }
        // Spec files are only written on an explicit save.
        let spec = match self.tabs.get_mut(self.active).map(|t| &mut t.kind) {
            Some(TabKind::Spec(t)) => Some(t.save()),
            _ => None,
        };
        match spec {
            Some(Ok(())) => {}
            Some(Err(e)) => return self.toast(trf("Save failed: {}", &[&e]), true),
            None => self.ws.flush(),
        }
        self.toast(tr("Saved"), false);
    }

    /// Moves a scratch tab's request into a collection.
    /// Returns false (and keeps the request in the tab) when the target
    /// collection or folder does not exist any more.
    pub fn save_scratch_to(&mut self, tab_id: u64, collection: &str, folder: Option<&str>, name: &str) -> bool {
        let Some(idx) = self.tab_index(tab_id) else { return false };
        let Some(c) = self.ws.data.collection_mut(collection) else { return false };
        let Some(at) = c.children_mut(folder).map(|v| v.len()) else { return false };
        let item = self.tabs[idx].kind.scratch_mut().and_then(|s| s.take());
        let Some(mut item) = item else { return false };
        item.name = name.to_string();
        let item_id = item.id.clone();
        c.insert(folder, at, Item::Request(item));
        self.ws.mark_collection(collection);
        let loc = ItemRef { collection: collection.to_string(), item: item_id };
        if let Some(l) = self.tabs[idx].kind.location_mut() {
            *l = Some(loc);
        }
        self.sidebar.expand_to(collection, folder);
        true
    }

    pub fn new_collection(&mut self, name: &str) -> Id {
        let c = Collection::new(name);
        let id = self.ws.add_collection(c);
        self.sidebar.expanded.insert(id.clone());
        id
    }

    pub fn new_environment(&mut self, name: &str) {
        let id = self.ws.add_environment(Environment::new(name));
        self.open_environment(Some(id));
    }

    fn persist_settings(&mut self) {
        if self.settings != self.last_settings_saved {
            if let Err(e) = self.settings.save(&self.settings_path) {
                tracing::warn!("could not save settings: {e}");
            }
            self.last_settings_saved = self.settings.clone();
        }
    }

    pub fn apply_settings(&mut self, mut new: AppSettings) {
        // Passwords go to the Keychain; settings keep a reference.
        Self::vault_settings_secrets(&mut new);
        if new.script_memory_mb != self.settings.script_memory_mb {
            self.scripts = Self::script_engine(&new);
        }
        let engine_changed = new.proxy != self.settings.proxy
            || new.certificates != self.settings.certificates
            || new.max_body_in_memory_bytes != self.settings.max_body_in_memory_bytes;
        crate::i18n::set_language(new.ui.language);
        #[cfg(target_os = "macos")]
        if new.ui.language != self.settings.ui.language
            && let Some(menu) = &mut self.menu
        {
            menu.rebuild();
        }
        self.ctx.set_theme(match new.ui.theme {
            ternarywire_core::settings::ThemePreference::System => egui::ThemePreference::System,
            ternarywire_core::settings::ThemePreference::Light => egui::ThemePreference::Light,
            ternarywire_core::settings::ThemePreference::Dark => egui::ThemePreference::Dark,
        });
        crate::theme::apply(&self.ctx, new.ui.font_size);
        self.ws.set_history_limit(new.history_limit);
        self.settings = new;
        if engine_changed {
            self.http.set_config(Self::engine_config(&self.settings, &self.ws));
        }
        self.persist_settings();
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        use crate::{icons, kit, theme};
        let p = theme::pal(ui);
        let window_w = ui.ctx().content_rect().width();
        // The bar's empty space moves the window; a double-click zooms it (macOS
        // draws no title bar of its own over it).
        #[cfg(target_os = "macos")]
        {
            let bar = ui.max_rect().expand2(egui::vec2(14.0, 0.0));
            let r = ui.interact(bar, ui.scope_id().with("titlebar-drag"), egui::Sense::click_and_drag());
            if r.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if r.double_clicked() {
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
        }
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            // macOS draws the traffic lights over the left end of the bar.
            #[cfg(target_os = "macos")]
            ui.add_space(70.0);
            // Workspace switcher: app icon, workspace name, chevron.
            let name = self.ws.data.name.clone();
            let name_g = ui.painter().layout_no_wrap(name.clone(), theme::semibold(13.0), p.text);
            let (rect, ws) = ui.allocate_exact_size(
                egui::vec2(6.0 + 20.0 + 8.0 + name_g.size().x + 8.0 + 13.0 + 8.0, 30.0),
                egui::Sense::click(),
            );
            if ws.hovered() {
                ui.painter().rect_filled(rect, egui::CornerRadius::same(7), p.hover);
            }
            egui::Image::new(egui::include_image!("../assets/icon-256.png")).corner_radius(5).paint_at(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 6.0, rect.center().y - 10.0),
                    egui::vec2(20.0, 20.0),
                ),
            );
            ui.painter().galley(
                egui::pos2(rect.left() + 34.0, rect.center().y - name_g.size().y / 2.0),
                name_g,
                p.text,
            );
            ui.painter().text(
                egui::pos2(rect.right() - 14.5, rect.center().y),
                egui::Align2::CENTER_CENTER,
                icons::CARET_DOWN,
                theme::icon_font(13.0),
                p.text_faint,
            );
            let root = self.ws.root().to_path_buf();
            let ws = ws.on_hover_text(root.display().to_string());
            crate::widgets::a11y_name(&ws, &format!("{} {name}", tr("Workspace")));
            egui::Popup::menu(&ws).show(|ui| {
                ui.set_min_width(300.0);
                kit::menu_header(ui, tr("Workspace"));
                let here = tilde_path(&root);
                kit::menu_item_ex(
                    ui,
                    kit::MenuItem { checked: Some(true), label: &name, detail: Some(&here), ..Default::default() },
                );
                let recents: Vec<PathBuf> =
                    self.settings.recent_workspaces.iter().filter(|p| p.as_path() != root).cloned().collect();
                for path in recents {
                    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let detail = tilde_path(&path);
                    let item = kit::MenuItem {
                        icon: Some(icons::FOLDER_SIMPLE),
                        label: &label,
                        detail: Some(&detail),
                        ..Default::default()
                    };
                    if kit::menu_item_ex(ui, item).on_hover_text(path.display().to_string()).clicked() {
                        self.open_workspace(path, None);
                    }
                }
                kit::menu_sep(ui);
                if kit::menu_item(ui, Some(icons::FOLDER_OPEN), tr("Open workspace…"), None).clicked() {
                    self.pick_workspace();
                }
                if kit::menu_item(ui, Some(icons::FOLDER_PLUS), tr("New workspace…"), None).clicked() {
                    self.dialog = Some(Dialog::new_workspace());
                }
                let reveal =
                    if cfg!(target_os = "macos") { tr("Reveal in Finder") } else { tr("Reveal in file manager") };
                if kit::menu_item(ui, Some(icons::ARROW_SQUARE_OUT), reveal, None).clicked() {
                    let _ = open::that(self.ws.root());
                }
                // macOS has it in the app menu of the menu bar.
                #[cfg(not(target_os = "macos"))]
                if kit::menu_item(ui, Some(icons::ARROWS_CLOCKWISE), tr("Check for updates…"), None).clicked() {
                    crate::update::check(self, true);
                }
            });
            title_divider(ui);
            let new = kit::button(
                ui,
                kit::Kind::Ghost,
                kit::Size::Sm,
                tr("New"),
                kit::ButtonOpts { icon: Some(icons::PLUS), caret: true, ..Default::default() },
            );
            egui::Popup::menu(&new).show(|ui| self.new_menu(ui));
            if window_w > 1000.0
                && kit::button_ex(ui, kit::Kind::Ghost, Some(icons::DOWNLOAD_SIMPLE), tr("Import"), true).clicked()
            {
                self.dialog = Some(Dialog::import());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if kit::icon_button(ui, icons::LOCK_KEY, tr("Vault")).clicked() {
                    self.dialog = Some(Dialog::vault());
                }
                if kit::icon_button(ui, icons::COOKIE, tr("Cookies")).clicked() {
                    self.dialog = Some(Dialog::cookies());
                }
                ui.add_space(4.0);
                title_divider(ui);
                ui.add_space(4.0);
                crate::env_tab::environment_selector(self, ui);
                // Search, centered in the space that is left (8 from its
                // neighbors at least).
                let free = ui.available_width() - 6.0;
                let w = free.min(460.0);
                if w >= 150.0 {
                    ui.add_space(6.0 + (free - w) / 2.0);
                    let (rect, r) = ui.allocate_exact_size(egui::vec2(w, 30.0), egui::Sense::click());
                    let r =
                        r.on_hover_text(format!("{} (⌘K) · {} (⇧⌘P)", tr("Search requests"), tr("Command palette")));
                    crate::widgets::a11y_button(&r, tr("Search requests"));
                    let painter = ui.painter();
                    let radius = egui::CornerRadius::same(8);
                    if !ui.visuals().dark_mode {
                        let shadow = egui::Shadow {
                            offset: [0, 1],
                            blur: 2,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(10),
                        };
                        painter.add(shadow.as_shape(rect, radius));
                    }
                    let stroke = if r.hovered() { p.border_strong } else { p.border };
                    painter.rect(rect, radius, p.surface, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                    painter.text(
                        egui::pos2(rect.left() + 17.0, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        icons::MAGNIFYING_GLASS,
                        theme::icon_font(14.0),
                        p.text_faint,
                    );
                    let hint = kit::elide(
                        ui,
                        tr("Search requests, run a command…"),
                        egui::FontId::proportional(13.0),
                        p.text_faint,
                        w - 30.0 - 44.0,
                    );
                    ui.painter().galley(
                        egui::pos2(rect.left() + 30.0, rect.center().y - hint.size().y / 2.0),
                        hint,
                        p.text_faint,
                    );
                    let k = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 6.0 - 32.0, rect.center().y - 9.0),
                        egui::vec2(32.0, 18.0),
                    );
                    kit::paint_kbd(ui, k, "⌘K");
                    if r.clicked() {
                        self.dialog = Some(Dialog::palette(false));
                    }
                }
            });
        });
    }

    /// The New menu of the title bar and the tab strip's +: requests, local
    /// servers and things to organize them with.
    pub fn new_menu(&mut self, ui: &mut egui::Ui) {
        use crate::{icons, kit};
        ui.set_min_width(240.0);
        kit::menu_header(ui, tr("Requests"));
        if kit::menu_item(ui, Some(icons::GLOBE), tr("HTTP request"), Some("⌘N")).clicked() {
            self.create_new(NewItem::Request);
        }
        if kit::menu_item(ui, Some(icons::GRAPH), tr("GraphQL request"), None).clicked() {
            self.create_new(NewItem::GraphQl);
        }
        if kit::menu_item(ui, Some(icons::PLUGS_CONNECTED), tr("WebSocket"), None).clicked() {
            self.create_new(NewItem::WebSocket);
        }
        let templates = protocol_templates();
        for name in ["Socket.IO", "MQTT", "gRPC", "MCP", "AI request"] {
            if let Some(i) = templates.iter().position(|(_, n, _)| *n == name)
                && kit::menu_item(ui, Some(protocol_icon(templates[i].1)), tr(templates[i].1), None).clicked()
            {
                self.create_new(NewItem::Protocol(i));
            }
        }
        kit::menu_sep(ui);
        kit::menu_header(ui, tr("Local servers"));
        let mock = kit::menu_item_ex(
            ui,
            kit::MenuItem { icon: Some(icons::CUBE), label: tr("Mock server…"), submenu: true, ..Default::default() },
        );
        let collections: Vec<(Id, String)> =
            self.ws.data.collections.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
        egui::Popup::menu(&mock).open_memory(None).show(|ui| {
            ui.set_min_width(200.0);
            if collections.is_empty() {
                ui.label(kit::hint(ui, tr("No collections yet")));
            }
            for (id, name) in &collections {
                if kit::menu_item(ui, Some(icons::STACK), name, None).clicked() {
                    self.open_mock(id.clone());
                }
            }
        });
        if kit::menu_item(ui, Some(icons::WEBHOOKS_LOGO), tr("Webhook listener"), None).clicked() {
            self.create_new(NewItem::Webhook);
        }
        if kit::menu_item(ui, Some(icons::BROADCAST), tr("Capture traffic (proxy)"), None).clicked() {
            self.create_new(NewItem::Capture);
        }
        kit::menu_sep(ui);
        kit::menu_header(ui, tr("Organize"));
        if kit::menu_item(ui, Some(icons::STACK), tr("Collection"), None).clicked() {
            self.create_new(NewItem::Collection);
        }
        if kit::menu_item(ui, Some(icons::GLOBE_SIMPLE), tr("Environment"), None).clicked() {
            self.create_new(NewItem::Environment);
        }
        if kit::menu_item(ui, Some(icons::FLOW_ARROW), tr("Flow"), None).clicked() {
            self.create_new(NewItem::Flow);
        }
        if kit::menu_item(ui, Some(icons::FILE_CODE), tr("API spec"), None).clicked() {
            self.create_new(NewItem::Spec);
        }
    }

    /// A new unsaved request with a GraphQL body.
    pub fn new_graphql_tab(&mut self) {
        let mut req = HttpRequest { method: "POST".into(), ..Default::default() };
        req.body.mode = ternarywire_core::model::BodyMode::GraphQl;
        let item = RequestItem::new(tr("Untitled request"), req);
        let mut tab = RequestTab::scratch(item);
        tab.focus_url = true;
        self.add_tab(TabKind::Request(Box::new(tab)));
    }

    /// The icon rail at the far left: sidebar sections, local servers, console and settings.
    fn rail(&mut self, ui: &mut egui::Ui) {
        use crate::{icons, theme};
        let p = theme::pal(ui);
        let running = self.tabs.iter().any(|t| match &t.kind {
            TabKind::Mock(m) => m.server.is_some(),
            TabKind::Webhook(w) => w.is_running(),
            TabKind::Capture(c) => c.is_running(),
            _ => false,
        });
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(4.0);
            let section = |ui: &mut egui::Ui,
                           app: &mut Self,
                           tab: SidebarTab,
                           icon: &str,
                           label: &str,
                           dot: Option<egui::Color32>| {
                let on = app.settings.ui.sidebar_visible && app.sidebar_tab == tab;
                let r = rail_button(ui, icon, label, on, dot);
                crate::widgets::a11y_tab(&r, label, app.sidebar_tab == tab);
                if r.clicked() {
                    if on {
                        app.settings.ui.sidebar_visible = false;
                    } else {
                        app.sidebar_tab = tab;
                        app.settings.ui.sidebar_visible = true;
                    }
                }
            };
            section(ui, self, SidebarTab::Collections, icons::STACK, tr("Collections"), None);
            section(ui, self, SidebarTab::Environments, icons::GLOBE, tr("Environments"), None);
            section(ui, self, SidebarTab::History, icons::CLOCK_COUNTER_CLOCKWISE, tr("History"), None);
            section(ui, self, SidebarTab::Apis, icons::BRACKETS_CURLY, tr("APIs"), None);
            section(ui, self, SidebarTab::Flows, icons::FLOW_ARROW, tr("Flows"), None);
            ui.add_space(6.0);
            let (sep, _) = ui.allocate_exact_size(egui::vec2(22.0, 1.0), egui::Sense::hover());
            ui.painter().rect_filled(sep, 0.0, p.border);
            ui.add_space(6.0);
            section(ui, self, SidebarTab::Servers, icons::BROADCAST, tr("Local servers"), running.then_some(p.ok));
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if rail_button(ui, icons::GEAR_SIX, &format!("{} (⌘,)", tr("Settings")), false, None).clicked() {
                    self.dialog = Some(Dialog::settings(&self.settings));
                }
                let errors = self.console.iter().filter(|l| l.kind == ternarywire_runner::ConsoleKind::Error).count();
                let r = rail_button(
                    ui,
                    icons::TERMINAL_WINDOW,
                    &format!("{} (⌥⌘C)", tr("Console")),
                    self.console_open,
                    (errors > 0).then_some(p.err),
                );
                if r.clicked() {
                    self.console_open = !self.console_open;
                }
            });
        });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        use crate::{icons, kit, theme};
        let p = theme::pal(ui);
        let dark = ui.visuals().dark_mode;
        let mut close = None;
        let mut activate: Option<TabBarAct> = None;
        let mut reorder: Option<(usize, usize)> = None;
        let h = theme::size::TAB_STRIP;
        const TAB_H: f32 = 32.0;
        let bar = ui.available_rect_before_wrap().with_max_y(ui.cursor().top() + h);
        // Scroll the active tab into view when it changes or the window is resized.
        let in_view = self.tabs.get(self.active).map(|t| (t.id, bar.width() as u32));
        let scroll_to = in_view.filter(|v| self.tab_in_view != Some(*v)).map(|v| v.0);
        self.tab_in_view = in_view;
        let top_r = egui::CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 };
        ui.painter().rect_filled(bar, top_r, p.well);
        ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, egui::Stroke::new(1.0, p.border));
        let strip = egui::Rect::from_min_max(bar.min + egui::vec2(6.0, 0.0), bar.max - egui::vec2(6.0, 0.0));
        // Each tab's title, method or icon, unsaved state and width: its
        // content (1 px border, 11 px padding, lead, 7, title, 7, close 18,
        // 8 px padding, border) within 110–220, and when the tabs don't fit,
        // narrower in proportion down to 110 (the prototype's `.dtab` shrinks).
        let unsaved: std::collections::HashSet<u64> =
            (0..self.tabs.len()).filter(|&i| self.tab_has_unsaved_work(i)).map(|i| self.tabs[i].id).collect();
        let specs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let (mut title, badge, mut dirty) = crate::dialogs::tab_title(tab, &self.ws);
                // A request opened from History is named by its path and is
                // unsaved only once edited.
                if let TabKind::Request(t) = &tab.kind
                    && t.from_history.is_some()
                {
                    title = url_path(&title).to_string();
                    dirty = unsaved.contains(&tab.id);
                }
                let lead = match &badge {
                    Some(b) => (
                        crate::widgets::method_short(b).to_string(),
                        theme::semibold(10.5),
                        theme::method_color(b, dark),
                    ),
                    None => (crate::dialogs::tab_icon(tab).to_string(), theme::icon_font(14.0), p.text_faint),
                };
                let lead_g = ui.painter().layout_no_wrap(lead.0, lead.1, lead.2);
                let font = if i == self.active { theme::medium(12.5) } else { egui::FontId::proportional(12.5) };
                let text_w =
                    ui.painter().layout_no_wrap(title.clone(), font.clone(), egui::Color32::PLACEHOLDER).size().x;
                let w = (1.0 + 11.0 + lead_g.size().x + 7.0 + text_w + 7.0 + 18.0 + 8.0 + 1.0).clamp(110.0, 220.0);
                (title, badge, dirty, lead_g, lead.2, font, w)
            })
            .collect();
        let mut widths: Vec<f32> = specs.iter().map(|s| s.6).collect();
        let room = strip.width() - 30.0 - 22.0 - 2.0 * 2.0 - 2.0 * widths.len().saturating_sub(1) as f32;
        let mut over = widths.iter().sum::<f32>() - room;
        while over > 0.5 {
            let free: f32 = widths.iter().filter(|w| **w > 110.0).sum();
            if free <= 0.0 {
                break;
            }
            let mut left = 0.0;
            for w in widths.iter_mut().filter(|w| **w > 110.0) {
                let next = *w - over * *w / free;
                left += (110.0 - next).max(0.0);
                *w = next.max(110.0);
            }
            over = left;
        }
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(strip).layout(egui::Layout::left_to_right(egui::Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                // The tabs scroll; the new-tab button follows them and stays in view
                // when they overflow; "All open tabs" sits at the right end.
                let tabs_w = (strip.width() - 30.0 - 2.0 - 28.0).max(0.0);
                egui::ScrollArea::horizontal()
                    .id_salt("tabs")
                    .max_height(h)
                    .max_width(tabs_w)
                    .auto_shrink([true, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        // Exactly the strip's height, from its top: the 32 px tabs sit on its bottom edge.
                        ui.allocate_ui_with_layout(
                            egui::vec2(0.0, h),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    let (title, _, dirty, lead_g, lead_color, font, _) = &specs[i];
                                    let (title, dirty, w) = (title.clone(), *dirty, widths[i]);
                                    let selected = i == self.active;
                                    let title_w = w - 2.0 - 11.0 - lead_g.size().x - 7.0 - 7.0 - 18.0 - 8.0;
                                    let title_g =
                                        kit::elide(ui, &title, font.clone(), egui::Color32::PLACEHOLDER, title_w);
                                    let (slot, r) =
                                        ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click_and_drag());
                                    // The tab stands on the strip's bottom hairline.
                                    let rect = egui::Rect::from_min_max(
                                        egui::pos2(slot.left(), slot.bottom() - 1.0 - TAB_H),
                                        egui::pos2(slot.right(), slot.bottom() - 1.0),
                                    );
                                    // Its content box: below the top border.
                                    let cy = rect.top() + 1.0 + (TAB_H - 1.0) / 2.0;
                                    if selected && scroll_to == Some(tab.id) {
                                        r.scroll_to_me(None);
                                    }
                                    let hovered = ui.rect_contains_pointer(rect);
                                    let painter = ui.painter();
                                    let radius = egui::CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 };
                                    if selected {
                                        // Raised onto the canvas: borders on three sides, and
                                        // the strip's bottom line hidden under it.
                                        let body = rect.with_max_y(rect.bottom() + 1.0);
                                        painter.rect_filled(body, radius, p.canvas);
                                        painter.rect_stroke(
                                            body,
                                            radius,
                                            egui::Stroke::new(1.0, p.border),
                                            egui::StrokeKind::Inside,
                                        );
                                        painter.rect_filled(
                                            egui::Rect::from_min_max(
                                                egui::pos2(rect.left() + 1.0, rect.bottom()),
                                                egui::pos2(rect.right() - 1.0, rect.bottom() + 1.0),
                                            ),
                                            0.0,
                                            p.canvas,
                                        );
                                        painter.rect_filled(
                                            egui::Rect::from_min_max(
                                                egui::pos2(rect.left() + 11.0, rect.top()),
                                                egui::pos2(rect.right() - 11.0, rect.top() + 2.0),
                                            ),
                                            egui::CornerRadius::same(1),
                                            p.accent,
                                        );
                                    } else if hovered {
                                        painter.rect_filled(rect, radius, p.hover);
                                    }
                                    let mut x = rect.left() + 12.0;
                                    painter.galley(
                                        egui::pos2(x, cy - lead_g.size().y / 2.0),
                                        lead_g.clone(),
                                        *lead_color,
                                    );
                                    x += lead_g.size().x + 7.0;
                                    let text_color = if selected || hovered { p.text } else { p.text_muted };
                                    painter.galley(egui::pos2(x, cy - title_g.size().y / 2.0), title_g, text_color);
                                    // Close button (or the unsaved dot until hovered).
                                    let x_rect = egui::Rect::from_center_size(
                                        egui::pos2(rect.right() - 1.0 - 8.0 - 9.0, cy),
                                        egui::vec2(18.0, 18.0),
                                    );
                                    let x_resp = ui.interact(
                                        x_rect,
                                        ui.scope_id().with(("tab-close", tab.id)),
                                        egui::Sense::click(),
                                    );
                                    crate::widgets::a11y_name(&x_resp, &format!("{} {title}", tr("Close")));
                                    if dirty && !hovered {
                                        ui.painter().circle_filled(x_rect.center(), 3.5, p.accent);
                                    } else if hovered || selected {
                                        if x_resp.hovered() {
                                            ui.painter().rect_filled(x_rect, egui::CornerRadius::same(4), p.subtle);
                                        }
                                        ui.painter().text(
                                            x_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            icons::X,
                                            theme::icon_font(11.0),
                                            if x_resp.hovered() { p.text } else { p.text_faint },
                                        );
                                    }
                                    if x_resp.clicked() {
                                        close = Some(i);
                                    }
                                    let name =
                                        if dirty { format!("{title} ({})", tr("unsaved")) } else { title.clone() };
                                    crate::widgets::a11y_tab(&r, &name, selected);
                                    let r = r.on_hover_text(&title);
                                    if r.clicked() && !x_resp.hovered() {
                                        activate = Some(TabBarAct::Activate(i));
                                    }
                                    if r.middle_clicked() {
                                        close = Some(i);
                                    }
                                    if r.dragged()
                                        && let Some(pos) = ui.ctx().pointer_interact_pos()
                                    {
                                        if pos.x > r.rect.right() + 4.0 && i + 1 < self.tabs.len() {
                                            reorder = Some((i, i + 1));
                                        } else if pos.x < r.rect.left() - 4.0 && i > 0 {
                                            reorder = Some((i, i - 1));
                                        }
                                    }
                                    r.context_menu(|ui| {
                                        if kit::menu_item(ui, Some(icons::X), tr("Close"), Some("⌘W")).clicked() {
                                            close = Some(i);
                                        }
                                        if kit::menu_item(ui, Some(icons::X_CIRCLE), tr("Close other tabs"), None)
                                            .clicked()
                                        {
                                            activate = Some(TabBarAct::CloseOthers(i));
                                        }
                                    });
                                }
                            },
                        );
                    });
                // New tab: 30 x 32, centered on the strip above its hairline.
                let (slot, _) = ui.allocate_exact_size(egui::vec2(30.0, h), egui::Sense::hover());
                let plus = egui::Rect::from_center_size(
                    egui::pos2(slot.center().x, slot.top() + (h - 1.0) / 2.0),
                    egui::vec2(30.0, 32.0),
                );
                let tip = format!("{} (⌘T)", tr("New Request"));
                let r = ui.interact(plus, ui.scope_id().with("tab-new"), egui::Sense::click()).on_hover_text(&tip);
                crate::widgets::a11y_button(&r, &tip);
                if r.hovered() {
                    ui.painter().rect_filled(plus, egui::CornerRadius::same(7), p.hover);
                }
                ui.painter().text(
                    plus.center(),
                    egui::Align2::CENTER_CENTER,
                    icons::PLUS,
                    theme::icon_font(13.0),
                    if r.hovered() { p.text } else { p.text_muted },
                );
                if r.clicked() {
                    activate = Some(TabBarAct::New);
                }
                // Without tabs (the start page) the strip has only the +.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.tabs.is_empty() {
                        return;
                    }
                    // Centered above the strip's bottom hairline.
                    ui.set_max_height(h - 1.0);
                    let r = kit::icon_button_sm(ui, icons::CARET_DOWN, tr("All open tabs"));
                    egui::Popup::menu(&r).show(|ui| {
                        ui.set_min_width(240.0);
                        kit::menu_header(ui, tr("Open tabs"));
                        for (i, tab) in self.tabs.iter().enumerate() {
                            let (title, badge, _) = crate::dialogs::tab_title(tab, &self.ws);
                            let icon = crate::dialogs::tab_icon(tab);
                            let item = kit::MenuItem {
                                method: badge.as_deref(),
                                icon: if badge.is_none() { Some(icon) } else { None },
                                label: &title,
                                selected: i == self.active,
                                ..Default::default()
                            };
                            if kit::menu_item_ex(ui, item).clicked() {
                                activate = Some(TabBarAct::Activate(i));
                            }
                        }
                        if !self.tabs.is_empty() {
                            kit::menu_sep(ui);
                            if kit::menu_item(ui, Some(icons::X_CIRCLE), tr("Close other tabs"), None).clicked() {
                                activate = Some(TabBarAct::CloseOthers(self.active));
                            }
                        }
                    });
                });
            },
        );
        ui.advance_cursor_after_rect(bar);
        self.apply_tab_bar(reorder, activate, close);
    }

    /// Applies what the tab bar asked for after drawing it.
    fn apply_tab_bar(&mut self, reorder: Option<(usize, usize)>, activate: Option<TabBarAct>, close: Option<usize>) {
        if let Some((a, b)) = reorder {
            self.tabs.swap(a, b);
            if self.active == a {
                self.active = b;
            } else if self.active == b {
                self.active = a;
            }
        }
        match activate {
            Some(TabBarAct::New) => self.new_request_tab(),
            Some(TabBarAct::CloseOthers(keep)) => {
                let keep_id = self.tabs[keep].id;
                let ids: Vec<usize> = (0..self.tabs.len()).rev().filter(|&j| self.tabs[j].id != keep_id).collect();
                // Tabs with unsaved work stay open.
                let mut kept = 0;
                for j in ids {
                    if self.tab_has_unsaved_work(j) {
                        kept += 1;
                    } else {
                        self.close_tab(j);
                    }
                }
                self.active = self.tab_index(keep_id).unwrap_or(0);
                if kept > 0 {
                    self.toast(trf("{} tabs with unsaved changes were kept open.", &[&kept]), false);
                }
            }
            Some(TabBarAct::Activate(i)) => self.active = i,
            None => {}
        }
        if let Some(i) = close {
            self.request_close_tab(i);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        use crate::{icons, kit, theme};
        let p = theme::pal(ui);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if status_button(ui, Some(icons::SIDEBAR_SIMPLE), "", None, tr("Toggle sidebar (⌘B)")).clicked() {
                self.settings.ui.sidebar_visible = !self.settings.ui.sidebar_visible;
            }
            let errors = self.console.iter().filter(|l| l.kind == ternarywire_runner::ConsoleKind::Error).count();
            let count = (errors > 0).then(|| errors.to_string());
            let r = status_button_ex(ui, Some(icons::TERMINAL_WINDOW), tr("Console"), None, "⌥⌘C", count.as_deref());
            if r.clicked() {
                self.console_open = !self.console_open;
            }
            if let Some(e) = &self.ws.last_error {
                let text = trf("Save failed: {}", &[&e]);
                status_button(ui, Some(icons::WARNING_CIRCLE), &text, Some(p.err), "");
            } else if self.ws.has_pending() {
                status_button(ui, Some(icons::CIRCLE_NOTCH), tr("Saving…"), None, "");
            } else {
                status_label(ui, icons::CHECK, p.ok, tr("All changes saved"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if status_button(ui, Some(icons::COMMAND), "", None, &format!("{} (⇧⌘P)", tr("Command palette")))
                    .clicked()
                {
                    self.dialog = Some(Dialog::palette(false));
                }
                let side = self.settings.ui.layout == ternarywire_core::settings::ResponseLayout::Side;
                let (icon, label) =
                    if side { (icons::COLUMNS, tr("Side by side")) } else { (icons::ROWS, tr("Stacked")) };
                if status_button(ui, Some(icon), label, None, tr("Toggle response position")).clicked() {
                    self.settings.ui.layout = if side {
                        ternarywire_core::settings::ResponseLayout::Below
                    } else {
                        ternarywire_core::settings::ResponseLayout::Side
                    };
                }
                let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, p.border);
                let root = self.ws.root().to_path_buf();
                let path = tilde_path(&root);
                // At most a third of the bar, cut from the left so the folder name stays.
                let max_chars = ((ui.available_width() / 3.0) / 6.5) as usize;
                let shown = if path.chars().count() > max_chars.max(12) {
                    let tail: String =
                        path.chars().rev().take(max_chars.max(12) - 1).collect::<Vec<_>>().into_iter().rev().collect();
                    format!("…{tail}")
                } else {
                    path
                };
                status_button(ui, Some(icons::FOLDER_SIMPLE), &shown, None, &root.display().to_string());
                crate::update::status_pill(self, ui);
            });
        });
        let _ = kit::size::STATUS_BAR;
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        use crate::{icons, theme};
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(if t.error { 8 } else { 3 }));
        if self.toasts.is_empty() {
            return;
        }
        let p = theme::palette(ctx.global_style().visuals.dark_mode);
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -42.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                for t in &self.toasts {
                    let (icon, color) =
                        if t.error { (icons::WARNING_CIRCLE, p.err) } else { (icons::CHECK_CIRCLE, p.ok) };
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .shadow(ui.visuals().popup_shadow)
                        .corner_radius(egui::CornerRadius::same(theme::RADIUS_LG))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                ui.label(egui::RichText::new(icon).font(theme::icon_font(16.0)).color(color));
                                ui.add(egui::Label::new(egui::RichText::new(&t.text).size(13.0).color(p.text)).wrap());
                            });
                        });
                    ui.add_space(8.0);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    /// The start page when no tab is open: quick starts, other protocols,
    /// shortcuts and recently sent requests (the prototype's `home`).
    fn home(&mut self, ui: &mut egui::Ui) {
        use crate::{icons, kit, theme};
        let p = theme::pal(ui);
        let dark = ui.visuals().dark_mode;
        let tall = ui.ctx().content_rect().height() >= 760.0;
        enum Go {
            Http,
            Import,
            Collection,
            Flow,
            Ws,
            Proto(&'static str),
            Settings,
            History,
            Open(String),
        }
        let mut go = None;
        egui::ScrollArea::vertical().id_salt("home").auto_shrink([false; 2]).show(ui, |ui| {
            let avail = ui.available_width();
            let w = (avail - 48.0).min(732.0);
            let side = ((avail - w) / 2.0).max(24.0);
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: side as i8,
                    right: side as i8,
                    top: if tall { 54 } else { 28 },
                    bottom: 40,
                })
                .show(ui, |ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                    ui.vertical_centered(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
                        let glow =
                            egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: theme::soft(p.accent, 0.25) };
                        ui.painter().add(glow.as_shape(rect, egui::CornerRadius::same(13)));
                        // A CSS shadow is not drawn under its box: the icon's
                        // transparent margin shows the canvas.
                        ui.painter().rect_filled(rect, egui::CornerRadius::same(13), p.canvas);
                        egui::Image::new(egui::include_image!("../assets/icon-256.png"))
                            .corner_radius(13)
                            .paint_at(ui, rect);
                        // Lines 1.45 high, as the prototype's CSS line boxes.
                        ui.add_space(14.0);
                        ui.label(
                            egui::RichText::new("TernaryWire")
                                .font(theme::semibold(24.0))
                                .color(p.text)
                                .extra_letter_spacing(-0.015 * 24.0)
                                .line_height(Some(24.0 * 1.45)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(tr("A fast, local-first API client. Start with one of these:"))
                                .size(13.0)
                                .color(p.text_muted)
                                .line_height(Some(13.0 * 1.45)),
                        );
                    });
                    ui.add_space(26.0);
                    // Four quick starts.
                    let big = [
                        (icons::GLOBE, tr("HTTP request"), tr("REST, GraphQL, SOAP"), Some("⌘N"), 0u8),
                        (icons::DOWNLOAD_SIMPLE, tr("Import"), tr("Postman, OpenAPI, cURL, …"), None, 1),
                        (icons::STACK, tr("New Collection"), tr("Organize requests"), None, 2),
                        (icons::FLOW_ARROW, tr("New flow"), tr("Chain requests visually"), None, 3),
                    ];
                    let cols = if w > 560.0 { 4 } else { 2 };
                    let cw = (w - 10.0 * (cols as f32 - 1.0)) / cols as f32;
                    for chunk in big.chunks(cols) {
                        ui.horizontal(|ui| {
                            for (icon, title, sub, keys, k) in chunk {
                                if home_card(ui, cw, icon, title, sub, *keys).clicked() {
                                    go = Some(match k {
                                        0 => Go::Http,
                                        1 => Go::Import,
                                        2 => Go::Collection,
                                        _ => Go::Flow,
                                    });
                                }
                            }
                        });
                        ui.add_space(10.0);
                    }
                    ui.add_space(8.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(w, 12.0 * 1.45),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_height(12.0 * 1.45);
                            ui.label(kit::eyebrow(ui, tr("Connect with another protocol")));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(kit::hint(
                                    ui,
                                    tr("Every request type has its own tab, message log and scripts"),
                                ));
                            });
                        },
                    );
                    ui.add_space(10.0);
                    let protos: [(&str, &str, &str, Option<&'static str>); 6] = [
                        ("WS", tr("WebSocket"), tr("Messages both ways"), None),
                        ("SIO", "Socket.IO", tr("Emit and listen to events"), Some("Socket.IO")),
                        ("MQTT", "MQTT", tr("Publish and subscribe"), Some("MQTT")),
                        ("gRPC", "gRPC", tr("Unary and streaming calls"), Some("gRPC")),
                        ("MCP", "MCP", tr("Call a server's tools"), Some("MCP")),
                        ("AI", tr("AI request"), tr("Chat with a model"), Some("AI request")),
                    ];
                    let pcols = if w > 560.0 { 3 } else { 2 };
                    let pw = (w - 10.0 * (pcols as f32 - 1.0)) / pcols as f32;
                    for chunk in protos.chunks(pcols) {
                        ui.horizontal(|ui| {
                            for (badge, title, sub, proto) in chunk {
                                if protocol_card(ui, pw, badge, title, sub, dark).clicked() {
                                    go = Some(match proto {
                                        None => Go::Ws,
                                        Some(n) => Go::Proto(n),
                                    });
                                }
                            }
                        });
                        ui.add_space(10.0);
                    }
                    ui.add_space(16.0);
                    // Shortcuts and recent requests.
                    let half = (w - 28.0) / 2.0;
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 28.0;
                        ui.vertical(|ui| {
                            ui.set_width(half);
                            if home_heading(ui, tr("Keyboard shortcuts"), tr("All shortcuts")) {
                                go = Some(Go::Settings);
                            }
                            for (keys, what) in [
                                ("⌘N", tr("New request")),
                                ("⌘↩", tr("Send request")),
                                ("⌘K", tr("Search requests")),
                                ("⌘E", tr("Switch environment")),
                                ("⇧⌘P", tr("Command palette")),
                                ("⌘B", tr("Toggle sidebar")),
                            ] {
                                ui.add_space(4.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(half, 30.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_height(30.0);
                                        ui.label(egui::RichText::new(what).size(13.0).color(p.text_muted));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.spacing_mut().item_spacing.x = 2.0;
                                            for c in keys.chars().rev() {
                                                kit::kbd(ui, &c.to_string());
                                            }
                                        });
                                    },
                                );
                            }
                        });
                        ui.vertical(|ui| {
                            ui.set_width(half);
                            if home_heading(ui, tr("Recent requests"), tr("History")) {
                                go = Some(Go::History);
                            }
                            let recent: Vec<_> = self.ws.history.iter().take(6).collect();
                            if recent.is_empty() {
                                ui.add_space(6.0);
                                ui.label(kit::hint(ui, tr("Requests you send appear here.")));
                            }
                            for h in recent {
                                ui.add_space(4.0);
                                if recent_row(ui, h, dark).clicked() {
                                    go = Some(Go::Open(h.id.clone()));
                                }
                            }
                        });
                    });
                    ui.add_space(26.0);
                    // "Workspace", the name (medium, text color) and the rest,
                    // 6 apart, centered (the prototype's flex row).
                    ui.add_space(4.0);
                    let n = self.ws.data.collections.len();
                    let rest = format!(
                        "· {} · {} · {}",
                        tilde_path(self.ws.root()),
                        trf("{} collections", &[&n]),
                        tr("plain JSON files you can commit")
                    );
                    let mut job = egui::text::LayoutJob::default();
                    let muted = egui::text::TextFormat::simple(egui::FontId::proportional(12.0), p.text_muted);
                    job.append(
                        icons::FOLDER_SIMPLE,
                        0.0,
                        egui::text::TextFormat::simple(theme::icon_font(12.0), p.text_muted),
                    );
                    job.append(tr("Workspace"), 6.0, muted.clone());
                    job.append(&self.ws.data.name, 6.0, egui::text::TextFormat::simple(theme::medium(12.0), p.text));
                    job.append(&rest, 6.0, muted);
                    ui.vertical_centered(|ui| ui.label(job));
                });
        });
        match go {
            Some(Go::Http) => self.new_request_tab(),
            Some(Go::Import) => self.dialog = Some(Dialog::import()),
            Some(Go::Collection) => self.dialog = Some(Dialog::prompt_new_collection()),
            Some(Go::Flow) => self.create_new(NewItem::Flow),
            Some(Go::Ws) => self.new_ws_tab(),
            Some(Go::Proto(name)) => {
                if let Some((_, n, k)) = protocol_templates().into_iter().find(|(_, n, _)| *n == name) {
                    self.new_proto_tab(n, k);
                }
            }
            Some(Go::Settings) => self.dialog = Some(Dialog::settings(&self.settings)),
            Some(Go::History) => {
                self.sidebar_tab = SidebarTab::History;
                self.settings.ui.sidebar_visible = true;
            }
            Some(Go::Open(id)) => self.open_history(&id),
            None => {}
        }
    }

    /// Opens a history entry in a new unsaved tab.
    pub fn open_history(&mut self, id: &str) {
        let Some(h) = self.ws.history.iter().find(|h| h.id == id).cloned() else { return };
        let name = h.name.clone().unwrap_or_else(|| h.request.url.clone());
        let item = RequestItem::new(name, h.request.clone());
        let mut tab = RequestTab::scratch(item);
        // Shows "Opened from History · <time> · Save to keep it" with the last status.
        tab.from_history = Some((h.timestamp, h.status));
        let tab = self.add_tab(TabKind::Request(Box::new(tab)));
        self.sidebar.history_tabs.insert(tab, h.id.clone());
    }

    /// The workbench card: the tab strip, the active view and the console
    /// drawer on one rounded card above the chrome. Views get the area under
    /// the tab strip with no margin and draw their own rows (`kit::bar`).
    fn bench(&mut self, ui: &mut egui::Ui) {
        use crate::theme::{RADIUS_LG, size};
        let p = crate::theme::pal(ui);
        let card = ui.available_rect_before_wrap();
        let radius = egui::CornerRadius::same(RADIUS_LG);
        if !ui.visuals().dark_mode {
            let shadow = egui::Shadow {
                offset: [0, 1],
                blur: 2,
                spread: 0,
                color: egui::Color32::from_rgba_unmultiplied(20, 20, 30, 10),
            };
            ui.painter().add(shadow.as_shape(card, radius));
        }
        ui.painter().rect_filled(card, radius, p.canvas);
        let inner = card.shrink(1.0);
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(inner);
                ui.spacing_mut().item_spacing.y = 0.0;
                self.tab_bar(ui);
                let rest = ui.available_rect_before_wrap();
                const CONSOLE_H: f32 = 210.0;
                let console_h =
                    if self.console_open { (CONSOLE_H + size::GUTTER).min(rest.height() * 0.6) } else { 0.0 };
                let view = egui::Rect::from_min_max(rest.min, egui::pos2(rest.right(), rest.bottom() - console_h));
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(view).layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        ui.set_clip_rect(view.intersect(inner));
                        ui.set_min_size(view.size());
                        ui.spacing_mut().item_spacing.y = 6.0;
                        self.central(ui);
                    },
                );
                if self.console_open {
                    let drawer = egui::Rect::from_min_max(egui::pos2(rest.left(), view.bottom()), rest.max);
                    ui.painter().hline(
                        drawer.x_range(),
                        drawer.top() + size::GUTTER / 2.0,
                        egui::Stroke::new(1.0, p.border),
                    );
                    let body =
                        egui::Rect::from_min_max(egui::pos2(drawer.left(), drawer.top() + size::GUTTER), drawer.max);
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| {
                            ui.set_clip_rect(body.intersect(inner));
                            crate::dialogs::console_ui(self, ui);
                        },
                    );
                }
            },
        );
        ui.painter().rect_stroke(card, radius, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            self.home(ui);
            return;
        }
        let idx = self.active.min(self.tabs.len() - 1);
        self.active = idx;
        let tab_id = self.tabs[idx].id;
        // Temporarily take the tab out so it can borrow the app mutably.
        let mut tab = std::mem::replace(
            &mut self.tabs[idx],
            Tab { id: 0, kind: TabKind::Environment(crate::env_tab::EnvTab::default()) },
        );
        // Widget state (text-edit undo history, bulk edit, revealed secrets,
        // scroll positions) is kept per tab.
        ui.push_id(("tab", tab_id), |ui| match &mut tab.kind {
            TabKind::Request(t) => crate::request_tab::show(self, ui, tab_id, t),
            TabKind::WebSocket(t) => crate::ws_tab::show(self, ui, tab_id, t),
            TabKind::Container(t) => crate::container_tab::show(self, ui, t),
            TabKind::Environment(t) => crate::env_tab::show(self, ui, t),
            TabKind::Runner(t) => crate::runner_tab::show(self, ui, tab_id, t),
            TabKind::Mock(t) => crate::mock_tab::show(self, ui, tab_id, t),
            TabKind::Spec(t) => crate::spec_tab::show(self, ui, t),
            TabKind::Mqtt(t) => crate::mqtt_tab::show(self, ui, tab_id, t),
            TabKind::Grpc(t) => crate::grpc_tab::show(self, ui, tab_id, t),
            TabKind::SocketIo(t) => crate::sio_tab::show(self, ui, tab_id, t),
            TabKind::Mcp(t) => crate::mcp_tab::show(self, ui, tab_id, t),
            TabKind::Ai(t) => crate::ai_tab::show(self, ui, tab_id, t),
            TabKind::Webhook(t) => crate::webhook_tab::show(self, ui, tab_id, t),
            TabKind::Flow(t) => crate::flow_tab::show(self, ui, tab_id, t),
            TabKind::Capture(t) => crate::capture_tab::show(self, ui, tab_id, t),
        });
        self.put_back(tab);
    }

    /// Puts the tab [`central`](Self::central) showed back in place of its
    /// placeholder. It may have been moved meanwhile, or closed: then it is
    /// closed itself, so what it runs stops (only the placeholder was).
    fn put_back(&mut self, tab: Tab) {
        match self.tabs.iter_mut().find(|t| t.id == 0) {
            Some(slot) => *slot = tab,
            None => {
                let active = self.active;
                self.tabs.push(tab);
                self.close_tab(self.tabs.len() - 1);
                self.active = active.min(self.tabs.len().saturating_sub(1));
            }
        }
    }
}

impl eframe::App for TernaryWireApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        #[cfg(target_os = "macos")]
        self.run_menu_commands(ctx, raw_input);
        self.axis_lock.apply(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(target_os = "macos")]
        crate::macos_titlebar::center_traffic_lights(_frame, crate::theme::size::TITLE_BAR as f64);
        self.handle_msgs();
        self.shortcuts(&ctx);
        crate::automation::tick(self, &ctx);

        let p = crate::theme::pal(ui);
        let chrome = |m: egui::Margin| egui::Frame::new().fill(p.chrome).inner_margin(m);
        use crate::theme::size;
        // No separator lines: the workbench card's border is the only edge
        // between the chrome and the content (as in the prototype).
        egui::Panel::top("top")
            .exact_size(size::TITLE_BAR)
            .show_separator_line(false)
            .frame(chrome(egui::Margin { left: 14, right: 10, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                self.top_bar(ui);
            });
        egui::Panel::bottom("status")
            .exact_size(size::STATUS_BAR)
            .show_separator_line(false)
            .frame(chrome(egui::Margin { left: 8, right: 10, top: 0, bottom: 0 }))
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("rail")
            .exact_size(size::RAIL)
            .resizable(false)
            .show_separator_line(false)
            .frame(chrome(egui::Margin::ZERO))
            .show(ui, |ui| self.rail(ui));
        let mut sidebar_visible = self.settings.ui.sidebar_visible;
        // About a fifth of the window, so small windows keep room for the editor.
        let width = ui.available_width() + size::RAIL;
        let sidebar_default = (width * 0.21).clamp(224.0, 276.0).round();
        let sidebar_max = (width * 0.28).clamp(240.0, 520.0);
        egui::Panel::left("sidebar")
            .resizable(true)
            .show_separator_line(false)
            .default_size(sidebar_default)
            .size_range(200.0..=sidebar_max)
            .frame(chrome(egui::Margin { left: 2, right: 8, top: 2, bottom: 8 }))
            .show_collapsible(ui, &mut sidebar_visible, |ui| crate::sidebar::show(self, ui));
        self.settings.ui.sidebar_visible = sidebar_visible;
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(p.chrome).inner_margin(egui::Margin {
                left: 0,
                right: 8,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| self.bench(ui));
        for action in std::mem::take(&mut self.deferred) {
            match action {
                Deferred::Send(id) => self.send(id),
                Deferred::RerunTests(id) => self.rerun_tests(id),
            }
        }

        crate::dialogs::show(self, &ctx);
        crate::update::poll_requests(self, &ctx);
        crate::update::dialog(self, &ctx);
        self.toasts_ui(&ctx);

        // Autosave and periodic upkeep.
        if let Some(wait) = self.ws.autosave() {
            ctx.request_repaint_after(wait);
        }
        let running = self.tabs.iter().any(|t| match &t.kind {
            TabKind::Request(r) => r.running.is_some(),
            TabKind::Runner(r) => r.is_running(),
            TabKind::Mock(m) => m.server.is_some(),
            TabKind::Webhook(w) => w.is_running(),
            TabKind::Ai(a) => a.is_running(),
            TabKind::Flow(f) => f.is_running(),
            TabKind::Capture(c) => c.is_running(),
            _ => false,
        });
        if running {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        // Crash recovery: the open tabs (unsaved ones included) are written
        // shortly after input, not only on exit.
        if ctx.input(|i| !i.events.is_empty()) {
            self.mark_session_dirty();
        }
        if let Some(since) = self.session_dirty {
            let wait = Duration::from_secs(2).saturating_sub(since.elapsed());
            if wait.is_zero() {
                self.session_dirty = None;
                crate::dialogs::save_session(self);
            } else {
                ctx.request_repaint_after(wait);
            }
        }
        self.persist_settings();
    }

    #[cfg(feature = "glow")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown();
    }

    #[cfg(not(feature = "glow"))]
    fn on_exit(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
impl TernaryWireApp {
    /// An app on a new workspace in `root` (in-memory secrets, no settings
    /// file), for tests.
    pub fn for_test(root: &std::path::Path) -> TernaryWireApp {
        let mut settings = AppSettings::default();
        settings.proxy.use_system = false;
        let ws = WsState::for_test(root);
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("runtime");
        let http = Arc::new(Self::engine(&settings, &ws));
        let (tx, rx) = mpsc::channel();
        TernaryWireApp {
            rt,
            last_settings_saved: settings.clone(),
            settings,
            settings_path: root.join("settings.json"),
            ws,
            ws_gen: 0,
            http,
            scripts: Arc::new(ternarywire_script::BoaEngine::new()),
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            sidebar_tab: SidebarTab::Collections,
            sidebar: Default::default(),
            console: Vec::new(),
            console_open: false,
            console_filter: Default::default(),
            tx,
            rx,
            dialog: None,
            toasts: Vec::new(),
            ctx: egui::Context::default(),
            pending_tokens: Default::default(),
            automation: None,
            pending_runner_start: None,
            last_session: Vec::new(),
            session_dirty: None,
            deferred: Vec::new(),
            scratch_baseline: Default::default(),
            close_after_save: None,
            update: Default::default(),
            tab_in_view: None,
            axis_lock: Default::default(),
            #[cfg(target_os = "macos")]
            menu: None,
        }
    }

    /// Whether the automation script (if any) has run all its steps.
    pub fn automation_finished(&self) -> bool {
        self.automation.as_ref().is_none_or(crate::automation::Automation::finished)
    }

    /// Draws the active tab and the open dialog for one frame (1280×820,
    /// headless) with `events`, for tests.
    pub fn draw_frame(&mut self, events: Vec<egui::Event>) -> DrawnFrame {
        let ctx = self.ctx.clone();
        ctx.enable_accesskit();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 820.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default_margins().show(ui, |ui| self.central(ui));
            crate::dialogs::show(self, ui.ctx());
        });
        let nodes = out.platform_output.accesskit_update.take().map(|u| u.nodes).unwrap_or_default();
        let labels = nodes.iter().filter_map(|(_, n)| n.label().or(n.value()).map(str::to_string)).collect();
        let shapes = out.shapes.len();
        out.drop_without_applying_deltas();
        DrawnFrame { shapes, labels }
    }
}

/// What [`TernaryWireApp::draw_frame`] drew: the number of shapes and the
/// names (or texts) screen readers get.
#[cfg(test)]
pub struct DrawnFrame {
    pub shapes: usize,
    pub labels: Vec<String>,
}

impl TernaryWireApp {
    /// Writes the flow edits the autosave has not written yet.
    fn flush_flows(&mut self) {
        for tab in &mut self.tabs {
            if let TabKind::Flow(f) = &mut tab.kind
                && f.flush(self.ws.root())
            {
                self.sidebar.flows_dirty = true;
            }
        }
    }

    fn shutdown(&mut self) {
        self.flush_flows();
        crate::dialogs::save_session(self);
        self.ws.flush_blocking();
        self.persist_settings();
        for tab in &self.tabs {
            if let TabKind::Mock(m) = &tab.kind {
                m.stop(&self.rt);
            }
        }
    }
}

/// `~/…` for paths in the home folder (the title bar, status bar and start page).
pub fn tilde_path(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_ref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// A 38 px rail button: on = raised surface with an accent icon; `dot` marks
/// state (a running server, console errors).
/// The path of a URL (`{{baseUrl}}/books?…` or `https://host/books?…` →
/// `/books?…`); other text as it is.
fn url_path(url: &str) -> &str {
    let rest = if let Some(r) = url.strip_prefix("{{") {
        r.find("}}").map(|i| &r[i + 2..])
    } else {
        url.split_once("://").and_then(|(_, r)| r.find('/').map(|i| &r[i..]))
    };
    rest.filter(|r| r.starts_with('/')).unwrap_or(url)
}

/// The 1 px divider between groups of the title bar (18 high).
fn title_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 18.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, crate::theme::pal(ui).border);
}

fn rail_button(ui: &mut egui::Ui, icon: &str, label: &str, on: bool, dot: Option<egui::Color32>) -> egui::Response {
    let p = crate::theme::pal(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::click());
    let r = r.on_hover_text(label);
    crate::widgets::a11y_button(&r, label);
    let radius = egui::CornerRadius::same(9);
    let painter = ui.painter();
    if on {
        if !ui.visuals().dark_mode {
            let shadow =
                egui::Shadow { offset: [0, 1], blur: 2, spread: 0, color: egui::Color32::from_black_alpha(10) };
            painter.add(shadow.as_shape(rect, radius));
        }
        // The ring sits outside the 38 px button (`box-shadow: 0 0 0 1px`);
        // the dark prototype has none (its shadow list is invalid there).
        let ring = if ui.visuals().dark_mode { egui::Stroke::NONE } else { egui::Stroke::new(1.0, p.border) };
        painter.rect(rect, radius, p.surface, ring, egui::StrokeKind::Outside);
    } else if r.hovered() {
        painter.rect_filled(rect, radius, p.hover);
    }
    let color = if on {
        p.accent_text
    } else if r.hovered() {
        p.text
    } else {
        p.text_muted
    };
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, icon, crate::theme::icon_font(19.0), color);
    if let Some(c) = dot {
        let at = egui::pos2(rect.right() - 10.5, rect.top() + 10.5);
        painter.circle_filled(at, 5.5, p.chrome);
        painter.circle_filled(at, 3.5, c);
    }
    r
}

/// A status bar item: 22 high, 7 px padding, icon 13, text 11.5, muted;
/// `tip` is the tooltip (and the name when there is no text).
fn status_button(
    ui: &mut egui::Ui,
    icon: Option<&str>,
    text: &str,
    color: Option<egui::Color32>,
    tip: &str,
) -> egui::Response {
    status_button_ex(ui, icon, text, color, tip, None)
}

/// [`status_button`] with a red count pill after the text (console errors).
fn status_button_ex(
    ui: &mut egui::Ui,
    icon: Option<&str>,
    text: &str,
    color: Option<egui::Color32>,
    tip: &str,
    count: Option<&str>,
) -> egui::Response {
    let p = crate::theme::pal(ui);
    let cg = count.map(|c| ui.painter().layout_no_wrap(c.to_string(), crate::theme::medium(10.5), p.err));
    let count_w = cg.as_ref().map_or(0.0, |g| 6.0 + g.size().x + 10.0);
    let ig = icon
        .map(|i| ui.painter().layout_no_wrap(i.to_string(), crate::theme::icon_font(13.0), egui::Color32::PLACEHOLDER));
    let tg = (!text.is_empty()).then(|| {
        ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(11.5), egui::Color32::PLACEHOLDER)
    });
    let w = 14.0
        + ig.as_ref().map_or(0.0, |g| g.size().x)
        + tg.as_ref().map_or(0.0, |g| g.size().x)
        + if ig.is_some() && tg.is_some() { 6.0 } else { 0.0 }
        + count_w;
    let (rect, r) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::click());
    let r = if tip.is_empty() { r } else { r.on_hover_text(tip) };
    crate::widgets::a11y_button(&r, if text.is_empty() { tip } else { text });
    if r.hovered() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(5), p.hover);
    }
    let c = color.unwrap_or(if r.hovered() { p.text } else { p.text_muted });
    let mut x = rect.left() + 7.0;
    if let Some(g) = ig {
        ui.painter().galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g.clone(), c);
        x += g.size().x + 6.0;
    }
    if let Some(g) = tg {
        x += g.size().x + 6.0;
        ui.painter().galley(egui::pos2(x - g.size().x - 6.0, rect.center().y - g.size().y / 2.0), g, c);
    }
    if let Some(g) = cg {
        let pill = egui::Rect::from_min_size(egui::pos2(x, rect.center().y - 8.0), egui::vec2(g.size().x + 10.0, 16.0));
        ui.painter().rect_filled(pill, egui::CornerRadius::same(8), p.err_soft);
        ui.painter().galley(pill.center() - g.size() / 2.0, g, egui::Color32::PLACEHOLDER);
    }
    r
}

/// A status bar label (not clickable) with a colored icon.
fn status_label(ui: &mut egui::Ui, icon: &str, icon_color: egui::Color32, text: &str) {
    let p = crate::theme::pal(ui);
    let ig = ui.painter().layout_no_wrap(icon.to_string(), crate::theme::icon_font(13.0), icon_color);
    let tg = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(11.5), p.text_muted);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(14.0 + ig.size().x + 6.0 + tg.size().x, 22.0), egui::Sense::hover());
    ui.painter().galley(egui::pos2(rect.left() + 7.0, rect.center().y - ig.size().y / 2.0), ig.clone(), icon_color);
    ui.painter().galley(
        egui::pos2(rect.left() + 13.0 + ig.size().x, rect.center().y - tg.size().y / 2.0),
        tg,
        p.text_muted,
    );
}

/// A quick-start card of the start page (the prototype's `.qc.big`).
fn home_card(ui: &mut egui::Ui, w: f32, icon: &str, title: &str, sub: &str, keys: Option<&str>) -> egui::Response {
    use crate::{kit, theme};
    let p = theme::pal(ui);
    // `.qc.big`: 1 px border, 14 padding, the 36 px tile, 10, title and
    // description lines (1.45 high), 14 padding.
    let (rect, r) = ui
        .allocate_exact_size(egui::vec2(w, 2.0 + 28.0 + 36.0 + 10.0 + 13.0 * 1.45 + 12.0 * 1.45), egui::Sense::click());
    crate::widgets::a11y_button(&r, title);
    let hovered = r.hovered();
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(11),
        if hovered { p.hover } else { p.surface },
        egui::Stroke::new(1.0, if hovered { p.border_strong } else { p.border }),
        egui::StrokeKind::Inside,
    );
    let tile = egui::Rect::from_min_size(rect.min + egui::vec2(15.0, 15.0), egui::vec2(36.0, 36.0));
    ui.painter().rect_filled(tile, egui::CornerRadius::same(9), p.accent_soft);
    ui.painter().text(tile.center(), egui::Align2::CENTER_CENTER, icon, theme::icon_font(18.0), p.accent_text);
    if let Some(keys) = keys {
        // One key cap per key, 8 apart (the card's row gap).
        let mut x = rect.right() - 15.0;
        for c in keys.chars().rev() {
            let k = c.to_string();
            let g = ui.painter().layout_no_wrap(k.clone(), egui::FontId::proportional(10.5), p.text_muted);
            let kw = g.size().x + 12.0;
            let kr = egui::Rect::from_min_size(egui::pos2(x - kw, tile.center().y - 9.0), egui::vec2(kw, 18.0));
            kit::paint_kbd(ui, kr, &k);
            x -= kw + 8.0;
        }
    }
    // Each line centered in its 1.45 line box.
    let title_top = tile.bottom() + 10.0;
    let t = kit::elide(ui, title, theme::medium(13.0), p.text, w - 30.0);
    let t_y = title_top + (13.0 * 1.45 - t.size().y) / 2.0;
    ui.painter().galley(egui::pos2(rect.left() + 15.0, t_y), t, p.text);
    let s = kit::elide(ui, sub, egui::FontId::proportional(12.0), p.text_muted, w - 30.0);
    let s_y = title_top + 13.0 * 1.45 + (12.0 * 1.45 - s.size().y) / 2.0;
    ui.painter().galley(egui::pos2(rect.left() + 15.0, s_y), s, p.text_muted);
    r
}

/// A protocol card of the start page (the prototype's `.pc`).
fn protocol_card(ui: &mut egui::Ui, w: f32, badge: &str, title: &str, sub: &str, dark: bool) -> egui::Response {
    use crate::{kit, theme};
    let p = theme::pal(ui);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(w, 46.0), egui::Sense::click());
    crate::widgets::a11y_button(&r, title);
    let hovered = r.hovered();
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(10),
        if hovered { p.hover } else { p.surface },
        egui::Stroke::new(1.0, if hovered { p.border_strong } else { p.border }),
        egui::StrokeKind::Inside,
    );
    let c = theme::method_color(badge, dark);
    let g = ui.painter().layout_no_wrap(badge.to_string(), theme::semibold(10.5), c);
    let b = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 13.0, rect.center().y - 11.0),
        // `.badge`: at least 40 wide, no padding.
        egui::vec2(g.size().x.max(40.0), 22.0),
    );
    ui.painter().rect_filled(b, egui::CornerRadius::same(6), theme::soft(c, 0.13));
    ui.painter().galley(b.center() - g.size() / 2.0, g, c);
    let x = b.right() + 10.0;
    let t = kit::elide(ui, title, theme::medium(12.5), p.text, rect.right() - x - 12.0);
    // Name and description lines (1.45 high), centered as a block.
    let top = rect.center().y - (12.5 + 11.5) * 1.45 / 2.0;
    ui.painter().galley(egui::pos2(x, top + (12.5 * 1.45 - t.size().y) / 2.0), t, p.text);
    let s = kit::elide(ui, sub, egui::FontId::proportional(11.5), p.text_muted, rect.right() - x - 12.0);
    let s_y = top + 12.5 * 1.45 + (11.5 * 1.45 - s.size().y) / 2.0;
    ui.painter().galley(egui::pos2(x, s_y), s, p.text_muted);
    r
}

/// A 28 px section heading with an accent link on the right; true when the link is clicked.
fn home_heading(ui: &mut egui::Ui, title: &str, link: &str) -> bool {
    let p = crate::theme::pal(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 28.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(28.0);
            ui.label(crate::kit::section_title(title).color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = ui.add(
                    egui::Label::new(egui::RichText::new(link).size(12.0).color(p.text_muted))
                        .sense(egui::Sense::click()),
                );
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                r.clicked()
            })
            .inner
        },
    )
    .inner
}

/// A recent request on the start page: method, path, status, time.
fn recent_row(ui: &mut egui::Ui, h: &ternarywire_core::history::HistoryEntry, dark: bool) -> egui::Response {
    use crate::{kit, theme};
    let p = theme::pal(ui);
    let full = ui.available_width();
    // The hover fill reaches 6 px past the column on each side (allocating
    // that would widen the column row after row).
    let (rect, r) = ui.allocate_exact_size(egui::vec2(full, 30.0), egui::Sense::click());
    let rect = rect.expand2(egui::vec2(6.0, 0.0));
    let url = if h.resolved_url.is_empty() { h.request.url.clone() } else { h.resolved_url.clone() };
    let r = r.on_hover_text(&url);
    crate::widgets::a11y_button(&r, &url);
    if r.hovered() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(6), p.hover);
    }
    let cy = rect.center().y;
    let m = h.request.method.as_str();
    let mg = ui.painter().layout_no_wrap(
        crate::widgets::method_short(m).to_string(),
        theme::semibold(10.5),
        theme::method_color(m, dark),
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 6.0 + 40.0 - mg.size().x, cy - mg.size().y / 2.0),
        mg,
        egui::Color32::PLACEHOLDER,
    );
    let path = url.split_once("://").map(|(_, r)| r).unwrap_or(&url);
    let path = path.find('/').map(|i| &path[i..]).unwrap_or(path);
    let (status, sc) = match (h.status, &h.error) {
        (Some(s), _) => (s.to_string(), p.tone(theme::status_tone(s)).0),
        (None, _) => (tr("Error").to_string(), p.err),
    };
    let time = chrono::DateTime::from_timestamp_millis(h.timestamp)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let tg = ui.painter().layout_no_wrap(time, egui::FontId::proportional(11.0), p.text_faint);
    let tx = rect.right() - 6.0 - tg.size().x;
    ui.painter().galley(egui::pos2(tx, cy - tg.size().y / 2.0), tg, p.text_faint);
    let sg = ui.painter().layout_no_wrap(status, theme::mono(11.0), sc);
    let sx = rect.right() - 6.0 - 36.0 - 8.0 - sg.size().x;
    ui.painter().galley(egui::pos2(sx, cy - sg.size().y / 2.0), sg, sc);
    let x = rect.left() + 6.0 + 40.0 + 8.0;
    let ug = kit::elide(ui, path, theme::mono(11.5), p.text, sx - 8.0 - x);
    ui.painter().galley(egui::pos2(x, cy - ug.size().y / 2.0), ug, p.text);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_raw_and_encoded_secrets() {
        let secrets = vec!["s3cr3t/key".to_string()];
        let url = "https://api.test/x?key=s3cr3t/key&k2=s3cr3t%2Fkey";
        assert_eq!(redact(url, &secrets), "https://api.test/x?key=•••&k2=•••");
    }

    #[test]
    fn settings_passwords_live_in_the_keychain() {
        let mut s = AppSettings::default();
        s.proxy.password = "hunter2".into();
        s.certificates.client_certs.push(ternarywire_core::settings::ClientCertificate {
            host: "api.example.com".into(),
            passphrase: "p12-pass".into(),
            ..Default::default()
        });
        assert!(TernaryWireApp::vault_settings_secrets(&mut s));
        assert_eq!(s.proxy.password, "{{vault:settings.proxy-password}}");
        assert_eq!(s.certificates.client_certs[0].passphrase, "{{vault:settings.client-cert-api.example.com}}");
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("hunter2") && !json.contains("p12-pass"), "{json}");
        assert!(!TernaryWireApp::vault_settings_secrets(&mut s));

        // Clients get the password itself, in any workspace.
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let mut new = app.settings.clone();
        new.proxy = ternarywire_core::settings::ProxySettings {
            custom_enabled: true,
            custom_url: "http://proxy.test:3128".into(),
            username: "alice".into(),
            password: "s3cret".into(),
            ..Default::default()
        };
        app.apply_settings(new);
        assert_eq!(app.settings.proxy.password, "{{vault:settings.proxy-password}}");
        assert_eq!(app.proxy_settings().password, "s3cret");
        let saved = std::fs::read_to_string(&app.settings_path).unwrap();
        assert!(!saved.contains("s3cret"), "{saved}");
    }

    #[test]
    fn slim_collection_keeps_only_the_path() {
        let mut c = Collection::new("C");
        let mut f = Folder::new("F");
        let r = RequestItem::new("R", HttpRequest::new("GET", "http://x"));
        let id = r.id.clone();
        f.items.push(Item::Request(r));
        f.items.push(Item::Request(RequestItem::new("Other", HttpRequest::new("GET", "http://y"))));
        c.items.push(Item::Folder(f));
        c.items.push(Item::Request(RequestItem::new("Root", HttpRequest::new("GET", "http://z"))));
        let slim = slim_collection(&c, &id);
        assert_eq!(slim.request_count(), 1);
        assert!(slim.find(&id).is_some());
        assert_eq!(slim.ancestors(&id).unwrap().len(), 2);
    }

    fn scratch_tab(app: &mut TernaryWireApp, name: &str) -> u64 {
        let item = RequestItem::new(name, HttpRequest::new("GET", format!("http://{name}.test")));
        app.add_tab(TabKind::Request(Box::new(RequestTab::scratch(item))))
    }

    fn tab_names(app: &TernaryWireApp) -> Vec<String> {
        app.tabs.iter().map(|t| crate::dialogs::tab_title(t, &app.ws).0).collect()
    }

    /// "Close other tabs" from the context menu of the first tab.
    #[test]
    fn close_other_tabs_keeps_the_first_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        for n in ["a", "b", "c"] {
            scratch_tab(&mut app, n);
        }
        app.apply_tab_bar(None, Some(TabBarAct::CloseOthers(0)), None);
        assert_eq!(tab_names(&app), ["a"]);
        assert_eq!(app.active, 0);
        app.apply_tab_bar(None, Some(TabBarAct::New), None);
        assert_eq!(tab_names(&app), ["a", "Untitled request"]);
    }

    #[test]
    fn closing_asks_only_when_work_would_be_lost() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let edited = scratch_tab(&mut app, "edited");
        let untouched = scratch_tab(&mut app, "untouched");
        let i = app.tab_index(edited).unwrap();
        if let TabKind::Request(t) = &mut app.tabs[i].kind
            && let Some(item) = t.scratch.as_mut()
            && let RequestKind::Http(h) = &mut item.request
        {
            h.set_url("http://edited.test/typed".into());
        }
        // An untouched new tab just closes.
        app.request_close_tab(app.tab_index(untouched).unwrap());
        assert_eq!(tab_names(&app), ["edited"]);
        assert!(app.dialog.is_none());
        // An edited one asks first and stays open.
        app.request_close_tab(app.tab_index(edited).unwrap());
        assert!(matches!(app.dialog, Some(Dialog::CloseUnsaved { tab }) if tab == edited));
        assert_eq!(tab_names(&app), ["edited"]);
        // "Close other tabs" keeps it open too.
        app.dialog = None;
        let other = scratch_tab(&mut app, "other");
        app.apply_tab_bar(None, Some(TabBarAct::CloseOthers(app.tab_index(other).unwrap())), None);
        assert_eq!(tab_names(&app), ["edited", "other"]);
        assert_eq!(app.active, app.tab_index(other).unwrap());
        // Saving it from the prompt closes it afterwards.
        let cid = app.new_collection("C");
        app.close_after_save = Some(edited);
        assert!(app.save_scratch_to(edited, &cid, None, "Edited"));
        assert!(app.tab_index(edited).is_some(), "saving alone keeps the tab");
    }

    /// Runs the global shortcuts on one key press; returns true when the key
    /// is still there for the tab afterwards.
    fn press(app: &mut TernaryWireApp, key: Key, modifiers: Modifiers) -> bool {
        let ctx = app.ctx.clone();
        let raw = egui::RawInput {
            events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
            ..Default::default()
        };
        let mut left = false;
        ctx.run_ui(raw, |ui| {
            app.shortcuts(ui.ctx());
            left = ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)));
        })
        .drop_without_applying_deltas();
        left
    }

    /// ⇧⌘↩ sends a WebSocket message; the global ⌘↩ must not swallow it.
    #[test]
    fn shift_cmd_enter_reaches_the_websocket_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        app.new_ws_tab();
        assert!(press(&mut app, Key::Enter, Modifiers::COMMAND | Modifiers::SHIFT));
        assert!(!press(&mut app, Key::Enter, Modifiers::COMMAND));
    }

    /// With Shift held the `[`/`]` keys arrive as `{`/`}` (macOS, US layout).
    #[test]
    fn shift_cmd_brackets_switch_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        for n in ["a", "b", "c"] {
            scratch_tab(&mut app, n);
        }
        assert_eq!(app.active, 2);
        press(&mut app, Key::OpenCurlyBracket, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(app.active, 1);
        press(&mut app, Key::CloseCurlyBracket, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(app.active, 2);
        press(&mut app, Key::CloseBracket, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(app.active, 0);
    }

    /// A pre-request script that copies a secret variable into a header (the
    /// usual `Bearer ' + pm.environment.get('token')`) must not put the
    /// secret into the history file; neither when the user switched
    /// environments while the request was in flight.
    #[test]
    fn history_does_not_keep_secret_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let mut env = Environment::new("Dev");
        env.variables.push(ternarywire_core::variables::Variable::secret("token", "s3cr3t-token"));
        let env = app.ws.add_environment(env);
        app.ws.set_active_environment(Some(env.clone()));
        let before = app.ws.scopes_for(None);
        app.ws.set_active_environment(None);
        let mut req = HttpRequest::new("GET", "");
        req.set_url("https://api.test/x?key=s3cr3t-token".into());
        req.headers.push(ternarywire_core::model::KeyValue::new("Authorization", "Bearer s3cr3t-token"));
        req.body = ternarywire_core::model::Body::raw_json("{\"t\": \"s3cr3t-token\"}");
        let exec = Execution {
            request: Some(req),
            sent: Some(ternarywire_http::SentRequest {
                url: "https://api.test/x?key=s3cr3t-token".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let msg = Msg::Request {
            tab: 0,
            ws: 0,
            exec: Box::new(exec),
            after: before.clone(),
            before,
            collection: None,
            env: Some(env),
            rerun: false,
        };
        app.tx.send(msg).unwrap();
        app.handle_msgs();
        let line = serde_json::to_string(&app.ws.history[0]).unwrap();
        assert!(!line.contains("s3cr3t-token"), "{line}");
    }

    /// Draws the tab area for one frame; returns the labelled widgets.
    fn frame(app: &mut TernaryWireApp, events: Vec<egui::Event>) -> Vec<(String, egui::Pos2)> {
        let ctx = app.ctx.clone();
        ctx.enable_accesskit();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 820.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default_margins().show(ui, |ui| app.central(ui));
        });
        let nodes = out.platform_output.accesskit_update.take().map(|u| u.nodes).unwrap_or_default();
        out.drop_without_applying_deltas();
        nodes
            .iter()
            .filter_map(|(_, n)| {
                let b = n.bounds()?;
                let text = n.label().or(n.value())?.to_string();
                Some((text, egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
            })
            .collect()
    }

    fn click(app: &mut TernaryWireApp, label: &str) {
        let widgets = frame(app, Vec::new());
        let Some((_, pos)) = widgets.iter().find(|(l, _)| l == label) else { panic!("no {label:?} in {widgets:?}") };
        let pos = *pos;
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(app, vec![egui::Event::PointerMoved(pos), button(true)]);
        frame(app, vec![button(false)]);
    }

    /// Widget state (bulk edit mode and text, text-edit undo history, revealed
    /// secrets) belongs to one tab: turning on bulk edit of the params in one
    /// request must not show that request's params in another tab.
    #[test]
    fn widget_state_is_kept_per_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        scratch_tab(&mut app, "a");
        scratch_tab(&mut app, "b");
        app.active = 0;
        click(&mut app, tr("Bulk edit"));
        let labels: Vec<String> = frame(&mut app, Vec::new()).into_iter().map(|(l, _)| l).collect();
        assert!(labels.iter().any(|l| l == tr("Key-value edit")), "{labels:?}");
        app.active = 1;
        frame(&mut app, Vec::new());
        let labels: Vec<String> = frame(&mut app, Vec::new()).into_iter().map(|(l, _)| l).collect();
        assert!(labels.iter().any(|l| l == tr("Bulk edit")), "{labels:?}");
        assert!(!labels.iter().any(|l| l == tr("Key-value edit")), "{labels:?}");
    }

    /// A listener that cannot start (port in use) says so; the error used to
    /// be shown only next to a selected request, which cannot exist then.
    #[test]
    fn webhook_start_error_is_shown() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let id = app.add_tab(TabKind::Webhook(Box::default()));
        app.tx
            .send(Msg::Webhook {
                tab: id,
                event: crate::webhook_tab::WebhookMsg::Started(Err("address already in use".into())),
            })
            .unwrap();
        app.handle_msgs();
        frame(&mut app, Vec::new());
        let labels: Vec<String> = frame(&mut app, Vec::new()).into_iter().map(|(l, _)| l).collect();
        assert!(labels.iter().any(|l| l.contains("address already in use")), "{labels:?}");
    }

    /// Switching workspaces stops what the old tabs were running, like
    /// closing them does (requests whose scripts would otherwise write
    /// variables and history into the new workspace, mock servers, ...).
    #[test]
    fn switching_workspace_stops_running_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(&dir.path().join("one"));
        scratch_tab(&mut app, "a");
        let cancel = CancellationToken::new();
        let TabKind::Request(t) = &mut app.tabs[0].kind else { panic!("request tab") };
        t.start_running(cancel.clone());
        let other = WsState::for_test(&dir.path().join("two"));
        let other_root = other.root().to_path_buf();
        app.switch_workspace(other, &other_root);
        assert_eq!(app.ws.root(), other_root);
        assert_eq!(tab_names(&app), ["Untitled request"]);
        assert!(cancel.is_cancelled());
    }

    /// A request cancelled by a workspace switch still reports what its
    /// scripts set and its history entry, and a token may still arrive: none
    /// of it may land in the new workspace (whose secrets would not even mask
    /// the old workspace's in the history).
    #[test]
    fn results_from_a_closed_workspace_are_dropped() {
        use ternarywire_core::script::{ScriptEvent, ScriptOutcome};
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(&dir.path().join("one"));
        let old = app.ws_gen;
        let other = WsState::for_test(&dir.path().join("two"));
        let other_root = other.root().to_path_buf();
        app.switch_workspace(other, &other_root);
        let before = VariableScopes::default();
        let mut after = before.clone();
        after.globals.insert("token".into(), "s3cr3t-token".into());
        let request = |ws: u64| Msg::Request {
            tab: 1,
            ws,
            exec: Box::new(Execution {
                request: Some(HttpRequest::new("GET", "https://api.test/x?key=s3cr3t-token")),
                ..Default::default()
            }),
            before: before.clone(),
            after: after.clone(),
            collection: None,
            env: None,
            rerun: false,
        };
        let script = crate::proto_common::ScriptRun {
            event: ScriptEvent::PreRequest,
            outcome: ScriptOutcome { variables: after.clone(), ..Default::default() },
            before: before.clone(),
            collection: None,
            env: None,
            name: "ws".into(),
            run: 0,
        };
        let token = OAuth2Token { access_token: "tok".into(), ..Default::default() };
        app.tx.send(request(old)).unwrap();
        app.tx.send(Msg::ProtoScript { tab: 1, ws: old, run: Box::new(script) }).unwrap();
        app.tx.send(Msg::Token { ws: old, owner: "req".into(), result: Ok(token) }).unwrap();
        let finished = Msg::RunFinished {
            tab: 1,
            ws: old,
            before: before.clone(),
            after: after.clone(),
            collection: "c".into(),
            env: None,
            persist: true,
        };
        app.tx.send(finished).unwrap();
        app.pump_messages();
        assert!(app.ws.history.is_empty());
        assert!(app.ws.data.globals.is_empty(), "{:?}", app.ws.data.globals);
        assert!(TokenStore::get(app.ws.secrets.as_ref(), "req").is_none());
        // What the new workspace's own tabs send still applies.
        app.tx.send(request(app.ws_gen)).unwrap();
        app.pump_messages();
        assert_eq!(app.ws.history.len(), 1);
        assert_eq!(app.ws.data.globals.len(), 1);
    }

    /// On-message scripts of a busy stream add console lines, test results
    /// and errors for every message; the console and the tab keep the latest.
    #[test]
    fn protocol_script_output_is_bounded() {
        use ternarywire_core::script::{ConsoleEntry, LogLevel, ScriptError, ScriptEvent, ScriptOutcome, TestResult};
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let id = app.add_tab(TabKind::Mqtt(Box::new(crate::mqtt_tab::MqttTab::new(Default::default()))));
        for i in 0..3000 {
            let test = TestResult { name: format!("t{i}"), passed: true, error: None, skipped: false };
            let error = ScriptError { location: "on-message".into(), message: format!("e{i}") };
            let outcome = ScriptOutcome {
                tests: vec![test.clone(), test],
                console: vec![ConsoleEntry { level: LogLevel::Log, message: format!("m{i}") }],
                errors: vec![error.clone(), error],
                ..Default::default()
            };
            let run = crate::proto_common::ScriptRun {
                event: ScriptEvent::OnMessage,
                outcome,
                before: VariableScopes::default(),
                collection: None,
                env: None,
                name: "mqtt".into(),
                run: 0,
            };
            app.tx.send(Msg::ProtoScript { tab: id, ws: 0, run: Box::new(run) }).unwrap();
        }
        app.handle_msgs();
        assert_eq!(app.console.len(), MAX_CONSOLE);
        assert_eq!(app.console.last().map(|l| l.message.as_str()), Some("on-message: e2999"));
        let s = app.tabs[0].kind.script_state_mut().unwrap();
        assert_eq!(s.tests.len(), crate::proto_common::MAX_SCRIPT_RESULTS);
        assert_eq!(s.errors.len(), crate::proto_common::MAX_SCRIPT_RESULTS);
        assert_eq!(s.tests.back().map(|t| t.name.as_str()), Some("t2999"));
        assert_eq!(s.tests.front().map(|t| t.name.as_str()), Some("t500"));
    }

    /// Typed variables (Postman `type`): a collection variable declared as a
    /// number is a number in the request's scripts, and a number a script
    /// sets is stored with its type (`any` holding 5 for a new variable, as in
    /// Postman; a declared number stays a number).
    #[test]
    fn scripts_see_and_set_typed_variables() {
        use std::io::{Read, Write};
        use ternarywire_core::variables::{VarType, Variable};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Some(mut s) = listener.incoming().flatten().next() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let mut dev = Environment::new("Dev");
        dev.variables.push(Variable { ty: VarType::Number, ..Variable::new("count", "1") });
        let env = app.ws.add_environment(dev);
        app.ws.set_active_environment(Some(env.clone()));
        let cid = app.new_collection("C");
        let mut req = RequestItem::new("typed", HttpRequest::new("GET", format!("http://127.0.0.1:{port}/")));
        req.scripts.pre_request = "pm.environment.set('seen', typeof pm.collectionVariables.get('limit'));\n\
                                   pm.environment.set('n', pm.collectionVariables.get('limit') / 2);\n\
                                   pm.environment.set('count', '7');"
            .into();
        let rid = req.id.clone();
        let c = app.ws.data.collection_mut(&cid).unwrap();
        c.variables.push(Variable { ty: VarType::Number, ..Variable::new("limit", "10") });
        c.items.push(Item::Request(req));
        app.open_item(ItemRef { collection: cid, item: rid });
        let tab = app.tabs[app.active].id;
        app.send(tab);
        let deadline = Instant::now() + Duration::from_secs(30);
        while matches!(&app.tabs[app.active].kind, TabKind::Request(t) if t.running.is_some()) {
            assert!(Instant::now() < deadline, "the request did not finish");
            std::thread::sleep(Duration::from_millis(20));
            app.handle_msgs();
        }
        let vars = &app.ws.data.environment(&env).unwrap().variables;
        let var = |k: &str| vars.iter().find(|v| v.key == k).unwrap_or_else(|| panic!("no {k} in {vars:?}"));
        assert_eq!(var("seen").effective(), "number");
        assert_eq!(var("n").effective(), "5");
        assert_eq!(var("n").ty, VarType::Any);
        assert_eq!(var("seen").ty, VarType::Default);
        assert_eq!((var("count").effective(), var("count").ty), ("7", VarType::Number));
    }

    /// Saving into a collection or folder deleted while the Save dialog was
    /// open keeps the unsaved request in its tab.
    #[test]
    fn save_as_into_a_missing_target_keeps_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let tab = scratch_tab(&mut app, "a");
        let cid = app.new_collection("C");
        assert!(!app.save_scratch_to(tab, "deleted-collection", None, "Saved"));
        assert!(!app.save_scratch_to(tab, &cid, Some("deleted-folder"), "Saved"));
        assert!(app.tabs[0].kind.scratch().is_some());
        assert!(app.tabs[0].kind.location().is_none());
        assert_eq!(app.ws.data.collection(&cid).unwrap().request_count(), 0);
        assert!(app.save_scratch_to(tab, &cid, None, "Saved"));
        assert!(app.tabs[0].kind.scratch().is_none());
        assert_eq!(tab_names(&app), ["Saved"]);
    }

    /// The running flow tab's state: (running, run number).
    fn flow_state(app: &TernaryWireApp) -> (bool, u64) {
        match &app.tabs[app.active].kind {
            TabKind::Flow(t) => (t.is_running(), t.run_epoch()),
            _ => panic!("flow tab"),
        }
    }

    /// ⌘↩ stops a running flow and ⌘↩ runs it again: the stopped run's last
    /// message must not end the new run (which then could not be stopped).
    #[test]
    fn a_stopped_flow_run_does_not_end_the_next_one() {
        use ternarywire_flows::{Block, BlockConfig, DelayConfig, Edge, Flow, FlowResult};
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let mut flow = Flow::new("slow");
        let start = flow.start().expect("a new flow starts with Start").id.clone();
        let wait = Block::new(BlockConfig::Delay(DelayConfig { ms: "60000".into() }), "Wait", (300.0, 0.0));
        let wait = flow.add_block(wait);
        flow.edges.push(Edge::new(&start, "next", &wait, "in"));
        let id = app.add_tab(TabKind::Flow(Box::new(crate::flow_tab::FlowTab::new(flow, None))));
        press(&mut app, Key::Enter, Modifiers::COMMAND);
        frame(&mut app, Vec::new());
        let (running, first) = flow_state(&app);
        assert!(running, "{:?}", app.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
        press(&mut app, Key::Enter, Modifiers::COMMAND);
        assert!(!flow_state(&app).0);
        press(&mut app, Key::Enter, Modifiers::COMMAND);
        frame(&mut app, Vec::new());
        assert!(flow_state(&app).0);
        let done = crate::flow_tab::FlowMsg::Done(Box::new(FlowResult { cancelled: true, ..Default::default() }));
        app.tx.send(Msg::Flow { tab: id, epoch: first, event: done }).unwrap();
        app.pump_messages();
        assert!(flow_state(&app).0, "the new run still runs");
        press(&mut app, Key::Enter, Modifiers::COMMAND);
        assert!(!flow_state(&app).0, "and can be stopped");
    }

    /// Tests re-run on the last response that finish while a new send is in
    /// flight must not end it: its spinner and Cancel would go, and the
    /// guard against sending twice with it.
    #[test]
    fn rerun_tests_do_not_end_a_send_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let id = scratch_tab(&mut app, "a");
        let cancel = CancellationToken::new();
        let TabKind::Request(t) = &mut app.tabs[0].kind else { panic!("request tab") };
        t.start_running(cancel.clone());
        let rerun = Msg::Request {
            tab: id,
            ws: 0,
            exec: Box::default(),
            before: VariableScopes::default(),
            after: VariableScopes::default(),
            collection: None,
            env: None,
            rerun: true,
        };
        app.tx.send(rerun).unwrap();
        app.pump_messages();
        let TabKind::Request(t) = &mut app.tabs[0].kind else { panic!("request tab") };
        assert!(t.running.is_some());
        press(&mut app, Key::Enter, Modifiers::COMMAND);
        assert!(cancel.is_cancelled(), "⌘↩ still cancels the send");
    }

    /// Flow edits are written when the tab closes or the app quits, also
    /// within the autosave's pause after an edit (they used to be lost).
    #[test]
    fn flow_edits_are_saved_on_close_and_quit() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let names = |app: &TernaryWireApp| {
            let mut names: Vec<String> =
                ternarywire_flows::storage::list(app.ws.root()).0.into_iter().map(|f| f.flow.name).collect();
            names.sort();
            names
        };
        let id = app.add_tab(TabKind::Flow(Box::new(crate::flow_tab::FlowTab::blank("Draft"))));
        app.close_tab(app.tab_index(id).unwrap());
        assert_eq!(names(&app), ["Draft"]);
        app.add_tab(TabKind::Flow(Box::new(crate::flow_tab::FlowTab::blank("Second"))));
        app.shutdown();
        assert_eq!(names(&app), ["Draft", "Second"]);
    }

    /// A tab closed while it was being shown (only its placeholder was in
    /// the tab list) is closed properly once it is back: its request stops.
    #[test]
    fn a_tab_closed_while_shown_is_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        scratch_tab(&mut app, "a");
        let id = scratch_tab(&mut app, "b");
        let cancel = CancellationToken::new();
        let i = app.tab_index(id).unwrap();
        let placeholder = Tab { id: 0, kind: TabKind::Environment(crate::env_tab::EnvTab::default()) };
        let mut shown = std::mem::replace(&mut app.tabs[i], placeholder);
        let TabKind::Request(t) = &mut shown.kind else { panic!("request tab") };
        t.start_running(cancel.clone());
        app.close_tab(i);
        app.put_back(shown);
        assert!(cancel.is_cancelled());
        assert_eq!(tab_names(&app), ["a"]);
        assert_eq!(app.active, 0);
    }
}
