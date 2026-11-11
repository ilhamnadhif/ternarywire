//! Shared components drawn with the design tokens of `theme.rs`. Every
//! component has a twin in the redesign prototype (`design/redesign/kit.js`
//! and `styles.css`) with the same sizes: screens use these instead of
//! styling egui widgets one by one, so every view looks and behaves the same.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use crate::theme::{self, RADIUS, RADIUS_SM};
pub use crate::theme::{Tone, size};

fn body_size(ui: &Ui) -> f32 {
    ui.style().text_styles.get(&egui::TextStyle::Body).map(|f| f.size).unwrap_or(13.0)
}

/// One line of text cut with an ellipsis at `width`.
pub fn elide(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Filled accent: the one main action of a view.
    Primary,
    /// Outlined surface: ordinary actions.
    Secondary,
    /// No frame until hovered: toolbars and quiet actions.
    Ghost,
    /// Outlined, red text: destructive actions.
    Danger,
}

/// Button heights: 24 (toolbars), 30 (default), 38 (the action of a pane, like Send).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Sm,
    Md,
    Lg,
}

/// Everything a button can show.
#[derive(Clone, Copy, Default)]
pub struct ButtonOpts<'a> {
    pub icon: Option<&'a str>,
    /// A chevron after the label (the button opens a menu).
    pub caret: bool,
    /// A keyboard hint after the label.
    pub kbd: Option<&'a str>,
    /// Drawn as selected (accent soft fill).
    pub on: bool,
    /// Minimum width.
    pub min_width: f32,
}

/// A button with an optional leading icon. `small` makes it 24 px high
/// instead of 30.
pub fn button_ex(ui: &mut Ui, kind: Kind, icon: Option<&str>, text: &str, small: bool) -> Response {
    button(ui, kind, if small { Size::Sm } else { Size::Md }, text, ButtonOpts { icon, ..Default::default() })
}

/// The general button: kind, size, label and options.
pub fn button(ui: &mut Ui, kind: Kind, size: Size, text: &str, o: ButtonOpts<'_>) -> Response {
    let p = theme::pal(ui);
    // `pad` is the side padding plus the 1 px border of `.btn` (transparent
    // on ghost buttons); the 38 px action button has its padding only.
    let (height, font_size, icon_size, pad, gap, radius) = match size {
        Size::Sm => (24.0, 12.0, 13.0, 9.0, 5.0, RADIUS_SM),
        Size::Md => (30.0, 12.5, 15.0, 13.0, 6.0, RADIUS),
        Size::Lg => (38.0, 13.0, 15.0, 16.0, 7.0, 9),
    };
    let enabled = ui.is_enabled();
    let text_color = match kind {
        Kind::Primary => p.on_accent,
        Kind::Danger => p.err,
        Kind::Secondary => p.text,
        Kind::Ghost if o.on => p.accent_text,
        Kind::Ghost => p.text_muted,
    };
    let galley = (!text.is_empty())
        .then(|| ui.painter().layout_no_wrap(text.to_string(), theme::medium(font_size), Color32::PLACEHOLDER));
    let icon_galley =
        o.icon.map(|i| ui.painter().layout_no_wrap(i.to_string(), theme::icon_font(icon_size), Color32::PLACEHOLDER));
    let caret_galley = o.caret.then(|| {
        ui.painter().layout_no_wrap(crate::icons::CARET_DOWN.into(), theme::icon_font(icon_size), Color32::PLACEHOLDER)
    });
    let kbd_w = o.kbd.map(|k| kbd_size(ui, k).x + gap).unwrap_or(0.0);
    let parts = [
        icon_galley.as_ref().map(|g| g.size().x),
        galley.as_ref().map(|g| g.size().x),
        caret_galley.as_ref().map(|g| g.size().x),
    ];
    let n = parts.iter().flatten().count();
    let content_w = parts.iter().flatten().sum::<f32>() + gap * n.saturating_sub(1) as f32 + kbd_w;
    let width = if galley.is_none() && caret_galley.is_none() { height } else { content_w + pad * 2.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width.max(height).max(o.min_width), height), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, text));
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        let pressed = enabled && resp.is_pointer_button_down_on();
        let (fill, stroke) = match kind {
            Kind::Primary => (if hovered || pressed { p.accent_hover } else { p.accent }, Stroke::NONE),
            Kind::Secondary | Kind::Danger => (
                if pressed {
                    p.subtle
                } else if hovered {
                    p.hover
                } else {
                    p.surface
                },
                Stroke::new(1.0, p.border_strong),
            ),
            Kind::Ghost => (
                if o.on {
                    p.accent_soft
                } else if pressed {
                    p.subtle
                } else if hovered {
                    p.hover
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::NONE,
            ),
        };
        let text_color = if kind == Kind::Ghost && hovered && !o.on { p.text } else { text_color };
        let painter = ui.painter();
        let r = CornerRadius::same(radius);
        painter.rect(rect, r, fill, stroke, StrokeKind::Inside);
        if resp.has_focus() {
            painter.rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(radius + 2),
                Stroke::new(1.5, p.accent),
                StrokeKind::Outside,
            );
        }
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        // Icons and the caret take the color of the label (`.btn .i`).
        let icon_color = text_color;
        let mut x = rect.center().x - content_w / 2.0;
        let put = |g: std::sync::Arc<egui::Galley>, color: Color32, x: &mut f32| {
            painter.galley(pos2(*x, rect.center().y - g.size().y / 2.0), g.clone(), fade(color));
            *x += g.size().x + gap;
        };
        if let Some(g) = icon_galley {
            put(g, icon_color, &mut x);
        }
        if let Some(g) = galley {
            put(g, text_color, &mut x);
        }
        if let Some(k) = o.kbd {
            let s = kbd_size(ui, k);
            paint_kbd(ui, Rect::from_min_size(pos2(x, rect.center().y - s.y / 2.0), s), k);
            x += s.x + gap;
        }
        if let Some(g) = caret_galley {
            put(g, text_color, &mut x);
        }
    }
    resp
}

pub fn primary(ui: &mut Ui, text: &str) -> Response {
    button_ex(ui, Kind::Primary, None, text, false)
}

pub fn primary_icon(ui: &mut Ui, icon: &str, text: &str) -> Response {
    button_ex(ui, Kind::Primary, Some(icon), text, false)
}

pub fn secondary(ui: &mut Ui, icon: Option<&str>, text: &str) -> Response {
    button_ex(ui, Kind::Secondary, icon, text, false)
}

/// A quiet toolbar button (24 high).
pub fn ghost(ui: &mut Ui, icon: Option<&str>, text: &str) -> Response {
    button_ex(ui, Kind::Ghost, icon, text, true)
}

/// The action of a pane (Send, Connect, Invoke): 38 high, next to the URL bar.
pub fn action_button(ui: &mut Ui, kind: Kind, icon: &str, text: &str) -> Response {
    button(ui, kind, Size::Lg, text, ButtonOpts { icon: Some(icon), ..Default::default() })
}

/// A square, frameless icon button (28) with a tooltip (also its accessible name).
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    icon_button_ex(ui, icon, tooltip, false, 28.0)
}

/// A small icon button (22): row actions, toolbars of dense panels.
pub fn icon_button_sm(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    icon_button_ex(ui, icon, tooltip, false, 22.0)
}

