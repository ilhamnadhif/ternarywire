//! Scripted UI automation for visual checks and benchmarks.
//!
//! Inactive unless `TERNARYWIRE_AUTOMATION` points to a JSON array of steps, e.g.
//! `[{"open": "List users"}, {"send": true}, {"wait_ms": 800},
//!   {"screenshot": "/tmp/a.png"}, {"quit": true}]`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::app::{ItemRef, SidebarTab, TabKind, TernaryWireApp};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Step {
    Open(String),
    /// Opens a new, empty request tab (its URL field gets the caret).
    NewRequest(bool),
    /// Shows the response body as `pretty`, `raw`, `preview` or `tree`.
    BodyView(String),
    /// Response body: opens the JSONPath filter with this path.
    BodyFilter(String),
    /// Response body: `expand`, `collapse`, `level1`, `level2`.
    BodyFold(String),
    /// Response body: searches for the text and jumps to the next match.
    BodySearch(String),
    /// Response body: scrolls the body view to `[x, y]`.
    BodyScroll([f32; 2]),
    /// Panics on the UI thread (checks the crash report).
    Panic(String),
    Send(bool),
    /// Presses the Send button of the active request tab.
    ClickSend(bool),
    WaitMs(u64),
    /// Waits until no request in the active tab is running.
    WaitIdle(bool),
    Screenshot(String),
    Section(String),
    RespSection(String),
    Sidebar(String),
    Dialog(String),
    CloseDialog(bool),
    Runner(String),
    RunnerStart(bool),
    Mock(String),
    Container(String),
    Environment(Option<String>),
    ActivateEnv(String),
    Layout(String),
    Theme(String),
    Language(String),
    Console(bool),
    Url(String),
    /// New unsaved gRPC request with this URL.
    NewGrpc(String),
    GrpcLoad(bool),
    /// `package.Service/Method`.
    GrpcMethod(String),
    GrpcInvoke(bool),
    /// Opens the gRPC method picker of the active tab.
    GrpcPicker(bool),
    /// New unsaved request from a "New …" template, by protocol name (`MCP`).
    NewProto(String),
    /// Replaces the request of the active (unsaved) tab, as `RequestKind` JSON.
    SetRequest(serde_json::Value),
    /// Sets the scripts of the active tab's unsaved request.
    SetScripts(ternarywire_core::model::Scripts),
    /// Protocol tab action: `connect`, `emit`, `send`, `start`, `select`, `call:<tool>:<json>`.
    Auto(String),
    /// New webhook listener tab on this port.
    NewWebhook(u16),
    /// New capture proxy tab on this port, started right away.
    NewCapture(u16),
    /// Opens the flow with this name (from `flows/`).
    OpenFlow(String),
    /// Runs the flow in the active tab.
    RunFlow(bool),
    /// Selects the block with this name in the active flow tab ("" clears
    /// the selection).
    FlowSelect(String),
    /// Flow tab action: `validate` (lists the problems, no toast),
    /// `add-block` (opens the Add block menu), `fit`.
    FlowAction(String),
    /// Sets options of the active runner tab: `mode` (`functional`,
    /// `performance`), `iterations`, `delay_ms`, `data_file` (relative to the
    /// workspace), `vus`, `duration_s`, `ramp_s`, `only` (request names).
    RunnerOptions(serde_json::Value),
    /// Selects the result `<request name>#<iteration>` in the active runner tab.
    RunnerSelect(String),
    /// Forks the collection with this name (as "<name> (fork)").
    Fork(String),
    /// Opens the pull (true) or merge (false) dialog of the fork with this name.
    ForkSync((String, bool)),
    /// Opens the first spec in `specs/` whose file name contains the text.
    OpenSpec(String),
    /// Spec tab: syncs the spec into the collection with this name, opens the
    /// Sync popover and makes an unsaved edit (see `SpecTab::automation`).
    SpecSync(String),
    /// Variables table: opens the menu of the row with this variable name.
    EnvRowMenu(String),
    /// Opens the menu of the title bar's environment selector.
    EnvMenu(bool),
    /// Sets a GraphQL body (scratch tabs).
    GraphqlQuery(String),
    Expand(String),
    Window([f32; 2]),
    /// Scrolls the collection tree to this offset (large values: the end).
    SidebarScroll(f32),
    /// Prints resident memory to stderr (benchmarks).
    ReportMemory(String),
    /// Presses the main button of the update pop-up (install and restart,
    /// download, or release page) once the startup check found an update
    /// (needs `TERNARYWIRE_UPDATE_URL`; waits up to 20 s).
    UpdateInstall(bool),
    /// Response panel of the active request tab (`request_tab::RequestTab::automation`):
    /// `maximize`, `minimize`, `restore`, `compare`, `compare_headers`,
    /// `generate_tests`, `add_generated_tests`, `set_var:<json path>`,
    /// `add_test:<json path>`.
    RespPanel(String),
    /// Merges this JSON into the draft of the open Settings dialog (it then
    /// shows unsaved changes), e.g. `{"proxy": {"custom_enabled": true}}`.
    SettingsDraft(serde_json::Value),
    /// Adds (or replaces) an external secrets provider, as
    /// `ternarywire_secrets::ProviderConfig` JSON:
    /// `{"id": "corp-vault", "name": "corp-vault", "kind": "hashicorp_vault", "address": "…"}`.
    VaultProvider(serde_json::Value),
    /// Closes every tab (the start page shows).
    CloseAllTabs(bool),
    /// Opens the newest History entry whose URL contains the text, as a tab.
    OpenHistory(String),
    /// Collapses a folder of the tree: `Collection/Folder`.
    Collapse(String),
    /// Opens the request named in that collection: `["Payments (fork)",
    /// "List customers"]` (`open` takes the first of that name).
    OpenIn([String; 2]),
    /// Adds an HTTP request at the top level of a collection:
    /// `["Payments", "Refund charge", "POST", "{{baseUrl}}/v1/refunds"]`.
    AddRequest([String; 4]),
    /// Deletes the request named in that collection: `["Payments", "Legacy webhook"]`.
    DeleteRequest([String; 2]),
    /// Sets (or adds) a collection variable: `["Payments", "apiVersion", "2026-11-01"]`.
    CollectionVar([String; 3]),
    /// Open pull/merge dialog: includes or leaves out the change of this name.
    ForkToggle(String),
    /// Open pull/merge dialog: shows the diff of the change of this name.
    ForkSelect(String),
    Quit(bool),
}

