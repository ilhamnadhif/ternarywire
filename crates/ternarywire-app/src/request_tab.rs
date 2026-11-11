//! HTTP (and GraphQL) request tab: URL bar, request editor sections and the
//! response panel (PRD 5.1, 5.3, 6.1, 7.1).
//!
//! Layout (the redesign's `http` screens): a 46 px title row, then two
//! panes side by side (or stacked when narrow). The request pane is the URL
//! row (56) + section tabs (36) + the section; the response pane is the
//! status row (56) + response tabs (36) + the response section, so the rows
//! of both panes line up.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use ternarywire_core::auth::{Auth, AuthType};
use ternarywire_core::model::{
    Body, BodyMode, Example, FormField, FormFieldKind, HttpRequest, HttpVersion, KeyValue, RawLanguage, RequestItem,
    RequestSettings,
};
use ternarywire_core::script::{ScriptEvent, TestResult};
use ternarywire_core::tokens::TokenStore;
use ternarywire_core::variables::Resolver;
use ternarywire_http::CancellationToken;
use ternarywire_runner::Execution;

use crate::app::{ItemRef, TernaryWireApp};
use crate::body_view::BodyState;
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::icons;
use crate::kit::{self, ButtonOpts, Kind, PillOpts, Size, Tab, Tone, size};
use crate::theme;
use crate::widgets::{self, KvOptions, VarCtx};

pub struct Running {
    pub cancel: CancellationToken,
    pub started: Instant,
}

pub struct ResultView {
    pub exec: Execution,
    pub body: BodyState,
    /// When the response arrived (shown when comparing).
    pub at: chrono::DateTime<chrono::Local>,
}

/// How much of the tab the response panel takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PanelMode {
    /// The split set by the divider.
    #[default]
    Normal,
    /// The request editor is folded to its URL row and section tabs
    /// (stacked) or hidden (side by side).
    Maximized,
    /// Only the response's status row is shown (a narrow rail beside the
    /// request).
    Minimized,
}

pub struct GraphQlState {
    pub schema: Option<Arc<ternarywire_convert::graphql::Schema>>,
    pub loading: bool,
    pub error: Option<String>,
    /// The schema explorer drawer beside the query (open by default).
    pub explorer_open: bool,
    pub filter: String,
}

impl Default for GraphQlState {
    fn default() -> Self {
        GraphQlState { schema: None, loading: false, error: None, explorer_open: true, filter: String::new() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Params,
    Auth,
    Headers,
    Body,
    Scripts,
    Settings,
    Docs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RespSection {
    Body,
    Headers,
    Cookies,
    Tests,
    Details,
    Visualize,
    /// The previous response against this one.
    Compare,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TestFilter {
    All,
    Passed,
    Failed,
    Skipped,
}

pub struct RequestTab {
    pub location: Option<ItemRef>,
    pub scratch: Option<RequestItem>,
    pub section: Section,
    pub script_event: ScriptEvent,
    pub resp_section: RespSection,
    pub running: Option<Running>,
    pub result: Option<Box<ResultView>>,
    pub graphql: GraphQlState,
    pub test_filter: TestFilter,
    renaming: Option<String>,
    /// Fraction of the panes given to the request at first (and after a
    /// double click on the divider).
    pub split: f32,
    /// Set by UI automation to press Send through the normal button path.
    pub click_send: bool,
    /// Put the caret in the URL field on the next frame (new request tabs).
    pub focus_url: bool,
    /// A running GraphQL subscription (graphql-transport-ws over WebSocket).
    pub gql_sub: Option<GqlSubscription>,
    /// Number of the subscription's connection (see `app::next_epoch`).
    pub ws_epoch: u64,
    /// Rendered `pm.visualizer.set` output, keyed by a hash of its input.
    viz: Option<(u64, Result<ternarywire_html::Document, String>)>,
    /// Maximized or minimized response panel.
    pub panel: PanelMode,
    /// The mode the last frame was laid out with.
    drawn_panel: PanelMode,
    /// The response before the current one (Compare).
    pub prev: Option<Box<crate::response_diff::Snapshot>>,
    /// Keep `prev` when new responses arrive.
    pub prev_pinned: bool,
    compare: crate::response_diff::CompareState,
    /// Set as variable, generated tests, confirmations.
    tools: crate::response_actions::Tools,
    /// A body value action requested by UI automation.
    pending_value: Option<crate::response_actions::ValueAction>,
    /// The tab was opened from a History entry: when it was sent (unix
    /// milliseconds) and the status it got (None: it failed). Shown as a
    /// notice under the title row until dismissed or saved.
    pub from_history: Option<(i64, Option<u16>)>,
    /// Docs: the description is shown rendered instead of edited.
    docs_preview: bool,
    /// UI automation asked for the GraphQL schema.
    pending_schema: bool,
}

pub struct GqlSubscription {
    session: ternarywire_http::ws::WsSession,
    subscribe: String,
    pub status: String,
    pub messages: Vec<(i64, String)>,
    started: Instant,
    /// The server completed, errored or the connection closed.
    pub done: bool,
}

/// True if the request is a GraphQL subscription operation.
pub fn is_subscription(item: &RequestItem) -> bool {
    item.http()
        .map(|h| h.body.mode == BodyMode::GraphQl && starts_with_subscription(&h.body.graphql.query))
        .unwrap_or(false)
}

fn starts_with_subscription(query: &str) -> bool {
    let mut q = query.trim_start();
    // Skip comments.
    while let Some(rest) = q.strip_prefix('#') {
        q = rest.split_once('\n').map(|(_, r)| r).unwrap_or("").trim_start();
    }
    q.starts_with("subscription")
}

impl RequestTab {
    /// WebSocket events of a GraphQL subscription.
    pub fn on_ws_event(&mut self, e: ternarywire_http::ws::WsEvent) {
        use ternarywire_http::ws::{WsDirection, WsEvent, WsPayload};
        let Some(sub) = &mut self.gql_sub else { return };
        match e {
            WsEvent::Connecting => sub.status = "Connecting…".into(),
            WsEvent::Connected { protocol, .. } => {
                sub.status =
                    format!("Connected ({}), initializing…", protocol.unwrap_or_else(|| "no subprotocol".into()));
                sub.session.send_text(serde_json::json!({"type": "connection_init", "payload": {}}).to_string());
            }
            WsEvent::Message { direction: WsDirection::Incoming, payload: WsPayload::Text(t), at_ms } => {
                let v: serde_json::Value = serde_json::from_str(&t).unwrap_or(serde_json::Value::Null);
                match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "connection_ack" => {
                        sub.session.send_text(sub.subscribe.clone());
                        sub.status = "Subscribed".into();
                    }
                    "next" | "data" => {
                        let payload = v.get("payload").cloned().unwrap_or(v.clone());
                        sub.messages.push((at_ms, serde_json::to_string_pretty(&payload).unwrap_or(t)));
                    }
                    "error" => {
                        sub.status = "Error".into();
                        sub.messages.push((at_ms, serde_json::to_string_pretty(&v).unwrap_or(t)));
                    }
                    "complete" => {
                        sub.status = "Completed by server".into();
                        sub.done = true;
                        sub.session.close(1000, "");
                    }
                    "ping" => {
                        sub.session.send_text(serde_json::json!({"type": "pong"}).to_string());
                    }
                    "ka" | "pong" => {}
                    _ => sub.messages.push((at_ms, t)),
                }
            }
            WsEvent::Message { .. } => {}
            WsEvent::Error(m) => {
                sub.status = format!("Error: {m}");
                sub.done = true;
            }
            WsEvent::Closed { code, reason, .. } => {
                sub.done = true;
                if sub.status.starts_with("Completed") || sub.status.starts_with("Error") {
                    return;
                }
                sub.status = format!(
                    "Closed{}{}",
                    code.map(|c| format!(" ({c})")).unwrap_or_default(),
                    if reason.is_empty() { String::new() } else { format!(": {reason}") }
                );
            }
        }
    }
}

impl RequestTab {
    fn base(location: Option<ItemRef>, scratch: Option<RequestItem>) -> Self {
        RequestTab {
            location,
            scratch,
            section: Section::Params,
            script_event: ScriptEvent::PostResponse,
            resp_section: RespSection::Body,
            running: None,
            result: None,
            graphql: GraphQlState::default(),
            test_filter: TestFilter::All,
            renaming: None,
            split: 0.5,
            click_send: false,
            focus_url: false,
            gql_sub: None,
            ws_epoch: 0,
            viz: None,
            panel: PanelMode::Normal,
            drawn_panel: PanelMode::Normal,
            prev: None,
            prev_pinned: false,
            compare: Default::default(),
            tools: Default::default(),
            pending_value: None,
            from_history: None,
            docs_preview: false,
            pending_schema: false,
        }
    }

    pub fn scratch(item: RequestItem) -> Self {
        Self::base(None, Some(item))
    }

    pub fn saved(r: ItemRef) -> Self {
        Self::base(Some(r), None)
    }

    pub fn start_running(&mut self, cancel: CancellationToken) {
        self.gql_sub = None;
        // A minimized panel opens again to show the new response.
        if self.panel == PanelMode::Minimized {
            self.panel = PanelMode::Normal;
        }
        self.running = Some(Running { cancel, started: Instant::now() });
    }

    pub fn cancel(&self) {
        if let Some(r) = &self.running {
            r.cancel.cancel();
        }
    }

    pub fn finish(&mut self, exec: Execution, rerun: bool) {
        if rerun {
            if let Some(r) = &mut self.result {
                r.exec.tests = exec.tests;
                r.exec.script_errors = exec.script_errors;
                r.exec.console.extend(exec.console);
                r.exec.visualization = exec.visualization.or(r.exec.visualization.take());
                self.resp_section = RespSection::Tests;
            }
            return;
        }
        // Only a send ends the send in progress (re-running tests is not one).
        self.running = None;
        let had_tests = !exec.tests.is_empty();
        // The new body keeps how the last one was viewed (view, wrap, filter).
        let mut body = BodyState::default();
        if let Some(old) = &self.result {
            body.view = old.body.view;
            body.wrap = old.body.wrap;
            body.filter = old.body.filter.clone();
            body.filter_open = old.body.filter_open;
        }
        // The response being replaced becomes the one to compare with,
        // unless a baseline is kept.
        if let Some(old) = self.result.take()
            && let Some(resp) = old.exec.response
            && !(self.prev_pinned && self.prev.is_some())
        {
            self.prev = Some(Box::new(crate::response_diff::Snapshot { resp, at: old.at }));
        }
        self.compare.reset();
        self.result = Some(Box::new(ResultView { exec, body, at: chrono::Local::now() }));
        if self.resp_section == RespSection::Tests && !had_tests {
            self.resp_section = RespSection::Body;
        }
    }

    /// UI automation of the response panel: `maximize`, `minimize`,
    /// `restore`, `compare`, `compare_headers`, `generate_tests`,
    /// `add_generated_tests` (presses "Add tests" in the preview),
    /// `set_var:<path>`, `add_test:<path>` (a path of the JSON body), and
    /// of the request: `fetch_schema` (GraphQL), `docs_preview`,
    /// `from_history:<unix ms>:<status>`.
    pub fn automation(&mut self, action: &str) {
        use crate::response_actions::ValueAction;
        match action {
            "maximize" => self.panel = PanelMode::Maximized,
            "minimize" => self.panel = PanelMode::Minimized,
            "restore" => self.panel = PanelMode::Normal,
            "fetch_schema" => self.pending_schema = true,
            "docs_preview" => self.docs_preview = true,
            // `from_history:<sent at, unix ms>:<status or "-">`: the notice of
            // a tab opened from History (set by `app::open_history`).
            a if a.starts_with("from_history:") => {
                let mut parts = a.splitn(3, ':').skip(1);
                if let Some(at) = parts.next().and_then(|t| t.parse().ok()) {
                    self.from_history = Some((at, parts.next().and_then(|s| s.parse().ok())));
                }
            }
            "compare" | "compare_headers" => {
                self.resp_section = RespSection::Compare;
                self.compare.mode = if action == "compare" {
                    crate::response_diff::Mode::Body
                } else {
                    crate::response_diff::Mode::Headers
                };
            }
            "add_generated_tests" => self.tools.auto_insert = true,
            "generate_tests" => {
                if let Some(resp) = self.result.as_ref().and_then(|r| r.exec.response.as_ref()) {
                    self.tools.generate(resp);
                }
            }
            other => {
                let Some((kind, path)) = other.split_once(':') else { return };
                let json: Option<serde_json::Value> = self
                    .result
                    .as_ref()
                    .and_then(|r| r.exec.response.as_ref())
                    .and_then(|r| r.body.read_all().ok())
                    .and_then(|b| serde_json::from_slice(&b).ok());
                let Some(value) =
                    json.as_ref().and_then(|j| crate::test_gen::lookup(j, &crate::test_gen::parse_path(path))).cloned()
                else {
                    return;
                };
                let path = path.to_string();
                self.pending_value = match kind {
                    "set_var" => Some(ValueAction::SetVariable { path, value }),
                    "add_test" => Some(ValueAction::AddTest { path, value }),
                    _ => None,
                };
            }
        }
    }
}

/// A recent History entry offered in the empty response.
struct Recent {
    id: String,
    method: String,
    path: String,
    status: Option<u16>,
    when: String,
}

/// Context computed before editing to avoid borrowing the workspace twice.
struct EditCtx {
    vars: VarCtx,
    inherited: Option<(String, AuthType)>,
    /// Where the inherited auth is set: (collection, folder).
    inherited_owner: Option<(String, Option<String>)>,
    breadcrumb: Vec<String>,
    /// The request is not saved in a collection.
    scratch: bool,
    token: Option<ternarywire_core::tokens::OAuth2Token>,
    fetching: bool,
    max_pretty: u64,
    side_layout: bool,
    /// Settings > Requests: what the request's own settings fall back to.
    defaults: ternarywire_core::settings::RequestDefaults,
    /// The last History entries (only when nothing was sent yet).
    recent: Vec<Recent>,
    /// `host:port` the request goes to (for "Waiting for … ").
    host: String,
}

#[derive(Default)]
struct Actions {
    send: bool,
    cancel: bool,
    save: bool,
    codegen: bool,
    fetch_schema: bool,
    get_token: bool,
    clear_token: bool,
    save_example: bool,
    rerun_tests: bool,
    duplicate: bool,
    /// What a pasted cURL command could not import exactly.
    curl_warnings: Vec<String>,
    /// "Set as variable" / "Add test" from the body.
    value_action: Option<crate::response_actions::ValueAction>,
    /// Open the "Generate tests" preview.
    generate_tests: bool,
    /// Copy the request as sent (resolved) as a cURL command.
    copy_curl: bool,
    /// Copy the request (as edited) as a cURL command.
    copy_request_curl: bool,
    /// Ask to delete the saved request.
    delete: bool,
    /// Open the collection or folder whose auth the request inherits.
    open_parent_auth: bool,
    /// Open a History entry (empty response).
    open_history: Option<String>,
    open_console: bool,
    open_cookies: bool,
    /// Keep the current response as the compare baseline.
    pin_baseline: bool,
}

fn get_item(app: &TernaryWireApp, tab: &RequestTab) -> Option<RequestItem> {
    match &tab.location {
        Some(loc) => app.ws.data.collection(&loc.collection)?.find_request(&loc.item).cloned(),
        None => tab.scratch.clone(),
    }
}

fn put_item(app: &mut TernaryWireApp, tab: &mut RequestTab, item: RequestItem) {
    match &tab.location {
        Some(loc) => {
            let cid = loc.collection.clone();
            if let Some(target) =
                app.ws.data.collection_mut(&loc.collection).and_then(|c| c.find_request_mut(&loc.item))
            {
                *target = item;
                app.ws.mark_collection(&cid);
            }
        }
        None => tab.scratch = Some(item),
    }
}

/// The path of a URL without its scheme and host (or a leading
/// `{{baseUrl}}`-style variable), for short lists.
fn url_path(url: &str) -> String {
    let rest = if url.starts_with("{{") {
        url.find("}}").map(|i| &url[i + 2..]).unwrap_or(url)
    } else if let Some(i) = url.find("://") {
        let after = &url[i + 3..];
        after.find(['/', '?']).map(|j| &after[j..]).unwrap_or("/")
    } else {
        url
    };
    if rest.is_empty() { "/".into() } else { rest.to_string() }
}

/// `host[:port]` of a resolved URL.
fn url_host(url: &str) -> String {
    let after = url.split_once("://").map(|(_, a)| a).unwrap_or(url);
    after.split(['/', '?', '#']).next().unwrap_or("").to_string()
}

/// "09:41" today, "Yesterday" or the date, for a unix-milliseconds time.
fn short_when(at_ms: i64, with_time: bool) -> String {
    let Some(t) = chrono::DateTime::from_timestamp_millis(at_ms).map(|d| d.with_timezone(&chrono::Local)) else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    let time = t.format("%H:%M").to_string();
    let day = t.date_naive();
    if day == today {
        if with_time { format!("{} {time}", tr("Today")) } else { time }
    } else if Some(day) == today.pred_opt() {
        if with_time { format!("{} {time}", tr("Yesterday")) } else { tr("Yesterday").to_string() }
    } else if with_time {
        t.format("%Y-%m-%d %H:%M").to_string()
    } else {
        t.format("%Y-%m-%d").to_string()
    }
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut RequestTab) {
    let Some(original) = get_item(app, tab) else {
        kit::empty_state(ui, icons::WARNING_CIRCLE, tr("This request no longer exists."), "", |_| {});
        return;
    };
    let mut item = original.clone();
    let collection_id = tab.location.as_ref().map(|l| l.collection.clone());
    let mut inherited = None;
    let mut inherited_owner = None;
    let mut breadcrumb = Vec::new();
    if let Some(loc) = &tab.location
        && let Some(c) = app.ws.data.collection(&loc.collection)
    {
        let chain = c.ancestors(&loc.item).unwrap_or_default();
        if let Some(e) = ternarywire_core::model::effective_from_chain(&chain) {
            inherited = Some((e.owner_name.to_string(), e.auth.kind));
            let folder = (e.owner_id != c.id.as_str()).then(|| e.owner_id.to_string());
            inherited_owner = Some((loc.collection.clone(), folder));
        }
        breadcrumb = chain.iter().map(|a| a.name().to_string()).collect();
    }
    let token = TokenStore::get(app.ws.secrets.as_ref(), &item.id);
    let nothing_sent = tab.result.is_none() && tab.running.is_none() && tab.gql_sub.is_none();
    let recent = if nothing_sent {
        app.ws
            .history
            .iter()
            .take(2)
            .map(|h| Recent {
                id: h.id.clone(),
                method: h.request.method.clone(),
                path: url_path(&h.request.url),
                status: h.status,
                when: short_when(h.timestamp, false),
            })
            .collect()
    } else {
        Vec::new()
    };
    let vars = app.var_ctx(collection_id.as_deref());
    let host = match (&tab.running, item.http()) {
        (Some(_), Some(http)) => {
            let scopes = app.ws.scopes_for(collection_id.as_deref());
            url_host(&Resolver::new(&scopes).resolve(&http.url))
        }
        _ => String::new(),
    };
    let ectx = EditCtx {
        vars,
        inherited,
        inherited_owner,
        breadcrumb,
        scratch: tab.location.is_none(),
        fetching: app.pending_tokens.contains(&item.id),
        token,
        max_pretty: app.settings.max_pretty_bytes,
        defaults: app.settings.request.clone(),
        // Side by side needs room for two usable panes; narrower views
        // have the response below.
        side_layout: app.settings.ui.layout == ternarywire_core::settings::ResponseLayout::Side
            && ui.available_width() >= size::SIDE_BY_SIDE_MIN,
        recent,
        host,
    };
    let mut actions = Actions::default();
    // Once saved, a tab from History is an ordinary saved request.
    if tab.location.is_some() {
        tab.from_history = None;
    }
    if std::mem::take(&mut tab.pending_schema) {
        actions.fetch_schema = true;
    }

    // Rows are drawn edge to edge.
    ui.spacing_mut().item_spacing.y = 0.0;
    let whole = ui.available_rect_before_wrap();
    title_row(ui, tab, &mut item, &ectx, &mut actions);
    if let Some((at, status)) = tab.from_history
        && history_notice(ui, at, status, &mut actions)
    {
        tab.from_history = None;
    }
    let resp_rect = panes(ui, tab, &mut item, &ectx, &mut actions);

    let mut after = crate::response_actions::After::default();
    // Over the response, or the whole view when the response is a rail or a bar.
    let notice_rect = if resp_rect.width() < 320.0 || resp_rect.height() < 160.0 { whole } else { resp_rect };
    let scripts_shown = tab.section == Section::Scripts
        && tab.script_event == ScriptEvent::PostResponse
        && tab.panel != PanelMode::Maximized;
    let busy = tab.running.is_some();
    crate::response_actions::notice(ui, &mut tab.tools, notice_rect, scripts_shown, busy, &mut after);

    // Body value actions (context menu of the body, or automation).
    if let Some(a) = actions.value_action.take().or_else(|| tab.pending_value.take())
        && crate::response_actions::start(&mut tab.tools, a, &mut item)
    {
        after.changed = true;
    }
    if actions.generate_tests
        && let Some(resp) = tab.result.as_ref().and_then(|r| r.exec.response.as_ref())
    {
        tab.tools.generate(resp);
    }
    let dialogs = crate::response_actions::dialogs(app, ui.ctx(), &mut tab.tools, &mut item, collection_id.as_deref());
    after.changed |= dialogs.changed;
    if dialogs.rerun_tests {
        actions.rerun_tests = true;
        tab.resp_section = RespSection::Tests;
        if tab.panel == PanelMode::Minimized {
            tab.panel = PanelMode::Normal;
        }
    }
    if after.rerun_tests {
        actions.rerun_tests = true;
    }
    if after.open_scripts || dialogs.open_scripts {
        tab.section = Section::Scripts;
        tab.script_event = ScriptEvent::PostResponse;
        if tab.panel == PanelMode::Maximized {
            tab.panel = PanelMode::Normal;
        }
    }
    if actions.pin_baseline
        && let Some(result) = &tab.result
        && let Some(resp) = &result.exec.response
    {
        tab.prev = Some(Box::new(crate::response_diff::Snapshot { resp: resp.clone(), at: result.at }));
        tab.prev_pinned = true;
        tab.compare.reset();
        tab.resp_section = RespSection::Compare;
    }
    if actions.copy_curl
        && let Some(prepared) = tab.result.as_ref().and_then(|r| r.exec.prepared.as_ref())
    {
        let code = ternarywire_convert::codegen::generate(ternarywire_convert::codegen::Language::Curl, prepared);
        ui.ctx().copy_text(code);
        app.toast(tr("Copied the request as sent, as a cURL command"), false);
    }
    if actions.copy_request_curl {
        let code = crate::dialogs::codegen_item(
            app,
            &item,
            collection_id.as_deref(),
            ternarywire_convert::codegen::Language::Curl,
        );
        ui.ctx().copy_text(code);
        app.toast(tr("Copied"), false);
    }

    if item != original {
        put_item(app, tab, item.clone());
    }
    // A new panel size lays the frame out again before it is shown, so
    // lists sized by the panel don't show a frame of the old size.
    if tab.panel != tab.drawn_panel {
        tab.drawn_panel = tab.panel;
        ui.ctx().request_discard("response panel resized");
    }

    // Deferred actions that need the whole app.
    if actions.send {
        if is_subscription(&item) {
            if tab.gql_sub.as_ref().map(|s| s.done).unwrap_or(true) {
                start_subscription(app, tab_id, tab, &item, collection_id.as_deref());
            }
        } else {
            app.deferred.push(crate::app::Deferred::Send(tab_id));
        }
    }
    if !actions.curl_warnings.is_empty() {
        app.toast(format!("{}\n{}", tr("Imported from cURL with warnings:"), actions.curl_warnings.join("\n")), true);
    }
    if actions.cancel {
        tab.cancel();
        if let Some(sub) = tab.gql_sub.take() {
            sub.session.close(1000, "");
        }
    }
    if actions.save {
        if tab.location.is_none() {
            app.dialog = Some(crate::dialogs::Dialog::save_as(tab_id, &app.ws));
        } else {
            app.ws.flush();
            app.toast(tr("Saved"), false);
            tab.from_history = None;
        }
    }
    if actions.delete
        && let Some(loc) = &tab.location
    {
        app.dialog = Some(crate::dialogs::Dialog::confirm_delete(
            loc.collection.clone(),
            Some(loc.item.clone()),
            item.name.clone(),
        ));
    }
    if actions.codegen {
        app.dialog = Some(crate::dialogs::Dialog::codegen(app, &item, collection_id.as_deref()));
    }
    if actions.open_cookies {
        app.dialog = Some(crate::dialogs::Dialog::cookies());
    }
    if actions.open_console {
        app.console_open = true;
    }
    if actions.open_parent_auth
        && let Some((c, folder)) = ectx.inherited_owner.clone()
    {
        app.open_container(c, folder);
    }
    if actions.get_token
        && let Some(http) = item.http()
    {
        let scopes = app.ws.scopes_for(collection_id.as_deref());
        app.fetch_token(http.auth.clone(), scopes, item.id.clone());
    }
    if actions.clear_token {
        TokenStore::set(app.ws.secrets.as_ref(), &item.id, None);
    }
    if actions.fetch_schema {
        fetch_schema(app, tab_id, tab, &item, collection_id.as_deref());
    }
    if actions.rerun_tests {
        app.deferred.push(crate::app::Deferred::RerunTests(tab_id));
    }
    if actions.save_example
        && let Some(result) = &tab.result
        && let Some(resp) = &result.exec.response
    {
        let mut req = item.http().cloned().unwrap_or_default();
        req.settings = RequestSettings::default();
        let body = if resp.body.len() <= 4 * 1024 * 1024 { resp.body.text_lossy() } else { String::new() };
        let ex = Example {
            name: format!("{} {}", resp.status, resp.status_text),
            request: Some(req),
            status: resp.status,
            status_text: resp.status_text.clone(),
            headers: resp
                .headers
                .iter()
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "content-length" | "transfer-encoding" | "connection" | "date" | "content-encoding"
                    )
                })
                .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
                .collect(),
            body,
            language: Some(resp.language().as_str().to_string()),
            ..Default::default()
        };
        item.examples.push(ex);
        put_item(app, tab, item.clone());
        app.toast(tr("Saved as example"), false);
        tab.section = Section::Docs;
    }
    if actions.duplicate {
        if let Some(loc) = &tab.location {
            if let Some(c) = app.ws.data.collection_mut(&loc.collection) {
                let parent = c.parent_of(&loc.item).map(str::to_string);
                let mut copy = ternarywire_core::model::Item::Request(item.clone()).duplicate();
                copy.set_name(trf("{} Copy", &[&item.name]));
                let new_id = copy.id().to_string();
                let idx = c
                    .children_mut(parent.as_deref())
                    .and_then(|v| v.iter().position(|i| i.id() == loc.item))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                c.insert(parent.as_deref(), idx, copy);
                let cid = loc.collection.clone();
                app.ws.mark_collection(&cid);
                app.open_item(ItemRef { collection: cid, item: new_id });
            }
        } else {
            let mut copy = item.clone();
            copy.id = ternarywire_core::util::new_id();
            copy.name = trf("{} Copy", &[&item.name]);
            app.add_tab(crate::app::TabKind::Request(Box::new(RequestTab::scratch(copy))));
        }
    }
    if let Some(id) = actions.open_history {
        app.open_history(&id);
    }
}

