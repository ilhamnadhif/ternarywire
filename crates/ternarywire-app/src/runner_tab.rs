//! Collection Runner tab (FR-U1..U7): functional runs with data files and
//! custom order, and performance runs with virtual users.
//!
//! The tab has a 46 px title row (collection, Functional | Performance), the
//! setup panel on the left (run order, options, a footer with Run) and the
//! results on the right (a toolbar with the run state and Export, number
//! cards, then the results table or the load test chart and tables).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, pos2, vec2,
};

use ternarywire_core::model::Id;
use ternarywire_http::CancellationToken;
use ternarywire_runner::data::DataRow;
use ternarywire_runner::perf::{PerfOptions, PerfReport, PerfSample};
use ternarywire_runner::{RequestResult, RunEvent, RunOptions, RunSummary};

use crate::app::{Msg, TernaryWireApp};
use crate::flow_tab::num_input;
use crate::i18n::{tr, trf};
use crate::kit::{Tone, size};
use crate::request_tab::{elide, fmt_ms};
use crate::state::WsState;
use crate::theme::{RADIUS, RADIUS_SM};
use crate::{icons, kit, theme, widgets};

/// Results kept in memory for display; everything is also spooled to disk.
const KEEP_RESULTS: usize = 2000;
/// Default width of the setup panel.
const SETUP_W: f32 = 336.0;
/// Rows of the run order.
const ORDER_ROW: f32 = 24.0;
/// Rows and header of the result tables.
const TABLE_ROW: f32 = 34.0;
const TABLE_HEAD: f32 = 30.0;

pub enum PerfMsg {
    Sample(PerfSample),
    Done(Box<PerfReport>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Functional,
    Performance,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Passed,
    Failed,
}

/// A request in the run order: (id, name, badge, selected, folder path).
type PlanItem = (Id, String, String, bool, String);

pub struct RunnerTab {
    pub collection: Id,
    folder: Option<Id>,
    mode: Mode,
    /// The requests in run order.
    plan: Vec<PlanItem>,
    /// [`WsState::collections_revision`] the plan was made at.
    plan_revision: u64,
    iterations: usize,
    delay_ms: u64,
    data_path: Option<PathBuf>,
    data: Vec<DataRow>,
    data_error: Option<String>,
    /// The first rows of the data file are shown.
    preview_data: bool,
    persist_vars: bool,
    save_cookies: bool,
    bail: bool,
    fail_rate: f32,
    cancel: Option<CancellationToken>,
    started: Option<Instant>,
    /// Wall clock time the run started (`09:52`).
    started_at: String,
    expected: usize,
    results: std::collections::VecDeque<RequestResult>,
    dropped: usize,
    summary: Option<RunSummary>,
    live: RunSummary,
    spool: Option<tempfile::NamedTempFile>,
    filter: Filter,
    selected: Option<usize>,
    /// Scroll the results table to the selected row on the next frame.
    scroll_to_selected: bool,
    /// Run order row being dragged by its handle.
    drag: Option<usize>,
    /// Width of the setup panel.
    panel_w: f32,
    /// Narrow windows show the setup or the results, not both.
    show_results: bool,
    /// A message for the toast area (export results).
    note: Option<(String, bool)>,
    /// Name of the collection (exports).
    collection_name: String,
    // Performance mode.
    vus: usize,
    duration_s: u64,
    ramp_s: u64,
    perf_samples: Vec<PerfSample>,
    perf_report: Option<Box<PerfReport>>,
}

impl RunnerTab {
    pub fn new(ws: &WsState, collection: Id, folder: Option<Id>) -> Self {
        let plan = plan_of(ws, &collection, folder.as_ref());
        RunnerTab {
            collection,
            folder,
            mode: Mode::Functional,
            plan,
            plan_revision: ws.collections_revision(),
            iterations: 1,
            delay_ms: 0,
            data_path: None,
            data: Vec::new(),
            data_error: None,
            preview_data: false,
            persist_vars: true,
            save_cookies: true,
            bail: false,
            fail_rate: 0.0,
            cancel: None,
            started: None,
            started_at: String::new(),
            expected: 0,
            results: Default::default(),
            dropped: 0,
            summary: None,
            live: RunSummary::default(),
            spool: None,
            filter: Filter::All,
            selected: None,
            scroll_to_selected: false,
            drag: None,
            panel_w: SETUP_W,
            show_results: false,
            note: None,
            collection_name: String::new(),
            vus: 10,
            duration_s: 30,
            ramp_s: 0,
            perf_samples: Vec::new(),
            perf_report: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.cancel.is_some()
    }

    /// Follows edits of the collection (requests added, removed, renamed)
    /// while keeping the chosen order and selection.
    fn refresh_plan(&mut self, ws: &WsState) {
        if self.plan_revision == ws.collections_revision() || self.is_running() {
            return;
        }
        self.plan_revision = ws.collections_revision();
        let fresh = plan_of(ws, &self.collection, self.folder.as_ref());
        let chosen: HashMap<&Id, (usize, bool)> = self.plan.iter().enumerate().map(|(i, p)| (&p.0, (i, p.3))).collect();
        // Requests already planned keep their place; new ones come last.
        let mut next: Vec<(usize, PlanItem)> = fresh
            .into_iter()
            .map(|mut p| match chosen.get(&p.0) {
                Some(&(i, on)) => {
                    p.3 = on;
                    (i, p)
                }
                None => (usize::MAX, p),
            })
            .collect();
        next.sort_by_key(|(i, _)| *i);
        self.plan = next.into_iter().map(|(_, p)| p).collect();
        self.drag = None;
    }

    pub fn cancel_all(&self) {
        if let Some(c) = &self.cancel {
            c.cancel();
        }
    }

    pub fn on_event(&mut self, e: RunEvent) {
        match e {
            RunEvent::RequestFinished(r) => {
                if let Some(spool) = &mut self.spool
                    && let Ok(line) = serde_json::to_string(&*r)
                {
                    let _ = writeln!(spool, "{line}");
                }
                self.live.executed += 1;
                if r.error.is_some() {
                    self.live.requests_failed += 1;
                }
                for t in &r.tests {
                    if t.skipped {
                        self.live.tests_skipped += 1;
                    } else if t.passed {
                        self.live.tests_passed += 1;
                    } else {
                        self.live.tests_failed += 1;
                    }
                }
                self.live.script_errors += r.script_errors.len();
                self.results.push_back(*r);
                if self.results.len() > KEEP_RESULTS {
                    self.results.pop_front();
                    self.dropped += 1;
                    self.selected = self.selected.and_then(|s| s.checked_sub(1));
                }
            }
            RunEvent::Finished(s) => self.summary = Some(s),
            _ => {}
        }
    }

    pub fn on_perf(&mut self, m: PerfMsg) {
        match m {
            PerfMsg::Sample(s) => self.perf_samples.push(s),
            PerfMsg::Done(r) => {
                self.perf_report = Some(r);
                self.cancel = None;
            }
        }
    }

    pub fn finished(&mut self) {
        self.cancel = None;
    }

    fn reset(&mut self) {
        self.results.clear();
        self.dropped = 0;
        self.summary = None;
        self.live = RunSummary::default();
        self.selected = None;
        self.perf_samples.clear();
        self.perf_report = None;
        self.spool = tempfile::NamedTempFile::new().ok();
    }

    /// The samples of the load test: the report's, or the live ones.
    fn samples(&self) -> &[PerfSample] {
        match &self.perf_report {
            Some(r) if !r.timeline.is_empty() => &r.timeline,
            _ => &self.perf_samples,
        }
    }

    /// Sets options from an automation step (see `automation::Step::RunnerOptions`).
    pub fn set_options(&mut self, v: &serde_json::Value, root: &Path) {
        let num = |k: &str| v.get(k).and_then(serde_json::Value::as_u64);
        match v.get("mode").and_then(serde_json::Value::as_str) {
            Some("performance") => self.mode = Mode::Performance,
            Some("functional") => self.mode = Mode::Functional,
            _ => {}
        }
        if let Some(f) = v.get("data_file").and_then(serde_json::Value::as_str) {
            let path = root.join(f);
            match ternarywire_runner::data::load(&path) {
                Ok(rows) => {
                    self.iterations = rows.len().max(1);
                    self.data = rows;
                    self.data_error = None;
                }
                Err(e) => self.data_error = Some(e.to_string()),
            }
            self.data_path = Some(path);
        }
        if let Some(n) = num("iterations") {
            self.iterations = n.max(1) as usize;
        }
        if let Some(n) = num("delay_ms") {
            self.delay_ms = n;
        }
        if let Some(n) = num("vus") {
            self.vus = n.clamp(1, 1000) as usize;
        }
        if let Some(n) = num("duration_s") {
            self.duration_s = n.max(1);
        }
        if let Some(n) = num("ramp_s") {
            self.ramp_s = n;
        }
        if let Some(only) = v.get("only").and_then(serde_json::Value::as_array) {
            let names: Vec<&str> = only.iter().filter_map(serde_json::Value::as_str).collect();
            for p in &mut self.plan {
                p.3 = names.contains(&p.1.as_str());
            }
        }
    }

    /// Selects the result `<request name>#<iteration>` (1-based; the first
    /// one of that name without `#`).
    pub fn select_result(&mut self, which: &str) {
        let (name, it) = match which.rsplit_once('#') {
            Some((n, i)) => (n, i.parse::<usize>().ok()),
            None => (which, None),
        };
        self.selected = self.results.iter().position(|r| r.name == name && it.is_none_or(|i| r.iteration + 1 == i));
        self.scroll_to_selected = true;
        self.show_results = true;
    }

    /// Method badge of a planned request (per-request rows of a load test).
    fn badge_of(&self, request_id: &str) -> Option<&str> {
        self.plan.iter().find(|p| p.0 == request_id).map(|p| p.2.as_str())
    }
}

fn start(app: &mut TernaryWireApp, tab_id: u64, tab: &mut RunnerTab) {
    let Some(c) = app.ws.data.collection(&tab.collection).cloned() else { return };
    let order: Vec<String> = tab.plan.iter().filter(|p| p.3).map(|p| p.0.clone()).collect();
    if order.is_empty() {
        app.toast(tr("Select at least one request to run"), true);
        return;
    }
    tab.reset();
    let before = app.ws.scopes_for(Some(&tab.collection));
    let env = app.ws.data.active_environment.clone();
    let mut services = app.services();
    if !tab.save_cookies {
        // Run against a copy of the jar so cookies set during the run are discarded.
        let jar = std::sync::Arc::new(ternarywire_http::CookieJar::new());
        for ck in app.ws.cookies.list() {
            let _ = jar.set(&ck.domain, &ck.to_set_cookie());
        }
        services.http = std::sync::Arc::new(ternarywire_http::HttpEngine::new(app.http.config(), jar));
    }
    let cancel = CancellationToken::new();
    tab.cancel = Some(cancel.clone());
    tab.started = Some(Instant::now());
    tab.started_at = chrono::Local::now().format("%H:%M").to_string();
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    let collection_id = tab.collection.clone();
    let persist = tab.persist_vars;
    let ws = app.ws_gen;
    match tab.mode {
        Mode::Functional => {
            let iterations = tab.iterations.max(1);
            tab.expected = iterations * order.len();
            let opts = RunOptions {
                iterations: Some(iterations),
                delay_ms: tab.delay_ms,
                data: tab.data.clone(),
                order: Some(order),
                bail: tab.bail,
                simulate_failure_rate: tab.fail_rate,
                base_dir: Some(app.ws.root().to_path_buf()),
                body_preview_bytes: 2048,
                keep_console: false,
                ..Default::default()
            };
            app.rt.spawn(async move {
                let mut vars = before.clone();
                let tx2 = tx.clone();
                let ctx2 = ctx.clone();
                let mut last_paint = Instant::now();
                let mut sink = move |e: RunEvent| {
                    let finished = matches!(e, RunEvent::Finished(_));
                    let _ = tx2.send(Msg::Run { tab: tab_id, event: Box::new(e) });
                    if finished || last_paint.elapsed() > Duration::from_millis(100) {
                        ctx2.request_repaint();
                        last_paint = Instant::now();
                    }
                };
                let result =
                    ternarywire_runner::run_collection(&services, &c, &mut vars, opts, &mut sink, &cancel).await;
                if let Err(e) = result {
                    let _ = tx.send(Msg::Toast(format!("Run failed: {e}"), true));
                }
                let _ = tx.send(Msg::RunFinished {
                    tab: tab_id,
                    ws,
                    before,
                    after: vars,
                    collection: collection_id,
                    env,
                    persist,
                });
                ctx.request_repaint();
            });
        }
        Mode::Performance => {
            let opts = PerfOptions {
                virtual_users: tab.vus.max(1),
                duration: Duration::from_secs(tab.duration_s.max(1)),
                ramp_up: Duration::from_secs(tab.ramp_s),
                request_ids: Some(order),
            };
            let base = app.ws.root().to_path_buf();
            app.rt.spawn(async move {
                let tx2 = tx.clone();
                let ctx2 = ctx.clone();
                let mut on_sample = move |s: &PerfSample| {
                    let _ = tx2.send(Msg::Perf { tab: tab_id, event: PerfMsg::Sample(s.clone()) });
                    ctx2.request_repaint();
                };
                let report = ternarywire_runner::perf::run_perf_in_dir(
                    &services,
                    &c,
                    &before,
                    opts,
                    Some(&base),
                    &mut on_sample,
                    &cancel,
                )
                .await;
                let _ = tx.send(Msg::Perf { tab: tab_id, event: PerfMsg::Done(Box::new(report)) });
                ctx.request_repaint();
            });
        }
    }
}

/// Writes every result of the functional run as `format` (json, junit,
/// html) to a file the user picks. `Ok(None)`: the dialog was cancelled.
fn export(tab: &RunnerTab, format: &str) -> Result<Option<PathBuf>, String> {
    let (Some(spool), Some(summary)) = (&tab.spool, &tab.summary) else { return Err("nothing to export".into()) };
    let ext = match format {
        "junit" => "xml",
        "html" => "html",
        _ => "json",
    };
    let Some(path) = rfd::FileDialog::new().set_file_name(format!("ternarywire-run.{ext}")).save_file() else {
        return Ok(None);
    };
    let mut reporter: Box<dyn ternarywire_runner::report::Reporter> = match format {
        "junit" => Box::new(ternarywire_runner::report::JunitReporter::new(&path, &summary.collection)),
        "html" => Box::new(ternarywire_runner::report::HtmlReporter::new(&path)),
        _ => Box::new(ternarywire_runner::report::JsonReporter::new(&path)),
    };
    let text = std::fs::read_to_string(spool.path()).map_err(|e| e.to_string())?;
    for line in text.lines() {
        if let Ok(r) = serde_json::from_str::<RequestResult>(line) {
            reporter.on_event(&RunEvent::RequestFinished(Box::new(r)));
        }
    }
    reporter.finish(summary).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

/// The requests of `collection` (or one folder of it) in run order, all
/// selected.
fn plan_of(ws: &WsState, collection: &Id, folder: Option<&Id>) -> Vec<PlanItem> {
    let Some(c) = ws.data.collection(collection) else { return Vec::new() };
    let opts = RunOptions { folder: folder.cloned(), ..Default::default() };
    // Every protocol runs: HTTP, gRPC, AI, MCP, and WebSocket, Socket.IO and
    // MQTT as sessions with a stop condition.
    ternarywire_runner::runner::plan(c, &opts)
        .map(|list| {
            list.into_iter()
                .map(|(r, path)| (r.id.clone(), r.name.clone(), r.request.badge(), true, path.join(" / ")))
                .collect()
        })
        .unwrap_or_default()
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut RunnerTab) {
    tab.refresh_plan(&app.ws);
    if let Some((text, error)) = tab.note.take() {
        app.toast(text, error);
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let p = theme::pal(ui);
    let name = app.ws.data.collection(&tab.collection).map(|c| c.name.clone()).unwrap_or_default();
    title_row(ui, tab, &name);
    tab.collection_name = name;
    let mut run = false;
    let body = ui.available_rect_before_wrap();
    let area = |ui: &mut Ui, rect: Rect, salt: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.scope_builder(UiBuilder::new().max_rect(rect).id_salt(salt).layout(Layout::top_down(Align::Min)), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    };
    if body.width() < 820.0 {
        // Narrow: one pane at a time, switched with a segmented control.
        let has_results = !tab.results.is_empty() || tab.summary.is_some() || tab.perf_report.is_some();
        let results_label = if has_results && tab.mode == Mode::Functional {
            format!("{} {}", tr("Results"), tab.results.len())
        } else {
            tr("Results").to_string()
        };
        kit::bar(ui, size::TOOLBAR, true, |ui| {
            kit::segmented_sm(ui, &mut tab.show_results, &[(false, tr("Setup")), (true, results_label.as_str())]);
        });
        let rest = ui.available_rect_before_wrap();
        if tab.show_results {
            area(ui, rest, "runner-results", &mut |ui| results(ui, tab));
        } else {
            area(ui, rest, "runner-setup", &mut |ui| setup_panel(ui, tab, &mut run));
        }
    } else {
        // Setup on the left (resizable by its edge), results on the right.
        let max_w = (body.width() * 0.5).max(290.0);
        let panel_w = tab.panel_w.clamp(290.0, max_w);
        let left = Rect::from_min_size(body.min, vec2(panel_w, body.height()));
        let right = Rect::from_min_max(pos2(left.right() + 1.0, body.top()), body.max);
        area(ui, left, "runner-setup", &mut |ui| setup_panel(ui, tab, &mut run));
        ui.painter().vline(left.right() + 0.5, body.y_range(), Stroke::new(1.0, p.border));
        area(ui, right, "runner-results", &mut |ui| results(ui, tab));
        let handle = Rect::from_center_size(pos2(left.right() + 0.5, body.center().y), vec2(7.0, body.height()));
        let h = ui.interact(handle, ui.scope_id().with(("runner-split", tab_id)), Sense::click_and_drag());
        if h.hovered() || h.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if h.dragged() {
            tab.panel_w = (panel_w + h.drag_delta().x).clamp(290.0, max_w);
        }
        if h.double_clicked() {
            tab.panel_w = SETUP_W;
        }
    }
    if app.pending_runner_start == Some(tab_id) {
        app.pending_runner_start = None;
        run = true;
    }
    if run {
        tab.show_results = true;
        start(app, tab_id, tab);
    }
    ui.advance_cursor_after_rect(body);
}

fn results(ui: &mut Ui, tab: &mut RunnerTab) {
    match tab.mode {
        Mode::Functional => functional_results(ui, tab),
        Mode::Performance => perf_results(ui, tab),
    }
}

/// The 46 px title row: the runner's tile and name, the collection, and the
/// mode switch.
fn title_row(ui: &mut Ui, tab: &mut RunnerTab, collection: &str) {
    let p = theme::pal(ui);
    let running = tab.is_running();
    kit::bar(ui, size::VIEW_TITLE, true, |ui| {
        ui.add_space(2.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(!running, |ui| {
                kit::segmented_ex(
                    ui,
                    &mut tab.mode,
                    &[
                        (Mode::Functional, Some(icons::LIST_CHECKS), tr("Functional")),
                        (Mode::Performance, Some(icons::GAUGE), tr("Performance")),
                    ],
                    true,
                );
            });
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (tile, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                ui.painter().rect_filled(tile, CornerRadius::same(8), p.accent_soft);
                ui.painter().text(
                    tile.center(),
                    Align2::CENTER_CENTER,
                    icons::PLAY_CIRCLE,
                    theme::icon_font(16.0),
                    p.accent_text,
                );
                ui.label(RichText::new(tr("Collection Runner")).font(theme::semibold(14.0)).color(p.text));
                ui.label(RichText::new("·").color(p.text_faint));
                ui.add(egui::Label::new(RichText::new(collection).color(p.text_muted)).truncate());
            });
        });
    });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

/// The setup panel: run order and options (scrolling), and a footer with
/// the summary and Run.
fn setup_panel(ui: &mut Ui, tab: &mut RunnerTab, run: &mut bool) {
    let p = theme::pal(ui);
    let rect = ui.max_rect();
    let foot = Rect::from_min_max(pos2(rect.left(), rect.bottom() - size::ACTION_ROW), rect.max);
    let scroll = Rect::from_min_max(rect.min, pos2(rect.right(), foot.top()));
    let mut c = ui.new_child(UiBuilder::new().max_rect(scroll).layout(Layout::top_down(Align::Min)));
    c.set_clip_rect(scroll.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt("runner-cfg").auto_shrink([false; 2]).show(&mut c, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        run_order(ui, tab);
        ui.add_space(4.0);
        let y = ui.cursor().top();
        ui.painter().hline(ui.max_rect().x_range(), y + 0.5, Stroke::new(1.0, p.border));
        ui.add_space(1.0);
        match tab.mode {
            Mode::Functional => functional_options(ui, tab),
            Mode::Performance => perf_options(ui, tab),
        }
    });

    // Footer with the main action.
    ui.painter().rect_filled(foot, CornerRadius { sw: theme::RADIUS_LG - 1, ..Default::default() }, p.well);
    ui.painter().hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, p.border));
    let mut f = ui.new_child(
        UiBuilder::new().max_rect(foot.shrink2(vec2(size::PAD, 0.0))).layout(Layout::right_to_left(Align::Center)),
    );
    let selected = tab.plan.iter().filter(|p| p.3).count();
    if tab.is_running() {
        let o = kit::ButtonOpts { icon: Some(icons::STOP), ..Default::default() };
        if kit::button(&mut f, kit::Kind::Danger, kit::Size::Md, tr("Stop"), o).clicked() {
            tab.cancel_all();
        }
    } else {
        f.add_enabled_ui(selected > 0, |ui| {
            let label = match tab.mode {
                Mode::Functional => tr("Run"),
                Mode::Performance => tr("Start load test"),
            };
            if kit::primary_icon(ui, icons::PLAY, label).clicked() {
                *run = true;
            }
        })
        .response
        .on_disabled_hover_text(tr("Select at least one request to run"));
    }
    let summary = match tab.mode {
        Mode::Functional if tab.iterations <= 1 => trf("{} requests", &[&selected]),
        Mode::Functional => trf("{} requests × {} iterations", &[&selected, &tab.iterations]),
        Mode::Performance => trf("{} users · {} s", &[&tab.vus, &tab.duration_s]),
    };
    f.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.add(egui::Label::new(kit::hint(ui, &summary)).truncate());
    });
}

