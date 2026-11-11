//! Modal dialogs, the console panel, session persistence and export helpers.
//!
//! Every dialog is drawn with [`Modal`]: a title row with a close button, a
//! padded [`body`] and a [`footer`] whose buttons sit on the right (main
//! action rightmost). Escape closes any dialog.

use std::path::PathBuf;
use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Galley, Key, Layout, Margin, Modifiers, Rect, Response, RichText,
    Sense, Stroke, StrokeKind, TextFormat, Ui, UiBuilder, Vec2, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use ternarywire_convert::codegen::Language;
use ternarywire_core::model::{HttpVersion, Id, Item, RequestItem};
use ternarywire_core::postman::ExportOptions;
use ternarywire_core::prepared::{PrepareContext, prepare};
use ternarywire_core::settings::{
    AppSettings, ClientCertificate, Language as UiLanguage, ResponseLayout, ThemePreference,
};
use ternarywire_core::variables::Resolver;

use crate::app::{ItemRef, Tab, TabKind, TernaryWireApp};
use crate::i18n::{tr, trf};
use crate::state::WsState;
use crate::{icons, kit, theme, widgets};

pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

/// Title, method badge and unsaved marker for a tab.
pub fn tab_title(tab: &Tab, ws: &WsState) -> (String, Option<String>, bool) {
    let lookup = |r: &ItemRef| {
        ws.data
            .collection(&r.collection)
            .and_then(|c| c.find_request(&r.item))
            .map(|i| (i.name.clone(), i.request.badge()))
    };
    let deleted = || (tr("(deleted)").to_string(), None, false);
    match &tab.kind {
        TabKind::Request(t) => match (&t.location, &t.scratch) {
            (Some(r), _) => lookup(r).map(|(n, b)| (n, Some(b), false)).unwrap_or_else(deleted),
            (None, Some(i)) => (i.name.clone(), Some(i.request.badge()), true),
            _ => ("Request".into(), None, false),
        },
        TabKind::WebSocket(t) => match (&t.location, &t.scratch) {
            (Some(r), _) => lookup(r).map(|(n, _)| (n, Some("WS".into()), false)).unwrap_or_else(deleted),
            (None, Some(i)) => (i.name.clone(), Some("WS".into()), true),
            _ => ("WebSocket".into(), None, false),
        },
        TabKind::Container(c) => {
            let name = ws
                .data
                .collection(&c.collection)
                .map(|col| match &c.folder {
                    Some(f) => col.find(f).map(|i| i.name().to_string()).unwrap_or_default(),
                    None => col.name.clone(),
                })
                .unwrap_or_default();
            (name, None, false)
        }
        TabKind::Environment(e) => match &e.env {
            None => (tr("Globals").to_string(), None, false),
            Some(id) => (ws.data.environment(id).map(|e| e.name.clone()).unwrap_or_default(), None, false),
        },
        TabKind::Runner(r) => {
            (ws.data.collection(&r.collection).map(|c| c.name.clone()).unwrap_or_default(), None, false)
        }
        TabKind::Spec(t) => (t.title(), None, t.dirty()),
        TabKind::Capture(c) => (format!("{}{}", tr("Capture"), if c.is_running() { " ●" } else { "" }), None, false),
        TabKind::Flow(f) => (f.flow.name.clone(), None, f.dirty()),
        TabKind::Webhook(w) => (format!("Webhook{}", if w.is_running() { " ●" } else { "" }), None, false),
        other @ (TabKind::Mqtt(_) | TabKind::Grpc(_) | TabKind::SocketIo(_) | TabKind::Mcp(_) | TabKind::Ai(_)) => {
            match (other.location(), other.scratch()) {
                (Some(r), _) => lookup(r).map(|(n, b)| (n, Some(b), false)).unwrap_or_else(deleted),
                (None, Some(i)) => (i.name.clone(), Some(i.request.badge()), true),
                _ => ("Request".into(), None, false),
            }
        }
        TabKind::Mock(m) => (
            format!("Mock · {}", ws.data.collection(&m.collection).map(|c| c.name.clone()).unwrap_or_default()),
            None,
            false,
        ),
    }
}

/// Icon shown in front of the title of tabs without a method badge.
pub fn tab_icon(tab: &Tab) -> &'static str {
    match &tab.kind {
        TabKind::Container(c) if c.folder.is_some() => icons::FOLDER_SIMPLE,
        TabKind::Container(_) => icons::STACK,
        TabKind::Environment(e) if e.env.is_none() => icons::GLOBE_SIMPLE,
        TabKind::Environment(_) => icons::GLOBE,
        TabKind::Runner(_) => icons::PLAY_CIRCLE,
        TabKind::Spec(_) => icons::FILE_CODE,
        TabKind::Capture(_) => icons::BROADCAST,
        TabKind::Flow(_) => icons::FLOW_ARROW,
        TabKind::Webhook(_) => icons::WEBHOOKS_LOGO,
        TabKind::Mock(_) => icons::CUBE,
        _ => icons::FILE,
    }
}

// ---------------------------------------------------------------------------
// Dialog chrome: frame, header, body, footer and the small parts dialogs share
// (`.modal`, `.mh`, `.mb`, `.mf` in design/redesign/styles.css)
// ---------------------------------------------------------------------------

/// Side padding of a dialog's header, body and footer.
const PAD: f32 = 18.0;
/// Top of a dialog that is not centered (the scrim's top padding).
const TOP: f32 = 70.0;
/// Height of the window a dialog leaves free, in total.
const SPARE_H: f32 = 100.0;
/// A footer with buttons: 12 + 30 + 12 under its 1 px hairline (one with
/// only text is as tall as its content).
const FOOTER_H: f32 = 55.0;

/// What a dialog wants after a frame.
pub enum Outcome {
    Keep,
    Close,
}

/// A dialog window. Configure it, then [`Modal::show`] its content, made of
/// [`body`], [`footer`] and (for two-pane dialogs) [`split`].
pub struct Modal {
    id: &'static str,
    width: f32,
    title: Option<String>,
    subtitle: Option<String>,
    icon: Option<&'static str>,
    sticky: bool,
    center: bool,
    mono_subtitle: bool,
}

/// Where the open dialog's content may go: its top and the lowest point
/// before the window cuts it off. Kept while its content is drawn.
#[derive(Clone, Copy)]
struct Geometry {
    bottom: f32,
}

fn geometry_id() -> egui::Id {
    egui::Id::unique("ternarywire-dialog-geometry")
}

impl Modal {
    pub fn new(id: &'static str, width: f32) -> Modal {
        Modal { id, width, title: None, subtitle: None, icon: None, sticky: false, center: false, mono_subtitle: false }
    }

    /// Title row with a close button. Dialogs without one draw their own top.
    pub fn title(mut self, title: impl Into<String>) -> Modal {
        self.title = Some(title.into());
        self
    }

    /// Muted line under the title.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Modal {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// The subtitle is code (a JSON path): mono 12.5.
    pub fn mono_subtitle(mut self) -> Modal {
        self.mono_subtitle = true;
        self
    }

    /// Icon tile in front of the title.
    pub fn icon(mut self, icon: &'static str) -> Modal {
        self.icon = Some(icon);
        self
    }

    /// A click outside does not close it (forms whose input would be lost).
    pub fn sticky(mut self) -> Modal {
        self.sticky = true;
        self
    }

    /// Pinned 70 px from the top of the window (the default), so a search
    /// field stays put while the list under it grows and shrinks.
    pub fn top(mut self) -> Modal {
        self.center = false;
        self
    }

    /// Centered in the window: alerts and dialogs of a fixed size.
    pub fn center(mut self) -> Modal {
        self.center = true;
        self
    }

    pub fn show(self, ctx: &egui::Context, add: impl FnOnce(&mut Ui) -> Outcome) -> Outcome {
        let style = ctx.global_style();
        let dark = style.visuals.dark_mode;
        let p = theme::palette(dark);
        let screen = ctx.content_rect();
        let width = self.width.min(screen.width() - 40.0).max(260.0);
        // `max-height: calc(100% - 100px)` of the scrim, whose content box
        // starts under its 70 px top padding when the dialog is not centered.
        let max_h = (screen.height() - SPARE_H - if self.center { 0.0 } else { TOP }).max(200.0);
        let id = egui::Id::unique(("ternarywire-dialog", self.id));
        let mut area = egui::Modal::default_area(id);
        if !self.center {
            area = area.anchor(Align2::CENTER_TOP, vec2(0.0, TOP));
        }
        let frame = egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(theme::RADIUS_DIALOG))
            .shadow(style.visuals.window_shadow);
        let mut close = false;
        let resp = egui::Modal::new(id)
            .area(area)
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(if dark { 128 } else { 82 }))
            .show(ctx, |ui| {
                ui.set_width(width - 2.0);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                // The prototype's lists end without egui's fade-out at the edges.
                ui.spacing_mut().scroll.fade.strength = 0.0;
                let top = ui.cursor().top();
                ui.data_mut(|d| d.insert_temp(geometry_id(), Geometry { bottom: top + max_h - 2.0 }));
                if let Some(title) = &self.title {
                    let sub_font = if self.mono_subtitle { theme::mono(12.5) } else { FontId::proportional(12.0) };
                    close = header(ui, title, self.subtitle.as_deref().map(|s| (s, sub_font)), self.icon);
                }
                add(ui)
            });
        let escape =
            resp.is_top_modal && !resp.any_popup_open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if close || escape || (!self.sticky && resp.backdrop_response.clicked()) {
            return Outcome::Close;
        }
        resp.inner
    }
}

/// Height left for the content of the open dialog above its footer, before
/// the dialog would be taller than the window allows.
pub(crate) fn room(ui: &Ui) -> f32 {
    let bottom = ui.data(|d| d.get_temp::<Geometry>(geometry_id())).map(|g| g.bottom).unwrap_or(f32::INFINITY);
    (bottom - ui.cursor().top() - FOOTER_H).max(80.0)
}

/// CSS line boxes (line-height 1.45): text laid out like the prototype's,
/// one row (`…` when cut) or wrapped at `max_w`.
pub(crate) fn text_box(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32, wrap: bool) -> Arc<Galley> {
    let line = font.size * 1.45;
    let format = TextFormat { font_id: font, color, line_height: Some(line), ..Default::default() };
    let mut job = LayoutJob::single_section(text.to_string(), format);
    job.wrap = if wrap {
        TextWrapping::wrap_at_width(max_w.max(1.0))
    } else {
        TextWrapping { max_width: max_w.max(1.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') }
    };
    ui.painter().layout_job(job)
}

/// Text with code spans (`<span class=mono>`, 12.5 px) wrapped at `max_w`:
/// `template` (already translated) with its `{}` filled in by `code`, in
/// CSS line boxes of `size` × 1.45.
pub(crate) fn code_text(ui: &Ui, template: &str, code: &[&str], size: f32, color: Color32, max_w: f32) -> Arc<Galley> {
    // A code span's own line box (12.5 × 1.45) makes its line taller.
    let line = Some((size * 1.45).max(12.5 * 1.45));
    let plain = TextFormat { font_id: FontId::proportional(size), color, line_height: line, ..Default::default() };
    let mono = TextFormat { font_id: theme::mono(12.5), color, line_height: line, ..Default::default() };
    let mut job = LayoutJob::default();
    for (i, part) in template.split("{}").enumerate() {
        if i > 0
            && let Some(c) = code.get(i - 1)
        {
            job.append(c, 0.0, mono.clone());
        }
        if !part.is_empty() {
            job.append(part, 0.0, plain.clone());
        }
    }
    job.wrap = TextWrapping::wrap_at_width(max_w.max(1.0));
    ui.painter().layout_job(job)
}

/// Title row: a 30 px icon tile, the title (15 semibold) with its muted
/// subtitle, and the close button. Returns true when it was pressed.
fn header(ui: &mut Ui, title: &str, subtitle: Option<(&str, FontId)>, icon: Option<&str>) -> bool {
    let p = theme::pal(ui);
    let w = ui.available_width();
    let (title_h, sub_h) = ((15.0f32 * 1.45).round(), (12.0f32 * 1.45).round());
    let text_h = title_h + subtitle.as_ref().map_or(0.0, |_| 2.0 + sub_h);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 16.0 + text_h.max(30.0) + 12.0), Sense::hover());
    let inner = Rect::from_min_max(rect.min + vec2(PAD, 16.0), rect.max - vec2(PAD, 12.0));
    let cy = inner.center().y;
    let mut x = inner.left();
    if let Some(icon) = icon {
        let tile = Rect::from_min_size(pos2(x, cy - 15.0), Vec2::splat(30.0));
        ui.painter().rect_filled(tile, CornerRadius::same(8), p.accent_soft);
        ui.painter().text(tile.center(), Align2::CENTER_CENTER, icon, theme::icon_font(16.0), p.accent_text);
        x += 40.0;
    }
    let close_rect = Rect::from_center_size(pos2(inner.right() - 14.0, cy), Vec2::splat(28.0));
    let text_w = (close_rect.left() - 10.0 - x).max(40.0);
    let top = cy - text_h / 2.0;
    let tg = text_box(ui, title, theme::semibold(15.0), p.text, text_w, false);
    ui.painter().galley(pos2(x, top), tg, p.text);
    if let Some((s, font)) = subtitle {
        let sg = text_box(ui, s, font, p.text_muted, text_w, false);
        ui.painter().galley(pos2(x, top + title_h + 2.0), sg, p.text_muted);
    }
    let name = format!("{} (Esc)", tr("Close"));
    place(ui, close_rect, |ui: &mut Ui| kit::icon_button(ui, icons::X, &name)).clicked()
}

/// The padded content of a dialog (4 18 16); it scrolls when the window is
/// too short for it.
pub fn body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    body_with(ui, 4.0, add)
}

/// [`body`] with another top padding (dialogs without a header).
pub(crate) fn body_with<R>(ui: &mut Ui, top: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let max = room(ui);
    egui::ScrollArea::vertical()
        .id_salt("dialog-body")
        .max_height(max)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin { left: PAD as i8, right: PAD as i8, top: top as i8, bottom: 16 })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = ui.ctx().global_style().spacing.item_spacing;
                    ui.set_width(ui.available_width());
                    add(ui)
                })
                .inner
        })
        .inner
}

/// The button bar of a dialog: a hairline, then a `well` strip (12 18).
/// Hints go first (left); the buttons go in [`trailing`], main action first
/// so that it ends up rightmost.
pub fn footer<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    // As tall as its tallest item (measured on the previous pass): 30 with
    // buttons, 18 with key hints only.
    let id = ui.scope_id().with("dialog-footer-h");
    let content_h: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(30.0);
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), content_h + 25.0));
    let r = theme::RADIUS_DIALOG - 1;
    ui.painter().rect_filled(rect, CornerRadius { nw: 0, ne: 0, sw: r, se: r }, p.well);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    let inner = Rect::from_min_max(rect.min + vec2(PAD, 13.0), rect.max - vec2(PAD, 12.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    child.spacing_mut().item_spacing = vec2(8.0, 6.0);
    let out = add(&mut child);
    let used = child.min_rect().height().round().max(16.0);
    if (used - content_h).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, used));
        ui.ctx().request_discard("dialog footer height");
    }
    ui.advance_cursor_after_rect(rect);
    out
}

/// The right-aligned end of a row; widgets added first end up rightmost.
pub fn trailing<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.with_layout(Layout::right_to_left(Align::Center), add).inner
}

/// Adds a widget filling `rect` (centered in it) without moving `ui`'s
/// cursor, unlike `Ui::put`: for widgets inside space already allocated.
pub(crate) fn place(ui: &mut Ui, rect: Rect, widget: impl egui::Widget) -> Response {
    let layout = Layout::centered_and_justified(egui::Direction::LeftToRight);
    ui.new_child(UiBuilder::new().max_rect(rect).layout(layout)).add(widget)
}

/// Runs `add` in a child ui over `rect` without moving `ui`'s cursor: for
/// drawing in space allocated (or about to be allocated) as a whole.
pub(crate) fn in_rect<R>(ui: &mut Ui, rect: Rect, layout: Layout, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(layout));
    add(&mut child)
}

/// Two panes of `height` with nothing around them (`.mb.dl-flush`): a list
/// on the `well` fill (`left`, `left_w` wide, then a hairline) and the
/// content (`right`), each in its own clipped child ui.
pub fn split(ui: &mut Ui, height: f32, left_w: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let left_rect = Rect::from_min_size(rect.min, vec2(left_w, height));
    let right_rect = Rect::from_min_max(pos2(left_rect.right() + 1.0, rect.top()), rect.max);
    ui.painter().rect_filled(left_rect, 0.0, p.well);
    ui.painter().vline(left_rect.right() + 0.5, rect.y_range(), Stroke::new(1.0, p.border));
    let spacing = ui.ctx().global_style().spacing.item_spacing;
    let mut l = ui.new_child(UiBuilder::new().max_rect(left_rect).layout(Layout::top_down(Align::Min)));
    l.set_clip_rect(left_rect.intersect(ui.clip_rect()));
    l.spacing_mut().item_spacing = vec2(spacing.x, 0.0);
    left(&mut l);
    let mut r = ui.new_child(UiBuilder::new().max_rect(right_rect).layout(Layout::top_down(Align::Min)));
    r.set_clip_rect(right_rect.intersect(ui.clip_rect()));
    r.spacing_mut().item_spacing = vec2(spacing.x, 0.0);
    right(&mut r);
}

/// The header of a pane in a two-pane dialog (`.lh`): 38 high, a section
/// title and a faint count on the right, a hairline under it.
pub(crate) fn pane_head(ui: &mut Ui, title: &str, count: &str) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let cy = rect.center().y;
    let cg = ui.painter().layout_no_wrap(count.to_string(), FontId::proportional(11.0), p.text_faint);
    let cx = rect.right() - 12.0 - cg.size().x;
    ui.painter().galley(pos2(cx, cy - cg.size().y / 2.0), cg, p.text_faint);
    let tg = line_galley(ui, title, theme::semibold(12.5), cx - rect.left() - 20.0);
    ui.painter().galley(pos2(rect.left() + 12.0, cy - tg.size().y / 2.0), tg, p.text);
}

/// The header row of a dialog's content pane (`height` 48 or 42): drawn by
/// `add` from 16 px in, with a hairline under it.
pub(crate) fn pane_bar<R>(ui: &mut Ui, height: f32, right_pad: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let inner = Rect::from_min_max(rect.min + vec2(16.0, 0.0), rect.max - vec2(right_pad, 1.0));
    let out = in_rect(ui, inner, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui)
    });
    ui.advance_cursor_after_rect(rect);
    out
}

/// Height a result list may take before the window cuts off the dialog,
/// over a footer `footer_h` high.
fn list_room(ui: &Ui, footer_h: f32) -> f32 {
    room(ui) + FOOTER_H - footer_h
}

/// Space a dialog body may use before the window cuts it off.
pub(crate) fn fit_height(ctx: &egui::Context, reserved: f32, max: f32) -> f32 {
    (ctx.content_rect().height() - reserved).max(140.0).min(max)
}

/// One line of text laid out to at most `max_w`, with `…` when cut. Drawn
/// with the color passed to `painter.galley`.
pub(crate) fn line_galley(ui: &Ui, text: &str, font: FontId, max_w: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, Color32::PLACEHOLDER));
    job.wrap =
        TextWrapping { max_width: max_w.max(1.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}

/// `↑ ↓ Navigate   ↩ Open   esc Close` key hints of a footer.
fn key_hints(ui: &mut Ui, hints: &[(&[&str], &str)]) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.x = 5.0;
    for (i, (keys, what)) in hints.iter().enumerate() {
        // `gap: 5px` around an 8 px spacer between pairs.
        if i > 0 {
            ui.add_space(13.0);
        }
        for k in *keys {
            kit::kbd(ui, k);
        }
        ui.label(RichText::new(*what).size(12.0).color(p.text_muted));
    }
}

/// Muted 12 px text with an icon before it (`.row.hint` of footers).
fn icon_hint(ui: &mut Ui, icon: &str, text: &str) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.x = 8.0;
    ui.label(RichText::new(icon).font(theme::icon_font(12.0)).color(p.text_muted));
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.text_muted)).truncate());
}

/// A single-line text input as tall as a button (30, 10 px padding inside
/// a 1 px border).
pub(crate) fn input(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(Margin::symmetric(11, 0))
        .min_size(vec2(0.0, 30.0))
        .align(Align2::LEFT_CENTER)
}

/// A one-line input as in the prototype (`.input`): 30 high (13 px text) or
/// `small` (26 high, 12 px); `mono` for names, URLs and paths. Add it with
/// [`add_input`].
pub(crate) fn line_edit<'t>(ui: &Ui, text: &'t mut String, hint: &str, small: bool, mono: bool) -> egui::TextEdit<'t> {
    let p = theme::pal(ui);
    let font = match (small, mono) {
        (true, true) => theme::mono(12.0),
        (true, false) => FontId::proportional(12.0),
        (false, true) => theme::mono(12.5),
        (false, false) => FontId::proportional(13.0),
    };
    // The padding plus the 1 px border.
    let (h, pad) = if small { (26.0, 9) } else { (30.0, 11) };
    egui::TextEdit::singleline(text)
        .font(font.clone())
        .hint_text(RichText::new(hint).font(font).color(p.text_faint))
        .margin(Margin::symmetric(pad, 0))
        .min_size(vec2(0.0, h))
        .align(Align2::LEFT_CENTER)
}

/// Placeholders of the text fields added to `ui` in the faint text color
/// (`::placeholder`): egui paints a hint in `weak_text_color`, whatever
/// color its text has.
pub(crate) fn faint_hints(ui: &mut Ui) {
    ui.visuals_mut().weak_text_color = Some(theme::pal(ui).text_faint);
}

/// Adds an input made with [`line_edit`]: rounded to its size, with the
/// accent focus ring.
pub(crate) fn add_input(ui: &mut Ui, edit: egui::TextEdit<'_>, small: bool) -> Response {
    let radius = if small { theme::RADIUS_SM } else { theme::RADIUS };
    let r = ui
        .scope(|ui| {
            let cr = CornerRadius::same(radius);
            faint_hints(ui);
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.corner_radius = cr;
            w.hovered.corner_radius = cr;
            w.active.corner_radius = cr;
            // egui paints placeholders in the weak text color.
            ui.visuals_mut().weak_text_color = Some(theme::pal(ui).text_faint);
            ui.spacing_mut().interact_size.y = if small { 26.0 } else { 30.0 };
            ui.add(edit)
        })
        .inner;
    if r.has_focus() {
        kit::focus_ring(ui, r.rect, radius);
    }
    r
}

/// A small input named `name` for screen readers.
pub(crate) fn small_input(ui: &mut Ui, text: &mut String, hint: &str, name: &str, width: f32, mono: bool) -> Response {
    let edit = line_edit(ui, text, hint, true, mono).desired_width(width);
    let r = add_input(ui, edit, true);
    widgets::a11y_name(&r, name);
    r
}

/// A masked small input with a show/hide button inside it (a proxy
/// password, the passphrase of a certificate, a secret's value).
pub(crate) fn secret_input(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    hint: &str,
    width: f32,
) -> Response {
    let id = ui.make_persistent_id(id);
    let mut reveal: bool = ui.data(|d| d.get_temp(id).unwrap_or(false));
    let edit = line_edit(ui, text, hint, true, false)
        .password(!reveal)
        .margin(Margin { left: 9, right: 30, top: 0, bottom: 0 })
        .desired_width(width);
    let r = add_input(ui, edit, true);
    let (icon, name) = if reveal { (icons::EYE_SLASH, tr("Hide")) } else { (icons::EYE, tr("Show")) };
    let eye = Rect::from_center_size(pos2(r.rect.right() - 15.0, r.rect.center().y), Vec2::splat(22.0));
    if place(ui, eye, |ui: &mut Ui| kit::icon_button_sm(ui, icon, name)).clicked() {
        reveal = !reveal;
        ui.data_mut(|d| d.insert_temp(id, reveal));
    }
    r
}

/// A path shown in a recessed box (`.dl-pathbox`): 30 high, mono 12.
fn path_box(ui: &mut Ui, path: Option<&str>, empty: &str, width: f32) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(theme::RADIUS), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let (text, font, color) = match path {
        Some(t) => (t, theme::mono(12.0), p.text),
        None => (empty, FontId::proportional(12.0), p.text_faint),
    };
    let g = line_galley(ui, text, font, rect.width() - 20.0);
    let elided = g.elided;
    ui.painter().galley(pos2(rect.left() + 10.0, rect.center().y - g.size().y / 2.0), g, color);
    if elided { resp.on_hover_text(text) } else { resp }
}

/// An on/off switch, named `name` for screen readers.
pub(crate) fn toggle(ui: &mut Ui, on: &mut bool, name: &str) -> Response {
    kit::switch(ui, on, name)
}

/// The tone of a [`banner`].
#[derive(Clone, Copy)]
pub(crate) enum Tone {
    Info,
    Ok,
    #[allow(dead_code)]
    Warn,
    Err,
}