pub struct Automation {
    steps: VecDeque<Step>,
    wait_until: Option<Instant>,
    pending_shot: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    started: Instant,
}

impl Automation {
    pub fn from_env() -> Option<Automation> {
        let path = std::env::var("TERNARYWIRE_AUTOMATION").ok()?;
        let text = std::fs::read_to_string(&path).ok()?;
        let steps: Vec<Step> = match serde_json::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("automation: {e}");
                return None;
            }
        };
        Some(Automation { steps: steps.into(), wait_until: None, pending_shot: None, started: Instant::now() })
    }

    /// Every step has run (and the last screenshot is written).
    #[cfg(test)]
    pub fn finished(&self) -> bool {
        self.steps.is_empty() && self.wait_until.is_none() && self.pending_shot.is_none()
    }
}

fn find_request(app: &TernaryWireApp, name: &str) -> Option<ItemRef> {
    for c in &app.ws.data.collections {
        for r in c.requests() {
            if r.name == name || r.id == name {
                return Some(ItemRef { collection: c.id.clone(), item: r.id.clone() });
            }
        }
    }
    None
}

fn find_collection(app: &TernaryWireApp, name: &str) -> Option<String> {
    app.ws.data.collections.iter().find(|c| c.name == name || c.id == name).map(|c| c.id.clone())
}