/// `kit::tabs_with` (36 high): its tab row is laid out in a row of the
/// default interact height (30) at the bottom of the bar, which puts the
/// 35 px tabs 5 px too low; a 36 px interact height keeps them in the bar.
fn pane_tabs_with<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: &mut T,
    items: &[Tab<'_, T>],
    right: impl FnOnce(&mut Ui),
) -> bool {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = size::TABS;
        kit::tabs_with(ui, current, items, right)
    })
    .inner
}

/// Width of the minimized response beside the request.
const MINIMIZED_RAIL: f32 = 46.0;

/// The two panes in the current panel mode. Returns the response's rect.
fn panes(ui: &mut Ui, tab: &mut RequestTab, item: &mut RequestItem, ectx: &EditCtx, actions: &mut Actions) -> Rect {
    let side = ectx.side_layout;
    let id = ui.make_persistent_id("req-panes");
    if tab.panel == PanelMode::Normal {
        // The request gets `split` of the whole width (side by side the
        // gutter comes out of the response), as in the prototype.
        let area = ui.available_rect_before_wrap();
        let len = if side { area.width() } else { area.height() };
        // Stacked, the divider's line is where the response's top border is.
        let request = if side { tab.split * len } else { tab.split * len - size::GUTTER / 2.0 };
        let default = (request / (len - size::GUTTER).max(1.0)).clamp(0.15, 0.85);
        // Both panes edit the tab; they are drawn one after the other.
        let cell = RefCell::new((tab, item, actions, Rect::NOTHING));
        kit::panes(
            ui,
            id,
            side,
            default,
            |ui| {
                let mut g = cell.borrow_mut();
                let (tab, item, actions, _) = &mut *g;
                request_pane(ui, tab, item, ectx, actions);
            },
            |ui| {
                let mut g = cell.borrow_mut();
                let (tab, _, actions, rect) = &mut *g;
                *rect = ui.max_rect();
                response_pane(ui, tab, ectx, actions);
            },
        );
        return cell.into_inner().3;
    }
    // Maximized or minimized: fixed sizes, and the divider brings the
    // split back.
    let area = ui.available_rect_before_wrap();
    let g = size::GUTTER;
    let (ra, rg, rb) = match (side, tab.panel) {
        (true, PanelMode::Maximized) => (None, None, area),
        (true, _) => {
            let a = (area.width() - MINIMIZED_RAIL - g).max(0.0);
            (
                Some(Rect::from_min_size(area.min, vec2(a, area.height()))),
                Some(Rect::from_min_size(pos2(area.left() + a, area.top()), vec2(g, area.height()))),
                Rect::from_min_max(pos2(area.left() + a + g, area.top()), area.max),
            )
        }
        (false, mode) => {
            let a = if mode == PanelMode::Maximized {
                size::ACTION_ROW + size::TABS
            } else {
                (area.height() - size::ACTION_ROW - g).max(0.0)
            };
            (
                Some(Rect::from_min_size(area.min, vec2(area.width(), a))),
                Some(Rect::from_min_size(pos2(area.left(), area.top() + a), vec2(area.width(), g))),
                Rect::from_min_max(pos2(area.left(), area.top() + a + g), area.max),
            )
        }
    };
    let pane = |ui: &mut Ui, r: Rect, salt: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(r).id_salt((id, salt)).layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_clip_rect(r.intersect(ui.clip_rect()));
                ui.set_min_size(r.size());
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui);
            },
        );
    };
    if let Some(ra) = ra {
        pane(ui, ra, "a", &mut |ui| request_pane(ui, tab, item, ectx, actions));
    }
    pane(ui, rb, "b", &mut |ui| response_pane(ui, tab, ectx, actions));
    if let Some(rg) = rg {
        let p = theme::pal(ui);
        let r = ui.interact(rg, id.with("restore-gutter"), Sense::click_and_drag());
        if side {
            ui.painter().vline(rg.center().x, rg.y_range(), Stroke::new(1.0, p.border));
        } else {
            ui.painter().hline(rg.x_range(), rg.center().y, Stroke::new(1.0, p.border));
        }
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(if side {
                egui::CursorIcon::ResizeHorizontal
            } else {
                egui::CursorIcon::ResizeVertical
            });
        }
        if r.drag_started() || r.double_clicked() {
            tab.panel = PanelMode::Normal;
        }
    }
    ui.advance_cursor_after_rect(area);
    rb
}

// ---------------------------------------------------------------------------
// Title row, history notice
// ---------------------------------------------------------------------------

/// The view's title row (46): breadcrumb (or "Unsaved") and the name, then
/// Code snippet, Save and the ⋯ menu.
fn title_row(ui: &mut Ui, tab: &mut RequestTab, item: &mut RequestItem, ectx: &EditCtx, actions: &mut Actions) {
    kit::bar(ui, size::VIEW_TITLE, true, |ui| {
        // 16 px on the left, 12 on the right.
        ui.add_space(4.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let more = kit::icon_button(ui, icons::DOTS_THREE, tr("More actions"));
            egui::Popup::menu(&more)
                .width(210.0)
                .align(egui::RectAlign::BOTTOM_END)
                .show(|ui| request_menu(ui, item, ectx, actions));
            let label = if ectx.scratch { tr("Save…") } else { tr("Save") };
            let save = kit::button(
                ui,
                Kind::Secondary,
                Size::Sm,
                label,
                ButtonOpts { icon: Some(icons::FLOPPY_DISK), ..Default::default() },
            );
            if save.on_hover_text(format!("{} (⌘S)", tr("Save"))).clicked() {
                actions.save = true;
            }
            if kit::icon_button(ui, icons::CODE, tr("Code snippet")).clicked() {
                actions.codegen = true;
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| crumbs(ui, tab, item, ectx));
        });
    });
}

/// Breadcrumb with caret separators and the request's name (double-click
/// renames it).
fn crumbs(ui: &mut Ui, tab: &mut RequestTab, item: &mut RequestItem, ectx: &EditCtx) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.x = 4.0;
    if ectx.scratch {
        kit::pill_ex(ui, tr("Unsaved"), Tone::Neutral, PillOpts { outline: true, ..Default::default() });
    } else {
        for part in &ectx.breadcrumb {
            ui.add(egui::Label::new(RichText::new(part.as_str()).size(12.5).color(p.text_muted)).selectable(false));
            ui.add(
                egui::Label::new(RichText::new(icons::CARET_RIGHT).font(theme::icon_font(11.0)).color(p.text_faint))
                    .selectable(false),
            );
        }
    }
    match &mut tab.renaming {
        Some(buf) => {
            let r = ui.add(
                egui::TextEdit::singleline(buf)
                    .font(theme::semibold(14.0))
                    .desired_width(ui.available_width().clamp(80.0, 320.0))
                    .margin(egui::Margin::symmetric(6, 3)),
            );
            r.request_focus();
            if r.lost_focus() {
                if !buf.trim().is_empty() {
                    item.name = buf.trim().to_string();
                }
                tab.renaming = None;
            }
        }
        None => {
            let r = ui.add(
                egui::Label::new(RichText::new(&item.name).font(theme::semibold(14.0)).color(p.text))
                    .sense(Sense::click())
                    .selectable(false)
                    .truncate(),
            );
            if r.on_hover_text(tr("Double-click to rename")).double_clicked() {
                tab.renaming = Some(item.name.clone());
            }
        }
    }
}

/// The ⋯ menu of the title row.
fn request_menu(ui: &mut Ui, item: &RequestItem, ectx: &EditCtx, actions: &mut Actions) {
    let entry = |icon, label| kit::MenuItem { icon: Some(icon), label, ..Default::default() };
    if kit::menu_item_ex(ui, entry(icons::CODE, tr("Code snippet"))).clicked() {
        actions.codegen = true;
    }
    if kit::menu_item_ex(ui, entry(icons::COPY, tr("Duplicate"))).clicked() {
        actions.duplicate = true;
    }
    if item.http().is_some() && kit::menu_item_ex(ui, entry(icons::TERMINAL, tr("Copy as cURL"))).clicked() {
        actions.copy_request_curl = true;
    }
    // A saved request has no "save a copy elsewhere" yet; an unsaved one
    // picks where it goes.
    if ectx.scratch && kit::menu_item_ex(ui, entry(icons::FLOPPY_DISK, tr("Save as…"))).clicked() {
        actions.save = true;
    }
    if !ectx.scratch {
        kit::menu_sep(ui);
        let del = kit::MenuItem { danger: true, ..entry(icons::TRASH, tr("Delete…")) };
        if kit::menu_item_ex(ui, del).clicked() {
            actions.delete = true;
        }
    }
}

/// "Opened from History" under the title row (36 high, info fill). Returns
/// true when it was dismissed.
fn history_notice(ui: &mut Ui, at_ms: i64, status: Option<u16>, actions: &mut Actions) -> bool {
    let p = theme::pal(ui);
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(Rect::from_min_size(full.min, vec2(full.width(), 36.0)), 0.0, p.info_soft);
    let mut dismiss = false;
    kit::bar(ui, 36.0, true, |ui| {
        ui.add_space(4.0);
        let text = |ui: &mut Ui, s: &str, color: Color32| {
            ui.add(egui::Label::new(RichText::new(s).size(12.5).color(color)).selectable(false));
        };
        ui.label(RichText::new(icons::CLOCK_COUNTER_CLOCKWISE).font(theme::icon_font(14.0)).color(p.info));
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            text(ui, &format!("{} ·", tr("Opened from History")), p.text);
            ui.add(
                egui::Label::new(RichText::new(short_when(at_ms, true)).font(theme::medium(12.5)).color(p.text))
                    .selectable(false),
            );
        });
        text(ui, "·", p.text_faint);
        text(ui, tr("last answer"), p.text_muted);
        match status {
            Some(s) => {
                let reason = reqwest::StatusCode::from_u16(s).ok().and_then(|c| c.canonical_reason()).unwrap_or("");
                kit::pill_ex(ui, format!("{s} {reason}").trim_end(), theme::status_tone(s), PillOpts::default());
            }
            None => {
                kit::tone_pill(ui, tr("Failed"), Tone::Err);
            }
        }
        text(ui, "·", p.text_faint);
        text(ui, tr("Save to keep it"), p.text_muted);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::icon_button_sm(ui, icons::X, tr("Dismiss")).clicked() {
                dismiss = true;
            }
            if kit::ghost(ui, Some(icons::FLOPPY_DISK), tr("Save to collection…")).clicked() {
                actions.save = true;
            }
        });
    });
    dismiss
}

// ---------------------------------------------------------------------------
// Request pane: URL row, section tabs
// ---------------------------------------------------------------------------

fn request_pane(ui: &mut Ui, tab: &mut RequestTab, item: &mut RequestItem, ectx: &EditCtx, actions: &mut Actions) {
    url_row(ui, tab, item, ectx, actions);
    let Some(http) = item.http() else { return };
    let enabled = |rows: &[KeyValue]| rows.iter().filter(|p| p.enabled).count();
    let scripts =
        [&item.scripts.pre_request, &item.scripts.post_response].iter().filter(|s| !s.trim().is_empty()).count();
    let items = [
        Tab::new(Section::Params, tr("Params")).count(enabled(&http.params)),
        Tab::new(Section::Auth, tr("Auth")).dot(!matches!(http.auth.kind, AuthType::Inherit | AuthType::None)),
        Tab::new(Section::Headers, tr("Headers")).count(enabled(&http.headers)),
        Tab::new(Section::Body, tr("Body")).dot(http.body.mode != BodyMode::None),
        Tab::new(Section::Scripts, tr("Scripts")).count(scripts),
        Tab::new(Section::Settings, tr("Settings")).count(settings_overridden(&http.settings)),
        Tab::new(Section::Docs, tr("Docs")).count(item.examples.len()),
    ];
    // With the response maximized (stacked) only the tabs show; picking
    // one brings the editor back.
    if tab.panel == PanelMode::Maximized {
        let mut show = false;
        let picked = pane_tabs_with(ui, &mut tab.section, &items, |ui| {
            show = kit::ghost(ui, Some(icons::CARET_DOWN), tr("Show editor")).clicked();
        });
        if picked || show {
            tab.panel = PanelMode::Normal;
        }
        return;
    }
    pane_tabs_with(ui, &mut tab.section, &items, |_| {});
    let id = item.id.clone();
    match tab.section {
        Section::Scripts => scripts_section(ui, tab, &mut item.scripts, &ectx.vars, actions),
        Section::Docs => docs_section(ui, tab, item),
        section => {
            let http = item.request.as_http_mut().expect("http request");
            match section {
                Section::Params => params_section(ui, http, ectx),
                Section::Auth => auth_section(ui, http, &id, ectx, actions),
                Section::Headers => headers_section(ui, http, ectx),
                Section::Body => body_section(ui, tab, http, ectx, actions),
                Section::Settings => settings_section(ui, &mut http.settings, &ectx.defaults),
                Section::Scripts | Section::Docs => {}
            }
        }
    }
}

/// Applies an edit of the URL field: a pasted cURL command becomes the whole
/// request (returning what it could not import exactly), anything else is
/// the URL.
fn url_edited(http: &mut HttpRequest, url: String) -> Vec<String> {
    let trimmed = url.trim_start();
    if (trimmed.starts_with("curl ") || trimmed.starts_with("curl\t"))
        && let Ok((parsed, warnings)) = ternarywire_convert::curl::parse_with_warnings(trimmed)
    {
        *http = parsed;
        return warnings;
    }
    http.set_url(url);
    Vec::new()
}

/// The URL row (56): the URL bar (method, URL) and Send, or Cancel while
/// sending.
fn url_row(ui: &mut Ui, tab: &mut RequestTab, item: &mut RequestItem, ectx: &EditCtx, actions: &mut Actions) {
    let running = tab.running.is_some() || tab.gql_sub.as_ref().map(|s| !s.done).unwrap_or(false);
    let Some(http) = item.http_mut() else { return };
    kit::bar(ui, size::ACTION_ROW, true, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if running {
                let r = kit::action_button(ui, Kind::Secondary, icons::STOP, tr("Cancel"));
                if r.on_hover_text(format!("{} (⌘↩)", tr("Cancel"))).clicked() {
                    actions.cancel = true;
                }
            } else {
                let r = kit::action_button(ui, Kind::Primary, icons::PAPER_PLANE_RIGHT, tr("Send"));
                if r.on_hover_text(format!("{} (⌘↩)", tr("Send"))).clicked() || std::mem::take(&mut tab.click_send)
                {
                    actions.send = true;
                }
            }
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
            url_bar(ui, rect, tab, http, ectx, actions);
        });
    });
}

/// The 38 px URL bar: the method picker on the left, then the URL (with
/// variable chips) fading out at the right edge.
fn url_bar(
    ui: &mut Ui,
    rect: Rect,
    tab: &mut RequestTab,
    http: &mut HttpRequest,
    ectx: &EditCtx,
    actions: &mut Actions,
) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    ui.painter().rect_filled(rect, CornerRadius::same(9), p.surface);
    // Method.
    let mg = ui.painter().layout_no_wrap(
        http.method.clone(),
        theme::semibold(12.5),
        theme::method_color(&http.method, dark),
    );
    let mw = (13.0 + mg.size().x + 6.0 + 11.0 + 10.0).max(93.0);
    let mrect = Rect::from_min_size(rect.min, vec2(mw, rect.height()));
    let mresp = ui.interact(mrect, ui.make_persistent_id("method"), Sense::click());
    mresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &http.method));
    widgets::a11y_name(&mresp, tr("Method"));
    if mresp.hovered() {
        ui.painter().rect_filled(mrect.shrink(1.0), CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, p.hover);
    }
    ui.painter().galley(pos2(mrect.left() + 13.0, mrect.center().y - mg.size().y / 2.0), mg, Color32::PLACEHOLDER);
    ui.painter().text(
        pos2(mrect.right() - 10.0, mrect.center().y),
        Align2::RIGHT_CENTER,
        icons::CARET_DOWN,
        theme::icon_font(11.0),
        p.text_faint,
    );
    ui.painter().vline(mrect.right() - 0.5, rect.y_range().shrink(1.0), Stroke::new(1.0, p.border));
    egui::Popup::menu(&mresp).width(180.0).show(|ui| method_menu(ui, &mut http.method));
    // URL.
    let urect =
        Rect::from_min_max(pos2(mrect.right() + 3.0, rect.top() + 1.0), pos2(rect.right() - 1.0, rect.bottom() - 1.0));
    let mut url = http.url.clone();
    let mut field = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_center_size(urect.center(), vec2(urect.width(), 34.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    field.set_clip_rect(urect.intersect(ui.clip_rect()));
    let r = widgets::var_cell(
        &mut field,
        "url",
        &mut url,
        &ectx.vars,
        tr("Enter a URL or paste a cURL command"),
        urect.width(),
    );
    if std::mem::take(&mut tab.focus_url) {
        r.request_focus();
    }
    if r.changed() {
        actions.curl_warnings = url_edited(http, url);
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        actions.send = true;
    }
    let focused = r.has_focus();
    if !focused {
        // A long URL fades out at the right edge.
        let fade = Rect::from_min_max(pos2(urect.right() - 28.0, urect.top()), urect.max);
        kit::fade(ui.painter(), fade, p.surface, true);
    }
    if focused {
        kit::focus_ring(ui, rect, 9);
    } else {
        ui.painter().rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
    }
}

/// Methods (with their colors) and a custom method.
fn method_menu(ui: &mut Ui, method: &mut String) {
    for m in widgets::METHODS {
        let item = kit::MenuItem { method: Some(m), label: m, selected: method == m, ..Default::default() };
        if kit::menu_item_ex(ui, item).clicked() {
            *method = m.to_string();
        }
    }
    kit::menu_sep(ui);
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        ui.set_height(30.0);
        ui.add_space(9.0);
        ui.label(RichText::new(icons::PENCIL_SIMPLE).font(theme::icon_font(15.0)).color(p.text_muted));
        ui.add_space(1.0);
        let mut custom = if widgets::METHODS.contains(&method.as_str()) { String::new() } else { method.clone() };
        let r = ui.add(
            egui::TextEdit::singleline(&mut custom)
                .hint_text(RichText::new(format!("{} PROPFIND", tr("Custom…"))).color(p.text_faint))
                .desired_width(ui.available_width() - 8.0)
                .font(egui::FontId::proportional(12.5))
                .margin(egui::Margin::symmetric(6, 3)),
        );
        widgets::a11y_name(&r, tr("Custom method"));
        if r.changed() && !custom.trim().is_empty() {
            *method = custom.trim().to_uppercase();
        }
    });
}