/// The requests to run, in order: drag handle, checkbox, method, name and
/// folder.
fn run_order(ui: &mut Ui, tab: &mut RunnerTab) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let selected = tab.plan.iter().filter(|p| p.3).count();
    kit::toolbar(ui, |ui| {
        ui.label(kit::section_title(tr("Run order")));
        kit::tone_pill(
            ui,
            &format!("{selected}/{}", tab.plan.len()),
            if selected == 0 { Tone::Err } else { Tone::Neutral },
        );
        ui.label(RichText::new(icons::INFO).font(theme::icon_font(14.0)).color(p.text_faint))
            .on_hover_text(tr("Drag the handles to change the order; right-click a request for more."));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if kit::ghost(ui, None, tr("None")).on_hover_text(tr("Deselect all")).clicked() {
                tab.plan.iter_mut().for_each(|p| p.3 = false);
            }
            if kit::ghost(ui, None, tr("All")).on_hover_text(tr("Select all")).clicked() {
                tab.plan.iter_mut().for_each(|p| p.3 = true);
            }
        });
    });
    if tab.plan.is_empty() {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
            kit::banner(ui, Tone::Info, icons::INFO, tr("This collection has no requests to run."));
        });
        return;
    }
    let list_top = ui.cursor().top();
    let width = ui.available_width();
    let n = tab.plan.len();
    let mut moved: Option<(usize, usize)> = None;
    let mut toggle_only: Option<usize> = None;
    let drop_at = tab.drag.and_then(|_| {
        ui.ctx().pointer_interact_pos().map(|pos| (((pos.y - list_top) / ORDER_ROW).round().max(0.0) as usize).min(n))
    });
    for i in 0..n {
        let (full, resp) = ui.allocate_exact_size(vec2(width, ORDER_ROW), Sense::click());
        let rect = full.shrink2(vec2(6.0, 0.0));
        let dragging = tab.drag == Some(i);
        let hovered = ui.rect_contains_pointer(rect);
        if dragging {
            ui.painter().rect_filled(rect, CornerRadius::same(RADIUS_SM), p.accent_soft);
        } else if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(RADIUS_SM), p.hover);
        }
        let (_, name, badge, on, folder) = &mut tab.plan[i];
        let cy = rect.center().y;
        // Drag handle.
        let handle = Rect::from_min_size(rect.min, vec2(24.0, ORDER_ROW));
        let h = ui.interact(handle, ui.make_persistent_id(("drag", i)), Sense::drag());
        widgets::a11y_name(&h, &trf("Move {}", &[&name]));
        if h.hovered() || h.dragged() {
            ui.ctx().set_cursor_icon(if h.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        let handle_color = if h.hovered() || dragging { p.text_muted } else { p.text_faint };
        ui.painter().text(
            pos2(rect.left() + 6.0 + 6.5, cy),
            Align2::CENTER_CENTER,
            icons::DOTS_SIX_VERTICAL,
            theme::icon_font(13.0),
            handle_color,
        );
        if h.drag_started() {
            tab.drag = Some(i);
        }
        if h.drag_stopped()
            && let Some(to) = drop_at
        {
            moved = Some((i, to));
        }
        // Checkbox.
        let mut x = rect.left() + 6.0 + 13.0 + 8.0;
        let check = Rect::from_min_size(pos2(x, cy - 7.5), vec2(15.0, 15.0));
        kit::check(&mut ui.new_child(UiBuilder::new().max_rect(check)), on, name);
        x += 15.0 + 8.0;
        // Method, name and folder.
        let g = ui.painter().layout_no_wrap(
            widgets::method_short(badge).to_string(),
            theme::semibold(10.5),
            theme::method_color(badge, dark),
        );
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, p.text);
        x += 34.0 + 8.0;
        let mut right = rect.right() - 12.0;
        if !folder.is_empty() {
            let g = elide(ui, folder, FontId::proportional(11.0), p.text_faint, 90.0);
            right -= g.size().x;
            ui.painter().galley(pos2(right, cy - g.size().y / 2.0), g, p.text_faint);
            right -= 8.0;
        }
        let color = if *on { p.text } else { p.text_faint };
        let g = elide(ui, name, FontId::proportional(13.0), color, (right - x).max(20.0));
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, color);
        let resp = resp.on_hover_text(tr("Right-click: Move up · Move down · Run only this"));
        if resp.clicked() {
            *on = !*on;
        }
        resp.context_menu(|ui| {
            if kit::menu_item(ui, Some(icons::ARROW_UP), tr("Move up"), None).clicked() && i > 0 {
                moved = Some((i, i - 1));
            }
            if kit::menu_item(ui, Some(icons::ARROW_DOWN), tr("Move down"), None).clicked() && i + 1 < n {
                moved = Some((i, i + 2));
            }
            if kit::menu_item(ui, Some(icons::CHECK_SQUARE), tr("Run only this"), None).clicked() {
                toggle_only = Some(i);
            }
        });
    }
    // Where the dragged row would land.
    if let (Some(from), Some(to)) = (tab.drag, drop_at)
        && to != from
        && to != from + 1
    {
        let y = list_top + to as f32 * ORDER_ROW;
        let left = ui.min_rect().left();
        ui.painter().hline((left + 10.0)..=(left + width - 10.0), y, Stroke::new(2.0, p.accent));
    }
    if moved.is_some() || !ui.input(|i| i.pointer.any_down()) {
        tab.drag = None;
    }
    if let Some((from, to)) = moved {
        let item = tab.plan.remove(from);
        let to = if to > from { to - 1 } else { to };
        tab.plan.insert(to.min(tab.plan.len()), item);
    }
    if let Some(i) = toggle_only {
        for (j, p) in tab.plan.iter_mut().enumerate() {
            p.3 = j == i;
        }
    }
}

