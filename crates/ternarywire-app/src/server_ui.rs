//! The "server" pattern shared by the mock server, the webhook listener and
//! the capture proxy (`design/redesign/screens-tools.js`): a 46 px title row,
//! a 56 px address bar (state, address, Copy, the server's actions), a
//! collapsible settings strip, then a list (left) and the detail of the
//! selected row (right).

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Galley, Id, Layout, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, TextFormat, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};

use crate::i18n::tr;
use crate::theme::{Palette, Tone};
use crate::{icons, kit, theme, widgets};

/// State of a local server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Busy,
    Live,
    Failed,
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/// A full-width row of `height` with `pad` (left, right): `left` laid out
/// left to right, `right` right to left from the right edge (add the
/// rightmost first), a hairline along its bottom. `left` gets the width the
/// right part leaves.
pub fn row(
    ui: &mut Ui,
    height: f32,
    pad: (f32, f32),
    gap: f32,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) -> Rect {
    row_ex(ui, height, pad, gap, true, left, right)
}

/// [`row`] with or without the hairline.
pub fn row_ex(
    ui: &mut Ui,
    height: f32,
    pad: (f32, f32),
    gap: f32,
    hairline: bool,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) -> Rect {
    let p = theme::pal(ui);
    let full = ui.available_rect_before_wrap();
    let rect = Rect::from_min_size(full.min, vec2(full.width(), height));
    // The hairline is the row's bottom border: content centers above it.
    let bottom = if hairline { rect.bottom() - 1.0 } else { rect.bottom() };
    let inner = Rect::from_min_max(pos2(rect.left() + pad.0, rect.top()), pos2(rect.right() - pad.1, bottom));
    // Drawn in children of the row's rect: they leave the cursor alone.
    let mut r = ui
        .new_child(UiBuilder::new().id_salt("row-right").max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    r.spacing_mut().item_spacing.x = gap;
    right(&mut r);
    let used = r.min_rect();
    let left_end = if used.width() > 0.5 { used.left() - gap } else { inner.right() };
    let left_rect = Rect::from_min_max(inner.min, pos2(left_end.max(inner.left()), inner.bottom()));
    let mut l = ui.new_child(
        UiBuilder::new().id_salt("row-left").max_rect(left_rect).layout(Layout::left_to_right(Align::Center)),
    );
    l.spacing_mut().item_spacing.x = gap;
    left(&mut l);
    if hairline {
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    }
    ui.advance_cursor_after_rect(rect);
    rect
}

/// An error under the address bar (why the server could not start), with
/// actions on its right.
pub fn error_row(ui: &mut Ui, text: &str, actions: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    let r = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::banner_with(ui, Tone::Err, icons::WARNING_CIRCLE, text, actions);
        })
        .response
        .rect;
    bottom_border(ui, r, p.border);
}

/// A 1 px border under `r` (outside it, like a CSS `border-bottom`).
fn bottom_border(ui: &mut Ui, r: Rect, color: Color32) {
    ui.painter().hline(r.x_range(), r.bottom() + 0.5, Stroke::new(1.0, color));
    ui.add_space(1.0);
}

/// The 46 px title row: icon tile, name, what the tool does (elided) and a
/// "More actions" menu.
pub fn tool_title(ui: &mut Ui, icon: &str, title: &str, subtitle: &str, menu: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    row(
        ui,
        kit::size::VIEW_TITLE,
        (16.0, 12.0),
        10.0,
        |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(7), p.accent_soft);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(15.0), p.accent_text);
            ui.add(egui::Label::new(RichText::new(title).font(theme::semibold(14.0)).color(p.text)).selectable(false));
            ui.add(
                egui::Label::new(RichText::new(subtitle).size(12.5).color(p.text_muted)).truncate().selectable(false),
            );
        },
        |ui| {
            let r = kit::icon_button_sm(ui, icons::DOTS_THREE, tr("More actions"));
            egui::Popup::menu(&r).width(220.0).show(menu);
        },
    );
}

/// The pill of a server's state, 24 high: a live dot, a ring (stopped), a
/// spinner (starting) or a red dot (failed).
pub fn server_pill(ui: &mut Ui, state: State, text: &str) -> Response {
    let p = theme::pal(ui);
    let (fg, bg) = match state {
        State::Idle => (p.text_muted, p.subtle),
        State::Busy => (p.info, p.info_soft),
        State::Live => (p.ok, p.ok_soft),
        State::Failed => (p.err, p.err_soft),
    };
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::medium(12.0), fg);
    let lead = match state {
        State::Idle => 8.0,
        State::Busy => 13.0,
        State::Live | State::Failed => 6.0,
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(9.0 + lead + 5.0 + g.size().x + 9.0, 24.0), Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(12), bg);
        let c = pos2(rect.left() + 9.0 + lead / 2.0, rect.center().y);
        match state {
            State::Idle => {
                painter.circle_stroke(c, 3.25, Stroke::new(1.5, fg));
            }
            State::Live => {
                painter.circle_filled(c, 6.0, theme::soft(fg, 0.25));
                painter.circle_filled(c, 3.0, fg);
            }
            State::Failed => {
                painter.circle_filled(c, 3.0, fg);
            }
            State::Busy => spinner_icon(ui, c, 13.0, fg),
        }
        ui.painter().galley(pos2(rect.left() + 9.0 + lead + 5.0, rect.center().y - g.size().y / 2.0), g, fg);
    }
    resp
}

/// A spinning `CIRCLE_NOTCH` centered on `c`.
pub fn spinner_icon(ui: &Ui, c: egui::Pos2, size: f32, color: Color32) {
    let t = ui.input(|i| i.time) as f32;
    let angle = t * std::f32::consts::TAU;
    let g = ui.painter().layout_no_wrap(icons::CIRCLE_NOTCH.to_string(), theme::icon_font(size), color);
    let rot = egui::emath::Rot2::from_angle(angle);
    let mut shape = egui::epaint::TextShape::new(pos2(0.0, 0.0), g.clone(), color);
    shape.pos = c - rot * (g.size() / 2.0);
    shape.angle = angle;
    ui.painter().add(shape);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

/// What the address bar shows.
pub struct Bar<'a> {
    pub state: State,
    pub label: &'a str,
    /// The address while running.
    pub addr: Option<&'a str>,
    /// Shown instead of the address while stopped.
    pub placeholder: &'a str,
    /// Widest the address field grows.
    pub max_w: f32,
}