// ---------------------------------------------------------------------------
// Request sections
// ---------------------------------------------------------------------------

fn params_section(ui: &mut Ui, http: &mut HttpRequest, ectx: &EditCtx) {
    kit::pane_body(ui, "params-body", |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut params = http.params.clone();
        if widgets::kv_table(
            ui,
            "params",
            &mut params,
            &ectx.vars,
            KvOptions {
                title: Some(tr("Query parameters")),
                show_description: true,
                allow_add: true,
                key_hint: tr("Key"),
                value_hint: tr("Value"),
                ..Default::default()
            },
        ) {
            http.params = params;
            http.sync_url_from_params();
        }
        // 14 + 6 px under the table, and 8 under the title; 2 and 1 more for
        // the prototype's line height (1.45) that egui's labels don't have.
        ui.add_space(22.0);
        let help =
            kit::hint_code(ui, tr("Write {} in the URL path to add one; its value is filled in here."), &[":name"]);
        if http.path_vars.is_empty() {
            ui.label(kit::section_title(tr("Path variables")));
            ui.add_space(9.0);
            ui.add(egui::Label::new(help).wrap());
            return;
        }
        let mut vars = http.path_vars.clone();
        if widgets::kv_table(
            ui,
            "path-vars",
            &mut vars,
            &ectx.vars,
            KvOptions {
                title: Some(tr("Path variables")),
                show_description: true,
                readonly_keys: true,
                allow_add: false,
                ..Default::default()
            },
        ) {
            http.path_vars = vars;
        }
        ui.add_space(8.0);
        ui.add(egui::Label::new(help).wrap());
    });
}

fn auth_section(ui: &mut Ui, http: &mut HttpRequest, id: &str, ectx: &EditCtx, actions: &mut Actions) {
    let ctx = crate::auth_ui::AuthUiCtx {
        is_root: false,
        inherited: ectx.inherited.clone(),
        owner_id: id,
        token: ectx.token.clone(),
        fetching: ectx.fetching,
        vars: &ectx.vars,
    };
    kit::toolbar(ui, |ui| crate::auth_ui::type_bar(ui, &mut http.auth, &ctx));
    kit::pane_body(ui, "auth-body", |ui| {
        let out = crate::auth_ui::auth_form(ui, &mut http.auth, &ctx, ectx.inherited_owner.is_some());
        match out.action {
            crate::auth_ui::AuthAction::GetToken => actions.get_token = true,
            crate::auth_ui::AuthAction::ClearToken => actions.clear_token = true,
            crate::auth_ui::AuthAction::None => {}
        }
        actions.open_parent_auth |= out.open_parent;
    });
}

fn headers_section(ui: &mut Ui, http: &mut HttpRequest, ectx: &EditCtx) {
    kit::pane_body(ui, "headers-body", |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut headers = http.headers.clone();
        if widgets::kv_table(
            ui,
            "headers",
            &mut headers,
            &ectx.vars,
            KvOptions {
                title: Some(tr("Request headers")),
                key_suggestions: Some(widgets::COMMON_HEADERS),
                show_description: true,
                allow_add: true,
                key_hint: tr("Key"),
                value_hint: tr("Value"),
                ..Default::default()
            },
        ) {
            http.headers = headers;
        }
        ui.add_space(14.0);
        auto_headers(ui, http, &ectx.defaults);
    });
}

/// Column x positions of a key/value grid (`.kv`): check | key | value |
/// description | action, as `kv_table` lays them out.
fn kv_columns(left: f32, width: f32) -> [f32; 4] {
    const CHECK: f32 = 30.0;
    const ACT: f32 = 30.0;
    let cols = width - CHECK - ACT;
    let key_w = cols / 3.3;
    let val_w = cols * 1.3 / 3.3;
    [left + CHECK, left + CHECK + key_w, left + CHECK + key_w + val_w, left + width - ACT]
}

/// Paints the header of a `.kv`-style grid (28 high on `well`).
fn kv_head(ui: &Ui, rect: Rect, columns: &[(f32, &str)]) {
    let p = theme::pal(ui);
    ui.painter().rect_filled(rect, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, p.well);
    for (x, label) in columns {
        ui.painter().text(
            pos2(x + 9.0, rect.center().y),
            Align2::LEFT_CENTER,
            *label,
            theme::medium(11.5),
            p.text_muted,
        );
    }
}

/// Headers the client adds by itself, in a collapsible table like the
/// request headers. Default headers can be switched off; calculated ones
/// have a lock.
fn auto_headers(ui: &mut Ui, http: &mut HttpRequest, defaults: &ternarywire_core::settings::RequestDefaults) {
    let p = theme::pal(ui);
    // (name, value, Some(on) when it can be switched off, description)
    let mut rows: Vec<(String, String, Option<bool>, &str)> = Vec::new();
    for (k, v) in defaults.default_headers() {
        let disabled = http.settings.disabled_default_headers.iter().any(|d| d.eq_ignore_ascii_case(k));
        rows.push((k.to_string(), v, Some(!disabled), tr("From Settings › Requests")));
    }
    let ct = match http.body.mode {
        BodyMode::Raw => Some(http.body.raw_language.content_type().to_string()),
        BodyMode::UrlEncoded => Some("application/x-www-form-urlencoded".into()),
        BodyMode::FormData => Some(format!("multipart/form-data; boundary=<{}>", tr("calculated"))),
        BodyMode::GraphQl => Some("application/json".into()),
        _ => None,
    };
    for (k, v) in [
        ("Host", tr("calculated when sent").to_string()),
        ("Accept-Encoding", "gzip, deflate, br".into()),
        ("Content-Length", tr("calculated").to_string()),
    ]
    .into_iter()
    .chain(ct.map(|c| ("Content-Type", c)))
    {
        rows.push((k.to_string(), v, None, ""));
    }
    let id = ui.make_persistent_id("auto-headers");
    let open = disclosure(ui, id, tr("Auto-generated headers"), Some(rows.len()), true);
    ui.add_space(8.0);
    if !open {
        ui.add(
            egui::Label::new(kit::hint(ui, tr("Sent with every request; headers you set above override them."))).wrap(),
        );
        return;
    }
    let width = ui.available_width().max(240.0);
    let left = ui.cursor().left();
    let top = ui.cursor().top();
    let xs = kv_columns(left, width);
    let head = Rect::from_min_size(pos2(left, top), vec2(width, size::TABLE_HEAD));
    let bg = ui.painter().add(egui::Shape::Noop);
    kv_head(ui, head, &[(xs[0], tr("Key")), (xs[1], tr("Value")), (xs[2], tr("Description"))]);
    let mono = theme::mono(12.0);
    let mut y = head.bottom();
    for (i, (k, v, on, desc)) in rows.into_iter().enumerate() {
        let row = Rect::from_min_size(pos2(left, y), vec2(width, size::ROW));
        ui.painter().hline(left..=left + width, y, Stroke::new(1.0, p.border));
        if ui.rect_contains_pointer(row) {
            ui.painter().rect_filled(row.shrink2(vec2(1.0, 0.0)).with_min_y(y + 0.5), 0.0, p.well);
        }
        let check = Rect::from_center_size(pos2(left + 15.0, row.center().y + 0.5), vec2(15.0, 15.0));
        let faded = on == Some(false);
        match on {
            Some(mut enabled) => {
                let r = ui.put(check, |ui: &mut Ui| kit::check(ui, &mut enabled, ""));
                widgets::a11y_name(&r, &format!("{} {k}", tr("Send")));
                if r.changed() {
                    if enabled {
                        http.settings.disabled_default_headers.retain(|d| !d.eq_ignore_ascii_case(&k));
                    } else {
                        http.settings.disabled_default_headers.push(k.to_lowercase());
                    }
                }
            }
            None => kit::paint_check(ui, check, true, false, false, false),
        }
        let key_color = if faded { p.text_faint } else { p.text };
        let g = elide(ui, &k, mono.clone(), key_color, xs[1] - xs[0] - 40.0);
        let key_end = xs[0] + 9.0 + g.size().x;
        ui.painter().galley(pos2(xs[0] + 9.0, row.center().y - g.size().y / 2.0), g, key_color);
        if on.is_none() {
            let lock = Rect::from_center_size(pos2(key_end + 13.0, row.center().y), vec2(16.0, 16.0));
            ui.painter().text(
                lock.center(),
                Align2::CENTER_CENTER,
                icons::LOCK_SIMPLE,
                theme::icon_font(12.0),
                p.text_faint,
            );
            ui.interact(lock, id.with(("lock", i)), Sense::hover())
                .on_hover_text(tr("Set by TernaryWire when the request is sent"));
        }
        let calculated = on.is_none() && (k == "Host" || k == "Content-Length");
        let val_color = if faded || calculated { p.text_faint } else { p.text };
        let font = if calculated { egui::FontId::proportional(12.5) } else { mono.clone() };
        let g = elide(ui, &v, font, val_color, xs[2] - xs[1] - 18.0);
        ui.painter().galley(pos2(xs[1] + 9.0, row.center().y - g.size().y / 2.0), g, val_color);
        if !desc.is_empty() {
            let g = elide(ui, desc, egui::FontId::proportional(12.5), p.text_muted, xs[3] - xs[2] - 18.0);
            ui.painter().galley(pos2(xs[2] + 9.0, row.center().y - g.size().y / 2.0), g, p.text_muted);
        }
        y += size::ROW;
    }
    let table = Rect::from_min_max(pos2(left, top), pos2(left + width, y));
    ui.painter().set(bg, egui::Shape::rect_filled(table, CornerRadius::same(8), p.surface));
    for x in &xs[..3] {
        ui.painter().vline(*x, table.y_range(), Stroke::new(1.0, p.border));
    }
    ui.painter().rect_stroke(table, CornerRadius::same(8), Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.advance_cursor_after_rect(table);
    ui.add_space(8.0);
    ui.add(
        egui::Label::new(kit::hint(
            ui,
            tr("Headers you set above override these. Cookies from the jar are added automatically."),
        ))
        .wrap(),
    );
}

/// Formats JSON that may contain `{{variables}}` outside of strings.
pub fn beautify_json_with_vars(text: &str) -> Option<String> {
    let mut placeholders: Vec<(String, String)> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escape = false;
    let mut i = 0;
    let b = text.as_bytes();
    while i < b.len() {
        let c = b[i];
        if !in_string
            && text[i..].starts_with("{{")
            && let Some(end) = text[i..].find("}}")
        {
            let var = &text[i..i + end + 2];
            let ph = format!("\"__ternarywire_var_{}__\"", placeholders.len());
            placeholders.push((ph.clone(), var.to_string()));
            out.push_str(&ph);
            i += end + 2;
            continue;
        }
        if in_string {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
        } else if c == b'"' {
            in_string = true;
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    let v: serde_json::Value = serde_json::from_str(&out).ok()?;
    let mut pretty = serde_json::to_string_pretty(&v).ok()?;
    for (ph, var) in placeholders {
        pretty = pretty.replace(&ph, &var);
    }
    Some(pretty)
}

/// Short label of a body type in the mode switch (the long names in its
/// tooltip).
fn body_mode_label(mode: BodyMode) -> &'static str {
    match mode {
        BodyMode::None => tr("None"),
        BodyMode::FormData => tr("Form"),
        BodyMode::UrlEncoded => tr("URL-encoded"),
        BodyMode::Raw => tr("Raw"),
        BodyMode::Binary => tr("Binary"),
        BodyMode::GraphQl => "GraphQL",
    }
}

fn body_section(ui: &mut Ui, tab: &mut RequestTab, http: &mut HttpRequest, ectx: &EditCtx, actions: &mut Actions) {
    let mode = http.body.mode;
    kit::toolbar(ui, |ui| {
        let items: Vec<(BodyMode, &str)> = BodyMode::ALL.iter().map(|m| (*m, body_mode_label(*m))).collect();
        let r = ui.scope(|ui| kit::segmented_sm(ui, &mut http.body.mode, &items)).response;
        r.on_hover_text("none · multipart/form-data · application/x-www-form-urlencoded · raw · binary · GraphQL");
        // The tools end at the right edge; when the row is too narrow they
        // follow the modes and are cut at the pane's edge (like the prototype).
        let id = ui.make_persistent_id("body-tools-w");
        let tools_w: f32 = ui.data(|d| d.get_temp(id).unwrap_or(0.0));
        ui.add_space((ui.available_width() - tools_w).max(0.0));
        // In a child the bar does not grow with: tools past the edge are cut.
        let start = ui.cursor().left();
        let row = ui.max_rect();
        let mut tools = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(start, row.top()), pos2(start + 2000.0, row.bottom())))
                .layout(Layout::left_to_right(Align::Center)),
        );
        tools.spacing_mut().item_spacing.x = 8.0;
        match mode {
            BodyMode::Raw => raw_tools(&mut tools, &mut http.body),
            BodyMode::GraphQl => graphql_tools(&mut tools, tab, &mut http.body),
            _ => {}
        }
        let w = (tools.cursor().left() - start - 8.0).max(0.0);
        if (w - tools_w).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(id, w));
            ui.ctx().request_repaint();
        }
    });
    let body: &mut Body = &mut http.body;
    match body.mode {
        BodyMode::None => {
            center_v(ui, ui.make_persistent_id("no-body"), |ui| {
                empty_block(
                    ui,
                    icons::FILE,
                    false,
                    tr("This request does not have a body"),
                    tr("Choose a body type above to send data with the request."),
                    |_| {},
                );
            });
        }
        BodyMode::Raw => {
            let lang = Lang::from_raw(body.raw_language);
            editor_well(ui, "raw-body-well", 12.0, |ui, rows| {
                widgets::code_editor_gutter(ui, "raw-body", &mut body.raw, lang, Some(&ectx.vars), rows, false, None);
            });
        }
        BodyMode::UrlEncoded => {
            kit::pane_body(ui, "urlenc-body", |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut rows = body.urlencoded.clone();
                if widgets::kv_table(
                    ui,
                    "urlencoded",
                    &mut rows,
                    &ectx.vars,
                    KvOptions {
                        show_description: true,
                        allow_add: true,
                        key_hint: tr("Key"),
                        value_hint: tr("Value"),
                        ..Default::default()
                    },
                ) {
                    body.urlencoded = rows;
                }
                ui.add_space(14.0);
                ui.label(kit::hint(ui, tr("Sent as application/x-www-form-urlencoded.")));
            });
        }
        BodyMode::FormData => {
            kit::pane_body(ui, "form-body", |ui| {
                let mut rows = body.form_data.clone();
                if form_data_table(ui, "formdata", &mut rows, &ectx.vars) {
                    body.form_data = rows;
                }
                ui.add(
                    egui::Label::new(kit::hint(
                        ui,
                        tr("Sent as multipart/form-data. Switch a field to File to upload files."),
                    ))
                    .wrap(),
                );
            });
        }
        BodyMode::Binary => kit::pane_body(ui, "binary-body", |ui| binary_body(ui, body)),
        BodyMode::GraphQl => graphql_body(ui, tab, body, ectx, actions),
    }
}

/// Rows a code editor asks for to fill `height`: it sizes itself by the
/// font's row height (its lines are then drawn 20 apart), plus 16 of margins.
fn fill_rows(ui: &Ui, height: f32) -> usize {
    let row = ui.fonts_mut(|f| f.row_height(&theme::mono(mono_size(ui)))).max(1.0);
    (((height - 16.0) / row).floor() as usize).max(1)
}

/// A `well` filling the space left in the pane (12 px from its sides,
/// `bottom` above its end), scrolling its content. `add` gets the number of
/// 20 px lines that fill it.
fn editor_well(ui: &mut Ui, id: &str, bottom: f32, add: impl FnOnce(&mut Ui, usize)) {
    let p = theme::pal(ui);
    let avail = ui.available_rect_before_wrap();
    let rect = Rect::from_min_max(
        pos2(avail.left() + size::PAD, avail.top()),
        pos2(avail.right() - size::PAD, (avail.bottom() - bottom).max(avail.top() + 60.0)),
    );
    ui.painter().rect_filled(rect, CornerRadius::same(8), p.well);
    let inner = rect.shrink(1.0);
    let rows = fill_rows(ui, inner.height());
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)), |ui| {
        ui.set_clip_rect(inner.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical().id_salt(id).auto_shrink([false; 2]).show(ui, |ui| add(ui, rows));
    });
    ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, p.border), StrokeKind::Inside);
    // The space under it (`bottom`) is the caller's.
    ui.advance_cursor_after_rect(Rect::from_min_max(avail.min, pos2(avail.right(), rect.bottom())));
}

fn raw_language_icon(l: RawLanguage) -> &'static str {
    match l {
        RawLanguage::Json => icons::BRACKETS_CURLY,
        RawLanguage::Xml | RawLanguage::Html => icons::BRACKETS_ANGLE,
        RawLanguage::JavaScript => icons::CODE,
        RawLanguage::Text => icons::TEXT_AA,
    }
}

/// Raw body: JSON validity, Beautify and the language picker.
fn raw_tools(ui: &mut Ui, body: &mut Body) {
    if body.raw_language == RawLanguage::Json {
        let valid = body.raw.trim().is_empty() || beautify_json_with_vars(&body.raw).is_some();
        if !valid {
            kit::pill_ex(
                ui,
                tr("Invalid JSON"),
                Tone::Err,
                PillOpts { icon: Some(icons::WARNING_CIRCLE), ..Default::default() },
            )
            .on_hover_text(tr("The body is not valid JSON; it is sent as typed."));
        }
        if kit::ghost(ui, Some(icons::SPARKLE), tr("Beautify")).clicked()
            && let Some(p) = beautify_json_with_vars(&body.raw)
        {
            body.raw = p;
        }
    }
    let label = body.raw_language.label();
    let tw =
        ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), Color32::PLACEHOLDER).size().x;
    let r = kit::select_box(ui, Some(raw_language_icon(body.raw_language)), label, 8.0 + 14.0 + 6.0 + tw + 24.0, true)
        .on_hover_text(tr("Language of the raw body (sets the Content-Type)"));
    egui::Popup::menu(&r).width(170.0).align(egui::RectAlign::BOTTOM_END).show(|ui| {
        for l in RawLanguage::ALL {
            let item = kit::MenuItem { checked: Some(body.raw_language == l), label: l.label(), ..Default::default() };
            if kit::menu_item_ex(ui, item).clicked() {
                body.raw_language = l;
            }
        }
    });
}

/// multipart/form-data fields in the grid of `widgets::kv_table`: enable
/// box, key, Text/File type (its menu also sets the part's content type),
/// value (or files).
fn form_data_table(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    rows: &mut Vec<FormField>,
    vars: &VarCtx,
) -> bool {
    let salt = egui::Id::unique(&id);
    ui.push_id(salt, |ui| form_data_table_inner(ui, salt, rows, vars)).inner
}

