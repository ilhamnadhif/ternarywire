//! Flows canvas (FR-D4): blocks connected by edges, edited visually, run
//! with live per-block status, saved to `flows/<name>.flow.json`.
//!
//! The tab has a 46 px title row (breadcrumb, save state, zoom, Inspector,
//! Fit / Validate / Add block / Run), the canvas (dot grid, block cards with
//! ports, bezier connections, a floating zoom capsule), the inspector on the
//! right at the full height of the view (flow settings, or the selected block
//! or connection as sections) and the run log under the canvas.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Key, Layout, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use ternarywire_core::model::RequestKind;
use ternarywire_core::protocols::AiProvider;
use ternarywire_core::util::human_ms;
use ternarywire_flows::{
    Block, BlockConfig, BlockKind, CompareOp, ConditionMode, Diagnostic, Edge, Flow, FlowEnv, FlowEvent, FlowInput,
    FlowResult, FlowRunOptions, ItemRef, LogLevelConfig, LoopMode, PORT_BRANCHES, RequestSource, Severity,
    TemplateMode, VariableScope,
};

use crate::app::{Msg, TabKind, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::{Tone, size};
use crate::theme::{self, Palette, RADIUS, RADIUS_SM};
use crate::widgets::{self, KvOptions};
use crate::{icons, kit};

/// Block card layout in canvas units (screen pixels at 100 %).
const BLOCK_W: f32 = 172.0;
/// Tinted header with the kind icon, the name and the run badge (its bottom
/// hairline included).
const HEAD_H: f32 = 30.0;
/// The summary line under the header (its bottom hairline included).
const SUMMARY_H: f32 = 24.0;
/// Top of the first port row: border, header, summary and 4 px padding.
const PORTS_TOP: f32 = 1.0 + HEAD_H + SUMMARY_H + 4.0;
const PORT_ROW: f32 = 20.0;
/// Snap grid: moved blocks snap to half of it; the dot grid uses it too.
const GRID: f32 = 20.0;
/// Default width of the inspector and height of the run log.
const INSPECTOR_W: f32 = 324.0;
const LOG_H: f32 = 196.0;
/// Rows of the run log.
const LOG_ROW: f32 = 22.0;
const MIN_ZOOM: f32 = 0.3;
const MAX_ZOOM: f32 = 2.0;
/// "Fit" never zooms out further than this, so blocks stay readable; a flow
/// too wide for the canvas is shown from its start instead.
const FIT_MIN_ZOOM: f32 = 0.7;
/// Below this zoom, port labels and summaries are left out.
const DETAIL_ZOOM: f32 = 0.55;

pub enum FlowMsg {
    Event(FlowEvent),
    Done(Box<FlowResult>),
}

#[derive(Clone, PartialEq)]
enum Selection {
    Block(String),
    Edge(usize),
}

#[derive(Clone)]
enum BlockStatus {
    Running { since: Instant },
    Done { port: String, failed: bool, ms: f64, output: Arc<Value>, error: Option<String>, tests: (usize, usize) },
}

/// The answer of a streaming AI block so far.
struct LiveText {
    block: String,
    text: String,
    /// Run log line that grows with it.
    log: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogKind {
    /// A block finished normally.
    Step,
    Info,
    Debug,
    Warn,
    Error,
    /// The run succeeded.
    Done,
}

struct LogLine {
    /// Time since the run started.
    at: Duration,
    kind: LogKind,
    /// Display name of the block the line is about.
    block: Option<String>,
    text: String,
}

#[derive(Default)]
struct RunState {
    /// Number of the run (see [`crate::app::next_epoch`]); a stopped run
    /// still sends events, which must not touch the next one.
    epoch: u64,
    cancel: Option<CancellationToken>,
    started: Option<Instant>,
    status: HashMap<String, BlockStatus>,
    log: Vec<LogLine>,
    result: Option<Box<FlowResult>>,
    /// Steps still running → block id (parallel branches run several blocks
    /// at once, sometimes the same block twice).
    running: HashMap<usize, String>,
    /// Streamed AI text by step.
    live: HashMap<usize, LiveText>,
}

impl RunState {
    fn live_text(&self, block: &str) -> Option<&str> {
        self.live.values().filter(|l| l.block == block).map(|l| l.text.as_str()).next()
    }

    fn push(&mut self, kind: LogKind, block: Option<String>, text: impl Into<String>) -> usize {
        let at = self.started.map(|s| s.elapsed()).unwrap_or_default();
        self.log.push(LogLine { at, kind, block, text: text.into() });
        self.log.len() - 1
    }
}

pub struct FlowTab {
    pub flow: Flow,
    pub path: Option<PathBuf>,
    saved: Flow,
    last_edit: Option<Instant>,
    selected: Option<Selection>,
    connecting: Option<(String, String)>,
    pan: Vec2,
    zoom: f32,
    run: RunState,
    diagnostics: Vec<Diagnostic>,
    inputs: HashMap<String, String>,
    /// Defaults of the flow inputs as typed, by input, with the value each
    /// was read as (see [`default_text`]).
    typed_defaults: HashMap<usize, (String, Value)>,
    error: Option<String>,
    /// Automation: run on the next frame.
    pub auto_run: bool,
    /// Fit all blocks into view on the next frame.
    fit: bool,
    /// Size of the canvas when last drawn (new blocks go to its middle).
    view: Vec2,
    /// The user zoomed or panned since the last fit (no refit on resize).
    moved_view: bool,
    /// Inspector shown; `None` decides by the width of the tab.
    inspector: Option<bool>,
    /// Width of the inspector and height of the run log (both resizable by
    /// their edge; a double click resets them).
    inspector_w: f32,
    log_h: f32,
    /// The flow name being edited in the header.
    renaming: Option<String>,
    /// Canvas position of the last right click ("Add block here").
    menu_at: Option<(f32, f32)>,
    /// Automation: `validate`, `add-block`, `fit` or `view:<zoom>:<x>:<y>`
    /// on the next frame.
    pub auto: Option<String>,
    /// Open the Add block menu on the next frame.
    open_add_menu: bool,
}

impl FlowTab {
    pub fn new(flow: Flow, path: Option<PathBuf>) -> Self {
        FlowTab {
            saved: flow.clone(),
            flow,
            path,
            last_edit: None,
            selected: None,
            connecting: None,
            pan: Vec2::new(40.0, 40.0),
            zoom: 1.0,
            run: RunState::default(),
            diagnostics: Vec::new(),
            inputs: HashMap::new(),
            typed_defaults: HashMap::new(),
            error: None,
            auto_run: false,
            fit: true,
            view: Vec2::ZERO,
            moved_view: false,
            inspector: None,
            inspector_w: INSPECTOR_W,
            log_h: LOG_H,
            renaming: None,
            menu_at: None,
            auto: None,
            open_add_menu: false,
        }
    }

    /// A new flow with a Start block.
    pub fn blank(name: &str) -> Self {
        let mut flow = Flow::new(name);
        if flow.start().is_none() {
            flow.add_block(Block::new(BlockKind::Start.default_config(), tr("Start"), (0.0, 120.0)));
        }
        let mut t = FlowTab::new(flow, None);
        t.last_edit = Some(Instant::now());
        t
    }

    pub fn dirty(&self) -> bool {
        self.flow != self.saved
    }

    pub fn is_running(&self) -> bool {
        self.run.cancel.is_some()
    }

    /// The number of the latest run; events carry the number of their run.
    pub fn run_epoch(&self) -> u64 {
        self.run.epoch
    }

    pub fn stop(&mut self) {
        if let Some(c) = self.run.cancel.take() {
            c.cancel();
        }
    }

    fn label_of(&self, block: &str) -> String {
        self.flow.block(block).map(|b| b.display_name().to_string()).unwrap_or_else(|| block.to_string())
    }

    pub fn on_msg(&mut self, m: FlowMsg) {
        match m {
            FlowMsg::Event(e) => match e {
                FlowEvent::Started { .. } => {}
                FlowEvent::BlockStarted { step, block, .. } => {
                    self.run.running.insert(step, block.clone());
                    if !matches!(self.run.status.get(&block), Some(BlockStatus::Running { .. })) {
                        self.run.status.insert(block, BlockStatus::Running { since: Instant::now() });
                    }
                }
                FlowEvent::Execution { .. } => {}
                FlowEvent::AiDelta { step, block, text } => match self.run.live.get_mut(&step) {
                    Some(live) => {
                        live.text.push_str(&text);
                        if let Some(line) = self.run.log.get_mut(live.log) {
                            line.text.push_str(&text);
                        }
                    }
                    None => {
                        let label = self.label_of(&block);
                        let log = self.run.push(LogKind::Info, Some(label), text.clone());
                        self.run.live.insert(step, LiveText { block, text, log });
                    }
                },
                FlowEvent::BlockFinished { step, block, port, output, duration_ms, failed, error, tests } => {
                    self.run.running.remove(&step);
                    self.run.live.remove(&step);
                    let passed = tests.iter().filter(|t| t.passed && !t.skipped).count();
                    let failed_tests = tests.iter().filter(|t| !t.passed && !t.skipped).count();
                    let label = self.label_of(&block);
                    match &error {
                        Some(e) => self.run.push(LogKind::Error, Some(label), e.clone()),
                        None => self.run.push(
                            if failed { LogKind::Error } else { LogKind::Step },
                            Some(label),
                            format!("→ {} · {}", tr(&port), human_ms(duration_ms)),
                        ),
                    };
                    // Still running in another branch.
                    if self.run.running.values().any(|b| *b == block) {
                        return;
                    }
                    self.run.status.insert(
                        block,
                        BlockStatus::Done {
                            port,
                            failed,
                            ms: duration_ms,
                            output,
                            error,
                            tests: (passed, failed_tests),
                        },
                    );
                }
                FlowEvent::Log { block, level, message } => {
                    let label = block.map(|b| self.label_of(&b));
                    let kind = match level {
                        ternarywire_core::script::LogLevel::Error => LogKind::Error,
                        ternarywire_core::script::LogLevel::Warn => LogKind::Warn,
                        ternarywire_core::script::LogLevel::Debug => LogKind::Debug,
                        ternarywire_core::script::LogLevel::Log | ternarywire_core::script::LogLevel::Info => {
                            LogKind::Info
                        }
                    };
                    self.run.push(kind, label, message);
                }
                FlowEvent::Error { block, message } => {
                    let label = block.map(|b| self.label_of(&b));
                    self.run.push(LogKind::Error, label, message);
                }
                FlowEvent::Finished { .. } => {}
            },
            FlowMsg::Done(result) => {
                self.run.cancel = None;
                self.run.running.clear();
                self.run.live.clear();
                self.run.status.retain(|_, s| !matches!(s, BlockStatus::Running { .. }));
                let (kind, head) = if result.cancelled {
                    (LogKind::Warn, tr("Stopped"))
                } else if result.success {
                    (LogKind::Done, tr("Succeeded"))
                } else {
                    (LogKind::Error, tr("Failed"))
                };
                let summary = format!("{} · {}", steps_text(result.steps.len()), human_ms(result.duration_ms));
                self.run.push(kind, None, format!("{head} · {summary}"));
                self.run.result = Some(result);
            }
        }
    }

    /// Selects the block with this display name (automation); an unknown
    /// or empty name clears the selection.
    pub fn select_block(&mut self, name: &str) {
        self.selected =
            self.flow.blocks.iter().find(|b| b.display_name() == name).map(|b| Selection::Block(b.id.clone()));
    }

    fn touch(&mut self) {
        self.last_edit = Some(Instant::now());
        self.diagnostics.clear();
    }
}

// ---------------------------------------------------------------------------
// Block kinds: color, icon, description, summary
// ---------------------------------------------------------------------------

/// The color of a block kind (its icon tile and header tint).
fn kind_color(kind: BlockKind, dark: bool) -> Color32 {
    let p = theme::palette(dark);
    match kind {
        BlockKind::Start | BlockKind::Output => p.ok,
        BlockKind::Request => p.accent,
        BlockKind::Condition => p.warn,
        BlockKind::Loop => theme::method_color("PATCH", dark),
        BlockKind::Parallel | BlockKind::Join => theme::method_color("WS", dark),
        BlockKind::Script => theme::method_color("PUT", dark),
        BlockKind::Ai => theme::method_color("GQL", dark),
        BlockKind::SetVariable => theme::method_color("HEAD", dark),
        BlockKind::Template => theme::method_color("OPTIONS", dark),
        BlockKind::Delay | BlockKind::Log => p.text_muted,
    }
}

fn kind_icon(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Start => icons::PLAY,
        BlockKind::Request => icons::GLOBE,
        BlockKind::Condition => icons::GIT_BRANCH,
        BlockKind::Loop => icons::ARROWS_CLOCKWISE,
        BlockKind::Parallel => icons::GIT_FORK,
        BlockKind::Join => icons::GIT_MERGE,
        BlockKind::Script => icons::CODE,
        BlockKind::Ai => icons::SPARKLE,
        BlockKind::Delay => icons::HOURGLASS,
        BlockKind::SetVariable => icons::BRACKETS_CURLY,
        BlockKind::Template => icons::FILE_TEXT,
        BlockKind::Output => icons::EXPORT,
        BlockKind::Log => icons::NOTE,
    }
}

fn kind_description(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Start => tr("Where the flow begins"),
        BlockKind::Request => tr("Sends a saved or inline request"),
        BlockKind::Condition => tr("Continues on true or false"),
        BlockKind::Loop => tr("Repeats for each item or a number of times"),
        BlockKind::Parallel => tr("Runs branches at the same time"),
        BlockKind::Join => tr("Waits for every parallel branch"),
        BlockKind::Script => tr("Runs JavaScript"),
        BlockKind::Ai => tr("Asks an AI model"),
        BlockKind::Delay => tr("Waits before continuing"),
        BlockKind::SetVariable => tr("Stores a value in a variable"),
        BlockKind::Template => tr("Builds text or JSON from a template"),
        BlockKind::Output => tr("Names a result of the flow"),
        BlockKind::Log => tr("Writes a message to the run log"),
    }
}

/// The color of a port (its dot and the connections leaving it).
fn port_color(port: &str, p: &Palette, dark: bool) -> Color32 {
    match port {
        "success" | "true" => p.ok,
        "failure" | "false" => p.err,
        "item" => p.info,
        "break" => p.warn,
        PORT_BRANCHES => kind_color(BlockKind::Parallel, dark),
        _ => p.text_faint,
    }
}

/// One line describing the block's configuration.
fn summary(app: &TernaryWireApp, flow: &Flow, b: &Block) -> String {
    match &b.config {
        BlockConfig::Start(_) => {
            if flow.inputs.is_empty() {
                tr("Entry point").into()
            } else {
                let names: Vec<&str> = flow.inputs.iter().map(|i| i.name.as_str()).collect();
                trf("inputs: {}", &[&names.join(", ")])
            }
        }
        BlockConfig::Request(c) => match &c.source {
            RequestSource::Saved(r) => request_name(app, r).unwrap_or_else(|| tr("(missing request)").into()),
            RequestSource::Inline { request, .. } => format!("{} {}", request.method, request.url),
        },
        BlockConfig::Condition(c) => match c.mode {
            ConditionMode::Compare if c.op.is_unary() => format!("{} {}", c.left, c.op.label()),
            ConditionMode::Compare => format!("{} {} {}", c.left, c.op.label(), c.right),
            ConditionMode::Expression => c.expression.clone(),
        },
        BlockConfig::Loop(c) => match c.mode {
            LoopMode::ForEach => format!("{} {}", tr("each of"), c.items),
            LoopMode::Count => format!("{} × {}", c.count, tr("times")),
        },
        BlockConfig::Script(c) => c.code.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_string(),
        BlockConfig::Ai(c) => {
            if c.model.is_empty() {
                c.prompt.clone()
            } else {
                format!("{} · {}", c.model, c.prompt)
            }
        }
        BlockConfig::Delay(c) => format!("{} ms", c.ms),
        BlockConfig::SetVariable(c) => format!("{} = {}", c.name, c.value),
        BlockConfig::Template(c) => match c.mode {
            TemplateMode::Template => c.template.clone(),
            TemplateMode::Expression => c.expression.clone(),
        },
        BlockConfig::Output(c) => format!("{} = {}", c.name, c.value),
        BlockConfig::Log(c) => c.message.clone(),
        BlockConfig::Parallel(_) => trf("{} branches at once", &[&flow.edges_from(&b.id, PORT_BRANCHES).count()]),
        BlockConfig::Join(_) => trf("waits for {} incoming", &[&flow.edges_to(&b.id).count()]),
    }
}

/// The end of `s` on one line: at most `max` characters, plus "…" when cut.
fn tail(s: &str, max: usize) -> String {
    let one_line = s.trim_end().replace('\n', " ");
    let n = one_line.chars().count();
    if n <= max { one_line } else { format!("…{}", one_line.chars().skip(n - max).collect::<String>()) }
}

fn blocks_text(n: usize) -> String {
    if n == 1 { tr("1 block").to_string() } else { trf("{} blocks", &[&n]) }
}

fn connections_text(n: usize) -> String {
    if n == 1 { tr("1 connection").to_string() } else { trf("{} connections", &[&n]) }
}

fn steps_text(n: usize) -> String {
    if n == 1 { tr("1 step").to_string() } else { trf("{} steps", &[&n]) }
}

pub(crate) fn problems_text(n: usize) -> String {
    if n == 1 { tr("1 problem found").to_string() } else { trf("{} problems found", &[&n]) }
}

fn elapsed(since: Instant) -> String {
    format!("{:.1} s", since.elapsed().as_secs_f64())
}

/// `mm:ss.mmm` since the start of the run.
fn clock(d: Duration) -> String {
    let ms = d.as_millis();
    format!("{:02}:{:02}.{:03}", ms / 60_000, (ms / 1000) % 60, ms % 1000)
}