/// The 56 px address bar: a URL-bar-like field with the state pill, the
/// address and Copy, then `mid` (session controls) and on the right `right`
/// (added right to left). Returns true when the address was copied.
pub fn server_bar(ui: &mut Ui, id: Id, b: Bar<'_>, mid: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) -> bool {
    let p = theme::pal(ui);
    let mut copied = false;
    row(
        ui,
        kit::size::ACTION_ROW,
        (16.0, 12.0),
        8.0,
        |ui| {
            // `mid` is measured on the previous frame: the field shrinks first.
            let mid_id = id.with("mid-w");
            let mid_w: f32 = ui.data(|d| d.get_temp(mid_id).unwrap_or(0.0));
            let w = (ui.available_width() - if mid_w > 0.0 { mid_w + 8.0 } else { 0.0 }).min(b.max_w).max(160.0);
            let (rect, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
            ui.painter().rect(
                rect,
                CornerRadius::same(9),
                p.surface,
                Stroke::new(1.0, p.border_strong),
                StrokeKind::Inside,
            );
            // Inside the 1 px border: the state (padding 0 10 0 8, a hairline
            // on its right), the address (padding 0 12), the tail (0 4).
            let mut seg = ui.new_child(
                UiBuilder::new().max_rect(rect.shrink2(vec2(9.0, 0.0))).layout(Layout::left_to_right(Align::Center)),
            );
            let pill = server_pill(&mut seg, b.state, b.label);
            let seg_right = pill.rect.right() + 10.0;
            ui.painter().vline(seg_right + 0.5, rect.y_range().shrink(1.0), Stroke::new(1.0, p.border));
            let tail_w = if b.addr.is_some() { 31.0 } else { 9.0 };
            let text_rect = Rect::from_min_max(
                pos2(seg_right + 1.0 + 12.0, rect.top()),
                pos2(rect.right() - tail_w - 12.0, rect.bottom()),
            );
            let (text, font, color) = match b.addr {
                Some(a) => (a, theme::mono(12.5), p.text),
                None => (b.placeholder, FontId::proportional(12.5), p.text_faint),
            };
            let g = kit::elide(ui, text, font, color, text_rect.width());
            ui.painter().galley(pos2(text_rect.left(), text_rect.center().y - g.size().y / 2.0), g, color);
            let r = ui.interact(text_rect, id.with("addr"), Sense::hover());
            r.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
            if let Some(a) = b.addr {
                let r = r.on_hover_text(a);
                let _ = r;
                let tail = Rect::from_min_size(pos2(rect.right() - 27.0, rect.center().y - 11.0), vec2(22.0, 22.0));
                let mut t = ui.new_child(UiBuilder::new().max_rect(tail).layout(Layout::left_to_right(Align::Center)));
                if kit::icon_button_sm(&mut t, icons::COPY, tr("Copy address")).clicked() {
                    ui.ctx().copy_text(a.to_string());
                    copied = true;
                }
            }
            let used = ui.scope(|ui| mid(ui)).response.rect.width();
            if (used - mid_w).abs() > 0.5 {
                ui.data_mut(|d| d.insert_temp(mid_id, used));
                ui.ctx().request_repaint();
            }
        },
        right,
    );
    copied
}

/// A small outlined button whose label mixes Inter and monospace parts
/// (`Use as {{baseUrl}}`): parts in `mono` are drawn in JetBrains Mono 11.5.
pub fn rich_button(ui: &mut Ui, icon: &str, text: &str, mono: &str) -> Response {
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    let normal = TextFormat::simple(theme::medium(12.0), p.text);
    let code = TextFormat::simple(theme::mono(11.5), p.text);
    match text.find(mono).filter(|_| !mono.is_empty()) {
        Some(i) => {
            job.append(&text[..i], 0.0, normal.clone());
            job.append(mono, 0.0, code);
            job.append(&text[i + mono.len()..], 0.0, normal);
        }
        None => job.append(text, 0.0, normal),
    }
    let g = ui.painter().layout_job(job);
    let ig = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(13.0), p.text_muted);
    let w = 8.0 + ig.size().x + 5.0 + g.size().x + 8.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, text));
    if ui.is_rect_visible(rect) {
        let fill = if resp.is_pointer_button_down_on() {
            p.subtle
        } else if resp.hovered() {
            p.hover
        } else {
            p.surface
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            fill,
            Stroke::new(1.0, p.border_strong),
            StrokeKind::Inside,
        );
        let mut x = rect.left() + 8.0;
        painter.galley(pos2(x, rect.center().y - ig.size().y / 2.0), ig.clone(), p.text_muted);
        x += ig.size().x + 5.0;
        painter.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.text);
        if resp.has_focus() {
            painter.rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(8),
                Stroke::new(1.5, p.accent),
                StrokeKind::Outside,
            );
        }
    }
    resp
}

// ---------------------------------------------------------------------------
// Settings strip
// ---------------------------------------------------------------------------

/// One part of a strip summary; `true` draws it in monospace.
pub type Part = (String, bool);