fn form_data_table_inner(ui: &mut Ui, id: egui::Id, rows: &mut Vec<FormField>, vars: &VarCtx) -> bool {
    use egui::TextEdit;
    let p = theme::pal(ui);
    let mut changed = false;
    const CHECK: f32 = 30.0;
    const DEL: f32 = 30.0;
    const KIND: f32 = 74.0;
    let row_h = size::ROW;
    let width = ui.available_width().max(300.0);
    let left = ui.cursor().left();
    let top = ui.cursor().top();
    let cols = width - CHECK - DEL - KIND;
    let key_w = cols / 2.3;
    let xs = [left + CHECK, left + CHECK + key_w, left + CHECK + key_w + KIND, left + width - DEL];
    let mono = theme::mono(12.0);
    let bg = ui.painter().add(egui::Shape::Noop);
    let head = Rect::from_min_size(pos2(left, top), vec2(width, size::TABLE_HEAD));
    kv_head(ui, head, &[(xs[0], tr("Key")), (xs[1], tr("Type")), (xs[2], tr("Value"))]);
    if !rows.is_empty() {
        let all = rows.iter().all(|r| r.enabled);
        let some = rows.iter().any(|r| r.enabled);
        let c = Rect::from_center_size(pos2(left + CHECK / 2.0, head.center().y), vec2(15.0, 15.0));
        let r = ui.interact(c, id.with("all"), Sense::click());
        widgets::a11y_name(&r, tr("Enabled"));
        kit::paint_check(ui, c, all, some && !all, r.hovered(), ui.is_enabled());
        if r.clicked() {
            for row in rows.iter_mut() {
                row.enabled = !all;
            }
            changed = true;
        }
    }
    let mut y = head.bottom();
    let mut remove = None;
    let n = rows.len();
    let ring = |ui: &Ui, r: Rect| {
        ui.painter().rect_stroke(r.shrink(1.0), CornerRadius::same(4), Stroke::new(1.5, p.accent), StrokeKind::Inside);
    };
    for i in 0..=n {
        let is_new = i == n;
        let mut row = if is_new { FormField::text("", "") } else { rows[i].clone() };
        let mut row_changed = false;
        let row_rect = Rect::from_min_size(pos2(left, y), vec2(width, row_h));
        ui.painter().hline(left..=left + width, y, Stroke::new(1.0, p.border));
        let hovered = ui.rect_contains_pointer(row_rect);
        if hovered && !is_new {
            ui.painter().rect_filled(row_rect.shrink2(vec2(1.0, 0.0)).with_min_y(y + 0.5), 0.0, p.well);
        }
        let cell = |x0: f32, x1: f32| Rect::from_min_max(pos2(x0 + 1.0, y + 1.0), pos2(x1, y + row_h));
        ui.push_id(i, |ui| {
            if !is_new {
                let c = Rect::from_center_size(pos2(left + CHECK / 2.0, row_rect.center().y + 0.5), vec2(15.0, 15.0));
                let r = ui.put(c, |ui: &mut Ui| kit::check(ui, &mut row.enabled, ""));
                widgets::a11y_name(&r, &format!("{} {}", tr("Enabled"), i + 1));
                row_changed |= r.changed();
            }
            let faded = !is_new && !row.enabled;
            let text_color = if faded { p.text_faint } else { p.text };
            // Key
            let key_rect = cell(xs[0], xs[1]);
            let r = ui.put(
                key_rect,
                TextEdit::singleline(&mut row.key)
                    .hint_text(RichText::new(if is_new { tr("Key") } else { "" }).color(p.text_faint))
                    .font(mono.clone())
                    .text_color(text_color)
                    // egui ignores `margin` when a frame is given: it goes in the frame.
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(9, 0)))
                    .min_size(vec2(0.0, key_rect.height()))
                    .align(Align2::LEFT_CENTER)
                    .desired_width(key_rect.width()),
            );
            widgets::a11y_name(&r, &format!("{} {}", tr("Key"), i + 1));
            if r.has_focus() {
                ring(ui, key_rect);
            }
            row_changed |= r.changed();
            // Type (and content type)
            if !is_new {
                let kind_rect = cell(xs[1], xs[2]);
                let label = match row.kind {
                    FormFieldKind::Text => tr("Text"),
                    FormFieldKind::File => tr("File"),
                };
                let sel = Rect::from_center_size(kind_rect.center(), vec2(kind_rect.width() - 8.0, 26.0));
                let r = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(sel), |ui| {
                        kit::select_box_ex(ui, None, None, label, sel.width(), true, true)
                    })
                    .inner;
                let r = match &row.content_type {
                    Some(ct) => r.on_hover_text(format!("{}: {ct}", tr("Content-Type"))),
                    None => r,
                };
                egui::Popup::menu(&r).width(200.0).show(|ui| {
                    for (k, icon, label) in [
                        (FormFieldKind::Text, icons::TEXT_AA, tr("Text")),
                        (FormFieldKind::File, icons::FILE, tr("File")),
                    ] {
                        let item =
                            kit::MenuItem { icon: Some(icon), label, selected: row.kind == k, ..Default::default() };
                        if kit::menu_item_ex(ui, item).clicked() && row.kind != k {
                            row.kind = k;
                            row_changed = true;
                        }
                    }
                    kit::menu_sep(ui);
                    kit::menu_header(ui, tr("Content-Type"));
                    let mut ct = row.content_type.clone().unwrap_or_default();
                    let r = ui.add(
                        TextEdit::singleline(&mut ct)
                            .hint_text(RichText::new(tr("Auto")).color(p.text_faint))
                            .font(theme::mono(12.0))
                            .margin(egui::Margin::symmetric(8, 4))
                            .desired_width(f32::INFINITY),
                    );
                    widgets::a11y_name(&r, &format!("{} {}", tr("Content-Type"), i + 1));
                    if r.changed() {
                        row.content_type = (!ct.trim().is_empty()).then_some(ct);
                        row_changed = true;
                    }
                });
            }
            // Value
            let val_rect = cell(xs[2], xs[3]);
            match row.kind {
                FormFieldKind::Text => {
                    let r = ui
                        .scope_builder(
                            egui::UiBuilder::new().max_rect(val_rect).layout(Layout::left_to_right(Align::Center)),
                            |ui| {
                                if faded {
                                    ui.visuals_mut().override_text_color = Some(p.text_faint);
                                }
                                widgets::var_cell(
                                    ui,
                                    "v",
                                    &mut row.value,
                                    vars,
                                    if is_new { tr("Value") } else { "" },
                                    val_rect.width(),
                                )
                            },
                        )
                        .inner;
                    widgets::a11y_name(&r, &format!("{} {}", tr("Value"), i + 1));
                    if r.has_focus() {
                        ring(ui, val_rect);
                    }
                    row_changed |= r.changed();
                }
                FormFieldKind::File => {
                    let r = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(val_rect), |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.add_space(5.0);
                                if row.files.is_empty() {
                                    kit::ghost(ui, Some(icons::UPLOAD_SIMPLE), tr("Select files"))
                                } else {
                                    let names = row
                                        .files
                                        .iter()
                                        .map(|f| {
                                            std::path::Path::new(f)
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_else(|| f.clone())
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    let bytes: u64 = row
                                        .files
                                        .iter()
                                        .filter_map(|f| std::fs::metadata(f).ok())
                                        .map(|m| m.len())
                                        .sum();
                                    let text = format!("{names} · {}", ternarywire_core::util::human_bytes(bytes));
                                    let name_w = (ui.available_width() - 30.0).max(40.0);
                                    let (rect, resp) = ui.allocate_exact_size(vec2(name_w, 24.0), Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(rect, CornerRadius::same(5), p.hover);
                                    }
                                    ui.painter().text(
                                        pos2(rect.left() + 4.0, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        icons::FILE,
                                        theme::icon_font(13.0),
                                        p.text_faint,
                                    );
                                    let g =
                                        elide(ui, &text, egui::FontId::proportional(12.5), text_color, name_w - 26.0);
                                    ui.painter().galley(
                                        pos2(rect.left() + 22.0, rect.center().y - g.size().y / 2.0),
                                        g,
                                        text_color,
                                    );
                                    if kit::icon_button_sm(ui, icons::X, tr("Clear files")).clicked() {
                                        row.files.clear();
                                        row_changed = true;
                                    }
                                    resp.on_hover_text(row.files.join("\n"))
                                }
                            })
                            .inner
                        })
                        .inner;
                    if r.clicked()
                        && let Some(files) = rfd::FileDialog::new().pick_files()
                    {
                        row.files = files.into_iter().map(|p| p.display().to_string()).collect();
                        row_changed = true;
                    }
                }
            }
            if !is_new && hovered {
                let d = Rect::from_center_size(pos2(xs[3] + DEL / 2.0, row_rect.center().y), vec2(22.0, 22.0));
                let r = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(d), |ui| {
                        kit::icon_button_sm(ui, icons::TRASH, tr("Remove"))
                    })
                    .inner;
                widgets::a11y_name(&r, &format!("{} {}", tr("Remove"), i + 1));
                if r.clicked() {
                    remove = Some(i);
                }
            }
        });
        y += row_h;
        if row_changed {
            changed = true;
            if is_new {
                if !row.key.is_empty() || !row.value.is_empty() {
                    row.enabled = true;
                    rows.push(row);
                }
            } else {
                rows[i] = row;
            }
        }
    }
    let table = Rect::from_min_max(pos2(left, top), pos2(left + width, y));
    ui.painter().set(bg, egui::Shape::rect_filled(table, CornerRadius::same(8), p.surface));
    for x in &xs[..3] {
        ui.painter().vline(*x, table.y_range(), Stroke::new(1.0, p.border));
    }
    ui.painter().rect_stroke(table, CornerRadius::same(8), Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.advance_cursor_after_rect(table);
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    changed
}

/// A file picked as the whole request body: a card with the file and its
/// size, and its path under it.
fn binary_body(ui: &mut Ui, body: &mut Body) {
    let p = theme::pal(ui);
    let path = body.binary_path.trim().to_string();
    let meta = (!path.is_empty()).then(|| std::fs::metadata(&path).ok()).flatten();
    let missing = !path.is_empty() && meta.is_none();
    kit::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (rect, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            let (fg, bg) = if missing { (p.err, p.err_soft) } else { (p.accent_text, p.accent_soft) };
            ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icons::FILE_ARROW_UP, theme::icon_font(20.0), fg);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if !path.is_empty() && kit::icon_button_sm(ui, icons::X, tr("Remove file")).clicked() {
                    body.binary_path.clear();
                }
                let label = if path.is_empty() { tr("Choose file…") } else { tr("Change…") };
                if kit::button(ui, Kind::Secondary, Size::Sm, label, ButtonOpts::default()).clicked()
                    && let Some(f) = rfd::FileDialog::new().pick_file()
                {
                    body.binary_path = f.display().to_string();
                }
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add_space(3.0);
                    let name = if path.is_empty() {
                        tr("No file selected").to_string()
                    } else {
                        std::path::Path::new(&path)
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone())
                    };
                    ui.add(egui::Label::new(RichText::new(name).font(theme::medium(13.0)).color(p.text)).truncate());
                    match &meta {
                        Some(m) => ui.add(
                            egui::Label::new(kit::hint(
                                ui,
                                &format!(
                                    "{} · {}",
                                    ternarywire_core::util::human_bytes(m.len()),
                                    tr("the file's bytes are sent as the request body")
                                ),
                            ))
                            .truncate(),
                        ),
                        None if missing => ui.label(RichText::new(tr("File not found")).size(12.0).color(p.err)),
                        None => ui.add(
                            egui::Label::new(kit::hint(ui, tr("The file's bytes are sent as the request body.")))
                                .truncate(),
                        ),
                    };
                });
            });
        });
    });
    let r = ui.add(
        egui::TextEdit::singleline(&mut body.binary_path)
            .desired_width(f32::INFINITY)
            .font(theme::mono(12.0))
            .min_size(vec2(0.0, 26.0))
            .align(Align2::LEFT_CENTER)
            .margin(egui::Margin::symmetric(8, 0))
            .hint_text(RichText::new("/path/to/file").color(p.text_faint)),
    );
    widgets::a11y_name(&r, tr("File path"));
}

/// GraphQL toolbar tools: Prettify and the schema explorer toggle.
fn graphql_tools(ui: &mut Ui, tab: &mut RequestTab, body: &mut Body) {
    if kit::ghost(ui, Some(icons::SPARKLE), tr("Prettify")).clicked() {
        body.graphql.query = prettify_graphql(&body.graphql.query);
    }
    let open = tab.graphql.explorer_open;
    if kit::icon_button_ex(ui, icons::BOOK_OPEN, tr("Schema explorer"), open, 22.0).clicked() {
        tab.graphql.explorer_open = !open;
    }
}

/// Query above Variables in one frame, with the schema explorer as a drawer
/// on its right.
fn graphql_body(ui: &mut Ui, tab: &mut RequestTab, body: &mut Body, ectx: &EditCtx, actions: &mut Actions) {
    let p = theme::pal(ui);
    let avail = ui.available_rect_before_wrap();
    let rect = Rect::from_min_max(
        pos2(avail.left() + size::PAD, avail.top()),
        pos2(avail.right() - size::PAD, (avail.bottom() - 12.0).max(avail.top() + 140.0)),
    );
    let explorer_w = if tab.graphql.explorer_open && rect.width() >= 224.0 + 160.0 { 224.0 } else { 0.0 };
    let left = Rect::from_min_max(rect.min, pos2(rect.right() - explorer_w, rect.bottom()));
    const HEAD: f32 = 30.0;
    let editors = (left.height() - 2.0 * HEAD).max(40.0);
    let q_h = (editors * 1.4 / 1.9).round();
    let qh = Rect::from_min_size(left.min, vec2(left.width(), HEAD));
    let q_rect = Rect::from_min_size(pos2(left.left(), qh.bottom()), vec2(left.width(), q_h));
    let vh = Rect::from_min_size(pos2(left.left(), q_rect.bottom()), vec2(left.width(), HEAD));
    let v_rect = Rect::from_min_max(pos2(left.left(), vh.bottom()), left.max);
    let painter = ui.painter().clone();
    painter.rect_filled(qh, CornerRadius { nw: 8, ne: if explorer_w > 0.0 { 0 } else { 8 }, sw: 0, se: 0 }, p.well);
    painter.rect_filled(vh, 0.0, p.well);
    painter.hline(qh.x_range(), qh.bottom() - 0.5, Stroke::new(1.0, p.border));
    painter.hline(vh.x_range(), vh.top() + 0.5, Stroke::new(1.0, p.border));
    painter.hline(vh.x_range(), vh.bottom() - 0.5, Stroke::new(1.0, p.border));
    let eyebrow = |r: Rect, text: &str| {
        let g = painter.layout_no_wrap(text.to_uppercase(), theme::semibold(10.5), p.text_faint);
        painter.galley(pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, p.text_faint);
    };
    eyebrow(qh, tr("Query"));
    eyebrow(vh, tr("Variables"));
    painter.text(
        pos2(vh.right() - 10.0, vh.center().y),
        Align2::RIGHT_CENTER,
        "JSON",
        egui::FontId::proportional(11.0),
        p.text_faint,
    );
    if tab.graphql.schema.is_some() {
        let g = painter.layout_no_wrap(tr("Schema loaded").to_string(), egui::FontId::proportional(11.0), p.text_faint);
        let x = qh.right() - 10.0 - g.size().x;
        painter.galley(pos2(x, qh.center().y - g.size().y / 2.0), g, p.text_faint);
        painter.text(
            pos2(x - 4.0, qh.center().y),
            Align2::RIGHT_CENTER,
            icons::CHECK_CIRCLE,
            theme::icon_font(12.0),
            p.ok,
        );
    }
    let schema = tab.graphql.schema.clone();
    let provider = |text: &str, cursor: usize| -> Option<(usize, Vec<widgets::CompletionItem>)> {
        let schema = schema.as_ref()?;
        let word_start = widgets::word_start(&text[..cursor], |c| c.is_alphanumeric() || c == '_');
        if word_start == cursor {
            return None;
        }
        let items: Vec<widgets::CompletionItem> = ternarywire_convert::graphql::suggest(schema, text, cursor)
            .into_iter()
            .map(|s| (s.label, s.insert, s.detail))
            .collect();
        Some((word_start, items))
    };
    let editor = |ui: &mut Ui, r: Rect, salt: &str, add: &mut dyn FnMut(&mut Ui, usize)| {
        let rows = fill_rows(ui, r.height());
        ui.scope_builder(egui::UiBuilder::new().max_rect(r).layout(Layout::top_down(Align::Min)), |ui| {
            ui.set_clip_rect(r.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false; 2]).show(ui, |ui| add(ui, rows));
        });
    };
    editor(ui, q_rect, "gql-query-scroll", &mut |ui, rows| {
        widgets::code_editor_gutter(
            ui,
            "gql-query",
            &mut body.graphql.query,
            Lang::GraphQl,
            Some(&ectx.vars),
            rows,
            false,
            Some(&provider),
        );
    });
    editor(ui, v_rect, "gql-vars-scroll", &mut |ui, rows| {
        widgets::code_editor_gutter(
            ui,
            "gql-vars",
            &mut body.graphql.variables,
            Lang::Json,
            Some(&ectx.vars),
            rows,
            false,
            None,
        );
    });
    if explorer_w > 0.0 {
        let er = Rect::from_min_max(pos2(left.right(), rect.top()), rect.max);
        painter.rect_filled(er, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, p.well);
        painter.vline(er.left() + 0.5, er.y_range(), Stroke::new(1.0, p.border));
        schema_explorer(ui, er.shrink2(vec2(0.0, 1.0)), tab, body, actions);
    }
    painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.advance_cursor_after_rect(avail);
}

/// The schema drawer: types, a filter, and the root fields with a button
/// that writes an operation for each.
fn schema_explorer(ui: &mut Ui, rect: Rect, tab: &mut RequestTab, body: &mut Body, actions: &mut Actions) {
    let p = theme::pal(ui);
    let head = Rect::from_min_size(pos2(rect.left() + 1.0, rect.top()), vec2(rect.width() - 1.0, 35.0));
    ui.painter().hline(head.x_range(), head.bottom() + 0.5, Stroke::new(1.0, p.border));
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(head.shrink2(vec2(10.0, 0.0))).layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(kit::section_title(tr("Schema")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if kit::icon_button_sm(ui, icons::X, tr("Close schema")).clicked() {
                    tab.graphql.explorer_open = false;
                }
                if tab.graphql.schema.is_some()
                    && !tab.graphql.loading
                    && kit::icon_button_sm(ui, icons::ARROWS_CLOCKWISE, tr("Fetch schema")).clicked()
                {
                    actions.fetch_schema = true;
                }
                if let Some(s) = &tab.graphql.schema {
                    ui.add_space(4.0);
                    kit::tone_pill(ui, &trf("{} types", &[&s.types.len()]), Tone::Ok);
                }
            });
        },
    );
    let body_rect = Rect::from_min_max(pos2(rect.left() + 1.0, head.bottom() + 1.0), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)), |ui| {
        ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        ui.spacing_mut().item_spacing.y = 0.0;
        let Some(schema) = tab.graphql.schema.clone() else {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(body_rect.width() - 24.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                if tab.graphql.loading {
                    ui.add(egui::Spinner::new().size(16.0).color(p.accent));
                    ui.label(kit::hint(ui, tr("Fetching schema…")));
                } else {
                    ui.add(
                        egui::Label::new(kit::hint(
                            ui,
                            tr("Load the schema for completions and to insert operations."),
                        ))
                        .wrap(),
                    );
                    let r = kit::button(
                        ui,
                        Kind::Secondary,
                        Size::Sm,
                        tr("Fetch schema"),
                        ButtonOpts { icon: Some(icons::ARROWS_CLOCKWISE), ..Default::default() },
                    );
                    if r.on_hover_text(tr("Run an introspection query against the URL")).clicked() {
                        actions.fetch_schema = true;
                    }
                }
                if let Some(e) = &tab.graphql.error {
                    ui.add(egui::Label::new(RichText::new(e).size(11.5).color(p.err)).wrap());
                }
            });
            return;
        };
        egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            kit::search_field_sm(ui, "gql-filter", &mut tab.graphql.filter, tr("Filter fields"), ui.available_width());
        });
        let filter = tab.graphql.filter.to_lowercase();
        egui::ScrollArea::vertical().id_salt("gql-explorer").auto_shrink([false; 2]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 6, top: 0, bottom: 8 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let w = ui.available_width();
                for (op, label) in [("query", "Query"), ("mutation", "Mutation"), ("subscription", "Subscription")] {
                    let Some(root_type) = schema.root(op).and_then(|r| schema.get(r)) else { continue };
                    let id = ui.make_persistent_id(("gql-root", op));
                    let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(op != "subscription"));
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
                    widgets::a11y_button(&resp, label);
                    if resp.clicked() {
                        open = !open;
                        ui.data_mut(|d| d.insert_temp(id, open));
                    }
                    let caret = if open || !filter.is_empty() { icons::CARET_DOWN } else { icons::CARET_RIGHT };
                    ui.painter().text(
                        pos2(r.left() + 10.0, r.center().y),
                        Align2::CENTER_CENTER,
                        caret,
                        theme::icon_font(11.0),
                        p.text_faint,
                    );
                    let g = ui.painter().layout_no_wrap(label.to_string(), theme::medium(12.0), p.text);
                    let gx = r.left() + 20.0;
                    let gw = g.size().x;
                    ui.painter().galley(pos2(gx, r.center().y - g.size().y / 2.0), g, p.text);
                    ui.painter().text(
                        pos2(gx + gw + 6.0, r.center().y),
                        Align2::LEFT_CENTER,
                        root_type.fields.len().to_string(),
                        egui::FontId::proportional(11.0),
                        p.text_faint,
                    );
                    if !open && filter.is_empty() {
                        continue;
                    }
                    for field in &root_type.fields {
                        if !filter.is_empty() && !field.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let (row, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
                        let plus =
                            Rect::from_min_size(pos2(row.left() + 18.0, row.center().y - 11.0), vec2(22.0, 22.0));
                        let add = kit::icon_button_sm(
                            &mut ui.new_child(egui::UiBuilder::new().max_rect(plus)),
                            icons::PLUS,
                            tr("Insert an operation for this field"),
                        );
                        if add.clicked() {
                            body.graphql.query = ternarywire_convert::graphql::skeleton(&schema, op, &field.name);
                            body.graphql.variables =
                                ternarywire_convert::graphql::skeleton_variables(&schema, op, &field.name);
                        }
                        let x = plus.right() + 6.0;
                        let name_color = if field.deprecated { p.text_faint } else { p.text };
                        let ng = elide(ui, &field.name, theme::mono(11.5), name_color, (row.right() - x) * 0.6);
                        let nw = ng.size().x;
                        ui.painter().galley(pos2(x, row.center().y - ng.size().y / 2.0), ng, name_color);
                        if field.deprecated {
                            ui.painter().hline(x..=x + nw, row.center().y, Stroke::new(1.0, p.text_faint));
                        }
                        let tx = x + nw + 6.0;
                        if tx < row.right() - 12.0 {
                            let tg =
                                elide(ui, &field.ty.display(), theme::mono(11.0), p.text_faint, row.right() - tx - 4.0);
                            ui.painter().galley(pos2(tx, row.center().y - tg.size().y / 2.0), tg, p.text_faint);
                        }
                        if let Some(d) = field.description.as_ref().filter(|d| !d.is_empty()) {
                            resp.on_hover_text(d);
                        }
                    }
                    ui.add_space(4.0);
                }
            });
        });
    });
}
/// The tokens of a GraphQL document for [`prettify_graphql`]: braces on
/// their own, strings (`"…"`, `"""…"""`) and comments whole (they may hold
/// braces and spaces), the rest split on whitespace.
fn graphql_tokens(q: &str) -> Vec<&str> {
    let b = q.as_bytes();
    let mut out = Vec::new();
    let mut word: Option<usize> = None;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' | b'}' => {
                out.extend(word.take().map(|s| &q[s..i]));
                out.push(&q[i..i + 1]);
                i += 1;
            }
            b'"' => {
                word.get_or_insert(i);
                if q[i..].starts_with("\"\"\"") {
                    i = q[i + 3..].find("\"\"\"").map_or(b.len(), |e| i + 3 + e + 3);
                } else {
                    i += 1;
                    while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                    i = (i + 1).min(b.len());
                }
            }
            b'#' if word.is_none() => {
                let end = q[i..].find('\n').map_or(b.len(), |e| i + e);
                out.push(q[i..end].trim_end());
                i = end;
            }
            c if c.is_ascii_whitespace() => {
                out.extend(word.take().map(|s| &q[s..i]));
                i += 1;
            }
            _ => {
                word.get_or_insert(i);
                i += 1;
            }
        }
    }
    out.extend(word.map(|s| &q[s..]));
    out
}

fn prettify_graphql(q: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for token in graphql_tokens(q) {
        match token {
            "{" => {
                out.push_str(" {\n");
                depth += 1;
                out.push_str(&"  ".repeat(depth));
            }
            "}" => {
                depth = depth.saturating_sub(1);
                while out.ends_with(' ') {
                    out.pop();
                }
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&"  ".repeat(depth));
                out.push_str("}\n");
                out.push_str(&"  ".repeat(depth));
            }
            // A comment ends its line.
            t if t.starts_with('#') => {
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push('\n');
                    out.push_str(&"  ".repeat(depth));
                }
                out.push_str(t);
                out.push('\n');
                out.push_str(&"  ".repeat(depth));
            }
            t => {
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') && !out.ends_with("  ") {
                    if t.starts_with('(') || t.starts_with(':') || out.ends_with('(') {
                    } else {
                        out.push('\n');
                        out.push_str(&"  ".repeat(depth));
                    }
                }
                out.push_str(t);
                if t.ends_with(':') || t.ends_with('(') || t.ends_with(',') {
                    out.push(' ');
                }
            }
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