fn request_name(app: &TernaryWireApp, r: &ItemRef) -> Option<String> {
    app.ws
        .data
        .collections
        .iter()
        .filter(|c| r.collection_id.is_empty() || c.id == r.collection_id)
        .find_map(|c| c.find_request(&r.request_id).map(|i| format!("{} / {}", c.name, i.name)))
}

fn reference_key(b: &Block) -> String {
    if b.label.trim().is_empty() { b.id.clone() } else { b.label.trim().to_string() }
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

fn block_height(kind: BlockKind) -> f32 {
    let rows = kind.input_ports().len().max(kind.output_ports().len()).max(1) as f32;
    PORTS_TOP + rows * PORT_ROW + 4.0 + 1.0
}

/// Rounds to the half grid, so moved blocks line up.
fn snap(v: f32) -> f32 {
    let step = GRID / 2.0;
    (v / step).round() * step
}

/// A font size at a zoom level, in half pixels (fewer glyph sizes to cache).
fn fs(size: f32, zoom: f32) -> f32 {
    ((size * zoom * 2.0).round() / 2.0).max(4.0)
}

struct Geo {
    origin: Pos2,
    zoom: f32,
}

impl Geo {
    fn to_screen(&self, p: (f32, f32)) -> Pos2 {
        self.origin + Vec2::new(p.0, p.1) * self.zoom
    }

    fn to_canvas(&self, p: Pos2) -> (f32, f32) {
        let v = (p - self.origin) / self.zoom;
        (v.x, v.y)
    }

    fn block_rect(&self, b: &Block) -> Rect {
        Rect::from_min_size(self.to_screen(b.position), Vec2::new(BLOCK_W, block_height(b.kind())) * self.zoom)
    }

    fn port_y(&self, r: Rect, i: usize) -> f32 {
        r.top() + (PORTS_TOP + PORT_ROW * (i as f32 + 0.5)) * self.zoom
    }

    fn in_port(&self, b: &Block, i: usize) -> Pos2 {
        let r = self.block_rect(b);
        Pos2::new(r.left(), self.port_y(r, i))
    }

    fn out_port(&self, b: &Block, i: usize) -> Pos2 {
        let r = self.block_rect(b);
        Pos2::new(r.right(), self.port_y(r, i))
    }
}

fn bezier(a: Pos2, b: Pos2, zoom: f32) -> [Pos2; 4] {
    // Backward connections (loops) bend as far out as forward ones.
    let dx = ((b.x - a.x).abs() * 0.5).max(36.0 * zoom);
    [a, Pos2::new(a.x + dx, a.y), Pos2::new(b.x - dx, b.y), b]
}

fn bezier_at(pts: &[Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    (pts[0].to_vec2() * (u * u * u)
        + pts[1].to_vec2() * (3.0 * u * u * t)
        + pts[2].to_vec2() * (3.0 * u * t * t)
        + pts[3].to_vec2() * (t * t * t))
        .to_pos2()
}

/// Distance from `p` to the curve (measured on 32 straight pieces).
fn bezier_distance(pts: &[Pos2; 4], p: Pos2) -> f32 {
    let mut prev = pts[0];
    let mut best = f32::INFINITY;
    for i in 1..=32 {
        let q = bezier_at(pts, i as f32 / 32.0);
        let ab = q - prev;
        let t = ((p - prev).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
        best = best.min((prev + ab * t).distance(p));
        prev = q;
    }
    best
}

/// Text on one line, cut with "…" at `max_w`.
fn one_line(painter: &egui::Painter, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.replace('\n', " "), TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(1.0));
    painter.layout_job(job)
}

/// A small spinning arc.
fn spinner(ui: &Ui, painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    let t = ui.input(|i| i.time) as f32;
    let start = t * 5.0;
    let pts: Vec<Pos2> = (0..=24)
        .map(|i| {
            let a = start + i as f32 / 24.0 * 4.2;
            center + radius * vec2(a.cos(), a.sin())
        })
        .collect();
    painter.add(Shape::line(pts, Stroke::new((radius * 0.32).max(1.2), color)));
    ui.ctx().request_repaint_after(Duration::from_millis(30));
}

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut FlowTab) {
    if std::mem::take(&mut tab.auto_run) && !tab.is_running() {
        start_run(app, tab_id, tab);
    }
    match tab.auto.take().as_deref() {
        Some("validate") => {
            tab.diagnostics = ternarywire_flows::validate(&tab.flow, Some(&app.ws.data.collections));
        }
        Some("add-block") => tab.open_add_menu = true,
        Some("fit") => tab.fit = true,
        // `view:<zoom>:<x>:<y>`: a fixed view, the canvas origin at (x, y)
        // from the canvas's corner (the prototype's 78 % at 12, 64).
        Some(v) if v.starts_with("view:") => {
            let n: Vec<f32> = v[5..].split(':').filter_map(|t| t.parse().ok()).collect();
            if let [zoom, x, y] = n[..] {
                tab.zoom = zoom;
                tab.pan = Vec2::new(x, y);
                tab.fit = false;
                tab.moved_view = true;
            }
        }
        _ => {}
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let width = ui.available_width();
    let inspector_open = tab.inspector.unwrap_or(width >= 820.0 || tab.selected.is_some());
    header(app, ui, tab_id, tab, inspector_open);

    // Three fixed zones: the canvas, the run log under it and the inspector
    // on the right at the full height of the view.
    let body = ui.available_rect_before_wrap();
    let insp_w = if inspector_open {
        tab.inspector_w.clamp(260.0, (body.width() * 0.5).max(260.0)).min((body.width() - 200.0).max(0.0))
    } else {
        0.0
    };
    let left = Rect::from_min_max(body.min, pos2(body.right() - insp_w, body.bottom()));
    let log_h = tab.log_h.clamp(110.0, (left.height() * 0.6).max(110.0)).min((left.height() - 90.0).max(0.0));
    let log = Rect::from_min_max(pos2(left.left(), left.bottom() - log_h), left.max);
    let canvas_rect = Rect::from_min_max(left.min, pos2(left.right(), log.top()));
    let area = |ui: &mut Ui, rect: Rect, salt: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.scope_builder(UiBuilder::new().max_rect(rect).id_salt(salt).layout(Layout::top_down(Align::Min)), |ui| {
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    };
    area(ui, canvas_rect, "flow-canvas-area", &mut |ui| canvas(app, ui, tab));
    area(ui, log, "flow-log-area", &mut |ui| run_log(ui, tab));
    let p = theme::pal(ui);
    ui.painter().hline(log.x_range(), log.top() + 0.5, Stroke::new(1.0, p.border));
    if insp_w > 0.0 {
        let insp = Rect::from_min_max(pos2(left.right(), body.top()), body.max);
        area(ui, insp, "flow-inspector-area", &mut |ui| inspector_panel(app, ui, tab));
        ui.painter().vline(insp.left() + 0.5, insp.y_range(), Stroke::new(1.0, p.border));
        let handle = Rect::from_center_size(pos2(insp.left(), insp.center().y), vec2(6.0, insp.height()));
        let (d, reset) = resize_handle(ui, ui.scope_id().with(("flow-inspector-edge", tab_id)), handle, true);
        if reset {
            tab.inspector_w = INSPECTOR_W;
        } else if d != 0.0 {
            tab.inspector_w = insp_w - d;
        }
    }
    let handle = Rect::from_center_size(pos2(log.center().x, log.top()), vec2(log.width(), 6.0));
    let (d, reset) = resize_handle(ui, ui.scope_id().with(("flow-log-edge", tab_id)), handle, false);
    if reset {
        tab.log_h = LOG_H;
    } else if d != 0.0 {
        tab.log_h = log_h - d;
    }
    ui.advance_cursor_after_rect(body);
    autosave(app, ui, tab);
}

/// A draggable edge between two zones (`vertical`: a vertical line, dragged
/// sideways). Returns the drag along its axis and whether it was double
/// clicked (reset).
fn resize_handle(ui: &mut Ui, id: egui::Id, rect: Rect, vertical: bool) -> (f32, bool) {
    let r = ui.interact(rect, id, Sense::click_and_drag());
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(if vertical { CursorIcon::ResizeHorizontal } else { CursorIcon::ResizeVertical });
    }
    let d = if vertical { r.drag_delta().x } else { r.drag_delta().y };
    (d, r.double_clicked())
}

fn autosave(app: &mut TernaryWireApp, ui: &Ui, tab: &mut FlowTab) {
    let Some(t) = tab.last_edit else { return };
    let wait = Duration::from_millis(700).saturating_sub(t.elapsed());
    if !wait.is_zero() {
        ui.ctx().request_repaint_after(wait);
        return;
    }
    if tab.flush(app.ws.root()) {
        app.sidebar.flows_dirty = true;
    }
}

impl FlowTab {
    /// Writes edits the autosave has not written yet (it waits for a pause
    /// in editing; the tab may close or the app quit before). Returns
    /// whether the flow file was written.
    pub fn flush(&mut self, root: &std::path::Path) -> bool {
        if self.last_edit.take().is_none() || (!self.dirty() && self.path.is_some()) {
            return false;
        }
        match ternarywire_flows::storage::save(root, &self.flow) {
            Ok(path) => {
                self.path = Some(path);
                self.saved = self.flow.clone();
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }
}

/// The 46 px title row: breadcrumb with the flow name and save state on the
/// left; zoom, Inspector, Fit, Validate, Add block and Run on the right.
fn header(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut FlowTab, inspector_open: bool) {
    let p = theme::pal(ui);
    let narrow = ui.available_width() < 780.0;
    let opts = |icon: &'static str| kit::ButtonOpts { icon: Some(icon), ..Default::default() };
    kit::bar(ui, size::VIEW_TITLE, true, |ui| {
        ui.add_space(4.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if tab.is_running() {
                let stop = kit::button(ui, kit::Kind::Danger, kit::Size::Sm, tr("Stop"), opts(icons::STOP));
                if stop.on_hover_text(tr("Stop the run")).clicked() {
                    tab.stop();
                }
            } else {
                let run = kit::button(ui, kit::Kind::Primary, kit::Size::Sm, tr("Run"), opts(icons::PLAY));
                if run.on_hover_text(tr("Run the flow (⌘↩)")).clicked() {
                    start_run(app, tab_id, tab);
                }
            }
            let add = if narrow {
                kit::icon_button_sm(ui, icons::PLUS, tr("Add block"))
            } else {
                let o = kit::ButtonOpts { icon: Some(icons::PLUS), caret: true, ..Default::default() };
                kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Add block"), o)
            };
            if std::mem::take(&mut tab.open_add_menu) {
                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&add));
            }
            egui::Popup::menu(&add)
                .width(268.0)
                .align(egui::RectAlign::BOTTOM_END)
                .show(|ui| block_menu(ui, tab, None));
            let validate = if narrow {
                kit::icon_button_sm(ui, icons::SHIELD_CHECK, tr("Validate"))
            } else {
                kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Validate"), opts(icons::SHIELD_CHECK))
            };
            if validate.on_hover_text(tr("Check the flow for problems")).clicked() {
                validate_flow(app, tab);
            }
            let fit = if narrow {
                kit::icon_button_sm(ui, icons::ARROWS_OUT, tr("Fit"))
            } else {
                kit::button(ui, kit::Kind::Ghost, kit::Size::Sm, tr("Fit"), opts(icons::ARROWS_OUT))
            };
            if fit.on_hover_text(tr("Show all blocks")).clicked() {
                tab.fit = true;
            }
            vdiv(ui, 18.0);
            if kit::icon_button_ex(ui, icons::SIDEBAR, tr("Inspector"), inspector_open, 22.0).clicked() {
                tab.inspector = Some(!inspector_open);
            }
            if !narrow {
                let text = format!("{:.0}%", tab.zoom * 100.0);
                let pill =
                    kit::pill_ex(ui, &text, Tone::Neutral, kit::PillOpts { outline: true, ..Default::default() });
                let reset = ui
                    .interact(pill.rect, ui.scope_id().with(("flow-zoom-reset", tab_id)), Sense::click())
                    .on_hover_text(tr("Zoom level (click for 100 %)"));
                widgets::a11y_button(&reset, tr("Reset zoom"));
                if reset.clicked() {
                    let anchor = tab.view / 2.0;
                    set_zoom(tab, 1.0, anchor);
                }
            }
            // Breadcrumb: Flows › name, then the save state.
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(icons::FLOW_ARROW).font(theme::icon_font(13.5)).color(p.accent_text));
                if !narrow {
                    ui.label(RichText::new(tr("Flows")).size(12.5).color(p.text_muted));
                    ui.label(RichText::new(icons::CARET_RIGHT).font(theme::icon_font(11.0)).color(p.text_faint));
                }
                let title_w = (ui.available_width() - 96.0).max(60.0);
                match &mut tab.renaming {
                    Some(buf) => {
                        let r = ui.add(
                            egui::TextEdit::singleline(buf)
                                .font(theme::semibold(14.0))
                                .desired_width(title_w.min(320.0)),
                        );
                        r.request_focus();
                        let escape = ui.input(|i| i.key_pressed(Key::Escape));
                        if r.lost_focus() || escape {
                            if !escape && !buf.trim().is_empty() && buf.trim() != tab.flow.name {
                                tab.flow.name = buf.trim().to_string();
                                tab.touch();
                            }
                            tab.renaming = None;
                        }
                    }
                    None => {
                        let name = RichText::new(&tab.flow.name).font(theme::semibold(14.0)).color(p.text);
                        let r = ui
                            .scope(|ui| {
                                ui.set_max_width(title_w);
                                ui.add(egui::Label::new(name).sense(Sense::click()).selectable(false).truncate())
                            })
                            .inner;
                        if r.on_hover_text(tr("Double-click to rename")).double_clicked() {
                            tab.renaming = Some(tab.flow.name.clone());
                        }
                    }
                }
                ui.add_space(2.0);
                save_pill(ui, tab);
            });
        });
    });
}

fn save_pill(ui: &mut Ui, tab: &FlowTab) {
    let o = |icon: &'static str| kit::PillOpts { icon: Some(icon), ..Default::default() };
    if let Some(e) = &tab.error {
        kit::pill_ex(ui, tr("Not saved"), Tone::Err, o(icons::WARNING_CIRCLE)).on_hover_text(e);
    } else if tab.dirty() || tab.last_edit.is_some() {
        kit::pill_ex(ui, tr("Saving…"), Tone::Neutral, o(icons::CLOUD_ARROW_UP));
    } else {
        let where_ = tab.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        kit::pill_ex(ui, tr("Saved"), Tone::Ok, o(icons::CHECK)).on_hover_text(where_);
    }
}

/// Zooms to `zoom`, keeping the canvas point under `anchor` (relative to
/// the canvas' top left corner) in place.
fn set_zoom(tab: &mut FlowTab, zoom: f32, anchor: Vec2) {
    tab.moved_view = true;
    let old = tab.zoom;
    tab.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let before = (anchor - tab.pan) / old;
    tab.pan = anchor - before * tab.zoom;
}

fn validate_flow(app: &mut TernaryWireApp, tab: &mut FlowTab) {
    tab.diagnostics = ternarywire_flows::validate(&tab.flow, Some(&app.ws.data.collections));
    if tab.diagnostics.is_empty() {
        app.toast(tr("No problems found"), false);
    } else {
        // The flow settings list the problems.
        tab.selected = None;
        tab.inspector = Some(true);
        app.toast(problems_text(tab.diagnostics.len()), true);
    }
}

/// The "Add block" menu: block kinds in groups (Actions, Logic, Data), each
/// with its colored tile and its description on a second line.
fn block_menu(ui: &mut Ui, tab: &mut FlowTab, at: Option<(f32, f32)>) {
    ui.set_min_width(256.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    let groups: [(&str, &[BlockKind]); 3] = [
        (tr("Actions"), &[BlockKind::Request, BlockKind::Script, BlockKind::Ai]),
        (tr("Logic"), &[BlockKind::Condition, BlockKind::Loop, BlockKind::Parallel, BlockKind::Join, BlockKind::Delay]),
        (tr("Data"), &[BlockKind::SetVariable, BlockKind::Template, BlockKind::Output, BlockKind::Log]),
    ];
    if tab.flow.start().is_none() && block_menu_item(ui, BlockKind::Start).clicked() {
        add_block(tab, BlockKind::Start, at);
        ui.close();
    }
    for (title, kinds) in groups.iter() {
        kit::menu_header(ui, title);
        for kind in kinds.iter() {
            if block_menu_item(ui, *kind).clicked() {
                add_block(tab, *kind, at);
                ui.close();
            }
        }
    }
}

/// One kind in the Add block menu: a 26 px tile, the name and the
/// description under it.
fn block_menu_item(ui: &mut Ui, kind: BlockKind) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let w = ui.available_width().max(240.0);
    let text_w = w - 8.0 - 26.0 - 10.0 - 8.0;
    // Line heights of the prototype: 13 × 1.45 and 11 × 1.25.
    let lay = |text: &str, font: FontId, color: Color32, line: f32, wrap: f32| {
        let mut job = LayoutJob::default();
        job.append(text, 0.0, TextFormat { line_height: Some(line), ..TextFormat::simple(font, color) });
        job.wrap.max_width = wrap;
        ui.painter().layout_job(job)
    };
    let label = lay(tr(kind.label()), theme::medium(13.0), p.text, 19.0, f32::INFINITY);
    let desc = lay(kind_description(kind), FontId::proportional(11.0), p.text_muted, 14.0, text_w);
    let h = (label.size().y + desc.size().y + 10.0).max(36.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    widgets::a11y_button(&resp, tr(kind.label()));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(RADIUS), p.hover);
        }
        let tile = Rect::from_min_size(pos2(rect.left() + 8.0, rect.center().y - 13.0), Vec2::splat(26.0));
        painter.rect_filled(tile, CornerRadius::same(RADIUS), kind_color(kind, dark));
        painter.text(tile.center(), Align2::CENTER_CENTER, kind_icon(kind), theme::icon_font(14.0), p.on_accent);
        let x = tile.right() + 10.0;
        let top = rect.center().y - (label.size().y + desc.size().y) / 2.0;
        let lh = label.size().y;
        painter.galley(pos2(x, top), label, p.text);
        painter.galley(pos2(x, top + lh), desc, p.text_muted);
    }
    resp
}