pub fn icon_button_ex(ui: &mut Ui, icon: &str, tooltip: &str, on: bool, side: f32) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    let resp = resp.on_hover_text(tooltip);
    let enabled = ui.is_enabled();
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, tooltip));
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        let painter = ui.painter();
        let radius = CornerRadius::same(if side <= 22.0 {
            5
        } else if side >= 36.0 {
            9
        } else {
            RADIUS_SM
        });
        if on {
            painter.rect_filled(rect, radius, p.accent_soft);
        } else if resp.is_pointer_button_down_on() {
            painter.rect_filled(rect, radius, p.subtle);
        } else if hovered {
            painter.rect_filled(rect, radius, p.hover);
        }
        if resp.has_focus() {
            painter.rect_stroke(rect, radius, Stroke::new(1.5, p.accent), StrokeKind::Inside);
        }
        let color = if on {
            p.accent_text
        } else if hovered {
            p.text
        } else {
            p.text_muted
        };
        let color = if enabled { color } else { color.gamma_multiply(0.45) };
        let icon_size = match side as u32 {
            0..=22 => 13.0,
            23..=30 => 16.0,
            _ => (side * 0.5).round(),
        };
        painter.text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(icon_size), color);
    }
    resp
}

// ---------------------------------------------------------------------------
// Tabs and segmented controls
// ---------------------------------------------------------------------------

/// One tab of [`tabs`].
#[derive(Clone)]
pub struct Tab<'a, T> {
    pub value: T,
    pub label: &'a str,
    pub count: Option<String>,
    /// Color of the count badge (results: ok / err).
    pub tone: Tone,
    /// A small green dot: the section has something set (body, auth).
    pub dot: bool,
    pub icon: Option<&'a str>,
}

impl<'a, T> Tab<'a, T> {
    pub fn new(value: T, label: &'a str) -> Self {
        Tab { value, label, count: None, tone: Tone::Neutral, dot: false, icon: None }
    }
    /// A count badge; hidden when `n` is 0.
    pub fn count(mut self, n: usize) -> Self {
        self.count = (n > 0).then(|| n.to_string());
        self
    }
    pub fn badge(mut self, text: impl Into<String>, tone: Tone) -> Self {
        self.count = Some(text.into());
        self.tone = tone;
        self
    }
    pub fn dot(mut self, on: bool) -> Self {
        self.dot = on;
        self
    }
    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// A full-width tab bar (36 high, 12 px side padding, hairline under the
/// whole width): the tabs of a pane. Returns true when the selection changed.
pub fn tabs<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[Tab<'_, T>]) -> bool {
    tabs_with(ui, current, items, |_| {})
}

/// [`tabs`] with extra controls at the right end of the bar (added right to left).
pub fn tabs_with<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: &mut T,
    items: &[Tab<'_, T>],
    right: impl FnOnce(&mut Ui),
) -> bool {
    let p = theme::pal(ui);
    let full = ui.available_rect_before_wrap();
    let rect = Rect::from_min_size(full.min, vec2(full.width(), size::TABS));
    // The tabs fill the 35 px above the hairline; the underline of the
    // selected one covers the hairline.
    let inner = Rect::from_min_max(
        pos2(rect.left() + size::PAD, rect.top()),
        pos2(rect.right() - size::PAD, rect.bottom() - 1.0),
    );
    let mut controls = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    controls.spacing_mut().item_spacing.x = 8.0;
    right(&mut controls);
    let used = controls.min_rect();
    let tabs_right = if used.width() > 0.5 { used.left() - 8.0 } else { inner.right() };
    let tabs_rect = inner.with_max_x(tabs_right.max(inner.left()));
    let mut tabs_ui = ui.new_child(UiBuilder::new().max_rect(tabs_rect).layout(Layout::left_to_right(Align::Center)));
    let changed = tab_row(&mut tabs_ui, current, items, false);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    ui.advance_cursor_after_rect(rect);
    changed
}

/// Paints the tabs at the cursor (35 high, the underline on its bottom edge).
fn tab_row<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[Tab<'_, T>], _compat: bool) -> bool {
    let p = theme::pal(ui);
    const PAD: f32 = 9.0;
    let h = size::TABS - 1.0;
    let measured: Vec<_> = items
        .iter()
        .map(|t| {
            let selected = *current == t.value;
            let g = ui.painter().layout_no_wrap(t.label.to_string(), theme::medium(12.5), Color32::PLACEHOLDER);
            let c = t.count.as_ref().map(|c| {
                let (fg, _) = badge_colors(&p, t.tone, selected);
                ui.painter().layout_no_wrap(c.clone(), theme::semibold(10.5), fg)
            });
            let ig = t
                .icon
                .map(|i| ui.painter().layout_no_wrap(i.to_string(), theme::icon_font(14.0), Color32::PLACEHOLDER));
            let w = PAD * 2.0
                + g.size().x
                + c.as_ref().map_or(0.0, |c| 6.0 + (c.size().x + 10.0).max(17.0))
                + if t.dot { 12.0 } else { 0.0 }
                + ig.as_ref().map_or(0.0, |g| g.size().x + 6.0);
            (selected, g, c, ig, w)
        })
        .collect();
    let total: f32 = measured.iter().map(|m| m.4).sum::<f32>() + 2.0 * items.len().saturating_sub(1) as f32;
    let mut changed = false;
    let mut draw = |ui: &mut Ui| {
        // Exactly the tabs' height (`ui.horizontal` would center them in an
        // interact-size row and push them down).
        ui.allocate_ui_with_layout(vec2(ui.available_width(), h), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (t, (selected, g, c, ig, w)) in items.iter().zip(&measured) {
                let (rect, resp) = ui.allocate_exact_size(vec2(*w, h), Sense::click());
                let name = match &t.count {
                    Some(c) => format!("{} ({c})", t.label),
                    None => t.label.to_string(),
                };
                crate::widgets::a11y_tab(&resp, &name, *selected);
                if resp.clicked() && !*selected {
                    *current = t.value;
                    changed = true;
                }
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let painter = ui.painter();
                let color = if *selected || resp.hovered() { p.text } else { p.text_muted };
                let mut x = rect.left() + PAD;
                let cy = rect.center().y;
                if let Some(ig) = ig {
                    painter.galley(pos2(x, cy - ig.size().y / 2.0), ig.clone(), color);
                    x += ig.size().x + 6.0;
                }
                painter.galley(pos2(x, cy - g.size().y / 2.0), g.clone(), color);
                x += g.size().x;
                if let Some(cg) = c {
                    let w = (cg.size().x + 10.0).max(17.0);
                    let pill = Rect::from_min_size(pos2(x + 6.0, cy - 8.0), vec2(w, 16.0));
                    let (_, bg) = badge_colors(&p, t.tone, *selected);
                    painter.rect_filled(pill, CornerRadius::same(8), bg);
                    painter.galley(pill.center() - cg.size() / 2.0, cg.clone(), Color32::PLACEHOLDER);
                    x = pill.right();
                }
                if t.dot {
                    painter.circle_filled(pos2(x + 9.0, cy), 3.0, p.ok);
                }
                if *selected {
                    let line = Rect::from_min_max(
                        pos2(rect.left() + 8.0, rect.bottom() - 1.0),
                        pos2(rect.right() - 8.0, rect.bottom() + 1.0),
                    );
                    painter.rect_filled(line, CornerRadius::same(1), p.accent);
                }
                if resp.has_focus() {
                    painter.rect_stroke(
                        rect.shrink(3.0),
                        CornerRadius::same(RADIUS_SM),
                        Stroke::new(1.5, p.accent),
                        StrokeKind::Inside,
                    );
                }
            }
        });
    };
    if total <= ui.available_width() + 0.5 {
        draw(ui);
    } else {
        let id = ui.next_auto_id().with("tab-strip");
        let out = egui::ScrollArea::horizontal()
            .id_salt(id)
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| draw(ui));
        let (view, offset) = (out.inner_rect, out.state.clamped_offset().x);
        let hidden_right = out.content_size.x - view.width() - offset;
        let fill = ui.visuals().panel_fill;
        if offset > 0.5 {
            fade(ui.painter(), Rect::from_min_size(view.left_top(), vec2(28.0, view.height() - 1.0)), fill, false);
        }
        if hidden_right > 0.5 {
            let edge = Rect::from_min_size(pos2(view.right() - 28.0, view.top()), vec2(28.0, view.height() - 1.0));
            fade(ui.painter(), edge, fill, true);
        }
    }
    changed
}