fn script_code(scripts: &mut ternarywire_core::model::Scripts, event: ScriptEvent) -> &mut String {
    match event {
        ScriptEvent::PreRequest => &mut scripts.pre_request,
        ScriptEvent::PostResponse => &mut scripts.post_response,
        ScriptEvent::OnMessage => &mut scripts.on_message,
    }
}

fn scripts_section(
    ui: &mut Ui,
    tab: &mut RequestTab,
    scripts: &mut ternarywire_core::model::Scripts,
    vars: &VarCtx,
    actions: &mut Actions,
) {
    let has_response = tab.result.as_ref().is_some_and(|r| r.exec.response.is_some());
    let post = tab.script_event == ScriptEvent::PostResponse;
    let event = &mut tab.script_event;
    kit::toolbar(ui, |ui| {
        script_event_seg(ui, event, scripts);
        let event = *event;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            snippets_button(ui, event, script_code(scripts, event));
            if post {
                let r = ui
                    .add_enabled_ui(has_response, |ui| kit::ghost(ui, Some(icons::TEST_TUBE), tr("Generate tests")))
                    .inner;
                let r = if has_response {
                    r.on_hover_text(tr("Write tests for the status, time, content type and body shape of the response"))
                } else {
                    r.on_disabled_hover_text(tr("Send the request first: tests are generated from its response."))
                };
                if r.clicked() {
                    actions.generate_tests = true;
                }
            }
        });
    });
    let event = tab.script_event;
    let code = script_code(scripts, event);
    editor_well(ui, if event == ScriptEvent::PreRequest { "pre-script" } else { "post-script" }, 38.0, |ui, rows| {
        widgets::code_editor_gutter(
            ui,
            ("script", event == ScriptEvent::PreRequest),
            code,
            Lang::JavaScript,
            Some(vars),
            rows,
            false,
            Some(&widgets::pm_completions),
        );
    });
    ui.add_space(8.0);
    kit::bar_ex(ui, 24.0, 14.0, false, |ui| {
        let p = theme::pal(ui);
        ui.label(RichText::new(icons::INFO).font(theme::icon_font(13.0)).color(p.text_faint));
        ui.add(
            egui::Label::new(kit::hint(
                ui,
                &format!(
                    "{} · {}",
                    tr("JavaScript with the Postman-compatible pm API"),
                    tr("type pm. for completions")
                ),
            ))
            .truncate(),
        );
    });
}

/// Pre-request / Post-response (small segmented), with a dot on the
/// scripts that have code.
fn script_event_seg(ui: &mut Ui, event: &mut ScriptEvent, scripts: &ternarywire_core::model::Scripts) {
    let p = theme::pal(ui);
    let pre = tr("Pre-request");
    let post = tr("Post-response");
    let r = ui
        .scope(|ui| kit::segmented_sm(ui, event, &[(ScriptEvent::PreRequest, pre), (ScriptEvent::PostResponse, post)]));
    // Segments are their text (medium 11.5) plus 16 wide, 1 apart.
    let seg = |text: &str| {
        ui.painter().layout_no_wrap(text.to_string(), theme::medium(11.5), Color32::PLACEHOLDER).size().x + 16.0
    };
    let (w_pre, w_post) = (seg(pre), seg(post));
    let rect = r.response.rect;
    for (has, right) in [
        (!scripts.pre_request.trim().is_empty(), rect.left() + 2.0 + w_pre),
        (!scripts.post_response.trim().is_empty(), rect.left() + 3.0 + w_pre + w_post),
    ] {
        if has {
            // In the top right corner of the segment (26 high, centered).
            ui.painter().circle_filled(pos2(right - 5.0, rect.center().y - 7.0), 2.5, p.ok);
        }
    }
}

/// The Snippets menu button: ready-made snippets by group and the pm API
/// reference.
fn snippets_button(ui: &mut Ui, event: ScriptEvent, code: &mut String) {
    let r = kit::button(
        ui,
        Kind::Secondary,
        Size::Sm,
        tr("Snippets"),
        ButtonOpts { icon: Some(icons::LIGHTNING), caret: true, ..Default::default() },
    );
    egui::Popup::menu(&r)
        .width(260.0)
        .align(egui::RectAlign::BOTTOM_END)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| snippets_menu(ui, event, code));
}

fn snippets_menu(ui: &mut Ui, event: ScriptEvent, code: &mut String) {
    let p = theme::pal(ui);
    let filter_id = ui.make_persistent_id(("snippet-filter", event == ScriptEvent::PreRequest));
    let mut filter: String = ui.data(|d| d.get_temp(filter_id).unwrap_or_default());
    egui::Frame::new().inner_margin(egui::Margin { left: 4, right: 4, top: 2, bottom: 4 }).show(ui, |ui| {
        if kit::search_field_sm(ui, filter_id.with("field"), &mut filter, tr("Filter snippets"), ui.available_width())
            .changed()
        {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
    });
    let needle = filter.to_lowercase();
    let max_h = (ui.ctx().content_rect().height() * 0.55).clamp(200.0, 420.0);
    egui::ScrollArea::vertical().id_salt("snippets").max_height(max_h).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut any = false;
        for group in crate::snippets::Group::ALL {
            let mut shown = crate::snippets::SNIPPETS
                .iter()
                .filter(|s| s.group == group)
                .filter(|s| event == ScriptEvent::PostResponse || !s.post_only)
                .filter(|s| needle.is_empty() || s.title.to_lowercase().contains(&needle))
                .peekable();
            if shown.peek().is_none() {
                continue;
            }
            kit::menu_header(ui, group.label());
            for s in shown {
                any = true;
                let item = kit::MenuItem { icon: Some(icons::PLUS), label: s.title, ..Default::default() };
                if kit::menu_item_ex(ui, item).on_hover_text(RichText::new(s.code).monospace()).clicked() {
                    insert_snippet(code, s.code);
                }
            }
        }
        if !any {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(9.0);
                ui.label(kit::hint(ui, tr("No snippets match.")));
            });
            ui.add_space(6.0);
        }
    });
    kit::menu_sep(ui);
    let api = kit::menu_item_ex(
        ui,
        kit::MenuItem {
            icon: Some(icons::BOOK_OPEN),
            label: tr("pm API reference"),
            submenu: true,
            ..Default::default()
        },
    );
    egui::containers::menu::SubMenu::new().show(ui, &api, |ui| {
        ui.set_min_width(300.0);
        egui::ScrollArea::vertical().id_salt("pm-api").max_height(max_h).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (sig, doc) in crate::snippets::PM_COMPLETIONS {
                if !needle.is_empty() && !sig.to_lowercase().contains(&needle) {
                    continue;
                }
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
                }
                let w = rect.width() - 18.0;
                let g = elide(ui, sig, theme::mono(11.5), p.text, w);
                ui.painter().galley(pos2(rect.left() + 9.0, rect.top() + 5.0), g, p.text);
                let g = elide(ui, tr(doc), egui::FontId::proportional(11.5), p.text_muted, w);
                ui.painter().galley(pos2(rect.left() + 9.0, rect.bottom() - 5.0 - g.size().y), g, p.text_muted);
                resp.on_hover_text(*doc);
            }
        });
    });
}

fn insert_snippet(code: &mut String, snippet: &str) {
    if !code.is_empty() && !code.ends_with('\n') {
        code.push('\n');
    }
    code.push_str(snippet);
}

/// A clickable row of a side list: icon, text, hover fill (accent fill when
/// `selected`).
#[allow(dead_code)] // Shared with the other views' side lists.
pub(crate) fn list_row(ui: &mut Ui, icon: &str, text: &str, selected: bool) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), size::TREE_ROW), Sense::click());
    widgets::a11y_button(&resp, text);
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if selected {
            ui.painter().rect_filled(rect, CornerRadius::same(6), p.accent_soft);
        } else if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
        }
        let icon_color = if selected {
            p.accent_text
        } else if hovered {
            p.text
        } else {
            p.text_muted
        };
        ui.painter().text(
            pos2(rect.left() + 9.0 + 8.0, rect.center().y),
            Align2::CENTER_CENTER,
            icon,
            theme::icon_font(15.0),
            icon_color,
        );
        let color = if selected { p.accent_text } else { p.text };
        let g = elide(ui, text, egui::FontId::proportional(12.5), color, rect.width() - 42.0);
        ui.painter().galley(pos2(rect.left() + 34.0, rect.center().y - g.size().y / 2.0), g, color);
    }
    resp
}

// ---------------------------------------------------------------------------
// Settings, Docs
// ---------------------------------------------------------------------------

/// Number of settings this request overrides.
fn settings_overridden(s: &RequestSettings) -> usize {
    [
        s.follow_redirects.is_some(),
        s.follow_original_method.is_some(),
        s.follow_auth_header.is_some(),
        s.verify_ssl.is_some(),
        s.encode_url.is_some(),
        s.disable_cookies.is_some(),
        s.max_redirects.is_some(),
        s.timeout_ms.is_some(),
        s.http_version.is_some(),
    ]
    .iter()
    .filter(|b| **b)
    .count()
}

/// An on/off setting that can also follow the app default.
fn tri_state(ui: &mut Ui, rows: &mut kit::GroupRows, label: &str, help: &str, value: &mut Option<bool>, default: bool) {
    ui.push_id(label, |ui| {
        rows.row(ui, label, help, |ui| {
            let default_label = if default { tr("Default (on)") } else { tr("Default (off)") };
            kit::segmented_sm(ui, value, &[(None, default_label), (Some(true), tr("On")), (Some(false), tr("Off"))]);
        });
    });
}

/// A number setting (empty: the default), right-aligned in a small field.
fn number_setting<T: std::str::FromStr + ToString + Copy>(
    ui: &mut Ui,
    rows: &mut kit::GroupRows,
    label: &str,
    help: &str,
    value: &mut Option<T>,
    default: &str,
    unit: &str,
) {
    rows.row(ui, label, help, |ui| {
        if !unit.is_empty() {
            ui.label(RichText::new(unit).size(12.5).color(theme::pal(ui).text_faint));
        }
        let mut v = value.map(|v| v.to_string()).unwrap_or_default();
        let r = ui.add(
            egui::TextEdit::singleline(&mut v)
                .hint_text(RichText::new(default).color(theme::pal(ui).text_faint))
                .desired_width(if unit.is_empty() { 64.0 } else { 74.0 })
                .font(egui::FontId::proportional(12.0))
                .min_size(vec2(0.0, 26.0))
                .align(Align2::RIGHT_CENTER)
                .margin(egui::Margin::symmetric(8, 0)),
        );
        widgets::a11y_name(&r, label);
        if r.changed() {
            *value = v.trim().parse().ok();
        }
    });
}

fn settings_section(ui: &mut Ui, s: &mut RequestSettings, d: &ternarywire_core::settings::RequestDefaults) {
    let overridden = settings_overridden(s) > 0;
    kit::toolbar(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = ui
                .add_enabled_ui(overridden, |ui| {
                    kit::ghost(ui, Some(icons::ARROW_COUNTER_CLOCKWISE), tr("Reset to defaults"))
                })
                .inner;
            if r.clicked() {
                let disabled = std::mem::take(&mut s.disabled_default_headers);
                *s = RequestSettings { disabled_default_headers: disabled, ..Default::default() };
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(kit::hint(
                        ui,
                        tr("These apply to this request only and override the app settings."),
                    ))
                    .truncate(),
                );
            });
        });
    });
    kit::pane_body(ui, "settings-body", |ui| {
        ui.spacing_mut().item_spacing.y = 18.0;
        kit::group(ui, tr("Redirects"), "", |ui, rows| {
            tri_state(
                ui,
                rows,
                tr("Follow redirects"),
                tr("Follow 3xx responses to their Location."),
                &mut s.follow_redirects,
                d.follow_redirects,
            );
            tri_state(
                ui,
                rows,
                tr("Follow original HTTP method"),
                tr("Redirect with the original method instead of switching to GET."),
                &mut s.follow_original_method,
                false,
            );
            tri_state(
                ui,
                rows,
                tr("Keep Authorization header on redirect"),
                tr("Also send it when the redirect goes to another host."),
                &mut s.follow_auth_header,
                false,
            );
            number_setting(
                ui,
                rows,
                tr("Maximum redirects"),
                tr("Stop after this many redirects."),
                &mut s.max_redirects,
                &d.max_redirects.to_string(),
                "",
            );
        });
        kit::group(ui, tr("Connection"), "", |ui, rows| {
            tri_state(
                ui,
                rows,
                tr("Verify SSL certificates"),
                tr("Reject servers whose TLS certificate is not trusted."),
                &mut s.verify_ssl,
                d.verify_ssl,
            );
            number_setting(
                ui,
                rows,
                tr("Request timeout"),
                tr("Cancel the request after this long; 0 waits forever."),
                &mut s.timeout_ms,
                tr("default"),
                "ms",
            );
            rows.row(ui, tr("HTTP version"), tr("HTTP/2 is used only with servers that support it."), |ui| {
                let mut v = if s.http_version == Some(HttpVersion::Auto) { None } else { s.http_version };
                if kit::segmented_sm(
                    ui,
                    &mut v,
                    &[(None, tr("Auto")), (Some(HttpVersion::Http1), "HTTP/1.x"), (Some(HttpVersion::Http2), "HTTP/2")],
                ) {
                    s.http_version = v;
                }
            });
        });
        kit::group(ui, tr("Request"), "", |ui, rows| {
            tri_state(
                ui,
                rows,
                tr("Encode URL automatically"),
                tr("Percent-encode reserved characters in the path and query."),
                &mut s.encode_url,
                d.encode_url,
            );
            tri_state(
                ui,
                rows,
                tr("Disable cookie jar"),
                tr("Do not send stored cookies or keep the ones this response sets."),
                &mut s.disable_cookies,
                false,
            );
        });
    });
}

fn docs_section(ui: &mut Ui, tab: &mut RequestTab, item: &mut RequestItem) {
    let p = theme::pal(ui);
    kit::toolbar(ui, |ui| {
        ui.label(kit::section_title(tr("Description")));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            kit::segmented_sm(ui, &mut tab.docs_preview, &[(false, tr("Write")), (true, tr("Preview"))]);
        });
    });
    let preview = tab.docs_preview;
    kit::pane_body(ui, "docs-body", |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        if preview {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(theme::RADIUS))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(5.0 * 19.5);
                    if item.description.trim().is_empty() {
                        ui.label(RichText::new(tr("No description yet.")).color(p.text_faint));
                    } else {
                        markdown_preview(ui, &item.description);
                    }
                });
        } else {
            let r = ui.add(
                egui::TextEdit::multiline(&mut item.description)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY)
                    .font(egui::FontId::proportional(13.0))
                    .margin(egui::Margin::symmetric(10, 8))
                    .hint_text(RichText::new(tr("Describe what the request does…")).color(p.text_faint)),
            );
            widgets::a11y_name(&r, tr("Description"));
        }
        ui.label(kit::hint(ui, tr("Markdown. Shown in the generated collection documentation.")));
        hrow(ui, 22.0, |ui| {
            ui.label(kit::section_title(tr("Examples")));
            if !item.examples.is_empty() {
                kit::tone_pill(ui, &item.examples.len().to_string(), Tone::Neutral);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(kit::hint(ui, tr("Also served by the mock server")));
            });
        });
        if item.examples.is_empty() {
            kit::banner(
                ui,
                Tone::Neutral,
                icons::BOOKMARK_SIMPLE,
                tr("Use “Save as example” on a response to document it and to serve it from a mock server."),
            );
        }
        let mut remove = None;
        for (i, ex) in item.examples.iter_mut().enumerate() {
            ui.push_id(ex.id.clone(), |ui| {
                if example_card(ui, ex) {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            item.examples.remove(i);
        }
    });
}

/// One saved example: status, name and language; open, its name, status
/// and body can be edited. Returns true when it should be deleted.
fn example_card(ui: &mut Ui, ex: &mut Example) -> bool {
    let p = theme::pal(ui);
    let mut delete = false;
    kit::card(ui).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let id = ui.make_persistent_id("ex-open");
        let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(false));
        let head = hrow(ui, 22.0, |ui| {
            let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
            ui.add(
                egui::Label::new(RichText::new(caret).font(theme::icon_font(12.0)).color(p.text_faint))
                    .selectable(false),
            );
            kit::pill_ex(ui, &ex.status.to_string(), theme::status_tone(ex.status), PillOpts::default());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::icon_button_sm(ui, icons::TRASH, tr("Delete example")).clicked() {
                    delete = true;
                }
                if let Some(l) = &ex.language {
                    ui.label(RichText::new(l.to_uppercase()).size(11.0).color(p.text_faint));
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&ex.name).font(theme::medium(13.0)).color(p.text))
                            .selectable(false)
                            .truncate(),
                    );
                });
            });
        });
        let toggle_rect = Rect::from_min_max(
            head.response.rect.min,
            pos2(head.response.rect.right() - 60.0, head.response.rect.bottom()),
        );
        let toggle = ui.interact(toggle_rect, id.with("head"), Sense::click());
        widgets::a11y_button(&toggle, &ex.name);
        if toggle.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(id, open));
        }
        if !open {
            return;
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(RichText::new(tr("Name")).size(12.0).color(p.text_muted));
            let r = ui.add(
                egui::TextEdit::singleline(&mut ex.name)
                    .desired_width((ui.available_width() - 120.0).max(100.0))
                    .font(egui::FontId::proportional(12.0))
                    .min_size(vec2(0.0, 26.0))
                    .align(Align2::LEFT_CENTER)
                    .margin(egui::Margin::symmetric(8, 0)),
            );
            widgets::a11y_name(&r, tr("Name"));
            ui.label(RichText::new(tr("Status")).size(12.0).color(p.text_muted));
            let mut code = ex.status.to_string();
            let r = ui.add(
                egui::TextEdit::singleline(&mut code)
                    .desired_width(44.0)
                    .font(egui::FontId::proportional(12.0))
                    .min_size(vec2(0.0, 26.0))
                    .align(Align2::LEFT_CENTER)
                    .margin(egui::Margin::symmetric(8, 0)),
            );
            widgets::a11y_name(&r, tr("Status"));
            if r.changed()
                && let Ok(c) = code.parse()
            {
                ex.status = c;
            }
        });
        ui.add_space(8.0);
        let lang = match ex.language.as_deref() {
            Some("json") => Lang::Json,
            Some("xml") | Some("html") => Lang::Xml,
            _ => Lang::Text,
        };
        kit::well(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // The editor without its own frame: the well is the frame.
            let v = ui.visuals_mut();
            v.text_edit_bg_color = Some(Color32::TRANSPARENT);
            v.extreme_bg_color = Color32::TRANSPARENT;
            v.selection.stroke = Stroke::NONE;
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
                w.bg_stroke = Stroke::NONE;
            }
            widgets::code_editor(ui, "ex-body", &mut ex.body, lang, None, 6, false);
        });
    });
    delete
}

/// A light Markdown rendering: headings (`#`), bullet lists (`-`, `*`),
/// paragraphs with `code`, **bold** and *italic*.
fn markdown_preview(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.y = 6.0;
    for line in text.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            ui.add_space(2.0);
            continue;
        }
        let level = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && t[level..].starts_with(' ') {
            let size = match level {
                1 => 17.0,
                2 => 15.0,
                _ => 13.5,
            };
            ui.add(egui::Label::new(RichText::new(t[level..].trim()).font(theme::semibold(size)).color(p.text)).wrap());
            continue;
        }
        let trimmed = t.trim_start();
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add_space(4.0);
                ui.label(RichText::new("•").color(p.text_muted));
                ui.add(egui::Label::new(inline_markdown(ui, rest)).wrap());
            });
            continue;
        }
        ui.add(egui::Label::new(inline_markdown(ui, t)).wrap());
    }
}

/// `code`, **bold** and *italic* spans of one line of Markdown.
fn inline_markdown(ui: &Ui, text: &str) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    let plain = TextFormat::simple(egui::FontId::proportional(13.0), p.text);
    let mut rest = text;
    while !rest.is_empty() {
        let next = ["`", "**", "*"]
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, *m)))
            .min_by_key(|(i, m)| (*i, -(m.len() as i32)));
        let Some((i, marker)) = next else {
            job.append(rest, 0.0, plain.clone());
            break;
        };
        let after = &rest[i + marker.len()..];
        let Some(end) = after.find(marker) else {
            job.append(rest, 0.0, plain.clone());
            break;
        };
        job.append(&rest[..i], 0.0, plain.clone());
        let inner = &after[..end];
        let format = match marker {
            "`" => TextFormat { font_id: theme::mono(12.0), color: p.text, background: p.subtle, ..Default::default() },
            "**" => TextFormat::simple(theme::semibold(13.0), p.text),
            _ => TextFormat { italics: true, ..plain.clone() },
        };
        job.append(inner, 0.0, format);
        rest = &after[end + marker.len()..];
    }
    job
}

// ---------------------------------------------------------------------------
// Response pane
// ---------------------------------------------------------------------------

fn response_pane(ui: &mut Ui, tab: &mut RequestTab, ectx: &EditCtx, actions: &mut Actions) {
    if tab.panel == PanelMode::Minimized && ectx.side_layout {
        minimized_rail(ui, tab);
        return;
    }
    if let Some(sub) = &tab.gql_sub {
        let mut panel = tab.panel;
        subscription_panel(ui, sub, &mut panel, ectx.side_layout, actions);
        tab.panel = panel;
        return;
    }
    status_row(ui, tab, ectx, actions);
    if tab.panel == PanelMode::Minimized {
        return;
    }
    response_tabs(ui, tab);
    response_content(ui, tab, ectx, actions);
}

/// Pass/fail of the result's tests, for the status row and the Tests tab.
fn tests_tone(exec: &Execution) -> Tone {
    if exec.tests_failed() > 0 || !exec.script_errors.is_empty() { Tone::Err } else { Tone::Ok }
}

/// A Phosphor icon turning around its center (spinners).
fn spinning_icon(ui: &Ui, center: egui::Pos2, icon: &str, size: f32, color: Color32) {
    let t = ui.input(|i| i.time) as f32;
    let g = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(size), color);
    let half = g.size() / 2.0;
    let angle = t * 6.0;
    let rot = egui::emath::Rot2::from_angle(angle);
    let mut shape = egui::epaint::TextShape::new(center - rot * half, g, color);
    shape.angle = angle;
    ui.painter().add(shape);
    ui.ctx().request_repaint();
}

/// A large pill (24) with a spinning icon: "Sending…".
fn spinner_pill(ui: &mut Ui, text: &str, tone: Tone) {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::medium(12.0), fg);
    let (rect, _) = ui.allocate_exact_size(vec2(9.0 + 13.0 + 5.0 + g.size().x + 9.0, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(12), bg);
    spinning_icon(ui, pos2(rect.left() + 9.0 + 6.5, rect.center().y), icons::CIRCLE_NOTCH, 13.0, fg);
    ui.painter().galley(pos2(rect.left() + 27.0, rect.center().y - g.size().y / 2.0), g, fg);
}