/// A message with an icon on a soft fill (errors, notes, results).
pub(crate) fn banner(ui: &mut Ui, tone: Tone, text: &str) {
    let (tone, icon) = match tone {
        Tone::Info => (kit::Tone::Info, icons::INFO),
        Tone::Ok => (kit::Tone::Ok, icons::CHECK_CIRCLE),
        Tone::Warn => (kit::Tone::Warn, icons::WARNING),
        Tone::Err => (kit::Tone::Err, icons::WARNING_CIRCLE),
    };
    kit::banner(ui, tone, icon, text);
}

/// A colored result next to the button that produced it (`Resolved (24
/// characters)`, `Connected · KV v2`): an icon and 12 px text.
pub(crate) fn inline_result(ui: &mut Ui, ok: bool, text: &str) {
    let p = theme::pal(ui);
    let color = if ok { p.ok } else { p.err };
    let icon = if ok { icons::CHECK_CIRCLE } else { icons::WARNING_CIRCLE };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(icon).font(theme::icon_font(12.0)).color(color));
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).truncate());
    });
}

/// A row of a settings navigation (`.dl-set-nav .ni`) or of a list pane
/// (`.dl-lrow`): 32 high, radius 7. The selected row is a raised white
/// card. `nav` rows have medium muted labels with the icon in the accent
/// when selected; list rows have plain labels and a faint text on the right.
fn pane_row(ui: &mut Ui, icon: Option<&str>, label: &str, trail: Option<&str>, selected: bool, nav: bool) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    widgets::a11y_tab(&resp, label, selected);
    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(theme::RADIUS);
        let hovered = resp.hovered();
        if selected {
            raised(ui, rect, radius);
        } else if hovered {
            ui.painter().rect_filled(rect, radius, p.hover);
        }
        let cy = rect.center().y;
        let mut x = rect.left() + 10.0;
        let text_c = if nav && !selected && !hovered { p.text_muted } else { p.text };
        if let Some(icon) = icon {
            let (size, color) =
                if nav { (15.0, if selected { p.accent_text } else { text_c }) } else { (13.0, p.text_muted) };
            let g = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(size), color);
            let w = g.size().x;
            ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, color);
            x += w + if nav { 9.0 } else { 8.0 };
        }
        let mut right = rect.right() - 10.0;
        if let Some(t) = trail {
            let g = line_galley(ui, t, FontId::proportional(11.5), 120.0);
            let w = g.size().x;
            ui.painter().galley(pos2(right - w, cy - g.size().y / 2.0), g, p.text_faint);
            right -= w + 8.0;
        }
        let font = if nav { theme::medium(12.5) } else { FontId::proportional(12.5) };
        let g = line_galley(ui, label, font, right - x);
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, text_c);
        if resp.has_focus() {
            ui.painter().rect_stroke(rect, radius, Stroke::new(1.5, p.accent), StrokeKind::Inside);
        }
    }
    resp
}

/// The selected row of a list on a `well`: a white card with a 1 px ring
/// and the card shadow. In the dark theme only the fill: the prototype's
/// ring goes with the card shadow, which the dark theme has not.
fn raised(ui: &Ui, rect: Rect, radius: CornerRadius) {
    let p = theme::pal(ui);
    if ui.visuals().dark_mode {
        ui.painter().rect_filled(rect, radius, p.surface);
        return;
    }
    let shadow = egui::Shadow { offset: [0, 1], blur: 2, spread: 0, color: Color32::from_black_alpha(10) };
    ui.painter().add(shadow.as_shape(rect, radius));
    ui.painter().rect(rect, radius, p.surface, Stroke::new(1.0, p.border), StrokeKind::Outside);
}

/// A row of a navigation or picker list: optional icon, label, optional
/// faint text on the right; the selected row is raised.
pub(crate) fn nav_item(ui: &mut Ui, icon: Option<&str>, label: &str, trail: Option<&str>, selected: bool) -> Response {
    pane_row(ui, icon, label, trail, selected, false)
}

/// A bordered list of rows (`.dl-vlist`): radius 9 on the surface.
pub(crate) fn vlist<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// One row of a [`vlist`] (`.dl-vrow`): `height` high, 12 px in from the
/// left and 10 from the right, a hairline above all but the first. Hovered
/// rows get the `well` fill, selected ones the accent tint; `add` learns
/// whether the row is hovered (to show its actions). Returns what `add`
/// returns and the click response of the whole row (under its widgets).
pub(crate) fn vrow<R>(
    ui: &mut Ui,
    (index, count): (usize, usize),
    height: f32,
    selected: bool,
    add: impl FnOnce(&mut Ui, bool) -> R,
) -> (R, Response) {
    let p = theme::pal(ui);
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
    let row = ui.interact(rect, ui.scope_id().with(("vrow", rect.left() as i32, rect.top() as i32)), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    let r = theme::RADIUS_CARD - 1;
    let corners = CornerRadius {
        nw: if index == 0 { r } else { 0 },
        ne: if index == 0 { r } else { 0 },
        sw: if index + 1 == count { r } else { 0 },
        se: if index + 1 == count { r } else { 0 },
    };
    if selected {
        ui.painter().rect_filled(rect, corners, p.accent_soft);
    } else if hovered {
        ui.painter().rect_filled(rect, corners, p.well);
    }
    if index > 0 {
        ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    }
    let inner = Rect::from_min_max(rect.min + vec2(12.0, 0.0), rect.max - vec2(10.0, 0.0));
    let out = in_rect(ui, inner, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        add(ui, hovered)
    });
    ui.advance_cursor_after_rect(rect);
    (out, row)
}

/// An icon in a 28 px tile (`.dl-tile`): on the `well` with a border, or
/// (`accent`) on the accent tint.
pub(crate) fn tile(ui: &mut Ui, icon: &str, accent: bool) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    let radius = CornerRadius::same(theme::RADIUS);
    if accent {
        ui.painter().rect_filled(rect, radius, p.accent_soft);
    } else {
        ui.painter().rect(rect, radius, p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    }
    let color = if accent { p.accent_text } else { p.text_muted };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(15.0), color);
}

/// A name over a faint second line (a provider and its kind), as one
/// block `width` wide, centered in a row.
pub(crate) fn two_lines(ui: &mut Ui, title: &str, title_font: FontId, sub: &str, width: f32) -> Response {
    let p = theme::pal(ui);
    let tg = text_box(ui, title, title_font, p.text, width, false);
    let sg = text_box(ui, sub, FontId::proportional(11.0), p.text_faint, width, false);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, tg.size().y + sg.size().y), Sense::hover());
    let title_h = tg.size().y;
    ui.painter().galley(rect.min, tg, p.text);
    ui.painter().galley(rect.min + vec2(0.0, title_h), sg, p.text_faint);
    resp
}

/// A `{{variable}}` chip in the color of secrets (mono 11.5).
pub(crate) fn secret_chip(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::mono(11.5), p.info);
    let (rect, _) = ui.allocate_exact_size(vec2(g.size().x + 6.0, 17.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), p.info_soft);
    ui.painter().galley(pos2(rect.left() + 3.0, rect.center().y - g.size().y / 2.0), g, p.info);
}

/// Red buttons kit.rs has no kind for: `solid` (red fill, the Delete of an
/// alert) or quiet (no frame, red text: Don't save, Clear all). `small`
/// makes it 24 high instead of 30.
pub(crate) fn danger_button(ui: &mut Ui, solid: bool, small: bool, icon: Option<&str>, text: &str) -> Response {
    let p = theme::pal(ui);
    // Padding plus the 1 px border of `.btn`, as in `kit::button`.
    let (h, font, icon_size, pad, gap, radius) = if small {
        (24.0, 12.0, 13.0, 9.0, 5.0, theme::RADIUS_SM)
    } else {
        (30.0, 12.5, 15.0, 13.0, 6.0, theme::RADIUS)
    };
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::medium(font), Color32::PLACEHOLDER);
    let ig =
        icon.map(|i| ui.painter().layout_no_wrap(i.to_string(), theme::icon_font(icon_size), Color32::PLACEHOLDER));
    let content = g.size().x + ig.as_ref().map_or(0.0, |i| i.size().x + gap);
    let (rect, resp) = ui.allocate_exact_size(vec2(content + pad * 2.0, h), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    if ui.is_rect_visible(rect) {
        let hot = enabled && (resp.hovered() || resp.is_pointer_button_down_on());
        let cr = CornerRadius::same(radius);
        let (fill, color) = if solid {
            (if hot { darker(p.err) } else { p.err }, p.on_accent)
        } else {
            (if hot { p.hover } else { Color32::TRANSPARENT }, p.err)
        };
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        ui.painter().rect_filled(rect, cr, fade(fill));
        if resp.has_focus() {
            let ring = CornerRadius::same(radius + 2);
            ui.painter().rect_stroke(rect.expand(2.0), ring, Stroke::new(1.5, p.accent), StrokeKind::Outside);
        }
        let mut x = rect.center().x - content / 2.0;
        if let Some(ig) = ig {
            let w = ig.size().x;
            ui.painter().galley(pos2(x, rect.center().y - ig.size().y / 2.0), ig, fade(color));
            x += w + gap;
        }
        ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, fade(color));
    }
    resp
}

/// `c` a little darker (the hover of a filled button).
fn darker(c: Color32) -> Color32 {
    let d = |v: u8| (v as f32 * 0.88).round() as u8;
    Color32::from_rgb(d(c.r()), d(c.g()), d(c.b()))
}

/// Small centered "nothing here" note for panes too small for
/// `kit::empty_state`.
pub(crate) fn compact_empty(ui: &mut Ui, icon: &str, title: &str, text: &str) {
    let p = theme::pal(ui);
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new(icon).font(theme::icon_font(22.0)).color(p.text_faint));
        ui.label(RichText::new(title).font(theme::semibold(13.0)).color(p.text));
        if !text.is_empty() {
            ui.add(egui::Label::new(RichText::new(text).size(12.5).color(p.text_muted)).wrap());
        }
    });
}

/// The small segmented control (26 high) from owned labels.
pub(crate) fn segmented_sm<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[(T, String)]) -> bool {
    let items: Vec<(T, &str)> = items.iter().map(|(v, l)| (*v, l.as_str())).collect();
    kit::segmented_sm(ui, current, &items)
}

/// The color of an environment's dot, from its place in the list (the
/// workspace format has no color of its own).
pub(crate) fn env_color(p: &theme::Palette, index: usize) -> Color32 {
    [p.ok, p.warn, p.accent, p.info, p.text_muted][index % 5]
}

// ---------------------------------------------------------------------------
// Console
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleFilter {
    #[default]
    All,
    Logs,
    Network,
    Errors,
}

/// The console drawer at the bottom of the workbench card: a 36 px header
/// (title, filter, search, copy / clear / close) over 26 px mono rows.
pub fn console_ui(app: &mut TernaryWireApp, ui: &mut Ui) {
    use ternarywire_runner::ConsoleKind as K;
    let search_id = egui::Id::unique("console-search");
    let mut search: String = ui.data(|d| d.get_temp(search_id).unwrap_or_default());
    let problems = app.console.iter().filter(|l| matches!(l.kind, K::Error | K::Warn)).count();
    let wide = ui.available_width() > 760.0;
    let has_lines = !app.console.is_empty();
    let (mut copy, mut clear, mut close) = (false, false, false);
    let errors = if problems > 0 { format!("{} {problems}", tr("Errors")) } else { tr("Errors").to_string() };
    kit::bar(ui, 36.0, true, |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.scope(|ui| {
            let p = theme::pal(ui);
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(icons::TERMINAL_WINDOW).font(theme::icon_font(13.0)).color(p.text_muted));
            ui.label(kit::section_title(tr("Console")).color(p.text));
        });
        kit::segmented_sm(
            ui,
            &mut app.console_filter,
            &[
                (ConsoleFilter::All, tr("All")),
                (ConsoleFilter::Logs, tr("Logs")),
                (ConsoleFilter::Network, tr("Network")),
                (ConsoleFilter::Errors, &errors),
            ],
        );
        trailing(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if kit::icon_button_sm(ui, icons::X, &format!("{} (⌥⌘C)", tr("Close console"))).clicked() {
                close = true;
            }
            if ui
                .add_enabled_ui(has_lines, |ui| kit::icon_button_sm(ui, icons::BROOM, tr("Clear console")))
                .inner
                .clicked()
            {
                clear = true;
            }
            if ui.add_enabled_ui(has_lines, |ui| kit::icon_button_sm(ui, icons::COPY, tr("Copy lines"))).inner.clicked()
            {
                copy = true;
            }
            if wide {
                kit::search_field_sm(ui, "console-filter", &mut search, tr("Filter"), 200.0);
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
    if close {
        app.console_open = false;
    }
    if clear {
        app.console.clear();
    }
    let filter = app.console_filter;
    let needle = search.trim().to_lowercase();
    let rows: Vec<&ternarywire_runner::ConsoleLine> = app
        .console
        .iter()
        .filter(|l| match filter {
            ConsoleFilter::All => true,
            ConsoleFilter::Logs => !matches!(l.kind, K::Network),
            ConsoleFilter::Network => matches!(l.kind, K::Network),
            ConsoleFilter::Errors => matches!(l.kind, K::Error | K::Warn),
        })
        .filter(|l| {
            needle.is_empty() || l.message.to_lowercase().contains(&needle) || l.source.to_lowercase().contains(&needle)
        })
        .collect();
    if copy {
        let text = rows.iter().map(|l| format!("[{}] {}: {}", l.phase, l.source, l.message)).collect::<Vec<_>>();
        ui.ctx().copy_text(text.join("\n"));
    }
    if rows.is_empty() {
        ui.add_space((ui.available_height() / 2.0 - 34.0).max(8.0));
        if app.console.is_empty() {
            compact_empty(
                ui,
                icons::TERMINAL_WINDOW,
                tr("No console output yet"),
                tr("console.log() from scripts and every request you send show up here."),
            );
        } else {
            compact_empty(ui, icons::FUNNEL, tr("Nothing matches this filter"), "");
        }
        return;
    }
    let area = egui::ScrollArea::vertical().id_salt("console").stick_to_bottom(true).auto_shrink([false; 2]);
    kit::show_rows(area, ui, 26.0, rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for l in &rows[range] {
            console_row(ui, l, wide);
        }
    });
}

/// One console line: time (86 px), kind icon in its tone, the message, and
/// the request (and script phase) on the right. Errors and warnings are
/// tinted.
fn console_row(ui: &mut Ui, l: &ternarywire_runner::ConsoleLine, wide: bool) {
    use ternarywire_runner::ConsoleKind as K;
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    // Network lines end with `→ 404 Not Found · …`: the icon takes the status color.
    let status = l
        .message
        .rsplit_once(" → ")
        .and_then(|(_, rest)| rest.get(..3))
        .and_then(|code| code.parse::<u16>().ok())
        .filter(|s| (100..600).contains(s));
    let (icon, color, tint) = match l.kind {
        K::Error => (icons::WARNING_CIRCLE, p.err, Some(p.err_soft)),
        K::Warn => (icons::WARNING, p.warn, Some(p.warn_soft)),
        K::Network => (icons::GLOBE_SIMPLE, status.map(|s| theme::status_color(s, dark)).unwrap_or(p.text_muted), None),
        K::Info => (icons::INFO, p.text_muted, None),
        K::Debug => (icons::BUG, p.text_faint, None),
        K::Log => (icons::TERMINAL, p.text_muted, None),
    };
    let painter = ui.painter();
    if let Some(bg) = tint {
        painter.rect_filled(rect, 0.0, bg);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, p.faint_row());
    }
    let cy = rect.center().y;
    let mut x = rect.left() + 12.0;
    let time = chrono::DateTime::from_timestamp_millis(l.at_ms)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string())
        .unwrap_or_default();
    painter.text(pos2(x, cy), Align2::LEFT_CENTER, time, theme::mono(12.0), p.text_faint);
    x += 86.0 + 10.0;
    let ig = painter.layout_no_wrap(icon.to_string(), theme::icon_font(12.5), color);
    let iw = ig.size().x;
    painter.galley(pos2(x, cy - ig.size().y / 2.0), ig, color);
    x += iw + 10.0;
    let mut right = rect.right() - 12.0;
    let source = if l.phase.is_empty() || l.phase == "network" || l.source.is_empty() {
        l.source.clone()
    } else {
        format!("{} · {}", l.source, l.phase)
    };
    if !source.is_empty() {
        let max: f32 = if wide { 240.0 } else { 120.0 };
        let g = line_galley(ui, &source, FontId::proportional(11.0), max.min((right - x) * 0.4));
        let w = g.size().x;
        ui.painter().galley(pos2(right - w, cy - g.size().y / 2.0), g, p.text_faint);
        right -= w + 10.0;
    }
    let first = l.message.lines().next().unwrap_or("");
    let multiline = l.message.trim_end().contains('\n');
    let shown = if multiline { format!("{first} …") } else { first.to_string() };
    let g = line_galley(ui, &shown, theme::mono(12.0), right - x);
    let elided = g.elided || multiline;
    ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, p.text);
    let resp = if elided { resp.on_hover_text(RichText::new(&l.message).monospace()) } else { resp };
    resp.context_menu(|ui| {
        if kit::menu_item(ui, Some(icons::COPY), tr("Copy message"), None).clicked() {
            ui.ctx().copy_text(l.message.clone());
        }
    });
}

// ---------------------------------------------------------------------------
// Session (open tabs), restored after restart or a crash
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum SavedTab {
    Request {
        location: Option<(Id, Id)>,
        scratch: Option<RequestItem>,
        /// The unsaved request was edited since its tab opened (see
        /// [`TernaryWireApp::tab_has_unsaved_work`]); older files did not say.
        #[serde(default = "edited_unknown")]
        edited: bool,
    },
    Websocket {
        location: Option<(Id, Id)>,
        scratch: Option<RequestItem>,
        #[serde(default = "edited_unknown")]
        edited: bool,
    },
    Container {
        collection: Id,
        folder: Option<Id>,
    },
    Environment {
        env: Option<Id>,
    },
    Spec {
        path: PathBuf,
        /// Edits not saved to the file yet.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unsaved: Option<String>,
    },
    Flow {
        path: PathBuf,
    },
    /// Any other protocol tab; the tab type follows the request.
    Proto {
        location: Option<(Id, Id)>,
        scratch: Option<RequestItem>,
        #[serde(default = "edited_unknown")]
        edited: bool,
    },
}

/// Closing a restored tab asks when the session does not say whether it
/// was edited.
fn edited_unknown() -> bool {
    true
}

#[derive(Serialize, Deserialize, Default)]
struct Session {
    tabs: Vec<SavedTab>,
    active: usize,
}

fn session_path(app: &TernaryWireApp) -> PathBuf {
    app.ws.local_dir().join("session.json")
}

/// Entry of the workspace's secret store with the credentials taken out of
/// unsaved tabs in session.json: request id → field ([`credential_fields`]) →
/// value.
const SESSION_SECRETS: &str = "session:unsaved-credentials";

type Stash = std::collections::BTreeMap<Id, std::collections::BTreeMap<String, String>>;

/// Header names whose values are usually credentials (Authorization, Cookie,
/// X-API-Key, X-Auth-Token, …). Erring on the side of caution costs nothing:
/// the value comes back from the secret store.
fn credential_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["auth", "cookie", "token", "key", "secret", "password", "session", "credential", "signature"]
        .iter()
        .any(|w| n.contains(w))
}

/// The text fields of an unsaved request that may hold a credential, named
/// the same when saving and restoring, and whether the field's own name
/// says it is one (a header such as Authorization, a password parameter).
fn credential_fields(item: &mut RequestItem) -> Vec<(String, bool, &mut String)> {
    use ternarywire_core::model::{KeyValue, RequestKind};
    fn list<'a>(out: &mut Vec<(String, bool, &'a mut String)>, name: &str, kvs: &'a mut [KeyValue]) {
        for (i, kv) in kvs.iter_mut().enumerate() {
            let named = name == "header" && credential_header(&kv.key);
            out.push((format!("{name}.{i}"), named, &mut kv.value));
        }
    }
    let mut out = Vec::new();
    let auth = match &mut item.request {
        RequestKind::Http(h) => {
            out.push(("url".to_string(), false, &mut h.url));
            list(&mut out, "param", &mut h.params);
            list(&mut out, "path", &mut h.path_vars);
            list(&mut out, "header", &mut h.headers);
            list(&mut out, "urlencoded", &mut h.body.urlencoded);
            for (i, f) in h.body.form_data.iter_mut().enumerate() {
                out.push((format!("form.{i}"), false, &mut f.value));
            }
            out.push(("raw".to_string(), false, &mut h.body.raw));
            out.push(("graphql".to_string(), false, &mut h.body.graphql.query));
            out.push(("graphql-variables".to_string(), false, &mut h.body.graphql.variables));
            &mut h.auth
        }
        RequestKind::WebSocket(w) => {
            out.push(("url".to_string(), false, &mut w.url));
            list(&mut out, "param", &mut w.params);
            list(&mut out, "header", &mut w.headers);
            &mut w.auth
        }
        other => other.auth_mut(),
    };
    let sensitive = auth.kind.sensitive_params();
    for (k, v) in auth.params.iter_mut() {
        if let serde_json::Value::String(s) = v {
            out.push((format!("auth.{k}"), sensitive.contains(&k.as_str()), s));
        }
    }
    out
}

/// Unsaved tabs are written without literal credentials (only `{{…}}`
/// references survive), like collection files: header values that look
/// like credentials, sensitive auth parameters and fields holding the
/// value of a secret variable or vault entry go to `stash` (kept in the
/// secret store), so the tab is whole again after a restart.
fn scrub(item: Option<&RequestItem>, secrets: &[String], stash: &mut Stash) -> Option<RequestItem> {
    let mut item = item?.clone();
    let mut taken = std::collections::BTreeMap::new();
    for (name, named, value) in credential_fields(&mut item) {
        if value.is_empty() || ternarywire_core::variables::is_pure_reference(value) {
            continue;
        }
        if named || crate::app::redact(value, secrets) != *value {
            taken.insert(name, std::mem::take(value));
        }
    }
    if !taken.is_empty() {
        stash.insert(item.id.clone(), taken);
    }
    Some(item)
}

/// Puts the credentials [`scrub`] took out of `item` back.
fn unstash(mut item: RequestItem, stash: &Stash) -> RequestItem {
    if let Some(taken) = stash.get(&item.id) {
        for (name, _, value) in credential_fields(&mut item) {
            if let Some(v) = taken.get(&name)
                && value.is_empty()
            {
                *value = v.clone();
            }
        }
    }
    item
}

/// Writes the open tabs (unless unchanged since the last write).
pub fn save_session(app: &mut TernaryWireApp) {
    let env = app.ws.data.active_environment.clone();
    let secrets = app.secret_values(None, env.as_deref(), &app.ws.scopes_for(None), None);
    let mut stash = Stash::new();
    let stash = &mut stash;
    let saved: Vec<Option<SavedTab>> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| match &t.kind {
            TabKind::Request(r) => Some(SavedTab::Request {
                location: r.location.as_ref().map(|l| (l.collection.clone(), l.item.clone())),
                scratch: scrub(r.scratch.as_ref(), &secrets, stash),
                edited: app.tab_has_unsaved_work(i),
            }),
            TabKind::WebSocket(r) => Some(SavedTab::Websocket {
                location: r.location.as_ref().map(|l| (l.collection.clone(), l.item.clone())),
                scratch: scrub(r.scratch.as_ref(), &secrets, stash),
                edited: app.tab_has_unsaved_work(i),
            }),
            TabKind::Container(c) => {
                Some(SavedTab::Container { collection: c.collection.clone(), folder: c.folder.clone() })
            }
            TabKind::Environment(e) => Some(SavedTab::Environment { env: e.env.clone() }),
            TabKind::Spec(t) => {
                Some(SavedTab::Spec { path: t.path.clone(), unsaved: t.unsaved_text().map(str::to_string) })
            }
            TabKind::Flow(f) => f.path.clone().map(|path| SavedTab::Flow { path }),
            other => other
                .location()
                .map(|l| (l.collection.clone(), l.item.clone()))
                .map(Some)
                .or_else(|| other.scratch().map(|_| None))
                .map(|location| SavedTab::Proto {
                    location,
                    scratch: scrub(other.scratch(), &secrets, stash),
                    edited: app.tab_has_unsaved_work(i),
                }),
        })
        .collect();
    // Written before the session file, which needs them to be whole.
    match serde_json::to_string(stash) {
        Ok(json) if !stash.is_empty() => app.ws.secrets.set(SESSION_SECRETS, &json),
        _ => app.ws.secrets.remove(SESSION_SECRETS),
    }
    // `active` indexes the saved tabs: the active tab, or the closest saved
    // one before it when it is not saved itself.
    let mut tabs = Vec::new();
    let mut active = 0;
    for (i, t) in saved.into_iter().enumerate() {
        if let Some(t) = t {
            if i <= app.active {
                active = tabs.len();
            }
            tabs.push(t);
        }
    }
    let s = Session { tabs, active };
    if let Ok(json) = serde_json::to_vec_pretty(&s)
        && json != app.last_session
        && ternarywire_core::storage::write_atomic(&session_path(app), &json).is_ok()
    {
        app.last_session = json;
    }
}