/// Adds a block at `at` (canvas units) or in the middle of the view, and
/// selects it.
fn add_block(tab: &mut FlowTab, kind: BlockKind, at: Option<(f32, f32)>) -> String {
    let (x, y) = at.unwrap_or_else(|| {
        let c = (tab.view / 2.0 - tab.pan) / tab.zoom;
        (c.x - BLOCK_W / 2.0, c.y - block_height(kind) / 2.0)
    });
    let mut pos = (snap(x), snap(y));
    while tab.flow.blocks.iter().any(|b| (b.position.0 - pos.0).abs() < GRID && (b.position.1 - pos.1).abs() < GRID) {
        pos = (pos.0 + 2.0 * GRID, pos.1 + 2.0 * GRID);
    }
    let id = tab.flow.add_block(Block::new(kind.default_config(), "", pos));
    tab.selected = Some(Selection::Block(id.clone()));
    tab.touch();
    id
}

/// Adds a block to the right of `from` and connects `port` to it.
fn add_after(tab: &mut FlowTab, from: &str, port: &str, kind: BlockKind) {
    let Some(b) = tab.flow.block(from) else { return };
    let pos = (b.position.0 + BLOCK_W + 60.0, b.position.1);
    let id = add_block(tab, kind, Some(pos));
    tab.flow.edges.push(Edge::new(from, port, &id, "in"));
}

fn duplicate_block(tab: &mut FlowTab, id: &str) {
    let Some(b) = tab.flow.block(id).cloned() else { return };
    if b.kind() == BlockKind::Start {
        return;
    }
    let copy =
        Block::new(b.config.clone(), trf("{} copy", &[&b.display_name()]), (b.position.0 + 40.0, b.position.1 + 40.0));
    let nid = tab.flow.add_block(copy);
    tab.selected = Some(Selection::Block(nid));
    tab.touch();
}

fn start_run(app: &mut TernaryWireApp, tab_id: u64, tab: &mut FlowTab) {
    let diags = ternarywire_flows::validate(&tab.flow, Some(&app.ws.data.collections));
    if diags.iter().any(|d| d.severity == Severity::Error) {
        tab.diagnostics = diags;
        tab.selected = None;
        tab.inspector = Some(true);
        app.toast(tr("Fix the problems listed in the inspector before running"), true);
        return;
    }
    tab.diagnostics = diags;
    let mut opts = FlowRunOptions::default();
    for input in &tab.flow.inputs {
        if let Some(text) = tab.inputs.get(&input.name).filter(|t| !t.trim().is_empty()) {
            let v = serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()));
            opts.inputs.insert(input.name.clone(), v);
        }
    }
    let env = FlowEnv {
        services: app.services(),
        collections: app.ws.data.collections.clone(),
        base_dir: Some(app.ws.root().to_path_buf()),
    };
    let mut vars = app.ws.scopes_for(None);
    let flow = tab.flow.clone();
    let cancel = CancellationToken::new();
    let epoch = crate::app::next_epoch();
    tab.run = RunState { epoch, cancel: Some(cancel.clone()), started: Some(Instant::now()), ..Default::default() };
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let events = tx.clone();
        let repaint = ctx.clone();
        let mut sink = move |e: FlowEvent| {
            let _ = events.send(Msg::Flow { tab: tab_id, epoch, event: FlowMsg::Event(e) });
            repaint.request_repaint();
        };
        let result = ternarywire_flows::run_flow(&env, &flow, &mut vars, opts, &mut sink, &cancel).await;
        let _ = tx.send(Msg::Flow { tab: tab_id, epoch, event: FlowMsg::Done(Box::new(result)) });
        ctx.request_repaint();
    });
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

/// What a block card shows.
struct CardLook<'a> {
    block: &'a Block,
    rect: Rect,
    zoom: f32,
    line: &'a str,
    selected: bool,
    hovered: bool,
    status: Option<&'a BlockStatus>,
    diag: Option<Severity>,
}

fn canvas(app: &TernaryWireApp, ui: &mut Ui, tab: &mut FlowTab) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let rect = ui.available_rect_before_wrap();
    if !tab.moved_view && tab.view != Vec2::ZERO && (tab.view - rect.size()).length() > 1.0 {
        tab.fit = true;
    }
    tab.view = rect.size();
    let bg = ui.interact(rect, ui.scope_id().with("flow-canvas"), Sense::click_and_drag());
    widgets::a11y_name(&bg, tr("Flow canvas"));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, p.well);

    // Zoom around the pointer with pinch or ⌘ + scroll; scroll or drag the
    // empty canvas to pan (diagonally too: no trackpad axis lock here).
    crate::scroll_lock::allow_free_scroll(ui.ctx(), rect);
    if ui.rect_contains_pointer(rect) && !egui::Popup::is_any_open(ui.ctx()) {
        let zoom = ui.input(|i| i.zoom_delta());
        if (zoom - 1.0).abs() > f32::EPSILON {
            if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
                let z = tab.zoom * zoom;
                set_zoom(tab, z, ptr - rect.min);
            }
        } else {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll != Vec2::ZERO {
                tab.pan += scroll;
                tab.moved_view = true;
            }
        }
    }
    if bg.dragged() && tab.connecting.is_none() {
        tab.pan += bg.drag_delta();
        tab.moved_view = true;
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }
    if std::mem::take(&mut tab.fit) {
        fit_view(tab, rect.size());
    }
    let geo = Geo { origin: rect.min + tab.pan, zoom: tab.zoom };
    let z = tab.zoom;

    // Dot grid (moves and scales with the blocks).
    let step = GRID * z;
    if step >= 8.0 {
        let ox = (geo.origin.x - rect.left()).rem_euclid(step);
        let oy = (geo.origin.y - rect.top()).rem_euclid(step);
        let mut y = rect.top() + oy;
        while y < rect.bottom() {
            let mut x = rect.left() + ox;
            while x < rect.right() {
                painter.circle_filled(Pos2::new(x, y), 1.0, p.border_strong);
                x += step;
            }
            y += step;
        }
    }

    let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|q| rect.contains(*q));
    let worst = |m: &mut HashMap<_, Severity>, k, s: Severity| {
        let e = m.entry(k).or_insert(s);
        if s == Severity::Error {
            *e = s;
        }
    };
    let mut diag_blocks: HashMap<String, Severity> = HashMap::new();
    let mut diag_edges: HashMap<usize, Severity> = HashMap::new();
    for d in &tab.diagnostics {
        if let Some(b) = &d.block {
            worst(&mut diag_blocks, b.clone(), d.severity);
        }
        if let Some(e) = d.edge {
            let entry = diag_edges.entry(e).or_insert(d.severity);
            if d.severity == Severity::Error {
                *entry = d.severity;
            }
        }
    }
    let over_block = pointer.is_some_and(|q| tab.flow.blocks.iter().any(|b| geo.block_rect(b).expand(6.0).contains(q)));

    // Edges: green and thicker where the last run went, thin gray elsewhere.
    let edge_pts: Vec<Option<[Pos2; 4]>> = tab
        .flow
        .edges
        .iter()
        .map(|e| {
            let (from, to) = (tab.flow.block(&e.from_block)?, tab.flow.block(&e.to_block)?);
            let oi = from.kind().output_ports().iter().position(|p| *p == e.from_port).unwrap_or(0);
            let ii = to.kind().input_ports().iter().position(|p| *p == e.to_port).unwrap_or(0);
            Some(bezier(geo.out_port(from, oi), geo.in_port(to, ii), z))
        })
        .collect();
    let hover_edge = match pointer {
        Some(q) if !over_block && tab.connecting.is_none() => edge_pts
            .iter()
            .enumerate()
            .filter_map(|(i, pts)| pts.map(|pts| (i, bezier_distance(&pts, q))))
            .filter(|(_, d)| *d < 7.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i),
        _ => None,
    };
    if hover_edge.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    for (i, e) in tab.flow.edges.iter().enumerate() {
        let Some(pts) = edge_pts[i] else { continue };
        let selected = tab.selected == Some(Selection::Edge(i));
        let hovered = hover_edge == Some(i);
        let failure = matches!(e.from_port.as_str(), "failure" | "false");
        let taken =
            matches!(tab.run.status.get(&e.from_block), Some(BlockStatus::Done { port, .. }) if *port == e.from_port);
        let (color, width) = if selected {
            (p.accent, 2.4)
        } else if let Some(s) = diag_edges.get(&i) {
            (if *s == Severity::Error { p.err } else { p.warn }, 2.0)
        } else if hovered {
            (theme::soft(p.accent, 0.75), 2.4)
        } else if taken {
            ((if failure { p.err } else { p.ok }).gamma_multiply(0.85), 2.2)
        } else {
            (p.text_faint, 1.6)
        };
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            pts,
            false,
            Color32::TRANSPARENT,
            Stroke::new((width * z).max(1.0), color),
        ));
    }

    // Blocks.
    let mut moved = false;
    let mut snap_block: Option<String> = None;
    let mut new_edge: Option<Edge> = None;
    let mut delete_block: Option<String> = None;
    let mut duplicate: Option<String> = None;
    let mut disconnect: Option<String> = None;
    let released = ui.input(|i| i.pointer.any_released());
    let ids: Vec<String> = tab.flow.blocks.iter().map(|b| b.id.clone()).collect();
    for id in &ids {
        let Some(b) = tab.flow.block(id).cloned() else { continue };
        let kind = b.kind();
        let r = geo.block_rect(&b);
        if !r.expand(24.0).intersects(rect) {
            continue;
        }
        let selected = tab.selected == Some(Selection::Block(b.id.clone()));
        let card = ui.interact(r, ui.scope_id().with(("flow-block", &b.id)), Sense::click_and_drag());
        widgets::a11y_button(&card, &format!("{} {}", tr(kind.label()), b.display_name()));
        if card.clicked() || card.drag_started() {
            tab.selected = Some(Selection::Block(b.id.clone()));
        }
        if card.dragged()
            && let Some(blk) = tab.flow.block_mut(&b.id)
        {
            let d = card.drag_delta() / z;
            blk.position.0 += d.x;
            blk.position.1 += d.y;
            moved = true;
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if card.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
        if card.drag_stopped() {
            snap_block = Some(b.id.clone());
        }
        card.context_menu(|ui| {
            if kind != BlockKind::Start && kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), Some("⌘D")).clicked()
            {
                duplicate = Some(b.id.clone());
            }
            if kit::menu_item(ui, Some(icons::LINK_SIMPLE), tr("Remove connections"), None).clicked() {
                disconnect = Some(b.id.clone());
            }
            if kind != BlockKind::Start {
                kit::menu_sep(ui);
                let del = kit::MenuItem {
                    icon: Some(icons::TRASH),
                    label: tr("Delete"),
                    shortcut: Some("⌫"),
                    danger: true,
                    ..Default::default()
                };
                if kit::menu_item_ex(ui, del).clicked() {
                    delete_block = Some(b.id.clone());
                }
            }
        });

        // A streaming AI block shows the end of its answer as it grows.
        let line = match tab.run.live_text(&b.id) {
            Some(text) => tail(text, 60),
            None => summary(app, &tab.flow, &b),
        };
        let status = tab.run.status.get(&b.id);
        if matches!(status, Some(BlockStatus::Running { .. })) {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let look = CardLook {
            block: &b,
            rect: r,
            zoom: z,
            line: &line,
            selected,
            hovered: card.hovered() || card.dragged(),
            status,
            diag: diag_blocks.get(&b.id).copied(),
        };
        draw_card(ui, &painter, &look);

        // Port rows: inputs on the left edge, outputs on the right one.
        let detail = z >= DETAIL_ZOOM;
        let label_font = FontId::proportional(fs(10.5, z));
        for (i, port) in kind.input_ports().iter().enumerate() {
            let c = geo.in_port(&b, i);
            let connected = tab.flow.edges.iter().any(|e| e.to_block == b.id && e.to_port == *port);
            let hover = pointer.is_some_and(|q| q.distance(c) < 12.0 * z.max(0.8));
            let target = tab.connecting.as_ref().is_some_and(|(from, _)| *from != b.id);
            if target {
                painter.circle_filled(c, 11.0 * z, theme::soft(p.accent, if hover { 0.35 } else { 0.14 }));
            }
            let color = if target && hover { p.accent } else { port_color(port, &p, dark) };
            draw_port(&painter, &p, c, z, color, connected || (target && hover), hover && target);
            if detail {
                painter.text(c + vec2(11.0 * z, 0.0), Align2::LEFT_CENTER, tr(port), label_font.clone(), p.text_muted);
            }
            if let Some((from, from_port)) = &tab.connecting
                && hover
                && released
                && *from != b.id
            {
                new_edge = Some(Edge::new(from, from_port, &b.id, port));
            }
        }
        // Output ports: drag from them to connect.
        for (i, port) in kind.output_ports().iter().enumerate() {
            let c = geo.out_port(&b, i);
            let taken = matches!(status, Some(BlockStatus::Done { port: taken, .. }) if taken == port);
            let connected = tab.flow.edges.iter().any(|e| e.from_block == b.id && e.from_port == *port);
            let pr = ui.interact(
                Rect::from_center_size(c, Vec2::splat(18.0 * z.max(0.8))),
                ui.scope_id().with(("flow-port", &b.id, port)),
                Sense::drag(),
            );
            widgets::a11y_name(&pr, &format!("{} {} · {}", tr("Connect from"), b.display_name(), tr(port)));
            let active = tab.connecting.as_ref().is_some_and(|(f, fp)| *f == b.id && fp == port);
            let color = port_color(port, &p, dark);
            draw_port(&painter, &p, c, z, color, connected || taken || active, pr.hovered() || active);
            if detail {
                painter.text(c - vec2(11.0 * z, 0.0), Align2::RIGHT_CENTER, tr(port), label_font.clone(), p.text_muted);
            }
            if pr.drag_started() {
                tab.connecting = Some((b.id.clone(), port.to_string()));
            }
            if pr.hovered() || pr.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            }
        }
    }

    // The connection being drawn.
    if let (Some((from, port)), Some(q)) = (&tab.connecting, ui.input(|i| i.pointer.hover_pos()))
        && let Some(b) = tab.flow.block(from)
    {
        let oi = b.kind().output_ports().iter().position(|x| x == port).unwrap_or(0);
        let a = geo.out_port(b, oi);
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            bezier(a, q, z),
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.0, p.accent),
        ));
        painter.circle_filled(q, 4.0, p.accent);
        ui.ctx().request_repaint();
    }
    if released {
        tab.connecting = None;
    }
    if let Some(e) = new_edge
        && !tab.flow.edges.contains(&e)
    {
        tab.flow.edges.push(e);
        tab.selected = Some(Selection::Edge(tab.flow.edges.len() - 1));
        tab.touch();
    }

    // The selected connection gets a delete button at its middle.
    let mut remove_edge = None;
    if let Some(Selection::Edge(i)) = tab.selected
        && let Some(Some(pts)) = edge_pts.get(i)
    {
        let mid = bezier_at(pts, 0.5);
        if rect.contains(mid) {
            let br = Rect::from_center_size(mid, Vec2::splat(22.0));
            let x = ui
                .interact(br, ui.scope_id().with("flow-edge-delete"), Sense::click())
                .on_hover_text(tr("Delete connection"));
            widgets::a11y_button(&x, tr("Delete connection"));
            painter.circle(mid, 11.0, p.surface, Stroke::new(1.0, if x.hovered() { p.err } else { p.border_strong }));
            painter.text(
                mid,
                Align2::CENTER_CENTER,
                icons::X,
                theme::icon_font(12.0),
                if x.hovered() { p.err } else { p.text_muted },
            );
            if x.clicked() {
                remove_edge = Some(i);
            }
        }
    }

    // Clicks on the empty canvas: select a connection or clear the selection.
    if bg.clicked() {
        tab.selected = hover_edge.map(Selection::Edge);
    }
    if bg.secondary_clicked() {
        tab.menu_at = ui.input(|i| i.pointer.interact_pos()).map(|q| geo.to_canvas(q));
        if let Some(i) = hover_edge {
            tab.selected = Some(Selection::Edge(i));
        }
    }
    bg.context_menu(|ui| {
        if let Some(Selection::Edge(i)) = tab.selected {
            let del = kit::MenuItem {
                icon: Some(icons::TRASH),
                label: tr("Delete connection"),
                shortcut: Some("⌫"),
                danger: true,
                ..Default::default()
            };
            if kit::menu_item_ex(ui, del).clicked() {
                remove_edge = Some(i);
            }
            kit::menu_sep(ui);
        }
        let add =
            kit::MenuItem { icon: Some(icons::PLUS), label: tr("Add block here"), submenu: true, ..Default::default() };
        let r = kit::menu_item_ex(ui, add);
        egui::containers::menu::SubMenu::default().show(ui, &r, |ui| {
            let at = tab.menu_at;
            block_menu(ui, tab, at);
        });
        if kit::menu_item(ui, Some(icons::ARROWS_OUT), tr("Fit"), None).clicked() {
            tab.fit = true;
        }
    });

    // Keyboard: delete, duplicate, deselect and nudge the selection.
    if !ui.ctx().egui_wants_keyboard_input() && app.dialog.is_none() && !egui::Popup::is_any_open(ui.ctx()) {
        let (del, esc, dup, nudge) = ui.input(|i| {
            let step = if i.modifiers.shift { GRID * 2.0 } else { GRID / 2.0 };
            let mut d = Vec2::ZERO;
            for (key, v) in [
                (Key::ArrowLeft, vec2(-step, 0.0)),
                (Key::ArrowRight, vec2(step, 0.0)),
                (Key::ArrowUp, vec2(0.0, -step)),
                (Key::ArrowDown, vec2(0.0, step)),
            ] {
                if i.key_pressed(key) {
                    d += v;
                }
            }
            (
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::Escape),
                i.modifiers.command && i.key_pressed(Key::D),
                d,
            )
        });
        match tab.selected.clone() {
            Some(Selection::Block(id)) => {
                let start = tab.flow.block(&id).is_some_and(|b| b.kind() == BlockKind::Start);
                if del && !start {
                    delete_block = Some(id.clone());
                }
                if dup {
                    duplicate = Some(id.clone());
                }
                if nudge != Vec2::ZERO
                    && let Some(b) = tab.flow.block_mut(&id)
                {
                    b.position = (snap(b.position.0 + nudge.x), snap(b.position.1 + nudge.y));
                    moved = true;
                }
            }
            Some(Selection::Edge(i)) if del => remove_edge = Some(i),
            _ => {}
        }
        if esc {
            tab.connecting = None;
            tab.selected = None;
        }
    }
    if let Some(i) = remove_edge.filter(|i| *i < tab.flow.edges.len()) {
        tab.flow.edges.remove(i);
        tab.selected = None;
        tab.touch();
    }
    if let Some(id) = delete_block {
        tab.flow.remove_block(&id);
        tab.selected = None;
        tab.touch();
    }
    if let Some(id) = disconnect {
        tab.flow.edges.retain(|e| e.from_block != id && e.to_block != id);
        tab.selected = Some(Selection::Block(id));
        tab.touch();
    }
    if let Some(id) = duplicate {
        duplicate_block(tab, &id);
    }
    if let Some(id) = snap_block
        && let Some(b) = tab.flow.block_mut(&id)
    {
        b.position = (snap(b.position.0), snap(b.position.1));
        moved = true;
    }
    if moved {
        tab.touch();
    }

    zoom_capsule(ui, tab, rect);
    if tab.flow.blocks.len() <= 1 {
        empty_canvas(ui, tab, rect);
    }
    ui.allocate_rect(rect, Sense::hover());
}