/// The status row (56): status, time, size and tests on the left; save as
/// example, ⋯, maximize and minimize on the right.
fn status_row(ui: &mut Ui, tab: &mut RequestTab, ectx: &EditCtx, actions: &mut Actions) {
    let p = theme::pal(ui);
    let minimized = tab.panel == PanelMode::Minimized;
    if minimized {
        // The whole bar brings the response back.
        let r = Rect::from_min_size(ui.available_rect_before_wrap().min, vec2(ui.available_width(), size::ACTION_ROW));
        let bar = ui
            .interact(r, ui.make_persistent_id("resp-min-bar"), Sense::click())
            .on_hover_text(tr("Show the response"));
        widgets::a11y_button(&bar, tr("Show the response"));
        if bar.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if bar.clicked() {
            tab.panel = PanelMode::Normal;
        }
    }
    let side = ectx.side_layout;
    kit::bar_ex(ui, size::ACTION_ROW, 10.0, true, |ui| {
        ui.add_space(4.0);
        let exec = tab.result.as_ref().map(|r| &r.exec);
        if let Some(r) = &tab.running {
            spinner_pill(ui, tr("Sending…"), Tone::Info);
            ui.label(
                RichText::new(format!("{:.2} s", r.started.elapsed().as_secs_f64()))
                    .font(theme::mono(12.0))
                    .color(p.text_muted),
            );
        } else if let Some(resp) = exec.and_then(|e| e.response.as_ref()) {
            let exec = exec.expect("result");
            kit::status_pill(ui, resp.status, &resp.status_text).on_hover_text(format!(
                "{}{}",
                resp.version,
                resp.remote_addr.as_ref().map(|a| format!(" · {a}")).unwrap_or_default()
            ));
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let time = kit::metric(ui, icons::TIMER, &ternarywire_core::util::human_ms(resp.timings.total_ms));
                let time =
                    ui.interact(time.rect, ui.make_persistent_id("timing"), Sense::click()).on_hover_text(tr("Timing"));
                egui::Popup::menu(&time)
                    .width(280.0)
                    .show(|ui| timing_popover(ui, resp.timings.ttfb_ms, resp.timings.total_ms));
                kit::metric(ui, icons::ARROW_LINE_DOWN, &ternarywire_core::util::human_bytes(resp.total_size()))
                    .on_hover_ui(|ui| {
                        ui.label(trf("Headers: {}", &[&ternarywire_core::util::human_bytes(resp.header_size)]));
                        let body = ternarywire_core::util::human_bytes(resp.body.len());
                        ui.label(if resp.body.is_file() {
                            trf("Body: {} (stored on disk)", &[&body])
                        } else {
                            trf("Body: {}", &[&body])
                        });
                    });
                if !exec.tests.is_empty() {
                    let tone = tests_tone(exec);
                    let (fg, _) = p.tone(tone);
                    let icon = if tone == Tone::Err { icons::X_CIRCLE } else { icons::CHECK_CIRCLE };
                    let text = trf("{}/{} tests", &[&exec.tests_passed(), &exec.tests.len()]);
                    let m = kit::metric_colored(ui, icon, &text, Some(fg));
                    let m = ui
                        .interact(m.rect, ui.make_persistent_id("tests-metric"), Sense::click())
                        .on_hover_text(tr("Tests"));
                    widgets::a11y_button(&m, &text);
                    if m.clicked() {
                        tab.resp_section = RespSection::Tests;
                        tab.panel = if minimized { PanelMode::Normal } else { tab.panel };
                    }
                }
            });
        } else if let Some(err) = exec.and_then(|e| e.error.as_ref()) {
            kit::pill_ex(
                ui,
                tr(err.kind_label()),
                Tone::Err,
                PillOpts { large: true, icon: Some(icons::WARNING_CIRCLE), ..Default::default() },
            );
        } else if exec.is_some_and(|e| e.skipped) {
            kit::pill_ex(ui, tr("Skipped"), Tone::Neutral, PillOpts { large: true, ..Default::default() });
        } else {
            ui.label(RichText::new(tr("Response")).font(theme::semibold(13.0)).color(p.text));
            ui.label(RichText::new("·").color(p.text_faint));
            ui.label(kit::hint(ui, tr("Not sent yet")));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let maximized = tab.panel == PanelMode::Maximized;
            if minimized {
                let icon = if side { icons::CARET_LEFT } else { icons::CARET_UP };
                if kit::icon_button_sm(ui, icon, tr("Restore layout")).clicked() {
                    tab.panel = PanelMode::Normal;
                }
            } else {
                let icon = if side { icons::CARET_RIGHT } else { icons::CARET_DOWN };
                if kit::icon_button_sm(ui, icon, tr("Minimize response")).clicked() {
                    tab.panel = PanelMode::Minimized;
                }
            }
            let (icon, tip) = if maximized {
                (icons::ARROWS_IN, tr("Restore layout"))
            } else {
                (icons::ARROWS_OUT, tr("Maximize response"))
            };
            if kit::icon_button_ex(ui, icon, tip, maximized, 22.0).clicked() {
                tab.panel = if maximized { PanelMode::Normal } else { PanelMode::Maximized };
            }
            ui.add_space(-4.0);
            kit::vdivider(ui, 18.0);
            ui.add_space(-4.0);
            let exec = tab.result.as_ref().map(|r| &r.exec);
            let has_response = tab.running.is_none() && exec.is_some_and(|e| e.response.is_some());
            let more = kit::icon_button_sm(ui, icons::DOTS_THREE, tr("More actions"));
            egui::Popup::menu(&more)
                .width(250.0)
                .align(egui::RectAlign::BOTTOM_END)
                .show(|ui| response_menu(ui, exec.filter(|_| has_response), actions));
            if has_response && kit::icon_button_sm(ui, icons::BOOKMARK_SIMPLE, tr("Save as example")).clicked() {
                actions.save_example = true;
            }
        });
    });
}

/// The ⋯ menu of the response (disabled until there is a response).
fn response_menu(ui: &mut Ui, exec: Option<&Execution>, actions: &mut Actions) {
    let entry = |icon, label| kit::MenuItem { icon: Some(icon), label, ..Default::default() };
    ui.add_enabled_ui(exec.is_some(), |ui| {
        if kit::menu_item_ex(ui, entry(icons::BOOKMARK_SIMPLE, tr("Save as example"))).clicked() {
            actions.save_example = true;
        }
        if kit::menu_item_ex(ui, entry(icons::TEST_TUBE, tr("Generate tests")))
            .on_hover_text(tr("Write tests for the status, time, content type and body shape of the response"))
            .clicked()
        {
            actions.generate_tests = true;
        }
        if kit::menu_item_ex(ui, entry(icons::ARROW_CLOCKWISE, tr("Re-run tests")))
            .on_hover_text(tr("Run post-response scripts again on this response"))
            .clicked()
        {
            actions.rerun_tests = true;
        }
        kit::menu_sep(ui);
        if kit::menu_item_ex(ui, entry(icons::PUSH_PIN, tr("Keep as compare baseline")))
            .on_hover_text(tr("Compare the next responses with this one"))
            .clicked()
        {
            actions.pin_baseline = true;
        }
        if exec.is_some_and(|e| e.prepared.is_some())
            && kit::menu_item_ex(ui, entry(icons::TERMINAL, tr("Copy as cURL (as sent)")))
                .on_hover_text(tr("The request that produced this response, with variables and auth applied"))
                .clicked()
        {
            actions.copy_curl = true;
        }
        if let Some(e) = exec.filter(|e| e.visualization.is_some())
            && kit::menu_item_ex(ui, entry(icons::ARROW_SQUARE_OUT, tr("Open visualization in browser"))).clicked()
        {
            open_visualization(e);
        }
    });
}

/// The phases of the response time as bars on one scale.
fn waterfall(ui: &mut Ui, ttfb: f64, total: f64, label_w: f32, value_w: f32, font: f32, rows: &[(&str, &str)]) {
    let p = theme::pal(ui);
    let total = total.max(ttfb).max(0.001);
    let phases = [(0.0, ttfb, p.accent), (ttfb, (total - ttfb).max(0.0), p.ok)];
    for ((label, help), (start, dur, color)) in rows.iter().zip(phases) {
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
        let g = elide(ui, label, egui::FontId::proportional(font), p.text_muted, label_w - 8.0);
        ui.painter().galley(pos2(row.left(), row.center().y - g.size().y / 2.0), g, p.text_muted);
        let track = Rect::from_min_max(
            pos2(row.left() + label_w + 12.0, row.center().y - 5.0),
            pos2(row.right() - value_w - 12.0, row.center().y + 5.0),
        );
        let x0 = track.left() + track.width() * (start / total) as f32;
        let w = (track.width() * (dur / total) as f32).max(3.0).min(track.right() - x0).max(1.0);
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(x0, track.top()), vec2(w, track.height())),
            CornerRadius::same(3),
            color,
        );
        ui.painter().text(
            pos2(row.right(), row.center().y),
            Align2::RIGHT_CENTER,
            fmt_ms(dur),
            theme::mono(font),
            p.text,
        );
        resp.on_hover_text(*help);
        ui.add_space(8.0);
    }
}

/// The popover of the time metric.
fn timing_popover(ui: &mut Ui, ttfb: f64, total: f64) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(9, 7)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.label(kit::section_title(tr("Timing")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(fmt_ms(total.max(ttfb))).font(theme::mono(12.5)).strong());
            });
        });
        ui.add_space(10.0);
        waterfall(
            ui,
            ttfb,
            total,
            100.0,
            50.0,
            11.5,
            &[
                (
                    tr("TTFB"),
                    tr("From sending the request to the first response byte: connecting, TLS and the server's work."),
                ),
                (tr("Download"), tr("Receiving the response body.")),
            ],
        );
    });
}

/// Body | Headers n | Cookies n | Tests p/t | Details | Compare | Visualize.
fn response_tabs(ui: &mut Ui, tab: &mut RequestTab) {
    let exec = tab.result.as_ref().filter(|_| tab.running.is_none()).map(|r| &r.exec);
    let resp = exec.and_then(|e| e.response.as_ref());
    let mut items = vec![
        Tab::new(RespSection::Body, tr("Body")),
        Tab::new(RespSection::Headers, tr("Headers")).count(resp.map(|r| r.headers.len()).unwrap_or(0)),
        Tab::new(RespSection::Cookies, tr("Cookies"))
            .count(resp.map(|r| r.headers_all("set-cookie").count()).unwrap_or(0)),
    ];
    let tests = Tab::new(RespSection::Tests, tr("Tests"));
    items.push(match exec {
        Some(e) if !e.tests.is_empty() || !e.script_errors.is_empty() => {
            tests.badge(format!("{}/{}", e.tests_passed(), e.tests.len()), tests_tone(e))
        }
        _ => tests,
    });
    items.push(Tab::new(RespSection::Details, tr("Details")));
    let comparable = tab.prev.is_some() && resp.is_some();
    if comparable {
        items.push(Tab::new(RespSection::Compare, tr("Compare")).icon(icons::GIT_DIFF));
    } else if tab.resp_section == RespSection::Compare && exec.is_some() {
        tab.resp_section = RespSection::Body;
    }
    if exec.is_some_and(|e| e.visualization.is_some()) {
        items.push(Tab::new(RespSection::Visualize, tr("Visualize")));
    } else if tab.resp_section == RespSection::Visualize && exec.is_some() {
        tab.resp_section = RespSection::Body;
    }
    pane_tabs_with(ui, &mut tab.resp_section, &items, |_| {});
}

fn response_content(ui: &mut Ui, tab: &mut RequestTab, ectx: &EditCtx, actions: &mut Actions) {
    if let Some(r) = &tab.running {
        let elapsed = r.started.elapsed();
        let _ = elapsed;
        center_v(ui, ui.make_persistent_id("resp-loading"), |ui| {
            let body = if ectx.host.is_empty() {
                String::new()
            } else {
                trf("Waiting for the first byte from {}", &[&ectx.host])
            };
            empty_block(ui, icons::CIRCLE_NOTCH, true, tr("Sending request…"), &body, |ui| {
                let r = kit::button(
                    ui,
                    Kind::Secondary,
                    Size::Sm,
                    tr("Cancel"),
                    ButtonOpts { icon: Some(icons::STOP), ..Default::default() },
                );
                if r.clicked() {
                    actions.cancel = true;
                }
            });
        });
        return;
    }
    let Some(result) = &mut tab.result else {
        empty_response(ui, ectx, actions);
        return;
    };
    let exec = &result.exec;
    match tab.resp_section {
        RespSection::Compare => {
            if let (Some(prev), Some(resp)) = (&tab.prev, &exec.response) {
                let cur = crate::response_diff::Snapshot { resp: resp.clone(), at: result.at };
                let acts = crate::response_diff::show(ui, &mut tab.compare, prev, &cur, tab.prev_pinned);
                if acts.toggle_pin {
                    tab.prev_pinned = !tab.prev_pinned;
                }
                if acts.maximize {
                    tab.panel = PanelMode::Maximized;
                }
            }
        }
        RespSection::Body | RespSection::Headers | RespSection::Cookies if exec.response.is_none() => {
            if let Some(err) = &exec.error {
                error_state(ui, err, actions);
            } else if exec.skipped {
                center_v(ui, ui.make_persistent_id("resp-skipped"), |ui| {
                    empty_block(
                        ui,
                        icons::ARROW_RIGHT,
                        false,
                        tr("Request skipped"),
                        tr("The request was skipped by pm.execution.skipRequest()."),
                        |_| {},
                    );
                });
            } else {
                no_response(ui);
            }
        }
        RespSection::Body => {
            let resp = exec.response.as_ref().expect("response");
            if resp.body.is_empty() {
                center_v(ui, ui.make_persistent_id("resp-no-body"), |ui| {
                    empty_block(
                        ui,
                        icons::FILE,
                        false,
                        tr("No body"),
                        &trf("The server answered {} {} without a body.", &[&resp.status, &resp.status_text]),
                        |_| {},
                    );
                });
            } else if let Some(a) = crate::body_view::show(ui, &mut result.body, resp, ectx.max_pretty) {
                actions.value_action = Some(a.into());
            }
        }
        RespSection::Headers => response_headers(ui, exec.response.as_ref().expect("response")),
        RespSection::Cookies => response_cookies(ui, exec.response.as_ref().expect("response"), actions),
        RespSection::Tests => {
            tests_view(ui, &mut tab.test_filter, exec, &mut tab.section, &mut tab.script_event, actions)
        }
        RespSection::Details => details_view(ui, exec, actions),
        RespSection::Visualize => {
            kit::pane_body(ui, "visualize", |ui| visualize_view(ui, &mut tab.viz, exec));
        }
    }
}

/// Centers `add` vertically in the space left (its height is measured on
/// the previous frame).
fn center_v<R>(ui: &mut Ui, id: egui::Id, add: impl FnOnce(&mut Ui) -> R) -> R {
    let h: f32 = ui.data(|d| d.get_temp(id).unwrap_or(0.0));
    let avail = ui.available_height();
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(((avail - h) / 2.0).max(16.0));
        let top = ui.cursor().top();
        let r = add(ui);
        let nh = ui.min_rect().bottom() - top;
        if (nh - h).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(id, nh));
            ui.ctx().request_repaint();
        }
        r
    })
    .inner
}

/// An empty state (`.empty`): an icon in a 52 px soft square, a title, a
/// line of explanation and actions, centered horizontally.
fn empty_block(ui: &mut Ui, icon: &str, spinning: bool, title: &str, body: &str, actions: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(14), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    if spinning {
        spinning_icon(ui, rect.center(), icon, 24.0, p.text_muted);
    } else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(24.0), p.text_muted);
    }
    ui.add_space(14.0);
    ui.label(RichText::new(title).font(theme::semibold(14.0)).color(p.text));
    if !body.is_empty() {
        ui.add_space(6.0);
        ui.scope(|ui| {
            ui.set_max_width(320.0);
            ui.add(egui::Label::new(RichText::new(body).size(12.5).color(p.text_muted)).halign(Align::Center).wrap());
        });
    }
    // Actions are centered by measuring them on the previous frame.
    let id = ui.make_persistent_id(("empty-actions", title));
    let w: f32 = ui.data(|d| d.get_temp(id).unwrap_or(0.0));
    ui.add_space(16.0);
    let r = ui
        .allocate_ui_with_layout(vec2(w.max(1.0), 30.0), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let start = ui.cursor().left();
            actions(ui);
            ui.min_rect().right() - start
        })
        .inner;
    if (r - w).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, r.max(0.0)));
        ui.ctx().request_repaint();
    }
}

/// Nothing sent yet: the shortcut and the last requests from History.
fn empty_response(ui: &mut Ui, ectx: &EditCtx, actions: &mut Actions) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    center_v(ui, ui.make_persistent_id("resp-empty"), |ui| {
        empty_block(
            ui,
            icons::PAPER_PLANE_RIGHT,
            false,
            tr("No response yet"),
            tr("Enter a URL and press Send to see the response here."),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                kit::kbd(ui, "⌘");
                kit::kbd(ui, "↩");
                ui.label(kit::hint(ui, tr("to send")));
            },
        );
        if ectx.recent.is_empty() {
            return;
        }
        // The empty state's 24 px padding, then 18.
        ui.add_space(42.0);
        let w = (ui.available_width() * 0.8).min(360.0);
        ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(w);
            ui.label(kit::eyebrow(ui, tr("Last sent from History")));
            ui.add_space(6.0);
            for r in &ectx.recent {
                let (row, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
                let resp = resp.on_hover_text(tr("Open in a new tab"));
                widgets::a11y_button(&resp, &format!("{} {}", r.method, r.path));
                ui.painter().rect_filled(row, CornerRadius::same(6), if resp.hovered() { p.hover } else { p.well });
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let cy = row.center().y;
                let mg = ui.painter().layout_no_wrap(
                    widgets::method_short(&r.method).to_string(),
                    theme::semibold(10.5),
                    theme::method_color(&r.method, dark),
                );
                ui.painter().galley(pos2(row.left() + 8.0, cy - mg.size().y / 2.0), mg.clone(), Color32::PLACEHOLDER);
                let mut right = row.right() - 8.0;
                let tg = ui.painter().layout_no_wrap(r.when.clone(), egui::FontId::proportional(11.0), p.text_faint);
                right -= tg.size().x;
                ui.painter().galley(pos2(right, cy - tg.size().y / 2.0), tg, p.text_faint);
                let (status, color) = match r.status {
                    Some(s) => (s.to_string(), theme::status_color(s, dark)),
                    None => (tr("Failed").to_string(), p.err),
                };
                let sg = ui.painter().layout_no_wrap(status, egui::FontId::proportional(11.0), color);
                right -= 8.0 + sg.size().x;
                ui.painter().galley(pos2(right, cy - sg.size().y / 2.0), sg, color);
                let x = row.left() + 8.0 + mg.size().x + 8.0;
                let g = elide(ui, &r.path, theme::mono(11.5), p.text, (right - 8.0 - x).max(20.0));
                ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, p.text);
                if resp.clicked() {
                    actions.open_history = Some(r.id.clone());
                }
                ui.add_space(4.0);
            }
        });
    });
}

/// Placeholder for response sections when no response arrived.
fn no_response(ui: &mut Ui) {
    center_v(ui, ui.make_persistent_id("resp-none"), |ui| {
        empty_block(
            ui,
            icons::WARNING_CIRCLE,
            false,
            tr("No response"),
            tr("The request failed before a response arrived; see Body for the error."),
            |_| {},
        );
    });
}

/// What to try next for a failed request.
fn error_hint(kind: ternarywire_http::error::HttpErrorKind) -> Option<&'static str> {
    use ternarywire_http::error::HttpErrorKind as K;
    Some(match kind {
        K::InvalidUrl => tr("Check the URL: variables must resolve to a full http(s) address."),
        K::InvalidHeader => tr("A header name or value contains characters that are not allowed."),
        K::Dns => tr("Check the host name and your network connection."),
        K::ConnectionRefused => tr("Check that the server is running and that the host and port are right."),
        K::ConnectionReset => tr("The server closed the connection. Try again, or check the server logs."),
        K::Timeout => tr("The server did not answer in time. Try again or raise the timeout in Settings."),
        K::Certificate => tr("For self-signed certificates, turn off SSL verification in the request Settings."),
        K::Tls => tr("Check that the server speaks HTTPS on this port."),
        K::Proxy => tr("Check the proxy settings."),
        K::TooManyRedirects => tr("Raise the maximum redirects in Settings, or turn off following redirects."),
        K::File => tr("Check that the body files still exist and can be read."),
        _ => return None,
    })
}

/// The checklist of a failed request: the main hint, then the usual
/// suspects for its kind of error.
fn error_checklist(kind: ternarywire_http::error::HttpErrorKind) -> Vec<&'static str> {
    use ternarywire_http::error::HttpErrorKind as K;
    let mut out: Vec<&'static str> = error_hint(kind).into_iter().collect();
    let proxy = tr("Using a proxy? Check Settings › Proxy.");
    let env = tr("Switch environment with ⌘E if this should go to another server.");
    match kind {
        K::Dns | K::ConnectionRefused | K::Timeout => out.extend([proxy, env]),
        K::ConnectionReset | K::Tls => out.push(proxy),
        K::InvalidUrl => out.extend([tr("Undefined variables are sent as typed; Details lists them."), env]),
        _ => {}
    }
    out
}

/// The icon and short category of a failed request.
fn error_kind_look(kind: ternarywire_http::error::HttpErrorKind) -> (&'static str, &'static str) {
    use ternarywire_http::error::HttpErrorKind as K;
    match kind {
        K::Dns | K::ConnectionRefused | K::ConnectionReset | K::Proxy => (icons::PLUG, tr("Connection")),
        K::Timeout => (icons::HOURGLASS, tr("Connection")),
        K::Certificate | K::Tls => (icons::LOCK_SIMPLE, "TLS"),
        K::InvalidUrl | K::InvalidHeader => (icons::LINK, tr("Request")),
        K::TooManyRedirects => (icons::ARROW_BEND_UP_LEFT, tr("Redirects")),
        K::File => (icons::FILE, tr("File")),
        _ => (icons::WARNING_CIRCLE, tr("Error")),
    }
}