/// An option row (`.frow` of the setup panel): a hairline above (except the
/// first), label and help on the left, the control (`control_w` wide) on the
/// right, both centered on the row, 7 px above and below.
fn frow(ui: &mut Ui, first: bool, label: &str, help: &str, control_w: f32, add: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    if !first {
        let y = ui.cursor().top();
        ui.painter().hline(ui.max_rect().x_range(), y + 0.5, Stroke::new(1.0, p.border));
    }
    ui.add_space(7.0);
    let w = ui.available_width();
    let label_w = (w - control_w - 14.0).max(80.0);
    let lay = |text: &str, font: FontId, color: Color32, line: f32| {
        let mut job = LayoutJob::default();
        job.append(text, 0.0, TextFormat { line_height: Some(line), ..TextFormat::simple(font, color) });
        job.wrap.max_width = label_w;
        ui.painter().layout_job(job)
    };
    let lg = lay(label, theme::medium(13.0), p.text, 19.0);
    let hg = (!help.is_empty()).then(|| lay(help, FontId::proportional(12.0), p.text_muted, 17.0));
    let text_h = lg.size().y + hg.as_ref().map_or(0.0, |g| 1.0 + g.size().y);
    let h = text_h.max(26.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let top = rect.top() + (h - text_h) / 2.0;
    let lh = lg.size().y;
    ui.painter().galley(pos2(rect.left(), top), lg, p.text);
    if let Some(g) = hg {
        ui.painter().galley(pos2(rect.left(), top + lh + 1.0), g, p.text_muted);
    }
    let control = Rect::from_min_max(pos2(rect.left() + label_w + 14.0, rect.top()), rect.max);
    add(&mut ui.new_child(UiBuilder::new().max_rect(control).layout(Layout::right_to_left(Align::Center))));
    ui.add_space(7.0);
}

/// A number input with a unit after it (`12`, `0 ms`).
fn num_unit<T: egui::emath::Numeric>(
    ui: &mut Ui,
    name: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        num_input(ui, ("runner-num", name), name, value, range, width);
        if !unit.is_empty() {
            ui.label(RichText::new(unit).size(12.0).color(theme::pal(ui).text_faint));
        }
    });
}

/// Options inside the setup panel: a 30 px heading, then rows with 12 px
/// side padding.
fn options_block(ui: &mut Ui, title: &str, head_h: f32, add: impl FnOnce(&mut Ui)) {
    kit::bar(ui, head_h, false, |ui| {
        ui.label(kit::section_title(title));
    });
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 8 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
}

fn functional_options(ui: &mut Ui, tab: &mut RunnerTab) {
    let p = theme::pal(ui);
    options_block(ui, tr("Options"), 30.0, |ui| {
        // Iterations and delay side by side.
        ui.add_space(7.0);
        let half = ((ui.available_width() - 12.0) / 2.0).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.allocate_ui_with_layout(vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
                kit::field(ui, tr("Iterations"), tr("Times to run the requests."), |ui| {
                    num_unit(ui, tr("Iterations"), &mut tab.iterations, 1.0..=1_000_000.0, 80.0, "");
                });
            });
            ui.allocate_ui_with_layout(vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
                kit::field(ui, tr("Delay"), tr("Pause between requests."), |ui| {
                    num_unit(ui, tr("Delay"), &mut tab.delay_ms, 0.0..=600_000.0, 80.0, "ms");
                });
            });
        });
        ui.add_space(7.0);
        let mut pick = false;
        let label = if tab.data_path.is_some() { tr("Change…") } else { tr("Select file") };
        // A small button: padding 8 and a 1 px border, a 13 px icon, a 5 px gap and the label.
        let button_w =
            ui.painter().layout_no_wrap(label.to_string(), theme::medium(12.0), p.text).size().x + 18.0 + 13.0 + 5.0;
        frow(ui, false, tr("Data file"), tr("CSV or JSON; one iteration per row."), button_w, |ui| {
            let o = kit::ButtonOpts { icon: Some(icons::FILE_ARROW_UP), ..Default::default() };
            pick = kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, label, o).clicked();
        });
        if pick && let Some(path) = rfd::FileDialog::new().add_filter("CSV or JSON", &["csv", "json"]).pick_file() {
            match ternarywire_runner::data::load(&path) {
                Ok(rows) => {
                    tab.iterations = rows.len().max(1);
                    tab.data = rows;
                    tab.data_error = None;
                }
                Err(e) => tab.data_error = Some(e.to_string()),
            }
            tab.data_path = Some(path);
        }
        if let Some(path) = tab.data_path.clone() {
            if file_chip(ui, tab, &path) {
                tab.data_path = None;
                tab.data.clear();
                tab.data_error = None;
                tab.preview_data = false;
            }
            ui.add_space(2.0);
            if tab.preview_data && !tab.data.is_empty() {
                ui.add_space(8.0);
                let keys: Vec<String> = tab.data.first().map(|r| r.keys().cloned().collect()).unwrap_or_default();
                if !keys.is_empty() {
                    let rows: Vec<Vec<String>> = tab
                        .data
                        .iter()
                        .take(10)
                        .map(|r| {
                            keys.iter()
                                .map(|k| r.get(k).map(ternarywire_core::variables::value_to_string).unwrap_or_default())
                                .collect()
                        })
                        .collect();
                    let cols: Vec<&str> = keys.iter().map(String::as_str).collect();
                    let widths = vec![1.0 / cols.len() as f32; cols.len()];
                    crate::request_tab::read_table(ui, "data-preview-table", &cols, &widths, &rows);
                }
                ui.add_space(8.0);
            }
        }
        if let Some(e) = &tab.data_error {
            ui.add_space(4.0);
            kit::banner(ui, Tone::Err, icons::WARNING_CIRCLE, e);
            ui.add_space(4.0);
        }
        frow(ui, false, tr("Keep variable values"), tr("Keep what scripts set, after the run."), 30.0, |ui| {
            kit::switch(ui, &mut tab.persist_vars, tr("Keep variable values after the run"));
        });
        frow(ui, false, tr("Save cookies"), tr("Off runs against a copy of the cookie jar."), 30.0, |ui| {
            kit::switch(ui, &mut tab.save_cookies, tr("Save cookies after the run"));
        });
        frow(ui, false, tr("Stop at the first failure"), "", 30.0, |ui| {
            kit::switch(ui, &mut tab.bail, tr("Stop at the first failure"));
        });
        frow(ui, false, tr("Simulate network failures"), tr("Requests that fail on purpose."), 118.0, |ui| {
            let mut pct = (tab.fail_rate * 100.0).round();
            ui.add_sized(
                [26.0, 18.0],
                egui::Label::new(RichText::new(format!("{pct:.0}%")).font(theme::mono(12.0)).color(p.text_muted)),
            );
            if pct_slider(ui, &mut pct, tr("Simulate network failures")).changed() {
                tab.fail_rate = pct / 100.0;
            }
        });
    });
}

/// The data file as a chip: icon, name, rows, Preview data and remove.
/// Returns true when removed.
fn file_chip(ui: &mut Ui, tab: &mut RunnerTab, path: &Path) -> bool {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(8), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let inner = Rect::from_min_max(pos2(rect.left() + 10.0, rect.top()), pos2(rect.right() - 4.0, rect.bottom()));
    let mut c = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    c.spacing_mut().item_spacing.x = 8.0;
    let remove = kit::icon_button_sm(&mut c, icons::X, tr("Remove data file")).clicked();
    let o = kit::ButtonOpts { icon: Some(icons::TABLE), on: tab.preview_data, ..Default::default() };
    if kit::button(&mut c, kit::Kind::Ghost, kit::Size::Sm, tr("Preview data"), o)
        .on_hover_text(tr("Show the first 10 rows"))
        .clicked()
    {
        tab.preview_data = !tab.preview_data;
    }
    c.label(RichText::new(trf("{} rows", &[&tab.data.len()])).size(11.0).color(p.text_faint));
    c.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.label(RichText::new(icons::FILE_TEXT).font(theme::icon_font(15.0)).color(p.text_muted));
        let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        ui.add(egui::Label::new(RichText::new(file).font(theme::medium(13.0)).color(p.text)).truncate())
            .on_hover_text(path.display().to_string());
    });
    remove
}

/// A percentage slider (84 px track, 5 % steps); ←/→ step it when focused.
fn pct_slider(ui: &mut Ui, pct: &mut f32, name: &str) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(84.0, 18.0), Sense::click_and_drag());
    let enabled = ui.is_enabled();
    let value = *pct as f64;
    resp.widget_info(|| egui::WidgetInfo::slider(enabled, value, name));
    let before = *pct;
    if (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let t = ((pos.x - rect.left() - 5.0) / (rect.width() - 10.0)).clamp(0.0, 1.0);
        *pct = (t * 20.0).round() * 5.0;
    }
    if resp.has_focus() {
        let step = ui.input(|i| {
            (i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp)) as i32
                - (i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown)) as i32
        });
        *pct = (*pct + step as f32 * 5.0).clamp(0.0, 100.0);
    }
    if *pct != before {
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
        painter.rect_filled(track, CornerRadius::same(2), p.subtle);
        let cx = rect.left() + 5.0 + (*pct / 100.0) * (rect.width() - 10.0);
        if *pct > 0.0 {
            let fill = Rect::from_min_max(track.min, pos2(cx, track.bottom()));
            painter.rect_filled(fill, CornerRadius::same(2), p.accent);
        }
        let knob = pos2(cx, rect.center().y);
        painter.circle_filled(knob + vec2(0.0, 1.0), 7.0, Color32::from_black_alpha(40));
        painter.circle(knob, 7.0, p.on_accent, Stroke::new(1.0, p.border_strong));
        if resp.has_focus() {
            painter.circle_stroke(knob, 9.0, Stroke::new(1.5, p.accent));
        }
    }
    resp
}