/// A port: a ring in the port's color while nothing is connected (an
/// unhandled branch stands out), a filled dot with a halo once it is.
fn draw_port(painter: &egui::Painter, p: &Palette, c: Pos2, z: f32, color: Color32, filled: bool, hovered: bool) {
    let s = z.clamp(0.6, 1.6);
    if hovered {
        painter.circle_filled(c, 10.0 * s, theme::soft(color, 0.2));
    }
    if filled {
        painter.circle_filled(c, 8.0 * s, color);
        painter.circle_filled(c, 7.0 * s, p.surface);
        painter.circle_filled(c, 5.0 * s, color);
    } else {
        painter.circle(c, 5.75 * s, p.surface, Stroke::new(1.5 * s, color));
    }
}

/// A block card: header tinted with the kind's color (tile, name, run
/// badge), a monospace summary line, then the port rows.
fn draw_card(ui: &Ui, painter: &egui::Painter, look: &CardLook) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let z = look.zoom;
    let r = look.rect;
    let kind = look.block.kind();
    let color = kind_color(kind, dark);
    let px = |v: f32| (v * z).round().clamp(0.0, 255.0) as u8;
    let cr = CornerRadius::same(px(10.0).max(3));
    if look.selected {
        // Accent halo, then a soft drop shadow.
        painter.rect_filled(r.expand(4.0 * z), CornerRadius::same(px(14.0)), theme::soft(p.accent, 0.2));
    }
    let shadow = if dark {
        egui::Shadow { offset: [0, px(4.0) as i8], blur: px(14.0), spread: 0, color: Color32::from_black_alpha(89) }
    } else if look.selected {
        egui::Shadow { offset: [0, px(6.0) as i8], blur: px(16.0), spread: 0, color: Color32::from_black_alpha(26) }
    } else {
        egui::Shadow { offset: [0, px(2.0) as i8], blur: px(8.0), spread: 0, color: Color32::from_black_alpha(18) }
    };
    painter.add(shadow.as_shape(r, cr));
    painter.rect_filled(r, cr, p.surface);

    // Header.
    let inner = r.shrink(1.0);
    let head = Rect::from_min_size(inner.min, vec2(inner.width(), HEAD_H * z - 1.0));
    let hr = px(9.0).max(2);
    painter.rect_filled(head, CornerRadius { nw: hr, ne: hr, sw: 0, se: 0 }, theme::soft(color, 0.10));
    painter.hline(inner.x_range(), head.bottom() + 0.5 * z.min(1.0), Stroke::new(1.0, p.border));
    let t = 20.0 * z;
    let tile = Rect::from_min_size(Pos2::new(inner.left() + 7.0 * z, head.center().y - t / 2.0), Vec2::splat(t));
    painter.rect_filled(tile, CornerRadius::same(px(6.0)), color);
    painter.text(tile.center(), Align2::CENTER_CENTER, kind_icon(kind), theme::icon_font(fs(12.0, z)), p.on_accent);
    let right = inner.right() - 6.0 * z;
    let badge_left = draw_badge(ui, painter, look, Pos2::new(right, head.center().y)).unwrap_or(right + 6.0 * z);
    let tx = tile.right() + 6.0 * z;
    let g =
        one_line(painter, look.block.display_name(), theme::semibold(fs(11.5, z)), p.text, badge_left - 6.0 * z - tx);
    painter.galley(Pos2::new(tx, head.center().y - g.size().y / 2.0), g, p.text);

    // Summary line.
    let sum = Rect::from_min_size(Pos2::new(inner.left(), r.top() + HEAD_H * z), vec2(inner.width(), SUMMARY_H * z));
    if z >= DETAIL_ZOOM {
        let text = if look.line.trim().is_empty() { "—" } else { look.line };
        let mut job = var_job(text, theme::mono(fs(10.5, z)), p.text_muted, dark, false);
        job.wrap = egui::text::TextWrapping::truncate_at_width((sum.width() - 16.0 * z).max(1.0));
        let g = painter.layout_job(job);
        painter.galley(Pos2::new(sum.left() + 8.0 * z, sum.center().y - 0.5 * z - g.size().y / 2.0), g, p.text_muted);
    }
    painter.hline(inner.x_range(), sum.bottom() - 0.5 * z.min(1.0), Stroke::new(1.0, p.border));

    let border = match (look.status, look.diag) {
        (Some(BlockStatus::Running { .. }), _) => Stroke::new(1.5, p.accent),
        (Some(BlockStatus::Done { failed: true, .. }), _) => Stroke::new(1.5, p.err),
        (None, Some(Severity::Error)) => Stroke::new(1.0, p.err),
        (None, Some(Severity::Warning)) => Stroke::new(1.0, p.warn),
        _ if look.selected => Stroke::new(1.0, p.accent),
        _ if look.hovered => Stroke::new(1.0, p.text_faint),
        _ => Stroke::new(1.0, p.border_strong),
    };
    painter.rect_stroke(r, cr, border, StrokeKind::Inside);
}

/// The run (or problem) badge at the right of a card's header. Returns its
/// left edge.
fn draw_badge(ui: &Ui, painter: &egui::Painter, look: &CardLook, anchor: Pos2) -> Option<f32> {
    let p = theme::pal(ui);
    let z = look.zoom;
    let (icon, text, tone) = match (look.status, look.diag) {
        (Some(BlockStatus::Running { since }), _) => (None, elapsed(*since), Tone::Accent),
        (Some(BlockStatus::Done { failed: true, ms, .. }), _) => (Some(icons::X), human_ms(*ms), Tone::Err),
        (Some(BlockStatus::Done { ms, tests, .. }), _) => {
            let tests =
                if tests.0 + tests.1 > 0 { format!(" · {}/{}", tests.0, tests.0 + tests.1) } else { String::new() };
            (Some(icons::CHECK), format!("{}{tests}", human_ms(*ms)), Tone::Ok)
        }
        (None, Some(Severity::Error)) => (Some(icons::WARNING_CIRCLE), String::new(), Tone::Err),
        (None, Some(Severity::Warning)) => (Some(icons::WARNING), String::new(), Tone::Warn),
        _ => return None,
    };
    let (fg, bg) = p.tone(tone);
    let text = if z < DETAIL_ZOOM { String::new() } else { text };
    let g = painter.layout_no_wrap(text, theme::medium(fs(10.0, z)), fg);
    let icon_w = 10.0 * z;
    let gap = if g.size().x > 0.0 { 3.0 * z } else { 0.0 };
    let pad = 5.0 * z;
    let h = 17.0 * z;
    let w = pad * 2.0 + icon_w + gap + g.size().x;
    let rect = Rect::from_min_size(Pos2::new(anchor.x - w, anchor.y - h / 2.0), vec2(w, h));
    painter.rect_filled(rect, CornerRadius::same((h / 2.0) as u8), bg);
    let ic = Pos2::new(rect.left() + pad + icon_w / 2.0, rect.center().y);
    match icon {
        Some(i) => {
            painter.text(ic, Align2::CENTER_CENTER, i, theme::icon_font(fs(10.5, z)), fg);
        }
        None => spinner(ui, painter, ic, icon_w * 0.42, fg),
    }
    painter.galley(Pos2::new(ic.x + icon_w / 2.0 + gap, rect.center().y - g.size().y / 2.0), g, fg);
    Some(rect.left())
}

/// The floating zoom capsule at the bottom right of the canvas: zoom out,
/// the zoom level (click for 100 %), zoom in, and Fit.
fn zoom_capsule(ui: &mut Ui, tab: &mut FlowTab, rect: Rect) {
    let p = theme::pal(ui);
    let size = vec2(3.0 + 22.0 + 2.0 + 40.0 + 2.0 + 22.0 + 2.0 + 5.0 + 2.0 + 22.0 + 3.0 + 2.0, 30.0);
    let r = Rect::from_min_size(rect.right_bottom() - size - vec2(14.0, 14.0), size);
    if !rect.contains_rect(r) {
        return;
    }
    let painter = ui.painter();
    let dark = ui.visuals().dark_mode;
    painter.add(
        egui::Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 128 } else { 31 }),
        }
        .as_shape(r, CornerRadius::same(theme::RADIUS_CARD)),
    );
    painter.rect(r, CornerRadius::same(theme::RADIUS_CARD), p.surface, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let mut c = ui.new_child(UiBuilder::new().max_rect(r.shrink(4.0)).layout(Layout::left_to_right(Align::Center)));
    c.spacing_mut().item_spacing.x = 2.0;
    let anchor = rect.size() / 2.0;
    if kit::icon_button_sm(&mut c, icons::MINUS, tr("Zoom out")).clicked() {
        set_zoom(tab, tab.zoom / 1.2, anchor);
    }
    let (zr, zresp) = c.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    let zresp = zresp.on_hover_text(tr("Zoom level (click for 100 %)"));
    widgets::a11y_button(&zresp, tr("Reset zoom"));
    c.painter().text(
        zr.center(),
        Align2::CENTER_CENTER,
        format!("{:.0}%", tab.zoom * 100.0),
        FontId::proportional(11.5),
        if zresp.hovered() { p.text } else { p.text_muted },
    );
    if zresp.clicked() {
        set_zoom(tab, 1.0, anchor);
    }
    if kit::icon_button_sm(&mut c, icons::PLUS, tr("Zoom in")).clicked() {
        set_zoom(tab, tab.zoom * 1.2, anchor);
    }
    let (dr, _) = c.allocate_exact_size(vec2(5.0, 14.0), Sense::hover());
    c.painter().vline(dr.center().x, dr.y_range(), Stroke::new(1.0, p.border));
    if kit::icon_button_sm(&mut c, icons::ARROWS_OUT, tr("Fit — show all blocks")).clicked() {
        tab.fit = true;
    }
}

/// A flow with only its Start block: what to do next, with quick buttons
/// that add a first block and connect it.
fn empty_canvas(ui: &mut Ui, tab: &mut FlowTab, rect: Rect) {
    let p = theme::pal(ui);
    let w = 420.0_f32.min(rect.width() - 32.0);
    let h = 132.0;
    if w < 240.0 || rect.height() < 300.0 {
        return;
    }
    let cy = (rect.center().y + 130.0).min(rect.bottom() - h / 2.0 - 56.0);
    let card = Rect::from_center_size(Pos2::new(rect.center().x, cy), vec2(w, h));
    ui.painter().rect(card, CornerRadius::same(10), p.surface, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let mut c =
        ui.new_child(UiBuilder::new().max_rect(card.shrink2(vec2(16.0, 16.0))).layout(Layout::top_down(Align::Center)));
    c.label(RichText::new(tr("Build your flow")).font(theme::semibold(15.0)).color(p.text));
    c.add_space(2.0);
    c.label(
        RichText::new(tr(
            "Add a first step after Start, or drag from an output port to an input port to connect blocks.",
        ))
        .color(p.text_muted),
    );
    c.add_space(12.0);
    let kinds = [BlockKind::Request, BlockKind::Condition, BlockKind::Loop, BlockKind::Script, BlockKind::Ai];
    let widths: f32 = kinds
        .iter()
        .map(|k| {
            c.painter().layout_no_wrap(tr(k.label()).to_string(), theme::medium(12.0), p.text).size().x
                + 14.0
                + 6.0
                + 16.0
                + 6.0
        })
        .sum();
    let mut chosen = None;
    c.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(((ui.available_width() - widths) / 2.0).max(0.0));
        for k in kinds {
            if kit::button_ex(ui, kit::Kind::Secondary, Some(kind_icon(k)), tr(k.label()), true)
                .on_hover_text(kind_description(k))
                .clicked()
            {
                chosen = Some(k);
            }
        }
    });
    if let Some(kind) = chosen {
        match tab.flow.start().map(|b| b.id.clone()) {
            Some(start) => add_after(tab, &start, "next", kind),
            None => {
                add_block(tab, kind, None);
            }
        }
    }
}

/// Zoom (between [`FIT_MIN_ZOOM`] and 100 %) and pan so the blocks are in
/// view: centered when they fit, from the left (where flows start) when not.
fn fit_view(tab: &mut FlowTab, size: Vec2) {
    tab.moved_view = false;
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for b in &tab.flow.blocks {
        min = min.min(Pos2::new(b.position.0, b.position.1));
        max = max.max(Pos2::new(b.position.0 + BLOCK_W, b.position.1 + block_height(b.kind())));
    }
    if !min.x.is_finite() || size.x <= 0.0 {
        tab.zoom = 1.0;
        tab.pan = Vec2::new(40.0, 40.0);
        return;
    }
    let margin = 48.0;
    let content = max - min;
    let zoom = ((size.x - 2.0 * margin) / content.x).min((size.y - 2.0 * margin) / content.y);
    tab.zoom = zoom.clamp(FIT_MIN_ZOOM, 1.0);
    let used = content * tab.zoom;
    let axis = |avail: f32, used: f32| if used + 2.0 * margin <= avail { (avail - used) / 2.0 } else { margin };
    tab.pan = Vec2::new(axis(size.x, used.x), axis(size.y, used.y)) - min.to_vec2() * tab.zoom;
}

// ---------------------------------------------------------------------------
// Run log
// ---------------------------------------------------------------------------