/// The 40 px settings strip: icon, label, a summary of the values, an
/// optional lock hint and a caret. Clicking it opens or closes the settings
/// (`open`).
pub fn strip(ui: &mut Ui, id: Id, open: &mut bool, label: &str, summary: &[Part], lock: Option<&str>, tip: &str) {
    let p = theme::pal(ui);
    let full = ui.available_rect_before_wrap();
    let rect = Rect::from_min_size(full.min, vec2(full.width(), size::STRIP));
    let resp = ui.interact(rect, id, Sense::click());
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let expanded = *open;
    ui.ctx().accesskit_node_builder(resp.id, |n| n.set_expanded(expanded));
    if resp.clicked() {
        *open = !*open;
    }
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, p.well);
    }
    // Centered above the bottom border.
    let cy = rect.center().y - 0.5;
    let mut x = rect.left() + 16.0;
    // The icon takes the strip's 12.5 px text size.
    let ig = painter.layout_no_wrap(icons::SLIDERS_HORIZONTAL.into(), theme::icon_font(12.5), p.text_muted);
    painter.galley(pos2(x, cy - ig.size().y / 2.0), ig.clone(), p.text_muted);
    x += ig.size().x + 10.0;
    let lg = painter.layout_no_wrap(label.to_string(), theme::medium(12.5), p.text);
    painter.galley(pos2(x, cy - lg.size().y / 2.0), lg.clone(), p.text);
    x += lg.size().x + 10.0;
    // Right side: caret, then the lock hint.
    let mut right = rect.right() - 14.0;
    let caret = if *open { icons::CARET_UP } else { icons::CARET_DOWN };
    let cg = painter.layout_no_wrap(caret.into(), theme::icon_font(12.5), p.text_faint);
    painter.galley(pos2(right - cg.size().x, cy - cg.size().y / 2.0), cg.clone(), p.text_faint);
    right -= cg.size().x + 10.0;
    if let Some(lock) = lock {
        let tg = painter.layout_no_wrap(lock.to_string(), FontId::proportional(12.0), p.text_muted);
        painter.galley(pos2(right - tg.size().x, cy - tg.size().y / 2.0), tg.clone(), p.text_muted);
        right -= tg.size().x + 5.0;
        let lg = painter.layout_no_wrap(icons::LOCK_SIMPLE.into(), theme::icon_font(12.0), p.text_muted);
        painter.galley(pos2(right - lg.size().x, cy - lg.size().y / 2.0), lg.clone(), p.text_muted);
        right -= lg.size().x + 10.0;
    }
    let mut job = LayoutJob::default();
    for (text, mono) in summary {
        let font = if *mono { theme::mono(11.5) } else { FontId::proportional(12.5) };
        job.append(
            text,
            0.0,
            TextFormat { font_id: font, color: p.text_muted, valign: Align::Center, ..Default::default() },
        );
    }
    job.wrap = TextWrapping::truncate_at_width((right - x).max(10.0));
    let sg = ui.painter().layout_job(job);
    ui.painter().galley(pos2(x, cy - sg.size().y / 2.0), sg, p.text_muted);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    ui.advance_cursor_after_rect(rect);
}

/// The open settings under a strip: a `well` panel, padding 12 16 14.
pub fn settings_body(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    let r = egui::Frame::new()
        .fill(p.well)
        .inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 14 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        })
        .response
        .rect;
    bottom_border(ui, r, p.border);
}

/// One settings group of [`settings_grid`].
pub type Group<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// Settings groups in columns at least 250 wide (24 apart; rows 12 apart).
pub fn settings_grid(ui: &mut Ui, groups: Vec<Group<'_>>) {
    let w = ui.available_width();
    let n = (((w + 24.0) / 274.0).floor() as usize).clamp(1, groups.len().max(1));
    let col_w = (w - 24.0 * (n as f32 - 1.0)) / n as f32;
    let mut groups = groups.into_iter().peekable();
    let mut first = true;
    while groups.peek().is_some() {
        if !first {
            ui.add_space(12.0);
        }
        first = false;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 24.0;
            for g in groups.by_ref().take(n) {
                ui.allocate_ui_with_layout(vec2(col_w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(col_w);
                    g(ui);
                });
            }
        });
    }
}

/// A settings group: an eyebrow (with a lock while `locked`), then its
/// fields, faded and read-only while locked, then an optional hint.
pub fn group(ui: &mut Ui, title: &str, locked: bool, hint: Option<&str>, add: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        hrow(ui, 15.0, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add(egui::Label::new(kit::eyebrow(ui, title)).selectable(false));
            if locked {
                ui.add(
                    egui::Label::new(
                        RichText::new(icons::LOCK_SIMPLE).font(theme::icon_font(10.5)).color(p.text_faint),
                    )
                    .selectable(false),
                );
            }
        });
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if locked {
                let o = ui.opacity();
                ui.disable();
                ui.set_opacity(o * LOCKED_OPACITY);
            }
            add(ui);
        });
        if let Some(h) = hint {
            hrow(ui, 18.0, |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(RichText::new(icons::LOCK_SIMPLE).font(theme::icon_font(12.0)).color(p.text_muted));
                ui.label(kit::hint(ui, h));
            });
        }
    });
}

/// How opaque the fields of a locked [`group`] are.
const LOCKED_OPACITY: f32 = 0.55;

/// Content of a locked [`group`] drawn at full opacity but still read-only
/// (the design fades only the labelled fields, not a segmented control).
pub fn undimmed<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        if !ui.is_enabled() {
            ui.set_opacity((ui.opacity() / LOCKED_OPACITY).min(1.0));
        }
        add(ui)
    })
    .inner
}

/// A left-to-right row at least `height` high, its content centered
/// vertically and as wide as it is (`ui.horizontal` rows are at least the
/// 30 px interact height).
pub fn hrow<R>(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), add).inner
}

/// A segmented-control track across the whole width (the design's control
/// stretches in a column); the control drawn after it covers its left end.
pub fn seg_track(ui: &Ui, height: f32) {
    let p = theme::pal(ui);
    let r = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
    ui.painter().rect_filled(r, CornerRadius::same(8), p.subtle);
}

/// A labelled control (`Port [3000]`): label 12 muted, 8 apart, 26 high.
pub fn field<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    hrow(ui, 26.0, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if !label.is_empty() {
            ui.add(egui::Label::new(RichText::new(label).size(12.0).color(p.text_muted)).selectable(false));
        }
        add(ui)
    })
}

/// Width of a [`field`] label.
pub fn label_w(ui: &Ui, label: &str) -> f32 {
    if label.is_empty() {
        return 0.0;
    }
    ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.0), Color32::PLACEHOLDER).size().x + 8.0
}

/// Fields side by side 16 apart when `need` fits, else one under the other
/// (16 apart, like a wrapping flex row).
pub fn fields(ui: &mut Ui, need: f32, add: impl FnOnce(&mut Ui)) {
    if need <= ui.available_width() + 0.5 {
        hrow(ui, 26.0, |ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            add(ui);
        });
    } else {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 16.0;
            add(ui);
        });
    }
}