fn perf_options(ui: &mut Ui, tab: &mut RunnerTab) {
    options_block(ui, tr("Load profile"), 36.0, |ui| {
        frow(ui, true, tr("Virtual users"), tr("Users running at the same time (1–1000)."), 64.0, |ui| {
            num_unit(ui, tr("Virtual users"), &mut tab.vus, 1.0..=1000.0, 64.0, "");
        });
        frow(ui, false, tr("Duration"), "", 78.0, |ui| {
            ui.label(RichText::new("s").size(12.0).color(theme::pal(ui).text_faint));
            num_input(ui, ("runner-num", "duration"), tr("Duration"), &mut tab.duration_s, 1.0..=3600.0, 64.0);
        });
        frow(ui, false, tr("Ramp-up"), tr("Time to reach all users."), 78.0, |ui| {
            ui.label(RichText::new("s").size(12.0).color(theme::pal(ui).text_faint));
            num_input(ui, ("runner-num", "ramp"), tr("Ramp-up"), &mut tab.ramp_s, 0.0..=3600.0, 64.0);
        });
        ui.add_space(6.0);
        ui.add(
            egui::Label::new(kit::hint(
                ui,
                tr("Each virtual user loops over the selected requests with its own copy of the variables."),
            ))
            .wrap(),
        );
        ui.add_space(4.0);
    });
}

// ---------------------------------------------------------------------------
// Results: shared pieces
// ---------------------------------------------------------------------------

/// Number cards in one row of `cols` equal columns (fewer when they would be
/// narrower than 118 px: six cards become two rows of three), 10 apart. The
/// value stays on one line; labels and hints wrap, and the cards of a row
/// share the height of the tallest.
fn stat_grid(ui: &mut Ui, cols: usize, cards: &[(&str, String, Option<Color32>, String)]) {
    if cards.is_empty() {
        return;
    }
    let p = theme::pal(ui);
    let gap = 10.0;
    let w = ui.available_width();
    let mut cols = cols.clamp(1, cards.len());
    while cols > 1 && (w - gap * (cols - 1) as f32) / cols as f32 <= 118.0 {
        cols = if cols.is_multiple_of(2) { cols / 2 } else { cols - 1 };
    }
    let cw = ((w - gap * (cols - 1) as f32) / cols as f32).floor();
    let text_w = (cw - 30.0).max(20.0);
    let measure = ui.painter().clone();
    let lay = |text: &str, font: FontId, color: Color32, line: f32, one_line: bool| {
        let mut job = LayoutJob::default();
        job.append(text, 0.0, TextFormat { line_height: Some(line), ..TextFormat::simple(font, color) });
        job.wrap = if one_line { egui::text::TextWrapping::truncate_at_width(text_w) } else { job.wrap };
        job.wrap.max_width = text_w;
        measure.layout_job(job)
    };
    for (r, chunk) in cards.chunks(cols).enumerate() {
        if r > 0 {
            ui.add_space(gap);
        }
        let laid: Vec<_> = chunk
            .iter()
            .map(|(label, value, color, sub)| {
                (
                    lay(label, FontId::proportional(11.5), p.text_muted, 17.0, false),
                    lay(value, theme::semibold(22.0), color.unwrap_or(p.text), 31.0, true),
                    lay(if sub.is_empty() { " " } else { sub }, FontId::proportional(11.5), p.text_faint, 17.0, false),
                )
            })
            .collect();
        let h = laid.iter().map(|(l, v, s)| l.size().y + 2.0 + v.size().y + s.size().y).fold(0.0, f32::max) + 26.0;
        let (row, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        for (i, (l, v, s)) in laid.into_iter().enumerate() {
            let card = Rect::from_min_size(pos2(row.left() + i as f32 * (cw + gap), row.top()), vec2(cw, h));
            let painter = ui.painter();
            painter.rect(
                card,
                CornerRadius::same(theme::RADIUS_CARD),
                p.surface,
                Stroke::new(1.0, p.border),
                StrokeKind::Inside,
            );
            let x = card.left() + 15.0;
            let mut y = card.top() + 13.0;
            let (lh, vh) = (l.size().y, v.size().y);
            painter.galley(pos2(x, y), l, p.text_muted);
            y += lh + 2.0;
            painter.galley(pos2(x, y), v, p.text);
            y += vh;
            painter.galley(pos2(x, y), s, p.text_faint);
        }
    }
}

/// A banner whose first sentence is bold and the rest muted.
fn result_banner(ui: &mut Ui, tone: Tone, icon: &str, bold: &str, rest: &str) {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    egui::Frame::new().fill(bg).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 9)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(RichText::new(icon).font(theme::icon_font(16.0)).color(fg));
                let mut job = LayoutJob::default();
                let strong = if matches!(tone, Tone::Ok | Tone::Err | Tone::Warn) { fg } else { p.text };
                job.append(bold, 0.0, TextFormat::simple(theme::semibold(12.5), strong));
                if !rest.is_empty() {
                    job.append(" ", 0.0, TextFormat::simple(FontId::proportional(12.5), p.text));
                    job.append(rest, 0.0, TextFormat::simple(FontId::proportional(12.5), p.text_muted));
                }
                job.wrap.max_width = ui.available_width();
                ui.add(egui::Label::new(job).wrap());
            });
        },
    );
}

/// The `.bar` progress line (6 px).
fn progress_bar(ui: &mut Ui, frac: f32) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(3), p.subtle);
    let w = (rect.width() * frac.clamp(0.0, 1.0)).max(6.0);
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(w, rect.height())), CornerRadius::same(3), p.accent);
    ui.ctx().request_repaint_after(Duration::from_millis(100));
}

/// A pill painted at `left_center` (cells of painted tables).
fn paint_pill(ui: &Ui, left_center: egui::Pos2, text: &str, tone: Tone) {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::medium(11.5), fg);
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 10.0), vec2(g.size().x + 14.0, 20.0));
    ui.painter().rect_filled(r, CornerRadius::same(10), bg);
    ui.painter().galley(pos2(r.left() + 7.0, r.center().y - g.size().y / 2.0), g, fg);
}

/// The frame of a painted table (`.tbl-wrap`): its header row on `well`,
/// the column titles (x, right-aligned) and the outline.
fn table_head(ui: &mut Ui, cols: &[(f32, bool, &str)]) -> Rect {
    let p = theme::pal(ui);
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), TABLE_HEAD), Sense::hover());
    ui.painter().rect_filled(head, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, p.well);
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));
    for (x, right, label) in cols {
        let align = if *right { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER };
        ui.painter().text(pos2(*x, head.center().y), align, *label, theme::medium(11.5), p.text_muted);
    }
    head
}

/// `1234567` → `1,234,567`.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The Export ▾ button with its menu: `(format, icon, label, detail)`.
fn export_menu(
    ui: &mut Ui,
    title: &str,
    width: f32,
    items: &[(&'static str, &str, &str, &str)],
) -> Option<&'static str> {
    let o = kit::ButtonOpts { icon: Some(icons::EXPORT), caret: true, ..Default::default() };
    let r = kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Export"), o);
    let mut chosen = None;
    egui::Popup::menu(&r).width(width).align(egui::RectAlign::BOTTOM_END).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        kit::menu_header(ui, title);
        for (format, icon, label, detail) in items {
            let item = kit::MenuItem { icon: Some(icon), label, detail: Some(detail), ..Default::default() };
            if kit::menu_item_ex(ui, item).clicked() {
                chosen = Some(*format);
            }
        }
    });
    chosen
}

fn export_note(result: Result<Option<PathBuf>, String>) -> Option<(String, bool)> {
    match result {
        Ok(Some(path)) => Some((trf("Exported to {}. {}", &[&path.display(), &""]).trim_end().to_string(), false)),
        Ok(None) => None,
        Err(e) => Some((trf("Export failed: {}", &[&e]), true)),
    }
}

// ---------------------------------------------------------------------------
// Results: functional
// ---------------------------------------------------------------------------

fn functional_results(ui: &mut Ui, tab: &mut RunnerTab) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    if tab.results.is_empty() && !tab.is_running() && tab.summary.is_none() {
        kit::empty_state(
            ui,
            icons::PLAY_CIRCLE,
            tr("Ready to run"),
            tr("Choose the requests and options on the left, then press Run."),
            |_| {},
        );
        return;
    }
    let s = tab.summary.clone().unwrap_or_else(|| tab.live.clone());
    let failed = s.tests_failed + s.script_errors;

    // Toolbar: run state, Export.
    let mut export_as = None;
    kit::toolbar(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if tab.summary.is_some() {
                export_as = export_menu(
                    ui,
                    tr("Export every result"),
                    220.0,
                    &[
                        ("json", icons::BRACKETS_CURLY, "JSON", ".json"),
                        ("junit", icons::CODE, "JUnit XML", ".xml"),
                        ("html", icons::FILE_TEXT, tr("HTML report"), ".html"),
                    ],
                );
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let elapsed = match (&tab.summary, tab.started) {
                    (Some(sum), _) => fmt_ms(sum.total_time_ms),
                    (None, Some(t)) => fmt_ms(t.elapsed().as_secs_f64() * 1000.0),
                    _ => String::new(),
                };
                if tab.is_running() {
                    kit::pill_ex(
                        ui,
                        tr("Running"),
                        Tone::Accent,
                        kit::PillOpts { dot: true, live: true, ..Default::default() },
                    );
                    ui.label(kit::hint(ui, &format!("{} / {} · {elapsed}", tab.live.executed, tab.expected)));
                } else if let Some(sum) = &tab.summary {
                    if sum.cancelled || sum.bailed {
                        kit::pill_ex(
                            ui,
                            tr("Stopped"),
                            Tone::Warn,
                            kit::PillOpts { icon: Some(icons::STOP), ..Default::default() },
                        );
                    } else {
                        kit::pill_ex(
                            ui,
                            tr("Finished"),
                            Tone::Neutral,
                            kit::PillOpts { icon: Some(icons::CHECK), ..Default::default() },
                        );
                    }
                    let iterations = if sum.iterations == 1 {
                        tr("1 iteration").to_string()
                    } else {
                        trf("{} iterations", &[&sum.iterations])
                    };
                    let mut hint = format!("{iterations} · {elapsed}");
                    if !tab.started_at.is_empty() {
                        hint.push_str(&format!(" · {}", trf("started {}", &[&tab.started_at])));
                    }
                    ui.add(egui::Label::new(kit::hint(ui, &hint)).truncate());
                }
            });
        });
    });
    if let Some(f) = export_as {
        tab.note = export_note(export(tab, f));
    }

    // Body: padding 4 12 16, 12 px between parts.
    let body = ui.available_rect_before_wrap();
    let content =
        Rect::from_min_max(pos2(body.left() + 12.0, body.top() + 4.0), pos2(body.right() - 12.0, body.bottom() - 16.0));
    let mut c = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    c.spacing_mut().item_spacing.y = 12.0;
    let ui = &mut c;

    let skipped = s.skipped + s.tests_skipped;
    let req_hint = if skipped > 0 { trf("{} skipped", &[&skipped]) } else { tr("requests").to_string() };
    let duration = match (&tab.summary, tab.started) {
        (Some(sum), _) => fmt_ms(sum.total_time_ms),
        (None, Some(t)) => fmt_ms(t.elapsed().as_secs_f64() * 1000.0),
        _ => "–".into(),
    };
    let avg = tab.summary.as_ref().map(|sum| fmt_ms(sum.response_time_avg_ms())).unwrap_or_else(|| "–".into());
    stat_grid(
        ui,
        6,
        &[
            (tr("Requests"), s.executed.to_string(), None, req_hint),
            (tr("Passed"), s.tests_passed.to_string(), (s.tests_passed > 0).then_some(p.ok), tr("tests").into()),
            (tr("Failed"), failed.to_string(), (failed > 0).then_some(p.err), tr("tests and scripts").into()),
            (
                tr("Errors"),
                s.requests_failed.to_string(),
                (s.requests_failed > 0).then_some(p.err),
                tr("network").into(),
            ),
            (tr("Duration"), duration, None, tr("total").into()),
            (tr("Avg response"), avg, None, tr("per request").into()),
        ],
    );
    if tab.is_running() {
        let frac = if tab.expected > 0 { tab.live.executed as f32 / tab.expected as f32 } else { 0.0 };
        progress_bar(ui, frac);
    } else if let Some(sum) = &tab.summary {
        let failures = failed + sum.requests_failed;
        let first = tab.results.iter().find(|r| r.failed()).map(failure_detail).unwrap_or_default();
        if sum.cancelled {
            result_banner(ui, Tone::Warn, icons::STOP, tr("Run cancelled"), "");
        } else if sum.bailed {
            result_banner(ui, Tone::Warn, icons::WARNING, tr("Stopped at the first failure"), &first);
        } else if failures == 0 && sum.tests_passed == 0 {
            result_banner(
                ui,
                Tone::Ok,
                icons::CHECK_CIRCLE,
                &trf("{} requests sent, no tests ran.", &[&sum.executed]),
                "",
            );
        } else if failures == 0 {
            let text = trf("All {} tests passed in {} requests.", &[&sum.tests_passed, &sum.executed]);
            result_banner(ui, Tone::Ok, icons::CHECK_CIRCLE, &text, "");
        } else {
            let head = if failures == 1 {
                trf("1 failure in {} requests.", &[&sum.executed])
            } else {
                trf("{} failures in {} requests.", &[&failures, &sum.executed])
            };
            result_banner(ui, Tone::Err, icons::WARNING_CIRCLE, &head, &first);
        }
    }

    // Filter, then the table and the details of the selected result.
    let n_failed = tab.results.iter().filter(|r| r.failed()).count();
    let n_all = tab.results.len();
    crate::flow_tab::row(ui, 26.0, |ui| {
        let all = format!("{} {n_all}", tr("All"));
        let passed = format!("{} {}", tr("Passed"), n_all - n_failed);
        let failed = format!("{} {n_failed}", tr("Failed"));
        kit::segmented_sm(
            ui,
            &mut tab.filter,
            &[(Filter::All, all.as_str()), (Filter::Passed, passed.as_str()), (Filter::Failed, failed.as_str())],
        );
        if tab.dropped > 0 {
            ui.label(kit::hint(
                ui,
                &trf(
                    "showing the latest {}; export for all {} results",
                    &[&KEEP_RESULTS, &(tab.dropped + tab.results.len())],
                ),
            ));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(kit::hint(ui, tr("Click a row for details")));
        });
    });
    let filtered: Vec<usize> = tab
        .results
        .iter()
        .enumerate()
        .filter(|(_, r)| match tab.filter {
            Filter::All => true,
            Filter::Passed => !r.failed(),
            Filter::Failed => r.failed(),
        })
        .map(|(i, _)| i)
        .collect();
    // Arrow keys move the selection when no text field has focus.
    if let Some(sel) = tab.selected
        && !ui.ctx().egui_wants_keyboard_input()
        && let Some(pos) = filtered.iter().position(|&i| i == sel)
    {
        let (down, up, esc) = ui.input(|i| {
            (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Escape))
        });
        if down && pos + 1 < filtered.len() {
            tab.selected = Some(filtered[pos + 1]);
            tab.scroll_to_selected = true;
        } else if up && pos > 0 {
            tab.selected = Some(filtered[pos - 1]);
            tab.scroll_to_selected = true;
        } else if esc {
            tab.selected = None;
        }
    }
    let rows_h = TABLE_HEAD + filtered.len().max(1) as f32 * TABLE_ROW + 2.0;
    let left = ui.available_height();
    let selected = tab.selected.and_then(|i| tab.results.get(i)).cloned();
    let table_h = if selected.is_some() {
        rows_h.min((left * 0.5).max(TABLE_HEAD + 3.0 * TABLE_ROW))
    } else {
        rows_h.min(left.max(TABLE_HEAD + 2.0 * TABLE_ROW))
    };
    results_table(ui, tab, &filtered, table_h, dark);
    if let Some(r) = selected {
        let h = ui.available_height().max(160.0);
        if result_detail(ui, &r, dark, h) {
            tab.selected = None;
        }
    }
}