pub fn restore_session(app: &mut TernaryWireApp) {
    let Ok(bytes) = std::fs::read(session_path(app)) else { return };
    let Ok(s) = serde_json::from_slice::<Session>(&bytes) else { return };
    let stash: Stash =
        app.ws.secrets.get(SESSION_SECRETS).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    // A restored unsaved tab that was edited before keeps counting as edited.
    let restore_edited = |app: &mut TernaryWireApp, id: u64, edited: bool| {
        if edited {
            app.mark_scratch_edited(id);
        }
    };
    // Saved tabs that cannot be restored are skipped, so the active one is
    // found by tab id rather than by position.
    let mut active_id = None;
    for (pos, t) in s.tabs.into_iter().enumerate() {
        let before = app.tabs.len();
        match t {
            SavedTab::Request { location: Some((c, i)), .. } => {
                if app.ws.data.collection(&c).and_then(|col| col.find(&i)).is_some() {
                    app.add_tab(TabKind::Request(Box::new(crate::request_tab::RequestTab::saved(ItemRef {
                        collection: c,
                        item: i,
                    }))));
                }
            }
            SavedTab::Request { location: None, scratch: Some(item), edited } => {
                let item = unstash(item, &stash);
                let id = app.add_tab(TabKind::Request(Box::new(crate::request_tab::RequestTab::scratch(item))));
                restore_edited(app, id, edited);
            }
            SavedTab::Websocket { location: Some((c, i)), .. } => {
                if app.ws.data.collection(&c).and_then(|col| col.find(&i)).is_some() {
                    app.add_tab(TabKind::WebSocket(Box::new(crate::ws_tab::WsTab::saved(ItemRef {
                        collection: c,
                        item: i,
                    }))));
                }
            }
            SavedTab::Websocket { location: None, scratch: Some(item), edited } => {
                let item = unstash(item, &stash);
                let id = app.add_tab(TabKind::WebSocket(Box::new(crate::ws_tab::WsTab::scratch(item))));
                restore_edited(app, id, edited);
            }
            SavedTab::Container { collection, folder } => {
                if app.ws.data.collection(&collection).is_some() {
                    app.open_container(collection, folder);
                }
            }
            SavedTab::Environment { env }
                if env.as_ref().map(|e| app.ws.data.environment(e).is_some()).unwrap_or(true) =>
            {
                app.open_environment(env);
            }
            SavedTab::Spec { path, unsaved } if path.is_file() => {
                crate::spec_tab::open_spec(app, path.clone());
                if let Some(text) = unsaved
                    && let Some(TabKind::Spec(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
                    && t.path == path
                {
                    t.restore_unsaved(text);
                }
            }
            SavedTab::Flow { path } if path.is_file() => crate::flow_tab::open_flow(app, path),
            SavedTab::Proto { location, scratch, edited } => {
                let location = location.map(|(collection, item)| ItemRef { collection, item });
                let scratch = scratch.map(|item| unstash(item, &stash));
                let edited = edited && location.is_none();
                let kind = match (&location, &scratch) {
                    (Some(l), _) => app
                        .ws
                        .data
                        .collection(&l.collection)
                        .and_then(|c| c.find_request(&l.item))
                        .map(|r| r.request.clone()),
                    (None, Some(item)) => Some(item.request.clone()),
                    (None, None) => None,
                };
                if let Some(tab) = kind.and_then(|k| TabKind::for_item(location, scratch, &k)) {
                    let id = app.add_tab(tab);
                    restore_edited(app, id, edited);
                }
            }
            _ => {}
        }
        if pos <= s.active && app.tabs.len() > before {
            active_id = app.tabs.get(app.active).map(|t| t.id);
        }
    }
    app.active = active_id.and_then(|id| app.tab_index(id)).unwrap_or(0).min(app.tabs.len().saturating_sub(1));
}

// ---------------------------------------------------------------------------
// Export and code generation helpers
// ---------------------------------------------------------------------------

pub fn export_collection(app: &mut TernaryWireApp, cid: &str, format: crate::sidebar::ExportFormat) {
    let Some(c) = app.ws.data.collection(cid).cloned() else { return };
    let slug = ternarywire_core::util::slugify(&c.name);
    let (value, name, note) = match format {
        crate::sidebar::ExportFormat::Postman => {
            let (v, report) = ternarywire_core::postman::export_collection(&c, ExportOptions::default());
            let mut note = String::new();
            if report.redacted > 0 {
                note = format!("{} secret values were left out of the export.", report.redacted);
            }
            for w in report.warnings {
                note.push_str(&format!(" {w}"));
            }
            (v, format!("{slug}.postman_collection.json"), note)
        }
        crate::sidebar::ExportFormat::OpenApi => {
            (ternarywire_convert::openapi::export(&c), format!("{slug}.openapi.json"), String::new())
        }
    };
    if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() {
        match std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap_or_default()) {
            Ok(()) => app.toast(trf("Exported to {}. {}", &[&path.display(), &note]), false),
            Err(e) => app.toast(trf("Export failed: {}", &[&e]), true),
        }
    }
}

fn docs_options(
    app: &TernaryWireApp,
    c: &ternarywire_core::model::Collection,
) -> ternarywire_convert::docs::DocsOptions {
    let mut variables = app.ws.scopes_for(app.ws.data.collection(&c.id).map(|x| x.id.as_str()));
    // Secret values never go into documentation.
    let mut secret_vars: Vec<ternarywire_core::variables::Variable> = app.ws.data.globals.clone();
    if let Some(env) = app.ws.data.active_environment() {
        secret_vars.extend(env.variables.iter().cloned());
    }
    secret_vars.extend(c.variables.iter().cloned());
    ternarywire_convert::docs::redact_secret_variables(&mut variables, &secret_vars);
    ternarywire_convert::docs::DocsOptions {
        variables,
        snippet_languages: vec![Language::Curl, Language::Python, Language::JavaScriptFetch],
        title: Some(c.name.clone()),
    }
}

/// Generates static HTML documentation for a collection and opens it.
pub fn preview_docs(app: &mut TernaryWireApp, c: &ternarywire_core::model::Collection) {
    let opts = docs_options(app, c);
    // A new folder only this user can write to (not a predictable name).
    let dir = tempfile::Builder::new()
        .prefix(&format!("ternarywire-docs-{}-", ternarywire_core::util::slugify(&c.name)))
        .tempdir()
        .map(|d| d.keep())
        .map_err(|e| e.to_string());
    match dir.and_then(|dir| ternarywire_convert::docs::write_html_site(c, &opts, &dir).map_err(|e| e.to_string())) {
        Ok(index) => {
            let _ = open::that(index);
        }
        Err(e) => app.toast(trf("Docs: {}", &[&e]), true),
    }
}

/// Writes documentation (HTML site or Markdown) to a chosen location.
pub fn export_docs(app: &mut TernaryWireApp, cid: &str, markdown: bool) {
    let Some(c) = app.ws.data.collection(cid).cloned() else { return };
    let opts = docs_options(app, &c);
    if markdown {
        let name = format!("{}.md", ternarywire_core::util::slugify(&c.name));
        if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() {
            match std::fs::write(&path, ternarywire_convert::docs::to_markdown(&c, &opts)) {
                Ok(()) => app.toast(trf("Documentation written to {}", &[&path.display()]), false),
                Err(e) => app.toast(trf("Docs: {}", &[&e]), true),
            }
        }
    } else if let Some(dir) = rfd::FileDialog::new().set_title("Folder for the documentation site").pick_folder() {
        match ternarywire_convert::docs::write_html_site(&c, &opts, &dir) {
            Ok(index) => {
                app.toast(trf("Documentation written to {}", &[&index.display()]), false);
                let _ = open::that(index);
            }
            Err(e) => app.toast(trf("Docs: {}", &[&e]), true),
        }
    }
}

pub fn export_environment(app: &mut TernaryWireApp, id: &str) {
    let Some(e) = app.ws.data.environment(id).cloned() else { return };
    let (v, report) = ternarywire_core::postman::export_environment(&e, ExportOptions::default());
    let name = format!("{}.postman_environment.json", ternarywire_core::util::slugify(&e.name));
    if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() {
        match std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default()) {
            Ok(()) => app.toast(
                if report.redacted > 0 {
                    trf("Exported; {} secret values left out", &[&report.redacted])
                } else {
                    tr("Exported").to_string()
                },
                false,
            ),
            Err(e) => app.toast(trf("Export failed: {}", &[&e]), true),
        }
    }
}

/// Generates a snippet for a request item with current variables applied.
pub fn codegen_item(app: &TernaryWireApp, item: &RequestItem, collection: Option<&str>, lang: Language) -> String {
    let Some(http) = item.http() else { return String::new() };
    let scopes = {
        let mut s = app.ws.scopes_for(collection);
        if let Some(c) = collection.and_then(|c| app.ws.data.collection(c))
            && let Some(chain) = c.ancestors(&item.id)
        {
            for a in chain {
                if let ternarywire_core::model::Ancestor::Folder(f) = a {
                    // Folder variables override the collection's, types included.
                    let values = ternarywire_core::variables::to_map(&f.variables);
                    s.types.collection.retain(|k, _| !values.contains_key(k));
                    s.types.collection.extend(ternarywire_core::variables::to_types(&f.variables));
                    s.collection.extend(values);
                }
            }
        }
        s
    };
    let auth = if http.auth.is_inherit() {
        collection
            .and_then(|c| app.ws.data.collection(c))
            .and_then(|c| c.effective_auth(&item.id).map(|e| e.auth.clone()))
            .unwrap_or_else(ternarywire_core::auth::Auth::none)
    } else {
        http.auth.clone()
    };
    let vault = app.ws.vault();
    let resolver = Resolver::new(&scopes).with_vault(vault.as_ref());
    let ctx = PrepareContext {
        resolver,
        auth: &auth,
        auth_owner_id: Some(&item.id),
        defaults: &app.settings.request,
        base_dir: Some(app.ws.root()),
    };
    let mut prepared = prepare(http, &ctx);
    if let Some(p) = ternarywire_auth::preview(&prepared) {
        prepared = p.request;
    }
    ternarywire_convert::codegen::generate_with(
        lang,
        &prepared,
        ternarywire_convert::codegen::Options { skip_default_headers: true },
    )
}

pub fn codegen_for(app: &TernaryWireApp, cid: &str, rid: &str, lang: Language) -> Option<String> {
    let item = app.ws.data.collection(cid)?.find_request(rid)?.clone();
    Some(codegen_item(app, &item, Some(cid), lang))
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

pub enum Prompt {
    NewCollection,
    NewEnvironment,
    RenameEnvironment(Id),
    /// Fork this collection.
    Fork(Id),
}

pub enum DeleteTarget {
    Item { collection: Id, item: Option<Id> },
    Environment(Id),
}

/// Pages of the Settings dialog.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub enum SettingsPage {
    #[default]
    General,
    Requests,
    Scripts,
    Proxy,
    Certificates,
    Shortcuts,
    About,
}

impl SettingsPage {
    const ALL: [SettingsPage; 7] = [
        SettingsPage::General,
        SettingsPage::Requests,
        SettingsPage::Scripts,
        SettingsPage::Proxy,
        SettingsPage::Certificates,
        SettingsPage::Shortcuts,
        SettingsPage::About,
    ];

    fn icon(self) -> &'static str {
        match self {
            SettingsPage::General => icons::SLIDERS_HORIZONTAL,
            SettingsPage::Requests => icons::PAPER_PLANE_RIGHT,
            SettingsPage::Scripts => icons::CODE,
            SettingsPage::Proxy => icons::SHARE,
            SettingsPage::Certificates => icons::SHIELD_CHECK,
            SettingsPage::Shortcuts => icons::KEYBOARD,
            SettingsPage::About => icons::INFO,
        }
    }

    /// The page named `name` (`general`, `requests`, `scripts`, `proxy`,
    /// `certs`, `shortcuts`, `about`), for automation.
    pub fn from_name(name: &str) -> Option<SettingsPage> {
        Some(match name {
            "general" => SettingsPage::General,
            "requests" => SettingsPage::Requests,
            "scripts" => SettingsPage::Scripts,
            "proxy" => SettingsPage::Proxy,
            "certs" | "certificates" => SettingsPage::Certificates,
            "shortcuts" => SettingsPage::Shortcuts,
            "about" => SettingsPage::About,
            _ => return None,
        })
    }

    fn label(self) -> &'static str {
        match self {
            SettingsPage::General => tr("General"),
            SettingsPage::Requests => tr("Requests"),
            SettingsPage::Scripts => tr("Scripts"),
            SettingsPage::Proxy => tr("Proxy"),
            SettingsPage::Certificates => tr("Certificates"),
            SettingsPage::Shortcuts => tr("Shortcuts"),
            SettingsPage::About => tr("About"),
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            SettingsPage::General => tr("Appearance, language, updates and history."),
            SettingsPage::Requests => tr("Defaults for every request you send."),
            SettingsPage::Scripts => tr("Limits for scripts and the JavaScript packages they can load."),
            SettingsPage::Proxy => tr("Send requests through a proxy server."),
            SettingsPage::Certificates => tr("Trust extra certificate authorities and send client certificates."),
            SettingsPage::Shortcuts => tr("Keys for the things you do most."),
            SettingsPage::About => "",
        }
    }
}

pub enum Dialog {
    Palette {
        query: String,
        requests_only: bool,
        selected: usize,
    },
    EnvSwitcher {
        query: String,
        /// Highlighted row; starts on the active environment.
        selected: Option<usize>,
    },
    QuickLook,
    Settings {
        draft: Box<AppSettings>,
        section: SettingsPage,
    },
    Vault(Box<crate::vault_ui::VaultDialog>),
    Cookies {
        domain: Option<String>,
        new_cookie: String,
        error: Option<String>,
        new_domain: String,
    },
    Import {
        text: String,
        error: Option<String>,
        preview: Option<ternarywire_convert::Imported>,
        source: String,
    },
    Codegen {
        item: Box<RequestItem>,
        collection: Option<Id>,
        lang: Language,
        code: String,
    },
    SaveAs {
        tab: u64,
        collection: Option<Id>,
        folder: Option<Id>,
        name: String,
        /// `name` was filled in from the tab (once: the user may empty it).
        named: bool,
        new_collection: String,
    },
    NewWorkspace {
        name: String,
        path: Option<PathBuf>,
    },
    Prompt {
        kind: Prompt,
        text: String,
    },
    Confirm {
        target: DeleteTarget,
        name: String,
    },
    ForkSync(Box<crate::fork_ui::ForkSync>),
    /// Closing a tab with unsaved work: save, discard or keep it open.
    CloseUnsaved {
        tab: u64,
    },
}

impl Dialog {
    pub fn palette(requests_only: bool) -> Dialog {
        Dialog::Palette { query: String::new(), requests_only, selected: 0 }
    }
    pub fn env_switcher() -> Dialog {
        Dialog::EnvSwitcher { query: String::new(), selected: None }
    }
    pub fn quick_look() -> Dialog {
        Dialog::QuickLook
    }
    pub fn settings(s: &AppSettings) -> Dialog {
        Dialog::Settings { draft: Box::new(s.clone()), section: SettingsPage::default() }
    }
    pub fn vault() -> Dialog {
        Dialog::Vault(Box::default())
    }
    pub fn cookies() -> Dialog {
        Dialog::Cookies { domain: None, new_cookie: String::new(), error: None, new_domain: String::new() }
    }
    pub fn import() -> Dialog {
        Dialog::Import { text: String::new(), error: None, preview: None, source: String::new() }
    }
    pub fn codegen(app: &TernaryWireApp, item: &RequestItem, collection: Option<&str>) -> Dialog {
        let lang = Language::Curl;
        let code = codegen_item(app, item, collection, lang);
        Dialog::Codegen { item: Box::new(item.clone()), collection: collection.map(str::to_string), lang, code }
    }
    /// The name is taken from the tab when the dialog is first shown (the tab
    /// may be out of `app.tabs` while it asks for the dialog).
    pub fn save_as(tab: u64, ws: &WsState) -> Dialog {
        Dialog::SaveAs {
            tab,
            collection: ws.data.collections.first().map(|c| c.id.clone()),
            folder: None,
            name: String::new(),
            named: false,
            new_collection: String::new(),
        }
    }
    pub fn new_workspace() -> Dialog {
        Dialog::NewWorkspace { name: "My Workspace".into(), path: None }
    }
    pub fn prompt_new_collection() -> Dialog {
        Dialog::Prompt { kind: Prompt::NewCollection, text: "New Collection".into() }
    }
    pub fn prompt_new_environment() -> Dialog {
        Dialog::Prompt { kind: Prompt::NewEnvironment, text: "New Environment".into() }
    }
    pub fn rename_env(id: Id, name: String) -> Dialog {
        Dialog::Prompt { kind: Prompt::RenameEnvironment(id), text: name }
    }
    pub fn confirm_delete(collection: Id, item: Option<Id>, name: String) -> Dialog {
        Dialog::Confirm { target: DeleteTarget::Item { collection, item }, name }
    }
    pub fn confirm_delete_env(id: Id, name: String) -> Dialog {
        Dialog::Confirm { target: DeleteTarget::Environment(id), name }
    }
}

pub fn show(app: &mut TernaryWireApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.dialog.take() else { return };
    let outcome = match &mut dialog {
        Dialog::Palette { query, requests_only, selected } => palette(app, ctx, query, *requests_only, selected),
        Dialog::EnvSwitcher { query, selected } => env_switcher(app, ctx, query, selected),
        Dialog::QuickLook => quick_look(app, ctx),
        Dialog::Settings { draft, section } => settings(app, ctx, draft, section),
        Dialog::Vault(d) => crate::vault_ui::show(app, ctx, d),
        Dialog::Cookies { domain, new_cookie, error, new_domain } => {
            cookies(app, ctx, domain, new_cookie, error, new_domain)
        }
        Dialog::Import { text, error, preview, source } => import(app, ctx, text, error, preview, source),
        Dialog::Codegen { item, collection, lang, code } => codegen(app, ctx, item, collection.as_deref(), lang, code),
        Dialog::SaveAs { tab, collection, folder, name, named, new_collection } => {
            save_as(app, ctx, *tab, collection, folder, name, named, new_collection)
        }
        Dialog::NewWorkspace { name, path } => new_workspace(app, ctx, name, path),
        Dialog::Prompt { kind, text } => prompt(app, ctx, kind, text),
        Dialog::Confirm { target, name } => confirm(app, ctx, target, name),
        Dialog::ForkSync(s) => crate::fork_ui::show(app, ctx, s),
        Dialog::CloseUnsaved { tab } => close_unsaved(app, ctx, *tab),
    };
    if matches!(outcome, Outcome::Keep) && app.dialog.is_none() {
        app.dialog = Some(dialog);
    }
}

/// Opens a dialog by the name automation scripts use: `settings:<page>`,
/// `palette:<query>`, `requests:<query>`, `env-switch`, `vault:providers`,
/// `vault:link:<name>|<reference>`, `import:<file>`, `snippet`, `save`, `new-ws[:<name>|<folder>]`,
/// `name-prompt`, `confirm:<request or collection>`, `unsaved`,
/// `update[:progress]`. Returns false for other names.
pub fn open_named(app: &mut TernaryWireApp, name: &str) -> bool {
    let (kind, arg) = name.split_once(':').unwrap_or((name, ""));
    let dialog = match kind {
        "settings" if !arg.is_empty() => Dialog::Settings {
            draft: Box::new(app.settings.clone()),
            section: SettingsPage::from_name(arg).unwrap_or_default(),
        },
        "palette" | "requests" if !arg.is_empty() => {
            Dialog::Palette { query: arg.to_string(), requests_only: kind == "requests", selected: 0 }
        }
        "env-switch" => Dialog::env_switcher(),
        "vault" if arg == "providers" => Dialog::Vault(Box::new(crate::vault_ui::VaultDialog::providers())),
        // `vault:link:<name>|<reference>`, both optional.
        "vault" if arg.starts_with("link") => {
            let rest = arg.strip_prefix("link").unwrap_or("").trim_start_matches(':');
            let (name, reference) = rest.split_once('|').unwrap_or((rest, ""));
            Dialog::Vault(Box::new(crate::vault_ui::VaultDialog::linking(name, reference)))
        }
        "import" if !arg.is_empty() => match read_import(std::path::Path::new(arg)) {
            Ok(text) => Dialog::Import { text, error: None, preview: None, source: arg.to_string() },
            Err(e) => Dialog::Import { text: String::new(), error: Some(e), preview: None, source: String::new() },
        },
        "snippet" | "codegen" => {
            let Some(tab) = app.tabs.get(app.active) else { return false };
            let found = match (tab.kind.location(), tab.kind.scratch()) {
                (Some(l), _) => app
                    .ws
                    .data
                    .collection(&l.collection)
                    .and_then(|c| c.find_request(&l.item))
                    .map(|i| (i.clone(), Some(l.collection.clone()))),
                (None, Some(s)) => Some((s.clone(), None)),
                _ => None,
            };
            let Some((item, collection)) = found else { return false };
            Dialog::codegen(app, &item, collection.as_deref())
        }
        // `save:<collection>/<folder>`: with that target picked.
        "save" => {
            let Some(tab) = app.tabs.get(app.active) else { return false };
            let mut dialog = Dialog::save_as(tab.id, &app.ws);
            let (coll, folder) = arg.split_once('/').unwrap_or((arg, ""));
            if let Dialog::SaveAs { collection, folder: target, .. } = &mut dialog
                && let Some(c) = app.ws.data.collections.iter().find(|c| c.name == coll)
            {
                *collection = Some(c.id.clone());
                fn find(items: &[Item], name: &str) -> Option<Id> {
                    items.iter().find_map(|i| match i {
                        Item::Folder(f) if f.name == name => Some(f.id.clone()),
                        Item::Folder(f) => find(&f.items, name),
                        _ => None,
                    })
                }
                *target = find(&c.items, folder);
            }
            dialog
        }
        // `new-ws:<name>|<folder>`, both optional.
        "new-ws" => {
            let (name, folder) = arg.split_once('|').unwrap_or((arg, ""));
            let path = (!folder.is_empty()).then(|| PathBuf::from(folder));
            let name = if name.is_empty() { "My Workspace".to_string() } else { name.to_string() };
            Dialog::NewWorkspace { name, path }
        }
        "name-prompt" => Dialog::prompt_new_collection(),
        "confirm" => {
            let request =
                app.ws.data.collections.iter().find_map(|c| {
                    c.requests().into_iter().find(|r| r.name == arg).map(|r| (c.id.clone(), r.id.clone()))
                });
            match request {
                Some((c, r)) => Dialog::confirm_delete(c, Some(r), arg.to_string()),
                None => match app.ws.data.collections.iter().find(|c| c.name == arg) {
                    Some(c) => Dialog::confirm_delete(c.id.clone(), None, arg.to_string()),
                    None => return false,
                },
            }
        }
        "unsaved" => {
            let Some(tab) = app.tabs.get(app.active) else { return false };
            Dialog::CloseUnsaved { tab: tab.id }
        }
        "update" => {
            crate::update::show_example(app, arg == "progress");
            return true;
        }
        _ => return false,
    };
    app.dialog = Some(dialog);
    true
}