/// A small text input (26 high, radius 6, 12 px text, 8 px padding).
pub fn input(ui: &mut Ui, id: Id, text: &mut String, hint: &str, width: f32, mono: bool) -> Response {
    input_ex(ui, id, text, hint, width, mono, Align::LEFT)
}

fn input_ex(ui: &mut Ui, id: Id, text: &mut String, hint: &str, width: f32, mono: bool, align: Align) -> Response {
    let p = theme::pal(ui);
    let font = if mono { theme::mono(12.0) } else { FontId::proportional(12.0) };
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(RichText::new(hint).color(p.text_faint).font(font.clone()))
        .font(font)
        .desired_width(width)
        .min_size(vec2(0.0, 26.0))
        .align(Align2([align, Align::Center]))
        .margin(egui::Margin::symmetric(8, 0));
    let r = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size.y = 26.0;
            // egui paints the hint in the weak text color: faint, like a placeholder.
            ui.visuals_mut().weak_text_color = Some(p.text_faint);
            let radius = CornerRadius::same(theme::RADIUS_SM);
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.corner_radius = radius;
            w.hovered.corner_radius = radius;
            w.active.corner_radius = radius;
            w.noninteractive.corner_radius = radius;
            ui.add(edit)
        })
        .inner;
    if r.has_focus() {
        kit::focus_ring(ui, r.rect, theme::RADIUS_SM);
    }
    r
}

/// A number input (right-aligned) for `value`, kept in `range`. The text
/// being typed is kept while the field has focus.
pub fn num_input(
    ui: &mut Ui,
    id: Id,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    width: f32,
    name: &str,
) -> Response {
    let key = id.with("typed");
    let typed: Option<String> = ui.data(|d| d.get_temp(key));
    let mut text = typed.unwrap_or_else(|| value.to_string());
    let r = input_ex(ui, id, &mut text, "", width, false, Align::RIGHT);
    if r.changed()
        && let Ok(v) = text.trim().parse::<u64>()
    {
        *value = v.clamp(*range.start(), *range.end());
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(key, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(key));
    }
    widgets::a11y_name(&r, name);
    r
}

/// A checkbox and its label; the label moves under the box when the line
/// is too narrow (like the wrapping row of the design).
pub fn check_line(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = theme::pal(ui);
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), p.text);
    let wide = 15.0 + 8.0 + g.size().x <= ui.available_width();
    // Wrapped: the box, 8 apart, then the label on a 13 × 1.45 line.
    let line_h = 13.0 * 1.45;
    let size = if wide { vec2(15.0 + 8.0 + g.size().x, 26.0) } else { vec2(ui.available_width(), 15.0 + 8.0 + line_h) };
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let value = *on;
    let enabled = ui.is_enabled();
    resp.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, value, label));
    let b = if wide {
        Rect::from_min_size(pos2(rect.left(), rect.center().y - 7.5), vec2(15.0, 15.0))
    } else {
        Rect::from_min_size(rect.min, vec2(15.0, 15.0))
    };
    kit::paint_check(ui, b, *on, false, resp.hovered(), true);
    let pos = if wide {
        pos2(b.right() + 8.0, b.center().y - g.size().y / 2.0)
    } else {
        pos2(rect.left(), rect.top() + 15.0 + 8.0 + (line_h - g.size().y) / 2.0)
    };
    ui.painter().galley(pos, g, p.text);
    if resp.has_focus() {
        ui.painter().rect_stroke(b.expand(2.0), CornerRadius::same(6), Stroke::new(1.5, p.accent), StrokeKind::Outside);
    }
    resp
}

/// A label on the left and a switch (30 × 18) on the right of the line.
pub fn switch_line(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let value = *on;
    let enabled = ui.is_enabled();
    resp.widget_info(|| WidgetInfo::selected(Role::Switch, enabled, value, label));
    let sw = Rect::from_min_size(pos2(rect.right() - 30.0, rect.center().y - 9.0), vec2(30.0, 18.0));
    let g = kit::elide(ui, label, FontId::proportional(13.0), p.text, sw.left() - rect.left() - 8.0);
    ui.painter().galley(pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, p.text);
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let fill = lerp_color(p.border_strong, p.accent, t);
    ui.painter().rect_filled(sw, CornerRadius::same(9), fill);
    let knob = pos2(egui::lerp((sw.left() + 9.0)..=(sw.right() - 9.0), t), sw.center().y);
    ui.painter().circle_filled(knob + vec2(0.0, 1.0), 7.0, Color32::from_black_alpha(50));
    ui.painter().circle_filled(knob, 7.0, Color32::WHITE);
    if resp.has_focus() {
        ui.painter().rect_stroke(
            sw.expand(2.0),
            CornerRadius::same(11),
            Stroke::new(1.5, p.accent),
            StrokeKind::Outside,
        );
    }
    resp
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

/// Width of a list column.
#[derive(Clone, Copy)]
pub enum Col {
    Px(f32),
    Fr(f32),
}

/// A row's rect without its 1 px bottom border (CSS `border-bottom`).
fn above_border(rect: Rect) -> Rect {
    Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - 1.0))
}

/// Column rects in a list row (12 px side padding, columns 10 apart).
fn columns(rect: Rect, cols: &[Col]) -> Vec<Rect> {
    let inner_w = rect.width() - 24.0;
    let gaps = 10.0 * cols.len().saturating_sub(1) as f32;
    let fixed: f32 = cols.iter().map(|c| if let Col::Px(w) = c { *w } else { 0.0 }).sum();
    let frs: f32 = cols.iter().map(|c| if let Col::Fr(f) = c { *f } else { 0.0 }).sum();
    let free = (inner_w - gaps - fixed).max(0.0);
    let mut x = rect.left() + 12.0;
    cols.iter()
        .map(|c| {
            let w = match c {
                Col::Px(w) => *w,
                Col::Fr(f) => free * f / frs.max(0.001),
            };
            let r = Rect::from_min_max(pos2(x, rect.top()), pos2(x + w, rect.bottom()));
            x += w + 10.0;
            r
        })
        .collect()
}