/// The request could not be sent: a card with the cause, a checklist and
/// the next steps.
fn error_state(ui: &mut Ui, err: &ternarywire_http::error::HttpError, actions: &mut Actions) {
    let p = theme::pal(ui);
    let detail_id = ui.make_persistent_id("err-detail");
    let mut show_detail: bool = ui.data(|d| d.get_temp(detail_id).unwrap_or(false));
    let has_detail = err.detail != err.message;
    egui::ScrollArea::vertical().id_salt("err-scroll").auto_shrink([false; 2]).show(ui, |ui| {
        center_v(ui, ui.make_persistent_id("err-center"), |ui| {
            let w = (ui.available_width() - 40.0).min(440.0);
            ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                kit::card(ui).inner_margin(egui::Margin::same(18)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (icon, category) = error_kind_look(err.kind);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let (rect, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 20.0, p.err_soft);
                        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(20.0), p.err);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(tr("Could not send request"))
                                        .font(theme::semibold(14.0))
                                        .color(p.text),
                                );
                                kit::tone_pill(ui, category, Tone::Err);
                            });
                            ui.add(egui::Label::new(RichText::new(&err.message).size(13.0).color(p.text_muted)).wrap());
                        });
                    });
                    let lines = error_checklist(err.kind);
                    if !lines.is_empty() {
                        ui.add_space(14.0);
                        for (i, line) in lines.iter().enumerate() {
                            if i > 0 {
                                ui.add_space(6.0);
                            }
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(RichText::new(icons::CHECK).font(theme::icon_font(13.0)).color(p.text_faint));
                                ui.add(egui::Label::new(RichText::new(*line).size(12.5).color(p.text)).wrap());
                            });
                        }
                    }
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let opts = ButtonOpts { icon: Some(icons::ARROW_CLOCKWISE), ..Default::default() };
                        if kit::button(ui, Kind::Primary, Size::Sm, tr("Try again"), opts).clicked() {
                            actions.send = true;
                        }
                        if has_detail {
                            let (label, icon) = if show_detail {
                                (tr("Hide details"), icons::CARET_UP)
                            } else {
                                (tr("Show details"), icons::CARET_DOWN)
                            };
                            if kit::ghost(ui, Some(icon), label).clicked() {
                                show_detail = !show_detail;
                                ui.data_mut(|d| d.insert_temp(detail_id, show_detail));
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if kit::ghost(ui, Some(icons::TERMINAL_WINDOW), tr("Open console")).clicked() {
                                actions.open_console = true;
                            }
                        });
                    });
                    if show_detail && has_detail {
                        ui.add_space(12.0);
                        kit::well(ui).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&err.detail).font(theme::mono(11.5)).color(p.text_muted),
                                )
                                .selectable(true)
                                .wrap(),
                            );
                        });
                    }
                });
            });
        });
    });
}

/// A toolbar with a muted count on the left and actions on the right
/// (added right to left).
fn count_toolbar(ui: &mut Ui, count: &str, right: impl FnOnce(&mut Ui)) {
    kit::toolbar(ui, |ui| {
        ui.label(kit::hint(ui, count));
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
}

/// Response headers: the count and Copy all, then a read-only table.
fn response_headers(ui: &mut Ui, resp: &ternarywire_core::response::HttpResponse) {
    if resp.headers.is_empty() {
        center_v(ui, ui.make_persistent_id("no-headers"), |ui| {
            empty_block(ui, icons::LIST, false, tr("No headers"), tr("The response has no headers."), |_| {});
        });
        return;
    }
    let count =
        if resp.headers.len() == 1 { tr("1 header").to_string() } else { trf("{} headers", &[&resp.headers.len()]) };
    count_toolbar(ui, &count, |ui| {
        if kit::ghost(ui, Some(icons::COPY), tr("Copy all")).clicked() {
            ui.ctx().copy_text(resp.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n"));
        }
    });
    let rows: Vec<Vec<String>> = resp.headers.iter().map(|(k, v)| vec![k.clone(), v.clone()]).collect();
    kit::pane_body(ui, "resp-headers", |ui| {
        read_table(ui, "resp-headers", &[tr("Name"), tr("Value")], &[0.38, 0.62], &rows);
    });
}

/// `Set-Cookie` headers split into name, value and attributes.
fn parse_set_cookie(c: &str) -> Vec<String> {
    let first = c.split(';').next().unwrap_or("");
    let (n, v) = first.split_once('=').unwrap_or((first, ""));
    let attrs = c.split(';').skip(1).map(str::trim).filter(|a| !a.is_empty()).collect::<Vec<_>>().join(" · ");
    vec![n.trim().to_string(), v.trim().to_string(), attrs]
}

fn response_cookies(ui: &mut Ui, resp: &ternarywire_core::response::HttpResponse, actions: &mut Actions) {
    let rows: Vec<Vec<String>> = resp.headers_all("set-cookie").map(parse_set_cookie).collect();
    if rows.is_empty() {
        center_v(ui, ui.make_persistent_id("no-cookies"), |ui| {
            empty_block(
                ui,
                icons::COOKIE,
                false,
                tr("No cookies"),
                tr("No cookies were set by this response."),
                |ui| {
                    let opts = ButtonOpts { icon: Some(icons::COOKIE), ..Default::default() };
                    if kit::button(ui, Kind::Ghost, Size::Sm, tr("Open cookie jar"), opts).clicked() {
                        actions.open_cookies = true;
                    }
                },
            );
        });
        return;
    }
    let count = if rows.len() == 1 {
        tr("1 cookie set by this response").to_string()
    } else {
        trf("{} cookies set by this response", &[&rows.len()])
    };
    count_toolbar(ui, &count, |ui| {
        if kit::ghost(ui, Some(icons::COOKIE), tr("Open cookie jar")).clicked() {
            actions.open_cookies = true;
        }
        if kit::ghost(ui, Some(icons::COPY), tr("Copy all")).clicked() {
            ui.ctx().copy_text(resp.headers_all("set-cookie").collect::<Vec<_>>().join("\n"));
        }
    });
    kit::pane_body(ui, "resp-cookies", |ui| {
        read_table(ui, "resp-cookies", &[tr("Name"), tr("Value"), tr("Attributes")], &[0.26, 0.34, 0.40], &rows);
    });
}

/// `pm.visualizer.set` output rendered in the app (FR-P4); pages that need
/// scripts (charts) can still be opened in the browser.
fn visualize_view(
    ui: &mut Ui,
    cache: &mut Option<(u64, Result<ternarywire_html::Document, String>)>,
    exec: &Execution,
) {
    use std::hash::{Hash, Hasher};
    let Some(v) = &exec.visualization else { return };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.template.hash(&mut h);
    v.data.to_string().hash(&mut h);
    let key = h.finish();
    if cache.as_ref().map(|(k, _)| *k) != Some(key) {
        *cache = Some((key, ternarywire_html::parse(&v.template, &v.data).map_err(|e| e.to_string())));
    }
    let Some((_, doc)) = cache.as_mut() else { return };
    match doc {
        Ok(doc) => {
            // `@media` width queries follow the panel width.
            let width = ternarywire_html::Options::default().css_width(ui);
            if doc.restyle_needed(width) {
                *doc = doc.restyled(width);
            }
            if doc.needs_browser() {
                let mut open = false;
                kit::banner_with(
                    ui,
                    Tone::Info,
                    icons::INFO,
                    tr(
                        "This visualization uses scripts or external resources; open it in the browser for the full page.",
                    ),
                    |ui| open = kit::ghost(ui, Some(icons::ARROW_SQUARE_OUT), tr("Open in browser")).clicked(),
                );
                if open {
                    open_visualization(exec);
                }
            }
            doc.show(ui, &mut |href: &str| {
                widgets::open_web_link(href);
            });
        }
        Err(e) => {
            kit::banner(ui, Tone::Err, icons::WARNING_CIRCLE, &format!("{}: {e}", tr("Visualizer template")));
        }
    }
}

fn event_label(e: ScriptEvent) -> &'static str {
    match e {
        ScriptEvent::PreRequest => tr("Pre-request"),
        ScriptEvent::PostResponse => tr("Post-response"),
        ScriptEvent::OnMessage => tr("On message"),
    }
}

/// Pass/fail/skip counts of a list of tests.
pub(crate) fn test_counts(tests: &[TestResult]) -> (usize, usize, usize) {
    let skipped = tests.iter().filter(|t| t.skipped).count();
    let passed = tests.iter().filter(|t| t.passed && !t.skipped).count();
    (passed, tests.len() - passed - skipped, skipped)
}

/// Pills with the pass/fail/skip counts (zero counts are neutral).
pub(crate) fn test_count_pills(ui: &mut Ui, passed: usize, failed: usize, skipped: usize) {
    let pill = |ui: &mut Ui, n: usize, text: String, tone: Tone, icon: &str| {
        let (tone, icon) = if n == 0 { (Tone::Neutral, None) } else { (tone, Some(icon)) };
        kit::pill_ex(ui, &text, tone, PillOpts { icon, ..Default::default() });
    };
    pill(ui, passed, trf("{} passed", &[&passed]), Tone::Ok, icons::CHECK);
    pill(ui, failed, trf("{} failed", &[&failed]), Tone::Err, icons::X);
    pill(ui, skipped, trf("{} skipped", &[&skipped]), Tone::Neutral, icons::MINUS);
}

/// A thin bar split into passed, failed and skipped parts (6 high).
#[allow(dead_code)] // Shared with the other views.
pub(crate) fn test_ratio_bar(ui: &mut Ui, passed: usize, failed: usize, skipped: usize, width: f32) {
    ratio_bar(ui, passed, failed, skipped, width, 6.0);
}

fn ratio_bar(ui: &mut Ui, passed: usize, failed: usize, skipped: usize, width: f32, height: f32) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let total = (passed + failed + skipped).max(1) as f32;
    let r = CornerRadius::same((height / 2.0) as u8);
    ui.painter().rect_filled(rect, r, p.subtle);
    let mut x = rect.left();
    let painter = ui.painter_at(rect);
    for (n, c) in [(passed, p.ok), (failed, p.err), (skipped, p.text_faint)] {
        if n == 0 {
            continue;
        }
        let w = rect.width() * n as f32 / total;
        painter.rect_filled(Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height())), r, c);
        x += w;
    }
}

fn tests_view(
    ui: &mut Ui,
    filter: &mut TestFilter,
    exec: &Execution,
    section: &mut Section,
    script_event: &mut ScriptEvent,
    actions: &mut Actions,
) {
    if exec.tests.is_empty() && exec.script_errors.is_empty() && exec.response.is_none() {
        center_v(ui, ui.make_persistent_id("tests-none"), |ui| {
            empty_block(
                ui,
                icons::TEST_TUBE,
                false,
                tr("Tests did not run"),
                tr("Post-response tests run only when a response arrives."),
                |_| {},
            );
        });
        return;
    }
    if exec.tests.is_empty() && exec.script_errors.is_empty() {
        center_v(ui, ui.make_persistent_id("tests-empty"), |ui| {
            empty_block(
                ui,
                icons::TEST_TUBE,
                false,
                tr("No tests yet"),
                tr("Add pm.test(...) calls in the Post-response script to check the response."),
                |ui| {
                    let r = kit::button(
                        ui,
                        Kind::Secondary,
                        Size::Sm,
                        tr("Generate tests"),
                        ButtonOpts { icon: Some(icons::TEST_TUBE), ..Default::default() },
                    );
                    if r.on_hover_text(tr(
                        "Write tests for the status, time, content type and body shape of the response",
                    ))
                    .clicked()
                    {
                        actions.generate_tests = true;
                    }
                    let r = kit::button(
                        ui,
                        Kind::Ghost,
                        Size::Sm,
                        tr("Write a test"),
                        ButtonOpts { icon: Some(icons::PLUS), ..Default::default() },
                    );
                    if r.clicked() {
                        *section = Section::Scripts;
                        *script_event = ScriptEvent::PostResponse;
                    }
                },
            );
        });
        return;
    }
    let (passed, failed, skipped) = test_counts(&exec.tests);
    kit::toolbar(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        test_count_pills(ui, passed, failed, skipped);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::ghost(ui, Some(icons::ARROW_CLOCKWISE), tr("Re-run tests"))
                .on_hover_text(tr("Run post-response scripts again on this response"))
                .clicked()
            {
                actions.rerun_tests = true;
            }
        });
    });
    kit::pane_body(ui, "tests", |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        ratio_bar(ui, passed, failed, skipped, ui.available_width(), 4.0);
        if !exec.tests.is_empty() {
            let all = format!("{} {}", tr("All"), exec.tests.len());
            let pass = format!("{} {passed}", tr("Passed"));
            let fail = format!("{} {failed}", tr("Failed"));
            let skip = format!("{} {skipped}", tr("Skipped"));
            hrow(ui, 26.0, |ui| {
                kit::segmented_sm(
                    ui,
                    filter,
                    &[
                        (TestFilter::All, all.as_str()),
                        (TestFilter::Passed, pass.as_str()),
                        (TestFilter::Failed, fail.as_str()),
                        (TestFilter::Skipped, skip.as_str()),
                    ],
                );
            });
        }
        for e in &exec.script_errors {
            kit::banner(
                ui,
                Tone::Err,
                icons::WARNING_CIRCLE,
                &format!("{}\n{}", trf("Script error in {} ({})", &[&event_label(e.event), &e.location]), e.message),
            );
        }
        let shown: Vec<&TestResult> = exec
            .tests
            .iter()
            .filter(|t| match filter {
                TestFilter::All => true,
                TestFilter::Passed => t.passed && !t.skipped,
                TestFilter::Failed => !t.passed && !t.skipped,
                TestFilter::Skipped => t.skipped,
            })
            .collect();
        if shown.is_empty() && !exec.tests.is_empty() {
            ui.label(kit::hint(ui, tr("No tests match this filter.")));
            return;
        }
        if test_list_ex(ui, &shown, true) {
            *section = Section::Scripts;
            *script_event = ScriptEvent::PostResponse;
        }
    });
}

/// [`test_list`], with (`add_row`) a last row saying how to add tests.
/// Returns true when that row was clicked.
fn test_list_ex(ui: &mut Ui, tests: &[&TestResult], add_row: bool) -> bool {
    if tests.is_empty() && !add_row {
        return false;
    }
    let p = theme::pal(ui);
    let top = ui.cursor().min;
    let width = ui.available_width();
    let mut clicked = false;
    let bg = ui.painter().add(egui::Shape::Noop);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, t) in tests.iter().enumerate() {
            if i > 0 {
                let y = ui.cursor().min.y;
                ui.painter().hline(top.x..=top.x + width, y, Stroke::new(1.0, p.border));
            }
            test_row(ui, t, width);
        }
        if add_row {
            if !tests.is_empty() {
                let y = ui.cursor().min.y;
                ui.painter().hline(top.x..=top.x + width, y, Stroke::new(1.0, p.border));
            }
            let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
            widgets::a11y_button(
                &resp,
                tr("Add tests in Scripts › Post-response, or right-click a value in the body."),
            );
            if resp.hovered() {
                ui.painter().rect_filled(rect.shrink(1.0), CornerRadius::same(8), p.well);
            }
            // Icon 13, 8 before the hint (12): the prototype's `.row`.
            ui.painter().text(
                pos2(rect.left() + 12.0 + 6.5, rect.center().y),
                Align2::CENTER_CENTER,
                icons::PLUS,
                theme::icon_font(13.0),
                p.text_faint,
            );
            let g = elide(
                ui,
                tr("Add tests in Scripts › Post-response, or right-click a value in the body."),
                egui::FontId::proportional(12.0),
                p.text_muted,
                width - 33.0 - 12.0,
            );
            ui.painter().galley(pos2(rect.left() + 33.0, rect.center().y - g.size().y / 2.0), g, p.text_muted);
            clicked = resp.clicked();
        }
    });
    let rect = Rect::from_min_max(top, pos2(top.x + width, ui.min_rect().bottom()));
    ui.painter().set(bg, egui::Shape::rect_filled(rect, CornerRadius::same(9), p.surface));
    ui.painter().rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, p.border), StrokeKind::Inside);
    clicked
}

/// One test: status icon, name, PASS/FAIL/SKIP pill, failure message under it.
fn test_row(ui: &mut Ui, t: &TestResult, width: f32) {
    let p = theme::pal(ui);
    let (icon, tone, label) = if t.skipped {
        (icons::MINUS, Tone::Neutral, tr("SKIP"))
    } else if t.passed {
        (icons::CHECK_CIRCLE, Tone::Ok, tr("PASS"))
    } else {
        (icons::X_CIRCLE, Tone::Err, tr("FAIL"))
    };
    let (color, _) = p.tone(tone);
    // Icon 13, 8 before the name (13), as the prototype's test row.
    let text_x = 12.0 + 13.0 + 8.0;
    let text_w = (width - text_x - 70.0).max(60.0);
    let name = ui.painter().layout(t.name.clone(), egui::FontId::proportional(13.0), p.text, text_w);
    let err = t
        .error
        .as_ref()
        .filter(|e| !e.is_empty())
        .map(|e| ui.painter().layout(e.clone(), theme::mono(11.5), p.text_muted, text_w));
    let pad = 11.0;
    let h = (pad + name.size().y + err.as_ref().map_or(0.0, |g| g.size().y + 3.0) + pad).max(38.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect.shrink(1.0), CornerRadius::same(8), p.well);
    }
    let first_line = if err.is_none() { rect.center().y } else { rect.top() + pad + name.size().y / 2.0 };
    let name_top = if err.is_none() { rect.center().y - name.size().y / 2.0 } else { rect.top() + pad };
    ui.painter().text(
        pos2(rect.left() + 12.0 + 6.5, first_line),
        Align2::CENTER_CENTER,
        icon,
        theme::icon_font(13.0),
        color,
    );
    ui.painter().galley(pos2(rect.left() + text_x, name_top), name.clone(), p.text);
    if let Some(g) = err {
        ui.painter().galley(pos2(rect.left() + text_x, name_top + name.size().y + 3.0), g, p.text_muted);
    }
    // The pill at the right edge, on the first line.
    let pill_w = ui.painter().layout_no_wrap(label.to_string(), theme::medium(11.5), color).size().x + 14.0;
    let pill_rect = Rect::from_min_size(pos2(rect.right() - 12.0 - pill_w, first_line - 10.0), vec2(pill_w, 20.0));
    kit::tone_pill(&mut ui.new_child(egui::UiBuilder::new().max_rect(pill_rect)), label, tone);
    widgets::a11y_button(&resp, &format!("{label} {}", t.name));
    resp.context_menu(|ui| {
        if kit::menu_item(ui, Some(icons::COPY), tr("Copy test name"), None).clicked() {
            ui.ctx().copy_text(t.name.clone());
        }
        if let Some(e) = &t.error
            && kit::menu_item(ui, Some(icons::COPY), tr("Copy message"), None).clicked()
        {
            ui.ctx().copy_text(e.clone());
        }
    });
}

/// Milliseconds with a precision that fits their size.
pub(crate) fn fmt_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{ms:.2} ms")
    } else if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        ternarywire_core::util::human_ms(ms)
    }
}

/// A label/value line of a details card (min 28 high, label 120 wide).
fn info_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.allocate_ui_with_layout(vec2(120.0, 28.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_width(120.0);
            ui.label(RichText::new(label).size(13.0).color(p.text_muted));
        });
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), add);
    });
}

/// A card with a title row (section title, then `right`) and content.
fn titled_card(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui), add: impl FnOnce(&mut Ui)) {
    kit::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        hrow(ui, 20.0, |ui| {
            ui.label(kit::section_title(title));
            ui.with_layout(Layout::right_to_left(Align::Center), right);
        });
        add(ui);
    });
}

fn details_view(ui: &mut Ui, exec: &Execution, actions: &mut Actions) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    kit::pane_body(ui, "details", |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.add_space(4.0);
        if !exec.unresolved.is_empty() {
            let names = exec.unresolved.iter().map(|n| format!("{{{{{n}}}}}")).collect::<Vec<_>>().join(", ");
            let text = if exec.unresolved.len() == 1 {
                trf("Undefined variable {} was sent as typed. Define it in Local or the collection.", &[&names])
            } else {
                trf("Undefined variables {} were sent as typed. Define them in Local or the collection.", &[&names])
            };
            kit::banner(ui, Tone::Warn, icons::WARNING, &text);
        }
        if let Some(resp) = &exec.response {
            let total = resp.timings.total_ms.max(resp.timings.ttfb_ms);
            titled_card(
                ui,
                tr("Timing"),
                |ui| {
                    ui.label(RichText::new(fmt_ms(total)).font(theme::semibold(12.5)).color(p.text));
                },
                |ui| {
                    ui.add_space(12.0);
                    waterfall(
                        ui,
                        resp.timings.ttfb_ms,
                        resp.timings.total_ms,
                        130.0,
                        70.0,
                        12.0,
                        &[
                            (
                                tr("Waiting (TTFB)"),
                                tr(
                                    "From sending the request to the first response byte: connecting, TLS and the server's work.",
                                ),
                            ),
                            (tr("Content download"), tr("Receiving the response body.")),
                        ],
                    );
                },
            );
            titled_card(
                ui,
                tr("Response"),
                |_| {},
                |ui| {
                    ui.add_space(8.0);
                    info_row(ui, tr("Status"), |ui| {
                        kit::status_pill(ui, resp.status, &resp.status_text);
                    });
                    info_row(ui, tr("HTTP version"), |ui| {
                        ui.label(RichText::new(&resp.version).size(13.0));
                    });
                    if let Some(addr) = &resp.remote_addr {
                        info_row(ui, tr("Remote address"), |ui| {
                            ui.add(egui::Label::new(RichText::new(addr).font(theme::mono(12.5))).selectable(true));
                        });
                    }
                    info_row(ui, tr("Security"), |ui| {
                        let (icon, text, color) = if resp.url.starts_with("https://") {
                            (icons::LOCK_SIMPLE, tr("Encrypted with TLS (HTTPS)"), p.ok)
                        } else {
                            (icons::LOCK_SIMPLE, tr("Not encrypted (plain HTTP)"), p.text_faint)
                        };
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new(icon).font(theme::icon_font(13.0)).color(color));
                        ui.label(RichText::new(text).size(13.0));
                    });
                    info_row(ui, tr("Size"), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                ternarywire_core::util::human_bytes(resp.total_size()),
                                trf(
                                    "headers {} · body {}",
                                    &[
                                        &ternarywire_core::util::human_bytes(resp.header_size),
                                        &ternarywire_core::util::human_bytes(resp.body.len()),
                                    ],
                                )
                            ))
                            .size(13.0),
                        );
                    });
                    info_row(ui, tr("URL"), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&resp.url).font(theme::mono(12.5)))
                                .selectable(true)
                                .truncate(),
                        );
                    });
                },
            );
            if !resp.redirects.is_empty() {
                titled_card(
                    ui,
                    tr("Redirects"),
                    |_| {},
                    |ui| {
                        ui.add_space(8.0);
                        for (i, r) in resp.redirects.iter().chain(std::iter::once(&resp.url)).enumerate() {
                            ui.horizontal(|ui| {
                                ui.set_min_height(26.0);
                                let last = i == resp.redirects.len();
                                let n =
                                    RichText::new(format!("{}", i + 1)).font(theme::medium(11.5)).color(p.text_muted);
                                ui.add_sized([18.0, 20.0], egui::Label::new(n));
                                ui.add(
                                    egui::Label::new(RichText::new(r).font(theme::mono(12.0)))
                                        .selectable(true)
                                        .truncate(),
                                );
                                if last {
                                    kit::status_pill(ui, resp.status, "");
                                }
                            });
                        }
                    },
                );
            }
        }
        if let Some(sent) = &exec.sent {
            kit::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                hrow(ui, 24.0, |ui| {
                    ui.label(widgets::method_badge(&sent.method, dark));
                    ui.label(kit::section_title(tr("Request sent")));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if exec.prepared.is_some()
                            && kit::ghost(ui, Some(icons::TERMINAL), tr("Copy as cURL")).clicked()
                        {
                            actions.copy_curl = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(RichText::new(&sent.url).font(theme::mono(12.0)).color(p.text_muted))
                        .selectable(true)
                        .wrap(),
                );
                if let Some(b) = sent.body_size {
                    ui.add_space(2.0);
                    ui.label(kit::hint(ui, &trf("Body: {}", &[&ternarywire_core::util::human_bytes(b)])));
                }
                ui.add_space(8.0);
                let rows: Vec<Vec<String>> = sent.headers.iter().map(|(k, v)| vec![k.clone(), v.clone()]).collect();
                read_table(ui, "sent-headers", &[tr("Header"), tr("Value")], &[0.38, 0.62], &rows);
            });
        }
        if !exec.auth_notes.is_empty() {
            titled_card(
                ui,
                tr("Authorization"),
                |_| {},
                |ui| {
                    for n in &exec.auth_notes {
                        ui.add_space(8.0);
                        kit::banner(ui, Tone::Info, icons::INFO, n);
                    }
                },
            );
        }
        if !exec.console.is_empty() {
            titled_card(
                ui,
                tr("Console"),
                |_| {},
                |ui| {
                    ui.add_space(8.0);
                    kit::well(ui).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for l in &exec.console {
                            let color = match l.kind {
                                ternarywire_runner::ConsoleKind::Warn => p.warn,
                                ternarywire_runner::ConsoleKind::Error => p.err,
                                _ => p.text,
                            };
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new(&l.phase).font(theme::mono(11.0)).color(p.text_faint));
                                ui.add(
                                    egui::Label::new(RichText::new(&l.message).font(theme::mono(11.5)).color(color))
                                        .selectable(true)
                                        .wrap(),
                                );
                            });
                        }
                    });
                },
            );
        }
    });
}