fn run_log(ui: &mut Ui, tab: &mut FlowTab) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let mut clear = false;
    // Header: title, result, steps · time, tests; outputs and the broom on the right.
    kit::bar_ex(ui, 36.0, 10.0, true, |ui| {
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(icons::TERMINAL_WINDOW).font(theme::icon_font(14.0)).color(p.text_muted));
        ui.label(kit::section_title(tr("Run log")));
        ui.add_space(2.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        let can_clear = !tab.is_running() && (!tab.run.log.is_empty() || tab.run.result.is_some());
        // The right part is measured first so the left one can give way.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let broom = ui
                .add_enabled_ui(can_clear, |ui| {
                    kit::icon_button_sm(ui, icons::BROOM, tr("Clear the log and block states"))
                })
                .inner;
            if broom.clicked() {
                clear = true;
            }
            let room_for_left = 260.0;
            if let Some(r) = &tab.run.result
                && !r.outputs.is_empty()
                && ui.available_width() > room_for_left + 140.0
            {
                for (k, v) in r.outputs.iter().take(3) {
                    let max_w = (ui.available_width() - room_for_left - 70.0).min(260.0);
                    if max_w < 90.0 {
                        break;
                    }
                    out_chip(ui, k, v, max_w, dark);
                }
                ui.label(kit::eyebrow(ui, tr("Outputs")));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if tab.is_running() {
                    let since = tab.run.started.map(elapsed).unwrap_or_default();
                    ui.add(egui::Spinner::new().size(13.0).color(p.accent));
                    ui.label(
                        RichText::new(trf("Running… {}", &[&since])).font(theme::medium(12.0)).color(p.accent_text),
                    );
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                } else if let Some(r) = &tab.run.result {
                    let (text, tone, icon) = if r.cancelled {
                        (tr("Stopped"), Tone::Warn, icons::STOP)
                    } else if r.success {
                        (tr("Succeeded"), Tone::Ok, icons::CHECK_CIRCLE)
                    } else {
                        (tr("Failed"), Tone::Err, icons::X_CIRCLE)
                    };
                    kit::pill_ex(ui, text, tone, kit::PillOpts { icon: Some(icon), ..Default::default() });
                    let summary = format!("{} · {}", steps_text(r.steps.len()), human_ms(r.duration_ms));
                    // Narrow logs keep the result only.
                    if ui.available_width() > 150.0 {
                        ui.label(kit::hint(ui, &summary));
                    }
                    let tests = r.tests_passed + r.tests_failed;
                    if tests > 0 && ui.available_width() > 80.0 {
                        let tone = if r.tests_failed > 0 { Tone::Err } else { Tone::Ok };
                        kit::tone_pill(ui, &trf("{}/{} tests", &[&r.tests_passed, &tests]), tone);
                    }
                } else if tab.run.log.is_empty() {
                    ui.add(
                        egui::Label::new(kit::hint(
                            ui,
                            tr("Run the flow to see each step, its timing and its messages here."),
                        ))
                        .truncate(),
                    );
                }
            });
        });
    });
    if clear {
        tab.run = RunState::default();
    }

    let body = ui.available_rect_before_wrap();
    if tab.run.log.is_empty() {
        let mut c = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Center)));
        if tab.is_running() {
            c.add_space((body.height() / 2.0 - 10.0).max(0.0));
            c.label(kit::hint(&c, tr("Waiting for the first block…")));
        } else {
            kit::empty_state(&mut c, icons::TERMINAL_WINDOW, tr("No run yet"), "", |ui| {
                // ⌘ ↩ runs the flow, centered on one line.
                let hint = ui.painter().layout_no_wrap(
                    tr("runs the flow").to_string(),
                    FontId::proportional(12.0),
                    p.text_muted,
                );
                let keys = kit::kbd_size(ui, "⌘").x + 6.0 + kit::kbd_size(ui, "↩").x;
                let w = keys + 8.0 + hint.size().x;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    kit::kbd(ui, "⌘");
                    kit::kbd(ui, "↩");
                    ui.add_space(2.0);
                    ui.label(kit::hint(ui, tr("runs the flow")));
                });
            });
        }
        ui.advance_cursor_after_rect(body);
        return;
    }
    // Lines stick to the bottom; only the visible ones are laid out.
    let inner = Rect::from_min_max(pos2(body.left(), body.top() + 4.0), pos2(body.right(), body.bottom() - 6.0));
    let n = tab.run.log.len();
    let content_h = n as f32 * LOG_ROW;
    let area_rect = if content_h < inner.height() {
        Rect::from_min_max(pos2(inner.left(), inner.bottom() - content_h), inner.max)
    } else {
        inner
    };
    let mut c = ui.new_child(UiBuilder::new().max_rect(area_rect).layout(Layout::top_down(Align::Min)));
    c.set_clip_rect(area_rect.intersect(ui.clip_rect()));
    let area = egui::ScrollArea::vertical().id_salt("flow-log-scroll").stick_to_bottom(true).auto_shrink([false; 2]);
    kit::show_rows(area, &mut c, LOG_ROW, n, |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in &tab.run.log[range] {
            log_row(ui, &p, line);
        }
    });
    // Lines fade out under the header.
    if content_h > inner.height() - 30.0 {
        vfade(ui.painter(), Rect::from_min_size(inner.min, vec2(inner.width(), 30.0)), p.canvas);
    }
    ui.advance_cursor_after_rect(body);
}

/// A named output of the run: `name = "value…"` on a well chip.
fn out_chip(ui: &mut Ui, key: &str, value: &Value, max_w: f32, dark: bool) -> Response {
    let p = theme::pal(ui);
    let full = match value {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    let shown = match value {
        Value::String(s) => format!("\"{}\"", crate::dialogs::ellipsize(&s.replace('\n', " "), 200)),
        v => crate::dialogs::ellipsize(&v.to_string(), 200),
    };
    let font = theme::mono(11.0);
    let mut job = LayoutJob::default();
    job.append(key, 0.0, TextFormat::simple(font.clone(), crate::highlight::json_colors(dark).key));
    job.append(" = ", 0.0, TextFormat::simple(font.clone(), p.text_muted));
    job.append(&shown, 0.0, TextFormat::simple(font, p.text));
    job.wrap = egui::text::TextWrapping::truncate_at_width((max_w - 18.0).max(20.0));
    let g = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 22.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(RADIUS_SM), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().galley(pos2(rect.left() + 9.0, rect.center().y - g.size().y / 2.0), g, p.text);
    resp.on_hover_text(full)
}

/// One run log line: clock, icon, block chip and text, on one line (the
/// whole text in a tooltip when it is cut).
fn log_row(ui: &mut Ui, p: &Palette, line: &LogLine) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, LOG_ROW), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let (icon, color) = match line.kind {
        LogKind::Step => (icons::CHECK, p.ok),
        LogKind::Info => (icons::INFO, p.info),
        LogKind::Debug => (icons::BUG, p.text_faint),
        LogKind::Warn => (icons::WARNING, p.warn),
        LogKind::Error => (icons::X_CIRCLE, p.err),
        LogKind::Done => (icons::CHECK_CIRCLE, p.ok),
    };
    let text_color = match line.kind {
        LogKind::Error => p.err,
        LogKind::Debug => p.text_faint,
        _ => p.text,
    };
    let painter = ui.painter();
    let cy = rect.center().y;
    let mut x = rect.left() + 14.0;
    painter.text(pos2(x, cy), Align2::LEFT_CENTER, clock(line.at), theme::mono(11.5), p.text_faint);
    x += 72.0 + 9.0;
    painter.text(pos2(x + 6.5, cy), Align2::CENTER_CENTER, icon, theme::icon_font(12.5), color);
    x += 13.0 + 9.0;
    if let Some(b) = &line.block {
        let g = one_line(painter, b, theme::medium(11.0), p.text, 140.0);
        let chip = Rect::from_min_size(pos2(x, cy - 9.0), vec2(g.size().x + 12.0, 18.0));
        painter.rect_filled(chip, CornerRadius::same(5), p.subtle);
        painter.galley(pos2(chip.left() + 6.0, cy - g.size().y / 2.0), g, p.text);
        x = chip.right() + 9.0;
    }
    let font =
        if line.kind == LogKind::Done && line.block.is_none() { theme::semibold(11.5) } else { theme::mono(11.5) };
    let g = one_line(painter, &line.text, font, text_color, rect.right() - 14.0 - x);
    let cut = g.elided;
    painter.galley(pos2(x, cy - g.size().y / 2.0), g, text_color);
    if cut {
        resp.on_hover_text(&line.text);
    }
}

// ---------------------------------------------------------------------------
// Inspector
// ---------------------------------------------------------------------------

/// An inspector section: a hairline across the panel, then 14 × 16 padding
/// and 10 px between its parts.
fn section<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    let top = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), top + 0.5, Stroke::new(1.0, p.border));
    // 14 × 16 padding under the 1 px top border.
    egui::Frame::new()
        .inner_margin(egui::Margin { left: 16, right: 16, top: 15, bottom: 14 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            add(ui)
        })
        .inner
}

/// The title of an inspector section, with an optional hint under it.
fn section_head(ui: &mut Ui, title: &str, help: Option<&str>) {
    ui.label(kit::section_title(title).line_height(Some(18.0)));
    if let Some(h) = help {
        rich_label(ui, h, 12.0, theme::pal(ui).text_muted);
    }
}

/// A section title with controls at its right end.
fn section_head_with(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    row(ui, 22.0, |ui| {
        ui.label(kit::section_title(title));
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
}

/// A field (label above the control) whose help may hold `**bold**` words
/// and `{{variables}}`.
fn field<R>(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        ui.label(RichText::new(label).font(theme::medium(12.0)).color(p.text).line_height(Some(17.5)));
        let r = add(ui);
        if !help.is_empty() {
            rich_label(ui, help, 11.5, p.text_muted);
        }
        r
    })
    .inner
}

/// A row of the run settings: label (and help) on the left, the control on
/// the right.
fn setting_row(ui: &mut Ui, label: &str, help: &str, control_w: f32, control: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        let label_w = (ui.available_width() - control_w - 8.0).max(80.0);
        ui.allocate_ui_with_layout(vec2(label_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(label_w);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(label).font(theme::medium(13.0)).color(p.text));
            if !help.is_empty() {
                ui.add(egui::Label::new(kit::hint(ui, help)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
}

/// The top of the inspector: a tinted 36 px tile, a small line above the
/// title, and actions on the right.
fn inspector_head(ui: &mut Ui, icon: &str, color: Color32, eyebrow: &str, title: &str, actions: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 14, top: 14, bottom: 12 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (tile, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            ui.painter().rect_filled(tile, CornerRadius::same(theme::RADIUS_CARD), theme::soft(color, 0.14));
            ui.painter().text(tile.center(), Align2::CENTER_CENTER, icon, theme::icon_font(18.0), color);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                actions(ui);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add_space(2.0);
                    ui.add(egui::Label::new(kit::eyebrow(ui, eyebrow)).truncate());
                    ui.add(egui::Label::new(RichText::new(title).font(theme::semibold(14.0)).color(p.text)).truncate());
                });
            });
        });
    });
}

/// The run state of a block on the soft fill of its tone: an icon (or a
/// spinner), the text, and an error under it.
fn run_banner(ui: &mut Ui, tone: Tone, icon: Option<&str>, text: &str, detail: Option<&str>) {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    egui::Frame::new().fill(bg).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 9)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                match icon {
                    Some(i) => {
                        ui.label(RichText::new(i).font(theme::icon_font(16.0)).color(fg));
                    }
                    None => {
                        ui.add(egui::Spinner::new().size(14.0).color(fg));
                    }
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let color = if matches!(tone, Tone::Ok | Tone::Err | Tone::Warn) { fg } else { p.text };
                    rich_label(ui, text, 12.5, color);
                    if let Some(d) = detail {
                        let c = if tone == Tone::Err { p.err } else { p.text_muted };
                        ui.add(egui::Label::new(RichText::new(d).font(theme::mono(11.5)).color(c)).wrap());
                    }
                });
            });
        },
    );
}

/// A clickable problem on the soft fill of its severity: who (bold) and
/// what, with `{{variables}}` drawn as chips.
fn problem_row(ui: &mut Ui, severity: Severity, who: Option<&str>, message: &str) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let (icon, tone) =
        if severity == Severity::Error { (icons::X_CIRCLE, Tone::Err) } else { (icons::WARNING, Tone::Warn) };
    let (fg, bg) = p.tone(tone);
    let w = ui.available_width();
    let mut job = LayoutJob::default();
    if let Some(who) = who {
        job.append(who, 0.0, TextFormat::simple(theme::semibold(12.5), p.text));
        job.append(" ", 0.0, TextFormat::simple(FontId::proportional(12.5), p.text));
    }
    append_vars(&mut job, message, FontId::proportional(12.5), p.text_muted, dark, true);
    job.wrap.max_width = (w - 10.0 - 15.0 - 8.0 - 10.0).max(40.0);
    let g = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, g.size().y + 16.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if resp.hovered() { theme::soft(fg, 0.22) } else { bg };
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        ui.painter().text(
            pos2(rect.left() + 10.0 + 7.5, rect.top() + 8.0 + 8.0),
            Align2::CENTER_CENTER,
            icon,
            theme::icon_font(14.5),
            fg,
        );
        ui.painter().galley(pos2(rect.left() + 10.0 + 15.0 + 8.0, rect.top() + 8.0), g, p.text);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    widgets::a11y_button(&resp, message);
    resp
}

fn inspector_panel(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut FlowTab) {
    egui::ScrollArea::vertical().id_salt("flow-inspector-scroll").auto_shrink([false; 2]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        match tab.selected.clone() {
            Some(Selection::Edge(i)) => edge_inspector(ui, tab, i),
            Some(Selection::Block(id)) => block_inspector(app, ui, tab, &id),
            None => flow_settings(ui, tab),
        }
        ui.add_space(8.0);
    });
}

#[derive(PartialEq)]
enum HeadAction {
    None,
    Close,
    Duplicate,
    Delete,
}

fn block_inspector(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut FlowTab, id: &str) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let Some(mut block) = tab.flow.block(id).cloned() else {
        tab.selected = None;
        return;
    };
    let kind = block.kind();
    let mut act = HeadAction::None;
    inspector_head(ui, kind_icon(kind), kind_color(kind, dark), tr(kind.label()), block.display_name(), |ui| {
        if kit::icon_button_sm(ui, icons::X, tr("Close")).clicked() {
            act = HeadAction::Close;
        }
        if kind != BlockKind::Start {
            if kit::icon_button_sm(ui, icons::TRASH, tr("Delete block")).clicked() {
                act = HeadAction::Delete;
            }
            if kit::icon_button_sm(ui, icons::COPY, tr("Duplicate (⌘D)")).clicked() {
                act = HeadAction::Duplicate;
            }
        }
    });
    match act {
        HeadAction::None => {}
        HeadAction::Close => {
            tab.selected = None;
            return;
        }
        HeadAction::Duplicate => {
            duplicate_block(tab, id);
            return;
        }
        HeadAction::Delete => {
            tab.flow.remove_block(id);
            tab.selected = None;
            tab.touch();
            return;
        }
    }

    // Run state first: it is what one looks for while a flow runs.
    let banner = match tab.run.status.get(&block.id) {
        Some(BlockStatus::Running { since }) => {
            Some((Tone::Accent, None, format!("**{}** {}", tr("Running…"), elapsed(*since)), None))
        }
        Some(BlockStatus::Done { port, failed, ms, error, tests, .. }) => {
            let (tone, icon) = if *failed { (Tone::Err, icons::X_CIRCLE) } else { (Tone::Ok, icons::CHECK_CIRCLE) };
            let head = if *failed { tr("Failed") } else { tr("Succeeded") };
            let mut text = format!("**{head}** → {} · {}", tr(port), human_ms(*ms));
            if tests.0 + tests.1 > 0 {
                text.push_str(&format!(" · {}", trf("{}/{} tests", &[&tests.0, &(tests.0 + tests.1)])));
            }
            Some((tone, Some(icon), text, error.clone()))
        }
        None => None,
    };
    if let Some((tone, icon, text, error)) = banner {
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 14 }).show(ui, |ui| {
            run_banner(ui, tone, icon, &text, error.as_deref());
        });
    }

    section(ui, |ui| {
        section_head(ui, tr("Block"), None);
        field(ui, tr("Label"), "", |ui| {
            let id = ui.make_persistent_id(("flow-label", &block.id));
            input_sm(ui, id, &mut block.label, tr(kind.label()), ui.available_width(), false, false);
        });
        let reference = format!("{{{{blocks.{}.output}}}}", reference_key(&block));
        field(ui, tr("Reference"), tr("Later blocks read this block's output with this reference."), |ui| {
            reference_tag(ui, &reference);
        });
    });

    let vars = app.var_ctx(None);
    section(ui, |ui| config_editor(app, ui, &mut block, &vars));

    let problems: Vec<&Diagnostic> =
        tab.diagnostics.iter().filter(|d| d.block.as_deref() == Some(block.id.as_str())).collect();
    if !problems.is_empty() {
        section(ui, |ui| {
            section_head_with(ui, tr("Problems"), |ui| {
                let errors = problems.iter().any(|d| d.severity == Severity::Error);
                kit::tone_pill(ui, &problems.len().to_string(), if errors { Tone::Err } else { Tone::Warn });
            });
            for d in &problems {
                problem_row(ui, d.severity, None, &d.message);
            }
        });
    }
    if let Some(text) = tab.run.live_text(&block.id) {
        let text = crate::dialogs::ellipsize(text, 20_000);
        section(ui, |ui| {
            section_head(ui, tr("Answer so far"), None);
            code_view(ui, &text, Lang::Text);
        });
    }
    if let Some(BlockStatus::Done { output, .. }) = tab.run.status.get(&block.id) {
        let text = serde_json::to_string_pretty(output.as_ref()).unwrap_or_default();
        section(ui, |ui| {
            section_head_with(ui, tr("Output"), |ui| {
                if kit::icon_button_sm(ui, icons::COPY, tr("Copy")).clicked() {
                    ui.ctx().copy_text(text.clone());
                }
            });
            ui.label(kit::hint(ui, tr("What the block produced in the last run.")));
            code_view(ui, &crate::dialogs::ellipsize(&text, 20_000), Lang::Json);
        });
    }
    let _ = p;
    if let Some(slot) = tab.flow.block_mut(&block.id)
        && *slot != block
    {
        *slot = block;
        tab.touch();
    }
}

/// `{{blocks.<name>.output}}` on a well, with a Copy button.
fn reference_tag(ui: &mut Ui, reference: &str) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(RADIUS), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let inner = Rect::from_min_max(pos2(rect.left() + 9.0, rect.top()), pos2(rect.right() - 4.0, rect.bottom()));
    let mut c = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    c.spacing_mut().item_spacing.x = 6.0;
    let copy = kit::button(
        &mut c,
        kit::Kind::Ghost,
        kit::Size::Sm,
        tr("Copy"),
        kit::ButtonOpts { icon: Some(icons::COPY), ..Default::default() },
    );
    if copy.clicked() {
        c.ctx().copy_text(reference.to_string());
    }
    let mut job = var_job(reference, theme::mono(11.5), p.text, dark, false);
    job.wrap = egui::text::TextWrapping::truncate_at_width(c.available_width().max(20.0));
    let g = c.painter().layout_job(job);
    c.painter().galley(pos2(inner.left(), rect.center().y - g.size().y / 2.0), g, p.text);
}