/// The 28 px column titles of a list. With `check` (on, mixed) the first
/// column holds a checkbox; returns true when it was clicked.
pub fn list_head(ui: &mut Ui, cols: &[Col], labels: &[&str], right: &[usize], check: Option<(bool, bool)>) -> bool {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), kit::size::TABLE_HEAD), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, p.well);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let rects = columns(above_border(rect), cols);
    let mut clicked = false;
    for (i, (label, r)) in labels.iter().zip(&rects).enumerate() {
        if i == 0
            && let Some((on, mixed)) = check
        {
            let b = Rect::from_min_size(pos2(r.left(), r.center().y - 7.5), vec2(15.0, 15.0));
            let c =
                ui.interact(b.expand(4.0), ui.make_persistent_id(("head-check", rect.top() as i32)), Sense::click());
            c.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, tr("Select all")));
            clicked = c.clicked();
            kit::paint_check(ui, b, on, mixed, c.hovered(), true);
            continue;
        }
        let g = kit::elide(ui, label, theme::medium(11.5), p.text_muted, r.width());
        let x = if right.contains(&i) { r.right() - g.size().x } else { r.left() };
        ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, p.text_muted);
    }
    clicked
}

/// A status in a list row.
#[derive(Clone, Copy)]
pub enum Stat<'a> {
    Code(u16),
    /// `dropped`, `failed`.
    Error(&'a str),
    /// `tunnel`.
    Faint(&'a str),
    /// Waiting for the response.
    Pending,
}

/// One cell of a list row.
#[derive(Clone, Copy)]
pub enum Cell<'a> {
    Empty,
    /// The row's checkbox (its state is the `check` argument of [`list_row`]).
    Check,
    /// A time or size: monospace 11.5, faint.
    Ts(&'a str),
    Method(&'a str),
    Status(Stat<'a>),
    /// Host (muted), path, `?query` (faint), and an optional pill after it.
    Path {
        host: &'a str,
        path: &'a str,
        query: &'a str,
        pill: Option<(&'a str, Tone)>,
    },
    Text {
        text: &'a str,
        muted: bool,
        mono: bool,
    },
    Pill(&'a str, Tone),
}

/// A 32 px list row with a hairline under it. Returns the row's response
/// (click to select); `check` is toggled by its checkbox.
pub fn list_row(
    ui: &mut Ui,
    cols: &[Col],
    cells: &[Cell<'_>],
    right: &[usize],
    selected: bool,
    name: &str,
    check: Option<&mut bool>,
) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), kit::size::ROW), Sense::click());
    resp.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, name));
    let rects = columns(above_border(rect), cols);
    let mut check_box = None;
    if let Some(on) = check
        && let Some(i) = cells.iter().position(|c| matches!(c, Cell::Check))
    {
        let b = Rect::from_min_size(pos2(rects[i].left(), rects[i].center().y - 7.5), vec2(15.0, 15.0));
        let c = ui.interact(b.expand(4.0), resp.id.with("check"), Sense::click());
        let value = *on;
        c.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, value, tr("Select")));
        if c.clicked() {
            *on = !*on;
        }
        check_box = Some((b, *on, c.hovered()));
    }
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, p.accent_soft);
        let bar = Rect::from_min_max(pos2(rect.left(), rect.top() + 6.0), pos2(rect.left() + 3.0, rect.bottom() - 7.0));
        painter.rect_filled(bar, CornerRadius { nw: 0, sw: 0, ne: 2, se: 2 }, p.accent);
    } else if ui.rect_contains_pointer(rect) {
        painter.rect_filled(rect, 0.0, p.well);
    }
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    for (i, (cell, r)) in cells.iter().zip(&rects).enumerate() {
        let at_right = right.contains(&i);
        let cy = r.center().y;
        let place = |g: &Arc<Galley>| {
            let x = if at_right { r.right() - g.size().x } else { r.left() };
            pos2(x, cy - g.size().y / 2.0)
        };
        let clip = ui.painter_at(r.expand2(vec2(0.0, 0.0)).intersect(ui.clip_rect()));
        match *cell {
            Cell::Empty => {}
            Cell::Check => {
                if let Some((b, on, hovered)) = check_box {
                    kit::paint_check(ui, b, on, false, hovered, true);
                }
            }
            Cell::Ts(t) => {
                let g = kit::elide(ui, t, theme::mono(11.5), p.text_faint, r.width());
                clip.galley(place(&g), g, p.text_faint);
            }
            Cell::Method(m) => {
                let w = method_tag_width(ui, m);
                let x = if at_right { r.right() - w } else { r.left() };
                paint_method_tag(&clip, pos2(x, cy), m, &p, dark);
            }
            Cell::Status(s) => paint_status(ui, &clip, r, s, at_right),
            Cell::Path { host, path, query, pill } => {
                let pill_w = pill.map_or(0.0, |(t, _)| pill_width(ui, t) + 6.0);
                let job = path_job(host, path, query, 12.0, &p, (r.width() - pill_w).max(10.0));
                let g = ui.painter().layout_job(job);
                // An elided path fills its space (the pill ends at the cell's end).
                let elided = g.elided;
                let end = if elided { r.right() - pill_w } else { r.left() + g.size().x };
                clip.galley(pos2(r.left(), cy - g.size().y / 2.0), g, p.text);
                if let Some((t, tone)) = pill {
                    paint_pill(ui, &clip, pos2(end + 6.0, cy), t, tone);
                }
            }
            Cell::Text { text, muted, mono } => {
                let font = if mono { theme::mono(12.0) } else { FontId::proportional(12.5) };
                let color = if muted { p.text_muted } else { p.text };
                let g = kit::elide(ui, text, font, color, r.width());
                clip.galley(place(&g), g, color);
            }
            Cell::Pill(t, tone) => {
                let w = pill_width(ui, t);
                let x = if at_right { r.right() - w } else { r.left() };
                paint_pill(ui, &clip, pos2(x, cy), t, tone);
            }
        }
    }
    resp
}