/// Merges `patch` (settings JSON) into the draft of the open Settings
/// dialog, as if it had been edited there.
pub fn merge_settings_draft(app: &mut TernaryWireApp, patch: &serde_json::Value) {
    fn merge(into: &mut serde_json::Value, patch: &serde_json::Value) {
        match (into, patch) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                for (k, v) in b {
                    merge(a.entry(k.clone()).or_insert(serde_json::Value::Null), v);
                }
            }
            (a, b) => *a = b.clone(),
        }
    }
    if let Some(Dialog::Settings { draft, .. }) = &mut app.dialog
        && let Ok(mut v) = serde_json::to_value(&**draft)
    {
        merge(&mut v, patch);
        match serde_json::from_value(v) {
            Ok(s) => **draft = s,
            Err(e) => eprintln!("automation: settings_draft: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Command palette, request search and environment switcher
// ---------------------------------------------------------------------------

/// Result groups, in the order they are listed when nothing is typed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
enum Group {
    Create,
    Actions,
    Requests,
    Environments,
    Runner,
    Preferences,
}

impl Group {
    fn label(self) -> &'static str {
        match self {
            Group::Create => tr("Create"),
            Group::Actions => tr("Actions"),
            Group::Requests => tr("Requests"),
            Group::Environments => tr("Environments"),
            Group::Runner => tr("Run collection"),
            Group::Preferences => tr("Preferences"),
        }
    }
}

/// What a result row shows in front of its name.
enum Lead {
    Method(String),
    Icon(&'static str),
    /// A faint icon ("No environment" in the switcher).
    Faint(&'static str),
    /// An environment's color.
    Dot(Color32),
    /// The active environment in the switcher: a green check.
    Active,
}

struct PaletteEntry {
    group: Group,
    lead: Lead,
    label: String,
    /// Muted text after the name (folder path, variable count).
    detail: String,
    /// Matched but not shown (URL, synonyms).
    extra: String,
    keys: &'static str,
    /// Shown with a check mark (active environment, current theme).
    current: bool,
    action: PaletteAction,
}

impl PaletteEntry {
    fn new(group: Group, icon: &'static str, label: impl Into<String>, action: PaletteAction) -> PaletteEntry {
        PaletteEntry {
            group,
            lead: Lead::Icon(icon),
            label: label.into(),
            detail: String::new(),
            extra: String::new(),
            keys: "",
            current: false,
            action,
        }
    }

    fn keys(mut self, keys: &'static str) -> PaletteEntry {
        self.keys = keys;
        self
    }

    fn detail(mut self, detail: impl Into<String>) -> PaletteEntry {
        self.detail = detail.into();
        self
    }

    fn extra(mut self, extra: impl Into<String>) -> PaletteEntry {
        self.extra = extra.into();
        self
    }

    fn current(mut self, current: bool) -> PaletteEntry {
        self.current = current;
        self
    }
}

enum PaletteAction {
    Open(ItemRef),
    NewRequest,
    NewWs,
    NewProto(usize),
    NewWebhook,
    NewCapture,
    NewFlow,
    NewCollection,
    NewEnvironment,
    Import,
    Settings,
    ToggleSidebar,
    ToggleConsole,
    Cookies,
    Vault,
    QuickLook,
    SwitchEnvironment,
    Env(Option<Id>),
    Run(Id),
    Theme(ThemePreference),
    Language(UiLanguage),
    OpenWorkspace,
    NewWorkspace,
    CheckUpdates,
}

fn fuzzy(hay: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let h = hay.to_lowercase();
    let n = needle.to_lowercase();
    if let Some(p) = h.find(&n) {
        // The start of the text beats the start of a word, which beats the
        // inside of a word; equal matches keep the order of the list.
        let word_start = !h[..p].chars().next_back().is_some_and(char::is_alphanumeric);
        return Some(match (p, word_start) {
            (0, _) => 1000,
            (_, true) => 900,
            _ => 800,
        });
    }
    let mut it = h.chars();
    let mut score = 0;
    for c in n.chars() {
        loop {
            match it.next() {
                Some(x) if x == c => {
                    score += 1;
                    break;
                }
                Some(_) => score -= 1,
                None => return None,
            }
        }
    }
    Some(score)
}

/// Which characters of `text` match `query` (the first substring match, or
/// the characters of a subsequence match), for highlighting.
fn match_marks(text: &str, query: &str) -> Vec<bool> {
    let lower: Vec<char> = text.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
    let mut marks = vec![false; lower.len()];
    let q: Vec<char> = query.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
    if q.is_empty() || q.len() > lower.len() {
        return marks;
    }
    if let Some(start) = (0..=lower.len() - q.len()).find(|&i| lower[i..i + q.len()] == q[..]) {
        marks[start..start + q.len()].iter_mut().for_each(|m| *m = true);
        return marks;
    }
    let mut qi = 0;
    for (i, c) in lower.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            marks[i] = true;
            qi += 1;
        }
    }
    if qi < q.len() {
        marks.iter_mut().for_each(|m| *m = false);
    }
    marks
}

fn score(e: &PaletteEntry, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let name = fuzzy(&e.label, query).map(|s| s + 2000);
    let rest = fuzzy(&format!("{} {}", e.detail, e.extra), query);
    // Requests are what people search for most: they win ties.
    let bias = if e.group == Group::Requests { 8 } else { 0 };
    name.max(rest).map(|s| s + bias)
}

/// Matching entries in display order: groups by their best match (then by
/// their usual order), rows by score.
fn rank(entries: &[PaletteEntry], query: &str) -> Vec<usize> {
    let mut scored: Vec<(i32, usize)> =
        entries.iter().enumerate().filter_map(|(i, e)| score(e, query).map(|s| (s, i))).collect();
    let mut best: std::collections::HashMap<Group, i32> = std::collections::HashMap::new();
    for (s, i) in &scored {
        let b = best.entry(entries[*i].group).or_insert(i32::MIN);
        *b = (*b).max(*s);
    }
    scored.sort_by(|a, b| {
        let (ga, gb) = (entries[a.1].group, entries[b.1].group);
        best[&gb].cmp(&best[&ga]).then(ga.cmp(&gb)).then(b.0.cmp(&a.0)).then(a.1.cmp(&b.1))
    });
    scored.truncate(120);
    scored.into_iter().map(|(_, i)| i).collect()
}

fn request_entries(app: &TernaryWireApp, out: &mut Vec<PaletteEntry>) {
    fn walk(c: &ternarywire_core::model::Collection, items: &[Item], path: &str, out: &mut Vec<PaletteEntry>) {
        for i in items {
            match i {
                Item::Folder(f) => walk(c, &f.items, &format!("{path} › {}", f.name), out),
                Item::Request(r) => out.push(PaletteEntry {
                    group: Group::Requests,
                    lead: Lead::Method(r.request.badge()),
                    label: r.name.clone(),
                    detail: path.to_string(),
                    extra: r.request.url().to_string(),
                    keys: "",
                    current: false,
                    action: PaletteAction::Open(ItemRef { collection: c.id.clone(), item: r.id.clone() }),
                }),
            }
        }
    }
    for c in &app.ws.data.collections {
        walk(c, &c.items, &c.name, out);
    }
}

fn environment_entries(app: &TernaryWireApp, out: &mut Vec<PaletteEntry>, dark: bool) {
    let active = app.ws.data.active_environment.as_deref();
    let p = theme::palette(dark);
    out.push(
        PaletteEntry::new(Group::Environments, icons::CIRCLE, tr("No environment"), PaletteAction::Env(None))
            .extra("environment")
            .current(active.is_none()),
    );
    for (i, e) in app.ws.data.environments.iter().enumerate() {
        let n = e.variables.iter().filter(|v| v.enabled).count();
        let vars = if n == 1 { tr("1 variable").to_string() } else { trf("{} variables", &[&n]) };
        let mut entry = PaletteEntry::new(
            Group::Environments,
            icons::GLOBE,
            e.name.clone(),
            PaletteAction::Env(Some(e.id.clone())),
        )
        .detail(vars)
        .extra("environment")
        .current(active == Some(e.id.as_str()));
        entry.lead = Lead::Dot(env_color(&p, i));
        out.push(entry);
    }
}

fn palette_entries(app: &TernaryWireApp, requests_only: bool, dark: bool) -> Vec<PaletteEntry> {
    let mut out = Vec::new();
    if !requests_only {
        use PaletteAction as A;
        let c = Group::Create;
        out.push(PaletteEntry::new(c, icons::GLOBE, tr("New Request"), A::NewRequest).keys("⌘N").extra("http"));
        out.push(PaletteEntry::new(c, icons::PLUGS_CONNECTED, tr("New WebSocket"), A::NewWs));
        for (i, (label, name, _)) in crate::app::protocol_templates().into_iter().enumerate() {
            out.push(PaletteEntry::new(c, crate::app::protocol_icon(name), tr(label), A::NewProto(i)));
        }
        out.push(PaletteEntry::new(c, icons::WEBHOOKS_LOGO, tr("New webhook listener"), A::NewWebhook));
        out.push(PaletteEntry::new(c, icons::BROADCAST, tr("Capture traffic (proxy)"), A::NewCapture));
        out.push(PaletteEntry::new(c, icons::FLOW_ARROW, tr("New flow"), A::NewFlow));
        out.push(PaletteEntry::new(c, icons::STACK, tr("New Collection"), A::NewCollection));
        out.push(PaletteEntry::new(c, icons::GLOBE_SIMPLE, tr("New Environment"), A::NewEnvironment));
        out.push(PaletteEntry::new(c, icons::DOWNLOAD_SIMPLE, tr("Import"), A::Import).extra("postman openapi curl"));
        let a = Group::Actions;
        out.push(PaletteEntry::new(a, icons::GLOBE, tr("Switch environment"), A::SwitchEnvironment).keys("⌘E"));
        out.push(PaletteEntry::new(a, icons::EYE, tr("Quick look"), A::QuickLook).extra("variables"));
        out.push(PaletteEntry::new(a, icons::SIDEBAR_SIMPLE, tr("Toggle sidebar"), A::ToggleSidebar).keys("⌘B"));
        out.push(PaletteEntry::new(a, icons::TERMINAL_WINDOW, tr("Toggle console"), A::ToggleConsole).keys("⌥⌘C"));
        out.push(PaletteEntry::new(a, icons::COOKIE, tr("Manage cookies"), A::Cookies));
        out.push(PaletteEntry::new(a, icons::LOCK_KEY, tr("Manage vault"), A::Vault).extra("secrets"));
        out.push(PaletteEntry::new(a, icons::FOLDER_OPEN, tr("Open workspace…"), A::OpenWorkspace));
        out.push(PaletteEntry::new(a, icons::FOLDER_PLUS, tr("New workspace…"), A::NewWorkspace));
        out.push(PaletteEntry::new(a, icons::GEAR_SIX, tr("Settings"), A::Settings).keys("⌘,").extra("preferences"));
        out.push(PaletteEntry::new(a, icons::ARROWS_CLOCKWISE, tr("Check for updates…"), A::CheckUpdates));
        let mut envs = Vec::new();
        environment_entries(app, &mut envs, dark);
        out.extend(envs);
        for col in &app.ws.data.collections {
            out.push(
                PaletteEntry::new(Group::Runner, icons::PLAY, col.name.clone(), A::Run(col.id.clone()))
                    .detail(tr("Collection runner"))
                    .extra("run runner"),
            );
        }
        let p = Group::Preferences;
        let theme = app.settings.ui.theme;
        for (t, icon, name) in [
            (ThemePreference::System, icons::CIRCLE_HALF, tr("System")),
            (ThemePreference::Light, icons::SUN, tr("Light")),
            (ThemePreference::Dark, icons::MOON, tr("Dark")),
        ] {
            out.push(
                PaletteEntry::new(p, icon, trf("Theme: {}", &[&name]), A::Theme(t)).extra("theme").current(theme == t),
            );
        }
        let lang = app.settings.ui.language;
        for (l, name) in [(UiLanguage::En, "English"), (UiLanguage::Id, "Bahasa Indonesia")] {
            out.push(
                PaletteEntry::new(p, icons::TRANSLATE, trf("Language: {}", &[&name]), A::Language(l))
                    .extra("language bahasa")
                    .current(lang == l),
            );
        }
    }
    request_entries(app, &mut out);
    out
}

/// Consumes ↑, ↓ and ↩ before the search field sees them.
fn nav_keys(ui: &mut Ui) -> (bool, bool, bool) {
    ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
        )
    })
}

/// The search row at the top of the palette and the switcher (`.dl-pal-in`):
/// 54 high, an 18 px icon, a frameless 15 px field and an `esc` key cap.
fn search_header(ui: &mut Ui, id: &str, query: &mut String, hint: &str, icon: &str) -> Response {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let cy = rect.center().y;
    let ig = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(18.0), p.text_faint);
    ui.painter().galley(pos2(rect.left() + 16.0, cy - ig.size().y / 2.0), ig, p.text_faint);
    let kbd_w = kit::kbd_size(ui, "esc").x;
    let kbd = Rect::from_min_size(pos2(rect.right() - 16.0 - kbd_w, cy - 9.0), vec2(kbd_w, 18.0));
    place(ui, kbd, |ui: &mut Ui| kit::kbd(ui, "esc"));
    // The icon is 18 wide, 10 px before the field, whose text starts after
    // the 2 px padding of an `<input>`. Its 15 px text sits 1 px above the
    // center of the row, where the browser puts it.
    let left = rect.left() + 16.0 + 18.0 + 10.0 + 2.0;
    let field = Rect::from_min_max(pos2(left, rect.top()), pos2(kbd.left() - 10.0, rect.bottom() - 2.0));
    let edit = egui::TextEdit::singleline(query)
        .id(egui::Id::unique(id))
        .hint_text(RichText::new(hint).font(FontId::proportional(15.0)).color(p.text_faint))
        .font(FontId::proportional(15.0))
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .min_size(vec2(0.0, field.height()))
        .align(Align2::LEFT_CENTER)
        .desired_width(field.width());
    let r = in_rect(ui, field, Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
        faint_hints(ui);
        ui.add(edit)
    });
    r.request_focus();
    widgets::a11y_name(&r, hint);
    r
}

/// What the right end of a result row shows besides its shortcut.
#[derive(Clone, Copy)]
enum RowEnd {
    /// The palette: a check on the current theme, language or environment.
    Check,
    /// The environment switcher: an `Active` pill on the active one.
    ActivePill,
}

/// A result row (`.dl-pal-row`): 36 high, a 38 px lead column (method,
/// icon or environment dot), the name with the matched characters in bold
/// accent, a faint detail, and on the right the shortcut or the check. The
/// highlighted row is tinted and says what ↩ does.
fn palette_row(ui: &mut Ui, e: &PaletteEntry, query: &str, selected: bool, end: RowEnd, enter: &str) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    widgets::a11y_button(&resp, &e.label);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let radius = CornerRadius::same(theme::RADIUS);
    if selected {
        ui.painter().rect_filled(rect, radius, p.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, radius, p.hover);
    }
    let cy = rect.center().y;
    let lead = pos2(rect.left() + 10.0 + 19.0, cy);
    match &e.lead {
        Lead::Method(m) => {
            let color = theme::method_color(m, dark);
            ui.painter().text(lead, Align2::CENTER_CENTER, widgets::method_short(m), theme::semibold(10.5), color);
        }
        Lead::Icon(icon) => {
            ui.painter().text(lead, Align2::CENTER_CENTER, *icon, theme::icon_font(16.0), p.text_muted);
        }
        Lead::Faint(icon) => {
            ui.painter().text(lead, Align2::CENTER_CENTER, *icon, theme::icon_font(16.0), p.text_faint);
        }
        Lead::Dot(c) => {
            ui.painter().circle_filled(lead, 4.0, *c);
        }
        Lead::Active => {
            ui.painter().text(lead, Align2::CENTER_CENTER, icons::CHECK_CIRCLE, theme::icon_font(16.0), p.ok);
        }
    }
    let mut x = rect.left() + 10.0 + 38.0 + 10.0;
    let mut right = rect.right() - 10.0;
    if selected {
        let tg = ui.painter().layout_no_wrap(enter.to_string(), FontId::proportional(12.0), p.accent_text);
        let ig = ui.painter().layout_no_wrap(icons::ARROW_BEND_UP_LEFT.into(), theme::icon_font(12.0), p.accent_text);
        let tx = right - tg.size().x;
        let ix = tx - 4.0 - ig.size().x;
        ui.painter().galley(pos2(tx, cy - tg.size().y / 2.0), tg, p.accent_text);
        ui.painter().galley(pos2(ix, cy - ig.size().y / 2.0), ig, p.accent_text);
        right = ix - 10.0;
    } else {
        if e.current {
            match end {
                RowEnd::Check => {
                    let g = ui.painter().layout_no_wrap(icons::CHECK.into(), theme::icon_font(13.0), p.accent_text);
                    let w = g.size().x;
                    ui.painter().galley(pos2(right - w, cy - g.size().y / 2.0), g, p.accent_text);
                    right -= w + 10.0;
                }
                RowEnd::ActivePill => {
                    let g = ui.painter().layout_no_wrap(tr("Active").into(), theme::medium(11.5), p.ok);
                    let pill = Rect::from_min_max(pos2(right - g.size().x - 14.0, cy - 10.0), pos2(right, cy + 10.0));
                    ui.painter().rect_filled(pill, CornerRadius::same(10), p.ok_soft);
                    ui.painter().galley(pill.center() - g.size() / 2.0, g, p.ok);
                    right = pill.left() - 10.0;
                }
            }
        }
        if !e.keys.is_empty() {
            let w = ui.painter().layout_no_wrap(e.keys.to_string(), FontId::proportional(10.5), p.text_muted).size().x
                + 10.0;
            let r = Rect::from_min_size(pos2(right - w, cy - 9.0), vec2(w, 18.0));
            place(ui, r, |ui: &mut Ui| kit::kbd(ui, e.keys));
            right -= w + 10.0;
        }
    }
    // The name with the matched characters in bold accent, then the faint
    // detail when there is room.
    let marks = match_marks(&e.label, query);
    let mut job = LayoutJob::default();
    let plain = TextFormat::simple(FontId::proportional(13.0), p.text);
    let hit = TextFormat::simple(theme::semibold(13.0), p.accent_text);
    let mut run = String::new();
    let mut run_hit = false;
    for (c, m) in e.label.chars().zip(marks.iter().copied()) {
        if m != run_hit && !run.is_empty() {
            job.append(&std::mem::take(&mut run), 0.0, if run_hit { hit.clone() } else { plain.clone() });
        }
        run_hit = m;
        run.push(c);
    }
    if !run.is_empty() {
        job.append(&run, 0.0, if run_hit { hit } else { plain });
    }
    let name_max = (right - x).max(40.0);
    job.wrap = TextWrapping { max_width: name_max, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let name = ui.painter().layout_job(job);
    let name_w = name.size().x;
    ui.painter().galley(pos2(x, cy - name.size().y / 2.0), name, p.text);
    x += name_w + 10.0;
    if !e.detail.is_empty() && right - x > 40.0 {
        let g = line_galley(ui, &e.detail, FontId::proportional(12.0), right - x);
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0 + 0.5), g, p.text_faint);
    }
    resp
}

/// A group header in the result list (`.dl-pal-grp`): 10.5 semibold faint
/// capitals, 10 px above and 4 below.
fn group_header(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let line = (10.5f32 * 1.45).round();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0 + line + 4.0), Sense::hover());
    let format = TextFormat {
        font_id: theme::semibold(10.5),
        color: p.text_faint,
        extra_letter_spacing: 0.63,
        ..Default::default()
    };
    let g = ui.painter().layout_job(LayoutJob::single_section(text.to_uppercase(), format));
    ui.painter().galley(pos2(rect.left() + 10.0, rect.top() + 10.0 + (line - g.size().y) / 2.0), g, p.text_faint);
}

/// The grouped result list (`.dl-pal-list`, padding 4 8 8). Returns the
/// entry that was clicked.
#[allow(clippy::too_many_arguments)]
fn result_list(
    ui: &mut Ui,
    entries: &[PaletteEntry],
    order: &[usize],
    query: &str,
    selected: &mut usize,
    scroll: bool,
    max_h: f32,
    end: RowEnd,
    enter: &str,
) -> Option<usize> {
    let mut chosen = None;
    let first_group = order.first().map(|i| entries[*i].group);
    let headers = order.iter().any(|i| Some(entries[*i].group) != first_group);
    let moved = ui.input(|i| i.pointer.delta() != Vec2::ZERO);
    egui::ScrollArea::vertical().id_salt("results").max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        egui::Frame::new().inner_margin(Margin { left: 8, right: 8, top: 4, bottom: 8 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut last = None;
            for (row, &i) in order.iter().enumerate() {
                let e = &entries[i];
                if headers && last != Some(e.group) {
                    group_header(ui, e.group.label());
                    last = Some(e.group);
                }
                let r = palette_row(ui, e, query, row == *selected, end, enter);
                if row == *selected && scroll {
                    r.scroll_to_me(None);
                }
                if r.hovered() && moved {
                    *selected = row;
                }
                if r.clicked() {
                    chosen = Some(i);
                }
            }
        });
    });
    chosen
}

/// "Nothing found" under a search field.
fn no_results(ui: &mut Ui, title: &str, text: &str) {
    let p = theme::pal(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(34.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
        ui.painter().rect(rect, CornerRadius::same(14), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icons::MAGNIFYING_GLASS,
            theme::icon_font(24.0),
            p.text_muted,
        );
        ui.add_space(14.0);
        ui.label(RichText::new(title).font(theme::semibold(14.0)).color(p.text));
        ui.add_space(2.0);
        ui.label(RichText::new(text).size(12.5).color(p.text_muted));
        ui.add_space(34.0);
    });
}

/// Moves the highlighted row with ↑/↓ (wrapping around).
fn step(selected: &mut usize, len: usize, up: bool, down: bool) {
    if len == 0 {
        *selected = 0;
        return;
    }
    if down {
        *selected = (*selected + 1) % len;
    }
    if up {
        *selected = (*selected + len - 1) % len;
    }
    *selected = (*selected).min(len - 1);
}

fn palette(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    query: &mut String,
    requests_only: bool,
    selected: &mut usize,
) -> Outcome {
    let dark = ctx.global_style().visuals.dark_mode;
    let mut entries = palette_entries(app, requests_only, dark);
    let q = query.trim().to_string();
    let order = rank(&entries, &q);
    let no_requests = !entries.iter().any(|e| e.group == Group::Requests);
    let mut chosen = None;
    let outcome = Modal::new("palette", 660.0).top().show(ctx, |ui| {
        let (up, down, enter) = nav_keys(ui);
        let hint = if requests_only { tr("Search requests…") } else { tr("Search requests and commands…") };
        if search_header(ui, "palette-search", query, hint, icons::MAGNIFYING_GLASS).changed() {
            *selected = 0;
        }
        // Over a footer of key hints (18 + 25).
        let max_h = list_room(ui, 43.0).min(if q.is_empty() { 440.0 } else { 300.0 });
        step(selected, order.len(), up, down);
        if order.is_empty() {
            if q.is_empty() && no_requests {
                no_results(ui, tr("No requests yet"), tr("Create one with ⌘N or import a collection."));
            } else {
                no_results(ui, &trf("No results for “{}”", &[&q]), tr("Check the spelling or try another word."));
            }
        } else {
            chosen = result_list(ui, &entries, &order, &q, selected, up || down, max_h, RowEnd::Check, tr("Open"));
        }
        if enter {
            chosen = order.get(*selected).copied();
        }
        footer(ui, |ui| {
            key_hints(ui, &[(&["↑", "↓"], tr("Navigate")), (&["↩"], tr("Open")), (&["esc"], tr("Close"))]);
            trailing(ui, |ui| {
                let n = order.len();
                let text = if n == 1 { tr("1 result").to_string() } else { trf("{} results", &[&n]) };
                ui.label(RichText::new(text).size(12.0).color(theme::pal(ui).text_faint));
            });
        });
        if chosen.is_some() { Outcome::Close } else { Outcome::Keep }
    });
    if let Some(i) = chosen {
        let action = entries.swap_remove(i).action;
        run_palette_action(app, ctx, action);
    }
    outcome
}

fn run_palette_action(app: &mut TernaryWireApp, ctx: &egui::Context, action: PaletteAction) {
    match action {
        PaletteAction::Open(r) => app.open_item(r),
        PaletteAction::NewRequest => app.new_request_tab(),
        PaletteAction::NewWs => app.new_ws_tab(),
        PaletteAction::NewProto(i) => {
            if let Some((_, name, kind)) = crate::app::protocol_templates().into_iter().nth(i) {
                app.new_proto_tab(name, kind);
            }
        }
        PaletteAction::NewCapture => {
            app.add_tab(TabKind::Capture(Box::default()));
        }
        PaletteAction::NewFlow => {
            app.add_tab(TabKind::Flow(Box::new(crate::flow_tab::FlowTab::blank(tr("Flow")))));
        }
        PaletteAction::NewWebhook => {
            app.add_tab(TabKind::Webhook(Box::default()));
        }
        PaletteAction::NewCollection => app.dialog = Some(Dialog::prompt_new_collection()),
        PaletteAction::NewEnvironment => app.dialog = Some(Dialog::prompt_new_environment()),
        PaletteAction::Import => app.dialog = Some(Dialog::import()),
        PaletteAction::Settings => app.dialog = Some(Dialog::settings(&app.settings)),
        PaletteAction::ToggleSidebar => app.settings.ui.sidebar_visible = !app.settings.ui.sidebar_visible,
        PaletteAction::ToggleConsole => app.console_open = !app.console_open,
        PaletteAction::Cookies => app.dialog = Some(Dialog::cookies()),
        PaletteAction::Vault => app.dialog = Some(Dialog::vault()),
        PaletteAction::QuickLook => app.dialog = Some(Dialog::quick_look()),
        PaletteAction::SwitchEnvironment => app.dialog = Some(Dialog::env_switcher()),
        PaletteAction::Env(e) => app.ws.set_active_environment(e),
        PaletteAction::Run(c) => app.open_runner(c, None),
        // Like the workspace menu: a folder that is not a workspace is refused.
        PaletteAction::OpenWorkspace => app.pick_workspace(),
        PaletteAction::NewWorkspace => app.dialog = Some(Dialog::new_workspace()),
        PaletteAction::CheckUpdates => crate::update::request_check(ctx),
        PaletteAction::Theme(t) => {
            let mut s = app.settings.clone();
            s.ui.theme = t;
            app.apply_settings(s);
        }
        PaletteAction::Language(l) => {
            let mut s = app.settings.clone();
            s.ui.language = l;
            app.apply_settings(s);
        }
    }
}