/// What went wrong with a failed result, for the result banner.
fn failure_detail(r: &RequestResult) -> String {
    let it = r.iteration + 1;
    match (&r.error, r.status) {
        (Some(e), _) => trf("{} (iteration {}) failed: {}", &[&r.name, &it, &crate::dialogs::ellipsize(e, 80)]),
        (None, Some(code)) if code >= 400 => trf("{} (iteration {}) answered {}.", &[&r.name, &it, &code]),
        _ => trf("{} (iteration {}) failed.", &[&r.name, &it]),
    }
}

/// The results as a table: iteration, method, folder / request, tests,
/// status, time. Only the rows in view are laid out.
fn results_table(ui: &mut Ui, tab: &mut RunnerTab, filtered: &[usize], height: f32, dark: bool) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        results_table_rows(ui, tab, filtered, height, dark);
    });
    if filtered.is_empty() {
        ui.label(kit::hint(
            ui,
            if tab.is_running() { tr("Waiting for results…") } else { tr("No results match this filter.") },
        ));
    }
}

fn results_table_rows(ui: &mut Ui, tab: &mut RunnerTab, filtered: &[usize], height: f32, dark: bool) {
    let p = theme::pal(ui);
    let width = ui.available_width();
    let (tests_w, status_w, time_w) = (84.0, 92.0, 78.0);
    let left = ui.cursor().left();
    let right = left + width;
    let head = table_head(
        ui,
        &[
            (left + 12.0, false, tr("Request")),
            (right - time_w - status_w - tests_w + 12.0, false, tr("Tests")),
            (right - time_w - status_w + 12.0, false, tr("Status")),
            (right - 12.0, true, tr("Time")),
        ],
    );
    let mut clicked = None;
    let body_h = (height - TABLE_HEAD).max(TABLE_ROW);
    let mut area = egui::ScrollArea::vertical().id_salt("run-results").auto_shrink([false, true]).max_height(body_h);
    if std::mem::take(&mut tab.scroll_to_selected)
        && let Some(pos) = filtered.iter().position(|&i| Some(i) == tab.selected)
    {
        // The selected row in the middle of the table.
        area = area.vertical_scroll_offset((pos as f32 * TABLE_ROW - (body_h - TABLE_ROW) / 2.0).max(0.0));
    }
    let scroll = kit::show_rows(area, ui, TABLE_ROW, filtered.len(), |ui, range| {
        for &i in &filtered[range] {
            let r = &tab.results[i];
            let (rect, resp) = ui.allocate_exact_size(vec2(width, TABLE_ROW), Sense::click());
            let painter = ui.painter();
            if tab.selected == Some(i) {
                painter.rect_filled(rect.shrink2(vec2(1.0, 0.0)), 0.0, p.accent_soft);
            } else if resp.hovered() {
                painter.rect_filled(rect.shrink2(vec2(1.0, 0.0)), 0.0, p.well);
            }
            if filtered.first() != Some(&i) {
                painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
            }
            let cy = rect.center().y;
            // Iteration, method, folders / name.
            let mut x = rect.left() + 12.0;
            painter.text(
                pos2(x, cy),
                Align2::LEFT_CENTER,
                format!("#{}", r.iteration + 1),
                theme::mono(11.0),
                p.text_faint,
            );
            x += 22.0 + 8.0;
            let badge = painter.layout_no_wrap(
                widgets::method_short(&r.method).to_string(),
                theme::semibold(10.5),
                theme::method_color(&r.method, dark),
            );
            painter.galley(pos2(x, cy - badge.size().y / 2.0), badge, p.text);
            x += 34.0 + 8.0;
            // The folders give way first, so the name stays readable.
            let name_w = (right - time_w - status_w - tests_w - x - 8.0).max(40.0);
            let name = elide(ui, &r.name, theme::medium(12.5), p.text, name_w);
            let room = name_w - name.size().x - 8.0;
            if !r.path.is_empty() && room >= 40.0 {
                let folders =
                    elide(ui, &format!("{} /", r.path.join(" / ")), FontId::proportional(12.5), p.text_muted, room);
                let w = folders.size().x;
                ui.painter().galley(pos2(x, cy - folders.size().y / 2.0), folders, p.text_muted);
                x += w + 8.0;
            }
            ui.painter().galley(pos2(x, cy - name.size().y / 2.0), name, p.text);
            // Tests.
            let tests_x = right - time_w - status_w - tests_w + 12.0;
            if !r.tests.is_empty() {
                let failed = r.tests_failed();
                let passed = r.tests.iter().filter(|t| t.passed && !t.skipped).count();
                let (text, color) = if failed > 0 {
                    (format!("{} {failed}/{}", icons::X, passed + failed), p.err)
                } else {
                    (format!("{} {passed}", icons::CHECK), p.ok)
                };
                ui.painter().text(pos2(tests_x, cy), Align2::LEFT_CENTER, text, theme::medium(12.5), color);
            } else {
                ui.painter().text(
                    pos2(tests_x, cy),
                    Align2::LEFT_CENTER,
                    "–",
                    FontId::proportional(12.5),
                    p.text_faint,
                );
            }
            // Status.
            let status_x = right - time_w - status_w + 12.0;
            let (label, tone) = match (r.status, &r.error) {
                (_, Some(_)) => (tr("Error").to_string(), Tone::Err),
                (Some(code), None) => (code.to_string(), theme::status_tone(code)),
                _ => (tr("Skipped").to_string(), Tone::Neutral),
            };
            paint_pill(ui, pos2(status_x, cy), &label, tone);
            // Time.
            ui.painter().text(
                pos2(right - 12.0, cy),
                Align2::RIGHT_CENTER,
                fmt_ms(r.response_time_ms),
                theme::mono(12.5),
                p.text,
            );
            widgets::a11y_button(&resp, &format!("{} {}", r.method, r.name));
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    });
    let bottom = scroll.inner_rect.bottom().max(head.bottom());
    let table = Rect::from_min_max(head.min, pos2(head.right(), bottom));
    ui.painter().rect_stroke(
        table,
        CornerRadius::same(theme::RADIUS_CARD),
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    if let Some(i) = clicked {
        tab.selected = if tab.selected == Some(i) { None } else { Some(i) };
    }
}

/// Details of one result in a card: a 44 px header (method, name,
/// iteration, status, time, size, close), then the URL, the tests, the
/// headers and the body. Returns true when closed.
fn result_detail(ui: &mut Ui, r: &RequestResult, dark: bool, height: f32) -> bool {
    let p = theme::pal(ui);
    let mut close = false;
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_max_height(height - 2.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        kit::bar_ex(ui, 44.0, 0.0, true, |ui| {
            ui.add_space(14.0);
            ui.label(widgets::method_badge(&r.method, dark));
            ui.label(RichText::new(&r.name).font(theme::semibold(13.0)).color(p.text));
            ui.label(RichText::new(trf("iteration {}", &[&(r.iteration + 1)])).size(11.0).color(p.text_faint));
            match (r.status, &r.error) {
                (_, Some(_)) => {
                    kit::pill_ex(ui, tr("Error"), Tone::Err, kit::PillOpts { large: true, ..Default::default() });
                }
                (Some(code), None) => {
                    kit::status_pill(ui, code, &r.status_text);
                }
                _ => {
                    kit::pill_ex(ui, tr("Skipped"), Tone::Neutral, kit::PillOpts { large: true, ..Default::default() });
                }
            }
            ui.spacing_mut().item_spacing.x = 4.0;
            kit::metric(ui, icons::TIMER, &fmt_ms(r.response_time_ms));
            kit::metric(ui, icons::ARROW_LINE_DOWN, &ternarywire_core::util::human_bytes(r.response_size));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                close = kit::icon_button_sm(ui, icons::X, tr("Close details")).clicked();
                ui.add(
                    egui::Label::new(RichText::new(tr("↑↓ move · Esc closes")).size(11.0).color(p.text_faint))
                        .truncate(),
                );
            });
        });
        egui::ScrollArea::vertical().id_salt("run-detail").auto_shrink([false, true]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 10.0;
                egui::Frame::new()
                    .fill(p.well)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(RADIUS))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(RichText::new(&r.url).font(theme::mono(12.0)).color(p.text))
                                .selectable(true)
                                .truncate(),
                        );
                    });
                if let Some(e) = &r.error {
                    kit::banner(ui, Tone::Err, icons::WARNING_CIRCLE, e);
                }
                for e in &r.script_errors {
                    kit::banner(ui, Tone::Err, icons::WARNING_CIRCLE, &format!("{}: {}", e.location, e.message));
                }
                if !r.tests.is_empty() {
                    test_rows(ui, &r.tests);
                }
                if !r.response_headers.is_empty() {
                    let id = ui.make_persistent_id("run-headers");
                    if kit::disclosure(ui, id, tr("Headers"), Some(r.response_headers.len()), false) {
                        let rows: Vec<Vec<String>> =
                            r.response_headers.iter().map(|(k, v)| vec![k.clone(), v.clone()]).collect();
                        crate::request_tab::read_table(
                            ui,
                            "run-headers-table",
                            &[tr("Key"), tr("Value")],
                            &[0.32, 0.68],
                            &rows,
                        );
                    }
                }
                if let Some(b) = &r.body_preview {
                    let id = ui.make_persistent_id("run-body");
                    if kit::disclosure(ui, id, tr("Body"), None, true) {
                        let pretty = serde_json::from_str::<serde_json::Value>(b)
                            .ok()
                            .map(|v| crate::highlight::pretty_json_value(&v));
                        let lang =
                            if pretty.is_some() { crate::highlight::Lang::Json } else { crate::highlight::Lang::Text };
                        crate::flow_tab::code_view(ui, pretty.as_deref().unwrap_or(b), lang);
                    }
                }
            });
        });
    });
    close
}