fn paint_status(ui: &Ui, painter: &egui::Painter, r: &Rect, s: Stat<'_>, at_right: bool) {
    let p = theme::pal(ui);
    let (text, font, color, bold) = match s {
        Stat::Code(c) => (c.to_string(), theme::mono(12.0), code_color(&p, c), true),
        Stat::Error(t) => (t.to_string(), FontId::proportional(11.5), p.err, false),
        Stat::Faint(t) => (t.to_string(), FontId::proportional(11.5), p.text_faint, false),
        Stat::Pending => {
            let t = ui.input(|i| i.time) as f32;
            let a = 0.675 + 0.325 * (t * std::f32::consts::TAU / 1.4).cos();
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            ("···".to_string(), theme::mono(12.0), p.text_faint.gamma_multiply(a), false)
        }
    };
    let g = kit::elide(ui, &text, font, color, r.width());
    let x = if at_right { r.right() - g.size().x } else { r.left() };
    let pos = pos2(x, r.center().y - g.size().y / 2.0);
    if bold {
        // JetBrains Mono ships in one weight: a second pass makes it semibold.
        painter.galley(pos + vec2(0.45, 0.0), g.clone(), color);
    }
    painter.galley(pos, g, color);
}

/// Color of a status code (1xx and 3xx info, 2xx ok, 4xx warn, 5xx err).
pub fn code_color(p: &Palette, code: u16) -> Color32 {
    match code {
        0..=199 => p.info,
        200..=299 => p.ok,
        300..=399 => p.info,
        400..=499 => p.warn,
        _ => p.err,
    }
}

/// Tone of a status code, like [`code_color`].
pub fn code_tone(code: u16) -> Tone {
    match code {
        0..=199 => Tone::Info,
        c => theme::status_tone(c),
    }
}

/// `host` (muted) + `path` + `?query` (faint) in monospace, cut at `width`.
pub fn path_job(host: &str, path: &str, query: &str, size: f32, p: &Palette, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let f = theme::mono(size);
    if !host.is_empty() {
        job.append(host, 0.0, TextFormat::simple(f.clone(), p.text_muted));
    }
    job.append(path, 0.0, TextFormat::simple(f.clone(), p.text));
    if !query.is_empty() {
        job.append(&format!("?{query}"), 0.0, TextFormat::simple(f, p.text_faint));
    }
    job.wrap = TextWrapping::truncate_at_width(width.max(10.0));
    job
}

fn method_color(m: &str, p: &Palette, dark: bool) -> Color32 {
    if m == "CONNECT" || m.is_empty() { p.text_muted } else { theme::method_color(m, dark) }
}

/// A method's text: semibold 10.5 with 0.02 em letter spacing.
fn method_galley(painter: &egui::Painter, m: &str, color: Color32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    let format = TextFormat { extra_letter_spacing: 0.21, ..TextFormat::simple(theme::semibold(10.5), color) };
    job.append(m, 0.0, format);
    painter.layout_job(job)
}

fn method_tag_width(ui: &Ui, m: &str) -> f32 {
    method_galley(ui.painter(), m, Color32::PLACEHOLDER).size().x + 12.0
}

/// A method on a soft fill of its color (`.mp`): 18 high, radius 5. `at` is
/// the left center.
fn paint_method_tag(painter: &egui::Painter, at: egui::Pos2, m: &str, p: &Palette, dark: bool) -> f32 {
    let c = method_color(m, p, dark);
    let g = method_galley(painter, m, c);
    let r = Rect::from_min_size(pos2(at.x, at.y - 9.0), vec2(g.size().x + 12.0, 18.0));
    painter.rect_filled(r, CornerRadius::same(5), theme::soft(c, 0.13));
    painter.galley(r.center() - g.size() / 2.0, g, c);
    r.width()
}

/// [`paint_method_tag`] as a widget.
pub fn method_tag(ui: &mut Ui, m: &str) -> Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let w = method_tag_width(ui, m);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, m));
    paint_method_tag(ui.painter(), rect.left_center(), m, &p, dark);
    resp
}

fn pill_width(ui: &Ui, text: &str) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), theme::medium(11.5), Color32::PLACEHOLDER).size().x + 14.0
}

/// A 20 px pill painted with its left center at `at`.
fn paint_pill(ui: &Ui, painter: &egui::Painter, at: egui::Pos2, text: &str, tone: Tone) {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    let g = painter.layout_no_wrap(text.to_string(), theme::medium(11.5), fg);
    let r = Rect::from_min_size(pos2(at.x, at.y - 10.0), vec2(g.size().x + 14.0, 20.0));
    painter.rect_filled(r, CornerRadius::same(10), bg);
    painter.galley(r.center() - g.size() / 2.0, g, fg);
}

/// The rows of a list, virtualized; `add` paints the rows in `range`.
pub fn list_scroll(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    total: usize,
    bottom: bool,
    add: impl FnOnce(&mut Ui, std::ops::Range<usize>),
) {
    let area = egui::ScrollArea::vertical().id_salt(id).auto_shrink([false; 2]).stick_to_bottom(bottom);
    kit::show_rows(area, ui, kit::size::ROW, total, |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui, range);
    });
}

// ---------------------------------------------------------------------------
// Panes and detail
// ---------------------------------------------------------------------------

/// List and detail side by side (the list `left` of the width) when the
/// view is at least 760 wide, else stacked (list 44 % of the height).
pub fn panes(ui: &mut Ui, id: Id, left_share: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let (w, h) = (ui.available_width(), ui.available_height());
    let side = w >= 760.0;
    let g = kit::size::GUTTER;
    // The design sizes the list as a share of the whole width.
    let default = if side { left_share * w / (w - g).max(1.0) } else { 0.44 * h / (h - g).max(1.0) };
    kit::panes(ui, id, side, default.clamp(0.15, 0.85), left, right);
}

/// The 36 px title of a list pane: name, count pill, actions on the right
/// (added right to left).
pub fn pane_title(ui: &mut Ui, label: &str, count: Option<&str>, end: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    row(
        ui,
        kit::size::TABS,
        (14.0, 12.0),
        8.0,
        |ui| {
            ui.add(egui::Label::new(kit::section_title(label).color(p.text)).selectable(false));
            if let Some(c) = count {
                kit::tone_pill(ui, c, Tone::Neutral);
            }
        },
        end,
    );
}

/// The 36 px head of the detail pane.
pub fn detail_head(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    row(ui, kit::size::TABS, (14.0, 10.0), 8.0, left, right);
}