fn badge_colors(p: &theme::Palette, tone: Tone, selected: bool) -> (Color32, Color32) {
    match tone {
        Tone::Neutral if selected => (p.accent_text, p.accent_soft),
        Tone::Neutral => (p.text_muted, p.subtle),
        t => p.tone(t),
    }
}

/// Covers `rect` with `color`, from clear to opaque towards the right
/// (`to_right`) or the left: the edge of a strip that hides more.
pub fn fade(painter: &egui::Painter, rect: Rect, color: Color32, to_right: bool) {
    let (left, right) = if to_right { (Color32::TRANSPARENT, color) } else { (color, Color32::TRANSPARENT) };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// The small segmented control of toolbars (26 high).
pub fn segmented_sm<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, items: &[(T, &str)]) -> bool {
    let items: Vec<(T, Option<&str>, &str)> = items.iter().map(|(v, l)| (*v, None, *l)).collect();
    segmented_ex(ui, current, &items, true)
}

/// A segmented control whose segments may have an icon.
pub fn segmented_ex<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: &mut T,
    items: &[(T, Option<&str>, &str)],
    small: bool,
) -> bool {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let (seg_h, font, pad) = if small { (22.0, 11.5, 8.0) } else { (24.0, 12.0, 10.0) };
    let parts: Vec<_> = items
        .iter()
        .map(|(_, icon, label)| {
            let g = ui.painter().layout_no_wrap(label.to_string(), theme::medium(font), Color32::PLACEHOLDER);
            // Icons take the size of the label (`.seg button`).
            let ig =
                icon.map(|i| ui.painter().layout_no_wrap(i.to_string(), theme::icon_font(font), Color32::PLACEHOLDER));
            let w = g.size().x + ig.as_ref().map_or(0.0, |g| g.size().x + 5.0) + pad * 2.0;
            (g, ig, w)
        })
        .collect();
    let total = parts.iter().map(|p| p.2).sum::<f32>() + items.len().saturating_sub(1) as f32 + 4.0;
    // Segment ids come from this control's own (auto) id, so two controls
    // with the same labels in one view don't share clicks.
    let (rect, base) = ui.allocate_exact_size(vec2(total, seg_h + 4.0), Sense::hover());
    let mut changed = false;
    ui.painter().rect_filled(rect, CornerRadius::same(8), p.subtle);
    let mut x = rect.left() + 2.0;
    for (i, ((value, _, label), (g, ig, w))) in items.iter().zip(parts).enumerate() {
        let seg = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(w, seg_h));
        x += w + 1.0;
        let resp = ui.interact(seg, base.id.with(("segment", label, i)), Sense::click());
        let selected = *current == *value;
        crate::widgets::a11y_tab(&resp, label, selected);
        if resp.clicked() && !selected {
            *current = *value;
            changed = true;
        }
        let painter = ui.painter();
        if selected {
            if dark {
                painter.rect(
                    seg,
                    CornerRadius::same(6),
                    p.hover,
                    Stroke::new(1.0, p.border_strong),
                    StrokeKind::Inside,
                );
            } else {
                let shadow = egui::Shadow { offset: [0, 1], blur: 2, spread: 0, color: Color32::from_black_alpha(20) };
                painter.add(shadow.as_shape(seg, CornerRadius::same(6)));
                painter.rect(seg, CornerRadius::same(6), p.surface, Stroke::new(0.5, p.border), StrokeKind::Inside);
            }
        }
        let color = if selected || resp.hovered() { p.text } else { p.text_muted };
        let content_w = g.size().x + ig.as_ref().map_or(0.0, |g| g.size().x + 5.0);
        let mut cx = seg.center().x - content_w / 2.0;
        if let Some(ig) = ig {
            painter.galley(pos2(cx, seg.center().y - ig.size().y / 2.0), ig.clone(), color);
            cx += ig.size().x + 5.0;
        }
        painter.galley(pos2(cx, seg.center().y - g.size().y / 2.0), g, color);
    }
    changed
}

// ---------------------------------------------------------------------------
// Labels, pills, small pieces
// ---------------------------------------------------------------------------

/// Options of [`pill_ex`].
#[derive(Clone, Copy, Default)]
pub struct PillOpts<'a> {
    /// A dot before the text (connection or server state).
    pub dot: bool,
    /// The dot has a halo (live connection, running server).
    pub live: bool,
    /// 24 high instead of 20 (the status of a response).
    pub large: bool,
    /// Outlined instead of filled.
    pub outline: bool,
    pub icon: Option<&'a str>,
}

/// A rounded label on the soft fill of a tone (statuses, counts, kinds).
pub fn pill_ex(ui: &mut Ui, text: &str, tone: Tone, o: PillOpts<'_>) -> Response {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    paint_pill(ui, text, fg, if o.outline { Color32::TRANSPARENT } else { bg }, o)
}

/// A pill in the tone of `tone`.
pub fn tone_pill(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    pill_ex(ui, text, tone, PillOpts::default())
}

/// A rounded label on a soft fill of `color` (methods, statuses, counts).
pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let p = theme::pal(ui);
    // Known colors use their token fill; others a soft tint of the color.
    let bg = [
        (p.ok, p.ok_soft),
        (p.warn, p.warn_soft),
        (p.err, p.err_soft),
        (p.info, p.info_soft),
        (p.accent_text, p.accent_soft),
        (p.accent, p.accent_soft),
    ]
    .iter()
    .find(|(c, _)| *c == color)
    .map(|(_, b)| *b)
    .unwrap_or_else(|| {
        if color == p.text_muted || color == p.text_faint {
            p.subtle
        } else {
            theme::soft(color, if ui.visuals().dark_mode { 0.18 } else { 0.11 })
        }
    });
    paint_pill(ui, text, color, bg, PillOpts::default())
}

fn paint_pill(ui: &mut Ui, text: &str, fg: Color32, bg: Color32, o: PillOpts<'_>) -> Response {
    let p = theme::pal(ui);
    let (h, pad, font) = if o.large { (24.0, 9.0, 12.0) } else { (20.0, 7.0, 11.5) };
    // An outline is a 1 px border beside the padding; icons take the text's size.
    let pad = if o.outline { pad + 1.0 } else { pad };
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::medium(font), fg);
    let icon = o.icon.map(|i| ui.painter().layout_no_wrap(i.to_string(), theme::icon_font(font), fg));
    let lead = if o.dot { 11.0 } else { 0.0 } + icon.as_ref().map_or(0.0, |g| g.size().x + 5.0);
    let size = vec2(galley.size().x + lead + pad * 2.0, h);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = CornerRadius::same((h / 2.0) as u8);
        painter.rect(
            rect,
            r,
            bg,
            if o.outline { Stroke::new(1.0, p.border) } else { Stroke::NONE },
            StrokeKind::Inside,
        );
        let mut x = rect.left() + pad;
        if o.dot {
            let c = pos2(x + 3.0, rect.center().y);
            if o.live {
                painter.circle_filled(c, 6.0, theme::soft(fg, 0.25));
            }
            painter.circle_filled(c, 3.0, fg);
            x += 11.0;
        }
        if let Some(ig) = icon {
            painter.galley(pos2(x, rect.center().y - ig.size().y / 2.0), ig.clone(), fg);
            x += ig.size().x + 5.0;
        }
        painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    }
    resp
}

/// `200 OK` style status pill (24 high).
pub fn status_pill(ui: &mut Ui, status: u16, reason: &str) -> Response {
    let text = if reason.is_empty() { status.to_string() } else { format!("{status} {reason}") };
    pill_ex(ui, &text, theme::status_tone(status), PillOpts { large: true, ..Default::default() })
}