/// Read-only text on a well (JSON highlighted), selectable.
pub(crate) fn code_view(ui: &mut Ui, text: &str, lang: Lang) {
    let dark = ui.visuals().dark_mode;
    kit::well(ui).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut job = crate::highlight::cached(
            ui.ctx(),
            crate::highlight::HlKey { text, lang, dark, search: "", vars: None, font_size: 12.5, wrap: true },
        );
        for s in &mut job.sections {
            s.format.line_height = Some(20.0);
        }
        job.wrap.max_width = ui.available_width();
        ui.add(egui::Label::new(job).selectable(true));
    });
}

fn edge_inspector(ui: &mut Ui, tab: &mut FlowTab, i: usize) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let Some(e) = tab.flow.edges.get(i).cloned() else {
        tab.selected = None;
        return;
    };
    let name = |id: &str| tab.flow.block(id).map(|b| b.display_name().to_string()).unwrap_or_default();
    let title = format!("{} → {}", name(&e.from_block), name(&e.to_block));
    let mut close = false;
    inspector_head(ui, icons::LINK_SIMPLE, port_color(&e.from_port, &p, dark), tr("Connection"), &title, |ui| {
        close = kit::icon_button_sm(ui, icons::X, tr("Close")).clicked();
    });
    if close {
        tab.selected = None;
        return;
    }
    let mut select = None;
    let mut delete = false;
    section(ui, |ui| {
        for (head, block, port) in [(tr("From"), &e.from_block, &e.from_port), (tr("To"), &e.to_block, &e.to_port)] {
            field(ui, head, "", |ui| {
                if endpoint_row(ui, tab.flow.block(block), port).clicked() {
                    select = Some(block.clone());
                }
            });
        }
        ui.label(kit::hint(
            ui,
            &trf(
                "“{}” runs when “{}” finishes on “{}”.",
                &[&name(&e.to_block), &name(&e.from_block), &tr(&e.from_port)],
            ),
        ));
        ui.horizontal(|ui| {
            let o = kit::ButtonOpts { icon: Some(icons::TRASH), ..Default::default() };
            delete = kit::button(ui, kit::Kind::Danger, kit::Size::Sm, tr("Delete connection"), o).clicked();
            kit::kbd(ui, "⌫");
        });
    });
    if delete {
        tab.flow.edges.remove(i);
        tab.selected = None;
        tab.touch();
    } else if let Some(b) = select {
        tab.selected = Some(Selection::Block(b));
    }
}

/// One end of a connection: the block (icon and name) and the port.
fn endpoint_row(ui: &mut Ui, block: Option<&Block>, port: &str) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(8),
        if resp.hovered() { p.hover } else { p.surface },
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    let (icon, color, name) = match block {
        Some(b) => (kind_icon(b.kind()), kind_color(b.kind(), dark), b.display_name().to_string()),
        None => (icons::QUESTION, p.text_muted, tr("(missing block)").to_string()),
    };
    let tile = Rect::from_center_size(Pos2::new(rect.left() + 20.0, rect.center().y), Vec2::splat(22.0));
    painter.rect_filled(tile, CornerRadius::same(RADIUS_SM), color);
    painter.text(tile.center(), Align2::CENTER_CENTER, icon, theme::icon_font(13.0), p.on_accent);
    let pc = port_color(port, &p, dark);
    let pg = painter.layout_no_wrap(tr(port).to_string(), theme::medium(11.5), pc);
    let pill = Rect::from_min_size(
        Pos2::new(rect.right() - 10.0 - pg.size().x - 14.0, rect.center().y - 10.0),
        vec2(pg.size().x + 14.0, 20.0),
    );
    painter.rect_filled(pill, CornerRadius::same(10), theme::soft(pc, if dark { 0.18 } else { 0.11 }));
    painter.galley(pill.center() - pg.size() / 2.0, pg, pc);
    let g = one_line(painter, &name, theme::medium(13.0), p.text, pill.left() - 8.0 - (tile.right() + 10.0));
    painter.galley(Pos2::new(tile.right() + 10.0, rect.center().y - g.size().y / 2.0), g, p.text);
    widgets::a11y_button(&resp, &name);
    resp
}

/// The text of a flow input's default: as typed while it still stands for
/// the default, so text that is not JSON (stored as a string) is not shown
/// quoted after the first key press.
fn default_text(typed: Option<(String, Value)>, default: &Value) -> String {
    match typed {
        Some((text, read_as)) if read_as == *default => text,
        _ if default.is_null() => String::new(),
        _ => serde_json::to_string(default).unwrap_or_default(),
    }
}

fn flow_settings(ui: &mut Ui, tab: &mut FlowTab) {
    let p = theme::pal(ui);
    let edges = tab.flow.edges.len();
    let blocks = tab.flow.blocks.len();
    inspector_head(
        ui,
        icons::FLOW_ARROW,
        p.accent,
        &format!("{} · {}", blocks_text(blocks), connections_text(edges)),
        tr("Flow settings"),
        |_| {},
    );
    let mut flow = tab.flow.clone();

    section(ui, |ui| {
        section_head(ui, tr("Description"), None);
        // The prototype's textarea: 13 px text on 19.5 px lines.
        let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let mut job = LayoutJob::simple(buf.as_str().to_owned(), FontId::proportional(13.0), p.text, wrap);
            for s in &mut job.sections {
                s.format.line_height = Some(19.5);
            }
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let hint = RichText::new(tr("What the flow does and what it needs")).size(13.0).line_height(Some(19.5));
        ui.add(
            egui::TextEdit::multiline(&mut flow.description)
                .hint_text(hint)
                .margin(egui::Margin::symmetric(10, 8))
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter),
        );
    });

    section(ui, |ui| {
        section_head(ui, tr("Inputs"), Some(tr("Values the flow starts with; blocks read them as {{inputs.name}}.")));
        let mut remove = None;
        for (i, input) in flow.inputs.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                kit::card(ui).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if kit::icon_button_sm(ui, icons::TRASH, tr("Remove input")).clicked() {
                                remove = Some(i);
                            }
                            check_label(ui, &mut input.required, tr("Required"), 12.0);
                            let id = ui.make_persistent_id("name");
                            input_sm(ui, id, &mut input.name, tr("name"), ui.available_width(), true, false);
                        });
                    });
                    let half = ((ui.available_width() - 8.0) / 2.0).floor();
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.allocate_ui_with_layout(vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
                            field(ui, tr("Default"), "", |ui| {
                                let mut default = default_text(tab.typed_defaults.remove(&i), &input.default);
                                let id = ui.make_persistent_id("default");
                                if input_sm(ui, id, &mut default, tr("JSON value"), half, true, false).changed() {
                                    input.default = if default.trim().is_empty() {
                                        Value::Null
                                    } else {
                                        serde_json::from_str(&default).unwrap_or(Value::String(default.clone()))
                                    };
                                }
                                tab.typed_defaults.insert(i, (default, input.default.clone()));
                            });
                        });
                        ui.allocate_ui_with_layout(vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
                            field(ui, tr("Next run"), "", |ui| {
                                let value = tab.inputs.entry(input.name.clone()).or_default();
                                let id = ui.make_persistent_id("next");
                                input_sm(ui, id, value, tr("next run only"), half, true, false)
                                    .on_hover_text(tr("value for the next run"));
                            });
                        });
                    });
                });
            });
        }
        if let Some(i) = remove {
            flow.inputs.remove(i);
        }
        ui.horizontal(|ui| {
            if kit::ghost(ui, Some(icons::PLUS), tr("Add input")).clicked() {
                flow.inputs.push(FlowInput::new(format!("input{}", flow.inputs.len() + 1), Value::Null));
            }
        });
    });

    section(ui, |ui| {
        section_head(ui, tr("Run settings"), None);
        setting_row(
            ui,
            tr("Limit steps"),
            tr("Stops a run after this many block runs (protects against endless loops)."),
            95.0,
            |ui| {
                let on = flow.settings.max_steps.is_some();
                let mut n = flow.settings.max_steps.unwrap_or(ternarywire_flows::DEFAULT_MAX_STEPS);
                let r = ui
                    .add_enabled_ui(on, |ui| {
                        num_input(ui, "flow-max-steps", tr("Max steps"), &mut n, 1.0..=1_000_000.0, 72.0)
                    })
                    .inner;
                if r.changed() && on {
                    flow.settings.max_steps = Some(n);
                }
                let mut checked = on;
                if kit::check(ui, &mut checked, tr("Limit steps")).changed() {
                    flow.settings.max_steps = checked.then_some(ternarywire_flows::DEFAULT_MAX_STEPS);
                }
            },
        );
        setting_row(
            ui,
            tr("Parallel branches at once"),
            tr("Branches of a Parallel block that run at the same time; 0 runs them all at once."),
            72.0,
            |ui| {
                num_input(
                    ui,
                    "flow-max-concurrent",
                    tr("Parallel branches at once"),
                    &mut flow.settings.max_concurrent,
                    0.0..=64.0,
                    72.0,
                );
            },
        );
        check_wrap(
            ui,
            &mut flow.settings.continue_on_failure,
            tr("Continue after a failed block without a failure connection"),
        );
    });

    if flow != tab.flow {
        tab.flow = flow;
        tab.touch();
    }

    if !tab.diagnostics.is_empty() {
        let mut select = None;
        section(ui, |ui| {
            let errors = tab.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
            row(ui, 20.0, |ui| {
                ui.label(kit::section_title(tr("Problems")));
                kit::tone_pill(ui, &tab.diagnostics.len().to_string(), if errors > 0 { Tone::Err } else { Tone::Warn });
            });
            ui.label(kit::hint(ui, tr("Click a problem to select its block.")));
            for d in &tab.diagnostics {
                let who = d.block.as_deref().and_then(|b| tab.flow.block(b)).map(|b| b.display_name().to_string());
                if problem_row(ui, d.severity, who.as_deref(), &d.message).clicked() {
                    select = d
                        .block
                        .clone()
                        .map(Selection::Block)
                        .or_else(|| d.edge.filter(|e| *e < tab.flow.edges.len()).map(Selection::Edge));
                }
            }
        });
        if select.is_some() {
            tab.selected = select;
        }
    }

    section(ui, |ui| {
        section_head(ui, tr("Keyboard"), None);
        ui.spacing_mut().item_spacing.y = 0.0;
        for (keys, what) in [
            ("⌫", tr("Delete the selection")),
            ("⌘D", tr("Duplicate the block")),
            ("← ↑ → ↓", tr("Move the block (⇧ for bigger steps)")),
            ("Esc", tr("Clear the selection")),
            ("⌘ + scroll", tr("Zoom around the pointer")),
        ] {
            row(ui, 24.0, |ui| {
                kit::kbd(ui, keys);
                ui.label(RichText::new(what).size(12.0).color(p.text_muted));
            });
        }
    });
}

/// Picker for saved requests of the given protocols ("Collection / name"),
/// with the method in its color.
fn request_picker(app: &TernaryWireApp, ui: &mut Ui, id: &str, current: &mut ItemRef, want_ai: bool) -> bool {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let mut changed = false;
    let badge = app
        .ws
        .data
        .collections
        .iter()
        .filter(|c| current.collection_id.is_empty() || c.id == current.collection_id)
        .find_map(|c| c.find_request(&current.request_id).map(|i| i.request.badge()));
    let shown = request_name(app, current).unwrap_or_else(|| tr("Choose a request").into());
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, tr("Request")));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let stroke = if resp.hovered() { p.text_faint } else { p.border_strong };
        painter.rect(rect, CornerRadius::same(RADIUS_SM), p.surface, Stroke::new(1.0, stroke), StrokeKind::Inside);
        if resp.has_focus() {
            kit::focus_ring(ui, rect, RADIUS_SM);
        }
        let mut x = rect.left() + 8.0;
        if let Some(m) = &badge {
            let g = painter.layout_no_wrap(
                widgets::method_short(m).to_string(),
                theme::semibold(10.5),
                theme::method_color(m, dark),
            );
            let gw = g.size().x;
            painter.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.text);
            x += gw + 6.0;
        }
        let color = if badge.is_some() { p.text } else { p.text_muted };
        let g = one_line(painter, &shown, FontId::proportional(12.0), color, rect.right() - 26.0 - x);
        painter.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, color);
        painter.text(
            pos2(rect.right() - 12.0, rect.center().y),
            Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            theme::icon_font(12.0),
            p.text_faint,
        );
    }
    egui::Popup::menu(&resp).id(ui.make_persistent_id(("request-picker", id))).width(w.max(240.0)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for c in &app.ws.data.collections {
                for r in c.requests() {
                    let ok = if want_ai {
                        matches!(r.request, RequestKind::Ai(_))
                    } else {
                        !matches!(r.request, RequestKind::Ai(_))
                    };
                    if !ok {
                        continue;
                    }
                    let badge = r.request.badge();
                    let label = format!("{} / {}", c.name, r.name);
                    let item = kit::MenuItem {
                        method: Some(&badge),
                        label: &label,
                        selected: current.request_id == r.id,
                        ..Default::default()
                    };
                    if kit::menu_item_ex(ui, item).clicked() {
                        *current = ItemRef::new(&c.id, &r.id);
                        changed = true;
                    }
                }
            }
        });
    });
    changed
}

/// A small drop-down at full width whose menu lists `choices`; returns the
/// chosen one.
fn select_menu<T: PartialEq + Copy>(ui: &mut Ui, name: &str, current: T, choices: &[(T, &str)]) -> Option<T> {
    let text = choices.iter().find(|(v, _)| *v == current).map(|(_, l)| *l).unwrap_or_default();
    let w = ui.available_width();
    let r = kit::select_box(ui, None, text, w, true);
    widgets::a11y_name(&r, name);
    let mut chosen = None;
    egui::Popup::menu(&r).width(w.max(200.0)).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (v, label) in choices {
                let item = kit::MenuItem { checked: Some(*v == current), label, ..Default::default() };
                if kit::menu_item_ex(ui, item).clicked() {
                    chosen = Some(*v);
                }
            }
        });
    });
    chosen
}

/// A labeled 26 px `{{variable}}` field.
fn var_field(ui: &mut Ui, vars: &widgets::VarCtx, label: &str, id: &str, value: &mut String, hint: &str) {
    field(ui, label, "", |ui| var_input(ui, vars, id, value, hint));
}

fn text_field(ui: &mut Ui, label: &str, id: &str, value: &mut String, hint: &str) {
    field(ui, label, "", |ui| {
        let id = ui.make_persistent_id(id);
        input_sm(ui, id, value, hint, ui.available_width(), false, false);
    });
}

/// A small segmented control on a track as wide as the section.
fn segmented_full<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[(T, &str)]) -> bool {
    let p = theme::pal(ui);
    let track = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 26.0));
    ui.painter().rect_filled(track, CornerRadius::same(8), p.subtle);
    kit::segmented_sm(ui, current, items)
}