fn env_switcher(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    query: &mut String,
    selected: &mut Option<usize>,
) -> Outcome {
    let dark = ctx.global_style().visuals.dark_mode;
    let mut entries = Vec::new();
    environment_entries(app, &mut entries, dark);
    // The switcher marks the active environment with a green check and
    // "No environment" with a faint circle.
    for e in &mut entries {
        if e.current && matches!(e.lead, Lead::Dot(_)) {
            e.lead = Lead::Active;
        } else if let Lead::Icon(i) = e.lead {
            e.lead = Lead::Faint(i);
        }
    }
    let q = query.trim().to_string();
    let order = rank(&entries, &q);
    // It opens on the environment after the active one, the likely switch.
    let mut sel = selected.unwrap_or_else(|| {
        let active = order.iter().position(|i| entries[*i].current).unwrap_or(0);
        (active + 1) % order.len().max(1)
    });
    let mut chosen = None;
    let mut edit = false;
    let out = Modal::new("env-switch", 500.0).top().show(ctx, |ui| {
        let (up, down, enter) = nav_keys(ui);
        if search_header(ui, "env-search", query, tr("Switch environment…"), icons::GLOBE).changed() {
            sel = 0;
        }
        // Over a footer with a small button (24 + 25).
        let max_h = list_room(ui, 49.0).min(440.0);
        step(&mut sel, order.len(), up, down);
        if order.is_empty() {
            no_results(ui, &trf("No results for “{}”", &[&q]), tr("Check the spelling or try another word."));
        } else {
            chosen =
                result_list(ui, &entries, &order, &q, &mut sel, up || down, max_h, RowEnd::ActivePill, tr("Activate"));
        }
        if enter {
            chosen = order.get(sel).copied();
        }
        footer(ui, |ui| {
            key_hints(ui, &[(&["↑", "↓"], tr("Navigate")), (&["↩"], tr("Activate"))]);
            trailing(ui, |ui| {
                if kit::ghost(ui, Some(icons::SLIDERS_HORIZONTAL), tr("Edit variables")).clicked() {
                    edit = true;
                }
            });
        });
        if chosen.is_some() || edit { Outcome::Close } else { Outcome::Keep }
    });
    *selected = Some(sel);
    if let Some(i) = chosen
        && let PaletteAction::Env(e) = entries.swap_remove(i).action
    {
        app.ws.set_active_environment(e);
    }
    if edit {
        app.open_environment(app.ws.data.active_environment.clone());
    }
    out
}

// ---------------------------------------------------------------------------
// Quick look
// ---------------------------------------------------------------------------

/// What the head of a quick look card shows in front of its name.
enum CardLead {
    Dot(Color32),
    Icon(&'static str),
}

/// A card listing variables (`.card` in the quick look): a 42 px head with
/// the name, an `Active` pill, the count and an Edit button, then key/value
/// rows of 30 px with hairlines. Returns true when Edit was pressed.
fn variables_card(
    ui: &mut Ui,
    lead: CardLead,
    title: &str,
    active: bool,
    vars: &[ternarywire_core::variables::Variable],
) -> bool {
    let p = theme::pal(ui);
    let mut edit = false;
    let shown: Vec<_> = vars.iter().filter(|v| v.enabled).collect();
    kit::card(ui).inner_margin(Margin::ZERO).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = ui.available_width();
        let head = Rect::from_min_size(ui.cursor().min, vec2(w, 42.0));
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));
        let inner = Rect::from_min_max(head.min + vec2(14.0, 0.0), head.max - vec2(10.0, 1.0));
        in_rect(ui, inner, Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            match lead {
                CardLead::Dot(c) => {
                    kit::dot(ui, c, 4.0);
                }
                CardLead::Icon(i) => {
                    ui.label(RichText::new(i).font(theme::icon_font(13.0)).color(p.text_muted));
                }
            }
            ui.label(RichText::new(title).font(theme::semibold(13.0)).color(p.text));
            if active {
                kit::tone_pill(ui, tr("Active"), kit::Tone::Ok);
            }
            let n = shown.len();
            let count = if n == 1 { tr("1 variable").to_string() } else { trf("{} variables", &[&n]) };
            ui.label(RichText::new(count).size(12.0).color(p.text_faint));
            trailing(ui, |ui| {
                if kit::ghost(ui, Some(icons::PENCIL_SIMPLE), tr("Edit")).clicked() {
                    edit = true;
                }
            });
        });
        ui.advance_cursor_after_rect(head);
        if shown.is_empty() {
            egui::Frame::new().inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.label(RichText::new(tr("No variables")).size(12.0).color(p.text_muted));
            });
        }
        // Columns .9fr / 1.3fr with a 12 px gap, 14 px in from the sides.
        let key_w = (w - 28.0 - 12.0) * 0.9 / 2.2;
        for (i, v) in shown.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
            if i > 0 {
                ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
            }
            let cy = rect.center().y;
            let kg = line_galley(ui, &v.key, theme::mono(12.0), key_w);
            ui.painter().galley(pos2(rect.left() + 14.0, cy - kg.size().y / 2.0), kg, p.text_muted);
            let vx = rect.left() + 14.0 + key_w + 12.0;
            if v.is_secret() {
                let lock = ui.painter().layout_no_wrap(icons::LOCK_SIMPLE.into(), theme::icon_font(12.0), p.text_faint);
                let lw = lock.size().x;
                ui.painter().galley(pos2(vx, cy - lock.size().y / 2.0), lock, p.text_faint);
                ui.painter().text(
                    pos2(vx + lw + 4.0, cy),
                    Align2::LEFT_CENTER,
                    "••••••••",
                    theme::mono(12.0),
                    p.text_faint,
                );
            } else {
                let value = v.effective();
                let g = line_galley(ui, value, theme::mono(12.0), rect.right() - vx - 14.0);
                let elided = g.elided;
                ui.painter().galley(pos2(vx, cy - g.size().y / 2.0), g, p.text);
                if elided {
                    let _ = resp.on_hover_text(RichText::new(value).monospace());
                }
            }
        }
    });
    edit
}

fn quick_look(app: &mut TernaryWireApp, ctx: &egui::Context) -> Outcome {
    let mut open_env = None;
    let mut choose = false;
    let out = Modal::new("quick-look", 620.0)
        .title(tr("Quick look"))
        .subtitle(tr("Variables in scope right now"))
        .icon(icons::EYE)
        .show(ctx, |ui| {
            body(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                let active = app.ws.data.active_environment.clone();
                let index = app.ws.data.environments.iter().position(|e| Some(&e.id) == active.as_ref());
                match (app.ws.data.active_environment(), index) {
                    (Some(e), Some(i)) => {
                        let color = env_color(&theme::pal(ui), i);
                        if variables_card(ui, CardLead::Dot(color), &e.name, true, &e.variables) {
                            open_env = Some(Some(e.id.clone()));
                        }
                    }
                    _ => {
                        kit::card(ui).inner_margin(Margin { left: 14, right: 10, top: 0, bottom: 0 }).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.set_height(42.0);
                                let p = theme::pal(ui);
                                ui.label(RichText::new(icons::GLOBE).font(theme::icon_font(13.0)).color(p.text_faint));
                                ui.label(RichText::new(tr("No environment is active")).color(p.text_muted));
                                trailing(ui, |ui| {
                                    if kit::button_ex(ui, kit::Kind::Secondary, None, tr("Choose…"), true).clicked() {
                                        choose = true;
                                    }
                                });
                            });
                        });
                    }
                }
                let globals = CardLead::Icon(icons::GLOBE_SIMPLE);
                if variables_card(ui, globals, tr("Globals"), false, &app.ws.data.globals) {
                    open_env = Some(None);
                }
            });
            footer(ui, |ui| {
                icon_hint(ui, icons::LOCK_SIMPLE, tr("Secret values stay hidden."));
                trailing(ui, |ui| {
                    if kit::secondary(ui, None, tr("Close")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    if choose {
        app.dialog = Some(Dialog::env_switcher());
        return Outcome::Close;
    }
    if let Some(e) = open_env {
        app.open_environment(e);
        return Outcome::Close;
    }
    out
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Height of the Settings dialog: fixed, so changing pages doesn't make it jump.
const SETTINGS_H: f32 = 640.0;

/// The help line of a settings row.
#[derive(Clone, Copy)]
enum Help<'a> {
    None,
    Text(&'a str),
    /// A header or a path, in the code font.
    Mono(&'a str),
    /// A translated template whose `{}` are filled with code spans.
    Code(&'a str, &'a [&'a str]),
}

/// Heights of the controls of settings rows, which set the row's height.
mod ctl {
    pub const SWITCH: f32 = 18.0;
    pub const SMALL: f32 = 26.0;
    pub const BUTTON_SM: f32 = 24.0;
    pub const KBD: f32 = 18.0;
    pub const TEXT: f32 = 17.0;
    pub const FIELD: f32 = 30.0;
}

/// A settings row (`.frow`) of a `kit::group`: the label (13 medium) and its
/// help (12 muted) take what the control leaves; the control (`ctrl_h`
/// high) sits on the right, vertically centered. `on: false` dims the row
/// and disables its control. The control is laid out right to left: add its
/// parts last to first.
fn frow(
    g: &mut kit::GroupRows,
    ui: &mut Ui,
    on: bool,
    label: &str,
    help: Help<'_>,
    ctrl_h: f32,
    add: impl FnOnce(&mut Ui),
) {
    g.block(ui, |ui| {
        let p = theme::pal(ui);
        let w = ui.available_width();
        let id = ui.make_persistent_id(("settings-row", label));
        let measured: Option<f32> = ui.data(|d| d.get_temp(id));
        let min_label = 140.0f32.min(w * 0.4);
        let label_w = (w - measured.unwrap_or(w * 0.4) - 14.0).max(min_label);
        let fade = |c: Color32| if on { c } else { c.gamma_multiply(0.45) };
        let lg = text_box(ui, label, theme::medium(13.0), p.text, label_w, true);
        let hg = match help {
            Help::None => None,
            Help::Text(t) => Some(text_box(ui, t, FontId::proportional(12.0), p.text_muted, label_w, true)),
            Help::Mono(t) => Some(text_box(ui, t, theme::mono(12.5), p.text_muted, label_w, true)),
            Help::Code(t, code) => Some(code_text(ui, t, code, 12.0, p.text_muted, label_w)),
        };
        let text_h = lg.size().y + hg.as_ref().map_or(0.0, |g| g.size().y + 1.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, text_h.max(ctrl_h)), Sense::hover());
        let y = rect.center().y - text_h / 2.0;
        let label_h = lg.size().y;
        ui.painter().galley(pos2(rect.left(), y), lg, fade(p.text));
        if let Some(hg) = hg {
            ui.painter().galley(pos2(rect.left(), y + label_h + 1.0), hg, fade(p.text_muted));
        }
        let area = Rect::from_min_max(pos2(rect.left() + min_label + 14.0, rect.top()), rect.max);
        let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(Layout::right_to_left(Align::Center)));
        child.spacing_mut().item_spacing = vec2(8.0, 6.0);
        if !on {
            child.disable();
        }
        add(&mut child);
        let used = child.min_rect().width();
        if measured.is_none_or(|m| (m - used).abs() > 0.5) {
            ui.data_mut(|d| d.insert_temp(id, used));
            ui.ctx().request_discard("settings row width");
        }
    });
}

/// A number field (`numUnit`): a small right-aligned input `width` wide and
/// a faint unit after it. Text that is not a number is ignored; the value
/// is kept in `range`.
pub(crate) fn number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    name: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
    width: f32,
) {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id(("number", name));
    let text_id = id.with("text");
    let shown = |v: N| {
        let f = v.to_f64();
        if N::INTEGRAL { format!("{}", f.round() as i64) } else { format!("{f}") }
    };
    let focused = ui.memory(|m| m.has_focus(id));
    let mut text: String =
        if focused { ui.data(|d| d.get_temp(text_id)).unwrap_or_else(|| shown(*value)) } else { shown(*value) };
    let rtl = ui.layout().prefer_right_to_left();
    let unit_label = |ui: &mut Ui| {
        if !unit.is_empty() {
            ui.label(RichText::new(unit).color(p.text_faint));
        }
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if rtl {
            unit_label(ui);
        }
        let edit = line_edit(ui, &mut text, "", true, false).id(id).align(Align2::RIGHT_CENTER).desired_width(width);
        let r = add_input(ui, edit, true);
        if !rtl {
            unit_label(ui);
        }
        widgets::a11y_name(&r, name);
        if r.changed()
            && let Ok(v) = text.trim().parse::<f64>()
        {
            *value = N::from_f64(v.clamp(range.start().to_f64(), range.end().to_f64()));
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(text_id, text));
        } else {
            ui.data_mut(|d| d.remove::<String>(text_id));
        }
    });
}

/// A size in MB stored as bytes.
fn megabytes(ui: &mut Ui, name: &str, bytes: &mut u64, max: u32) {
    let mut mb = (*bytes / (1024 * 1024)) as u32;
    let before = mb;
    number(ui, name, &mut mb, 1..=max, "MB", 72.0);
    if mb != before {
        *bytes = mb as u64 * 1024 * 1024;
    }
}

/// A slider (`.dl-slider`): 150 × 18, a 4 px track, the accent fill up to a
/// 16 px white knob. Arrow keys step it when focused.
fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32, name: &str) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(150.0, 18.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let before = *value;
    if (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        *value = ((lo + t * (hi - lo)) / step).round() * step;
    }
    if resp.has_focus() {
        let (less, more) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp),
            )
        });
        if less {
            *value -= step;
        }
        if more {
            *value += step;
        }
    }
    *value = value.clamp(lo, hi);
    if *value != before {
        resp.mark_changed();
    }
    let enabled = ui.is_enabled();
    let v = *value as f64;
    resp.widget_info(|| egui::WidgetInfo::slider(enabled, v, name));
    if ui.is_rect_visible(rect) {
        let t = (*value - lo) / (hi - lo).max(f32::EPSILON);
        let cy = rect.center().y;
        let x = rect.left() + t * rect.width();
        let track = Rect::from_min_max(pos2(rect.left(), cy - 2.0), pos2(rect.right(), cy + 2.0));
        ui.painter().rect_filled(track, CornerRadius::same(2), p.subtle);
        ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), CornerRadius::same(2), p.accent);
        let knob = pos2(x, cy);
        ui.painter().circle_filled(knob + vec2(0.0, 1.0), 8.5, Color32::from_black_alpha(36));
        ui.painter().circle(knob, 8.0, Color32::WHITE, Stroke::new(1.0, p.border_strong));
        if resp.has_focus() {
            ui.painter().circle_stroke(knob, 10.5, Stroke::new(1.5, p.accent));
        }
    }
    resp
}

/// A secret already in the secret store (`stored()`): an outlined `Stored
/// in the Keychain` pill and a Change button. Returns true when Change was
/// pressed.
fn stored(ui: &mut Ui) -> bool {
    let rtl = ui.layout().prefer_right_to_left();
    let pill = |ui: &mut Ui| {
        let o = kit::PillOpts { outline: true, icon: Some(icons::LOCK_SIMPLE), ..Default::default() };
        kit::pill_ex(ui, tr("Stored in the Keychain"), kit::Tone::Neutral, o);
    };
    if !rtl {
        pill(ui);
    }
    let change = kit::ghost(ui, None, tr("Change")).clicked();
    if rtl {
        pill(ui);
    }
    change
}

/// An editable path in the look of `.dl-pathbox` (30 high, `well` fill, mono 12).
fn path_input(ui: &mut Ui, value: &mut String, name: &str, width: f32) -> Response {
    let p = theme::pal(ui);
    let edit = egui::TextEdit::singleline(value)
        .font(theme::mono(12.0))
        .hint_text(RichText::new(tr("No file")).size(12.0).color(p.text_faint))
        .background_color(p.well)
        .margin(Margin::symmetric(11, 0))
        .min_size(vec2(0.0, 30.0))
        .align(Align2::LEFT_CENTER)
        .desired_width(width);
    let r = ui
        .scope(|ui| {
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.bg_stroke = Stroke::new(1.0, p.border);
            ui.spacing_mut().interact_size.y = 30.0;
            add_input(ui, edit, false)
        })
        .inner;
    widgets::a11y_name(&r, name);
    r
}

/// A file path field with a Choose button after it, laid out left to right.
fn path_field(ui: &mut Ui, value: &mut String, name: &str, width: f32) {
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        path_input(ui, value, name, width - 30.0);
        if kit::icon_button_sm(ui, icons::FOLDER_OPEN, tr("Choose file…")).clicked()
            && let Some(p) = rfd::FileDialog::new().pick_file()
        {
            *value = p.display().to_string();
        }
    });
}

fn settings(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    draft: &mut AppSettings,
    page: &mut SettingsPage,
) -> Outcome {
    let mut save = false;
    let changed = *draft != app.settings;
    let current = *page;
    let out =
        Modal::new("settings", 860.0).title(tr("Settings")).icon(icons::GEAR_SIX).sticky().center().show(ctx, |ui| {
            // The header is 58 high and the border 1 px at the top and the
            // bottom: the content keeps the dialog at 640.
            let height = room(ui).min(SETTINGS_H - 2.0 - 58.0 - FOOTER_H);
            split(
                ui,
                height,
                195.0,
                |ui| settings_nav(ui, page),
                |ui| {
                    egui::ScrollArea::vertical().id_salt(("settings-page", current)).auto_shrink([false; 2]).show(
                        ui,
                        |ui| {
                            egui::Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 28 }).show(
                                ui,
                                |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.spacing_mut().item_spacing.y = 20.0;
                                    settings_page(app, ui, draft, current);
                                },
                            );
                        },
                    );
                },
            );
            footer(ui, |ui| {
                let p = theme::pal(ui);
                if changed {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    kit::dot(ui, p.accent, 4.0);
                    ui.label(RichText::new(tr("Unsaved changes")).size(12.0).color(p.text));
                } else {
                    ui.label(RichText::new(tr("Changes apply when you save.")).size(12.0).color(p.text_muted));
                }
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if kit::primary(ui, tr("Save")).clicked() {
                        save = true;
                    }
                    if kit::secondary(ui, None, tr("Cancel")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    if save {
        app.apply_settings(draft.clone());
        return Outcome::Close;
    }
    out
}

/// The page list of Settings (`.dl-set-nav`), with a note at its bottom.
fn settings_nav(ui: &mut Ui, page: &mut SettingsPage) {
    let p = theme::pal(ui);
    let rect = ui.max_rect().shrink2(vec2(8.0, 10.0));
    in_rect(ui, rect, Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for pg in SettingsPage::ALL {
            if pane_row(ui, Some(pg.icon()), pg.label(), None, *page == pg, true).clicked() {
                *page = pg;
            }
        }
    });
    let note = tr("Settings are per user and apply to every workspace.");
    let g = text_box(ui, note, FontId::proportional(11.5), p.text_muted, rect.width() - 20.0, true);
    ui.painter().galley(pos2(rect.left() + 10.0, rect.bottom() - 8.0 - g.size().y), g, p.text_muted);
}

fn settings_page(app: &TernaryWireApp, ui: &mut Ui, draft: &mut AppSettings, page: SettingsPage) {
    let p = theme::pal(ui);
    if page != SettingsPage::About {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let w = ui.available_width();
            ui.label(text_box(ui, page.label(), theme::semibold(16.0), p.text, w, false));
            ui.label(text_box(ui, page.blurb(), FontId::proportional(12.5), p.text_muted, w, true));
        });
    }
    match page {
        SettingsPage::General => general_page(ui, draft),
        SettingsPage::Requests => requests_page(ui, draft),
        SettingsPage::Scripts => scripts_page(app, ui, draft),
        SettingsPage::Proxy => proxy_page(ui, draft),
        SettingsPage::Certificates => certificates_page(ui, draft),
        SettingsPage::Shortcuts => shortcuts_page(ui),
        SettingsPage::About => about_page(app, ui),
    }
}

fn general_page(ui: &mut Ui, draft: &mut AppSettings) {
    kit::group(ui, tr("Appearance"), "", |ui, g| {
        let help = Help::Text(tr("Follow the system, or always use light or dark."));
        frow(g, ui, true, tr("Theme"), help, ctl::SMALL, |ui| {
            kit::segmented_ex(
                ui,
                &mut draft.ui.theme,
                &[
                    (ThemePreference::System, Some(icons::MONITOR), tr("System")),
                    (ThemePreference::Light, Some(icons::SUN), tr("Light")),
                    (ThemePreference::Dark, Some(icons::MOON), tr("Dark")),
                ],
                true,
            );
        });
        let help = Help::Text(tr("The language of menus and messages."));
        frow(g, ui, true, tr("Language"), help, ctl::SMALL, |ui| {
            kit::segmented_sm(
                ui,
                &mut draft.ui.language,
                &[(UiLanguage::En, "English"), (UiLanguage::Id, "Bahasa Indonesia")],
            );
        });
        let help = Help::Text(tr("Text size of the whole interface."));
        frow(g, ui, true, tr("Font size"), help, ctl::SWITCH, |ui| {
            let p = theme::pal(ui);
            ui.spacing_mut().item_spacing.x = 12.0;
            slider(ui, &mut draft.ui.font_size, 10.0..=20.0, 0.5, tr("Font size"));
            let (r, _) = ui.allocate_exact_size(vec2(44.0, 18.0), Sense::hover());
            let text = format!("{} px", draft.ui.font_size);
            ui.painter().text(pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, text, theme::mono(12.0), p.text);
        });
        let help = Help::Text(tr("Where the response appears next to the request."));
        frow(g, ui, true, tr("Response panel"), help, ctl::SMALL, |ui| {
            kit::segmented_ex(
                ui,
                &mut draft.ui.layout,
                &[
                    (ResponseLayout::Below, Some(icons::ROWS), tr("Below")),
                    (ResponseLayout::Side, Some(icons::COLUMNS), tr("Side by side")),
                ],
                true,
            );
        });
    });
    kit::group(ui, tr("Updates"), "", |ui, g| {
        let label = tr("Check for updates on startup");
        let help = Help::Text(tr(
            "TernaryWire asks the release feed for the latest version and shows what is new. Nothing about you or your work is sent.",
        ));
        frow(g, ui, true, label, help, ctl::SWITCH, |ui| {
            toggle(ui, &mut draft.updates.check_on_startup, label);
        });
        let version = trf("TernaryWire {}", &[&crate::update::CURRENT]);
        let help = Help::Text(tr("The version installed on this computer."));
        frow(g, ui, true, &version, help, ctl::BUTTON_SM, |ui| {
            if kit::button_ex(ui, kit::Kind::Secondary, Some(icons::ARROWS_CLOCKWISE), tr("Check now"), true).clicked()
            {
                crate::update::request_check(ui.ctx());
            }
        });
    });
    kit::group(ui, tr("History"), "", |ui, g| {
        let help = Help::Text(tr("The oldest entries are removed when History is full."));
        frow(g, ui, true, tr("History entries"), help, ctl::SMALL, |ui| {
            number(ui, tr("History entries"), &mut draft.history_limit, 10..=100_000, "", 96.0);
        });
    });
}

fn requests_page(ui: &mut Ui, draft: &mut AppSettings) {
    let r = &mut draft.request;
    kit::group(ui, tr("Sending"), "", |ui, g| {
        let help = Help::Text(tr("How long to wait for a response; 0 waits forever."));
        frow(g, ui, true, tr("Request timeout"), help, ctl::SMALL, |ui| {
            number(ui, tr("Request timeout"), &mut r.timeout_ms, 0..=3_600_000, "ms", 96.0);
        });
        let help = Help::Text(tr("Auto lets client and server agree on one."));
        frow(g, ui, true, tr("HTTP version"), help, ctl::SMALL, |ui| {
            kit::segmented_sm(
                ui,
                &mut r.http_version,
                &[(HttpVersion::Auto, tr("Auto")), (HttpVersion::Http1, "HTTP/1.1"), (HttpVersion::Http2, "HTTP/2")],
            );
        });
        let help = Help::Text(tr("Percent-encode characters that are not allowed in a URL."));
        frow(g, ui, true, tr("Encode URL automatically"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut r.encode_url, tr("Encode URL automatically"));
        });
    });
    kit::group(ui, tr("Redirects"), "", |ui, g| {
        let help = Help::Text(tr("Go on to the address a 3xx response points to."));
        frow(g, ui, true, tr("Follow redirects"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut r.follow_redirects, tr("Follow redirects"));
        });
        frow(g, ui, r.follow_redirects, tr("Maximum redirects"), Help::None, ctl::SMALL, |ui| {
            number(ui, tr("Maximum redirects"), &mut r.max_redirects, 0..=100, "", 72.0);
        });
    });
    kit::group(ui, tr("Security"), "", |ui, g| {
        let help = Help::Text(tr("Turn this off only for local servers with self-signed certificates."));
        frow(g, ui, true, tr("Verify SSL certificates"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut r.verify_ssl, tr("Verify SSL certificates"));
        });
    });
    kit::group(ui, tr("Default headers"), "", |ui, g| {
        let agent = format!("User-Agent: {}", ternarywire_core::settings::USER_AGENT);
        frow(g, ui, true, tr("Send User-Agent header"), Help::Mono(&agent), ctl::SWITCH, |ui| {
            toggle(ui, &mut r.send_user_agent, tr("Send User-Agent header"));
        });
        let help = Help::Mono("Cache-Control: no-cache");
        frow(g, ui, true, tr("Send no-cache header"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut r.send_no_cache, tr("Send no-cache header"));
        });
    });
    kit::group(ui, tr("Responses"), "", |ui, g| {
        let help = Help::Text(tr("Larger bodies are kept in a temporary file."));
        frow(g, ui, true, tr("Keep bodies in memory up to"), help, ctl::SMALL, |ui| {
            megabytes(ui, tr("Keep bodies in memory up to"), &mut draft.max_body_in_memory_bytes, 1024);
        });
        let help = Help::Text(tr("Larger bodies are shown as plain text."));
        frow(g, ui, true, tr("Pretty-print bodies up to"), help, ctl::SMALL, |ui| {
            megabytes(ui, tr("Pretty-print bodies up to"), &mut draft.max_pretty_bytes, 200);
        });
    });
}