/// A muted metric (`142 ms`, `956 B`) with an icon; hover highlights it
/// (it usually explains itself in a tooltip or popover).
pub fn metric(ui: &mut Ui, icon: &str, text: &str) -> Response {
    metric_colored(ui, icon, text, None)
}

pub fn metric_colored(ui: &mut Ui, icon: &str, text: &str, color: Option<Color32>) -> Response {
    let p = theme::pal(ui);
    let ig = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(13.0), Color32::PLACEHOLDER);
    let tg = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(vec2(ig.size().x + 4.0 + tg.size().x + 12.0, 22.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(5), p.hover);
        }
        let c = color.unwrap_or(if hovered { p.text } else { p.text_muted });
        let mut x = rect.left() + 6.0;
        ui.painter().galley(pos2(x, rect.center().y - ig.size().y / 2.0), ig.clone(), c);
        x += ig.size().x + 4.0;
        ui.painter().galley(pos2(x, rect.center().y - tg.size().y / 2.0), tg, c);
    }
    resp
}

/// The size of a key cap for `keys`: the text, 5 px padding and a 1 px
/// border on each side (`.kbd`).
pub fn kbd_size(ui: &Ui, keys: &str) -> Vec2 {
    let g = ui.painter().layout_no_wrap(keys.to_string(), FontId::proportional(10.5), Color32::PLACEHOLDER);
    vec2(g.size().x + 12.0, 18.0)
}

/// Paints a key cap in `rect` (18 high; see [`kbd_size`]).
pub fn paint_kbd(ui: &Ui, rect: Rect, keys: &str) {
    let p = theme::pal(ui);
    let painter = ui.painter();
    // A key cap: 1 px border, 2 px at the bottom.
    painter.rect_filled(rect, CornerRadius::same(4), p.border);
    painter.rect_filled(
        Rect::from_min_max(rect.min + vec2(1.0, 1.0), rect.max - vec2(1.0, 2.0)),
        CornerRadius::same(3),
        p.well,
    );
    painter.text(rect.center() - vec2(0.0, 0.5), Align2::CENTER_CENTER, keys, FontId::proportional(10.5), p.text_muted);
}

/// A keyboard shortcut hint (`⌘↩`).
pub fn kbd(ui: &mut Ui, keys: &str) -> Response {
    let size = kbd_size(ui, keys);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_kbd(ui, rect, keys);
    }
    resp
}

/// A filled dot (environment color, connection state).
pub fn dot(ui: &mut Ui, color: Color32, radius: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 2.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
    resp
}

/// The chevron of a drop-down, for `ComboBox::icon` (egui's default is a
/// filled triangle that doesn't match the line icons).
pub fn combo_caret(ui: &Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, open: bool) {
    let c = rect.center();
    let (w, h) = (3.5, if open { -1.8 } else { 1.8 });
    let color = visuals.fg_stroke.color.gamma_multiply(0.6);
    let points = vec![pos2(c.x - w, c.y - h), pos2(c.x, c.y + h), pos2(c.x + w, c.y - h)];
    ui.painter().add(egui::Shape::line(points, Stroke::new(1.4, color)));
}

/// Focus indication of an input or input group drawn by hand: a 1 px accent
/// border and a soft 3 px accent halo just outside it.
pub fn focus_ring(ui: &Ui, rect: Rect, radius: u8) {
    let p = theme::pal(ui);
    let halo = theme::soft(p.accent, 0.16);
    let painter = ui.painter();
    painter.rect_stroke(rect, CornerRadius::same(radius), Stroke::new(3.0, halo), StrokeKind::Outside);
    painter.rect_stroke(rect, CornerRadius::same(radius), Stroke::new(1.0, p.accent), StrokeKind::Inside);
}

/// A thin vertical divider between toolbar groups (18 high by default).
pub fn vdivider(ui: &mut Ui, height: f32) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, height), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, p.border));
}

/// A section title (`Query parameters`): semibold 12.5.
pub fn section_title(text: &str) -> RichText {
    RichText::new(text).font(theme::semibold(12.5))
}

/// A small upper-case label above a group (`ENVIRONMENTS`, `QUERY`).
pub fn eyebrow(ui: &Ui, text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .font(theme::semibold(10.5))
        .color(theme::pal(ui).text_faint)
        .extra_letter_spacing(0.7)
}

/// Muted helper text under a field or section (12).
pub fn hint(ui: &Ui, text: &str) -> RichText {
    RichText::new(text).color(theme::pal(ui).text_muted).size(12.0)
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// A 15 px checkbox (accent fill when on). `mixed` draws a dash (some rows on).
pub fn check(ui: &mut Ui, on: &mut bool, name: &str) -> Response {
    check_ex(ui, on, false, name)
}

pub fn check_ex(ui: &mut Ui, on: &mut bool, mixed: bool, name: &str) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let enabled = ui.is_enabled();
    let value = *on;
    resp.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, value, name));
    if ui.is_rect_visible(rect) {
        paint_check(ui, rect, *on, mixed, resp.hovered(), enabled);
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(6),
                Stroke::new(1.5, p.accent),
                StrokeKind::Outside,
            );
        }
    }
    resp
}

/// Paints a checkbox in `rect` (15 × 15).
pub fn paint_check(ui: &Ui, rect: Rect, on: bool, mixed: bool, hovered: bool, enabled: bool) {
    let p = theme::pal(ui);
    let painter = ui.painter();
    let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
    if on || mixed {
        painter.rect_filled(rect, CornerRadius::same(4), fade(if hovered { p.accent_hover } else { p.accent }));
        if mixed && !on {
            painter.rect_filled(
                Rect::from_center_size(rect.center(), vec2(7.0, 2.0)),
                CornerRadius::same(1),
                Color32::WHITE,
            );
        } else {
            painter.text(
                rect.center() + vec2(0.0, 0.5),
                Align2::CENTER_CENTER,
                crate::icons::CHECK,
                theme::icon_font(11.0),
                Color32::WHITE,
            );
        }
    } else {
        let stroke = if hovered { p.text_faint } else { p.border_strong };
        painter.rect(rect, CornerRadius::same(4), p.surface, Stroke::new(1.0, fade(stroke)), StrokeKind::Inside);
    }
}

/// A checkbox with a label after it (the label is clickable too).
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let p = theme::pal(ui);
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(body_size(ui)), p.text);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(15.0 + 8.0 + g.size().x, g.size().y.max(18.0)), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let enabled = ui.is_enabled();
    let value = *on;
    resp.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, value, text));
    if ui.is_rect_visible(rect) {
        let b = Rect::from_min_size(pos2(rect.left(), rect.center().y - 7.5), vec2(15.0, 15.0));
        paint_check(ui, b, *on, false, resp.hovered(), enabled);
        let color = if enabled { p.text } else { p.text.gamma_multiply(0.45) };
        ui.painter().galley(pos2(b.right() + 8.0, rect.center().y - g.size().y / 2.0), g, color);
        if resp.has_focus() {
            ui.painter().rect_stroke(
                b.expand(2.0),
                CornerRadius::same(6),
                Stroke::new(1.5, p.accent),
                StrokeKind::Outside,
            );
        }
    }
    resp
}

/// An on/off switch (30 × 18). Returns a response that is `changed()` when toggled.
pub fn switch(ui: &mut Ui, on: &mut bool, name: &str) -> Response {
    let p = theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(30.0, 18.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let enabled = ui.is_enabled();
    let value = *on;
    resp.widget_info(|| WidgetInfo::selected(Role::Switch, enabled, value, name));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let fill = mix(p.border_strong, p.accent, t);
        let fill = if enabled { fill } else { fill.gamma_multiply(0.45) };
        ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
        let x = egui::lerp((rect.left() + 9.0)..=(rect.right() - 9.0), t);
        let knob = pos2(x, rect.center().y);
        ui.painter().circle_filled(knob + vec2(0.0, 1.0), 7.0, Color32::from_black_alpha(50));
        ui.painter().circle_filled(knob, 7.0, Color32::WHITE);
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(11),
                Stroke::new(1.5, p.accent),
                StrokeKind::Outside,
            );
        }
    }
    resp
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// A field-like drop-down button (`Auth type ▾`): 30 high (26 when `small`),
/// an optional icon, the value, and a chevron. Open a menu from its response.
pub fn select_box(ui: &mut Ui, icon: Option<&str>, text: &str, width: f32, small: bool) -> Response {
    select_box_ex(ui, icon, None, text, width, small, false)
}