fn config_editor(app: &TernaryWireApp, ui: &mut Ui, block: &mut Block, vars: &widgets::VarCtx) {
    let id = block.id.clone();
    let key = reference_key(block);
    match &mut block.config {
        BlockConfig::Start(_) => {
            section_head(
                ui,
                tr("Start"),
                Some(tr(
                    "The flow starts here. Declare its inputs in the flow settings (click an empty spot of the canvas).",
                )),
            );
        }
        BlockConfig::Request(c) => {
            section_head(ui, tr("Request"), None);
            let mut saved = matches!(c.source, RequestSource::Saved(_));
            if segmented_full(ui, &mut saved, &[(true, tr("Saved request")), (false, tr("Inline request"))]) {
                c.source = if saved { RequestSource::Saved(ItemRef::default()) } else { RequestSource::default() };
            }
            match &mut c.source {
                RequestSource::Saved(r) => {
                    field(ui, tr("Request"), tr("Runs the saved request with its auth, scripts and tests."), |ui| {
                        request_picker(app, ui, &format!("flow-req-{id}"), r, false);
                    });
                }
                RequestSource::Inline { request, .. } => {
                    field(ui, tr("URL"), "", |ui| {
                        ui.horizontal(|ui| {
                            widgets::method_picker(ui, &mut request.method);
                            let mut url = request.url.clone();
                            if widgets::var_line(
                                ui,
                                ("flow-url", &id),
                                &mut url,
                                vars,
                                "https://api.example.com/items",
                                ui.available_width(),
                            )
                            .changed()
                            {
                                request.set_url(url);
                            }
                        });
                    });
                    field(ui, tr("Headers"), "", |ui| {
                        let mut h = request.headers.clone();
                        if widgets::kv_table(
                            ui,
                            ("flow-headers", &id),
                            &mut h,
                            vars,
                            KvOptions { allow_add: true, ..Default::default() },
                        ) {
                            request.headers = h;
                        }
                    });
                    field(ui, tr("Body (JSON)"), "", |ui| {
                        let mut raw = request.body.raw.clone();
                        if widgets::code_editor(ui, ("flow-body", &id), &mut raw, Lang::Json, Some(vars), 3, false)
                            .changed()
                        {
                            request.body = if raw.trim().is_empty() {
                                ternarywire_core::model::Body::default()
                            } else {
                                ternarywire_core::model::Body::raw_json(raw)
                            };
                        }
                    });
                }
            }
            field(
                ui,
                tr("Success status"),
                tr("Statuses that leave through **success**; any other status leaves through **failure**."),
                |ui| var_input(ui, vars, &format!("flow-st-{id}"), &mut c.success_status, "2xx"),
            );
        }
        BlockConfig::Condition(c) => {
            section_head(ui, tr("Condition"), Some(tr("Continues on “true” or “false”.")));
            segmented_full(
                ui,
                &mut c.mode,
                &[(ConditionMode::Compare, tr("Compare")), (ConditionMode::Expression, tr("JavaScript"))],
            );
            match c.mode {
                ConditionMode::Compare => {
                    var_field(
                        ui,
                        vars,
                        tr("Value"),
                        &format!("flow-left-{id}"),
                        &mut c.left,
                        "{{blocks.list.output.status}}",
                    );
                    field(ui, tr("Operator"), "", |ui| {
                        let ops: Vec<(CompareOp, &str)> =
                            CompareOp::ALL.iter().map(|op| (*op, tr(op.label()))).collect();
                        if let Some(op) = select_menu(ui, tr("Operator"), c.op, &ops) {
                            c.op = op;
                        }
                    });
                    if !c.op.is_unary() {
                        var_field(ui, vars, tr("Compared with"), &format!("flow-right-{id}"), &mut c.right, "200");
                    }
                }
                ConditionMode::Expression => {
                    field(ui, tr("Expression"), tr("input.status === 200 && input.json.items.length > 0"), |ui| {
                        widgets::code_editor(
                            ui,
                            ("flow-expr", &id),
                            &mut c.expression,
                            Lang::JavaScript,
                            None,
                            3,
                            false,
                        );
                    });
                }
            }
        }
        BlockConfig::Loop(c) => {
            section_head(
                ui,
                tr("Loop"),
                Some(tr(
                    "Blocks connected to “item” run once per element ({{loop.item}}, {{loop.index}}); “done” runs after the last one.",
                )),
            );
            segmented_full(ui, &mut c.mode, &[(LoopMode::ForEach, tr("For each")), (LoopMode::Count, tr("Count"))]);
            match c.mode {
                LoopMode::ForEach => var_field(
                    ui,
                    vars,
                    tr("Items"),
                    &format!("flow-items-{id}"),
                    &mut c.items,
                    "{{blocks.list.output.json.items}}",
                ),
                LoopMode::Count => var_field(ui, vars, tr("Times"), &format!("flow-count-{id}"), &mut c.count, "5"),
            }
            setting_row(ui, tr("Max iterations"), "", 90.0, |ui| {
                let mut n = c.max_iterations.unwrap_or(ternarywire_flows::DEFAULT_MAX_ITERATIONS);
                let salt = format!("flow-max-iter-{id}");
                if num_input(ui, salt.as_str(), tr("Max iterations"), &mut n, 1.0..=1_000_000.0, 90.0).changed() {
                    c.max_iterations = Some(n);
                }
            });
        }
        BlockConfig::Script(c) => {
            section_head(
                ui,
                tr("Script"),
                Some(tr("blocks, inputs, input, vars, loop, flow and pm are available; return sets the output.")),
            );
            widgets::code_editor(ui, ("flow-script", &id), &mut c.code, Lang::JavaScript, None, 10, false);
        }
        BlockConfig::Ai(c) => {
            section_head(ui, tr("AI"), None);
            let mut saved = c.request.is_some();
            if kit::checkbox(ui, &mut saved, tr("Based on a saved AI request")).changed() {
                c.request = saved.then(ItemRef::default);
            }
            if let Some(r) = &mut c.request {
                field(ui, tr("Request"), "", |ui| {
                    request_picker(app, ui, &format!("flow-ai-{id}"), r, true);
                });
            } else {
                let current = c.provider.unwrap_or_default();
                field(ui, tr("Provider"), "", |ui| {
                    let choices: Vec<(AiProvider, &str)> = AiProvider::ALL.iter().map(|p| (*p, p.label())).collect();
                    if let Some(p) = select_menu(ui, tr("Provider"), current, &choices) {
                        c.provider = Some(p);
                    }
                });
                var_field(ui, vars, tr("Model"), &format!("flow-ai-model-{id}"), &mut c.model, "gpt-5-mini");
                var_field(
                    ui,
                    vars,
                    tr("Base URL"),
                    &format!("flow-ai-url-{id}"),
                    &mut c.base_url,
                    ternarywire_ai::default_base_url(current),
                );
                var_field(ui, vars, tr("API key"), &format!("flow-ai-key-{id}"), &mut c.api_key, "{{vault:openai}}");
            }
            field(ui, tr("System prompt"), "", |ui| {
                widgets::code_editor(ui, ("flow-ai-system", &id), &mut c.system, Lang::Text, Some(vars), 2, false);
            });
            field(ui, tr("Prompt"), "", |ui| {
                widgets::code_editor(ui, ("flow-ai-prompt", &id), &mut c.prompt, Lang::Text, Some(vars), 4, false);
            });
            check_wrap(ui, &mut c.require_json, tr("Fail unless the answer is JSON"));
            check_wrap(ui, &mut c.stream, tr("Stream the answer (shown live while the flow runs)"));
        }
        BlockConfig::Parallel(_) => {
            section_head(
                ui,
                tr("Parallel"),
                Some(tr(
                    "Every block connected to “branches” starts a branch, and the branches run at the same time. Each branch starts from a copy of the variables; their changes are applied in branch order (top to bottom) once all are done.",
                )),
            );
            ui.label(kit::hint(
                ui,
                tr("Connect the ends of the branches to a Join to continue once all of them have finished."),
            ));
        }
        BlockConfig::Join(_) => {
            section_head(
                ui,
                tr("Join"),
                Some(tr(
                    "Waits until every branch of the Parallel block has finished, then continues once with the results of all branches.",
                )),
            );
            for (r, what) in [
                (format!("{{{{blocks.{key}.output.results.<block>}}}}"), tr("output of a block by its name")),
                (format!("{{{{blocks.{key}.output.items}}}}"), tr("all outputs as a list")),
                (format!("{{{{blocks.{key}.output.count}}}}"), tr("number of branches that arrived")),
            ] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    reference_tag(ui, &r);
                    ui.label(kit::hint(ui, what));
                });
            }
        }
        BlockConfig::Delay(c) => {
            section_head(ui, tr("Delay"), None);
            var_field(ui, vars, tr("Milliseconds"), &format!("flow-ms-{id}"), &mut c.ms, "1000");
        }
        BlockConfig::SetVariable(c) => {
            section_head(ui, tr("Set variable"), None);
            text_field(ui, tr("Name"), &format!("flow-var-name-{id}"), &mut c.name, "token");
            var_field(
                ui,
                vars,
                tr("Value"),
                &format!("flow-val-{id}"),
                &mut c.value,
                "{{blocks.login.output.json.token}}",
            );
            field(ui, tr("Scope"), "", |ui| {
                segmented_full(
                    ui,
                    &mut c.scope,
                    &[
                        (VariableScope::Flow, tr("Flow")),
                        (VariableScope::Environment, tr("Environment")),
                        (VariableScope::Global, tr("Global")),
                    ],
                );
            });
        }
        BlockConfig::Template(c) => {
            section_head(ui, tr("Template"), None);
            segmented_full(
                ui,
                &mut c.mode,
                &[(TemplateMode::Template, tr("Template")), (TemplateMode::Expression, tr("JavaScript"))],
            );
            match c.mode {
                TemplateMode::Template => {
                    widgets::code_editor(ui, ("flow-tpl", &id), &mut c.template, Lang::Json, Some(vars), 4, false);
                    check_wrap(ui, &mut c.parse_json, tr("Parse the result as JSON"));
                }
                TemplateMode::Expression => {
                    widgets::code_editor(ui, ("flow-tpl-js", &id), &mut c.expression, Lang::JavaScript, None, 4, false);
                }
            }
        }
        BlockConfig::Output(c) => {
            section_head(
                ui,
                tr("Output"),
                Some(tr("Named results appear in the run log and in `ternarywire flow run`.")),
            );
            text_field(ui, tr("Name"), &format!("flow-out-name-{id}"), &mut c.name, "summary");
            var_field(
                ui,
                vars,
                tr("Value"),
                &format!("flow-outv-{id}"),
                &mut c.value,
                "{{blocks.summary.output.text}}",
            );
            check_wrap(ui, &mut c.fail, tr("Mark the run as failed when reached"));
        }
        BlockConfig::Log(c) => {
            section_head(ui, tr("Log"), None);
            var_field(ui, vars, tr("Message"), &format!("flow-log-msg-{id}"), &mut c.message, "{{loop.item.name}}");
            field(ui, tr("Level"), "", |ui| {
                segmented_full(
                    ui,
                    &mut c.level,
                    &[
                        (LogLevelConfig::Info, tr("Info")),
                        (LogLevelConfig::Log, tr("Log")),
                        (LogLevelConfig::Warn, tr("Warn")),
                        (LogLevelConfig::Error, tr("Error")),
                        (LogLevelConfig::Debug, tr("Debug")),
                    ],
                );
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Opening, sidebar list
// ---------------------------------------------------------------------------

/// Opens the flow at `path` (or focuses its tab).
pub fn open_flow(app: &mut TernaryWireApp, path: PathBuf) {
    if let Some(i) = app.tabs.iter().position(|t| matches!(&t.kind, TabKind::Flow(f) if f.path.as_ref() == Some(&path)))
    {
        app.active = i;
        return;
    }
    match ternarywire_flows::storage::load(&path) {
        Ok(flow) => {
            app.add_tab(TabKind::Flow(Box::new(FlowTab::new(flow, Some(path)))));
        }
        Err(e) => app.toast(e.to_string(), true),
    }
}

/// What the sidebar list shows of a flow besides its name.
#[derive(Clone, Default)]
struct FlowMeta {
    blocks: usize,
    description: String,
}

enum ListAct {
    New,
    Open(PathBuf),
    Run(PathBuf),
    Stop(PathBuf),
    Rename(String, String),
    Duplicate(PathBuf),
    Delete(String, String, PathBuf),
}

/// A sidebar row: icon, name, a muted detail on the right, and action
/// buttons while hovered (or while its menu is open).
pub(crate) struct SidebarRow<'a> {
    pub icon: &'a str,
    pub icon_color: Color32,
    /// A spinning icon instead of `icon` (running).
    pub busy: bool,
    pub name: &'a str,
    pub detail: &'a str,
    pub active: bool,
    /// Unsaved changes (a dot after the name).
    pub dirty: bool,
    /// Icon and accessible name of an extra hover action (Run).
    pub action: Option<(&'a str, &'a str)>,
}

pub(crate) struct SidebarRowOut {
    pub row: Response,
    /// The "More actions" button, with the id its menu must use.
    pub more: Option<(Response, egui::Id)>,
    pub action: Option<Response>,
}

/// A sidebar list row drawn with [`kit::tree_row`] (28 high), with its hover
/// actions (the extra action and "More actions"), which stay while the
/// row's menu is open.
pub(crate) fn sidebar_row(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    spec: &SidebarRow,
) -> SidebarRowOut {
    let p = theme::pal(ui);
    let popup_id = ui.scope_id().with(("sidebar-row-menu", key));
    let menu_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let actions_w = 22.0 * (1.0 + spec.action.is_some() as u8 as f32);
    let row = kit::TreeRow {
        icon: Some(if spec.busy { icons::CIRCLE_NOTCH } else { spec.icon }),
        icon_color: Some(if spec.busy || spec.active { p.accent_text } else { spec.icon_color }),
        spinning: spec.busy,
        label: spec.name,
        meta: (!spec.detail.is_empty()).then_some(spec.detail),
        unsaved: spec.dirty,
        selected: spec.active,
        actions_width: actions_w,
        force_actions: menu_open,
        ..Default::default()
    };
    let (resp, actions) = kit::tree_row(ui, row, |_| {});
    widgets::a11y_button(&resp, spec.name);
    let mut out = SidebarRowOut { row: resp, more: None, action: None };
    if let Some(r) = actions {
        let mut c = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        c.spacing_mut().item_spacing.x = 0.0;
        let more = kit::icon_button_ex(&mut c, icons::DOTS_THREE, tr("More actions"), menu_open, 22.0);
        out.more = Some((more, popup_id));
        if let Some((icon, name)) = spec.action {
            out.action = Some(kit::icon_button_sm(&mut c, icon, name));
        }
    }
    out
}

/// A centered confirmation dialog; `Some(true)` when confirmed, `Some(false)`
/// when dismissed.
pub(crate) fn confirm_dialog(ctx: &egui::Context, id: &str, title: &str, body: &str, confirm: &str) -> Option<bool> {
    let style = ctx.global_style();
    let p = theme::palette(style.visuals.dark_mode);
    let mut answer = None;
    let resp = egui::Modal::new(egui::Id::unique(id))
        .frame(
            egui::Frame::popup(&style)
                .inner_margin(egui::Margin::same(20))
                .corner_radius(CornerRadius::same(theme::RADIUS_LG)),
        )
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (r, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 18.0, theme::soft(p.err, 0.12));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, icons::TRASH, theme::icon_font(18.0), p.err);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).font(theme::semibold(15.0)).color(p.text));
                    ui.add_space(2.0);
                    ui.label(RichText::new(body).color(p.text_muted));
                });
            });
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if kit::button_ex(ui, kit::Kind::Danger, Some(icons::TRASH), confirm, false).clicked() {
                    answer = Some(true);
                }
                if kit::secondary(ui, None, tr("Cancel")).clicked() {
                    answer = Some(false);
                }
            });
        });
    if answer.is_none() && resp.should_close() {
        answer = Some(false);
    }
    answer
}

/// The actions of the Flows sidebar header (drawn by `sidebar.rs` in a
/// right-to-left layout): New flow.
pub fn sidebar_actions(app: &mut TernaryWireApp, ui: &mut Ui) {
    if kit::icon_button_sm(ui, icons::PLUS, tr("New flow")).clicked() {
        new_flow(app);
    }
}

/// Opens a new, blank flow in a tab.
fn new_flow(app: &mut TernaryWireApp) {
    let n = app.sidebar.flows.as_ref().map(|f| f.len()).unwrap_or(0) + 1;
    let tab = FlowTab::blank(&format!("{} {n}", tr("Flow")));
    app.add_tab(TabKind::Flow(Box::new(tab)));
}