/// The 40 px metrics row under [`detail_head`].
pub fn detail_meta(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    row(ui, kit::size::TOOLBAR, (12.0, 10.0), 8.0, left, right);
}

/// A path in the detail head: monospace 12 semibold, query faint, elided.
pub fn path_label(ui: &mut Ui, host: &str, path: &str, query: &str) -> Response {
    let p = theme::pal(ui);
    let job = path_job(host, path, query, 12.0, &p, ui.available_width());
    let g = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x, 20.0), Sense::hover());
    let full = format!("{host}{path}{}{query}", if query.is_empty() { "" } else { "?" });
    resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, &full));
    let pos = pos2(rect.left(), rect.center().y - g.size().y / 2.0);
    ui.painter().galley(pos + vec2(0.45, 0.0), g.clone(), p.text);
    ui.painter().galley(pos, g, p.text);
    if g_elided(&resp, &full, ui) {
        return resp.on_hover_text(full);
    }
    resp
}

fn g_elided(resp: &Response, full: &str, ui: &Ui) -> bool {
    let w = ui.painter().layout_no_wrap(full.to_string(), theme::mono(12.0), Color32::PLACEHOLDER).size().x;
    w > resp.rect.width() + 0.5
}

/// A muted metric (`⏱ 12 ms`), 22 high; the icon is optional.
pub fn metric(ui: &mut Ui, icon: Option<&str>, text: &str, tip: Option<&str>) -> Response {
    let p = theme::pal(ui);
    let ig = icon.map(|i| ui.painter().layout_no_wrap(i.to_string(), theme::icon_font(13.0), Color32::PLACEHOLDER));
    let (tg, text_w) = kit::tabular_text(ui, text, FontId::proportional(12.0));
    let w = 6.0 + ig.as_ref().map_or(0.0, |g| g.size().x + 4.0) + text_w + 6.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(5), p.hover);
        }
        let c = if hovered { p.text } else { p.text_muted };
        let mut x = rect.left() + 6.0;
        if let Some(ig) = ig {
            ui.painter().galley(pos2(x, rect.center().y - ig.size().y / 2.0), ig.clone(), c);
            x += ig.size().x + 4.0;
        }
        ui.painter().galley(pos2(x, rect.center().y - tg.size().y / 2.0), tg, c);
    }
    match tip {
        Some(t) => resp.on_hover_text(t),
        None => resp,
    }
}

/// Metrics 4 apart.
pub fn metrics(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        add(ui);
    });
}

/// The scrolling content of the detail pane: padding 12 12 16, 16 apart.
pub fn detail_scroll(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false; 2]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 12, bottom: 16 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 16.0;
            add(ui);
        });
    });
}

/// A titled section: title, optional count, `end` on the right; content 8
/// under it.
pub fn section(
    ui: &mut Ui,
    title: &str,
    count: Option<usize>,
    end: impl FnOnce(&mut Ui),
    content: impl FnOnce(&mut Ui),
) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        hrow(ui, 20.0, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add(egui::Label::new(kit::section_title(title).color(p.text)).selectable(false));
            if let Some(n) = count {
                kit::tone_pill(ui, &n.to_string(), Tone::Neutral);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), end);
        });
        content(ui);
    });
}

/// A section that opens and closes (caret, title, count); `id` keeps its state.
pub fn disclosure(
    ui: &mut Ui,
    id: Id,
    title: &str,
    count: Option<usize>,
    default_open: bool,
    content: impl FnOnce(&mut Ui),
) {
    let p = theme::pal(ui);
    let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(default_open));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        let head =
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 20.0), Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
                ui.add(
                    egui::Label::new(RichText::new(caret).font(theme::icon_font(12.5)).color(p.text_faint))
                        .selectable(false),
                );
                ui.add(egui::Label::new(kit::section_title(title).color(p.text)).selectable(false));
                if let Some(n) = count {
                    kit::tone_pill(ui, &n.to_string(), Tone::Neutral);
                }
            });
        let r = ui.interact(head.response.rect, id.with("toggle"), Sense::click());
        r.widget_info(|| WidgetInfo::labeled(Role::Button, true, title));
        r.ctx.accesskit_node_builder(r.id, |n| n.set_expanded(open));
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(id, open));
        }
        if open {
            content(ui);
        }
    });
}

/// Headers (or any name/value pairs) as a table: a 30 px head on `well`,
/// 34 px rows, names muted, both monospace 12 and cut with an ellipsis
/// (hover shows the whole value; right-click copies it).
pub fn headers_table(ui: &mut Ui, id: Id, rows: &[(&str, &str)]) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            let name_w = head.width() * 0.36;
            ui.painter().rect_filled(head, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, p.well);
            ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));
            // Cells center their text above their bottom border.
            let head_cy = head.center().y - 0.5;
            for (x, t) in [(head.left() + 12.0, tr("Name")), (head.left() + name_w + 12.0, tr("Value"))] {
                ui.painter().text(pos2(x, head_cy), Align2::LEFT_CENTER, t, theme::medium(11.5), p.text_muted);
            }
            for (i, (k, v)) in rows.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                let resp = resp;
                if resp.hovered() {
                    let r = if i + 1 == rows.len() {
                        CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }
                    } else {
                        CornerRadius::ZERO
                    };
                    ui.painter().rect_filled(rect, r, p.well);
                }
                let last = i + 1 == rows.len();
                if !last {
                    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
                }
                let cy = if last { rect.center().y } else { rect.center().y - 0.5 };
                let kg = kit::elide(ui, k, theme::mono(12.0), p.text_muted, name_w - 24.0);
                ui.painter().galley(pos2(rect.left() + 12.0, cy - kg.size().y / 2.0), kg, p.text_muted);
                let vw = rect.width() - name_w - 24.0;
                let vg = kit::elide(ui, v, theme::mono(12.0), p.text, vw);
                let cut =
                    vg.size().x + 1.0 < ui.painter().layout_no_wrap(v.to_string(), theme::mono(12.0), p.text).size().x;
                ui.painter().galley(pos2(rect.left() + name_w + 12.0, cy - vg.size().y / 2.0), vg, p.text);
                resp.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{k}: {v}")));
                let resp = if cut { resp.on_hover_text(*v) } else { resp };
                resp.context_menu(|ui| {
                    if kit::menu_item(ui, Some(icons::COPY), tr("Copy value"), None).clicked() {
                        ui.ctx().copy_text(v.to_string());
                    }
                    if kit::menu_item(ui, None, tr("Copy"), None).clicked() {
                        ui.ctx().copy_text(format!("{k}: {v}"));
                    }
                });
                let _ = id;
            }
        });
}