/// [`select_box`] with an optional color dot and a frameless (`quiet`) look.
pub fn select_box_ex(
    ui: &mut Ui,
    icon: Option<&str>,
    dot: Option<Color32>,
    text: &str,
    width: f32,
    small: bool,
    quiet: bool,
) -> Response {
    let p = theme::pal(ui);
    let (h, font, radius, pad) = if small { (26.0, 12.0, RADIUS_SM, 8.0) } else { (30.0, 12.5, RADIUS, 10.0) };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| WidgetInfo::labeled(Role::ComboBox, enabled, text));
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        let painter = ui.painter();
        let r = CornerRadius::same(radius);
        if quiet {
            if hovered {
                painter.rect_filled(rect, r, p.hover);
            }
        } else {
            painter.rect(
                rect,
                r,
                p.surface,
                Stroke::new(1.0, if hovered { p.text_faint } else { p.border_strong }),
                StrokeKind::Inside,
            );
        }
        if resp.has_focus() {
            focus_ring(ui, rect, radius);
        }
        let mut x = rect.left() + pad;
        if let Some(c) = dot {
            painter.circle_filled(pos2(x + 4.0, rect.center().y), 4.0, c);
            x += 14.0;
        }
        if let Some(icon) = icon {
            let g = painter.layout_no_wrap(icon.to_string(), theme::icon_font(font + 2.0), p.text_muted);
            painter.galley(pos2(x, rect.center().y - g.size().y / 2.0), g.clone(), p.text_muted);
            x += g.size().x + 6.0;
        }
        let g = elide(ui, text, FontId::proportional(font), p.text, rect.right() - x - 22.0);
        ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.text);
        ui.painter().text(
            pos2(rect.right() - pad - 4.0, rect.center().y),
            Align2::CENTER_CENTER,
            crate::icons::CARET_DOWN,
            theme::icon_font(12.0),
            p.text_faint,
        );
    }
    resp
}

/// The small search field of sidebars and toolbars (26 high).
pub fn search_field_sm(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    hint_text: &str,
    width: f32,
) -> Response {
    search_field_ex(ui, id, text, hint_text, width, true)
}

fn search_field_ex(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    hint_text: &str,
    width: f32,
    small: bool,
) -> Response {
    let p = theme::pal(ui);
    let empty = text.is_empty();
    let h: f32 = if small { 26.0 } else { 30.0 };
    let font = if small { 12.0 } else { 12.5 };
    let edit = egui::TextEdit::singleline(text)
        .id_salt(id)
        .hint_text(RichText::new(hint_text).color(p.text_faint))
        .font(FontId::proportional(font))
        .desired_width(width)
        .min_size(vec2(0.0, h))
        .align(Align2::LEFT_CENTER)
        // Text after the 30 px padding and the 1 px border; room for the
        // clear button only when there is text to clear.
        .margin(egui::Margin { left: 31, right: if empty { 9 } else { 22 }, top: 0, bottom: 0 });
    let resp = ui
        .scope(|ui| {
            // egui paints a hint in `weak_text_color`, whatever its own color.
            ui.visuals_mut().weak_text_color = Some(p.text_faint);
            ui.spacing_mut().interact_size.y = h;
            let radius = CornerRadius::same(if small { RADIUS_SM } else { RADIUS });
            ui.visuals_mut().widgets.inactive.corner_radius = radius;
            ui.visuals_mut().widgets.hovered.corner_radius = radius;
            ui.visuals_mut().widgets.active.corner_radius = radius;
            ui.add(edit)
        })
        .inner;
    let r = resp.rect;
    ui.painter().text(
        pos2(r.left() + 16.0, r.center().y),
        Align2::CENTER_CENTER,
        crate::icons::MAGNIFYING_GLASS,
        theme::icon_font(14.0),
        p.text_faint,
    );
    if !text.is_empty() {
        let x_rect = Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), Vec2::splat(16.0));
        let x = ui.interact(x_rect, resp.id.with("clear"), Sense::click());
        let color = if x.hovered() { p.text } else { p.text_faint };
        ui.painter().text(
            x_rect.center(),
            Align2::CENTER_CENTER,
            crate::icons::X_CIRCLE,
            theme::icon_font(14.0),
            color,
        );
        if x.clicked() {
            text.clear();
        }
    }
    resp
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// A bordered card (radius 9, 14 × 16 padding).
pub fn card(ui: &Ui) -> egui::Frame {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(egui::Margin::symmetric(16, 14))
}

/// A recessed panel (code, previews): `well` fill, border, radius 8.
pub fn well(ui: &Ui) -> egui::Frame {
    let p = theme::pal(ui);
    egui::Frame::new().fill(p.well).stroke(Stroke::new(1.0, p.border)).corner_radius(CornerRadius::same(8))
}

/// A centered empty state: an icon in a soft square, a title, a line of
/// explanation, then whatever `actions` adds (buttons, shortcut hints).
pub fn empty_state(ui: &mut Ui, icon: &str, title: &str, body: &str, actions: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    // Centered in the space left: measured on the previous frame.
    let id = ui.next_auto_id().with("empty-h");
    let content_h: f32 = ui.data(|d| d.get_temp(id).unwrap_or(150.0));
    let top = ui.cursor().top();
    ui.vertical_centered(|ui| {
        ui.add_space(((ui.available_height() - content_h) / 2.0).clamp(12.0, 400.0));
        let start = ui.cursor().top();
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
        ui.painter().rect(rect, CornerRadius::same(14), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(24.0), p.text_muted);
        ui.add_space(14.0);
        ui.label(RichText::new(title).font(theme::semibold(14.0)).color(p.text));
        if !body.is_empty() {
            ui.add_space(6.0);
            ui.scope(|ui| {
                ui.set_max_width(320.0);
                ui.add(
                    egui::Label::new(RichText::new(body).size(12.5).color(p.text_muted)).halign(Align::Center).wrap(),
                );
            });
        }
        ui.add_space(10.0);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            actions(ui);
        });
        let h = ui.cursor().top() - start;
        if (h - content_h).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(id, h));
            ui.ctx().request_repaint();
        }
    });
    let _ = top;
}

/// A message on the soft fill of a tone with an icon (info, warning, error).
pub fn banner(ui: &mut Ui, tone: Tone, icon: &str, text: &str) -> Response {
    banner_with(ui, tone, icon, text, |_| {})
}

/// [`banner`] with actions on the right.
pub fn banner_with(ui: &mut Ui, tone: Tone, icon: &str, text: &str, actions: impl FnOnce(&mut Ui)) -> Response {
    let p = theme::pal(ui);
    let (fg, bg) = p.tone(tone);
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(RichText::new(icon).font(theme::icon_font(16.0)).color(fg));
                // The actions' width is measured on the previous frame so the
                // text can wrap in the space left of them.
                let wid = ui.scope_id().with(("banner-actions", text));
                let aw: f32 = ui.data(|d| d.get_temp(wid).unwrap_or(0.0));
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - aw - if aw > 0.0 { 10.0 } else { 0.0 }).max(80.0));
                    ui.add(egui::Label::new(RichText::new(text).size(12.5).color(p.text)).wrap());
                });
                let before = ui.cursor().left();
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let r = ui.scope(|ui| actions(ui)).response;
                    let w = if r.rect.width() > 1.0 { ui.max_rect().right() - r.rect.left() } else { 0.0 };
                    let _ = before;
                    if (w - aw).abs() > 0.5 {
                        ui.data_mut(|d| d.insert_temp(wid, w));
                    }
                });
            });
        })
        .response
}