/// The tests of a result as rows of a bordered list: icon, name (and the
/// assertion message under it), PASS / FAIL / SKIP on the right.
fn test_rows(ui: &mut Ui, tests: &[ternarywire_core::script::TestResult]) {
    let p = theme::pal(ui);
    let w = ui.available_width();
    let painter = ui.painter().clone();
    let rows: Vec<_> = tests
        .iter()
        .map(|t| {
            let (icon, tone, word) = if t.skipped {
                (icons::MINUS, Tone::Neutral, "SKIP")
            } else if t.passed {
                (icons::CHECK_CIRCLE, Tone::Ok, "PASS")
            } else {
                (icons::X_CIRCLE, Tone::Err, "FAIL")
            };
            let pill_w = painter.layout_no_wrap(word.to_string(), theme::medium(11.5), p.text).size().x + 14.0;
            let text_w = (w - 12.0 - 15.0 - 8.0 - 8.0 - pill_w - 12.0).max(40.0);
            let name = painter.layout(t.name.clone(), FontId::proportional(13.0), p.text, text_w);
            let err = t
                .error
                .as_ref()
                .filter(|_| !t.passed && !t.skipped)
                .map(|e| painter.layout(e.clone(), theme::mono(11.5), p.err, text_w));
            let h = match &err {
                Some(e) => (6.0 + name.size().y + e.size().y + 6.0).max(36.0),
                None => 36.0_f32.max(name.size().y + 12.0),
            };
            (icon, tone, word, pill_w, name, err, h)
        })
        .collect();
    let total = rows.iter().map(|r| r.6).sum::<f32>() + 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, total), Sense::hover());
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_CARD),
        p.surface,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    let mut y = rect.top() + 1.0;
    for (i, (icon, tone, word, pill_w, name, err, h)) in rows.into_iter().enumerate() {
        if i > 0 {
            painter.hline(rect.x_range(), y - 0.5, Stroke::new(1.0, p.border));
        }
        let (fg, _) = p.tone(tone);
        // One line: everything centered; with a message: aligned to the top.
        let line_c = if err.is_some() { y + 6.0 + name.size().y / 2.0 } else { y + h / 2.0 };
        let x = rect.left() + 12.0;
        painter.text(pos2(x + 7.5, line_c), Align2::CENTER_CENTER, icon, theme::icon_font(15.0), fg);
        let tx = x + 15.0 + 8.0;
        let name_h = name.size().y;
        painter.galley(pos2(tx, line_c - name_h / 2.0), name, p.text);
        if let Some(e) = err {
            painter.galley(pos2(tx, line_c + name_h / 2.0), e, p.err);
        }
        paint_pill(ui, pos2(rect.right() - 12.0 - pill_w, line_c), word, tone);
        y += h;
    }
}

// ---------------------------------------------------------------------------
// Results: performance
// ---------------------------------------------------------------------------

fn perf_results(ui: &mut Ui, tab: &mut RunnerTab) {
    let p = theme::pal(ui);
    if tab.perf_samples.is_empty() && tab.perf_report.is_none() && !tab.is_running() {
        kit::empty_state(
            ui,
            icons::GAUGE,
            tr("Load test"),
            tr("Set virtual users and duration, then start the test to measure throughput and latency."),
            |_| {},
        );
        return;
    }
    // Toolbar: run state, Export.
    let mut export_as = None;
    kit::toolbar(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if tab.perf_report.is_some() {
                export_as = export_menu(
                    ui,
                    tr("Export load test"),
                    250.0,
                    &[
                        ("html", icons::FILE_TEXT, tr("HTML report"), tr("chart + tables")),
                        ("csv", icons::TABLE, tr("CSV per second"), ".csv"),
                        ("json", icons::BRACKETS_CURLY, tr("JSON samples"), ".json"),
                    ],
                );
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let (vus, secs) = match &tab.perf_report {
                    Some(r) => (r.virtual_users, (r.duration_ms / 1000.0).round() as u64),
                    None => (tab.vus, tab.duration_s),
                };
                let mut hint = format!("{secs} s · {}", trf("{} virtual users", &[&vus]));
                if tab.ramp_s > 0 {
                    hint.push_str(&format!(" · {}", trf("{} s ramp-up", &[&tab.ramp_s])));
                }
                if tab.is_running() {
                    kit::pill_ex(
                        ui,
                        tr("Running"),
                        Tone::Accent,
                        kit::PillOpts { dot: true, live: true, ..Default::default() },
                    );
                    let at = tab.perf_samples.last().map(|s| s.second.min(tab.duration_s)).unwrap_or(0);
                    hint = format!("{at} / {} s · {}", tab.duration_s, trf("{} virtual users", &[&tab.vus]));
                } else if tab.perf_report.as_ref().is_some_and(|r| r.cancelled) {
                    kit::pill_ex(
                        ui,
                        tr("Stopped"),
                        Tone::Warn,
                        kit::PillOpts { icon: Some(icons::STOP), ..Default::default() },
                    );
                } else {
                    kit::pill_ex(
                        ui,
                        tr("Finished"),
                        Tone::Neutral,
                        kit::PillOpts { icon: Some(icons::CHECK), ..Default::default() },
                    );
                }
                ui.add(egui::Label::new(kit::hint(ui, &hint)).truncate());
            });
        });
    });
    if let Some(f) = export_as {
        let collection = tab.collection_name.clone();
        tab.note = export_note(export_perf(tab, &collection, f));
    }

    kit::pane_body(ui, "perf-results", |ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.add_space(4.0);
        if let Some(e) = tab.perf_report.as_ref().and_then(|r| r.error.as_ref()) {
            kit::banner(ui, Tone::Err, icons::WARNING_CIRCLE, &trf("The load test could not run: {}", &[e]));
        }
        let stats = PerfStats::of(tab);
        stat_grid(
            ui,
            4,
            &[
                (tr("Requests"), thousands(stats.total), None, trf("{} virtual users", &[&stats.users])),
                (tr("Throughput"), rate(stats.rps), Some(p.accent_text), tr("requests per second").into()),
                (
                    tr("Error rate"),
                    percent(stats.error_rate),
                    (stats.errors > 0).then_some(p.err),
                    trf("{} errors", &[&thousands(stats.errors)]),
                ),
                (
                    tr("Avg latency"),
                    fmt_ms(stats.avg_ms),
                    None,
                    stats.min_ms.map(|m| trf("min {}", &[&fmt_ms(m)])).unwrap_or_default(),
                ),
            ],
        );
        if tab.is_running() {
            let at = tab.perf_samples.last().map(|s| s.second).unwrap_or(0);
            progress_bar(ui, at as f32 / tab.duration_s.max(1) as f32);
        } else if tab.perf_report.as_ref().is_some_and(|r| r.cancelled) {
            kit::banner(
                ui,
                Tone::Warn,
                icons::STOP,
                tr("The test was stopped before the end; numbers cover the time it ran."),
            );
        }
        kit::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            crate::flow_tab::row(ui, 18.0, |ui| {
                ui.label(kit::section_title(tr("Throughput and latency")));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (color, label) in [(p.info, tr("Average latency")), (p.accent, tr("Requests per second"))] {
                        ui.label(RichText::new(label).size(12.0).color(p.text_muted));
                        kit::dot(ui, color, 4.0);
                        ui.add_space(4.0);
                    }
                });
            });
            timeline_chart(ui, tab.samples());
        });
        if let Some(r) = &tab.perf_report {
            let total = ui.available_width();
            let percentiles_card = |ui: &mut Ui| {
                kit::card(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(kit::section_title(tr("Latency percentiles")));
                    ui.add_space(12.0 - 6.0);
                    percentiles(ui, &r.latency);
                });
            };
            if total >= 500.0 {
                let gap = 12.0;
                let lw = ((total - gap) * 0.34).floor();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(vec2(lw, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(lw);
                        percentiles_card(ui);
                    });
                    let rw = ui.available_width();
                    ui.allocate_ui_with_layout(vec2(rw, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(rw);
                        per_request(ui, tab, r);
                    });
                });
            } else {
                percentiles_card(ui);
                per_request(ui, tab, r);
            }
        }
    });
}

/// The numbers of the cards of a load test: from the report, or from the
/// samples while it runs.
struct PerfStats {
    total: u64,
    users: usize,
    rps: f64,
    errors: u64,
    error_rate: f64,
    avg_ms: f64,
    min_ms: Option<f64>,
}

impl PerfStats {
    fn of(tab: &RunnerTab) -> Self {
        if let Some(r) = &tab.perf_report {
            return PerfStats {
                total: r.total,
                users: r.virtual_users,
                rps: r.throughput_rps,
                errors: r.errors,
                error_rate: r.error_rate,
                avg_ms: r.latency.avg_ms,
                min_ms: Some(r.latency.min_ms),
            };
        }
        let s = &tab.perf_samples;
        let total: u64 = s.iter().map(|x| x.requests).sum();
        let errors: u64 = s.iter().map(|x| x.errors).sum();
        let weighted: f64 = s.iter().map(|x| x.avg_latency_ms * x.requests as f64).sum();
        PerfStats {
            total,
            users: s.last().map(|x| x.active_vus).unwrap_or(tab.vus),
            rps: s.last().map(|x| x.rps).unwrap_or(0.0),
            errors,
            error_rate: if total > 0 { errors as f64 / total as f64 } else { 0.0 },
            avg_ms: if total > 0 { weighted / total as f64 } else { 0.0 },
            min_ms: None,
        }
    }
}

/// Requests per second: whole numbers from 100 up.
fn rate(rps: f64) -> String {
    if rps >= 100.0 { format!("{rps:.0}") } else { format!("{rps:.1}") }
}

/// A fraction as a percentage with one decimal (two below 0.1 %).
fn percent(f: f64) -> String {
    let v = f * 100.0;
    if v > 0.0 && v < 0.1 { format!("{v:.2}%") } else { format!("{v:.1}%") }
}

/// Latency percentiles as horizontal bars (p50 green, p90/p95 blue, p99
/// amber, max red).
fn percentiles(ui: &mut Ui, l: &ternarywire_runner::perf::LatencyStats) {
    let p = theme::pal(ui);
    let max = l.max_ms.max(0.001);
    ui.spacing_mut().item_spacing.y = 10.0;
    for (label, v, color) in [
        ("p50", l.p50_ms, p.ok),
        ("p90", l.p90_ms, p.info),
        ("p95", l.p95_ms, p.info),
        ("p99", l.p99_ms, p.warn),
        (tr("Max"), l.max_ms, p.err),
    ] {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
        let painter = ui.painter();
        let cy = rect.center().y;
        painter.text(pos2(rect.left(), cy), Align2::LEFT_CENTER, label, theme::medium(12.0), p.text_muted);
        let track = Rect::from_min_max(pos2(rect.left() + 44.0, cy - 4.0), pos2(rect.right() - 72.0, cy + 4.0));
        painter.rect_filled(track, CornerRadius::same(4), p.subtle);
        let w = (track.width() * ((v / max) as f32).max(0.04)).min(track.width());
        painter.rect_filled(Rect::from_min_size(track.min, vec2(w, track.height())), CornerRadius::same(4), color);
        painter.text(pos2(rect.right(), cy), Align2::RIGHT_CENTER, fmt_ms(v), theme::mono(12.0), p.text);
    }
}

/// The per-request numbers of a load test as a table.
fn per_request(ui: &mut Ui, tab: &RunnerTab, r: &PerfReport) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(kit::section_title(tr("Per request")));
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        let left = ui.cursor().left();
        let right = left + width;
        // Right edges of the number columns (Max, p95, Avg, Errors, Total).
        let edges = {
            let mut x = right - 8.0;
            let mut e = Vec::new();
            for w in [66.0, 60.0, 60.0, 52.0, 60.0] {
                e.push(x);
                x -= w;
            }
            e
        };
        let name_end = edges[4] - 60.0 + 8.0;
        let head = table_head(
            ui,
            &[
                (left + 8.0, false, tr("Request")),
                (edges[4], true, tr("Total")),
                (edges[3], true, tr("Errors")),
                (edges[2], true, tr("Avg")),
                (edges[1], true, "p95"),
                (edges[0], true, tr("Max")),
            ],
        );
        for (i, q) in r.per_request.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(width, TABLE_ROW), Sense::hover());
            let painter = ui.painter();
            if i > 0 {
                painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
            }
            let cy = rect.center().y;
            let mut x = rect.left() + 8.0;
            if let Some(badge) = tab.badge_of(&q.request_id) {
                let g = painter.layout_no_wrap(
                    widgets::method_short(badge).to_string(),
                    theme::semibold(10.5),
                    theme::method_color(badge, dark),
                );
                painter.galley(pos2(x, cy - g.size().y / 2.0), g, p.text);
                x += 34.0 + 8.0;
            }
            let g = elide(ui, &q.name, FontId::proportional(12.0), p.text, (name_end - x - 8.0).max(20.0));
            ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, p.text);
            let cells = [
                (thousands(q.total), p.text),
                (thousands(q.errors), if q.errors > 0 { p.err } else { p.text }),
                (fmt_ms(q.latency.avg_ms), p.text),
                (fmt_ms(q.latency.p95_ms), p.text),
                (fmt_ms(q.latency.max_ms), p.text),
            ];
            for (j, (text, color)) in cells.iter().enumerate() {
                ui.painter().text(pos2(edges[4 - j], cy), Align2::RIGHT_CENTER, text, theme::mono(12.0), *color);
            }
        }
        let bottom = ui.cursor().top();
        let table = Rect::from_min_max(head.min, pos2(head.right(), bottom.max(head.bottom())));
        ui.painter().rect_stroke(
            table,
            CornerRadius::same(theme::RADIUS_CARD),
            Stroke::new(1.0, p.border),
            StrokeKind::Inside,
        );
    });
}