/// A titled section that is not a settings group: a section title with
/// buttons on its right (`add_buttons`, right to left), then `add`.
fn section(ui: &mut Ui, title: &str, add_buttons: impl FnOnce(&mut Ui), add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 24.0), Layout::left_to_right(Align::Center), |ui| {
            ui.label(kit::section_title(title).color(theme::pal(ui).text));
            trailing(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                add_buttons(ui);
            });
        });
        add(ui);
    });
}

fn scripts_page(app: &TernaryWireApp, ui: &mut Ui, draft: &mut AppSettings) {
    kit::group(ui, tr("Limits"), "", |ui, g| {
        let help = Help::Text(tr("Scripts that run longer are stopped."));
        frow(g, ui, true, tr("Script timeout"), help, ctl::SMALL, |ui| {
            number(ui, tr("Script timeout"), &mut draft.script_timeout_ms, 100..=600_000, "ms", 96.0);
        });
        let help = Help::Text(tr(
            "Scripts run in a separate process that is stopped when it uses more memory than this; 0 means no limit.",
        ));
        frow(g, ui, true, tr("Script memory limit"), help, ctl::SMALL, |ui| {
            number(ui, tr("Script memory limit"), &mut draft.script_memory_mb, 0..=65_536, "MB", 72.0);
        });
        frow(g, ui, true, tr("Script engine"), Help::None, ctl::TEXT, |ui| {
            ui.label(RichText::new(engine_label(app.scripts.name())).size(12.0).color(theme::pal(ui).text_muted));
        });
    });
    kit::group(ui, tr("Package registry"), "", |ui, g| {
        let code = ["pm.require('npm:…')", "pm.require('jsr:…')"];
        let help = Help::Code(tr("esm.sh-compatible; serves {} and {}."), &code);
        frow(g, ui, true, tr("Registry URL"), help, ctl::SMALL, |ui| {
            small_input(ui, &mut draft.package_registry, "https://esm.sh", tr("Registry URL"), 240.0, true);
        });
    });
    let root = app.ws.root().to_path_buf();
    let dir = root.join("packages");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "js" || x == "mjs" || x == "cjs") {
                out.push(p);
            }
        }
    }
    walk(&dir, &mut files);
    files.sort();
    let buttons = |ui: &mut Ui| {
        if kit::button_ex(ui, kit::Kind::Secondary, Some(icons::PLUS), tr("New package"), true).clicked() {
            let mut n = 1;
            let mut path = dir.join("my-helpers.js");
            while path.exists() {
                n += 1;
                path = dir.join(format!("my-helpers-{n}.js"));
            }
            let body = "// Shared helpers: pm.require('my-helpers') in any script.\nmodule.exports = {\n  isoNow: () => new Date().toISOString(),\n};\n";
            if std::fs::create_dir_all(&dir).is_ok()
                && ternarywire_core::storage::write_atomic(&path, body.as_bytes()).is_ok()
            {
                let _ = open::that(&path);
            }
        }
        if kit::ghost(ui, Some(icons::FOLDER_OPEN), tr("Open folder")).clicked() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = open::that(&dir);
        }
    };
    section(ui, tr("Package Library"), buttons, |ui| {
        let p = theme::pal(ui);
        let w = ui.available_width();
        let template = tr("Your own modules in {}, shared by every script in the workspace.");
        ui.label(code_text(ui, template, &["packages/"], 12.0, p.text_muted, w));
        vlist(ui, |ui| {
            if files.is_empty() {
                vrow(ui, (0, 1), 38.0, false, |ui, _| {
                    tile(ui, icons::PACKAGE, false);
                    ui.label(RichText::new(tr("No packages yet.")).size(12.0).color(p.text_muted));
                });
            }
            for (i, f) in files.iter().enumerate() {
                let rel = f.strip_prefix(&dir).unwrap_or(f).display().to_string().replace('\\', "/");
                let name = rel
                    .trim_end_matches(".js")
                    .trim_end_matches(".mjs")
                    .trim_end_matches(".cjs")
                    .trim_end_matches("/index")
                    .to_string();
                vrow(ui, (i, files.len()), 38.0, false, |ui, hovered| {
                    tile(ui, icons::PACKAGE, false);
                    ui.label(RichText::new(format!("pm.require('{name}')")).font(theme::mono(12.0)).color(p.text));
                    trailing(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.scope(|ui| {
                            if !hovered {
                                ui.set_invisible();
                            }
                            if kit::ghost(ui, None, tr("Open")).clicked() {
                                let _ = open::that(f);
                            }
                        });
                        ui.label(RichText::new(format!("packages/{rel}")).size(11.0).color(p.text_faint));
                    });
                });
            }
        });
        let lock = root.join("packages.lock.json");
        if let Ok(text) = std::fs::read_to_string(&lock) {
            let v = serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default();
            let count = |k: &str| v.get(k).and_then(|m| m.as_object()).map(|o| o.len()).unwrap_or(0);
            kit::well(ui).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                let summary = trf(
                    "packages.lock.json: {} {} · {} {}",
                    &[&count("packages"), &tr("packages"), &count("modules"), &tr("pinned modules")],
                );
                ui.label(RichText::new(summary).size(12.0).color(p.text_muted));
                ui.add_space(2.0);
                for (spec, url) in v.get("packages").and_then(|m| m.as_object()).into_iter().flatten() {
                    let mut job = LayoutJob::default();
                    let line = Some((11.5f32 * 1.7).round());
                    let f = |color| TextFormat {
                        font_id: theme::mono(11.5),
                        color,
                        line_height: line,
                        ..Default::default()
                    };
                    job.append(spec, 0.0, f(p.text));
                    job.append(&format!(" → {}", url.as_str().unwrap_or("")), 0.0, f(p.text_faint));
                    job.wrap = TextWrapping::truncate_at_width(ui.available_width());
                    ui.label(job);
                }
            });
        }
    });
}

fn proxy_page(ui: &mut Ui, draft: &mut AppSettings) {
    let px = &mut draft.proxy;
    kit::group(ui, tr("System proxy"), "", |ui, g| {
        let help = Help::Text(tr(
            "Honours HTTP_PROXY / HTTPS_PROXY / NO_PROXY and the OS proxy settings, including a proxy auto-config (PAC) script.",
        ));
        frow(g, ui, true, tr("Use system proxy"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut px.use_system, tr("Use system proxy"));
        });
    });
    kit::group(ui, tr("Custom proxy"), "", |ui, g| {
        let help = Help::Text(tr("Send requests through a proxy you set here."));
        frow(g, ui, true, tr("Use custom proxy"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut px.custom_enabled, tr("Use custom proxy"));
        });
        let on = px.custom_enabled;
        frow(g, ui, on, tr("Proxy URL"), Help::None, ctl::SMALL, |ui| {
            let hint = tr("http://proxy:3128 or socks5://127.0.0.1:1080");
            small_input(ui, &mut px.custom_url, hint, tr("Proxy URL"), 260.0, true);
        });
        frow(g, ui, on, tr("Username"), Help::None, ctl::SMALL, |ui| {
            small_input(ui, &mut px.username, "", tr("Username"), 260.0, false);
        });
        let help = Help::Text(tr("Stored in the Keychain, not in the settings file."));
        frow(g, ui, on, tr("Password"), help, ctl::SMALL, |ui| {
            if ternarywire_core::variables::is_pure_reference(&px.password) {
                if stored(ui) {
                    px.password.clear();
                }
            } else {
                let r = secret_input(ui, "proxy-pass", &mut px.password, "", 260.0);
                widgets::a11y_name(&r, tr("Password"));
            }
        });
        let help = Help::Text(tr("Hosts reached directly, separated by commas."));
        frow(g, ui, on, tr("Bypass"), help, ctl::SMALL, |ui| {
            small_input(ui, &mut px.bypass, tr("localhost, *.internal"), tr("Bypass"), 260.0, true);
        });
        let help = Help::Text(tr("A PAC script picks the proxy for each request instead of the proxy URL."));
        frow(g, ui, on, tr("Auto-config (PAC) URL"), help, ctl::SMALL, |ui| {
            let name = tr("Auto-config (PAC) URL");
            small_input(ui, &mut px.pac_url, "http://wpad.example/proxy.pac", name, 260.0, true);
        });
    });
}

fn certificates_page(ui: &mut Ui, draft: &mut AppSettings) {
    let certs = &mut draft.certificates;
    kit::group(ui, tr("Certificate authorities"), "", |ui, g| {
        let help = Help::Text(tr("Trusted in addition to the system certificate store."));
        frow(g, ui, true, tr("Trust an extra CA certificate (PEM)"), help, ctl::SWITCH, |ui| {
            toggle(ui, &mut certs.ca_enabled, tr("Trust an extra CA certificate (PEM)"));
        });
        frow(g, ui, certs.ca_enabled, tr("CA file"), Help::None, ctl::FIELD, |ui| {
            if kit::icon_button_sm(ui, icons::FOLDER_OPEN, tr("Choose file…")).clicked()
                && let Some(p) = rfd::FileDialog::new().pick_file()
            {
                certs.ca_path = p.display().to_string();
            }
            ui.add_space(-2.0);
            path_input(ui, &mut certs.ca_path, tr("CA file"), 272.0);
        });
    });
    let mut add_clicked = false;
    section(
        ui,
        tr("Client certificates"),
        |ui| {
            if kit::ghost(ui, Some(icons::PLUS), tr("Add certificate")).clicked() {
                add_clicked = true;
            }
        },
        |ui| client_certificates(ui, &mut certs.client_certs),
    );
    if add_clicked {
        certs.client_certs.push(ClientCertificate { enabled: true, ..Default::default() });
    }
}

/// The client certificates, one card each: on/off and host, the three
/// files, the passphrase.
fn client_certificates(ui: &mut Ui, certs: &mut Vec<ClientCertificate>) {
    if certs.is_empty() {
        kit::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(6.0);
            compact_empty(
                ui,
                icons::SHIELD_CHECK,
                tr("No client certificates"),
                tr("Add one to send it to servers that ask for it (mutual TLS)."),
            );
            ui.add_space(6.0);
        });
    }
    let mut remove = None;
    for (i, c) in certs.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            kit::card(ui).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                let w = ui.available_width();
                // Rows as tall as their controls (`ui.horizontal` would make them 30).
                let row = |ui: &mut Ui, h: f32, add: &mut dyn FnMut(&mut Ui)| {
                    ui.allocate_ui_with_layout(vec2(w, h), Layout::left_to_right(Align::Center), |ui| add(ui));
                };
                row(ui, 26.0, &mut |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    toggle(ui, &mut c.enabled, tr("Enabled"));
                    trailing(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        if kit::icon_button_sm(ui, icons::TRASH, tr("Remove certificate")).clicked() {
                            remove = Some(i);
                        }
                        let w = ui.available_width();
                        let hint = tr("api.example.com:443, *.example.com or *");
                        small_input(ui, &mut c.host, hint, tr("Host"), w, true);
                    });
                });
                ui.add_space(4.0);
                let label_w = 96.0;
                for (label, value) in
                    [("CRT (PEM)", &mut c.cert_path), ("KEY (PEM)", &mut c.key_path), ("PFX / P12", &mut c.pfx_path)]
                {
                    row(ui, 30.0, &mut |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        cell_label(ui, label, label_w, 30.0);
                        path_field(ui, value, label, w - label_w - 8.0);
                    });
                }
                // As tall as the control: the pill and its button, or an input.
                let stored_pass = ternarywire_core::variables::is_pure_reference(&c.passphrase);
                let h = if stored_pass { 24.0 } else { 26.0 };
                row(ui, h, &mut |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    cell_label(ui, tr("Passphrase"), label_w, h);
                    if stored_pass {
                        if stored(ui) {
                            c.passphrase.clear();
                        }
                    } else {
                        let r = secret_input(ui, ("cert-pass", i), &mut c.passphrase, "", w - label_w - 8.0);
                        widgets::a11y_name(&r, tr("Passphrase"));
                    }
                });
                ui.add_space(2.0);
                let hint = tr(
                    "A PKCS#12 bundle replaces the PEM files. The passphrase is stored in the Keychain, not in the settings file.",
                );
                ui.add(egui::Label::new(kit::hint(ui, hint).line_height(Some(12.0 * 1.45))).wrap());
            });
        });
    }
    if let Some(i) = remove {
        certs.remove(i);
    }
}

/// A muted 12 px label in a fixed-width column, `height` high, for small forms.
fn cell_label(ui: &mut Ui, text: &str, width: f32, height: f32) {
    let p = theme::pal(ui);
    ui.allocate_ui_with_layout(vec2(width, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(width);
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.text_muted)).truncate());
    });
}

/// Key caps for a shortcut written as `⌘ N` (one cap per key) or, for two
/// shortcuts, `⇧ ⌘ [ ⇧ ⌘ ]`. Laid out right to left.
fn shortcut_chips(ui: &mut Ui, keys: &str) {
    ui.spacing_mut().item_spacing.x = 3.0;
    for k in keys.split(' ').rev() {
        kit::kbd(ui, k);
    }
}

fn shortcuts_page(ui: &mut Ui) {
    let groups: [(&str, &[(&str, &str)]); 3] = [
        (
            "General",
            &[
                ("New request", "⌘ N"),
                ("New request (tab)", "⌘ T"),
                ("Send request · run or stop a flow", "⌘ ↩"),
                ("Send a message (WebSocket, gRPC, AI…)", "⇧ ⌘ ↩"),
                ("Save", "⌘ S"),
                ("Close tab", "⌘ W"),
            ],
        ),
        (
            "Navigation",
            &[
                ("Search requests", "⌘ K"),
                ("Command palette", "⇧ ⌘ P"),
                ("Switch environment", "⌘ E"),
                ("Toggle sidebar", "⌘ B"),
                ("Toggle console", "⌥ ⌘ C"),
                ("Settings", "⌘ ,"),
                ("Previous / next tab", "⇧ ⌘ [ ⇧ ⌘ ]"),
            ],
        ),
        ("Editing", &[("Accept a variable suggestion", "Tab"), ("Close a dialog or filter", "esc")]),
    ];
    for (title, rows) in groups {
        kit::group(ui, tr(title), "", |ui, g| {
            for (action, keys) in rows {
                frow(g, ui, true, tr(action), Help::None, ctl::KBD, |ui| shortcut_chips(ui, keys));
            }
        });
    }
    ui.label(kit::hint(ui, tr("⌘ is Ctrl on Windows and Linux.")).line_height(Some(12.0 * 1.45)));
}

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// What runs scripts, for people: the engine and where it runs.
fn engine_label(name: &str) -> String {
    match name {
        "boa (isolated)" => tr("Boa · separate worker process").to_string(),
        "boa" => tr("Boa · in the app process").to_string(),
        other => other.to_string(),
    }
}

/// The license as words: `MIT or Apache-2.0` for the SPDX `MIT OR Apache-2.0`.
fn license() -> String {
    match env!("CARGO_PKG_LICENSE").split_once(" OR ") {
        Some((a, b)) => trf("{} or {}", &[&a, &b]),
        None => env!("CARGO_PKG_LICENSE").to_string(),
    }
}

fn about_page(app: &TernaryWireApp, ui: &mut Ui) {
    let p = theme::pal(ui);
    // A centered column (gap 8, padding 12 0 4) with line boxes of 1.45.
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(12.0);
        ui.add(
            egui::Image::new(egui::include_image!("../assets/icon-256.png"))
                .fit_to_exact_size(vec2(76.0, 76.0))
                .corner_radius(18),
        );
        ui.label(RichText::new("TernaryWire").font(theme::semibold(20.0)).color(p.text).line_height(Some(29.0)));
        kit::tone_pill(ui, &trf("Version {}", &[&crate::update::CURRENT]), kit::Tone::Accent);
        ui.scope(|ui| {
            ui.set_max_width(420.0);
            let blurb = tr("A lightweight API client written in Rust. No account, no telemetry: data stays in your workspace folder.");
            ui.add(
                egui::Label::new(RichText::new(blurb).color(p.text_muted).line_height(Some(13.0 * 1.45)))
                    .halign(Align::Center)
                    .wrap(),
            );
        });
        // Centered row of links: measured on the previous frame.
        let links = [
            (icons::GIT_BRANCH, tr("Source code"), REPOSITORY.to_string()),
            (icons::NOTE, tr("Release notes"), format!("{REPOSITORY}/releases")),
            (icons::BUG, tr("Report an issue"), format!("{REPOSITORY}/issues")),
        ];
        let id = ui.scope_id().with("about-links");
        let row_w: f32 = ui.data(|d| d.get_temp(id).unwrap_or(330.0));
        ui.add_space(6.0);
        let w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(w, 24.0), Layout::left_to_right(Align::Center), |ui| {
            ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));
            let start = ui.cursor().left();
            for (icon, label, url) in &links {
                let b = kit::button_ex(ui, kit::Kind::Secondary, Some(*icon), label, true);
                if b.on_hover_text(url).clicked() {
                    widgets::open_web_link(url);
                }
            }
            let w = ui.min_rect().right() - start;
            if (w - row_w).abs() > 0.5 {
                ui.data_mut(|d| d.insert_temp(id, w));
            }
        });
    });
    // The column's bottom padding, before the page's gap.
    ui.add_space(4.0);
    kit::group(ui, tr("System"), "", |ui, g| {
        let path = app.settings_path.display().to_string();
        let shown = crate::app::tilde_path(&app.settings_path);
        frow(g, ui, true, tr("Settings file"), Help::Mono(&shown), ctl::BUTTON_SM, |ui| {
            if kit::ghost(ui, Some(icons::COPY), tr("Copy path")).clicked() {
                ui.ctx().copy_text(path.clone());
            }
        });
        let muted = |ui: &mut Ui, text: String| {
            let label = egui::Label::new(RichText::new(&text).size(12.0).color(theme::pal(ui).text_muted)).truncate();
            ui.add(label).on_hover_text(text);
        };
        frow(g, ui, true, tr("Secrets"), Help::None, ctl::TEXT, |ui| muted(ui, app.ws.secrets.backend_description()));
        frow(g, ui, true, tr("Script engine"), Help::None, ctl::TEXT, |ui| muted(ui, engine_label(app.scripts.name())));
        frow(g, ui, true, tr("License"), Help::None, ctl::TEXT, |ui| muted(ui, license()));
    });
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

fn cookies(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    domain: &mut Option<String>,
    new_cookie: &mut String,
    error: &mut Option<String>,
    new_domain: &mut String,
) -> Outcome {
    let jar = app.ws.cookies.clone();
    let all = jar.list();
    let mut domains = jar.domains();
    if let Some(d) = domain.as_ref()
        && !domains.contains(d)
    {
        domains.push(d.clone());
    }
    if domain.is_none() {
        *domain = domains.first().cloned();
    }
    let mut changed = false;
    let mut picked: Option<String> = None;
    let subtitle = match (all.len(), domains.len()) {
        (0, _) => tr("Cookies set by responses are kept here and sent with matching requests.").to_string(),
        (n, d) => format!("{} · {}", plural(n, "{} cookie", "{} cookies"), plural(d, "{} domain", "{} domains")),
    };
    let out = Modal::new("cookies", 880.0).title(tr("Cookies")).subtitle(subtitle).icon(icons::COOKIE).center().show(
        ctx,
        |ui| {
            let current = domain.clone();
            let height = room(ui).min(440.0);
            split(
                ui,
                height,
                229.0,
                |ui| {
                    let p = theme::pal(ui);
                    pane_head(ui, tr("Domains"), &domains.len().to_string());
                    let list_h = (ui.available_height() - 43.0).max(40.0);
                    egui::ScrollArea::vertical()
                        .id_salt("cookie-domains")
                        .max_height(list_h)
                        .min_scrolled_height(list_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::new().inner_margin(Margin::same(6)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 1.0;
                                if domains.is_empty() {
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.add_space(4.0);
                                        ui.label(RichText::new(tr("No domains yet")).size(12.0).color(p.text_muted));
                                    });
                                }
                                for d in &domains {
                                    let n = all.iter().filter(|c| &c.domain == d).count().to_string();
                                    let on = current.as_deref() == Some(d);
                                    if nav_item(ui, Some(icons::GLOBE_SIMPLE), d, Some(n.as_str()), on).clicked() {
                                        picked = Some(d.clone());
                                    }
                                }
                            });
                        });
                    let r = ui.max_rect();
                    let foot = Rect::from_min_max(pos2(r.left(), r.bottom() - 43.0), r.max);
                    ui.painter().hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, p.border));
                    let mut foot_ui =
                        ui.new_child(UiBuilder::new().max_rect(foot.shrink(8.0).translate(vec2(0.0, 0.5))));
                    let w = foot_ui.available_width();
                    let r = small_input(&mut foot_ui, new_domain, tr("Add a domain…"), tr("Add a domain"), w, false);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !new_domain.trim().is_empty() {
                        picked = Some(new_domain.trim().to_string());
                        new_domain.clear();
                    }
                },
                |ui| {
                    let Some(d) = current.clone() else {
                        ui.add_space(ui.available_height() / 2.0 - 50.0);
                        compact_empty(
                            ui,
                            icons::COOKIE,
                            tr("No cookies yet"),
                            tr("Cookies set by responses appear here. Add a domain to set one by hand."),
                        );
                        return;
                    };
                    let list: Vec<_> = all.iter().filter(|c| c.domain == d).collect();
                    pane_bar(ui, 48.0, 16.0, |ui| {
                        let p = theme::pal(ui);
                        ui.label(RichText::new(&d).font(theme::semibold(13.0)).color(p.text));
                        ui.label(
                            RichText::new(plural(list.len(), "{} cookie", "{} cookies")).size(12.0).color(p.text_faint),
                        );
                        trailing(ui, |ui| {
                            let r = ui.add_enabled_ui(!list.is_empty(), |ui| {
                                danger_button(ui, false, true, Some(icons::TRASH), tr("Clear all"))
                            });
                            if r.inner.clicked() {
                                jar.clear_domain(&d);
                                changed = true;
                            }
                        });
                    });
                    egui::ScrollArea::vertical().id_salt("cookie-page").auto_shrink([false; 2]).show(ui, |ui| {
                        egui::Frame::new().inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 14.0;
                            changed |= cookie_table(ui, &jar, &list, new_cookie, error);
                            changed |= add_cookie(ui, &jar, &d, new_cookie, error);
                        });
                    });
                },
            );
            footer(ui, |ui| {
                icon_hint(ui, icons::LOCK_SIMPLE, tr("Stored in this workspace, never committed."));
                trailing(ui, |ui| {
                    if kit::secondary(ui, None, tr("Close")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        },
    );
    if let Some(d) = picked {
        *domain = Some(d);
        *error = None;
    }
    if changed {
        app.ws.mark_cookies();
    }
    out
}

/// The cookies of one domain (`.tbl`): Name 22 % · Value · Path 12 % ·
/// Expires 22 % · actions 64 px; the row being edited is selected and the
/// actions show on hover. Returns true when the jar changed.
fn cookie_table(
    ui: &mut Ui,
    jar: &ternarywire_http::cookies::CookieJar,
    list: &[&ternarywire_http::cookies::CookieInfo],
    new_cookie: &mut String,
    error: &mut Option<String>,
) -> bool {
    let p = theme::pal(ui);
    let mut changed = false;
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = ui.available_width();
            let widths = [w * 0.22, 0.0, w * 0.12, w * 0.22, 64.0];
            let value_w = w - widths.iter().sum::<f32>();
            let widths = [widths[0], value_w, widths[2], widths[3], widths[4]];
            let starts: Vec<f32> = widths
                .iter()
                .scan(0.0, |x, w| {
                    let s = *x;
                    *x += w;
                    Some(s)
                })
                .collect();
            let (head, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
            let r = theme::RADIUS_CARD - 1;
            ui.painter().rect_filled(head, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, p.well);
            ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));
            for (i, t) in [tr("Name"), tr("Value"), tr("Path"), tr("Expires")].into_iter().enumerate() {
                let g = line_galley(ui, t, theme::medium(11.5), widths[i] - 24.0);
                ui.painter().galley(
                    pos2(head.left() + starts[i] + 12.0, head.center().y - g.size().y / 2.0),
                    g,
                    p.text_muted,
                );
            }
            if list.is_empty() {
                egui::Frame::new().inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.label(
                        RichText::new(tr("No cookies for this domain. Add one below.")).size(12.5).color(p.text_muted),
                    );
                });
            }
            for (i, c) in list.iter().enumerate() {
                let set_cookie = c.to_set_cookie();
                let selected = !new_cookie.is_empty() && *new_cookie == set_cookie;
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                let last = i + 1 == list.len();
                let corners = if last { CornerRadius { nw: 0, ne: 0, sw: r, se: r } } else { CornerRadius::ZERO };
                let hovered = ui.rect_contains_pointer(rect);
                if selected {
                    ui.painter().rect_filled(rect, corners, p.accent_soft);
                } else if hovered {
                    ui.painter().rect_filled(rect, corners, p.well);
                }
                if !last {
                    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
                }
                let cy = rect.center().y;
                let at = |col: usize| rect.left() + starts[col] + 12.0;
                let cell = |col: usize| widths[col] - 24.0;
                let name = line_galley(ui, &c.name, theme::mono(12.5), cell(0));
                ui.painter().galley(pos2(at(0), cy - name.size().y / 2.0), name, p.text);
                let value = line_galley(ui, &c.value, theme::mono(12.5), cell(1));
                ui.painter().galley(pos2(at(1), cy - value.size().y / 2.0), value, p.text_muted);
                let path = line_galley(ui, &c.path, theme::mono(12.5), cell(2));
                ui.painter().galley(pos2(at(2), cy - path.size().y / 2.0), path, p.text);
                match c.expires.and_then(|e| chrono::DateTime::from_timestamp(e, 0)) {
                    Some(t) => {
                        let text = t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string();
                        let g = line_galley(ui, &text, FontId::proportional(12.5), cell(3));
                        ui.painter().galley(pos2(at(3), cy - g.size().y / 2.0), g, p.text_muted);
                    }
                    None => {
                        let cell_rect =
                            Rect::from_min_max(pos2(at(3), rect.top()), pos2(at(3) + cell(3), rect.bottom()));
                        let mut cell = ui.new_child(
                            UiBuilder::new().max_rect(cell_rect).layout(Layout::left_to_right(Align::Center)),
                        );
                        kit::tone_pill(&mut cell, tr("Session"), kit::Tone::Neutral);
                    }
                }
                let _ = resp.on_hover_text(RichText::new(&set_cookie).monospace());
                if hovered || selected {
                    let actions = Rect::from_min_max(pos2(rect.right() - 56.0, rect.top()), rect.max - vec2(8.0, 0.0));
                    let mut child =
                        ui.new_child(UiBuilder::new().max_rect(actions).layout(Layout::right_to_left(Align::Center)));
                    child.spacing_mut().item_spacing.x = 0.0;
                    if kit::icon_button_sm(&mut child, icons::TRASH, tr("Delete cookie")).clicked() {
                        jar.remove(&c.domain, &c.path, &c.name);
                        changed = true;
                    }
                    if kit::icon_button_sm(&mut child, icons::PENCIL_SIMPLE, tr("Edit cookie")).clicked() {
                        *new_cookie = set_cookie.clone();
                        *error = None;
                    }
                }
            }
        });
    changed
}