/// An icon on a soft rounded square (radius = side / 4).
pub fn icon_tile(ui: &mut Ui, icon: &str, color: Color32, side: f32) -> Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let bg = if color == p.accent || color == p.accent_text { p.accent_soft } else { theme::soft(color, 0.14) };
    let fg = if color == p.accent { p.accent_text } else { color };
    ui.painter().rect_filled(rect, CornerRadius::same((side / 3.8).round() as u8), bg);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font((side * 0.53).round()), fg);
    resp
}

/// A collapsible section header (caret, title, optional count). Returns
/// whether it is open.
pub fn disclosure(ui: &mut Ui, id: Id, title: &str, count: Option<usize>, default_open: bool) -> bool {
    let p = theme::pal(ui);
    let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(default_open));
    let row = ui.horizontal(|ui| {
        ui.set_height(26.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        let caret = if open { crate::icons::CARET_DOWN } else { crate::icons::CARET_RIGHT };
        ui.add(
            egui::Label::new(RichText::new(caret).font(theme::icon_font(12.0)).color(p.text_faint)).selectable(false),
        );
        ui.add(egui::Label::new(section_title(title)).selectable(false));
        if let Some(n) = count {
            tone_pill(ui, &n.to_string(), Tone::Neutral);
        }
    });
    let r = ui.interact(row.response.rect, id.with("toggle"), Sense::click());
    r.widget_info(|| WidgetInfo::labeled(Role::Button, true, title));
    r.ctx.accesskit_node_builder(r.id, |n| n.set_expanded(open));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    open
}

/// A label above its control (12 medium), with an optional help line under it.
pub fn field<R>(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        // Line boxes of 1.45 as in the prototype (`.field`).
        ui.label(RichText::new(label).font(theme::medium(12.0)).color(p.text).line_height(Some(12.0 * 1.45)));
        let r = add(ui);
        if !help.is_empty() {
            ui.label(RichText::new(help).size(11.5).color(p.text_muted).line_height(Some(11.5 * 1.45)));
        }
        r
    })
    .inner
}

/// Rows of a settings [`group`]: a hairline between rows.
pub struct GroupRows {
    first: bool,
}

impl GroupRows {
    /// A settings row: label (13 medium) and help (12 muted) on the left, the
    /// control on the right; 11 px above and below.
    pub fn row<R>(&mut self, ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        let p = theme::pal(ui);
        self.separator(ui);
        ui.add_space(11.0);
        let out = ui
            .horizontal(|ui| {
                let w = ui.available_width();
                let label_w = (w * 0.55).max(120.0);
                ui.allocate_ui_with_layout(vec2(label_w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(label_w);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new(label).font(theme::medium(13.0)).color(p.text).line_height(Some(13.0 * 1.45)),
                    );
                    if !help.is_empty() {
                        let help = RichText::new(help).size(12.0).color(p.text_muted).line_height(Some(12.0 * 1.45));
                        ui.add(egui::Label::new(help).wrap());
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| add(ui)).inner
            })
            .inner;
        ui.add_space(11.0);
        out
    }

    /// A free-form row (lists, editors) with the same separators.
    pub fn block<R>(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        self.separator(ui);
        ui.add_space(11.0);
        let out = add(ui);
        ui.add_space(11.0);
        out
    }

    /// The 1 px border on top of every row but the first (`.frow +
    /// .frow`), which adds to the row's height.
    fn separator(&mut self, ui: &mut Ui) {
        if !std::mem::replace(&mut self.first, false) {
            let p = theme::pal(ui);
            let y = ui.cursor().top();
            let r = ui.max_rect();
            ui.painter().hline(r.left()..=r.right(), y + 0.5, Stroke::new(1.0, p.border));
            ui.add_space(1.0);
        }
    }
}

/// A titled settings group: the title (12.5 semibold) and optional help above
/// a bordered card (radius 10, 14 px side padding) of rows.
pub fn group(ui: &mut Ui, title: &str, help: &str, add: impl FnOnce(&mut Ui, &mut GroupRows)) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        // Line boxes of 1.45 as in the prototype.
        if !title.is_empty() {
            ui.label(section_title(title).color(p.text).line_height(Some(12.5 * 1.45)));
        }
        if !help.is_empty() {
            ui.add(egui::Label::new(hint(ui, help).line_height(Some(12.0 * 1.45))).wrap());
        }
        egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui, &mut GroupRows { first: true });
            });
    });
}

// ---------------------------------------------------------------------------
// Layout: bars, panes
// ---------------------------------------------------------------------------

/// A full-width row of fixed `height`: content laid out left to right,
/// vertically centered, 12 px side padding, and (`hairline`) a 1 px border
/// along its bottom across the whole width.
pub fn bar<R>(ui: &mut Ui, height: f32, hairline: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    bar_ex(ui, height, size::PAD, hairline, add)
}

/// [`bar`] with custom side padding.
pub fn bar_ex<R>(ui: &mut Ui, height: f32, pad: f32, hairline: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::pal(ui);
    let full = ui.available_rect_before_wrap();
    let rect = Rect::from_min_size(full.min, vec2(full.width(), height));
    let inner = rect.shrink2(vec2(pad, 0.0));
    let r = ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.set_min_size(inner.size());
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui)
    });
    if hairline {
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    }
    ui.advance_cursor_after_rect(rect);
    r.inner
}

/// The 40 px toolbar above a pane's content (no hairline).
pub fn toolbar<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    bar(ui, size::TOOLBAR, false, add)
}

/// Content of a pane under its bars: scrolls vertically, padding 0 12 16.
pub fn pane_body<R>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 16 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 14.0;
                    add(ui)
                })
                .inner
        })
        .inner
}

/// Two panes side by side (`side`) or stacked, with a 9 px gutter that can be
/// dragged (double-click resets to `default`). The split is kept per `id`.
/// Each pane gets a clipped `Ui` filling its rect, laid out top-down with no
/// padding: panes draw their own bars.
pub fn panes(ui: &mut Ui, id: Id, side: bool, default: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    let rect = ui.available_rect_before_wrap();
    let key = id.with(if side { "split-side" } else { "split-stack" });
    let mut split: f32 = ui.data_mut(|d| *d.get_persisted_mut_or(key, default));
    let g = size::GUTTER;
    let (len, min_a, min_b): (f32, f32, f32) =
        if side { (rect.width() - g, 340.0, 300.0) } else { (rect.height() - g, 120.0, 140.0) };
    let a = (len * split).clamp(min_a.min(len * 0.5), (len - min_b).max(len * 0.5));
    let (ra, rg, rb) = if side {
        (
            Rect::from_min_size(rect.min, vec2(a, rect.height())),
            Rect::from_min_size(pos2(rect.left() + a, rect.top()), vec2(g, rect.height())),
            Rect::from_min_max(pos2(rect.left() + a + g, rect.top()), rect.max),
        )
    } else {
        (
            Rect::from_min_size(rect.min, vec2(rect.width(), a)),
            Rect::from_min_size(pos2(rect.left(), rect.top() + a), vec2(rect.width(), g)),
            Rect::from_min_max(pos2(rect.left(), rect.top() + a + g), rect.max),
        )
    };
    let pane = |ui: &mut Ui, r: Rect, add: Box<dyn FnOnce(&mut Ui) + '_>, salt: &str| {
        ui.scope_builder(UiBuilder::new().max_rect(r).id_salt((id, salt)).layout(Layout::top_down(Align::Min)), |ui| {
            ui.set_clip_rect(r.intersect(ui.clip_rect()));
            ui.set_min_size(r.size());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    };
    pane(ui, ra, Box::new(left), "a");
    pane(ui, rb, Box::new(right), "b");
    // Gutter: a hairline in its middle and a grip on hover.
    let resp = ui.interact(rg, id.with("gutter"), Sense::click_and_drag());
    let painter = ui.painter();
    if side {
        painter.vline(rg.center().x, rg.y_range(), Stroke::new(1.0, p.border));
    } else {
        painter.hline(rg.x_range(), rg.center().y, Stroke::new(1.0, p.border));
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(if side {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        });
        let grip = if side {
            Rect::from_center_size(rg.center(), vec2(5.0, 28.0))
        } else {
            Rect::from_center_size(rg.center(), vec2(28.0, 5.0))
        };
        painter.rect_filled(grip, CornerRadius::same(3), p.border_strong);
    }
    if resp.dragged()
        && let Some(pos) = ui.ctx().pointer_interact_pos()
    {
        let at = if side { pos.x - rect.left() - g / 2.0 } else { pos.y - rect.top() - g / 2.0 };
        split = (at / len).clamp(0.15, 0.85);
    }
    if resp.double_clicked() {
        split = default;
    }
    ui.data_mut(|d| d.insert_persisted(key, split));
    ui.advance_cursor_after_rect(rect);
}

// ---------------------------------------------------------------------------
// Menus
// ---------------------------------------------------------------------------

/// One item of a popup menu: 30 high, an icon (15, muted) in a 16 px column,
/// the label, and on the right a shortcut, a detail text or a check.
#[derive(Clone, Copy, Default)]
pub struct MenuItem<'a> {
    pub icon: Option<&'a str>,
    /// A method label in place of the icon.
    pub method: Option<&'a str>,
    /// A color dot in place of the icon (environments).
    pub dot: Option<Color32>,
    /// Some(true) draws a check in the icon column; Some(false) leaves it empty.
    pub checked: Option<bool>,
    pub label: &'a str,
    pub shortcut: Option<&'a str>,
    /// Muted text on the right (counts, paths).
    pub detail: Option<&'a str>,
    pub danger: bool,
    /// A chevron on the right: opens a submenu.
    pub submenu: bool,
    pub selected: bool,
}