/// The "Flows" sidebar list: a filter, the flows (running ones spin, unsaved
/// ones have a dot) and a note at the bottom.
pub fn sidebar_list(app: &mut TernaryWireApp, ui: &mut Ui) {
    let p = theme::pal(ui);
    let meta_id = egui::Id::unique("flow-list-meta");
    let stale = ui.data(|d| d.get_temp::<Arc<HashMap<PathBuf, FlowMeta>>>(meta_id)).is_none();
    if app.sidebar.flows_dirty || app.sidebar.flows.is_none() || stale {
        let (flows, errors) = ternarywire_flows::storage::list(app.ws.root());
        for e in errors {
            app.toast(e, true);
        }
        let meta: HashMap<PathBuf, FlowMeta> = flows
            .iter()
            .map(|f| {
                (f.path.clone(), FlowMeta { blocks: f.flow.blocks.len(), description: f.flow.description.clone() })
            })
            .collect();
        ui.data_mut(|d| d.insert_temp(meta_id, Arc::new(meta)));
        app.sidebar.flows = Some(flows.into_iter().map(|f| (f.flow.id, f.flow.name, f.path)).collect());
        app.sidebar.flows_dirty = false;
    }
    let meta: Arc<HashMap<PathBuf, FlowMeta>> = ui.data(|d| d.get_temp(meta_id)).unwrap_or_default();
    let flows = app.sidebar.flows.clone().unwrap_or_default();
    let mut act: Option<ListAct> = None;
    let delete_id = egui::Id::unique("flow-list-delete");
    let rename_id = egui::Id::unique("flow-list-rename");
    let note = tr("Flows are saved in flows/ and also run with ternarywire flow run.");

    if flows.is_empty() {
        kit::empty_state(
            ui,
            icons::FLOW_ARROW,
            tr("No flows yet"),
            tr("Chain requests, conditions, loops and AI steps, then run them in one go."),
            |ui| {
                if kit::primary_icon(ui, icons::PLUS, tr("New flow")).clicked() {
                    act = Some(ListAct::New);
                }
            },
        );
    } else {
        let filter_id = egui::Id::unique("flow-list-filter");
        let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(4.0);
        let r = kit::search_field_sm(ui, "flow-filter", &mut filter, tr("Filter flows"), ui.available_width());
        widgets::a11y_name(&r, tr("Filter flows"));
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        ui.add_space(8.0);
        let active_path = app.tabs.get(app.active).and_then(|t| match &t.kind {
            TabKind::Flow(f) => f.path.clone(),
            _ => None,
        });
        let open: HashMap<PathBuf, (bool, bool)> = app
            .tabs
            .iter()
            .filter_map(|t| match &t.kind {
                TabKind::Flow(f) => f.path.clone().map(|p| (p, (f.is_running(), f.dirty()))),
                _ => None,
            })
            .collect();
        let mut renaming: Option<(String, String)> = ui.data(|d| d.get_temp(rename_id));
        let q = filter.trim().to_lowercase();
        let list_h = (ui.max_rect().bottom()
            - crate::sidebar::sidebar_footer_height(ui, note, ui.max_rect().width())
            - ui.cursor().top()
            - 4.0)
            .max(40.0);
        egui::ScrollArea::vertical().id_salt("flow-list").auto_shrink([false, true]).max_height(list_h).show(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut shown = 0;
                for (id, name, path) in &flows {
                    if !q.is_empty() && !name.to_lowercase().contains(&q) {
                        continue;
                    }
                    shown += 1;
                    if let Some((rid, buf)) = &mut renaming
                        && rid == id
                    {
                        let r = ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width()));
                        r.request_focus();
                        let escape = ui.input(|i| i.key_pressed(Key::Escape));
                        if r.lost_focus() || escape {
                            if !escape && !buf.trim().is_empty() && buf.trim() != name {
                                act = Some(ListAct::Rename(id.clone(), buf.trim().to_string()));
                            }
                            ui.data_mut(|d| {
                                d.remove_temp::<(String, String)>(rename_id);
                            });
                        }
                        continue;
                    }
                    let m = meta.get(path).cloned().unwrap_or_default();
                    let (busy, dirty) = open.get(path).copied().unwrap_or_default();
                    let detail = blocks_text(m.blocks);
                    let out = sidebar_row(
                        ui,
                        id,
                        &SidebarRow {
                            icon: icons::FLOW_ARROW,
                            icon_color: p.text_muted,
                            busy,
                            name,
                            detail: &detail,
                            active: active_path.as_ref() == Some(path),
                            dirty,
                            action: Some(if busy { (icons::STOP, tr("Stop")) } else { (icons::PLAY, tr("Run")) }),
                        },
                    );
                    let row = if m.description.trim().is_empty() {
                        out.row
                    } else {
                        out.row.on_hover_text(m.description.trim())
                    };
                    if row.clicked() {
                        act = Some(ListAct::Open(path.clone()));
                    }
                    if row.double_clicked() {
                        ui.data_mut(|d| d.insert_temp(rename_id, (id.clone(), name.clone())));
                    }
                    if out.action.is_some_and(|r| r.clicked()) {
                        act = Some(if busy { ListAct::Stop(path.clone()) } else { ListAct::Run(path.clone()) });
                    }
                    let mut menu = |ui: &mut Ui| {
                        if kit::menu_item(ui, Some(icons::FOLDER_OPEN), tr("Open"), None).clicked() {
                            act = Some(ListAct::Open(path.clone()));
                        }
                        if busy {
                            if kit::menu_item(ui, Some(icons::STOP), tr("Stop"), None).clicked() {
                                act = Some(ListAct::Stop(path.clone()));
                            }
                        } else if kit::menu_item(ui, Some(icons::PLAY), tr("Run"), None).clicked() {
                            act = Some(ListAct::Run(path.clone()));
                        }
                        if kit::menu_item(ui, Some(icons::PENCIL_SIMPLE), tr("Rename"), None).clicked() {
                            ui.data_mut(|d| d.insert_temp(rename_id, (id.clone(), name.clone())));
                        }
                        if kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked() {
                            act = Some(ListAct::Duplicate(path.clone()));
                        }
                        kit::menu_sep(ui);
                        let del = kit::MenuItem {
                            icon: Some(icons::TRASH),
                            label: tr("Delete…"),
                            danger: true,
                            ..Default::default()
                        };
                        if kit::menu_item_ex(ui, del).clicked() {
                            act = Some(ListAct::Delete(id.clone(), name.clone(), path.clone()));
                        }
                    };
                    if let Some((more, popup_id)) = &out.more {
                        egui::Popup::menu(more).id(*popup_id).show(&mut menu);
                    }
                    row.context_menu(&mut menu);
                }
                if shown == 0 {
                    ui.add_space(8.0);
                    ui.label(RichText::new(trf("No flows match “{}”", &[&filter.trim()])).color(p.text_muted));
                }
            },
        );
    }
    crate::sidebar::sidebar_footer(ui, note);

    // Delete confirmation.
    if let Some((id, name, path)) = ui.data(|d| d.get_temp::<(String, String, PathBuf)>(delete_id)) {
        match confirm_dialog(
            ui.ctx(),
            "flow-delete-confirm",
            &trf("Delete “{}”?", &[&name]),
            tr("The flow file is removed from the workspace. This cannot be undone."),
            tr("Delete flow"),
        ) {
            Some(true) => {
                ui.data_mut(|d| d.remove_temp::<(String, String, PathBuf)>(delete_id));
                if let Some(i) = app.tabs.iter().position(
                    |t| matches!(&t.kind, TabKind::Flow(f) if f.path.as_ref() == Some(&path) || f.flow.id == id),
                ) {
                    app.close_tab(i);
                }
                match ternarywire_flows::storage::delete(app.ws.root(), &id) {
                    Ok(_) => app.sidebar.flows_dirty = true,
                    Err(e) => app.toast(e.to_string(), true),
                }
            }
            Some(false) => {
                ui.data_mut(|d| d.remove_temp::<(String, String, PathBuf)>(delete_id));
            }
            None => {}
        }
    }

    match act {
        None => {}
        Some(ListAct::New) => new_flow(app),
        Some(ListAct::Open(path)) => open_flow(app, path),
        Some(ListAct::Run(path)) => {
            open_flow(app, path.clone());
            if let Some(TabKind::Flow(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
                && t.path.as_ref() == Some(&path)
            {
                t.auto_run = true;
            }
        }
        Some(ListAct::Stop(path)) => {
            for t in app.tabs.iter_mut() {
                if let TabKind::Flow(f) = &mut t.kind
                    && f.path.as_ref() == Some(&path)
                {
                    f.stop();
                }
            }
        }
        Some(ListAct::Rename(id, name)) => match ternarywire_flows::storage::rename(app.ws.root(), &id, &name) {
            Ok(path) => {
                for t in app.tabs.iter_mut() {
                    if let TabKind::Flow(f) = &mut t.kind
                        && f.flow.id == id
                    {
                        f.flow.name = name.clone();
                        f.saved.name = name.clone();
                        f.path = Some(path.clone());
                    }
                }
                app.sidebar.flows_dirty = true;
            }
            Err(e) => app.toast(e.to_string(), true),
        },
        Some(ListAct::Duplicate(path)) => match ternarywire_flows::storage::load(&path) {
            Ok(mut flow) => {
                flow.id = ternarywire_core::util::new_id();
                flow.name = trf("{} copy", &[&flow.name]);
                match ternarywire_flows::storage::save(app.ws.root(), &flow) {
                    Ok(p) => {
                        app.sidebar.flows_dirty = true;
                        open_flow(app, p);
                    }
                    Err(e) => app.toast(e.to_string(), true),
                }
            }
            Err(e) => app.toast(e.to_string(), true),
        },
        Some(ListAct::Delete(id, name, path)) => {
            ui.data_mut(|d| d.insert_temp(delete_id, (id, name, path)));
        }
    }
}

// ---------------------------------------------------------------------------
// Small controls of the redesign shared by the flow and runner views (they
// belong in kit.rs; see the report).
// ---------------------------------------------------------------------------

/// A left-to-right row of exactly `height` with its content centered
/// (`ui.horizontal` is at least `interact_size.y`, 30, high).
pub(crate) fn row<R>(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(height);
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui)
    })
    .inner
}

/// A thin vertical divider (1 px wide, the row's gap on both sides).
pub(crate) fn vdiv(ui: &mut Ui, height: f32) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, height), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, p.border));
}

/// The small input of the prototype (`.input.sm`): 26 high, 12 px text,
/// radius 6, 8 px side padding.
pub(crate) fn input_sm(
    ui: &mut Ui,
    id: egui::Id,
    text: &mut String,
    hint: &str,
    width: f32,
    mono: bool,
    right: bool,
) -> Response {
    let p = theme::pal(ui);
    let font = if mono { theme::mono(12.0) } else { FontId::proportional(12.0) };
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 26.0;
        // egui paints placeholders in the weak text color; the prototype's are faint.
        ui.visuals_mut().weak_text_color = Some(p.text_faint);
        let r = CornerRadius::same(RADIUS_SM);
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.corner_radius = r;
        w.hovered.corner_radius = r;
        w.active.corner_radius = r;
        ui.add(
            egui::TextEdit::singleline(text)
                .id(id)
                .hint_text(RichText::new(hint).color(p.text_faint).font(font.clone()))
                .font(font)
                .desired_width(width)
                .min_size(vec2(0.0, 26.0))
                .margin(egui::Margin::symmetric(8, 0))
                .align(if right { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER }),
        )
    })
    .inner
}

/// A number in a small right-aligned input: typed text is kept while the
/// field has focus; the value follows every valid number (clamped to
/// `range`), ↑ and ↓ step it.
pub(crate) fn num_input<T: egui::emath::Numeric>(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
) -> Response {
    let id = ui.make_persistent_id(salt);
    let buf_id = id.with("text");
    let show = |v: f64| if T::INTEGRAL { format!("{}", v.round() as i64) } else { format!("{v}") };
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    let mut text = if focused {
        ui.data(|d| d.get_temp::<String>(buf_id)).unwrap_or_else(|| show(value.to_f64()))
    } else {
        show(value.to_f64())
    };
    let mut r = input_sm(ui, id, &mut text, "", width, false, true);
    widgets::a11y_name(&r, name);
    let clamp = |v: f64| v.clamp(*range.start(), *range.end());
    if r.changed()
        && let Ok(v) = text.trim().parse::<f64>()
    {
        *value = T::from_f64(clamp(v));
    }
    if r.has_focus() {
        let step = ui.input(|i| (i.key_pressed(Key::ArrowUp) as i32 - i.key_pressed(Key::ArrowDown) as i32) as f64);
        if step != 0.0 {
            let v = clamp(value.to_f64() + step);
            *value = T::from_f64(v);
            text = show(v);
            r.mark_changed();
        }
        ui.data_mut(|d| d.insert_temp(buf_id, text));
    } else {
        ui.data_mut(|d| d.remove_temp::<String>(buf_id));
    }
    r
}

/// A 26 px `{{variable}}` input with its frame (`var_line` is 30 high).
fn var_input(ui: &mut Ui, vars: &widgets::VarCtx, id: &str, text: &mut String, hint: &str) -> Response {
    let p = theme::pal(ui);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
    let frame = ui.painter().add(Shape::Noop);
    let inner = rect.shrink2(vec2(0.0, 1.0));
    let mut c = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    let r = widgets::var_cell(&mut c, id, text, vars, hint, w);
    let stroke = if r.has_focus() {
        p.accent
    } else if r.hovered() {
        p.text_faint
    } else {
        p.border_strong
    };
    ui.painter().set(frame, Shape::rect_filled(rect, CornerRadius::same(RADIUS_SM), p.surface));
    ui.painter().rect_stroke(rect, CornerRadius::same(RADIUS_SM), Stroke::new(1.0, stroke), StrokeKind::Inside);
    if r.has_focus() {
        kit::focus_ring(ui, rect, RADIUS_SM);
    }
    r
}

/// A checkbox followed by a label of `size` (the label toggles it too).
fn check_label(ui: &mut Ui, on: &mut bool, text: &str, size: f32) -> Response {
    let p = theme::pal(ui);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        // Right to left: the label first.
        let label = ui.add(
            egui::Label::new(RichText::new(text).size(size).color(p.text)).sense(Sense::click()).selectable(false),
        );
        let mut r = kit::check(ui, on, text);
        if label.clicked() {
            *on = !*on;
            r.mark_changed();
        }
        r
    })
    .inner
}

/// A checkbox with a label that wraps (the label toggles it too).
fn check_wrap(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let p = theme::pal(ui);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let mut r = ui
            .vertical(|ui| {
                ui.add_space(1.0);
                kit::check(ui, on, text)
            })
            .inner;
        let label =
            ui.add(egui::Label::new(RichText::new(text).color(p.text)).sense(Sense::click()).selectable(false).wrap());
        if label.clicked() {
            *on = !*on;
            r.mark_changed();
        }
        r
    })
    .inner
}

/// Appends `text` to `job` with its `{{variables}}` drawn as chips (`bad`:
/// the undefined-variable colors).
fn append_vars(job: &mut LayoutJob, text: &str, font: FontId, color: Color32, dark: bool, bad: bool) {
    let p = theme::palette(dark);
    let (vc, vbg) = if bad { (theme::var_undefined(dark), p.var_bad_bg) } else { (p.var_ok, p.var_ok_bg) };
    let mut rest = text;
    while let Some(s) = rest.find("{{") {
        let Some(e) = rest[s..].find("}}").map(|e| s + e + 2) else { break };
        if s > 0 {
            job.append(&rest[..s], 0.0, TextFormat::simple(font.clone(), color));
        }
        let chip = TextFormat { background: vbg, ..TextFormat::simple(font.clone(), vc) };
        job.append(&rest[s..e], 0.0, chip);
        rest = &rest[e..];
    }
    if !rest.is_empty() {
        job.append(rest, 0.0, TextFormat::simple(font, color));
    }
}

/// One line of text with its `{{variables}}` as chips.
fn var_job(text: &str, font: FontId, color: Color32, dark: bool, bad: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    append_vars(&mut job, &text.replace('\n', " "), font, color, dark, bad);
    job
}

/// Wrapped text of `size` where `**words**` are semibold (in the text color)
/// and `{{variables}}` are chips.
fn rich_job(text: &str, size: f32, color: Color32, strong: Color32, dark: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, part) in text.split("**").enumerate() {
        if part.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            job.append(part, 0.0, TextFormat::simple(theme::semibold(size), strong));
        } else {
            append_vars(&mut job, part, FontId::proportional(size), color, dark, false);
        }
    }
    // CSS line height 1.45.
    for s in &mut job.sections {
        s.format.line_height = Some((size * 1.45).round());
    }
    job
}

fn rich_label(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    let p = theme::pal(ui);
    let strong = if color == p.text_muted { p.text } else { color };
    let mut job = rich_job(text, size, color, strong, ui.visuals().dark_mode);
    job.wrap.max_width = ui.available_width();
    ui.add(egui::Label::new(job).wrap())
}

/// Covers `rect` with `color`, opaque at the top and clear at the bottom.
fn vfade(painter: &egui::Painter, rect: Rect, color: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), color);
    mesh.colored_vertex(rect.right_top(), color);
    mesh.colored_vertex(rect.right_bottom(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.left_bottom(), Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(step: usize, block: &str) -> FlowMsg {
        FlowMsg::Event(FlowEvent::BlockStarted {
            step,
            block: block.into(),
            label: block.into(),
            kind: BlockKind::Delay,
            depth: 1,
            iteration: None,
            branch: Some(step.to_string()),
        })
    }

    fn finished(step: usize, block: &str) -> FlowMsg {
        FlowMsg::Event(FlowEvent::BlockFinished {
            step,
            block: block.into(),
            port: "next".into(),
            output: Arc::new(Value::Null),
            duration_ms: 1.0,
            failed: false,
            error: None,
            tests: Vec::new(),
        })
    }

    fn running(tab: &FlowTab, block: &str) -> bool {
        matches!(tab.run.status.get(block), Some(BlockStatus::Running { .. }))
    }

    #[test]
    fn parallel_blocks_run_at_once_and_ai_text_grows() {
        let mut flow = Flow::new("t");
        flow.add_block(Block::with_id("a", "A", BlockKind::Delay.default_config()));
        flow.add_block(Block::with_id("b", "B", BlockKind::Ai.default_config()));
        let mut tab = FlowTab::new(flow, None);
        // Two branches: "a" runs twice (steps 1 and 3), "b" once.
        for m in [started(1, "a"), started(2, "b"), started(3, "a")] {
            tab.on_msg(m);
        }
        assert!(running(&tab, "a") && running(&tab, "b"));
        for piece in ["Hel", "lo"] {
            tab.on_msg(FlowMsg::Event(FlowEvent::AiDelta { step: 2, block: "b".into(), text: piece.into() }));
        }
        assert_eq!(tab.run.live_text("b"), Some("Hello"));
        let last = tab.run.log.last().map(|l| (l.block.as_deref(), l.text.as_str()));
        assert_eq!(last, Some((Some("B"), "Hello")));
        tab.on_msg(finished(1, "a"));
        assert!(running(&tab, "a"), "step 3 still runs");
        tab.on_msg(finished(3, "a"));
        assert!(matches!(tab.run.status.get("a"), Some(BlockStatus::Done { .. })));
        tab.on_msg(finished(2, "b"));
        assert!(!running(&tab, "b"));
        assert_eq!(tab.run.live_text("b"), None);
        let answers = tab.run.log.iter().filter(|l| l.block.as_deref() == Some("B") && l.text == "Hello").count();
        assert_eq!(answers, 1, "the answer stays in the log");
        assert_eq!(tail("first line\nand the end", 7), "…the end");
    }

    /// Typing a default that is not JSON (`abc`) keeps the text as typed;
    /// it used to turn into `"abc"` after the first key press.
    #[test]
    fn input_defaults_keep_the_typed_text() {
        let mut typed = None;
        let mut default = Value::Null;
        for c in "ab c".chars() {
            let mut text = default_text(typed.take(), &default);
            text.push(c);
            default = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
            typed = Some((text, default.clone()));
        }
        assert_eq!(default, Value::String("ab c".into()));
        assert_eq!(default_text(typed, &default), "ab c");
        // A default set elsewhere is shown as JSON.
        assert_eq!(default_text(None, &serde_json::json!({"a": 1})), "{\"a\":1}");
        assert_eq!(default_text(Some(("x".into(), Value::Null)), &Value::from(2)), "2");
    }

    #[test]
    fn fit_keeps_blocks_readable() {
        let mut flow = Flow::new("wide");
        for i in 0..10 {
            flow.add_block(Block::with_id(format!("b{i}"), "", BlockKind::Delay.default_config()));
            flow.blocks[i].position = (i as f32 * 280.0, 0.0);
        }
        let mut tab = FlowTab::new(flow, None);
        fit_view(&mut tab, Vec2::new(900.0, 500.0));
        assert!((tab.zoom - FIT_MIN_ZOOM).abs() < 1e-6, "zoom {}", tab.zoom);
        // Too wide: shown from the first block, vertically centered.
        assert!((tab.pan.x - 48.0).abs() < 1e-3);
        let small = Flow::new("small");
        let mut tab = FlowTab::new(small, None);
        tab.flow.add_block(Block::with_id("s", "", BlockKind::Start.default_config()));
        fit_view(&mut tab, Vec2::new(900.0, 500.0));
        assert_eq!(tab.zoom, 1.0);
        let h = block_height(BlockKind::Start);
        assert!((tab.pan.x - (900.0 - BLOCK_W) / 2.0).abs() < 1e-3 && (tab.pan.y - (500.0 - h) / 2.0).abs() < 1e-3);
        assert_eq!(clock(Duration::from_millis(61_234)), "01:01.234");
    }
}