/// "Add or replace a cookie": a Set-Cookie field and Add. Returns true when
/// the jar changed.
fn add_cookie(
    ui: &mut Ui,
    jar: &ternarywire_http::cookies::CookieJar,
    d: &str,
    new_cookie: &mut String,
    error: &mut Option<String>,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(kit::section_title(tr("Add or replace a cookie")).color(theme::pal(ui).text));
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), Layout::right_to_left(Align::Center), |ui| {
            let add = kit::button_ex(ui, kit::Kind::Secondary, Some(icons::PLUS), tr("Add"), true).clicked();
            let w = ui.available_width();
            let r = small_input(ui, new_cookie, tr("name=value; Path=/; Expires=…"), tr("Cookie"), w, true);
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (add || enter) && !new_cookie.trim().is_empty() {
                match jar.set(d, new_cookie) {
                    Ok(()) => {
                        new_cookie.clear();
                        *error = None;
                        changed = true;
                    }
                    Err(e) => *error = Some(e),
                }
            }
        });
        ui.label(kit::hint(ui, tr("Paste a Set-Cookie value. Hover a row to see its Set-Cookie text.")));
        if let Some(e) = error {
            banner(ui, Tone::Err, e);
        }
    });
    changed
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Points along a rounded rectangle, for a dashed outline.
fn rounded_outline(rect: Rect, r: f32) -> Vec<egui::Pos2> {
    let mut pts = Vec::new();
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        for k in 0..=6 {
            let a = (start + 90.0 * k as f32 / 6.0).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts.push(pts[0]);
    pts
}

/// Reads a file to import.
fn read_import(path: &std::path::Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

/// `1 cookie` / `3 cookies` (the Indonesian table has both forms).
pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { trf(one, &[&n]) } else { trf(many, &[&n]) }
}

/// A recessed code box (`.dl-codebox`): an editable, highlighted text of
/// `height` on the `well` fill, frameless inside, mono 12.5 on 20 px lines.
fn code_box(
    ui: &mut Ui,
    id: &str,
    text: &mut String,
    lang: crate::highlight::Lang,
    hint: &str,
    height: f32,
) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    kit::well(ui)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // egui paints placeholders in the weak text color.
            ui.visuals_mut().weak_text_color = Some(p.text_faint);
            let h = height - 2.0;
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(h)
                .min_scrolled_height(h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                        let key = crate::highlight::HlKey {
                            text: buf.as_str(),
                            lang,
                            dark,
                            search: "",
                            vars: None,
                            font_size: 12.5,
                            wrap: true,
                        };
                        let mut job = crate::highlight::cached(ui.ctx(), key);
                        job.wrap.max_width = wrap_width;
                        for s in &mut job.sections {
                            s.format.line_height = Some(20.0);
                        }
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                    let edit = egui::TextEdit::multiline(text)
                        .id_salt(id)
                        .font(theme::mono(12.5))
                        .hint_text(RichText::new(hint).font(theme::mono(12.5)).color(p.text_faint))
                        // A framed TextEdit ignores `margin`: the padding goes in the frame.
                        .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(14, 10)))
                        .min_size(vec2(0.0, h))
                        .desired_width(f32::INFINITY)
                        .lock_focus(true)
                        .layouter(&mut layouter);
                    let r = ui.add(edit);
                    widgets::a11y_name(&r, hint);
                    r
                })
                .inner
        })
        .inner
}

fn import(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    text: &mut String,
    error: &mut Option<String>,
    preview: &mut Option<ternarywire_convert::Imported>,
    source: &mut String,
) -> Outcome {
    let mut do_import = false;
    // A file dropped anywhere on the window while the dialog is open.
    let dropped = ctx.input(|i| i.raw.dropped_files.first().cloned());
    if let Some(f) = dropped {
        let read = read_import(f.path())
            .or_else(|e| f.bytes().map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|_| e));
        match read {
            Ok(t) => {
                *text = t;
                *source = f.path().display().to_string();
                *preview = None;
                *error = None;
            }
            Err(e) => *error = Some(e),
        }
    }
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    let out = Modal::new("import", 700.0)
        .title(tr("Import"))
        .subtitle(tr("Bring in collections, environments and requests from other tools."))
        .icon(icons::DOWNLOAD_SIMPLE)
        .sticky()
        .show(ctx, |ui| {
            body(ui, |ui| {
                let p = theme::pal(ui);
                ui.spacing_mut().item_spacing.y = 14.0;
                if source.is_empty() || hovering {
                    if drop_zone(ui, hovering)
                        && let Some(path) = rfd::FileDialog::new().pick_file()
                    {
                        match read_import(&path) {
                            Ok(t) => {
                                *text = t;
                                *source = path.display().to_string();
                                *preview = None;
                                *error = None;
                            }
                            Err(e) => *error = Some(e),
                        }
                    }
                } else if file_chip(ui, source) {
                    source.clear();
                    text.clear();
                }
                // "or paste" divider
                let label = tr("or paste the content");
                let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(11.5), p.text_faint);
                // As tall as the prototype's line box (11.5 × 1.45).
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
                let (gw, mid) = (g.size().x, r.center());
                ui.painter().hline(r.left()..=mid.x - gw / 2.0 - 10.0, mid.y, Stroke::new(1.0, p.border));
                ui.painter().hline(mid.x + gw / 2.0 + 10.0..=r.right(), mid.y, Stroke::new(1.0, p.border));
                ui.painter().galley(mid - g.size() / 2.0, g, p.text_faint);
                let trimmed = text.trim_start();
                let lang = if trimmed.starts_with('{') || trimmed.starts_with('[') {
                    crate::highlight::Lang::Json
                } else if trimmed.starts_with('<') {
                    crate::highlight::Lang::Xml
                } else if trimmed.starts_with("openapi") || trimmed.starts_with("swagger") {
                    crate::highlight::Lang::Yaml
                } else {
                    crate::highlight::Lang::Text
                };
                let changed = if source.is_empty() {
                    let hint = tr("A Postman or OpenAPI file, a HAR export or a cURL command…");
                    code_box(ui, "import-text", text, lang, hint, 150.0).changed()
                } else {
                    // The file's text is not shown: clearing the file frees the box.
                    note_box(ui, tr("Clear the file to paste content instead."), 96.0);
                    false
                };
                if changed || (preview.is_none() && error.is_none() && !text.trim().is_empty()) {
                    match ternarywire_convert::import_auto(text) {
                        Ok(pr) => {
                            *preview = Some(pr);
                            *error = None;
                        }
                        Err(e) => {
                            *preview = None;
                            *error = Some(e.to_string());
                        }
                    }
                }
                if text.trim().is_empty() {
                    *preview = None;
                    if source.is_empty() {
                        *error = None;
                    }
                }
                if let Some(e) = error {
                    banner(ui, Tone::Err, &trf("This can't be imported: {}", &[&e]));
                }
                match preview {
                    Some(pr) => import_summary(ui, text, pr),
                    None if error.is_none() => {
                        let hint = tr(
                            "The format is detected as you paste. A file dropped anywhere on the window is imported too.",
                        );
                        ui.add(egui::Label::new(kit::hint(ui, hint).line_height(Some(12.0 * 1.45))).wrap());
                    }
                    None => {}
                }
            });
            footer(ui, |ui| {
                let p = theme::pal(ui);
                ui.label(RichText::new(tr("Imported items are added to this workspace.")).size(12.0).color(p.text_muted));
                trailing(ui, |ui| {
                    let r = ui.add_enabled_ui(preview.is_some(), |ui| {
                        kit::primary_icon(ui, icons::DOWNLOAD_SIMPLE, tr("Import"))
                    });
                    if r.inner.clicked() {
                        do_import = true;
                    }
                    if kit::secondary(ui, None, tr("Cancel")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    if do_import && let Some(p) = preview.take() {
        let n = p.collections.len();
        for mut c in p.collections {
            if app.ws.data.collection(&c.id).is_some() {
                c.id = ternarywire_core::util::new_id();
            }
            let id = app.ws.add_collection(c);
            app.sidebar.expanded.insert(id);
        }
        for mut e in p.environments {
            if app.ws.data.environment(&e.id).is_some() {
                e.id = ternarywire_core::util::new_id();
            }
            app.ws.add_environment(e);
        }
        if !p.globals.is_empty() {
            for g in p.globals {
                app.ws.data.globals.retain(|x| x.key != g.key);
                app.ws.data.globals.push(g);
            }
            app.ws.mark_globals();
        }
        app.sidebar_tab = crate::app::SidebarTab::Collections;
        app.toast(trf("Imported {} collection(s)", &[&n]), false);
        return Outcome::Close;
    }
    out
}

/// The drop zone (`.dl-dz`): 112 high, a dashed outline on the `well`; a
/// button that opens the file picker. Returns true when clicked.
fn drop_zone(ui: &mut Ui, hovering: bool) -> bool {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 112.0), Sense::click());
    widgets::a11y_button(&resp, tr("Choose file…"));
    let hot = hovering || resp.hovered();
    ui.painter().rect_filled(rect, CornerRadius::same(12), if hot { p.accent_soft } else { p.well });
    let stroke = Stroke::new(1.5, if hot { p.accent } else { p.border_strong });
    ui.painter().extend(egui::Shape::dashed_line(&rounded_outline(rect.shrink(0.75), 12.0), stroke, 5.0, 4.0));
    let cy = rect.center().y;
    ui.painter().text(
        pos2(rect.center().x, cy - 23.0),
        Align2::CENTER_CENTER,
        icons::FILE_ARROW_DOWN,
        theme::icon_font(24.0),
        p.accent_text,
    );
    let title = if hovering { tr("Drop to import") } else { tr("Drop a file here, or click to choose one") };
    ui.painter().text(pos2(rect.center().x, cy + 4.5), Align2::CENTER_CENTER, title, theme::medium(13.0), p.text);
    ui.painter().text(
        pos2(rect.center().x, cy + 26.5),
        Align2::CENTER_CENTER,
        "Postman · OpenAPI / Swagger · Insomnia · HAR · Hoppscotch · Thunder Client · WSDL · cURL",
        FontId::proportional(12.0),
        p.text_faint,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// The chosen file: a tile, its name and folder, and a clear button.
/// Returns true when it was cleared.
fn file_chip(ui: &mut Ui, source: &str) -> bool {
    let p = theme::pal(ui);
    let mut clear = false;
    let path = std::path::Path::new(source);
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| source.to_string());
    let dir = path.parent().map(home_relative).unwrap_or_default();
    kit::well(ui).corner_radius(CornerRadius::same(theme::RADIUS_CARD)).inner_margin(Margin::symmetric(12, 10)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            // Name and folder in CSS line boxes (13 × 1.45 over 11 × 1.45).
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 35.0), Sense::hover());
            let tile_rect = Rect::from_min_size(pos2(row.left(), row.center().y - 14.0), Vec2::splat(28.0));
            in_rect(ui, tile_rect, Layout::left_to_right(Align::Center), |ui| tile(ui, icons::FILE_TEXT, true));
            let close = Rect::from_center_size(pos2(row.right() - 11.0, row.center().y), Vec2::splat(22.0));
            if place(ui, close, |ui: &mut Ui| kit::icon_button_sm(ui, icons::X, tr("Clear"))).clicked() {
                clear = true;
            }
            let x = tile_rect.right() + 8.0;
            let w = close.left() - 8.0 - x;
            let ng = text_box(ui, &name, theme::medium(13.0), p.text, w, false);
            let dg = text_box(ui, &dir, theme::mono(11.0), p.text_faint, w, false);
            let top = row.center().y - (ng.size().y + dg.size().y) / 2.0;
            let name_h = ng.size().y;
            ui.painter().galley(pos2(x, top), ng, p.text);
            ui.painter().galley(pos2(x, top + name_h), dg, p.text_faint);
        },
    );
    clear
}

/// A folder for display: `~/…` inside the home folder.
fn home_relative(dir: &std::path::Path) -> String {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home.as_deref().and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    }
}

/// A recessed box (`.dl-codebox`) holding one faint mono line instead of
/// an editor.
fn note_box(ui: &mut Ui, note: &str, height: f32) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(8), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let g = line_galley(ui, note, theme::mono(12.5), rect.width() - 30.0);
    // `.code .src`: padding 8 14 on 20 px lines.
    ui.painter().galley(pos2(rect.left() + 15.0, rect.top() + 9.0 + (20.0 - g.size().y) / 2.0), g, p.text_faint);
}

/// "Ready to import" card: detected format, what will be added, warnings.
fn import_summary(ui: &mut Ui, text: &str, pr: &ternarywire_convert::Imported) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.ok.lerp_to_gamma(p.border, 0.6)))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            // Rows as tall as their text (egui's rows are 30 by default).
            ui.spacing_mut().interact_size.y = 20.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new(icons::CHECK_CIRCLE).font(theme::icon_font(13.0)).color(p.ok));
                ui.label(RichText::new(tr("Ready to import")).font(theme::semibold(13.0)).color(p.text));
                if let Some(fmt) = ternarywire_convert::detect(text) {
                    kit::tone_pill(ui, fmt.label(), kit::Tone::Ok);
                }
            });
            ui.add_space(6.0);
            // The lines under it are 12 px text in 17 px line boxes.
            ui.spacing_mut().interact_size.y = 17.0;
            let reqs: usize = pr.collections.iter().map(|c| c.request_count()).sum();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let stats = [
                    (icons::STACK, pr.collections.len(), "{} collection", "{} collections"),
                    (icons::GLOBE, reqs, "{} request", "{} requests"),
                    (icons::GLOBE_SIMPLE, pr.environments.len(), "{} environment", "{} environments"),
                    (icons::HASH, pr.globals.len(), "{} global variable", "{} global variables"),
                ];
                for (i, (icon, n, one, many)) in stats.into_iter().enumerate() {
                    if i >= 2 && n == 0 {
                        continue;
                    }
                    ui.label(RichText::new(format!("{icon} {}", plural(n, one, many))).size(12.0).color(p.text_muted));
                }
            });
            if !pr.warnings.is_empty() {
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                for w in pr.warnings.iter().take(6) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new(icons::WARNING).font(theme::icon_font(12.0)).color(p.warn));
                        ui.add(egui::Label::new(RichText::new(w).size(12.0).color(p.text_muted)).wrap());
                    });
                }
                if pr.warnings.len() > 6 {
                    ui.label(kit::hint(ui, &trf("{} more warnings", &[&(pr.warnings.len() - 6)])));
                }
            }
        });
}

// ---------------------------------------------------------------------------
// Code snippet
// ---------------------------------------------------------------------------

/// The name of a snippet language's family, shown next to it.
fn language_family(lang: Language) -> &'static str {
    match lang.syntax() {
        "bash" => tr("Shell"),
        "http" => "HTTP",
        "javascript" => "JavaScript",
        "python" => "Python",
        "go" => "Go",
        "rust" => "Rust",
        "java" => "Java",
        "kotlin" => "Kotlin",
        "csharp" => "C#",
        "php" => "PHP",
        "swift" => "Swift",
        "ruby" => "Ruby",
        "dart" => "Dart",
        "powershell" => "PowerShell",
        other => other,
    }
}

/// Read-only code with a line-number gutter (`.code`): mono 12.5 on 20 px
/// lines, wrapped (anywhere) or scrolling sideways. The text can be selected.
fn code_view(ui: &mut Ui, id: &str, code: &str, lang: crate::highlight::Lang, wrap: bool) {
    let p = theme::pal(ui);
    // A final line break starts no numbered line.
    let code = code.strip_suffix('\n').unwrap_or(code);
    let dark = ui.visuals().dark_mode;
    let rect = ui.available_rect_before_wrap();
    let lines = code.split('\n').count().max(1);
    // `.gut div`: 12 px in, the number, 4 px, a 10 px fold column, 8 px.
    let gutter_w = 12.0 + lines.to_string().len().max(1) as f32 * 7.5 + 4.0 + 10.0 + 8.0;
    let gutter = Rect::from_min_max(rect.min, pos2(rect.left() + gutter_w, rect.bottom()));
    ui.painter().rect_filled(gutter, 0.0, p.well);
    ui.painter().vline(gutter.right() - 0.5, gutter.y_range(), Stroke::new(1.0, p.border));
    let key = crate::highlight::HlKey { text: code, lang, dark, search: "", vars: None, font_size: 12.5, wrap };
    let mut job = crate::highlight::cached(ui.ctx(), key);
    for s in &mut job.sections {
        s.format.line_height = Some(20.0);
    }
    job.wrap = if wrap {
        TextWrapping { max_width: rect.width() - gutter_w - 28.0, break_anywhere: true, ..Default::default() }
    } else {
        TextWrapping::no_max_width()
    };
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let content = Rect::from_min_max(pos2(gutter.right(), rect.top()), rect.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    child.set_clip_rect(content.intersect(ui.clip_rect()));
    let out = egui::ScrollArea::new([!wrap, true]).id_salt(id).auto_shrink([false; 2]).show(&mut child, |ui| {
        egui::Frame::new().inner_margin(Margin::symmetric(14, 8)).show(ui, |ui| {
            ui.add(egui::Label::new(galley.clone()).selectable(true));
        });
    });
    let top = content.top() + 8.0 - out.state.clamped_offset().y;
    let painter = ui.painter().with_clip_rect(gutter.intersect(ui.clip_rect()));
    let mut n = 0;
    let mut new_line = true;
    for row in &galley.rows {
        if new_line {
            n += 1;
            let y = top + row.rect().center().y;
            if y > gutter.top() - 20.0 && y < gutter.bottom() + 20.0 {
                painter.text(
                    pos2(gutter.right() - 22.0, y),
                    Align2::RIGHT_CENTER,
                    n.to_string(),
                    theme::mono(12.5),
                    p.text_faint,
                );
            }
        }
        new_line = row.ends_with_newline;
    }
    ui.advance_cursor_after_rect(rect);
}

fn codegen(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    item: &RequestItem,
    collection: Option<&str>,
    lang: &mut Language,
    code: &mut String,
) -> Outcome {
    let mut regenerate = false;
    let copied_id = egui::Id::unique("codegen-copied");
    let wrap_id = egui::Id::unique("codegen-wrap");
    let filter_id = egui::Id::unique("codegen-filter");
    let now = ctx.input(|i| i.time);
    let copied_at: Option<f64> = ctx.data(|d| d.get_temp(copied_id));
    let copied = copied_at.is_some_and(|t| now - t < 1.6);
    if copied {
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
    }
    let mut wrap: bool = ctx.data(|d| d.get_temp(wrap_id).unwrap_or(true));
    let mut filter: String = ctx.data(|d| d.get_temp(filter_id).unwrap_or_default());
    let out = Modal::new("codegen", 920.0)
        .title(tr("Code snippet"))
        .subtitle(item.name.clone())
        .icon(icons::CODE)
        .center()
        .show(ctx, |ui| {
            let current = *lang;
            let height = room(ui).min(460.0);
            split(
                ui,
                height,
                219.0,
                |ui| {
                    egui::Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                        let w = ui.available_width();
                        kit::search_field_sm(ui, "codegen-filter", &mut filter, tr("Filter languages"), w);
                    });
                    let needle = filter.trim().to_lowercase();
                    egui::ScrollArea::vertical().id_salt("codegen-langs").auto_shrink([false; 2]).show(ui, |ui| {
                        egui::Frame::new().inner_margin(Margin { left: 6, right: 6, top: 0, bottom: 6 }).show(
                            ui,
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 1.0;
                                let shown: Vec<&Language> = Language::ALL
                                    .iter()
                                    .filter(|l| l.label().to_lowercase().contains(&needle))
                                    .collect();
                                if shown.is_empty() {
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.add_space(4.0);
                                        ui.label(kit::hint(ui, tr("No languages match the filter.")));
                                    });
                                }
                                for l in shown {
                                    let (name, variant) = l.label().split_once(" - ").unwrap_or((l.label(), ""));
                                    let trail = (!variant.is_empty()).then_some(variant);
                                    if nav_item(ui, None, name, trail, *lang == *l).clicked() && *lang != *l {
                                        *lang = *l;
                                        regenerate = true;
                                    }
                                }
                            },
                        );
                    });
                },
                |ui| {
                    pane_bar(ui, 42.0, 12.0, |ui| {
                        let p = theme::pal(ui);
                        let name = current.label().split_once(" - ").map_or(current.label(), |(n, _)| n);
                        ui.label(RichText::new(name).font(theme::semibold(13.0)).color(p.text));
                        ui.label(RichText::new(language_family(current)).size(12.0).color(p.text_faint));
                        trailing(ui, |ui| {
                            if kit::icon_button_ex(ui, icons::TEXT_ALIGN_LEFT, tr("Wrap lines"), wrap, 22.0).clicked() {
                                wrap = !wrap;
                            }
                        });
                    });
                    let hl = match current.syntax() {
                        "http" => crate::highlight::Lang::Text,
                        "bash" => crate::highlight::Lang::Shell,
                        _ => crate::highlight::Lang::JavaScript,
                    };
                    code_view(ui, &format!("codegen-{}", current.id()), code, hl, wrap);
                },
            );
            footer(ui, |ui| {
                let p = theme::pal(ui);
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(icons::WARNING).font(theme::icon_font(12.0)).color(p.warn));
                let w = (ui.available_width() - 190.0).max(60.0);
                ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(tr(
                                "Variables are resolved with the active environment; secrets appear in clear text.",
                            ))
                            .size(12.0)
                            .color(p.text_muted),
                        )
                        .truncate(),
                    );
                });
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let (icon, label) = if copied { (icons::CHECK, tr("Copied")) } else { (icons::COPY, tr("Copy")) };
                    if kit::primary_icon(ui, icon, label).clicked() {
                        ui.ctx().copy_text(code.clone());
                        ui.ctx().data_mut(|d| d.insert_temp(copied_id, now));
                    }
                    if kit::secondary(ui, None, tr("Close")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    ctx.data_mut(|d| {
        d.insert_temp(wrap_id, wrap);
        d.insert_temp(filter_id, filter);
    });
    if regenerate {
        *code = codegen_item(app, item, collection, *lang);
    }
    out
}