/// A menu item with an icon and an optional shortcut.
pub fn menu_item(ui: &mut Ui, icon: Option<&str>, label: &str, shortcut: Option<&str>) -> Response {
    menu_item_ex(ui, MenuItem { icon, label, shortcut, ..Default::default() })
}

pub fn menu_item_ex(ui: &mut Ui, m: MenuItem<'_>) -> Response {
    let p = theme::pal(ui);
    let width = ui.available_width().max(ui.spacing().menu_width - 10.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| WidgetInfo::labeled(Role::MenuItem, enabled, m.label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if (resp.hovered() && enabled) || m.selected {
            painter.rect_filled(rect, CornerRadius::same(6), p.hover);
        }
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        let text_c = fade(if m.danger { p.err } else { p.text });
        let icon_c = fade(if m.danger { p.err } else { p.text_muted });
        let mut x = rect.left() + 9.0;
        let cy = rect.center().y;
        let lead = m.icon.is_some() || m.method.is_some() || m.dot.is_some() || m.checked.is_some();
        if let Some(i) = m.icon {
            painter.text(pos2(x + 8.0, cy), Align2::CENTER_CENTER, i, theme::icon_font(15.0), icon_c);
        } else if let Some(meth) = m.method {
            let g = painter.layout_no_wrap(
                crate::widgets::method_short(meth).to_string(),
                theme::semibold(10.0),
                theme::method_color(meth, ui.visuals().dark_mode),
            );
            painter.galley(pos2(x, cy - g.size().y / 2.0), g, Color32::PLACEHOLDER);
        } else if let Some(c) = m.dot {
            painter.circle_filled(pos2(x + 8.0, cy), 4.0, c);
        } else if m.checked == Some(true) {
            painter.text(
                pos2(x + 8.0, cy),
                Align2::CENTER_CENTER,
                crate::icons::CHECK,
                theme::icon_font(14.0),
                p.accent_text,
            );
        }
        if lead {
            x += if m.method.is_some() { 34.0 } else { 16.0 + 9.0 };
        }
        let mut right = rect.right() - 9.0;
        if m.submenu {
            painter.text(
                pos2(right - 6.0, cy),
                Align2::CENTER_CENTER,
                crate::icons::CARET_RIGHT,
                theme::icon_font(12.0),
                p.text_faint,
            );
            right -= 18.0;
        }
        if let Some(k) = m.shortcut {
            let s = kbd_size(ui, k);
            paint_kbd(ui, Rect::from_min_size(pos2(right - s.x, cy - s.y / 2.0), s), k);
            right -= s.x + 8.0;
        }
        if let Some(d) = m.detail {
            let g = ui.painter().layout_no_wrap(d.to_string(), FontId::proportional(11.5), p.text_faint);
            ui.painter().galley(pos2(right - g.size().x, cy - g.size().y / 2.0), g.clone(), p.text_faint);
            right -= g.size().x + 8.0;
        }
        let g = elide(ui, m.label, FontId::proportional(12.5), text_c, right - x);
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, text_c);
    }
    if resp.clicked() && !m.submenu {
        ui.close();
    }
    resp
}

/// A group header in a menu (`REQUESTS`).
pub fn menu_header(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 9.0, rect.bottom() - 4.0),
        Align2::LEFT_BOTTOM,
        text.to_uppercase(),
        theme::semibold(10.5),
        p.text_faint,
    );
}

/// A hairline between menu groups.
pub fn menu_sep(ui: &mut Ui) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range().shrink(2.0), rect.center().y, Stroke::new(1.0, p.border));
}

// ---------------------------------------------------------------------------
// Sidebar rows
// ---------------------------------------------------------------------------

/// One row of a sidebar list or tree (28 high).
#[derive(Clone, Copy, Default)]
pub struct TreeRow<'a> {
    /// Nesting level: 14 px indent each, with a guide line per level.
    pub depth: usize,
    /// Some(open): a caret column (folders, collections); None: an empty
    /// caret column when `indent_caret`, else nothing.
    pub caret: Option<bool>,
    pub indent_caret: bool,
    pub icon: Option<&'a str>,
    pub icon_color: Option<Color32>,
    /// Spins the icon (a running flow).
    pub spinning: bool,
    /// A method label (right-aligned in a 38 px column).
    pub method: Option<&'a str>,
    pub label: &'a str,
    /// Monospace label (history URLs).
    pub mono: bool,
    /// Semibold label (collections).
    pub strong: bool,
    pub faint: bool,
    /// Muted text on the right, hidden while hover actions show.
    pub meta: Option<&'a str>,
    pub meta_color: Option<Color32>,
    /// An accent dot after the label (unsaved).
    pub unsaved: bool,
    pub selected: bool,
    /// Width of the hover actions on the right (0: none).
    pub actions_width: f32,
    /// Show the actions even when not hovered (their menu is open).
    pub force_actions: bool,
    /// Extra left padding: flat lists use 4 (text 8 px in, like the
    /// prototype's `padding-left: 8px`); trees leave it 0.
    pub pad_left: f32,
}