/// Physical footprint as shown by Activity Monitor (macOS `footprint`).
pub fn footprint_mb() -> Option<f64> {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("footprint").arg(&pid).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("Footprint:"))?;
    let after = line.split("Footprint:").nth(1)?.trim();
    let mut parts = after.split_whitespace();
    let value: f64 = parts.next()?.parse().ok()?;
    Some(match parts.next()? {
        "KB" => value / 1024.0,
        "GB" => value * 1024.0,
        _ => value,
    })
}

pub fn resident_memory_mb() -> f64 {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

pub fn tick(app: &mut TernaryWireApp, ctx: &egui::Context) {
    let Some(mut auto) = app.automation.take() else { return };
    ctx.request_repaint_after(Duration::from_millis(30));
    // Wait for a pending screenshot to be written.
    if let Some(flag) = &auto.pending_shot {
        if !flag.load(std::sync::atomic::Ordering::SeqCst) {
            app.automation = Some(auto);
            return;
        }
        auto.pending_shot = None;
    }
    if let Some(t) = auto.wait_until {
        if Instant::now() < t {
            app.automation = Some(auto);
            return;
        }
        auto.wait_until = None;
    }
    let Some(step) = auto.steps.pop_front() else {
        app.automation = Some(auto);
        return;
    };
    app.mark_session_dirty();
    match step {
        Step::Open(name) => match find_request(app, &name) {
            Some(r) => app.open_item(r),
            None => eprintln!("automation: request {name} not found"),
        },
        Step::Send(_) => {
            if let Some(t) = app.tabs.get(app.active) {
                let id = t.id;
                app.send(id);
            }
        }
        Step::ClickSend(_) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.click_send = true;
            }
        }
        Step::WaitMs(ms) => auto.wait_until = Some(Instant::now() + Duration::from_millis(ms)),
        Step::WaitIdle(_) => {
            let busy = app.tabs.iter().any(|t| match &t.kind {
                TabKind::Request(r) => r.running.is_some(),
                TabKind::Runner(r) => r.is_running(),
                _ => false,
            });
            if busy {
                auto.steps.push_front(Step::WaitIdle(true));
                auto.wait_until = Some(Instant::now() + Duration::from_millis(50));
            }
        }
        Step::Screenshot(path) => {
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = done.clone();
            let repaint = ctx.clone();
            let cb = egui::ScreenshotCallback::new(move |image| {
                let [w, h] = image.size;
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::RgbaImage::from_raw(w as u32, h as u32, bytes).map(|b| b.save(&path)) {
                    Some(Ok(())) => eprintln!("automation: saved {path}"),
                    other => eprintln!("automation: screenshot {path} failed: {other:?}"),
                }
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                repaint.request_repaint();
            });
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
            auto.pending_shot = Some(done);
        }
        Step::Section(s) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.section = match s.as_str() {
                    "params" => crate::request_tab::Section::Params,
                    "auth" => crate::request_tab::Section::Auth,
                    "headers" => crate::request_tab::Section::Headers,
                    "body" => crate::request_tab::Section::Body,
                    "scripts" => crate::request_tab::Section::Scripts,
                    "settings" => crate::request_tab::Section::Settings,
                    _ => crate::request_tab::Section::Docs,
                };
            }
            if s == "scripts"
                && let Some(TabKind::Grpc(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
            {
                t.show_scripts();
            }
        }
        Step::RespSection(s) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.resp_section = match s.as_str() {
                    "headers" => crate::request_tab::RespSection::Headers,
                    "cookies" => crate::request_tab::RespSection::Cookies,
                    "tests" => crate::request_tab::RespSection::Tests,
                    "details" => crate::request_tab::RespSection::Details,
                    "visualize" => crate::request_tab::RespSection::Visualize,
                    _ => crate::request_tab::RespSection::Body,
                };
            }
        }
        Step::Sidebar(s) => {
            app.sidebar_tab = match s.as_str() {
                "environments" => SidebarTab::Environments,
                "history" => SidebarTab::History,
                "apis" => SidebarTab::Apis,
                "flows" => SidebarTab::Flows,
                "servers" => SidebarTab::Servers,
                _ => SidebarTab::Collections,
            }
        }
        // `settings:proxy`, `vault:providers`, `update`… (see `dialogs::open_named`).
        Step::Dialog(d) if crate::dialogs::open_named(app, &d) => {}
        Step::Dialog(d) => {
            app.dialog = Some(match d.as_str() {
                "settings" => crate::dialogs::Dialog::settings(&app.settings),
                "import" => crate::dialogs::Dialog::import(),
                "cookies" => crate::dialogs::Dialog::cookies(),
                "vault" => crate::dialogs::Dialog::vault(),
                "requests" => crate::dialogs::Dialog::palette(true),
                "quicklook" => crate::dialogs::Dialog::quick_look(),
                _ => crate::dialogs::Dialog::palette(false),
            });
        }
        Step::CloseDialog(_) => app.dialog = None,
        Step::Runner(name) => {
            if let Some(c) = find_collection(app, &name) {
                app.open_runner(c, None);
            }
        }
        Step::RunnerStart(_) => {
            if let Some(tab) = app.tabs.get(app.active)
                && matches!(tab.kind, TabKind::Runner(_))
            {
                app.pending_runner_start = Some(tab.id);
            }
        }
        Step::Mock(name) => {
            if let Some(c) = find_collection(app, &name) {
                app.open_mock(c);
            }
        }
        Step::Container(name) => {
            if let Some(c) = find_collection(app, &name) {
                app.open_container(c, None);
            }
        }
        Step::Environment(name) => {
            let id = name.and_then(|n| app.ws.data.environments.iter().find(|e| e.name == n).map(|e| e.id.clone()));
            app.open_environment(id);
        }
        Step::ActivateEnv(name) => {
            let id = app.ws.data.environments.iter().find(|e| e.name == name).map(|e| e.id.clone());
            app.ws.set_active_environment(id);
        }
        Step::Layout(l) => {
            app.settings.ui.layout = if l == "side" {
                ternarywire_core::settings::ResponseLayout::Side
            } else {
                ternarywire_core::settings::ResponseLayout::Below
            }
        }
        Step::Theme(t) => {
            let mut s = app.settings.clone();
            s.ui.theme = match t.as_str() {
                "light" => ternarywire_core::settings::ThemePreference::Light,
                "dark" => ternarywire_core::settings::ThemePreference::Dark,
                _ => ternarywire_core::settings::ThemePreference::System,
            };
            app.apply_settings(s);
        }
        Step::Language(l) => {
            let mut s = app.settings.clone();
            s.ui.language = if l == "id" {
                ternarywire_core::settings::Language::Id
            } else {
                ternarywire_core::settings::Language::En
            };
            app.apply_settings(s);
        }
        Step::Console(on) => app.console_open = on,
        Step::Url(url) => {
            let loc = app.tabs.get(app.active).and_then(|t| t.kind.location().cloned());
            if let Some(loc) = loc {
                if let Some(h) = app
                    .ws
                    .data
                    .collection_mut(&loc.collection)
                    .and_then(|c| c.find_request_mut(&loc.item))
                    .and_then(|r| r.http_mut())
                {
                    h.set_url(url);
                    app.ws.mark_collection(&loc.collection);
                }
            } else if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
                && let Some(item) = t.scratch.as_mut().and_then(|i| i.http_mut())
            {
                item.set_url(url);
            }
        }
        Step::GraphqlQuery(q) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
                && let Some(item) = t.scratch.as_mut().and_then(|i| i.http_mut())
            {
                item.method = "POST".into();
                item.body.mode = ternarywire_core::model::BodyMode::GraphQl;
                item.body.graphql.query = q;
                t.section = crate::request_tab::Section::Body;
            }
        }
        Step::OpenSpec(name) => {
            let found = crate::spec_tab::list_specs(app.ws.root())
                .into_iter()
                .find(|p| p.file_name().map(|f| f.to_string_lossy().contains(&name)).unwrap_or(false));
            if let Some(p) = found {
                crate::spec_tab::open_spec(app, p);
            }
        }
        Step::SpecSync(name) => {
            if let Some(TabKind::Spec(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.auto = Some(format!("spec_sync:{name}"));
            }
        }
        Step::EnvRowMenu(name) => crate::env_tab::automation_row_menu(&app.ctx, name),
        Step::EnvMenu(_) => crate::env_tab::automation_selector_menu(&app.ctx),
        Step::NewProto(name) => {
            if let Some((_, n, kind)) =
                crate::app::protocol_templates().into_iter().find(|(_, n, _)| n.eq_ignore_ascii_case(&name))
            {
                app.new_proto_tab(n, kind);
            }
        }
        Step::SetRequest(v) => match serde_json::from_value::<ternarywire_core::model::RequestKind>(v) {
            Ok(kind) => {
                if let Some(Some(item)) = app.tabs.get_mut(app.active).and_then(|t| t.kind.scratch_mut()) {
                    item.request = kind;
                }
            }
            Err(e) => eprintln!("automation: set_request: {e}"),
        },
        Step::SetScripts(scripts) => {
            if let Some(Some(item)) = app.tabs.get_mut(app.active).and_then(|t| t.kind.scratch_mut()) {
                item.scripts = scripts;
            }
        }
        Step::Auto(a) => match app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
            Some(TabKind::Mcp(t)) => t.auto = Some(a),
            Some(TabKind::SocketIo(t)) => t.auto = Some(a),
            Some(TabKind::Ai(t)) => t.auto = Some(a),
            Some(TabKind::Webhook(t)) => t.auto = Some(a),
            Some(TabKind::Capture(t)) if a == "select" => t.select_latest(),
            // Local servers: `start`, `stop`, `settings`, `select:<n>`, mock
            // `routes`/`logs`/`hit:<METHOD> <path> [body]`, capture `cert`.
            Some(TabKind::Capture(t)) => t.auto = Some(a),
            Some(TabKind::Mock(t)) => t.auto = Some(a),
            Some(TabKind::Spec(t)) if a == "preview" => t.show_preview(),
            Some(TabKind::Spec(t)) => t.auto = Some(a),
            Some(TabKind::Container(t)) => t.automation(&a),
            Some(TabKind::WebSocket(t)) => t.auto = Some(a),
            Some(TabKind::Mqtt(t)) => t.auto = Some(a),
            Some(TabKind::Grpc(t)) => t.auto = Some(a),
            _ => {}
        },
        Step::NewCapture(port) => {
            let mut t = crate::capture_tab::CaptureTab::with_port(port);
            t.auto_start = true;
            app.add_tab(TabKind::Capture(Box::new(t)));
        }
        Step::OpenFlow(name) => {
            let found = ternarywire_flows::storage::list(app.ws.root()).0.into_iter().find(|f| f.flow.name == name);
            if let Some(f) = found {
                crate::flow_tab::open_flow(app, f.path);
            }
        }
        Step::FlowSelect(name) => {
            if let Some(TabKind::Flow(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.select_block(&name);
            }
        }
        Step::FlowAction(a) => {
            if let Some(TabKind::Flow(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.auto = Some(a);
            }
        }
        Step::RunnerOptions(v) => {
            let root = app.ws.root().to_path_buf();
            if let Some(TabKind::Runner(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.set_options(&v, &root);
            }
        }
        Step::RunnerSelect(which) => {
            if let Some(TabKind::Runner(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.select_result(&which);
            }
        }
        Step::RunFlow(_) => {
            if let Some(TabKind::Flow(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.auto_run = true;
            }
        }
        Step::Fork(name) => {
            if let Some(c) = find_collection(app, &name) {
                crate::fork_ui::create(app, &c, &format!("{name} (fork)"));
            }
        }
        Step::ForkSync((name, pull)) => {
            if let Some(c) = find_collection(app, &name)
                && let Some(s) = crate::fork_ui::ForkSync::new(app, &c, pull)
            {
                app.dialog = Some(crate::dialogs::Dialog::ForkSync(Box::new(s)));
            }
        }
        Step::NewWebhook(port) => {
            let mut t = crate::webhook_tab::WebhookTab::default();
            t.set_port(port);
            app.add_tab(TabKind::Webhook(Box::new(t)));
        }
        Step::NewGrpc(url) => {
            let req = ternarywire_core::protocols::GrpcRequest { url, ..Default::default() };
            app.new_proto_tab("gRPC", ternarywire_core::model::RequestKind::Grpc(req));
        }
        Step::GrpcLoad(_) => {
            if let Some(TabKind::Grpc(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.auto_load = true;
            }
        }
        Step::GrpcMethod(m) => {
            if let Some(TabKind::Grpc(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                let (svc, method) = m.split_once('/').unwrap_or((&m, ""));
                let template = t.schema().and_then(|s| s.method(svc, method)).map(|m| m.input_template);
                if let Some(item) = t.slot.scratch.as_mut()
                    && let ternarywire_core::model::RequestKind::Grpc(g) = &mut item.request
                {
                    g.service = svc.to_string();
                    g.method = method.to_string();
                    if let Some(tpl) = template {
                        g.message = tpl.replace("\"\"", "\"TernaryWire\"");
                    }
                }
            }
        }
        Step::GrpcInvoke(_) => {
            if let Some(TabKind::Grpc(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.auto_invoke = true;
            }
        }
        Step::GrpcPicker(_) => {
            if let Some(TabKind::Grpc(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.auto = Some("picker".into());
            }
        }
        Step::Expand(name) => {
            if let Some(c) = find_collection(app, &name) {
                app.sidebar.expanded.insert(c.clone());
                if let Some(col) = app.ws.data.collection(&c) {
                    fn all_folders(items: &[ternarywire_core::model::Item], out: &mut Vec<String>) {
                        for i in items {
                            if let ternarywire_core::model::Item::Folder(f) = i {
                                out.push(f.id.clone());
                                all_folders(&f.items, out);
                            }
                        }
                    }
                    let mut f = Vec::new();
                    all_folders(&col.items, &mut f);
                    app.sidebar.expanded.extend(f);
                }
            }
        }
        Step::Window([w, h]) => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h))),
        Step::SidebarScroll(y) => app.sidebar.scroll_to = Some(y),
        Step::ReportMemory(label) => {
            let since_start = crate::START.get().map(|s| s.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
            eprintln!(
                "automation: memory[{label}] = rss {:.1} MB, footprint {} MB, {:.0} ms since process start ({:.0} ms since first frame)",
                resident_memory_mb(),
                footprint_mb().map(|f| format!("{f:.0}")).unwrap_or_else(|| "?".into()),
                since_start,
                auto.started.elapsed().as_secs_f64() * 1000.0
            );
        }
        Step::NewRequest(_) => app.new_request_tab(),
        Step::Panic(message) => panic!("{message}"),
        Step::BodyView(v) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
                && let Some(r) = t.result.as_mut()
            {
                use crate::body_view::BodyView;
                r.body.view = match v.as_str() {
                    "raw" => BodyView::Raw,
                    "preview" => BodyView::Preview,
                    "tree" => BodyView::Tree,
                    _ => BodyView::Pretty,
                };
            }
        }
        Step::BodyFilter(_) | Step::BodyFold(_) | Step::BodySearch(_) | Step::BodyScroll(_) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
                && let Some(r) = t.result.as_mut()
            {
                match step {
                    Step::BodyFilter(p) => r.body.set_filter(&p),
                    Step::BodyFold(f) => r.body.fold(&f),
                    Step::BodySearch(s) => r.body.find_next(&s),
                    Step::BodyScroll(o) => r.body.scroll_body(o),
                    _ => {}
                }
            }
        }
        Step::UpdateInstall(_) => {
            if app.update.available.is_some() {
                crate::update::press_primary(app);
            } else if auto.started.elapsed() < Duration::from_secs(20) {
                // The startup check has not answered yet.
                auto.steps.push_front(Step::UpdateInstall(true));
                auto.wait_until = Some(Instant::now() + Duration::from_millis(100));
            } else {
                eprintln!("automation: no update to install");
            }
        }
        Step::RespPanel(a) => {
            if let Some(TabKind::Request(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind) {
                t.automation(&a);
            }
        }
        Step::SettingsDraft(patch) => crate::dialogs::merge_settings_draft(app, &patch),
        Step::VaultProvider(v) => {
            app.attach_external_secrets(true);
            match (serde_json::from_value::<ternarywire_secrets::ProviderConfig>(v), app.ws.external.clone()) {
                (Ok(config), Some(ext)) => {
                    if let Err(e) = ext.upsert_provider(config) {
                        eprintln!("automation: vault_provider: {}", e.message);
                    }
                }
                (Err(e), _) => eprintln!("automation: vault_provider: {e}"),
                (_, None) => eprintln!("automation: vault_provider: external secrets are unavailable"),
            }
        }
        Step::CloseAllTabs(_) => {
            while !app.tabs.is_empty() {
                app.close_tab(app.tabs.len() - 1);
            }
        }
        Step::OpenHistory(text) => {
            let id = app
                .ws
                .history
                .iter()
                .find(|h| h.request.url.contains(&text) || h.resolved_url.contains(&text))
                .map(|h| h.id.clone());
            if let Some(id) = id {
                app.open_history(&id);
            }
        }
        Step::Collapse(path) => {
            let (coll, folder) = path.split_once('/').unwrap_or((&path, ""));
            if let Some(c) = find_collection(app, coll) {
                if folder.is_empty() {
                    app.sidebar.expanded.remove(&c);
                } else if let Some(col) = app.ws.data.collection(&c) {
                    fn find(items: &[ternarywire_core::model::Item], name: &str) -> Option<String> {
                        items.iter().find_map(|i| match i {
                            ternarywire_core::model::Item::Folder(f) if f.name == name => Some(f.id.clone()),
                            ternarywire_core::model::Item::Folder(f) => find(&f.items, name),
                            _ => None,
                        })
                    }
                    if let Some(id) = find(&col.items, folder) {
                        app.sidebar.expanded.remove(&id);
                    }
                }
            }
        }
        Step::OpenIn([coll, name]) => {
            let found = find_collection(app, &coll).and_then(|c| {
                let col = app.ws.data.collection(&c)?;
                col.requests()
                    .into_iter()
                    .find(|r| r.name == name)
                    .map(|r| ItemRef { collection: c.clone(), item: r.id.clone() })
            });
            match found {
                Some(r) => app.open_item(r),
                None => eprintln!("automation: request {coll}/{name} not found"),
            }
        }
        Step::AddRequest([coll, name, method, url]) => {
            if let Some(c) = find_collection(app, &coll) {
                let item = ternarywire_core::model::RequestItem::new(
                    name,
                    ternarywire_core::model::HttpRequest::new(method, url),
                );
                if let Some(col) = app.ws.data.collection_mut(&c) {
                    col.items.push(ternarywire_core::model::Item::Request(item));
                    app.ws.mark_collection(&c);
                }
            }
        }
        Step::DeleteRequest([coll, name]) => {
            let found = find_collection(app, &coll).and_then(|c| {
                let id = app.ws.data.collection(&c)?.requests().into_iter().find(|r| r.name == name)?.id.clone();
                Some((c, id))
            });
            if let Some((c, id)) = found
                && let Some(col) = app.ws.data.collection_mut(&c)
            {
                col.remove(&id);
                app.ws.mark_collection(&c);
            }
        }
        Step::CollectionVar([coll, key, value]) => {
            if let Some(c) = find_collection(app, &coll)
                && let Some(col) = app.ws.data.collection_mut(&c)
            {
                match col.variables.iter_mut().find(|v| v.key == key) {
                    Some(v) => v.value = value,
                    None => col.variables.push(ternarywire_core::variables::Variable::new(key, value)),
                }
                app.ws.mark_collection(&c);
            }
        }
        Step::ForkToggle(name) => {
            if let Some(crate::dialogs::Dialog::ForkSync(f)) = &mut app.dialog
                && let Some(e) = f.plan.entries.iter_mut().find(|e| e.name == name)
            {
                e.apply = !e.apply;
            }
        }
        Step::ForkSelect(name) => {
            if let Some(crate::dialogs::Dialog::ForkSync(f)) = &mut app.dialog
                && let Some(i) = f.plan.entries.iter().position(|e| e.name == name)
            {
                f.selected = i;
            }
        }
        Step::Quit(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
    }
    app.automation = Some(auto);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the `TERNARYWIRE_AUTOMATION` script on the real app, with the
    /// settings in `TERNARYWIRE_CONFIG_DIR`, drawn off screen on the GPU: no
    /// window opens, so visual checks never take over the screen. The window
    /// size is `TERNARYWIRE_SIZE` (default `1280x820`), at `TERNARYWIRE_PPP` pixels
    /// per point (default 2).
    ///
    /// ```text
    /// TERNARYWIRE_CONFIG_DIR=… TERNARYWIRE_AUTOMATION=steps.json \
    ///   cargo test -p ternarywire-app offscreen -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a GPU, TERNARYWIRE_AUTOMATION and TERNARYWIRE_CONFIG_DIR"]
    fn offscreen() {
        use ternarywire_core::settings::AppSettings;
        assert!(std::env::var_os("TERNARYWIRE_AUTOMATION").is_some(), "set TERNARYWIRE_AUTOMATION");
        assert!(
            std::env::var_os("TERNARYWIRE_CONFIG_DIR").is_some(),
            "set TERNARYWIRE_CONFIG_DIR, never the real settings"
        );
        let size = std::env::var("TERNARYWIRE_SIZE")
            .ok()
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some(egui::vec2(w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or(egui::vec2(1280.0, 820.0));
        ternarywire_http::tls::install_provider();
        let path = AppSettings::default_path();
        let settings = AppSettings::load(&path);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size)
            .with_pixels_per_point(std::env::var("TERNARYWIRE_PPP").ok().and_then(|s| s.parse().ok()).unwrap_or(2.0))
            .wgpu()
            // As eframe does: characters the app's fonts lack come from the
            // system fonts, drawn as boxes while those are being looked up.
            .with_context_setup(|ctx| {
                ctx.add_font_provider(std::sync::Arc::new(egui_system_font_provider::SystemFontProvider::new()))
            })
            .allow_missing_glyphs()
            .build_eframe(|cc| TernaryWireApp::new(cc, settings, path, None));
        let started = Instant::now();
        let mut unnamed = std::collections::HashSet::new();
        while !harness.state().automation_finished() {
            assert!(started.elapsed() < Duration::from_secs(300), "the script did not finish");
            harness.step();
            // Listed, not failed on: the run goes on to the remaining steps.
            for w in harness.unnamed_widgets() {
                if unnamed.insert(w.clone()) {
                    eprintln!("automation: no accessible name: {w}");
                }
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}