/// A "nice" axis top for four grid steps (1, 2, 2.5, 3, 4, 5, 6 or 8 × 10ⁿ each).
fn nice_max(v: f64) -> f64 {
    let raw = (v / 4.0).max(1e-6);
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw * 0.999)
        .unwrap_or(10.0 * mag);
    step * 4.0
}

/// Requests per second (filled area and line, left axis) and average
/// latency (line, right axis) per second, with grid lines, both axes, and a
/// hover marker with a tooltip.
fn timeline_chart(ui: &mut Ui, samples: &[PerfSample]) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let w = ui.available_width();
    let h = (w * 226.0 / 700.0).clamp(180.0, 240.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let plot = Rect::from_min_max(rect.min + vec2(46.0, 14.0), rect.max - vec2(46.0, 30.0));
    let painter = ui.painter_at(rect.expand(2.0));
    let max_rps = nice_max(samples.iter().map(|s| s.rps).fold(1.0, f64::max));
    let max_lat = nice_max(samples.iter().map(|s| s.avg_latency_ms).fold(0.1, f64::max));
    let small = FontId::proportional(11.0);
    for k in 0..=4 {
        let f = k as f64 / 4.0;
        let y = plot.bottom() - plot.height() * f as f32;
        painter.hline(plot.x_range(), y, Stroke::new(1.0, p.border));
        let rps = max_rps * f;
        let left = if max_rps >= 4.0 { format!("{rps:.0}") } else { format!("{rps:.1}") };
        painter.text(pos2(plot.left() - 8.0, y), Align2::RIGHT_CENTER, left, small.clone(), p.text_faint);
        let lat = if k == 0 { "0 ms".to_string() } else { fmt_ms(max_lat * f) };
        painter.text(pos2(plot.right() + 8.0, y), Align2::LEFT_CENTER, lat, small.clone(), p.text_faint);
    }
    if samples.len() < 2 {
        painter.text(
            plot.center(),
            Align2::CENTER_CENTER,
            tr("Collecting samples…"),
            FontId::proportional(12.0),
            p.text_muted,
        );
        return;
    }
    let n = (samples.len() - 1) as f32;
    let x_of = |i: usize| plot.left() + plot.width() * i as f32 / n;
    let y_rps = |v: f64| plot.bottom() - plot.height() * (v / max_rps).min(1.0) as f32;
    let y_lat = |v: f64| plot.bottom() - plot.height() * (v / max_lat).min(1.0) as f32;
    let rps: Vec<egui::Pos2> = samples.iter().enumerate().map(|(i, s)| pos2(x_of(i), y_rps(s.rps))).collect();
    let lat: Vec<egui::Pos2> =
        samples.iter().enumerate().map(|(i, s)| pos2(x_of(i), y_lat(s.avg_latency_ms))).collect();
    // The area under the throughput line, as one mesh (no seams).
    let fill = theme::soft(p.accent, if dark { 0.22 } else { 0.16 });
    let mut mesh = egui::Mesh::default();
    for (i, q) in rps.iter().enumerate() {
        mesh.colored_vertex(*q, fill);
        mesh.colored_vertex(pos2(q.x, plot.bottom()), fill);
        if i > 0 {
            let b = (i * 2) as u32;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    painter.add(mesh);
    painter.add(egui::Shape::line(rps.clone(), Stroke::new(2.0, p.accent)));
    painter.add(egui::Shape::line(lat.clone(), Stroke::new(2.0, p.info)));
    // Time axis.
    let last = samples.len() - 1;
    let y = rect.bottom() - 6.0;
    painter.text(
        pos2(plot.left(), y),
        Align2::LEFT_BOTTOM,
        format!("{} s", samples[0].second),
        small.clone(),
        p.text_faint,
    );
    painter.text(
        pos2(plot.right(), y),
        Align2::RIGHT_BOTTOM,
        format!("{} s", samples[last].second),
        small,
        p.text_faint,
    );
    // Hover: a dashed line at the second under the pointer, its points and
    // a tooltip with the numbers.
    if let Some(pos) = resp.hover_pos()
        && (plot.left() - 4.0..=plot.right() + 4.0).contains(&pos.x)
    {
        let i = (((pos.x - plot.left()) / plot.width()) * n).round().clamp(0.0, n) as usize;
        let s = &samples[i];
        let x = x_of(i);
        painter.extend(egui::Shape::dashed_line(
            &[pos2(x, plot.top()), pos2(x, plot.bottom())],
            Stroke::new(1.0, p.text_faint),
            3.0,
            3.0,
        ));
        for (q, c) in [(rps[i], p.accent), (lat[i], p.info)] {
            painter.circle(q, 4.0, c, Stroke::new(2.0, p.surface));
        }
        let lines = [
            format!("{:.0} req/s", s.rps),
            format!("{} {}", fmt_ms(s.avg_latency_ms), tr("avg")),
            format!("{} {}", s.active_vus, tr("users")),
        ];
        let tw = lines
            .iter()
            .map(|l| ui.painter().layout_no_wrap(l.clone(), FontId::proportional(11.5), p.text).size().x)
            .fold(0.0, f32::max)
            + 32.0;
        let size = vec2(tw.max(110.0), 74.0);
        let mut tx = x + 12.0;
        if tx + size.x > rect.right() {
            tx = x - 12.0 - size.x;
        }
        let tip = Rect::from_min_size(pos2(tx, plot.top() + 6.0), size);
        let tp = ui
            .painter()
            .clone()
            .with_layer_id(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("perf-tip")));
        tp.rect(tip, CornerRadius::same(RADIUS), p.surface, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
        tp.text(
            pos2(tip.left() + 10.0, tip.top() + 9.0),
            Align2::LEFT_TOP,
            format!("{} s", s.second),
            theme::semibold(11.5),
            p.text,
        );
        for (k, (c, text)) in
            [(Some(p.accent), &lines[0]), (Some(p.info), &lines[1]), (None, &lines[2])].iter().enumerate()
        {
            let cy = tip.top() + 32.0 + k as f32 * 16.0;
            if let Some(c) = c {
                tp.circle_filled(pos2(tip.left() + 13.0, cy), 3.5, *c);
                tp.text(
                    pos2(tip.left() + 22.0, cy),
                    Align2::LEFT_CENTER,
                    text.as_str(),
                    FontId::proportional(11.5),
                    p.text,
                );
            } else {
                tp.text(
                    pos2(tip.left() + 10.0, cy),
                    Align2::LEFT_CENTER,
                    text.as_str(),
                    FontId::proportional(11.0),
                    p.text_muted,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Load test export
// ---------------------------------------------------------------------------

/// What an exported load test says about how it ran.
struct PerfMeta<'a> {
    collection: &'a str,
    ramp_up_s: u64,
    /// Method badge by request id.
    methods: HashMap<String, String>,
}

/// Writes the load test as `format` (html, csv, json) to a file the user
/// picks. `Ok(None)`: the dialog was cancelled.
fn export_perf(tab: &RunnerTab, collection: &str, format: &str) -> Result<Option<PathBuf>, String> {
    let Some(report) = &tab.perf_report else { return Err("nothing to export".into()) };
    let meta = PerfMeta {
        collection,
        ramp_up_s: tab.ramp_s,
        methods: tab.plan.iter().map(|p| (p.0.clone(), p.2.clone())).collect(),
    };
    let samples = tab.samples();
    let Some(path) = rfd::FileDialog::new().set_file_name(format!("ternarywire-load-test.{format}")).save_file() else {
        return Ok(None);
    };
    let text = match format {
        "csv" => perf_csv(&meta, report, samples),
        "html" => perf_html(&meta, report, samples, theme::palette(false)),
        _ => perf_json(&meta, report, samples),
    };
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

fn perf_json(meta: &PerfMeta, r: &PerfReport, samples: &[PerfSample]) -> String {
    let per_request: Vec<serde_json::Value> = r
        .per_request
        .iter()
        .map(|q| {
            serde_json::json!({
                "request_id": q.request_id,
                "name": q.name,
                "method": meta.methods.get(&q.request_id),
                "total": q.total,
                "errors": q.errors,
                "latency_ms": q.latency,
            })
        })
        .collect();
    let v = serde_json::json!({
        "collection": meta.collection,
        "summary": {
            "virtual_users": r.virtual_users,
            "ramp_up_s": meta.ramp_up_s,
            "duration_ms": r.duration_ms,
            "requests": r.total,
            "errors": r.errors,
            "error_rate": r.error_rate,
            "tests_failed": r.tests_failed,
            "throughput_rps": r.throughput_rps,
            "latency_ms": r.latency,
            "cancelled": r.cancelled,
            "error": r.error,
        },
        "samples": samples,
        "per_request": per_request,
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// A CSV field, quoted when it holds a comma, a quote or a line break.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// Three tables, each under a title line and separated by a blank line:
/// the summary, the per-second samples and the per-request numbers.
fn perf_csv(meta: &PerfMeta, r: &PerfReport, samples: &[PerfSample]) -> String {
    let l = &r.latency;
    let mut out = String::from("Summary\nmetric,value\n");
    let rows: Vec<(&str, String)> = vec![
        ("collection", meta.collection.to_string()),
        ("virtual_users", r.virtual_users.to_string()),
        ("ramp_up_s", meta.ramp_up_s.to_string()),
        ("duration_ms", format!("{:.0}", r.duration_ms)),
        ("requests", r.total.to_string()),
        ("errors", r.errors.to_string()),
        ("error_rate", format!("{:.6}", r.error_rate)),
        ("tests_failed", r.tests_failed.to_string()),
        ("throughput_rps", format!("{:.3}", r.throughput_rps)),
        ("latency_min_ms", format!("{:.3}", l.min_ms)),
        ("latency_avg_ms", format!("{:.3}", l.avg_ms)),
        ("latency_p50_ms", format!("{:.3}", l.p50_ms)),
        ("latency_p90_ms", format!("{:.3}", l.p90_ms)),
        ("latency_p95_ms", format!("{:.3}", l.p95_ms)),
        ("latency_p99_ms", format!("{:.3}", l.p99_ms)),
        ("latency_max_ms", format!("{:.3}", l.max_ms)),
        ("cancelled", r.cancelled.to_string()),
    ];
    for (k, v) in rows {
        out.push_str(&format!("{k},{}\n", csv_field(&v)));
    }
    out.push_str("\nPer second\nsecond,requests,requests_per_second,avg_latency_ms,errors,active_users\n");
    for s in samples {
        out.push_str(&format!(
            "{},{},{:.3},{:.3},{},{}\n",
            s.second, s.requests, s.rps, s.avg_latency_ms, s.errors, s.active_vus
        ));
    }
    out.push_str("\nPer request\nrequest,method,total,errors,avg_ms,p50_ms,p90_ms,p95_ms,p99_ms,max_ms\n");
    for q in &r.per_request {
        let m = meta.methods.get(&q.request_id).map(String::as_str).unwrap_or("");
        let l = &q.latency;
        out.push_str(&format!(
            "{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}\n",
            csv_field(&q.name),
            csv_field(m),
            q.total,
            q.errors,
            l.avg_ms,
            l.p50_ms,
            l.p90_ms,
            l.p95_ms,
            l.p99_ms,
            l.max_ms
        ));
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn css(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// A self-contained report: the numbers, the chart as SVG, the
/// percentiles, the per-request table and the per-second samples.
fn perf_html(meta: &PerfMeta, r: &PerfReport, samples: &[PerfSample], p: theme::Palette) -> String {
    let (w, h, l, rt, t, b) = (720.0_f64, 240.0_f64, 52.0, 52.0, 14.0, 30.0);
    let (pw, ph) = (w - l - rt, h - t - b);
    let max_rps = nice_max(samples.iter().map(|s| s.rps).fold(1.0, f64::max));
    let max_lat = nice_max(samples.iter().map(|s| s.avg_latency_ms).fold(0.1, f64::max));
    let n = samples.len().saturating_sub(1).max(1) as f64;
    let x = |i: usize| l + pw * i as f64 / n;
    let mut svg = format!(r#"<svg viewBox="0 0 {w} {h}" width="100%" role="img" aria-label="Throughput and latency">"#);
    for k in 0..=4 {
        let f = k as f64 / 4.0;
        let y = t + ph - ph * f;
        svg.push_str(&format!(
            r#"<line x1="{l}" x2="{}" y1="{y}" y2="{y}" stroke="{}"/><text x="{}" y="{}" text-anchor="end" class="ax">{:.0}</text><text x="{}" y="{}" class="ax">{}</text>"#,
            w - rt,
            css(p.border),
            l - 8.0,
            y + 4.0,
            max_rps * f,
            w - rt + 8.0,
            y + 4.0,
            html_escape(&fmt_ms(max_lat * f))
        ));
    }
    if samples.len() >= 2 {
        let pts = |f: &dyn Fn(&PerfSample) -> f64, max: f64| {
            samples
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{:.1},{:.1}", x(i), t + ph - ph * (f(s) / max).min(1.0)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let rps = pts(&|s| s.rps, max_rps);
        let lat = pts(&|s| s.avg_latency_ms, max_lat);
        svg.push_str(&format!(
            r#"<polygon points="{:.1},{:.1} {rps} {:.1},{:.1}" fill="{}" fill-opacity=".16"/><polyline points="{rps}" fill="none" stroke="{}" stroke-width="2"/><polyline points="{lat}" fill="none" stroke="{}" stroke-width="2"/>"#,
            x(0),
            t + ph,
            x(samples.len() - 1),
            t + ph,
            css(p.accent),
            css(p.accent),
            css(p.info)
        ));
        svg.push_str(&format!(
            r#"<text x="{l}" y="{}" class="ax">{} s</text><text x="{}" y="{}" text-anchor="end" class="ax">{} s</text>"#,
            h - 8.0,
            samples[0].second,
            w - rt,
            h - 8.0,
            samples[samples.len() - 1].second
        ));
    }
    svg.push_str("</svg>");
    let lat = &r.latency;
    let stat = |label: &str, value: String, sub: String| {
        format!(
            r#"<div class="stat"><div class="l">{}</div><div class="v">{}</div><div class="s">{}</div></div>"#,
            html_escape(label),
            html_escape(&value),
            html_escape(&sub)
        )
    };
    let stats = [
        stat(tr("Requests"), thousands(r.total), trf("{} virtual users", &[&r.virtual_users])),
        stat(tr("Throughput"), rate(r.throughput_rps), tr("requests per second").into()),
        stat(tr("Error rate"), percent(r.error_rate), trf("{} errors", &[&thousands(r.errors)])),
        stat(tr("Avg latency"), fmt_ms(lat.avg_ms), trf("min {}", &[&fmt_ms(lat.min_ms)])),
    ]
    .join("");
    let pct_rows: String =
        [("p50", lat.p50_ms), ("p90", lat.p90_ms), ("p95", lat.p95_ms), ("p99", lat.p99_ms), (tr("Max"), lat.max_ms)]
            .iter()
            .map(|(k, v)| {
                format!("<tr><td>{}</td><td class=\"n\">{}</td></tr>", html_escape(k), html_escape(&fmt_ms(*v)))
            })
            .collect();
    let req_rows: String = r
        .per_request
        .iter()
        .map(|q| {
            format!(
                "<tr><td>{} {}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                html_escape(meta.methods.get(&q.request_id).map(String::as_str).unwrap_or("")),
                html_escape(&q.name),
                thousands(q.total),
                thousands(q.errors),
                html_escape(&fmt_ms(q.latency.avg_ms)),
                html_escape(&fmt_ms(q.latency.p95_ms)),
                html_escape(&fmt_ms(q.latency.max_ms))
            )
        })
        .collect();
    let sample_rows: String = samples
        .iter()
        .map(|s| {
            format!(
                "<tr><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{:.1}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                s.second,
                s.requests,
                s.rps,
                html_escape(&fmt_ms(s.avg_latency_ms)),
                s.errors,
                s.active_vus
            )
        })
        .collect();
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title><style>
body{{font:13px/1.45 system-ui,sans-serif;color:{text};background:{well};margin:0;padding:24px}}
h1{{font-size:18px;margin:0 0 4px}} h2{{font-size:13px;margin:24px 0 8px}} .sub{{color:{muted}}}
.grid{{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-top:16px}}
.stat,.card{{background:{surface};border:1px solid {border};border-radius:9px;padding:12px 14px}}
.stat .l,.stat .s{{font-size:11.5px;color:{muted}}} .stat .v{{font-size:22px;font-weight:600}}
.card{{margin-top:12px}} .ax{{font-size:11px;fill:{faint}}}
table{{border-collapse:collapse;width:100%;background:{surface};border:1px solid {border}}}
th,td{{padding:6px 10px;border-bottom:1px solid {border};text-align:left}} th{{background:{well};color:{muted};font-weight:500}}
.n{{text-align:right;font-variant-numeric:tabular-nums}}
</style></head><body>
<h1>{title}</h1><div class="sub">{collection} · {users} · {secs} s · {ramp} s ramp-up</div>
<div class="grid">{stats}</div>
<div class="card"><b>{chart_title}</b>{svg}</div>
<h2>{pct_title}</h2><table><tbody>{pct_rows}</tbody></table>
<h2>{req_title}</h2><table><thead><tr><th>{req}</th><th class="n">{total}</th><th class="n">{errors}</th><th class="n">{avg}</th><th class="n">p95</th><th class="n">{max}</th></tr></thead><tbody>{req_rows}</tbody></table>
<h2>{sec_title}</h2><table><thead><tr><th class="n">s</th><th class="n">{total}</th><th class="n">req/s</th><th class="n">{avg}</th><th class="n">{errors}</th><th class="n">{users_h}</th></tr></thead><tbody>{sample_rows}</tbody></table>
</body></html>"#,
        title = html_escape(tr("Load test")),
        text = css(p.text),
        well = css(p.well),
        muted = css(p.text_muted),
        faint = css(p.text_faint),
        surface = css(p.surface),
        border = css(p.border),
        collection = html_escape(meta.collection),
        users = html_escape(&trf("{} virtual users", &[&r.virtual_users])),
        secs = (r.duration_ms / 1000.0).round(),
        ramp = meta.ramp_up_s,
        chart_title = html_escape(tr("Throughput and latency")),
        pct_title = html_escape(tr("Latency percentiles")),
        req_title = html_escape(tr("Per request")),
        sec_title = html_escape(tr("Per second")),
        req = html_escape(tr("Request")),
        total = html_escape(tr("Total")),
        errors = html_escape(tr("Errors")),
        avg = html_escape(tr("Avg")),
        max = html_escape(tr("Max")),
        users_h = html_escape(tr("users")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The run order follows the collection: a request added after the
    /// runner opened is in it, a deleted one is gone; the user's order and
    /// selection stay.
    #[test]
    fn plan_follows_collection_edits() {
        use ternarywire_core::model::{HttpRequest, Item, RequestItem};
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let cid = app.new_collection("C");
        let add = |app: &mut TernaryWireApp, name: &str| {
            let r = RequestItem::new(name, HttpRequest::new("GET", "http://x.test"));
            let id = r.id.clone();
            app.ws.data.collection_mut(&cid).unwrap().items.push(Item::Request(r));
            app.ws.mark_collection(&cid);
            id
        };
        add(&mut app, "a");
        let b = add(&mut app, "b");
        add(&mut app, "d");
        let mut tab = RunnerTab::new(&app.ws, cid.clone(), None);
        tab.plan.swap(0, 2);
        tab.plan[0].3 = false;
        add(&mut app, "c");
        app.ws.data.collection_mut(&cid).unwrap().remove(&b);
        app.ws.mark_collection(&cid);
        tab.refresh_plan(&app.ws);
        let plan: Vec<(&str, bool)> = tab.plan.iter().map(|p| (p.1.as_str(), p.3)).collect();
        assert_eq!(plan, [("d", false), ("a", true), ("c", true)]);
        let name = app.ws.data.collection_mut(&cid).unwrap().items.iter_mut().find_map(|i| match i {
            Item::Request(r) if r.name == "a" => Some(&mut r.name),
            _ => None,
        });
        *name.unwrap() = "renamed".into();
        app.ws.mark_collection(&cid);
        tab.refresh_plan(&app.ws);
        assert_eq!(tab.plan[1].1, "renamed");
    }

    fn result(name: String) -> RunEvent {
        RunEvent::RequestFinished(Box::new(RequestResult { name, ..Default::default() }))
    }

    /// Only the latest results are kept; the selected one must stay selected
    /// while older ones are dropped from the front.
    #[test]
    fn selection_follows_results_when_old_ones_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let ws = WsState::for_test(dir.path());
        let mut tab = RunnerTab::new(&ws, "none".into(), None);
        for i in 0..KEEP_RESULTS {
            tab.on_event(result(format!("r{i}")));
        }
        tab.selected = Some(5);
        tab.on_event(result("next".into()));
        assert_eq!(tab.selected.and_then(|i| tab.results.get(i)).map(|r| r.name.as_str()), Some("r5"));
        tab.selected = Some(0);
        tab.on_event(result("next2".into()));
        assert_eq!(tab.selected, None);
    }

    fn report() -> (PerfReport, Vec<PerfSample>) {
        use ternarywire_runner::perf::{LatencyStats, RequestStats};
        let samples: Vec<PerfSample> = (1..=3)
            .map(|s| PerfSample {
                second: s,
                requests: 100 * s,
                rps: 100.0 * s as f64,
                avg_latency_ms: 10.0 + s as f64,
                errors: s - 1,
                active_vus: 5,
            })
            .collect();
        let latency = LatencyStats {
            min_ms: 2.0,
            max_ms: 90.0,
            avg_ms: 12.0,
            p50_ms: 10.0,
            p90_ms: 20.0,
            p95_ms: 30.0,
            p99_ms: 60.0,
        };
        let report = PerfReport {
            virtual_users: 5,
            duration_ms: 3000.0,
            total: 600,
            errors: 3,
            error_rate: 0.005,
            throughput_rps: 200.0,
            latency: latency.clone(),
            per_request: vec![RequestStats {
                request_id: "r1".into(),
                name: "List, \"all\" books".into(),
                total: 600,
                errors: 3,
                latency,
            }],
            timeline: samples.clone(),
            ..Default::default()
        };
        (report, samples)
    }

    /// The load test export writes the summary, the per-second samples and
    /// the per-request rows in every format.
    #[test]
    fn perf_export_has_summary_samples_and_requests() {
        let (r, samples) = report();
        let meta = PerfMeta {
            collection: "Bookstore",
            ramp_up_s: 2,
            methods: [("r1".to_string(), "GET".to_string())].into_iter().collect(),
        };
        let json: serde_json::Value = serde_json::from_str(&perf_json(&meta, &r, &samples)).unwrap();
        assert_eq!(json["summary"]["requests"], 600);
        assert_eq!(json["summary"]["ramp_up_s"], 2);
        assert_eq!(json["samples"].as_array().unwrap().len(), 3);
        assert_eq!(json["samples"][2]["rps"], 300.0);
        assert_eq!(json["per_request"][0]["method"], "GET");
        assert_eq!(json["per_request"][0]["latency_ms"]["p95_ms"], 30.0);

        let csv = perf_csv(&meta, &r, &samples);
        let sections: Vec<&str> = csv.split("\n\n").collect();
        assert_eq!(sections.len(), 3, "{csv}");
        assert!(sections[0].starts_with("Summary\nmetric,value\n") && sections[0].contains("requests,600"));
        assert_eq!(sections[1].lines().count(), 2 + 3);
        assert!(sections[1].contains("\n3,300,300.000,13.000,2,5"), "{}", sections[1]);
        assert!(sections[2].contains("\"List, \"\"all\"\" books\",GET,600,3,"), "{}", sections[2]);

        let html = perf_html(&meta, &r, &samples, theme::palette(false));
        assert!(html.contains("<svg") && html.contains("<polyline"));
        assert!(html.contains("List, &quot;all&quot; books"));
        assert_eq!(html.matches("<tr><td class=\"n\">").count(), 3, "a row per second");
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(thousands(18342), "18,342");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(nice_max(650.0), 800.0);
        assert_eq!(nice_max(57.0), 80.0);
        assert_eq!(nice_max(1.0), 1.0);
        assert_eq!(nice_max(1030.0), 1200.0);
        assert_eq!(rate(611.4), "611");
        assert_eq!(rate(12.34), "12.3");
        assert_eq!(percent(0.00398), "0.4%");
        assert_eq!(percent(0.0004), "0.04%");
    }
}