/// Paints a sidebar row and returns its response plus the rect where hover
/// actions go (when the row is hovered and `actions_width` > 0). Pills after
/// the label are drawn by the caller with `trailing`.
pub fn tree_row(ui: &mut Ui, row: TreeRow<'_>, trailing: impl FnOnce(&mut Ui)) -> (Response, Option<Rect>) {
    let p = theme::pal(ui);
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, size::TREE_ROW), Sense::click_and_drag());
    let hovered = ui.rect_contains_pointer(rect);
    let mut actions = None;
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // Guides of the levels above.
        for d in 0..row.depth {
            let x = rect.left() + 4.0 + d as f32 * 14.0 + 8.0;
            painter.vline(x, rect.y_range(), Stroke::new(1.0, p.border));
        }
        if row.selected {
            let shadow = egui::Shadow {
                offset: [0, 1],
                blur: 2,
                spread: 0,
                color: Color32::from_black_alpha(if ui.visuals().dark_mode { 0 } else { 10 }),
            };
            painter.add(shadow.as_shape(rect, CornerRadius::same(6)));
            painter.rect(rect, CornerRadius::same(6), p.surface, Stroke::new(1.0, p.border), StrokeKind::Inside);
            painter.rect_filled(
                Rect::from_min_size(pos2(rect.left() - 2.0, rect.top() + 7.0), vec2(3.0, rect.height() - 14.0)),
                CornerRadius::same(2),
                p.accent,
            );
        } else if hovered {
            painter.rect_filled(rect, CornerRadius::same(6), p.hover);
        }
        let cy = rect.center().y;
        let mut x = rect.left() + 4.0 + row.pad_left + row.depth as f32 * 14.0;
        if let Some(open) = row.caret {
            let c = if open { crate::icons::CARET_DOWN } else { crate::icons::CARET_RIGHT };
            painter.text(pos2(x + 8.0, cy), Align2::CENTER_CENTER, c, theme::icon_font(11.0), p.text_faint);
            x += 16.0 + 6.0;
        } else if row.indent_caret {
            x += 16.0 + 6.0;
        }
        if let Some(icon) = row.icon {
            let c = row.icon_color.unwrap_or(p.text_muted);
            if row.spinning {
                let t = ui.input(|i| i.time) as f32;
                let center = pos2(x + 7.5, cy);
                let mut shape = egui::epaint::TextShape::new(
                    pos2(0.0, 0.0),
                    painter.layout_no_wrap(icon.to_string(), theme::icon_font(15.0), c),
                    c,
                );
                let half = shape.galley.size() / 2.0;
                let rot = egui::emath::Rot2::from_angle(t * 6.0);
                shape.pos = center - rot * half;
                shape.angle = t * 6.0;
                painter.add(shape);
                ui.ctx().request_repaint();
            } else {
                painter.text(pos2(x, cy), Align2::LEFT_CENTER, icon, theme::icon_font(15.0), c);
            }
            x += 15.0 + 6.0;
        }
        if let Some(m) = row.method {
            let g = painter.layout_no_wrap(
                crate::widgets::method_short(m).to_string(),
                theme::semibold(10.0),
                theme::method_color(m, ui.visuals().dark_mode),
            );
            painter.galley(pos2(x + 38.0 - g.size().x, cy - g.size().y / 2.0), g, Color32::PLACEHOLDER);
            x += 38.0 + 6.0;
        }
        let show_actions = (hovered || row.force_actions) && row.actions_width > 0.0;
        let mut right = rect.right() - 6.0;
        if show_actions {
            actions = Some(Rect::from_min_max(pos2(right - row.actions_width, rect.top()), pos2(right, rect.bottom())));
            right -= row.actions_width + 4.0;
        } else if let Some(meta) = row.meta {
            let g = painter.layout_no_wrap(
                meta.to_string(),
                FontId::proportional(11.0),
                row.meta_color.unwrap_or(p.text_faint),
            );
            painter.galley(pos2(right - g.size().x, cy - g.size().y / 2.0), g.clone(), Color32::PLACEHOLDER);
            right -= g.size().x + 8.0;
        }
        // Trailing pills (fork, Active, live server) right after the label.
        let trailing_id = ui.scope_id().with(("tree-trailing", row.label, rect.top() as i32));
        let tw: f32 = ui.data(|d| d.get_temp(trailing_id).unwrap_or(0.0));
        let font = if row.strong {
            theme::semibold(12.5)
        } else if row.mono {
            theme::mono(11.5)
        } else {
            FontId::proportional(12.5)
        };
        let color = if row.faint { p.text_faint } else { p.text };
        let g = elide(
            ui,
            row.label,
            font,
            color,
            (right - x - tw - if tw > 0.0 { 6.0 } else { 0.0 } - if row.unsaved { 12.0 } else { 0.0 }).max(20.0),
        );
        let label_end = x + g.size().x;
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, color);
        let mut after = label_end;
        if row.unsaved {
            ui.painter().circle_filled(pos2(after + 7.0, cy), 3.0, p.accent);
            after += 12.0;
        }
        let tr = Rect::from_min_max(pos2(after + 6.0, rect.top()), pos2(right.max(after + 6.0), rect.bottom()));
        // A child that doesn't move the list's cursor (the row is already allocated).
        // Pills after the label sit at the right end, before the meta (the
        // label takes the free space, like the prototype's `.nm { flex: 1 }`).
        let mut child = ui.new_child(UiBuilder::new().max_rect(tr).layout(Layout::right_to_left(Align::Center)));
        child.spacing_mut().item_spacing.x = 6.0;
        trailing(&mut child);
        let used = if child.min_rect().width() > 0.5 { tr.right() - child.min_rect().left() } else { 0.0 };
        if (used - tw).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(trailing_id, used.max(0.0)));
        }
    }
    (resp, actions)
}

/// A group header in a sidebar list (`TODAY`, `ENVIRONMENTS`).
pub fn sidebar_group(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.bottom() - 6.0),
        Align2::LEFT_BOTTOM,
        text.to_uppercase(),
        theme::semibold(10.5),
        p.text_faint,
    );
}

/// `ScrollArea::show_rows` for rows of exactly `row_h` laid out edge to edge.
/// egui places the rows `row_h + item_spacing.y` apart, so the spacing is
/// zeroed around the area too: otherwise the rows slip by that much each
/// time one scrolls out of view and the bottom of the list is left empty.
pub fn show_rows<R>(
    area: egui::ScrollArea,
    ui: &mut Ui,
    row_h: f32,
    total: usize,
    add: impl FnOnce(&mut Ui, std::ops::Range<usize>) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        area.show_rows(ui, row_h, total, add)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows drawn by `show_rows` (top, bottom) and the list's visible rect.
    fn drawn_rows(offset: Option<f32>) -> (Vec<(f32, f32)>, Rect) {
        let ctx = egui::Context::default();
        let mut rows = Vec::new();
        let mut inner = Rect::NOTHING;
        // The third frame knows the content size and has scrolled.
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, 400.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let mut area = egui::ScrollArea::vertical().auto_shrink([false; 2]);
                    area = match offset {
                        Some(y) => area.vertical_scroll_offset(y),
                        None => area.stick_to_bottom(true),
                    };
                    rows.clear();
                    inner = show_rows(area, ui, 28.0, 100, |ui, range| {
                        // As the callers do.
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for _ in range {
                            let (rect, _) = ui.allocate_exact_size(vec2(100.0, 28.0), Sense::hover());
                            rows.push((rect.top(), rect.bottom()));
                        }
                    })
                    .inner_rect;
                });
            });
            out.drop_without_applying_deltas();
        }
        (rows, inner)
    }

    /// The rows in view fill the list, down to the last one at its bottom.
    #[test]
    fn rows_fill_the_list_down_to_the_bottom() {
        for offset in [Some(0.0), Some(500.0), Some(1234.0), None] {
            let (rows, inner) = drawn_rows(offset);
            let (top, bottom) = (rows.first().unwrap().0, rows.last().unwrap().1);
            assert!(top <= inner.top() + 0.5, "{offset:?}: first row at {top}, list from {}", inner.top());
            assert!(bottom >= inner.bottom() - 0.5, "{offset:?}: last row ends {bottom}, list to {}", inner.bottom());
            assert!(rows.windows(2).all(|w| w[0].1 == w[1].0), "{offset:?}: gaps between rows");
        }
        // At the end of the list its last row ends exactly at the bottom.
        let (rows, inner) = drawn_rows(None);
        assert!(
            (rows.last().unwrap().1 - inner.bottom()).abs() < 0.5,
            "{} vs {}",
            rows.last().unwrap().1,
            inner.bottom()
        );
    }
}