// ---------------------------------------------------------------------------
// Save request
// ---------------------------------------------------------------------------

/// A row of the collection/folder picker (`.dl-tree-pick .tp`): 30 high,
/// 18 px per level, the target tinted with a check.
fn tree_row(ui: &mut Ui, depth: usize, icon: &str, label: &str, selected: bool) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    widgets::a11y_tab(&resp, label, selected);
    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(theme::RADIUS_SM);
        if selected {
            ui.painter().rect_filled(rect, radius, p.accent_soft);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, radius, p.hover);
        }
        let cy = rect.center().y;
        let x = rect.left() + 10.0 + 18.0 * depth as f32;
        let (color, icon_c) = if selected { (p.accent_text, p.accent_text) } else { (p.text, p.text_muted) };
        let ig = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(15.0), icon_c);
        let iw = ig.size().x;
        ui.painter().galley(pos2(x, cy - ig.size().y / 2.0), ig, icon_c);
        let font = if selected { theme::medium(12.5) } else { FontId::proportional(12.5) };
        let g = line_galley(ui, label, font, rect.right() - x - iw - 40.0);
        ui.painter().galley(pos2(x + iw + 8.0, cy - g.size().y / 2.0), g, color);
        if selected {
            ui.painter().text(
                pos2(rect.right() - 10.0, cy),
                Align2::RIGHT_CENTER,
                icons::CHECK,
                theme::icon_font(13.0),
                p.accent_text,
            );
        }
        if resp.has_focus() {
            ui.painter().rect_stroke(rect, radius, Stroke::new(1.5, p.accent), StrokeKind::Inside);
        }
    }
    resp
}

#[allow(clippy::too_many_arguments)]
fn save_as(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    tab: u64,
    collection: &mut Option<Id>,
    folder: &mut Option<Id>,
    name: &mut String,
    named: &mut bool,
    new_collection: &mut String,
) -> Outcome {
    if !*named && let Some(i) = app.tab_index(tab) {
        *name = app.tabs[i].kind.scratch().map(|s| s.name.clone()).unwrap_or_default();
        *named = true;
    }
    let mut save = false;
    let mut create = false;
    // Where it goes, for the footer: "Collection › Folder".
    let target = collection.as_ref().and_then(|c| app.ws.data.collection(c)).map(|c| {
        let mut path = c.name.clone();
        if let Some(f) = folder.as_ref()
            && let Some(chain) = c.ancestors(f)
        {
            for a in chain {
                if let ternarywire_core::model::Ancestor::Folder(x) = a {
                    path.push_str(&format!(" › {}", x.name));
                }
            }
            if let Some(it) = c.find(f) {
                path.push_str(&format!(" › {}", it.name()));
            }
        }
        path
    });
    let valid = collection.is_some() && !name.trim().is_empty();
    let out =
        Modal::new("save-as", 520.0).title(tr("Save request")).icon(icons::FLOPPY_DISK).sticky().show(ctx, |ui| {
            body(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                kit::field(ui, tr("Name"), "", |ui| {
                    let edit = line_edit(ui, name, "", false, false).desired_width(f32::INFINITY);
                    let r = add_input(ui, edit, false);
                    widgets::a11y_name(&r, tr("Name"));
                    if ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && valid {
                        save = true;
                    }
                });
                kit::field(ui, tr("Save to"), "", |ui| {
                    kit::well(ui).inner_margin(Margin::same(4)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt("save-tree")
                            .max_height(212.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                if app.ws.data.collections.is_empty() {
                                    egui::Frame::new().inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
                                        ui.label(
                                            RichText::new(tr("No collections yet. Create one below."))
                                                .size(12.5)
                                                .color(theme::pal(ui).text_muted),
                                        );
                                    });
                                }
                                for c in &app.ws.data.collections {
                                    let selected = collection.as_deref() == Some(&c.id) && folder.is_none();
                                    if tree_row(ui, 0, icons::STACK, &c.name, selected).clicked() {
                                        *collection = Some(c.id.clone());
                                        *folder = None;
                                    }
                                    fn folders(
                                        ui: &mut Ui,
                                        c: &ternarywire_core::model::Collection,
                                        items: &[Item],
                                        depth: usize,
                                        collection: &mut Option<Id>,
                                        folder: &mut Option<Id>,
                                    ) {
                                        for i in items {
                                            if let Item::Folder(f) = i {
                                                let selected = folder.as_deref() == Some(&f.id);
                                                let icon =
                                                    if selected { icons::FOLDER_OPEN } else { icons::FOLDER_SIMPLE };
                                                if tree_row(ui, depth, icon, &f.name, selected).clicked() {
                                                    *collection = Some(c.id.clone());
                                                    *folder = Some(f.id.clone());
                                                }
                                                folders(ui, c, &f.items, depth + 1, collection, folder);
                                            }
                                        }
                                    }
                                    folders(ui, c, &c.items, 1, collection, folder);
                                }
                            });
                    });
                });
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), 26.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        let ok = !new_collection.trim().is_empty();
                        let b = ui.add_enabled_ui(ok, |ui| {
                            kit::button_ex(ui, kit::Kind::Secondary, Some(icons::PLUS), tr("Create collection"), true)
                        });
                        let w = ui.available_width();
                        let hint = tr("New collection name");
                        let r = small_input(ui, new_collection, hint, hint, w, false);
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if (b.inner.clicked() || enter) && ok {
                            create = true;
                        }
                    },
                );
            });
            footer(ui, |ui| {
                if let Some(t) = &target {
                    let p = theme::pal(ui);
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(icons::FOLDER_OPEN).font(theme::icon_font(12.0)).color(p.text_muted));
                    let w = (ui.available_width() - 190.0).max(40.0);
                    ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(t).size(12.0).color(p.text_muted)).truncate());
                    });
                }
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add_enabled_ui(valid, |ui| kit::primary_icon(ui, icons::FLOPPY_DISK, tr("Save")))
                        .inner
                        .clicked()
                    {
                        save = true;
                    }
                    if kit::secondary(ui, None, tr("Cancel")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    if create {
        let id = app.new_collection(new_collection.trim());
        *collection = Some(id);
        *folder = None;
        new_collection.clear();
    }
    let out = if save { Outcome::Close } else { out };
    if save && let Some(c) = collection.clone() {
        if app.save_scratch_to(tab, &c, folder.as_deref(), name.trim()) {
            app.toast(tr("Saved"), false);
            // Saving was asked for while closing the tab.
            if app.close_after_save == Some(tab)
                && let Some(i) = app.tab_index(tab)
            {
                app.close_tab(i);
            }
        } else {
            app.toast(tr("The selected collection or folder no longer exists."), true);
        }
    }
    if matches!(out, Outcome::Close) {
        app.close_after_save = None;
    }
    out
}

// ---------------------------------------------------------------------------
// Alerts: unsaved changes, delete
// ---------------------------------------------------------------------------

/// The top of an alert (no header): a 44 px tinted circle with an icon, a
/// 15 px semibold title and a muted sentence.
fn alert_body(ui: &mut Ui, icon: &str, tone: kit::Tone, title: &str, text: &str) {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    egui::Frame::new().inner_margin(Margin { left: 20, right: 20, top: 22, bottom: 18 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 22.0, bg);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(22.0), fg);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let w = ui.available_width();
                ui.label(text_box(ui, title, theme::semibold(15.0), p.text, w, true));
                ui.label(text_box(ui, text, FontId::proportional(13.0), p.text_muted, w, true));
            });
        });
    });
}

/// What to do with a tab whose unsaved work closing it would lose.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseChoice {
    Save,
    Discard,
}

fn close_unsaved(app: &mut TernaryWireApp, ctx: &egui::Context, tab: u64) -> Outcome {
    let Some(idx) = app.tab_index(tab) else { return Outcome::Close };
    let (name, _, _) = tab_title(&app.tabs[idx], &app.ws);
    let mut choice = None;
    // Enter saves and Escape cancels (the modal closes on Escape).
    let out = Modal::new("close-unsaved", 460.0).center().show(ctx, |ui| {
        alert_body(
            ui,
            icons::WARNING,
            kit::Tone::Warn,
            &trf("Save changes to “{}”?", &[&name]),
            tr("Your changes will be lost if you close the tab without saving them."),
        );
        footer(ui, |ui| {
            let discard = danger_button(ui, false, false, None, tr("Don't save"));
            widgets::a11y_name(&discard, &trf("Don't save \"{}\"", &[&name]));
            if discard.clicked() {
                choice = Some(CloseChoice::Discard);
            }
            trailing(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let o = kit::ButtonOpts { kbd: Some("↩"), ..Default::default() };
                let save =
                    kit::button(ui, kit::Kind::Primary, kit::Size::Md, tr("Save…"), o).on_hover_text(tr("Enter"));
                widgets::a11y_name(&save, &trf("Save \"{}\"…", &[&name]));
                widgets::a11y_shortcut(&save, "Enter");
                let o = kit::ButtonOpts { kbd: Some("esc"), ..Default::default() };
                let cancel =
                    kit::button(ui, kit::Kind::Secondary, kit::Size::Md, tr("Cancel"), o).on_hover_text(tr("Esc"));
                widgets::a11y_name(&cancel, &trf("Cancel closing \"{}\"", &[&name]));
                widgets::a11y_shortcut(&cancel, "Escape");
                // Save… is the default button: Enter presses it unless Tab
                // moved the focus to another button (then Enter/Space presses
                // that one). Its ↩ hint marks it, not a focus ring.
                let focus = ui.memory(|m| m.focused());
                let other = focus.is_some_and(|f| f == discard.id || f == cancel.id);
                if save.clicked() || (!other && ui.input(|i| i.key_pressed(Key::Enter))) {
                    choice = Some(CloseChoice::Save);
                }
                if cancel.clicked() { Outcome::Close } else { Outcome::Keep }
            })
        })
    });
    match choice {
        Some(CloseChoice::Discard) => app.close_tab(idx),
        Some(CloseChoice::Save) => {
            if let TabKind::Spec(t) = &mut app.tabs[idx].kind {
                match t.save() {
                    Ok(()) => app.close_tab(idx),
                    Err(e) => app.toast(trf("Save failed: {}", &[&e]), true),
                }
            } else {
                app.close_after_save = Some(tab);
                app.dialog = Some(Dialog::save_as(tab, &app.ws));
            }
        }
        None => return out,
    }
    Outcome::Close
}

fn confirm(app: &mut TernaryWireApp, ctx: &egui::Context, target: &DeleteTarget, name: &str) -> Outcome {
    let mut yes = false;
    let (title, text) = match target {
        DeleteTarget::Item { item: None, .. } => (
            trf("Delete collection “{}”?", &[&name]),
            tr("The collection, its folders and its requests are removed from the workspace folder."),
        ),
        DeleteTarget::Item { collection, item: Some(id) } => {
            let folder = app
                .ws
                .data
                .collection(collection)
                .and_then(|c| c.find(id))
                .is_some_and(|i| matches!(i, Item::Folder(_)));
            if folder {
                (
                    trf("Delete folder “{}”?", &[&name]),
                    tr("The folder and the requests in it are removed from the workspace folder."),
                )
            } else {
                (trf("Delete request “{}”?", &[&name]), tr("The request is removed from the workspace folder."))
            }
        }
        DeleteTarget::Environment(_) => (
            trf("Delete environment “{}”?", &[&name]),
            tr("The environment and its variables are removed from the workspace folder."),
        ),
    };
    let out = Modal::new("confirm", 460.0).center().show(ctx, |ui| {
        alert_body(ui, icons::TRASH, kit::Tone::Err, &title, text);
        footer(ui, |ui| {
            trailing(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if danger_button(ui, true, false, Some(icons::TRASH), tr("Delete")).clicked() {
                    yes = true;
                }
                let cancel = kit::secondary(ui, None, tr("Cancel"));
                if cancel.clicked() || yes { Outcome::Close } else { Outcome::Keep }
            })
        })
    });
    if yes {
        match target {
            DeleteTarget::Item { collection, item: None } => app.ws.delete_collection(collection),
            DeleteTarget::Item { collection, item: Some(id) } => {
                if let Some(c) = app.ws.data.collection_mut(collection) {
                    c.remove(id);
                }
                let cid = collection.clone();
                app.ws.mark_collection(&cid);
            }
            DeleteTarget::Environment(id) => app.ws.delete_environment(id),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// New workspace, prompt
// ---------------------------------------------------------------------------

fn new_workspace(
    app: &mut TernaryWireApp,
    ctx: &egui::Context,
    name: &mut String,
    path: &mut Option<PathBuf>,
) -> Outcome {
    let mut create = false;
    let existing = path.as_ref().is_some_and(|p| ternarywire_core::storage::WorkspaceStore::is_workspace(p));
    let out = Modal::new("new-ws", 520.0)
        .title(tr("New workspace"))
        .subtitle(tr("A folder of plain files you can keep in Git."))
        .icon(icons::FOLDER_PLUS)
        .sticky()
        .show(ctx, |ui| {
            body(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                kit::field(ui, tr("Name"), "", |ui| {
                    let r = add_input(ui, line_edit(ui, name, "", false, false).desired_width(f32::INFINITY), false);
                    widgets::a11y_name(&r, tr("Name"));
                    if ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                });
                kit::field(ui, tr("Location"), "", |ui| {
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), 30.0),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            let choose = kit::button_ex(
                                ui,
                                kit::Kind::Secondary,
                                Some(icons::FOLDER_OPEN),
                                tr("Choose folder…"),
                                true,
                            );
                            if choose.clicked()
                                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                            {
                                *path = Some(dir);
                            }
                            let w = ui.available_width();
                            let shown = path.as_ref().map(|p| p.display().to_string());
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                path_box(ui, shown.as_deref(), tr("No folder selected"), w);
                            });
                        },
                    );
                });
                if existing {
                    banner(ui, Tone::Info, tr("This folder already holds a workspace; it will be opened as it is."));
                } else {
                    let hint =
                        tr("A workspace is a folder of plain JSON files; put it in a Git repository to share it.");
                    ui.add(egui::Label::new(kit::hint(ui, hint).line_height(Some(12.0 * 1.45))).wrap());
                }
            });
            footer(ui, |ui| {
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let label = if existing { tr("Open") } else { tr("Create") };
                    if ui.add_enabled_ui(path.is_some(), |ui| kit::primary(ui, label)).inner.clicked() {
                        create = true;
                    }
                    if kit::secondary(ui, None, tr("Cancel")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    if create && let Some(p) = path.clone() {
        if existing {
            app.open_workspace(p, None);
        } else {
            app.open_workspace(p, Some(name.clone()));
        }
        return Outcome::Close;
    }
    out
}

fn prompt(app: &mut TernaryWireApp, ctx: &egui::Context, kind: &Prompt, text: &mut String) -> Outcome {
    let (title, icon, action) = match kind {
        Prompt::NewCollection => (tr("New Collection"), icons::STACK, tr("Create")),
        Prompt::NewEnvironment => (tr("New Environment"), icons::GLOBE_SIMPLE, tr("Create")),
        Prompt::RenameEnvironment(_) => (tr("Rename environment"), icons::PENCIL_SIMPLE, tr("Rename")),
        Prompt::Fork(_) => (tr("Fork collection"), icons::GIT_FORK, tr("Fork")),
    };
    let mut ok = false;
    let modal = Modal::new("prompt", 440.0).title(title).icon(icon).sticky().center();
    let valid = !text.trim().is_empty();
    let out = modal.show(ctx, |ui| {
        body(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            kit::field(ui, tr("Name"), "", |ui| {
                let id = egui::Id::unique("prompt-text");
                let first = !ui.memory(|m| m.has_focus(id));
                let edit = line_edit(ui, text, "", false, false).id(id).desired_width(f32::INFINITY);
                let r = add_input(ui, edit, false);
                widgets::a11y_name(&r, tr("Name"));
                r.request_focus();
                if first && let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
                    // Start with the whole name selected, ready to be typed over.
                    let all = egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(text.chars().count()),
                    );
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), id);
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && valid {
                    ok = true;
                }
            });
            if let Prompt::Fork(_) = kind {
                ui.label(kit::hint(ui, tr("A copy that can pull changes from the original and merge back.")));
            }
        });
        footer(ui, |ui| {
            trailing(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add_enabled_ui(valid, |ui| kit::primary(ui, action)).inner.clicked() {
                    ok = true;
                }
                if kit::secondary(ui, None, tr("Cancel")).clicked() || ok { Outcome::Close } else { Outcome::Keep }
            })
        })
    });
    if ok && !text.trim().is_empty() {
        let name = text.trim().to_string();
        match kind {
            Prompt::NewCollection => {
                app.new_collection(&name);
                app.sidebar_tab = crate::app::SidebarTab::Collections;
            }
            Prompt::NewEnvironment => app.new_environment(&name),
            Prompt::Fork(id) => crate::fork_ui::create(app, id, &name),
            Prompt::RenameEnvironment(id) => {
                if let Some(e) = app.ws.data.environments.iter_mut().find(|e| &e.id == id) {
                    e.name = name;
                }
                let id = id.clone();
                app.ws.mark_env(&id);
            }
        }
        return Outcome::Close;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ternarywire_core::model::HttpRequest;

    #[test]
    fn palette_ranks_word_starts_alike_in_list_order() {
        let e = |l: &str| PaletteEntry::new(Group::Requests, icons::GLOBE, l, PaletteAction::NewRequest);
        let entries = [e("Notebook"), e("List books"), e("Get book"), e("Bookmarks")];
        let labels: Vec<&str> = rank(&entries, "book").iter().map(|i| entries[*i].label.as_str()).collect();
        assert_eq!(labels, ["Bookmarks", "List books", "Get book", "Notebook"]);
    }

    fn scratch(app: &mut TernaryWireApp, name: &str) {
        let item = RequestItem::new(name, HttpRequest::new("GET", format!("http://{name}.test")));
        app.add_tab(TabKind::Request(Box::new(crate::request_tab::RequestTab::scratch(item))));
    }

    fn active_title(app: &TernaryWireApp) -> String {
        tab_title(&app.tabs[app.active], &app.ws).0
    }

    /// Tabs that are not written to the session (webhook listener) and saved
    /// requests deleted meanwhile must not shift the restored active tab.
    #[test]
    fn session_restores_the_active_tab() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        app.add_tab(TabKind::Webhook(Box::default()));
        scratch(&mut app, "a");
        scratch(&mut app, "b");
        app.active = 1;
        save_session(&mut app);
        app.tabs.clear();
        restore_session(&mut app);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(active_title(&app), "a");

        // A saved request before the active tab is deleted before restoring.
        let cid = app.new_collection("C");
        let gone = RequestItem::new("gone", HttpRequest::new("GET", "http://gone.test"));
        let gone_id = gone.id.clone();
        app.ws.data.collection_mut(&cid).unwrap().items.push(Item::Request(gone));
        app.tabs.clear();
        app.active = 0;
        app.open_item(ItemRef { collection: cid.clone(), item: gone_id.clone() });
        scratch(&mut app, "c");
        scratch(&mut app, "d");
        app.active = 1;
        save_session(&mut app);
        app.ws.data.collection_mut(&cid).unwrap().remove(&gone_id);
        app.tabs.clear();
        restore_session(&mut app);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(active_title(&app), "c");
    }

    /// An unsaved request edited before a restart still counts as edited
    /// after it (closing it asks); an untouched one still just closes.
    #[test]
    fn restored_tabs_remember_unsaved_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        scratch(&mut app, "edited");
        scratch(&mut app, "untouched");
        if let TabKind::Request(t) = &mut app.tabs[0].kind
            && let Some(item) = t.scratch.as_mut()
            && let ternarywire_core::model::RequestKind::Http(h) = &mut item.request
        {
            h.set_url("http://edited.test/typed".into());
        }
        for _ in 0..2 {
            save_session(&mut app);
            app.tabs.clear();
            restore_session(&mut app);
            assert_eq!(app.tabs.len(), 2);
            assert!(app.tab_has_unsaved_work(0));
            assert!(!app.tab_has_unsaved_work(1));
        }
    }

    /// Credentials typed into an unsaved request (an Authorization header, an
    /// auth parameter, a secret variable's value) stay out of session.json and
    /// come back from the secret store after a restart.
    #[test]
    fn session_keeps_credentials_out_of_the_file() {
        use ternarywire_core::model::KeyValue;
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let mut env = ternarywire_core::environment::Environment::new("Dev");
        env.variables.push(ternarywire_core::variables::Variable::secret("token", "s3cr3t-token"));
        let env = app.ws.add_environment(env);
        app.ws.set_active_environment(Some(env));
        let mut req = HttpRequest::new("GET", "https://api.test/x?key=s3cr3t-token");
        req.headers.push(KeyValue::new("Authorization", "Bearer typed-in-1234"));
        req.headers.push(KeyValue::new("X-Api-Key", "{{apiKey}}"));
        req.headers.push(KeyValue::new("Accept", "application/json"));
        req.auth = ternarywire_core::auth::Auth::bearer("auth-param-5678");
        let item = RequestItem::new("creds", req.clone());
        app.add_tab(TabKind::Request(Box::new(crate::request_tab::RequestTab::scratch(item))));
        save_session(&mut app);
        let file = std::fs::read_to_string(session_path(&app)).unwrap();
        for secret in ["s3cr3t-token", "typed-in-1234", "auth-param-5678"] {
            assert!(!file.contains(secret), "{secret} in {file}");
        }
        assert!(file.contains("{{apiKey}}") && file.contains("application/json"), "{file}");
        app.tabs.clear();
        restore_session(&mut app);
        assert_eq!(app.tabs[0].kind.scratch().and_then(|i| i.http()), Some(&req));
        assert!(!app.tab_has_unsaved_work(0));
    }

    /// External secrets that cannot be set up (the CA file is gone) are
    /// reported once when the Vault opens, not on every frame.
    #[test]
    fn vault_reports_a_provider_setup_error_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        app.settings.certificates.ca_enabled = true;
        app.settings.certificates.ca_path = dir.path().join("missing.pem").display().to_string();
        app.dialog = Some(Dialog::vault());
        for _ in 0..3 {
            app.draw_frame(Vec::new());
        }
        let toasts: Vec<&str> = app.toasts.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(toasts.len(), 1, "{toasts:?}");
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    /// The Save dialog takes the tab's name once; emptying the field to type
    /// another name used to fill it in again on the next frame.
    #[test]
    fn save_as_fills_in_the_name_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        scratch(&mut app, "first");
        app.dialog = Some(Dialog::save_as(app.tabs[0].id, &app.ws));
        let name = |app: &mut TernaryWireApp| match &mut app.dialog {
            Some(Dialog::SaveAs { name, .. }) => std::mem::take(name),
            _ => panic!("no Save dialog"),
        };
        app.draw_frame(Vec::new());
        assert_eq!(name(&mut app), "first");
        app.draw_frame(Vec::new());
        app.draw_frame(Vec::new());
        assert_eq!(name(&mut app), "");
    }

    /// The prompt for closing unsaved work: its buttons are named for screen
    /// readers, Escape keeps the tab open, Enter saves.
    #[test]
    fn close_unsaved_prompt_keys_and_names() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        scratch(&mut app, "edited");
        let id = app.tabs[0].id;
        app.dialog = Some(Dialog::CloseUnsaved { tab: id });
        // The first frame of a new window only measures it.
        app.draw_frame(Vec::new());
        let labels = app.draw_frame(Vec::new()).labels;
        for name in ["Save \"edited\"…", "Don't save \"edited\"", "Cancel closing \"edited\""] {
            assert!(labels.iter().any(|l| l == name), "no {name:?} in {labels:?}");
        }
        app.draw_frame(vec![key(egui::Key::Escape)]);
        assert!(app.dialog.is_none());
        assert_eq!(app.tabs.len(), 1);

        // A request that was never saved asks where to save it, then closes.
        app.dialog = Some(Dialog::CloseUnsaved { tab: id });
        app.draw_frame(Vec::new());
        app.draw_frame(vec![key(egui::Key::Enter)]);
        assert!(matches!(app.dialog, Some(Dialog::SaveAs { tab, .. }) if tab == id));
        assert_eq!(app.close_after_save, Some(id));
        assert_eq!(app.tabs.len(), 1);

        // A spec is written and closed right away.
        app.dialog = None;
        let path = dir.path().join("api.yaml");
        std::fs::write(&path, crate::spec_tab::OPENAPI_TEMPLATE).unwrap();
        crate::spec_tab::open_spec(&mut app, path.clone());
        let spec = app.tabs[app.active].id;
        app.dialog = Some(Dialog::CloseUnsaved { tab: spec });
        if let TabKind::Spec(t) = &mut app.tabs[app.active].kind {
            t.restore_unsaved(format!("{}# edited\n", crate::spec_tab::OPENAPI_TEMPLATE));
        }
        app.draw_frame(vec![key(egui::Key::Enter)]);
        assert!(app.tab_index(spec).is_none());
        assert!(std::fs::read_to_string(&path).unwrap().ends_with("# edited\n"));
    }
}