/// A body in a bordered box: a read-only code view with line numbers (JSON
/// is formatted and highlighted).
pub fn body_box(ui: &mut Ui, id: Id, text: &str) {
    let p = theme::pal(ui);
    let start = text.trim_start();
    let json = (start.starts_with('{') || start.starts_with('['))
        && (text.len() > 64 * 1024 || serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok());
    let lang = if json { crate::highlight::Lang::Json } else { crate::highlight::Lang::Text };
    egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(8)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            let mut t = text.to_string();
            let r = widgets::code_editor_gutter(ui, id, &mut t, lang, None, 1, true, None);
            widgets::a11y_name(&r, tr("Body"));
        },
    );
}

/// "No body" in a dashed 32 px box.
pub fn no_body(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    dashed_rect(ui.painter(), rect.shrink(0.5), Stroke::new(1.0, p.border));
    let mut x = rect.left() + 12.0;
    let ig = ui.painter().layout_no_wrap(icons::MINUS.into(), theme::icon_font(12.0), p.text_faint);
    ui.painter().galley(pos2(x, rect.center().y - ig.size().y / 2.0), ig.clone(), p.text_faint);
    x += ig.size().x + 8.0;
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), p.text_muted);
    ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.text_muted);
}

fn dashed_rect(painter: &egui::Painter, r: Rect, stroke: Stroke) {
    let (dash, gap) = (4.0, 3.0);
    for pts in [
        [r.left_top() + vec2(6.0, 0.0), r.right_top() - vec2(6.0, 0.0)],
        [r.left_bottom() + vec2(6.0, 0.0), r.right_bottom() - vec2(6.0, 0.0)],
        [r.left_top() + vec2(0.0, 6.0), r.left_bottom() - vec2(0.0, 6.0)],
        [r.right_top() + vec2(0.0, 6.0), r.right_bottom() - vec2(0.0, 6.0)],
    ] {
        painter.extend(egui::Shape::dashed_line(&pts, stroke, dash, gap));
    }
    // Rounded corners (radius 8, drawn as short arcs).
    for (c, a0) in [
        (r.left_top() + vec2(6.0, 6.0), std::f32::consts::PI),
        (r.right_top() + vec2(-6.0, 6.0), -std::f32::consts::FRAC_PI_2),
        (r.right_bottom() + vec2(-6.0, -6.0), 0.0),
        (r.left_bottom() + vec2(6.0, -6.0), std::f32::consts::FRAC_PI_2),
    ] {
        let pts: Vec<egui::Pos2> = (0..=6)
            .map(|i| {
                let a = a0 + i as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
                c + 6.0 * vec2(a.cos(), a.sin())
            })
            .collect();
        painter.line(pts, stroke);
    }
}

/// The empty detail pane: "Select a request" and what will appear there.
pub fn select_hint(ui: &mut Ui, body: &str) {
    kit::empty_state(ui, icons::ARROW_LEFT, tr("Select a request"), body, |_| {});
}

/// True when Escape should close the selected row's detail (nothing has
/// focus and no popup is open).
pub fn esc_closes(ui: &Ui) -> bool {
    ui.input(|i| i.key_pressed(egui::Key::Escape))
        && ui.memory(|m| m.focused().is_none())
        && !egui::Popup::is_any_open(ui.ctx())
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// `14:02:31.123` in local time.
pub fn clock_ms(at_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string())
        .unwrap_or_default()
}

/// The reason phrase of common status codes.
pub fn reason(code: u16) -> &'static str {
    match code {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// `curl` command line that sends a recorded request again.
pub fn curl(method: &str, url: &str, headers: &[(String, String)], body: &str) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let mut out = format!("curl -X {method} {}", q(url));
    for (k, v) in headers {
        if ["host", "content-length", "connection", "transfer-encoding"].iter().any(|h| k.eq_ignore_ascii_case(h)) {
            continue;
        }
        out.push_str(&format!(" \\\n  -H {}", q(&format!("{k}: {v}"))));
    }
    if !body.is_empty() {
        out.push_str(&format!(" \\\n  --data-raw {}", q(body)));
    }
    out
}

/// The value of `"type"` in a JSON object body (webhook event types).
pub fn event_type(body: &[u8]) -> Option<String> {
    let start = body.iter().position(|b| !b.is_ascii_whitespace())?;
    if body[start] != b'{' || body.len() > 1024 * 1024 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    v.get("type")?.as_str().map(str::to_string)
}

/// Fixed heights of the pattern.
pub mod size {
    /// The settings strip.
    pub const STRIP: f32 = 40.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_share_the_free_width() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(424.0, 32.0));
        let c = columns(r, &[Col::Px(86.0), Col::Fr(1.0), Col::Px(62.0)]);
        assert_eq!(c[0].left(), 12.0);
        assert_eq!(c[1].left(), 108.0);
        assert_eq!(c[1].width(), 400.0 - 20.0 - 148.0);
        assert_eq!(c[2].right(), 412.0);
    }

    #[test]
    fn curl_quotes_and_skips_hop_headers() {
        let h = vec![("Host".to_string(), "x".to_string()), ("X-A".to_string(), "it's".to_string())];
        let c = curl("POST", "http://127.0.0.1:1/a", &h, "{}");
        assert_eq!(c, "curl -X POST 'http://127.0.0.1:1/a' \\\n  -H 'X-A: it'\\''s' \\\n  --data-raw '{}'");
    }

    #[test]
    fn event_types_come_from_json_bodies() {
        assert_eq!(event_type(br#" {"type":"charge.succeeded","id":1}"#).as_deref(), Some("charge.succeeded"));
        assert_eq!(event_type(b"type=x"), None);
        assert_eq!(event_type(br#"{"type":3}"#), None);
    }
}