fn open_visualization(exec: &Execution) {
    let Some(v) = &exec.visualization else { return };
    let Ok(html) = ternarywire_html::browser_html(&v.template, &v.data) else { return };
    // The template's scripts run from this file: nobody else may change it.
    if let Ok(path) = crate::widgets::private_temp_file("ternarywire-visualize-", ".html")
        && std::fs::write(&path, html).is_ok()
    {
        let _ = open::that(&path);
    }
}

/// The response pane of a running GraphQL subscription.
fn subscription_panel(ui: &mut Ui, sub: &GqlSubscription, panel: &mut PanelMode, side: bool, actions: &mut Actions) {
    let p = theme::pal(ui);
    kit::bar_ex(ui, size::ACTION_ROW, 10.0, true, |ui| {
        ui.add_space(4.0);
        let tone = if sub.status.starts_with("Error") {
            Tone::Err
        } else if sub.done {
            Tone::Neutral
        } else {
            Tone::Ok
        };
        kit::pill_ex(ui, &sub.status, tone, PillOpts { large: true, dot: true, live: !sub.done, ..Default::default() });
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            kit::metric(ui, icons::CHATS, &trf("{} events", &[&sub.messages.len()]));
            kit::metric(
                ui,
                icons::TIMER,
                &ternarywire_core::util::human_ms(sub.started.elapsed().as_secs_f64() * 1000.0),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let icon = if side { icons::CARET_RIGHT } else { icons::CARET_DOWN };
            if kit::icon_button_sm(ui, icon, tr("Minimize response")).clicked() {
                *panel = PanelMode::Minimized;
            }
            let maximized = *panel == PanelMode::Maximized;
            let (icon, tip) = if maximized {
                (icons::ARROWS_IN, tr("Restore layout"))
            } else {
                (icons::ARROWS_OUT, tr("Maximize response"))
            };
            if kit::icon_button_ex(ui, icon, tip, maximized, 22.0).clicked() {
                *panel = if maximized { PanelMode::Normal } else { PanelMode::Maximized };
            }
            ui.add_space(-4.0);
            kit::vdivider(ui, 18.0);
            ui.add_space(-4.0);
            let (icon, label) = if sub.done { (icons::X, tr("Close")) } else { (icons::STOP, tr("Stop")) };
            if kit::button(ui, Kind::Secondary, Size::Sm, label, ButtonOpts { icon: Some(icon), ..Default::default() })
                .clicked()
            {
                actions.cancel = true;
            }
        });
    });
    kit::bar(ui, size::TABS, true, |ui| {
        ui.label(kit::section_title(tr("Subscription")));
    });
    if sub.messages.is_empty() {
        center_v(ui, ui.make_persistent_id("sub-empty"), |ui| {
            empty_block(
                ui,
                icons::BROADCAST,
                false,
                tr("Waiting for events"),
                tr("Events the server sends appear here."),
                |_| {},
            );
        });
        return;
    }
    egui::ScrollArea::vertical().id_salt("gql-sub").stick_to_bottom(true).auto_shrink([false; 2]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 12, bottom: 16 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            for (at, text) in &sub.messages {
                let t = chrono::DateTime::from_timestamp_millis(*at)
                    .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string())
                    .unwrap_or_default();
                kit::card(ui).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("{} {t}", icons::ARROW_DOWN)).size(11.5).color(p.text_muted));
                    ui.add(egui::Label::new(RichText::new(text).font(theme::mono(12.0))).selectable(true));
                });
            }
        });
    });
}

/// The minimized response beside the request: a narrow rail with the
/// buttons to bring it back and the outcome.
fn minimized_rail(ui: &mut Ui, tab: &mut RequestTab) {
    let p = theme::pal(ui);
    let rect = ui.max_rect();
    let bar = ui.interact(rect, ui.make_persistent_id("resp-min-rail"), Sense::click());
    if bar.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let bar = bar.on_hover_text(tr("Show the response"));
    widgets::a11y_button(&bar, tr("Show the response"));
    let mut restore = bar.clicked();
    let resp = tab.result.as_ref().and_then(|r| r.exec.response.as_ref());
    let failed = tab.result.as_ref().is_some_and(|r| r.exec.error.is_some());
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.add_space(14.0);
        if kit::icon_button_sm(ui, icons::CARET_LEFT, tr("Restore layout")).clicked() {
            restore = true;
        }
        if kit::icon_button_sm(ui, icons::ARROWS_OUT, tr("Maximize response")).clicked() {
            tab.panel = PanelMode::Maximized;
        }
        ui.add_space(4.0);
        if tab.running.is_some() {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            spinning_icon(ui, r.center(), icons::CIRCLE_NOTCH, 14.0, p.info);
        } else if let Some(resp) = resp {
            let (fg, _) = p.tone(theme::status_tone(resp.status));
            ui.label(RichText::new(resp.status.to_string()).font(theme::semibold(11.5)).color(fg))
                .on_hover_text(format!("{} {}", resp.status, resp.status_text));
        } else if failed {
            ui.label(RichText::new(icons::WARNING_CIRCLE).font(theme::icon_font(15.0)).color(p.err));
        }
    });
    if restore {
        tab.panel = PanelMode::Normal;
    }
}
/// Starts a GraphQL subscription over WebSocket (graphql-transport-ws; the
/// legacy graphql-ws protocol is accepted too).
fn start_subscription(
    app: &mut TernaryWireApp,
    tab_id: u64,
    tab: &mut RequestTab,
    item: &RequestItem,
    collection_id: Option<&str>,
) {
    let Some(http) = item.http() else { return };
    let scopes = app.ws.scopes_for(collection_id);
    let vault = app.ws.vault();
    let resolver = Resolver::new(&scopes).with_vault(vault.as_ref());
    let auth = if http.auth.is_inherit() {
        collection_id
            .and_then(|c| app.ws.data.collection(c))
            .and_then(|c| c.effective_auth(&item.id).map(|e| e.auth.clone()))
            .unwrap_or_else(Auth::none)
    } else {
        http.auth.clone()
    };
    let ctx = ternarywire_core::prepared::PrepareContext {
        resolver,
        auth: &auth,
        auth_owner_id: Some(&item.id),
        defaults: &app.settings.request,
        base_dir: None,
    };
    let mut prepared = ternarywire_core::prepared::prepare(http, &ctx);
    if let Some(p) = ternarywire_auth::preview(&prepared) {
        prepared = p.request;
    }
    let mut url = prepared.url.clone();
    if let Some(rest) = url.strip_prefix("http://") {
        url = format!("ws://{rest}");
    } else if let Some(rest) = url.strip_prefix("https://") {
        url = format!("wss://{rest}");
    }
    let gql = &http.body.graphql;
    let variables: serde_json::Value =
        serde_json::from_str(&resolver.resolve(&gql.variables)).unwrap_or(serde_json::json!({}));
    let mut payload = serde_json::json!({"query": resolver.resolve(&gql.query), "variables": variables});
    if let Some(op) = gql.operation_name.as_ref().filter(|o| !o.is_empty()) {
        payload["operationName"] = serde_json::Value::String(op.clone());
    }
    let subscribe = serde_json::json!({"id": "1", "type": "subscribe", "payload": payload}).to_string();
    let headers = prepared
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("accept"))
        .cloned()
        .collect();
    let tls = match app.tls_setup(&url, prepared.settings.verify_ssl) {
        Ok(t) => t,
        Err(e) => {
            app.toast(e, true);
            return;
        }
    };
    let cfg = ternarywire_http::ws::WsConfig {
        url,
        headers,
        subprotocols: vec!["graphql-transport-ws".into(), "graphql-ws".into()],
        verify_ssl: prepared.settings.verify_ssl,
        tls,
        proxy: app.proxy_settings(),
        ..Default::default()
    };
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    tab.ws_epoch = crate::app::next_epoch();
    let epoch = tab.ws_epoch;
    let session = ternarywire_http::ws::WsSession::connect(app.rt.handle(), cfg, move |event| {
        let _ = tx.send(crate::app::Msg::Ws { tab: tab_id, epoch, event });
        ctx.request_repaint();
    });
    tab.result = None;
    tab.gql_sub = Some(GqlSubscription {
        session,
        subscribe,
        status: "Connecting…".into(),
        messages: Vec::new(),
        started: Instant::now(),
        done: false,
    });
}

fn fetch_schema(
    app: &mut TernaryWireApp,
    tab_id: u64,
    tab: &mut RequestTab,
    item: &RequestItem,
    collection_id: Option<&str>,
) {
    let Some(http) = item.http() else { return };
    let mut req = http.clone();
    req.body = Body::default();
    req.body.mode = BodyMode::GraphQl;
    req.body.graphql.query = ternarywire_convert::graphql::INTROSPECTION_QUERY.to_string();
    req.method = "POST".into();
    let scopes = app.ws.scopes_for(collection_id);
    let auth = if req.auth.is_inherit() {
        collection_id
            .and_then(|c| app.ws.data.collection(c))
            .and_then(|c| c.effective_auth(&item.id).map(|e| e.auth.clone()))
            .unwrap_or_else(Auth::none)
    } else {
        req.auth.clone()
    };
    let services = app.services();
    tab.graphql.loading = true;
    tab.graphql.error = None;
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    let owner = item.id.clone();
    app.rt.spawn(async move {
        let resolver = Resolver::new(&scopes).with_vault(services.vault.as_deref().unwrap());
        let pctx = ternarywire_core::prepared::PrepareContext {
            resolver,
            auth: &auth,
            auth_owner_id: Some(&owner),
            defaults: &services.defaults,
            base_dir: None,
        };
        let mut prepared = ternarywire_core::prepared::prepare(&req, &pctx);
        let result = async {
            ternarywire_auth::apply(
                &mut prepared,
                &ternarywire_auth::AuthContext { tokens: services.tokens.as_ref(), http: services.http.as_ref() },
            )
            .await
            .map_err(|e| e.to_string())?;
            let out = services
                .http
                .send(&prepared, &ternarywire_http::SendOptions::default())
                .await
                .map_err(|e| e.message)?;
            let json: serde_json::Value =
                serde_json::from_slice(&out.response.body.read_all().map_err(|e| e.to_string())?)
                    .map_err(|e| format!("introspection response is not JSON: {e}"))?;
            ternarywire_convert::graphql::parse_introspection(&json).map_err(|e| e.to_string())
        }
        .await;
        let _ = tx.send(crate::app::Msg::Schema { tab: tab_id, result });
        ctx.request_repaint();
    });
}

// ---------------------------------------------------------------------------
// Building blocks shared by the request, runner, environment and container
// views (candidates for kit.rs).
// ---------------------------------------------------------------------------

#[allow(dead_code)] // Shared with the other views.
pub(crate) fn body_size(ui: &Ui) -> f32 {
    ui.style().text_styles.get(&egui::TextStyle::Body).map(|f| f.size).unwrap_or(13.0)
}

pub(crate) fn mono_size(ui: &Ui) -> f32 {
    ui.style().text_styles.get(&egui::TextStyle::Monospace).map(|f| f.size).unwrap_or(12.5)
}

/// A left-to-right row exactly `height` high with its content centered
/// vertically (`ui.horizontal` rows are at least the interact height, 30).
pub(crate) fn hrow<R>(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(height);
        add(ui)
    })
}

/// One line of text cut with an ellipsis at `width`.
pub(crate) fn elide(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    kit::elide(ui, text, font, color, width)
}

/// A collapsible section header (caret, title, optional count). Returns
/// whether it is open.
pub(crate) fn disclosure(ui: &mut Ui, id: egui::Id, title: &str, count: Option<usize>, default_open: bool) -> bool {
    kit::disclosure(ui, id, title, count, default_open)
}

/// A hairline between form rows inside a [`settings_group`] (11 px around
/// it, like `.frow`).
#[allow(dead_code)] // Shared with the other views.
pub(crate) fn form_divider(ui: &mut Ui) {
    let p = theme::pal(ui);
    ui.add_space(11.0);
    let y = ui.cursor().top();
    let r = ui.max_rect();
    ui.painter().hline(r.left()..=r.right(), y, Stroke::new(1.0, p.border));
    ui.add_space(11.0);
}

/// A read-only table (`.tbl` in a `.tbl-wrap`): a 30 px header on `well`,
/// 34 px rows with hairlines, one-line cells (whole on hover), hover fill,
/// a copy button on the hovered row and a context menu. `widths` are column
/// fractions. With two columns the first (names) is muted; with more, the
/// last (notes) is muted proportional text.
pub(crate) fn read_table(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    columns: &[&str],
    widths: &[f32],
    rows: &[Vec<String>],
) {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id(id);
    const HEAD: f32 = 30.0;
    const COPY: f32 = 34.0;
    const PAD: f32 = 12.0;
    let width = ui.available_width().max(220.0);
    let left = ui.cursor().left();
    let top = ui.cursor().top();
    let mut xs = vec![left];
    for w in widths {
        let last = *xs.last().unwrap_or(&left);
        xs.push(last + width * w);
    }
    let bg = ui.painter().add(egui::Shape::Noop);
    let head = Rect::from_min_size(pos2(left, top), vec2(width, HEAD));
    ui.painter().rect_filled(head, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, p.well);
    for (c, label) in columns.iter().enumerate() {
        ui.painter().text(
            pos2(xs[c] + PAD, head.center().y),
            Align2::LEFT_CENTER,
            *label,
            theme::medium(11.5),
            p.text_muted,
        );
    }
    ui.painter().hline(left..=left + width, head.bottom() - 0.5, Stroke::new(1.0, p.border));
    let mut y = head.bottom();
    let n_cols = columns.len();
    for (i, row) in rows.iter().enumerate() {
        // One line per cell (cut with an ellipsis; the hover shows it whole).
        let galleys: Vec<_> = row
            .iter()
            .enumerate()
            .take(n_cols)
            .map(|(c, text)| {
                let w = (xs[c + 1].min(left + width) - xs[c] - 2.0 * PAD).max(20.0);
                let (font, color) = if n_cols == 2 && c == 0 {
                    (theme::mono(12.5), p.text_muted)
                } else if n_cols > 2 && c + 1 == n_cols {
                    (egui::FontId::proportional(12.5), p.text_muted)
                } else {
                    (theme::mono(12.5), p.text)
                };
                let line = text.replace(['\n', '\r'], " ");
                (elide(ui, &line, font, color, w), color)
            })
            .collect();
        let h = 34.0;
        let row_rect = Rect::from_min_size(pos2(left, y), vec2(width, h));
        if i > 0 {
            ui.painter().hline(left..=left + width, y, Stroke::new(1.0, p.border));
        }
        let elided = galleys.iter().any(|(g, _)| g.elided);
        let mut resp = ui.interact(row_rect, id.with(i), Sense::click());
        if elided {
            resp = resp.on_hover_ui(|ui| {
                ui.set_max_width(480.0);
                for (c, text) in row.iter().enumerate().take(n_cols) {
                    ui.label(RichText::new(text).font(if c + 1 == n_cols && n_cols > 2 {
                        egui::FontId::proportional(12.0)
                    } else {
                        theme::mono(12.0)
                    }));
                }
            });
        }
        let hovered = ui.rect_contains_pointer(row_rect);
        if hovered {
            let last = i + 1 == rows.len();
            let r = row_rect
                .shrink2(vec2(1.0, 0.0))
                .with_min_y(y + 0.5)
                .with_max_y(row_rect.bottom() - if last { 1.0 } else { 0.0 });
            let radius = if last { CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 } } else { CornerRadius::ZERO };
            ui.painter().rect_filled(r, radius, p.well);
        }
        for (c, (g, color)) in galleys.into_iter().enumerate() {
            ui.painter().galley(pos2(xs[c] + PAD, row_rect.center().y - g.size().y / 2.0), g, color);
        }
        let row_text = if row.len() == 2 { format!("{}: {}", row[0], row[1]) } else { row.join("\t") };
        if hovered {
            // The copy button covers the end of the row.
            let cover =
                Rect::from_min_max(pos2(left + width - COPY - 8.0, y + 1.0), pos2(left + width - 1.0, y + h - 1.0));
            ui.painter().rect_filled(cover, 0.0, p.well);
            let d = Rect::from_center_size(pos2(left + width - COPY / 2.0, y + h / 2.0), vec2(22.0, 22.0));
            let r = ui
                .new_child(egui::UiBuilder::new().max_rect(d))
                .add(|ui: &mut Ui| kit::icon_button_sm(ui, icons::COPY, tr("Copy")));
            if r.clicked() {
                ui.ctx().copy_text(row_text.clone());
            }
        }
        resp.context_menu(|ui| {
            if row.len() > 1 && kit::menu_item(ui, Some(icons::COPY), tr("Copy value"), None).clicked() {
                ui.ctx().copy_text(row[1].clone());
            }
            if kit::menu_item(ui, Some(icons::COPY), tr("Copy name"), None).clicked() {
                ui.ctx().copy_text(row[0].clone());
            }
            if kit::menu_item(ui, Some(icons::CLIPBOARD_TEXT), tr("Copy row"), None).clicked() {
                ui.ctx().copy_text(row_text.clone());
            }
        });
        y += h;
    }
    let table = Rect::from_min_max(pos2(left, top), pos2(left + width, y));
    ui.painter().set(bg, egui::Shape::rect_filled(table, CornerRadius::same(9), p.surface));
    ui.painter().rect_stroke(table, CornerRadius::same(9), Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.advance_cursor_after_rect(table);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prettify keeps strings and comments as they are: braces and spaces
    /// in them used to be split onto lines of their own.
    #[test]
    fn prettify_graphql_keeps_strings_and_comments() {
        let q = r#"query { search(q: "a {b} c", tag: """x { y""") { id # the {id}
 name } }"#;
        let want =
            "query {\n  search(q: \"a {b} c\", tag: \"\"\"x { y\"\"\") {\n    id\n    # the {id}\n    name\n  }\n}";
        assert_eq!(prettify_graphql(q), want);
        assert_eq!(prettify_graphql("{ a b }"), "{\n  a\n  b\n}");
    }

    /// The request settings show the app's defaults (Settings > Requests)
    /// as "Default (on/off)", not the built-in ones.
    #[test]
    fn request_settings_show_the_app_defaults() {
        let defaults_off = |on: bool| {
            let dir = tempfile::tempdir().unwrap();
            let mut app = TernaryWireApp::for_test(dir.path());
            app.settings.request.follow_redirects = on;
            app.settings.request.verify_ssl = on;
            let mut tab = RequestTab::scratch(RequestItem::new("r", HttpRequest::new("GET", "http://x.test")));
            tab.section = Section::Settings;
            app.add_tab(crate::app::TabKind::Request(Box::new(tab)));
            app.draw_frame(Vec::new());
            app.draw_frame(Vec::new()).labels.iter().filter(|l| *l == tr("Default (off)")).count()
        };
        assert_eq!(defaults_off(false), defaults_off(true) + 2);
    }

    #[test]
    fn detects_subscriptions() {
        assert!(starts_with_subscription("  subscription OnMsg { msg }"));
        assert!(starts_with_subscription("# comment\nsubscription { a }"));
        assert!(!starts_with_subscription("query { a }"));
    }

    #[test]
    fn beautify_keeps_variables() {
        let out = beautify_json_with_vars("{\"id\": {{id}}, \"name\":\"{{name}}\", \"a\":[1,2]}").unwrap();
        assert!(out.contains("\"id\": {{id}}"));
        assert!(out.contains("\"name\": \"{{name}}\""));
        assert!(beautify_json_with_vars("{bad").is_none());
    }

    /// A pasted cURL command becomes the request and reports what it could
    /// not import exactly (shown as a toast); other text is the URL.
    #[test]
    fn pasted_curl_reports_its_warnings() {
        let mut http = HttpRequest::default();
        let warnings = url_edited(&mut http, "curl --data-urlencode name@notes.txt https://api.test/x".into());
        assert_eq!(http.method, "POST");
        assert!(http.url.starts_with("https://api.test/x"), "{}", http.url);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("notes.txt"), "{warnings:?}");
        assert!(url_edited(&mut http, "curl https://api.test/y".into()).is_empty());
        assert!(url_edited(&mut http, "https://api.test/{{path}}".into()).is_empty());
        assert_eq!(http.url, "https://api.test/{{path}}");
    }
}
