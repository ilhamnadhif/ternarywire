//! Shared pieces of the protocol tabs (WebSocket, MQTT, Socket.IO, gRPC, MCP,
//! AI) and the local server tabs (webhook, mock, capture): where the request
//! lives, the header and connection bar, status pills, variable resolution
//! and the message log.

use std::collections::VecDeque;
use std::sync::Arc;

use egui::{
    Align, Align2, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use ternarywire_core::model::RequestItem;
use ternarywire_core::variables::{Resolver, VariableScopes};

use crate::app::{ItemRef, TernaryWireApp};
use crate::i18n::{tr, trf};
use crate::kit::{Tone, size};
use crate::{icons, kit, theme, widgets};

/// A request either saved in a collection or kept in the tab (unsaved).
#[derive(Default)]
pub struct ItemSlot {
    pub location: Option<ItemRef>,
    pub scratch: Option<RequestItem>,
}

impl ItemSlot {
    pub fn load(&self, app: &TernaryWireApp) -> Option<RequestItem> {
        match &self.location {
            Some(loc) => app.ws.data.collection(&loc.collection)?.find_request(&loc.item).cloned(),
            None => self.scratch.clone(),
        }
    }

    /// Writes `item` back if it differs from `original`.
    pub fn store(&mut self, app: &mut TernaryWireApp, original: &RequestItem, item: RequestItem) {
        if &item == original {
            return;
        }
        match &self.location {
            Some(loc) => {
                let cid = loc.collection.clone();
                if let Some(target) =
                    app.ws.data.collection_mut(&loc.collection).and_then(|c| c.find_request_mut(&loc.item))
                {
                    *target = item;
                    app.ws.mark_collection(&cid);
                }
            }
            None => self.scratch = Some(item),
        }
    }

    pub fn collection_id(&self) -> Option<String> {
        self.location.as_ref().map(|l| l.collection.clone())
    }
}

// ---------------------------------------------------------------------------
// The view grammar: title row, two panes, connect row, status row
// ---------------------------------------------------------------------------
//
// Every protocol view is laid out like the HTTP request: a 46 px title row,
// then two panes (side by side when wide enough, else stacked). The compose
// pane starts with the 56 px connect row, then 36 px tabs, a 40 px toolbar
// and its content; the traffic pane with the 56 px status row, then 36 px
// tabs or the log bar, then the log or the result.

/// Color of a protocol badge (`WS`, `SIO`, `MQTT`, `gRPC`, `MCP`, `AI`).
pub fn proto_color(badge: &str, dark: bool) -> Color32 {
    theme::method_color(badge, dark)
}

/// State of a connection, a call or a local server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conn {
    /// Not connected / stopped.
    Idle,
    /// Connecting, starting, running a call.
    Busy,
    /// Connected / listening.
    Live,
    /// The last attempt failed.
    Failed,
}

/// A status pill (24 high): `● Connected` with a halo while live, a dot when
/// failed, a spinner while busy and a ring when idle.
pub fn status_pill(ui: &mut Ui, state: Conn, text: &str) -> egui::Response {
    conn_pill(ui, state, text, true)
}

/// [`status_pill`], 20 high unless `large` (pills of a title row).
pub fn conn_pill(ui: &mut Ui, state: Conn, text: &str, large: bool) -> egui::Response {
    let p = theme::pal(ui);
    let (tone, opts) = match state {
        Conn::Idle => (Tone::Neutral, kit::PillOpts { large, ..Default::default() }),
        Conn::Busy => (Tone::Info, kit::PillOpts { large, ..Default::default() }),
        Conn::Live => (Tone::Ok, kit::PillOpts { large, dot: true, live: true, ..Default::default() }),
        Conn::Failed => (Tone::Err, kit::PillOpts { large, dot: true, ..Default::default() }),
    };
    if matches!(state, Conn::Live | Conn::Failed) {
        let r = kit::pill_ex(ui, text, tone, opts);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
        return r;
    }
    // Idle and busy draw their own lead (a ring, a spinner).
    let (fg, bg) = p.tone(tone);
    let (h, pad, font) = if large { (24.0, 9.0, 12.0) } else { (20.0, 7.0, 11.5) };
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::medium(font), fg);
    let lead = if state == Conn::Busy { 13.0 } else { 8.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(pad * 2.0 + lead + 5.0 + galley.size().x, h), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same((h / 2.0) as u8), bg);
        let c = pos2(rect.left() + pad + lead / 2.0, rect.center().y);
        if state == Conn::Busy {
            let t = ui.input(|i| i.time) as f32;
            let start = t * std::f32::consts::TAU * 0.9;
            let points: Vec<egui::Pos2> = (0..=16)
                .map(|i| {
                    let a = start + i as f32 / 16.0 * std::f32::consts::TAU * 0.72;
                    c + 4.5 * vec2(a.cos(), a.sin())
                })
                .collect();
            painter.line(points, Stroke::new(1.5, fg));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        } else {
            painter.circle_stroke(c, 3.25, Stroke::new(1.5, fg));
        }
        painter.galley(pos2(rect.left() + pad + lead + 5.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    }
    resp
}

/// `12 s`, `4:05`, `1:02:03`: how long a connection has been open.
pub fn uptime(since: std::time::Instant) -> String {
    let s = since.elapsed().as_secs();
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{}:{:02}", s / 60, s % 60)
    } else {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    }
}

/// The text of a connection's state, with how long it has been open
/// (redrawn every second while connected).
pub fn conn_text(ui: &Ui, state: Conn, since: Option<std::time::Instant>) -> String {
    match state {
        Conn::Idle => tr("Disconnected").to_string(),
        Conn::Busy => tr("Connecting…").to_string(),
        Conn::Live => match since {
            Some(s) => {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                format!("{} · {}", tr("Connected"), uptime(s))
            }
            None => tr("Connected").to_string(),
        },
        Conn::Failed => tr("Connection failed").to_string(),
    }
}

/// Draws in the rect of `builder` (already allocated, or laid over other
/// widgets) without moving the parent's cursor, clipped to that rect.
pub fn inside<R>(ui: &mut Ui, builder: UiBuilder, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(builder);
    child.set_clip_rect(child.max_rect().intersect(ui.clip_rect()));
    add(&mut child)
}

/// A full-width row of `height` with `left`/`right` padding: content laid out
/// left to right and centered vertically, a hairline along its bottom.
pub fn row_bar<R>(
    ui: &mut Ui,
    height: f32,
    left: f32,
    right: f32,
    hairline: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = theme::pal(ui);
    let full = ui.available_rect_before_wrap();
    let rect = Rect::from_min_size(full.min, vec2(full.width(), height));
    let inner = Rect::from_min_max(pos2(rect.left() + left, rect.top()), pos2(rect.right() - right, rect.bottom()));
    let r = inside(ui, UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.set_min_size(inner.size());
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui)
    });
    if hairline {
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    }
    ui.advance_cursor_after_rect(rect);
    r
}

/// What the More actions menu of a title row asks for.
enum More {
    SaveAs,
    Duplicate,
    Delete,
}

/// The title row of a protocol view (46): breadcrumb, name (double-click to
/// rename), `pills`, Save and More actions, like the HTTP request.
pub fn title_row(
    ui: &mut Ui,
    app: &mut TernaryWireApp,
    tab_id: u64,
    slot: &ItemSlot,
    item: &mut RequestItem,
    pills: impl FnOnce(&mut Ui),
) {
    let p = theme::pal(ui);
    let crumbs: Vec<String> = match &slot.location {
        Some(loc) => app
            .ws
            .data
            .collection(&loc.collection)
            .and_then(|c| c.ancestors(&loc.item))
            .map(|chain| chain.iter().map(|a| a.name().to_string()).collect())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let mut save = false;
    let mut more = None;
    row_bar(ui, size::VIEW_TITLE, 16.0, 12.0, true, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = kit::icon_button(ui, icons::DOTS_THREE, tr("More actions"));
            egui::Popup::menu(&r).width(210.0).show(|ui| {
                if slot.location.is_none() {
                    if kit::menu_item(ui, Some(icons::FLOPPY_DISK), tr("Save as…"), Some("⌘S")).clicked() {
                        more = Some(More::SaveAs);
                    }
                } else {
                    if kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked() {
                        more = Some(More::Duplicate);
                    }
                    kit::menu_sep(ui);
                    let del = kit::MenuItem {
                        icon: Some(icons::TRASH),
                        label: tr("Delete…"),
                        danger: true,
                        ..Default::default()
                    };
                    if kit::menu_item_ex(ui, del).clicked() {
                        more = Some(More::Delete);
                    }
                }
            });
            let label = if slot.location.is_none() { tr("Save…") } else { tr("Save") };
            let opts = kit::ButtonOpts { icon: Some(icons::FLOPPY_DISK), ..Default::default() };
            if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, label, opts)
                .on_hover_text(tr("Save (⌘S)"))
                .clicked()
            {
                save = true;
            }
            ui.add_space(4.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let path = crumbs.join(" › ");
                if slot.location.is_none() {
                    kit::pill_ex(
                        ui,
                        tr("Unsaved"),
                        Tone::Neutral,
                        kit::PillOpts { outline: true, ..Default::default() },
                    );
                    ui.add_space(2.0);
                } else if ui.available_width() > 560.0 {
                    for part in &crumbs {
                        ui.add(
                            egui::Label::new(RichText::new(part.as_str()).size(12.5).color(p.text_muted))
                                .selectable(false),
                        );
                        ui.label(RichText::new(icons::CARET_RIGHT).font(theme::icon_font(11.0)).color(p.text_faint));
                    }
                }
                // The pills keep their room (measured on the previous frame).
                let pills_id = ui.scope_id().with("title-pills");
                let pills_w: f32 = ui.data(|d| d.get_temp(pills_id).unwrap_or(0.0));
                let rename_id = egui::Id::unique(("proto-rename", tab_id));
                let renaming: Option<String> = ui.data(|d| d.get_temp(rename_id));
                match renaming {
                    Some(mut buf) => {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut buf)
                                .desired_width(260.0)
                                .font(theme::semibold(14.0))
                                .margin(egui::Margin::symmetric(6, 3)),
                        );
                        r.request_focus();
                        if r.lost_focus() {
                            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                            if !cancelled && !buf.trim().is_empty() {
                                item.name = buf.trim().to_string();
                            }
                            ui.data_mut(|d| d.remove::<String>(rename_id));
                        } else {
                            ui.data_mut(|d| d.insert_temp(rename_id, buf));
                        }
                    }
                    None => {
                        let max = (ui.available_width() - pills_w - 8.0).max(60.0);
                        let g = kit::elide(ui, &item.name, theme::semibold(14.0), p.text, max);
                        let (rect, r) = ui.allocate_exact_size(vec2(g.size().x, 22.0), Sense::click());
                        ui.painter().galley(pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, p.text);
                        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &item.name));
                        let hint = if path.is_empty() {
                            tr("Double-click to rename").to_string()
                        } else {
                            format!("{path} › {}\n{}", item.name, tr("Double-click to rename"))
                        };
                        if r.on_hover_text(hint).double_clicked() {
                            ui.data_mut(|d| d.insert_temp(rename_id, item.name.clone()));
                        }
                    }
                }
                // `.crumbs` gap 4 and the pills' 8 px margin.
                ui.add_space(8.0);
                let start = ui.cursor().left();
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    pills(ui);
                });
                let used = ui.cursor().left() - start;
                if (used - pills_w).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(pills_id, used));
                }
            });
        });
    });
    if save {
        if slot.location.is_none() {
            app.dialog = Some(crate::dialogs::Dialog::save_as(tab_id, &app.ws));
        } else {
            app.ws.flush();
            app.toast(tr("Saved"), false);
        }
    }
    match (more, &slot.location) {
        (Some(More::SaveAs), _) => app.dialog = Some(crate::dialogs::Dialog::save_as(tab_id, &app.ws)),
        (Some(More::Duplicate), Some(loc)) => {
            if let Some(c) = app.ws.data.collection_mut(&loc.collection)
                && let Some(found) = c.find(&loc.item).cloned()
            {
                let parent = c.parent_of(&loc.item).map(str::to_string);
                let mut copy = found.duplicate();
                copy.set_name(trf("{} Copy", &[&found.name()]));
                let idx = c
                    .children_mut(parent.as_deref())
                    .and_then(|v| v.iter().position(|i| i.id() == loc.item))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                c.insert(parent.as_deref(), idx, copy);
                app.ws.mark_collection(&loc.collection);
            }
        }
        (Some(More::Delete), Some(loc)) => {
            app.dialog = Some(crate::dialogs::Dialog::confirm_delete(
                loc.collection.clone(),
                Some(loc.item.clone()),
                item.name.clone(),
            ));
        }
        _ => {}
    }
}

/// The title row ([`title_row`]); `status` widgets go after the name.
pub fn header(
    ui: &mut Ui,
    app: &mut TernaryWireApp,
    tab_id: u64,
    slot: &ItemSlot,
    item: &mut RequestItem,
    status: impl FnOnce(&mut Ui),
) {
    title_row(ui, app, tab_id, slot, item, status);
}

/// The 56 px connect row of a compose pane: the grouped bar
/// `[badge | fields…]` (38 high, like the URL bar of HTTP requests) and the
/// pane's action (38 high) after it. `fields` returns whether one of its
/// inputs has focus (the bar then shows a focus ring); `button` draws the
/// action and its result is returned. `button_w` is the room kept for the
/// action until it has been measured.
pub fn connect_bar<B>(
    ui: &mut Ui,
    badge: &str,
    button_w: f32,
    fields: impl FnOnce(&mut Ui) -> bool,
    button: impl FnOnce(&mut Ui) -> B,
) -> B {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let width_id = ui.scope_id().with(("connect-button-w", badge));
    let bw: f32 = ui.data(|d| d.get_temp(width_id).unwrap_or(button_w));
    kit::bar(ui, size::ACTION_ROW, true, |ui| {
        let bar_w = (ui.available_width() - bw - 8.0).max(160.0);
        let (rect, _) = ui.allocate_exact_size(vec2(bar_w, 38.0), Sense::hover());
        let bg = ui.painter().add(egui::Shape::Noop);
        let inner = rect.shrink(1.0);
        let focused = inside(ui, UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let color = proto_color(badge, dark);
            let g = ui.painter().layout_no_wrap(badge.to_string(), theme::semibold(11.0), color);
            let (b, _) = ui.allocate_exact_size(vec2(g.size().x + 24.0, inner.height()), Sense::hover());
            ui.painter().galley(b.center() - g.size() / 2.0, g, color);
            bar_divider(ui);
            fields(ui)
        });
        ui.painter().set(bg, egui::Shape::rect_filled(rect, CornerRadius::same(theme::RADIUS_CARD), p.surface));
        if focused {
            kit::focus_ring(ui, rect, theme::RADIUS_CARD);
        } else {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(theme::RADIUS_CARD),
                Stroke::new(1.0, p.border_strong),
                StrokeKind::Inside,
            );
        }
        let r = ui.scope(button);
        let w = r.response.rect.width();
        if w > 1.0 && (w - bw).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(width_id, w));
            ui.ctx().request_repaint();
        }
        r.inner
    })
}

/// A hairline between the cells of a [`connect_bar`], its full height.
pub fn bar_divider(ui: &mut Ui) {
    let p = theme::pal(ui);
    let h = ui.available_height();
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, h), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, p.border));
}

/// A field of a [`connect_bar`] with `{{variables}}` (an address, a model):
/// monospace, 12 px side padding, `width` wide.
pub fn bar_var(
    ui: &mut Ui,
    id: &str,
    text: &mut String,
    vars: &widgets::VarCtx,
    hint: &str,
    width: f32,
) -> egui::Response {
    ui.add_space(3.0);
    let r = widgets::var_cell(ui, id, text, vars, hint, (width - 6.0).max(40.0));
    ui.add_space(3.0);
    r
}

/// A drop-down cell of a [`connect_bar`] (`HTTP ▾`, a provider, a method):
/// `add` paints its content from the left edge and returns the content
/// width; open a menu from the response.
pub fn bar_combo(ui: &mut Ui, name: &str, width: f32, add: impl FnOnce(&Ui, Rect)) -> egui::Response {
    let p = theme::pal(ui);
    let h = ui.available_height();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, ui.is_enabled(), name));
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, p.hover);
        }
        let content =
            Rect::from_min_max(pos2(rect.left() + 12.0, rect.top()), pos2(rect.right() - 26.0, rect.bottom()));
        add(ui, content);
        ui.painter().text(
            pos2(rect.right() - 16.0, rect.center().y),
            Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            theme::icon_font(11.0),
            p.text_faint,
        );
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                CornerRadius::same(5),
                Stroke::new(1.5, p.accent),
                StrokeKind::Inside,
            );
        }
    }
    resp
}

/// A caret-only drop-down cell of a [`connect_bar`] (27 wide: 8 px around
/// the caret), e.g. the model list after the model field.
pub fn bar_caret(ui: &mut Ui, name: &str) -> egui::Response {
    let p = theme::pal(ui);
    let h = ui.available_height();
    let (rect, resp) = ui.allocate_exact_size(vec2(27.0, h), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, ui.is_enabled(), name));
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, p.hover);
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icons::CARET_DOWN,
            theme::icon_font(11.0),
            p.text_faint,
        );
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                CornerRadius::same(5),
                Stroke::new(1.5, p.accent),
                StrokeKind::Inside,
            );
        }
    }
    resp
}

/// The width of a [`bar_combo`] showing `text` (12.5) and its caret.
pub fn bar_combo_width(ui: &Ui, text: &str) -> f32 {
    let g = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(12.5), Color32::PLACEHOLDER);
    12.0 + g.size().x + 7.0 + 11.0 + 10.0
}

/// A frameless single-line field for a [`connect_bar`] (a namespace, a
/// working directory…).
pub fn bar_field(ui: &mut Ui, id: &str, text: &mut String, hint: &str, width: f32) -> egui::Response {
    let h = ui.available_height().clamp(20.0, 36.0);
    let r = ui.add(
        egui::TextEdit::singleline(text)
            .id_salt(id)
            .hint_text(hint)
            .desired_width((width - 24.0).max(20.0))
            .font(egui::TextStyle::Monospace)
            // With a frame set, `margin` is ignored: the padding goes in the frame.
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 0)))
            .min_size(vec2(0.0, h))
            .align(egui::Align2::LEFT_CENTER),
    );
    widgets::a11y_name(&r, hint);
    r
}

/// Connect / Cancel / Disconnect for a [`connect_bar`] (38 high). Returns the
/// clicked action.
pub fn connect_button(ui: &mut Ui, state: Conn) -> Option<ConnAction> {
    match state {
        Conn::Idle | Conn::Failed => {
            let r = kit::action_button(ui, kit::Kind::Primary, icons::PLUGS_CONNECTED, tr("Connect"));
            r.on_hover_text(tr("Or press ↩ in the address field")).clicked().then_some(ConnAction::Connect)
        }
        Conn::Busy => {
            let r = kit::action_button(ui, kit::Kind::Secondary, icons::X, tr("Cancel"));
            r.clicked().then_some(ConnAction::Cancel)
        }
        Conn::Live => {
            let r = kit::action_button(ui, kit::Kind::Secondary, icons::PLUGS, tr("Disconnect"));
            r.clicked().then_some(ConnAction::Disconnect)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnAction {
    Connect,
    Cancel,
    Disconnect,
}

/// How the traffic pane of a view is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PaneMode {
    #[default]
    Normal,
    /// The traffic pane fills the view.
    Max,
    /// The traffic pane is folded to its status row (a strip when side by side).
    Min,
}

fn pane_mode(ui: &Ui, id: egui::Id) -> PaneMode {
    ui.data(|d| d.get_temp(id.with("pane-mode")).unwrap_or_default())
}

fn set_pane_mode(ui: &Ui, id: egui::Id, mode: PaneMode) {
    ui.data_mut(|d| d.insert_temp(id.with("pane-mode"), mode));
    ui.ctx().request_repaint();
}

/// The two panes of a protocol view: compose (`left`) and traffic (`right`),
/// side by side when the view is wide enough, else stacked; the traffic pane
/// can be maximized or folded with [`pane_actions`] (same `id`).
pub fn view_panes(ui: &mut Ui, id: egui::Id, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    let side = ui.available_width() >= size::SIDE_BY_SIDE_MIN;
    ui.data_mut(|d| d.insert_temp(id.with("side"), side));
    let rect = ui.available_rect_before_wrap();
    let pane = |ui: &mut Ui, r: Rect, add: Box<dyn FnOnce(&mut Ui) + '_>, salt: &str| {
        inside(ui, UiBuilder::new().max_rect(r).id_salt((id, salt)).layout(Layout::top_down(Align::Min)), |ui| {
            ui.set_min_size(r.size());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    };
    match pane_mode(ui, id) {
        PaneMode::Normal => kit::panes(ui, id, side, 0.5, left, right),
        PaneMode::Max => {
            pane(ui, rect, Box::new(right), "b");
            ui.advance_cursor_after_rect(rect);
        }
        PaneMode::Min if side => {
            const STRIP: f32 = 44.0;
            let a = Rect::from_min_max(rect.min, pos2(rect.right() - STRIP, rect.bottom()));
            let strip = Rect::from_min_max(pos2(a.right(), rect.top()), rect.max);
            pane(ui, a, Box::new(left), "a");
            ui.painter().vline(strip.left() + 0.5, strip.y_range(), Stroke::new(1.0, p.border));
            let button =
                Rect::from_center_size(pos2(strip.center().x, rect.top() + size::ACTION_ROW / 2.0), vec2(22.0, 22.0));
            let r = inside(ui, UiBuilder::new().max_rect(button), |ui| {
                kit::icon_button_sm(ui, icons::CARET_LEFT, tr("Restore"))
            });
            if r.clicked() {
                set_pane_mode(ui, id, PaneMode::Normal);
            }
            ui.advance_cursor_after_rect(rect);
        }
        PaneMode::Min => {
            let b_h = size::ACTION_ROW;
            let a = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - b_h - 1.0));
            let b = Rect::from_min_max(pos2(rect.left(), rect.bottom() - b_h), rect.max);
            pane(ui, a, Box::new(left), "a");
            ui.painter().hline(rect.x_range(), b.top() - 0.5, Stroke::new(1.0, p.border));
            pane(ui, b, Box::new(right), "b");
            ui.advance_cursor_after_rect(rect);
        }
    }
}

/// Maximize and minimize of the traffic pane (the end of its status row,
/// right to left): a divider, then the two buttons.
pub fn pane_actions(ui: &mut Ui, id: egui::Id) {
    let side: bool = ui.data(|d| d.get_temp(id.with("side")).unwrap_or(true));
    let mode = pane_mode(ui, id);
    let (icon, tip, to) = match mode {
        PaneMode::Min => (if side { icons::CARET_LEFT } else { icons::CARET_UP }, tr("Restore"), PaneMode::Normal),
        _ => (if side { icons::CARET_RIGHT } else { icons::CARET_DOWN }, tr("Minimize"), PaneMode::Min),
    };
    if kit::icon_button_sm(ui, icon, tip).clicked() {
        set_pane_mode(ui, id, to);
    }
    let (icon, tip, to) = match mode {
        PaneMode::Max => (icons::ARROWS_IN, tr("Restore"), PaneMode::Normal),
        _ => (icons::ARROWS_OUT, tr("Maximize"), PaneMode::Max),
    };
    if kit::icon_button_sm(ui, icon, tip).clicked() {
        set_pane_mode(ui, id, to);
    }
    vdiv(ui);
}

/// A 1 px divider, 18 high, between groups of a row.
pub fn vdiv(ui: &mut Ui) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, p.border));
}

/// The 56 px status row of a traffic pane: `left` (a status pill, then
/// metrics) and `right` (actions, added right to left).
pub fn status_row(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    row_bar(ui, size::ACTION_ROW, 14.0, 10.0, true, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            right(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                left(ui);
            });
        });
    });
}

/// Metrics after a status pill (`.rmeta`): 4 px apart.
pub fn metrics(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        add(ui);
    });
}

/// A metric without an icon (`TTFT 380 ms`), as [`kit::metric`].
pub fn metric_text(ui: &mut Ui, text: &str) -> egui::Response {
    let p = theme::pal(ui);
    let g = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(12.0), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 12.0, 22.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let c = if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(5), p.hover);
            p.text
        } else {
            p.text_muted
        };
        ui.painter().galley(pos2(rect.left() + 6.0, rect.center().y - g.size().y / 2.0), g, c);
    }
    resp
}

/// The 36 px bar over a log (`.rbar`): `left` and `right` (right to left).
pub fn log_bar(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    row_bar(ui, size::TABS, 12.0, 10.0, true, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            right(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                left(ui);
            });
        });
    });
}

/// A 40 px toolbar: `left`, then `right` added right to left.
pub fn toolbar(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    kit::toolbar(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            right(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                left(ui);
            });
        });
    });
}

/// The title of a toolbar with an optional count pill.
pub fn toolbar_title(ui: &mut Ui, title: &str, count: Option<usize>) {
    ui.add(egui::Label::new(kit::section_title(title)).selectable(false));
    if let Some(n) = count {
        kit::tone_pill(ui, &n.to_string(), Tone::Neutral);
    }
}

/// Content of a pane under its bars: scrolls, `top` px of padding above,
/// 12 px on the sides and 16 below, `gap` between items.
pub fn body<R>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    top: f32,
    gap: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin { left: 12, right: 12, top: top as i8, bottom: 16 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = gap;
                    add(ui)
                })
                .inner
        })
        .inner
}

/// The primary action of a composer toolbar (`Send`, `Emit`, `Publish`):
/// its ⇧⌘↩ hint, then the small primary button (right to left).
pub fn toolbar_send(ui: &mut Ui, label: &str, enabled: bool, disabled_hint: &str) -> bool {
    let opts = kit::ButtonOpts { icon: Some(icons::PAPER_PLANE_RIGHT), ..Default::default() };
    let r = ui.add_enabled_ui(enabled, |ui| kit::button(ui, kit::Kind::Primary, kit::Size::Sm, label, opts)).inner;
    let clicked = if enabled {
        r.on_hover_text(format!("{label} (⇧⌘↩)")).clicked()
    } else {
        r.on_disabled_hover_text(disabled_hint);
        false
    };
    kit::kbd(ui, "⇧⌘↩");
    clicked
}

/// A small single-line input (`.input.sm`: 26 high, radius 6, 12 px text).
pub fn input_sm(ui: &mut Ui, text: &mut String, hint: &str, width: f32, mono: bool) -> egui::Response {
    let p = theme::pal(ui);
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 26.0;
        // egui draws placeholders in the weak text color: faint, as `::placeholder`.
        ui.visuals_mut().weak_text_color = Some(p.text_faint);
        let r = CornerRadius::same(theme::RADIUS_SM);
        let w = &mut ui.visuals_mut().widgets;
        for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            v.corner_radius = r;
        }
        let font = if mono { theme::mono(12.0) } else { egui::FontId::proportional(12.0) };
        let r = ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(RichText::new(hint).font(font.clone()).color(p.text_faint))
                .desired_width(width.max(36.0))
                .font(font)
                .margin(egui::Margin::symmetric(8, 0))
                .min_size(vec2(0.0, 26.0))
                .align(Align2::LEFT_CENTER),
        );
        if !hint.is_empty() {
            widgets::a11y_name(&r, hint);
        }
        r
    })
    .inner
}

/// A frameless input of a list row (`.lrow` inputs): no border until
/// hovered or focused.
pub fn row_input(ui: &mut Ui, text: &mut String, hint: &str, width: f32, mono: bool) -> egui::Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().text_edit_bg_color = Some(Color32::TRANSPARENT);
        input_sm(ui, text, hint, width, mono)
    })
    .inner
}

/// A small input with `{{variables}}` (26 high, framed).
pub fn var_input_sm(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    vars: &widgets::VarCtx,
    hint: &str,
    width: f32,
) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    let bg = ui.painter().add(egui::Shape::Noop);
    let inner = rect.shrink(1.0);
    let r = inside(ui, UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.visuals_mut().weak_text_color = Some(p.text_faint);
        widgets::var_cell_sized(ui, id, text, vars, hint, inner.width(), 12.0)
    });
    let stroke = if r.hovered() { p.text_faint } else { p.border_strong };
    ui.painter().set(
        bg,
        egui::Shape::Rect(egui::epaint::RectShape::new(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface,
            Stroke::new(1.0, stroke),
            StrokeKind::Inside,
        )),
    );
    if r.has_focus() {
        kit::focus_ring(ui, rect, theme::RADIUS_SM);
    }
    r
}

/// A number in a small input (`width` wide, right-aligned), kept as typed
/// while it has focus.
pub fn number_input<T>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
    width: f32,
    name: &str,
) -> egui::Response
where
    T: std::str::FromStr + ToString + Copy + Default,
{
    let id = ui.make_persistent_id(id);
    let p = theme::pal(ui);
    let focused = ui.memory(|m| m.has_focus(id));
    let mut text = if focused {
        ui.data(|d| d.get_temp::<String>(id.with("text"))).unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    };
    let r = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size.y = 26.0;
            let w = &mut ui.visuals_mut().widgets;
            for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                v.corner_radius = CornerRadius::same(theme::RADIUS_SM);
            }
            ui.add(
                egui::TextEdit::singleline(&mut text)
                    .id(id)
                    .desired_width(width.max(36.0))
                    .font(egui::FontId::proportional(12.0))
                    .text_color(p.text)
                    .margin(egui::Margin::symmetric(8, 0))
                    .min_size(vec2(0.0, 26.0))
                    .align(Align2::RIGHT_CENTER),
            )
        })
        .inner;
    widgets::a11y_name(&r, name);
    if r.changed() {
        let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        *value = if digits.is_empty() { T::default() } else { digits.parse().unwrap_or(*value) };
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id.with("text"), text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id.with("text")));
    }
    r
}

/// A checkbox with a label after it, in the small text of toolbars.
pub fn check_label(ui: &mut Ui, on: &mut bool, label: &str) -> egui::Response {
    let size = ui.style().text_styles.get(&egui::TextStyle::Body).map_or(13.0, |f| f.size);
    kit::checkbox_ex(ui, on, label, 6.0, size)
}

/// [`check_label`] with a small (12) label.
pub fn check_label_sm(ui: &mut Ui, on: &mut bool, label: &str) -> egui::Response {
    kit::checkbox_ex(ui, on, label, 6.0, 12.0)
}

/// A bordered list (`.tbl-wrap`) of rows at least `row_h` high (`.lrow`:
/// 40) with hairlines between them; `rows` gets each row's `Ui`, laid out
/// left to right with 8 px gaps and 8/12 px side padding.
pub fn list_box(ui: &mut Ui, n: usize, row_h: f32, mut row: impl FnMut(&mut Ui, usize)) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in 0..n {
                let top = ui.cursor().top();
                let w = ui.available_width();
                let hover_bg = ui.painter().add(egui::Shape::Noop);
                if i > 0 {
                    ui.painter().hline(ui.max_rect().x_range(), top, Stroke::new(1.0, p.border));
                }
                let r = ui.push_id(i, |ui| {
                    ui.allocate_ui_with_layout(vec2(w, row_h), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_min_size(vec2(w, row_h));
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.add_space(12.0);
                        row(ui, i);
                    })
                });
                let rect = r.inner.response.rect;
                if ui.rect_contains_pointer(rect) {
                    let rounding = if n == 1 {
                        CornerRadius::same(theme::RADIUS_CARD - 1)
                    } else if i == 0 {
                        CornerRadius { nw: theme::RADIUS_CARD - 1, ne: theme::RADIUS_CARD - 1, sw: 0, se: 0 }
                    } else if i + 1 == n {
                        CornerRadius { nw: 0, ne: 0, sw: theme::RADIUS_CARD - 1, se: theme::RADIUS_CARD - 1 }
                    } else {
                        CornerRadius::ZERO
                    };
                    ui.painter()
                        .set(hover_bg, egui::Shape::rect_filled(rect.shrink2(vec2(0.0, 0.5)), rounding, p.well));
                }
            }
        });
}

/// A collapsible section header (caret, title, optional count, and `end` on
/// the right). Returns whether it is open.
pub fn disclosure(
    ui: &mut Ui,
    id: egui::Id,
    title: &str,
    count: Option<usize>,
    default_open: bool,
    end: impl FnOnce(&mut Ui),
) -> bool {
    let p = theme::pal(ui);
    let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(default_open));
    // As high as its title line (12.5 on 1.45), or its count pill.
    let h = if count.is_some() { 20.0 } else { 12.5 * 1.45 };
    let row = ui.allocate_ui_with_layout(vec2(ui.available_width(), h), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
        ui.add(
            egui::Label::new(RichText::new(caret).font(theme::icon_font(13.0)).color(p.text_faint)).selectable(false),
        );
        ui.add(egui::Label::new(kit::section_title(title)).selectable(false));
        if let Some(n) = count {
            kit::tone_pill(ui, &n.to_string(), Tone::Neutral);
        }
        let start = ui.cursor().left();
        ui.with_layout(Layout::right_to_left(Align::Center), end);
        start
    });
    let header = Rect::from_min_max(row.response.rect.min, pos2(row.inner + 4.0, row.response.rect.bottom()));
    let r = ui.interact(header, id.with("toggle"), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
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

/// The connection error of a view (with Retry and Dismiss), at the top of
/// its traffic pane. Returns (retry, dismiss).
pub fn connection_error(ui: &mut Ui, title: &str, cause: &str) -> (bool, bool) {
    let mut retry = false;
    let mut dismiss = false;
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 0 }).show(ui, |ui| {
        error_banner(ui, title, cause, |ui| {
            dismiss = kit::icon_button_sm(ui, icons::X, tr("Dismiss")).clicked();
            retry = kit::button(
                ui,
                kit::Kind::Secondary,
                kit::Size::Sm,
                tr("Retry"),
                kit::ButtonOpts { icon: Some(icons::ARROW_CLOCKWISE), ..Default::default() },
            )
            .clicked();
        });
    });
    (retry, dismiss)
}

/// Where an inherited auth of `item` comes from (owner name and type).
pub fn inherited_auth(
    app: &TernaryWireApp,
    slot: &ItemSlot,
    item: &RequestItem,
) -> Option<(String, ternarywire_core::auth::AuthType)> {
    let c = app.ws.data.collection(&slot.location.as_ref()?.collection)?;
    let chain = c.ancestors(&item.id)?;
    ternarywire_core::model::effective_from_chain(&chain).map(|e| (e.owner_name.to_string(), e.auth.kind))
}

/// The auth editor of a protocol request in its pane.
pub fn auth_pane(
    ui: &mut Ui,
    id: &str,
    auth: &mut ternarywire_core::auth::Auth,
    item_id: &str,
    inherited: Option<(String, ternarywire_core::auth::AuthType)>,
    note: Option<&str>,
    vars: &widgets::VarCtx,
) {
    body(ui, id, 12.0, 12.0, |ui| {
        crate::auth_ui::auth_editor(
            ui,
            auth,
            crate::auth_ui::AuthUiCtx {
                is_root: false,
                inherited,
                owner_id: item_id,
                token: None,
                fetching: false,
                vars,
            },
        );
        if let Some(n) = note {
            ui.label(kit::hint(ui, n));
        }
    });
}

/// Whether a request sets its own auth (the tab then shows a dot).
pub fn has_own_auth(auth: &ternarywire_core::auth::Auth) -> bool {
    !auth.is_inherit() && auth.kind != ternarywire_core::auth::AuthType::None
}

/// A code editor with a line-number gutter in a well (`margin` around it),
/// `height` high or filling the space left; longer text scrolls inside.
#[allow(clippy::too_many_arguments)]
pub fn editor_well(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Clone,
    text: &mut String,
    lang: crate::highlight::Lang,
    vars: Option<&widgets::VarCtx>,
    margin: egui::Margin,
    height: Option<f32>,
    completions: Option<widgets::CompletionFn<'_>>,
) -> egui::Response {
    let p = theme::pal(ui);
    let avail = ui.available_rect_before_wrap();
    let (ml, mr, mt, mb) = (margin.left as f32, margin.right as f32, margin.top as f32, margin.bottom as f32);
    let h = height.unwrap_or(avail.height() - mt - mb).max(48.0);
    let rect = Rect::from_min_size(avail.min + vec2(ml, mt), vec2((avail.width() - ml - mr).max(60.0), h));
    ui.painter().rect(rect, CornerRadius::same(8), p.well, Stroke::new(1.0, p.border), StrokeKind::Inside);
    let inner = rect.shrink(1.0);
    let rows = (((h - 18.0) / 20.0).floor() as usize).max(2);
    let r = inside(ui, UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)), |ui| {
        egui::ScrollArea::vertical()
            .id_salt((id.clone(), "well-scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| widgets::code_editor_gutter(ui, id, text, lang, vars, rows, false, completions))
            .inner
    });
    ui.advance_cursor_after_rect(Rect::from_min_max(avail.min, pos2(avail.right(), rect.bottom() + mb)));
    r
}

/// A frameless code editor (inside a card or bubble), no gutter, no
/// padding, 20 px lines (`.code .src` without its padding).
pub fn plain_editor(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    lang: crate::highlight::Lang,
    vars: Option<&widgets::VarCtx>,
    rows: usize,
) -> egui::Response {
    ui.scope(|ui| {
        ui.visuals_mut().selection.stroke = Stroke::NONE;
        widgets::line_editor(ui, id, text, lang, vars, rows, None, 20.0)
    })
    .inner
}

/// Read-only text in a bordered surface box (`.code` without gutter): JSON
/// highlighted, other text wrapped.
pub fn code_block(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            text_view(ui, id, text);
        });
}

/// Read-only JSON or text with a line-number gutter in a bordered surface
/// box (`.code` with its gutter).
pub fn code_block_gutter(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let p = theme::pal(ui);
    let start = text.trim_start();
    let is_json = (start.starts_with('{') || start.starts_with('['))
        && (text.len() > 64 * 1024 || serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok());
    let lang = if is_json { crate::highlight::Lang::Json } else { crate::highlight::Lang::Text };
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(1))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut t = text.to_string();
            widgets::code_editor_gutter(ui, id, &mut t, lang, None, 1, true, None);
        });
}

/// A banner for an error (`.banner.err`): an icon, a title, the cause and
/// optional actions on the right.
pub fn error_banner(ui: &mut Ui, title: &str, cause: &str, action: impl FnOnce(&mut Ui)) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.err_soft)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(RichText::new(icons::WARNING_CIRCLE).font(theme::icon_font(16.0)).color(p.err));
                // The actions' width is measured on the previous frame so the
                // text can wrap in the space left of them.
                let wid = ui.scope_id().with(("error-banner-actions", title));
                let aw: f32 = ui.data(|d| d.get_temp(wid).unwrap_or(0.0));
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - aw - if aw > 0.0 { 10.0 } else { 0.0 }).max(80.0));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(title).font(theme::semibold(12.5)).color(p.text));
                        ui.add(egui::Label::new(RichText::new(cause).size(12.5).color(p.text_muted)).wrap());
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let r = ui.scope(action).response;
                    let w = if r.rect.width() > 1.0 { ui.max_rect().right() - r.rect.left() } else { 0.0 };
                    if (w - aw).abs() > 0.5 {
                        ui.data_mut(|d| d.insert_temp(wid, w));
                    }
                });
            });
        });
}

/// [`notice_card`] with `end` at the right of the title (a count, a queue).
pub fn notice_card_ex<R>(
    ui: &mut Ui,
    icon: &str,
    title: &str,
    end: impl FnOnce(&mut Ui),
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.accent_soft)
        .stroke(Stroke::new(1.0, theme::soft(p.accent, 0.30)))
        .corner_radius(CornerRadius::same(theme::RADIUS_LG))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            // The title's 1.45 line (a pill at the end makes it 20).
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 13.0 * 1.45),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    end(ui);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(RichText::new(icon).font(theme::icon_font(16.0)).color(p.accent_text));
                        ui.add(
                            egui::Label::new(RichText::new(title).font(theme::semibold(13.0)).color(p.text)).truncate(),
                        );
                    });
                },
            );
            add(ui)
        })
        .inner
}

/// Key/value rows (headers, trailers) as a table (`.tbl`): a 30 px header
/// on the well fill (`Key`, `Value`), 34 px rows, monospace cells.
pub fn kv_table_view<'a>(ui: &mut Ui, rows: impl Iterator<Item = (&'a str, &'a str)>, empty: &str) {
    let p = theme::pal(ui);
    let rows: Vec<(&str, &str)> = rows.collect();
    if rows.is_empty() {
        ui.label(kit::hint(ui, empty));
        return;
    }
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = ui.available_width();
            let key_w = (w * 0.4).round();
            let (head, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
            let r = theme::RADIUS_CARD - 1;
            ui.painter().rect_filled(head, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, p.well);
            ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));
            for (text, x) in [(tr("Key"), head.left() + 12.0), (tr("Value"), head.left() + key_w + 12.0)] {
                ui.painter().text(
                    pos2(x, head.center().y),
                    Align2::LEFT_CENTER,
                    text,
                    theme::medium(11.5),
                    p.text_muted,
                );
            }
            let n = rows.len();
            for (i, (k, v)) in rows.into_iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                if resp.hovered() {
                    let rr = if i + 1 == n { CornerRadius { nw: 0, ne: 0, sw: r, se: r } } else { CornerRadius::ZERO };
                    ui.painter().rect_filled(rect, rr, p.well);
                }
                if i + 1 < n {
                    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
                }
                let font = theme::mono(12.0);
                let kg = kit::elide(ui, k, font.clone(), p.text_muted, key_w - 24.0);
                ui.painter().galley(pos2(rect.left() + 12.0, rect.center().y - kg.size().y / 2.0), kg, p.text_muted);
                let (vt, vc) = if v.is_empty() { (tr("(empty)"), p.text_faint) } else { (v, p.text) };
                let vg = kit::elide(ui, vt, font, vc, w - key_w - 24.0);
                ui.painter().galley(pos2(rect.left() + key_w + 12.0, rect.center().y - vg.size().y / 2.0), vg, vc);
                let r = ui.interact(rect, ui.make_persistent_id(("kv-row", i, k)), Sense::click());
                if !v.is_empty() {
                    let r = r.on_hover_text(v);
                    r.context_menu(|ui| {
                        if kit::menu_item(ui, Some(icons::COPY), tr("Copy value"), None).clicked() {
                            ui.ctx().copy_text(v.to_string());
                        }
                    });
                }
            }
        });
}

/// What was clicked in a [`saved_list`] row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowAction {
    Use,
    Send,
    Remove,
}

/// Saved messages or calls as a bordered list (`.lrow`s, 40 high): `row`
/// draws the left part of each row (name, tags, summary) in the width it is
/// given; the actions (load into the composer, send now when `can_send`,
/// remove) are on the right.
pub fn saved_list<T>(
    ui: &mut Ui,
    id: &str,
    items: &mut [T],
    can_send: bool,
    send_label: &str,
    mut row: impl FnMut(&mut Ui, &mut T),
) -> Option<(usize, RowAction)> {
    let mut action = None;
    let n = items.len();
    ui.push_id(id, |ui| {
        list_box(ui, n, 40.0, |ui, i| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                ui.spacing_mut().item_spacing.x = 0.0;
                if kit::icon_button_sm(ui, icons::TRASH, tr("Remove")).clicked() {
                    action = Some((i, RowAction::Remove));
                }
                let tip = if can_send { send_label } else { tr("Connect first") };
                let send = ui.add_enabled_ui(can_send, |ui| kit::icon_button_sm(ui, icons::PAPER_PLANE_RIGHT, tip));
                if send.inner.clicked() {
                    action = Some((i, RowAction::Send));
                }
                if kit::icon_button_sm(ui, icons::ARROW_BEND_UP_LEFT, tr("Load into the composer")).clicked() {
                    action = Some((i, RowAction::Use));
                }
                ui.add_space(8.0);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    row(ui, &mut items[i]);
                });
            });
        });
    });
    action
}

/// A muted one-line summary that fills the rest of a row (mono 11.5).
pub fn row_summary(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let w = ui.available_width();
    if w < 20.0 {
        return;
    }
    let g = kit::elide(ui, &preview_text(text), theme::mono(11.5), p.text_faint, w);
    let (rect, _) = ui.allocate_exact_size(vec2(w, g.size().y), Sense::hover());
    ui.painter().galley(rect.left_top(), g, p.text_faint);
}

/// An outlined monospace tag (`.pill.outline.mono`, 11): an event, a topic.
pub fn mono_tag(ui: &mut Ui, text: &str, max: f32) -> egui::Response {
    mono_tag_ex(ui, text, max, 11.0)
}

/// [`mono_tag`] with text of `size` (11.5 without the prototype's inline 11).
pub fn mono_tag_ex(ui: &mut Ui, text: &str, max: f32, size: f32) -> egui::Response {
    let p = theme::pal(ui);
    let g = kit::elide(ui, text, theme::mono(size), p.text_muted, max);
    // 7 px padding inside a 1 px border.
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 20.0), Sense::hover());
    ui.painter().rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().galley(pos2(rect.left() + 8.0, rect.center().y - g.size().y / 2.0), g, p.text_muted);
    r
}

/// "Save message" for a composer toolbar: a small icon button, or a ghost
/// button with its label when `wide` (in a right-to-left row).
pub fn save_message_button(ui: &mut Ui, enabled: bool) -> bool {
    save_message_button_ex(ui, enabled, false)
}

/// [`save_message_button`]; `wide` shows the label.
pub fn save_message_button_ex(ui: &mut Ui, enabled: bool, wide: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        let r = if wide {
            kit::ghost(ui, Some(icons::BOOKMARK_SIMPLE), tr("Save message"))
        } else {
            kit::icon_button_sm(ui, icons::BOOKMARK_SIMPLE, tr("Save message"))
        };
        r.on_hover_text(tr("Keep this message with the request")).clicked()
    })
    .inner
}

/// A name field for list rows: frameless until hovered or focused.
pub fn name_field(ui: &mut Ui, text: &mut String, width: f32) -> egui::Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().text_edit_bg_color = Some(Color32::TRANSPARENT);
        ui.add(
            egui::TextEdit::singleline(text)
                .desired_width(width)
                .font(theme::medium(13.0))
                .margin(egui::Margin::symmetric(4, 3))
                .hint_text(tr("Name")),
        )
    })
    .inner
}

/// A placeholder painted in an empty [`editor_well`] editor.
pub fn well_placeholder(ui: &Ui, editor: &egui::Response, text: &str, hint: &str) {
    if !text.is_empty() || editor.has_focus() {
        return;
    }
    let p = theme::pal(ui);
    ui.painter().text(
        editor.rect.left_top() + vec2(14.0, 10.0),
        Align2::LEFT_TOP,
        hint,
        theme::mono(12.5),
        p.text_faint,
    );
}

/// The number of scripts a request has (the count of its Scripts tab).
pub fn scripts_count(scripts: &ternarywire_core::model::Scripts) -> usize {
    [&scripts.pre_request, &scripts.post_response, &scripts.on_message].iter().filter(|s| !s.trim().is_empty()).count()
}

/// Resolves `{{variables}}` with the scopes of `collection` (plus vault),
/// owning its data so it can be moved into async tasks.
pub fn resolver(app: &TernaryWireApp, collection: Option<&str>) -> impl Fn(&str) -> String + Send + Sync + 'static {
    let scopes: VariableScopes = app.ws.scopes_for(collection);
    let vault: Arc<dyn ternarywire_core::variables::VaultLookup> = app.ws.vault();
    move |s: &str| Resolver::new(&scopes).with_vault(vault.as_ref()).resolve(s)
}

/// True when ⇧⌘↩ (the main action of a protocol tab: send, publish, emit,
/// invoke, call) was pressed; ⌘↩ alone belongs to the app.
pub fn send_shortcut(ui: &Ui) -> bool {
    ui.input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Enter,
        ))
    })
}

// ---------------------------------------------------------------------------
// Message log
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    In,
    Out,
    System,
}

pub struct LogLine {
    pub dir: Dir,
    /// Topic, event name or method.
    pub title: String,
    pub text: String,
    pub at_ms: i64,
    pub size: usize,
    pub error: bool,
    /// A binary payload, shown as text (hex or base64).
    pub binary: bool,
    /// Short facts shown after the title (`QoS 1 · retained`).
    pub meta: String,
}

/// Which lines the log shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogFilter {
    #[default]
    All,
    Sent,
    Received,
}

/// A protocol tab's message log: the latest `cap` lines, one row each in a
/// list that lays out only the visible rows (logs hold thousands), and the
/// selected message in full below it.
pub struct Log {
    /// Add lines with [`Log::push`]/[`Log::push_line`].
    pub lines: VecDeque<LogLine>,
    pub search: String,
    pub filter: LogFilter,
    pub selected: Option<usize>,
    pub cap: usize,
    /// Title and explanation shown while the log is empty.
    pub empty: (&'static str, &'static str),
    /// Title of the log bar (translated when drawn); empty for none.
    pub title: &'static str,
    /// Lines ever pushed; the first line kept is number `pushed - lines.len()`.
    pushed: u64,
    /// Messages sent and received since the log was cleared.
    sent: u64,
    received: u64,
    found: Found,
    /// Rows the list had last frame.
    rows_shown: usize,
    /// The selected message as shown (pretty JSON), by line number.
    shown: Option<(u64, String, bool)>,
    /// Scroll offset and height of the list last frame (keyboard navigation).
    view: (f32, f32),
}

/// The lines matching a search (by number), found incrementally so a long
/// log is not searched again every frame.
#[derive(Default)]
struct Found {
    query: String,
    filter: LogFilter,
    numbers: VecDeque<u64>,
    /// The next line number to check.
    next: u64,
}

impl Default for Log {
    fn default() -> Self {
        Log {
            lines: VecDeque::new(),
            search: String::new(),
            filter: LogFilter::All,
            selected: None,
            cap: 5000,
            empty: ("No messages yet", "Connect and send a message to see the conversation here."),
            title: "Messages",
            pushed: 0,
            sent: 0,
            received: 0,
            found: Found::default(),
            rows_shown: 0,
            shown: None,
            view: (0.0, 0.0),
        }
    }
}

/// Incoming messages kept for on-message scripts until the tab is drawn
/// again (a tab in the background is not), so busy streams stay bounded.
pub const MAX_INBOX: usize = 5000;

pub fn queue_message(inbox: &mut VecDeque<serde_json::Value>, m: serde_json::Value) {
    inbox.push_back(m);
    if inbox.len() > MAX_INBOX {
        inbox.pop_front();
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Rows of the log (`.log .lr`), with a hairline under each.
const ROW_H: f32 = 34.0;
/// Share of the pane the selected message takes under the list.
const DETAIL: f32 = 0.42;

impl Log {
    /// A log with its own empty-state text.
    pub fn with_empty(title: &'static str, body: &'static str) -> Log {
        Log { empty: (title, body), ..Default::default() }
    }

    pub fn push(&mut self, dir: Dir, title: impl Into<String>, text: impl Into<String>, at_ms: i64) {
        let text = text.into();
        let size = text.len();
        self.push_line(LogLine {
            dir,
            title: title.into(),
            text,
            at_ms,
            size,
            error: false,
            binary: false,
            meta: String::new(),
        });
    }

    pub fn push_line(&mut self, line: LogLine) {
        match line.dir {
            Dir::In => self.received += 1,
            Dir::Out => self.sent += 1,
            Dir::System => {}
        }
        self.lines.push_back(line);
        self.pushed += 1;
        if self.lines.len() > self.cap {
            self.lines.pop_front();
            self.selected = self.selected.and_then(|s| s.checked_sub(1));
        }
    }

    pub fn system(&mut self, text: impl Into<String>) {
        self.push(Dir::System, "", text, now_ms());
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Dir::System, "", text, now_ms());
        if let Some(l) = self.lines.back_mut() {
            l.error = true;
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.selected = None;
        self.shown = None;
        self.sent = 0;
        self.received = 0;
    }

    /// Messages sent and received since the log was cleared.
    pub fn counts(&self) -> (u64, u64) {
        (self.sent, self.received)
    }

    /// Number of the first line kept.
    fn first(&self) -> u64 {
        self.pushed.saturating_sub(self.lines.len() as u64)
    }

    /// Indexes of the lines matching `search` and `filter`, or `None` when
    /// every line is shown.
    fn matching(&mut self) -> Option<Vec<usize>> {
        let q = self.search.to_lowercase();
        if q.is_empty() && self.filter == LogFilter::All {
            return None;
        }
        let first = self.first();
        let filter = self.filter;
        let found = &mut self.found;
        if found.query != q || found.filter != filter {
            *found = Found { query: q, filter, numbers: VecDeque::new(), next: first };
        }
        while found.numbers.front().is_some_and(|&n| n < first) {
            found.numbers.pop_front();
        }
        for n in found.next.max(first)..self.pushed {
            let Some(l) = self.lines.get((n - first) as usize) else { break };
            let dir_ok = match filter {
                LogFilter::All => true,
                LogFilter::Sent => l.dir == Dir::Out,
                LogFilter::Received => l.dir == Dir::In,
            };
            let text_ok = found.query.is_empty()
                || l.text.to_lowercase().contains(&found.query)
                || l.title.to_lowercase().contains(&found.query);
            if dir_ok && text_ok {
                found.numbers.push_back(n);
            }
        }
        found.next = self.pushed;
        Some(found.numbers.iter().map(|n| (n - first) as usize).collect())
    }

    #[allow(dead_code)] // the whole log in one call (tests)
    pub fn ui(&mut self, ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug) {
        self.ui_with(ui, id, |_| {});
    }

    /// The log bar (with `status` widgets after its title) and the log.
    #[allow(dead_code)] // the whole log in one call (tests)
    pub fn ui_with(&mut self, ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, status: impl FnOnce(&mut Ui)) {
        if !self.title.is_empty() {
            self.bar_with(ui, &id, status);
        }
        self.body(ui, &id);
    }

    /// The 36 px bar over the log: its title and count, then the filter,
    /// the search and Clear.
    pub fn bar(&mut self, ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug) {
        self.bar_with(ui, id, |_| {});
    }

    fn bar_with(&mut self, ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, status: impl FnOnce(&mut Ui)) {
        let title = self.title;
        let n = self.lines.len();
        log_bar(
            ui,
            |ui| {
                if !title.is_empty() {
                    toolbar_title(ui, tr(title), Some(n));
                }
                status(ui);
            },
            |ui| {
                // The search (150) gives way to the title and its count.
                let left = if title.is_empty() {
                    0.0
                } else {
                    text_width(ui, tr(title), theme::semibold(12.5)) + 8.0 + pill_width(ui, &n.to_string())
                };
                let seg = kit::segmented_width(ui, &Self::filters(), true);
                let w = (ui.available_width() - left - 16.0 - seg - 8.0 - 8.0 - 22.0).clamp(90.0, 150.0);
                self.tools(ui, &id, true, w)
            },
        );
    }

    /// The direction filter: (icon, label) of All, Sent and Received.
    fn filters() -> [(Option<&'static str>, &'static str); 3] {
        [(None, tr("All")), (Some(icons::ARROW_UP), tr("Sent")), (Some(icons::ARROW_DOWN), tr("Received"))]
    }

    /// The log's controls for a bar or the end of a tab strip (right to
    /// left): Clear, the search (when `search_w` > 0) and the direction
    /// filter (when `filter`).
    pub fn tools(&mut self, ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, filter: bool, search_w: f32) {
        let clear =
            ui.add_enabled_ui(!self.lines.is_empty(), |ui| kit::icon_button_sm(ui, icons::TRASH, tr("Clear the log")));
        if clear.inner.clicked() {
            self.clear();
        }
        if search_w > 0.0 {
            kit::search_field_sm(ui, (&id, "search"), &mut self.search, tr("Search messages"), search_w);
        }
        if filter {
            let [all, sent, received] = Self::filters();
            ui.push_id((&id, "filter"), |ui| {
                kit::segmented_ex(
                    ui,
                    &mut self.filter,
                    &[
                        (LogFilter::All, all.0, all.1),
                        (LogFilter::Sent, sent.0, sent.1),
                        (LogFilter::Received, received.0, received.1),
                    ],
                    true,
                );
            });
        }
    }

    /// The rows, and the selected message under them.
    pub fn body(&mut self, ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug) {
        let p = theme::pal(ui);
        let rows = self.matching();
        let count = rows.as_ref().map_or(self.lines.len(), Vec::len);
        if count < self.rows_shown {
            // Fewer rows (a search): the list scrolls back into range at the
            // end of this frame and shows them in the next.
            ui.ctx().request_repaint();
        }
        self.rows_shown = count;
        if self.lines.is_empty() {
            kit::empty_state(ui, icons::CHATS, tr(self.empty.0), tr(self.empty.1), |_| {});
            return;
        }
        if count == 0 {
            ui.add_space(28.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icons::MAGNIFYING_GLASS).font(theme::icon_font(22.0)).color(p.text_faint));
                ui.label(RichText::new(tr("No messages match")).color(p.text_muted));
            });
            return;
        }
        // Keyboard: ↑/↓ move the selection, Esc closes it.
        let row_of = |i: usize| rows.as_ref().map_or(Some(i), |r| r.iter().position(|&x| x == i));
        let mut moved = None;
        if ui.memory(|m| m.focused().is_none())
            && let Some(sel) = self.selected
            && let Some(row) = row_of(sel)
        {
            let (up, down, esc) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            let target = if up {
                row.checked_sub(1)
            } else if down {
                Some(row + 1).filter(|r| *r < count)
            } else {
                None
            };
            if let Some(t) = target {
                self.selected = Some(rows.as_ref().map_or(t, |r| r[t]));
                moved = Some(t);
            }
            if esc {
                self.selected = None;
            }
        }
        let total = ui.available_height();
        let selected_line = self.selected.filter(|&i| i < self.lines.len());
        let detail_h = if selected_line.is_some() { (total * DETAIL).max(120.0).min(total - ROW_H * 2.0) } else { 0.0 };
        let list_h = if selected_line.is_some() { total - detail_h - size::GUTTER } else { total };
        let mut area = egui::ScrollArea::vertical()
            .id_salt(&id)
            .stick_to_bottom(true)
            .auto_shrink([false; 2])
            .max_height(list_h.max(ROW_H));
        if let Some(t) = moved {
            let (offset, height) = self.view;
            let top = t as f32 * ROW_H;
            if top < offset {
                area = area.vertical_scroll_offset(top);
            } else if top + ROW_H > offset + height {
                area = area.vertical_scroll_offset(top + ROW_H - height);
            }
        }
        let mut clicked = None;
        let selected = self.selected;
        let out = kit::show_rows(area, ui, ROW_H, count, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in range {
                let i = rows.as_ref().map_or(row, |r| r[row]);
                if let Some(l) = self.lines.get(i)
                    && log_row(ui, l, selected == Some(i)).clicked()
                {
                    clicked = Some(i);
                }
            }
        });
        self.view = (out.state.clamped_offset().y, out.inner_rect.height());
        if let Some(i) = clicked {
            self.selected = if self.selected == Some(i) { None } else { Some(i) };
        }
        let Some(sel) = selected_line else { return };
        let number = self.first() + sel as u64;
        if self.shown.as_ref().is_none_or(|(n, _, _)| *n != number) {
            let text = &self.lines[sel].text;
            let (shown, json) = match serde_json::from_str::<serde_json::Value>(text) {
                Ok(v) if (v.is_object() || v.is_array()) && text.len() < 2_000_000 => {
                    (serde_json::to_string_pretty(&v).unwrap_or_else(|_| text.clone()), true)
                }
                _ => (text.clone(), false),
            };
            self.shown = Some((number, shown, json));
        }
        let l = &self.lines[sel];
        let Some((_, shown, json)) = &self.shown else { return };
        // A 9 px gutter with a hairline, then the message.
        let (g, _) = ui.allocate_exact_size(vec2(ui.available_width(), size::GUTTER), Sense::hover());
        ui.painter().hline(g.x_range(), g.center().y, Stroke::new(1.0, p.border));
        let mut close = false;
        row_bar(ui, 34.0, 12.0, 10.0, false, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                close = kit::icon_button_sm(ui, icons::X, tr("Close (Esc)")).clicked();
                if kit::icon_button_sm(ui, icons::COPY, tr("Copy message")).clicked() {
                    ui.ctx().copy_text(shown.clone());
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    let (icon, fg, bg, label) = dir_look(&p, l);
                    let (t, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    ui.painter().rect_filled(t, CornerRadius::same(6), bg);
                    ui.painter().text(t.center(), Align2::CENTER_CENTER, icon, theme::icon_font(12.0), fg);
                    ui.label(RichText::new(label).font(theme::medium(13.0)).color(p.text));
                    ui.label(RichText::new(clock(l.at_ms)).font(theme::mono(11.5)).color(p.text_faint));
                    if !l.title.is_empty() {
                        mono_tag(ui, &l.title, 220.0);
                    }
                    if !l.meta.is_empty() {
                        ui.label(RichText::new(&l.meta).size(11.0).color(p.text_faint));
                    }
                    if l.size > 0 && l.dir != Dir::System {
                        ui.label(RichText::new(size_text(l)).size(11.0).color(p.text_faint));
                    }
                });
            });
        });
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 12 }).show(ui, |ui| {
            let h = ui.available_height().max(40.0);
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
            ui.painter().rect(rect, CornerRadius::same(8), p.surface, Stroke::new(1.0, p.border), StrokeKind::Inside);
            let inner = rect.shrink(1.0);
            inside(ui, UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)), |ui| {
                egui::ScrollArea::vertical().id_salt((&id, "message")).auto_shrink([false; 2]).show(ui, |ui| {
                    // `.code .src` (JSON) or `.mono` text, 10 / 12 in.
                    let margin = if *json { egui::Margin::symmetric(14, 8) } else { egui::Margin::symmetric(12, 10) };
                    egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if *json {
                            text_view(ui, (&id, "message-text", number), shown);
                        } else {
                            let rt = RichText::new(shown.as_str())
                                .font(theme::mono(12.5))
                                .line_height(Some(18.0))
                                .color(if l.error { p.err } else { p.text });
                            ui.add(egui::Label::new(rt).wrap().selectable(true));
                        }
                    });
                });
            });
        });
        if close {
            self.selected = None;
        }
    }
}

/// The width of `text` in `font` on one line.
pub fn text_width(ui: &Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER).size().x
}

/// The width of a (20 high) [`kit::tone_pill`] showing `text`.
pub fn pill_width(ui: &Ui, text: &str) -> f32 {
    text_width(ui, text, theme::medium(11.5)) + 14.0
}

/// `14:02:31.123` in local time.
fn clock(at_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M:%S%.3f").to_string())
        .unwrap_or_default()
}

fn size_text(l: &LogLine) -> String {
    let size = ternarywire_core::util::human_bytes(l.size as u64);
    if l.binary { format!("{size} · bin") } else { size }
}

/// Icon, colors (text, tile) and spoken direction of a log line.
fn dir_look(p: &theme::Palette, l: &LogLine) -> (&'static str, Color32, Color32, &'static str) {
    match l.dir {
        Dir::In => (icons::ARROW_DOWN, p.ok, p.ok_soft, tr("Received")),
        Dir::Out => (icons::ARROW_UP, p.info, p.info_soft, tr("Sent")),
        Dir::System if l.error => (icons::WARNING, p.err, p.err_soft, tr("Error")),
        Dir::System => (icons::INFO, p.text_muted, p.subtle, tr("Event")),
    }
}

/// One row of the log (painted, for speed): the direction tile, the time,
/// the title chip, the start of the message and its size on the right.
/// Clicking selects it.
fn log_row(ui: &mut Ui, l: &LogLine, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let preview = preview_text(&l.text);
    response.widget_info(|| {
        let name = if l.title.is_empty() { preview.clone() } else { format!("{} {preview}", l.title) };
        egui::WidgetInfo::selected(egui::Role::Button, true, selected, name)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let p = theme::pal(ui);
    let painter = ui.painter_at(rect.intersect(ui.clip_rect()));
    if selected {
        painter.rect_filled(rect, 0.0, p.accent_soft);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, p.well);
    } else if l.error {
        painter.rect_filled(rect, 0.0, theme::soft(p.err, 0.07));
    }
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let cy = rect.center().y;
    let mut x = rect.left() + 12.0;
    let (icon, fg, bg, _) = dir_look(&p, l);
    let tile = Rect::from_center_size(pos2(x + 10.0, cy), vec2(20.0, 20.0));
    painter.rect_filled(tile, CornerRadius::same(6), bg);
    painter.text(tile.center(), Align2::CENTER_CENTER, icon, theme::icon_font(12.0), fg);
    x += 22.0 + 8.0;
    let t = painter.layout_no_wrap(clock(l.at_ms), theme::mono(11.5), p.text_faint);
    painter.galley(pos2(x, cy - t.size().y / 2.0), t, p.text_faint);
    x += 92.0 + 8.0;
    // Size on the right (a column at least 36 wide).
    let mut right = rect.right() - 12.0;
    if l.size > 0 && l.dir != Dir::System {
        let s = painter.layout_no_wrap(size_text(l), egui::FontId::proportional(11.5), p.text_faint);
        painter.galley(pos2(right - s.size().x, cy - s.size().y / 2.0), s.clone(), p.text_faint);
        right -= s.size().x.max(36.0);
    } else {
        right -= 36.0;
    }
    right -= 8.0;
    if !l.title.is_empty() {
        // As wide as the title (a long topic), leaving the message some room.
        let max = ((right - x) * 0.6).max(128.0);
        let g = truncated(&painter, &l.title, theme::mono(11.5), p.text, max);
        let chip = Rect::from_min_size(pos2(x, cy - g.size().y / 2.0 - 1.0), vec2(g.size().x + 12.0, g.size().y + 2.0));
        painter.rect_filled(chip, CornerRadius::same(5), if selected { p.surface } else { p.subtle });
        painter.galley(pos2(chip.left() + 6.0, cy - g.size().y / 2.0), g, p.text);
        x = chip.right();
    }
    x += 8.0;
    // The meta first (as the prototype's span), the message in what is left.
    if !l.meta.is_empty() && right - x > 60.0 {
        let g = truncated(&painter, &l.meta, egui::FontId::proportional(11.5), p.text_faint, right - x - 16.0);
        painter.galley(pos2(x, cy - g.size().y / 2.0), g.clone(), p.text_faint);
        x += g.size().x + 8.0;
    }
    if right > x + 8.0 {
        let (font, color) = if l.dir == Dir::System {
            (egui::FontId::proportional(12.0), if l.error { p.text } else { p.text_muted })
        } else {
            (theme::mono(12.0), p.text)
        };
        let g = truncated(&painter, &preview, font, color, right - x);
        painter.galley(pos2(x, cy - g.size().y / 2.0), g, color);
    }
    response
}

/// The start of a message on one line: control characters become spaces
/// and runs of spaces (pretty JSON indentation) collapse into one.
pub fn preview_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(400));
    let mut space = false;
    for c in text.chars() {
        if out.len() >= 400 {
            break;
        }
        if c.is_whitespace() || c.is_control() {
            if !space && !out.is_empty() {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

/// One line of text cut to `max` width with an ellipsis.
fn truncated(painter: &egui::Painter, text: &str, font: egui::FontId, color: Color32, max: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: max.max(10.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

/// Pretty JSON when the payload is JSON, otherwise text (lossy) or hex.
pub fn payload_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(v) if v.is_object() || v.is_array() => {
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| s.to_string())
            }
            _ => s.to_string(),
        },
        Err(_) => bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
    }
}

/// Username/password fields stored as Basic auth (MQTT credentials), as a
/// "Credentials" settings group.
pub fn basic_credentials(ui: &mut Ui, auth: &mut ternarywire_core::auth::Auth, vars: &widgets::VarCtx) {
    use ternarywire_core::auth::{Auth, AuthType};
    kit::group(ui, tr("Credentials"), "", |ui, rows| {
        rows.block(ui, |ui| {
            let mut on = auth.kind == AuthType::Basic;
            if kit::checkbox(ui, &mut on, tr("Use username and password")).changed() {
                *auth = if on { Auth::basic("", "") } else { Auth::default() };
            }
        });
        if auth.kind != AuthType::Basic {
            return;
        }
        let mut user = auth.get_string("username");
        let mut pass = auth.get_string("password");
        rows.row(ui, tr("Username"), "", |ui| {
            if var_input_sm(ui, "mqtt-user", &mut user, vars, "", 190.0).changed() {
                auth.set("username", user.clone());
            }
        });
        rows.row(ui, tr("Password"), "", |ui| {
            let r = if ternarywire_core::variables::is_pure_reference(&pass) {
                var_input_sm(ui, "mqtt-pass", &mut pass, vars, "", 190.0)
            } else {
                widgets::secret_field(ui, "mqtt-pass", &mut pass, 190.0)
            };
            if r.changed() {
                auth.set("password", pass.clone());
            }
        });
    });
}

/// Read-only, selectable text; JSON keeps its indentation and is highlighted.
pub fn text_view(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    // Drawn every frame: a large text is taken for JSON by its first
    // character (parsing megabytes each frame stalls the UI), a small one
    // is checked without building it.
    let start = text.trim_start();
    let is_json = (start.starts_with('{') || start.starts_with('['))
        && (text.len() > 64 * 1024 || serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok());
    let lang = if is_json { crate::highlight::Lang::Json } else { crate::highlight::Lang::Text };
    let mut t = text.to_string();
    widgets::code_view(ui, id, &mut t, lang);
}

// ---------------------------------------------------------------------------
// Protocol scripts (FR-S7)
// ---------------------------------------------------------------------------

use ternarywire_core::script::{ProtocolContext, ScriptEvent, ScriptJob, ScriptOutcome, TestResult};

/// One finished protocol script run, delivered to its tab.
pub struct ScriptRun {
    pub event: ScriptEvent,
    pub outcome: ScriptOutcome,
    pub before: VariableScopes,
    pub collection: Option<String>,
    /// Environment active when the script started.
    pub env: Option<String>,
    pub name: String,
    /// Number of the run (see [`run_scripts`]).
    pub run: u64,
}

/// Test results and script errors a protocol tab keeps (the latest; on-message
/// scripts of a busy stream add some for every message), like the console.
pub const MAX_SCRIPT_RESULTS: usize = 5000;

/// Script editor selection, test results and the pending pre-request result
/// of a protocol tab.
pub struct ScriptState {
    pub editing: ScriptEvent,
    pub tests: VecDeque<TestResult>,
    pub errors: VecDeque<String>,
    /// Pre-request finished: the request JSON to use, or `None` when the
    /// script called `pm.execution.skipRequest()`.
    pre: Option<Option<serde_json::Value>>,
    /// The pre-request run the tab waits for (its number): the tab counts
    /// as busy, and the result of an earlier, cancelled run is not used.
    waiting: Option<u64>,
}

impl Default for ScriptState {
    fn default() -> Self {
        ScriptState {
            editing: ScriptEvent::PreRequest,
            tests: VecDeque::new(),
            errors: VecDeque::new(),
            pre: None,
            waiting: None,
        }
    }
}

impl ScriptState {
    /// Waits for pre-request run `run` (from [`run_scripts`]).
    pub fn wait_for(&mut self, run: u64) {
        self.waiting = Some(run);
    }

    pub fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }

    /// Cancel: the pending pre-request result is not used.
    pub fn stop_waiting(&mut self) {
        self.waiting = None;
    }

    pub fn clear_results(&mut self) {
        self.tests.clear();
        self.errors.clear();
    }

    pub fn on_run(&mut self, run: &ScriptRun) {
        self.tests.extend(run.outcome.tests.iter().cloned());
        self.errors.extend(run.outcome.errors.iter().map(|e| format!("{}: {}", e.location, e.message)));
        let extra = self.tests.len().saturating_sub(MAX_SCRIPT_RESULTS);
        self.tests.drain(..extra);
        let extra = self.errors.len().saturating_sub(MAX_SCRIPT_RESULTS);
        self.errors.drain(..extra);
        if run.event == ScriptEvent::PreRequest && self.waiting == Some(run.run) {
            self.waiting = None;
            self.pre = Some(if run.outcome.skip_request { None } else { run.outcome.protocol_request.clone() });
        }
    }

    /// The pre-request result, once (see [`ScriptState::pre`]).
    pub fn take_pre(&mut self) -> Option<Option<serde_json::Value>> {
        self.pre.take()
    }
}

fn script_sources(
    app: &TernaryWireApp,
    slot: &ItemSlot,
    item: &RequestItem,
    event: ScriptEvent,
) -> Vec<ternarywire_core::script::ScriptSource> {
    let collection = slot.location.as_ref().and_then(|l| app.ws.data.collection(&l.collection));
    let chain = collection.and_then(|c| c.ancestors(&item.id)).unwrap_or_default();
    ternarywire_runner::pipeline::script_chain(&chain, &item.scripts, &item.name, event)
}

/// Runs the item's scripts for `event` (collection and folder scripts first)
/// on a worker thread; the result arrives as [`crate::app::Msg::ProtoScript`].
/// Returns the number of the run ([`ScriptState::wait_for`]), or `None`
/// when there is nothing to run.
pub fn run_scripts(
    app: &TernaryWireApp,
    tab_id: u64,
    slot: &ItemSlot,
    item: &RequestItem,
    event: ScriptEvent,
    ctx: ProtocolContext,
) -> Option<u64> {
    let scripts = script_sources(app, slot, item, event);
    if scripts.is_empty() {
        return None;
    }
    let collection = slot.collection_id();
    let before = app.ws.scopes_for(collection.as_deref());
    let env = app.ws.data.active_environment.clone();
    let services = app.services();
    let mut job = ScriptJob::for_protocol(event, scripts, ctx);
    job.variables = before.clone();
    job.info.request_name = item.name.clone();
    job.info.request_id = item.id.clone();
    job.info.iteration_count = 1;
    job.info.collection_name =
        collection.as_deref().and_then(|c| app.ws.data.collection(c)).map(|c| c.name.clone()).unwrap_or_default();
    job.packages = services.packages.clone();
    job.timeout = services.script_timeout;
    // Files the scripts send must come from inside the workspace folder.
    job.http = Some(Arc::new(
        ternarywire_runner::script_http::PipelineHttp::new(services.clone()).with_base_dir(Some(app.ws.root())),
    ));
    let engine = services.scripts.clone();
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    let name = item.name.clone();
    let ws = app.ws_gen;
    let run = crate::app::next_epoch();
    app.rt.spawn_blocking(move || {
        let outcome = engine.run(job);
        let run = ScriptRun { event, outcome, before, collection, env, name, run };
        let _ = tx.send(crate::app::Msg::ProtoScript { tab: tab_id, ws, run: Box::new(run) });
        ctx.request_repaint();
    });
    Some(run)
}

/// The Scripts section of a protocol tab: a toolbar with the events (a dot
/// on those with code) and the snippets, the latest test results, then the
/// editor of the selected event filling the pane.
pub fn scripts_ui(
    ui: &mut Ui,
    scripts: &mut ternarywire_core::model::Scripts,
    events: &[(ScriptEvent, &str)],
    state: &mut ScriptState,
    vars: &widgets::VarCtx,
) {
    let p = theme::pal(ui);
    if !events.iter().any(|(e, _)| *e == state.editing) {
        state.editing = events[0].0;
    }
    let editing = state.editing;
    let mut snippet: Option<&'static str> = None;
    toolbar(
        ui,
        |ui| {
            let labels: Vec<String> = events
                .iter()
                .map(|(event, label)| {
                    let dot = if scripts.code(*event).trim().is_empty() { "" } else { " •" };
                    format!("{}{dot}", tr(label))
                })
                .collect();
            let items: Vec<(ScriptEvent, Option<&str>, &str)> =
                events.iter().zip(&labels).map(|((e, _), l)| (*e, None, l.as_str())).collect();
            kit::segmented_ex(ui, &mut state.editing, &items, true);
        },
        |ui| {
            let r = kit::icon_button_sm(ui, icons::LIGHTNING, tr("Snippets"));
            egui::Popup::menu(&r).width(260.0).show(|ui| {
                for s in
                    crate::snippets::SNIPPETS.iter().filter(|s| editing == ScriptEvent::PostResponse || !s.post_only)
                {
                    if kit::menu_item(ui, None, s.title, None)
                        .on_hover_text(RichText::new(s.code).monospace())
                        .clicked()
                    {
                        snippet = Some(s.code);
                    }
                }
            });
            ui.add(
                egui::Label::new(kit::hint(ui, tr("JavaScript with pm.request, pm.response and pm.message")))
                    .truncate(),
            );
        },
    );
    let code = match state.editing {
        ScriptEvent::PreRequest => &mut scripts.pre_request,
        ScriptEvent::PostResponse => &mut scripts.post_response,
        ScriptEvent::OnMessage => &mut scripts.on_message,
    };
    if let Some(s) = snippet {
        if !code.is_empty() && !code.ends_with('\n') {
            code.push('\n');
        }
        code.push_str(s);
    }
    let has_results = !state.tests.is_empty() || !state.errors.is_empty();
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        if has_results {
            let passed = state.tests.iter().filter(|t| t.passed && !t.skipped).count();
            let failed = state.tests.iter().filter(|t| !t.passed && !t.skipped).count();
            ui.horizontal(|ui| {
                ui.set_height(24.0);
                ui.label(kit::section_title(tr("Test results")));
                kit::tone_pill(ui, &format!("{} {}", icons::CHECK, trf("{} passed", &[&passed])), Tone::Ok);
                kit::tone_pill(ui, &trf("{} failed", &[&failed]), if failed > 0 { Tone::Err } else { Tone::Neutral });
                if !state.errors.is_empty() {
                    kit::tone_pill(ui, &trf("{} errors", &[&state.errors.len()]), Tone::Err);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if kit::ghost(ui, None, tr("Clear")).clicked() {
                        state.clear_results();
                    }
                });
            });
            let lines = state.tests.len() + state.errors.len();
            let h = (lines as f32 * 32.0).min(32.0 * 4.0) + 2.0;
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
            ui.painter().rect(
                rect,
                CornerRadius::same(theme::RADIUS_CARD),
                p.surface,
                Stroke::new(1.0, p.border),
                StrokeKind::Inside,
            );
            let inner = rect.shrink(1.0);
            inside(ui, UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)), |ui| {
                egui::ScrollArea::vertical().id_salt("proto-tests").auto_shrink([false; 2]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let w = ui.available_width();
                    let row = |ui: &mut Ui,
                               icon: &str,
                               color: Color32,
                               name: &str,
                               detail: Option<&str>,
                               pill: Option<(&str, Tone)>,
                               last: bool| {
                        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
                        if !last {
                            ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
                        }
                        inside(
                            ui,
                            UiBuilder::new()
                                .max_rect(r.shrink2(vec2(12.0, 0.0)))
                                .layout(Layout::left_to_right(Align::Center)),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(RichText::new(icon).font(theme::icon_font(14.0)).color(color));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if let Some((t, tone)) = pill {
                                        kit::tone_pill(ui, t, tone);
                                    }
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(name).size(12.5).color(p.text)).truncate(),
                                        );
                                        if let Some(d) = detail {
                                            ui.add(
                                                egui::Label::new(RichText::new(d).size(12.0).color(p.text_muted))
                                                    .truncate(),
                                            );
                                        }
                                    });
                                });
                            },
                        );
                    };
                    let total = state.tests.len() + state.errors.len();
                    for (i, t) in state.tests.iter().enumerate() {
                        let (icon, color, pill) = if t.skipped {
                            (icons::MINUS, p.text_muted, ("SKIP", Tone::Neutral))
                        } else if t.passed {
                            (icons::CHECK_CIRCLE, p.ok, ("PASS", Tone::Ok))
                        } else {
                            (icons::X_CIRCLE, p.err, ("FAIL", Tone::Err))
                        };
                        row(ui, icon, color, &t.name, t.error.as_deref(), Some(pill), i + 1 == total);
                    }
                    for (i, e) in state.errors.iter().enumerate() {
                        row(ui, icons::WARNING_CIRCLE, p.err, e, None, None, state.tests.len() + i + 1 == total);
                    }
                });
            });
        }
        if state.is_waiting() {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0).color(p.accent));
                ui.label(RichText::new(tr("Running the pre-request script…")).color(p.text_muted));
            });
        }
    });
    if has_results || state.is_waiting() {
        ui.add_space(10.0);
    }
    let id = ("proto-script", state.editing == ScriptEvent::PreRequest, state.editing == ScriptEvent::OnMessage);
    let h = (ui.available_height() - 16.0).max(210.0);
    editor_well(
        ui,
        id,
        code,
        crate::highlight::Lang::JavaScript,
        Some(vars),
        egui::Margin { left: 12, right: 12, top: 0, bottom: 16 },
        Some(h),
        Some(&widgets::pm_completions),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the pre-request run a tab waits for is used: a run cancelled
    /// before it finished (Connect, Cancel, Connect) does not connect too.
    #[test]
    fn only_the_awaited_pre_request_result_counts() {
        let run = |run| ScriptRun {
            event: ScriptEvent::PreRequest,
            outcome: ScriptOutcome::default(),
            before: VariableScopes::default(),
            collection: None,
            env: None,
            name: String::new(),
            run,
        };
        let mut s = ScriptState::default();
        s.wait_for(1);
        s.stop_waiting();
        s.wait_for(2);
        s.on_run(&run(1));
        assert!(s.is_waiting() && s.take_pre().is_none());
        s.on_run(&run(2));
        assert!(!s.is_waiting() && s.take_pre().is_some());
        // Nothing waits for it after Cancel.
        s.on_run(&run(2));
        assert!(s.take_pre().is_none());
    }

    fn draw(ctx: &egui::Context, log: &mut Log) -> (std::time::Duration, Vec<String>) {
        ctx.enable_accesskit();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 820.0))),
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default_margins().show(ui, |ui| log.ui(ui, "log"));
        });
        let took = started.elapsed();
        let nodes = out.platform_output.accesskit_update.take().map(|u| u.nodes).unwrap_or_default();
        out.drop_without_applying_deltas();
        (took, nodes.iter().filter_map(|(_, n)| n.label().or(n.value()).map(str::to_string)).collect())
    }

    /// The MQTT, Socket.IO, gRPC and MCP logs lay out only the rows in view,
    /// one line each, so a log of 20,000 messages (some long and multi-line)
    /// draws in milliseconds; the selected message is shown in full.
    #[test]
    fn big_log_draws_only_visible_rows() {
        let mut log = Log { cap: 20_000, ..Default::default() };
        let json = serde_json::to_string_pretty(&serde_json::json!({"items": vec![1; 500]})).unwrap();
        for i in 0..20_000 {
            let text = if i % 7 == 0 { json.clone() } else { format!("message {i}") };
            log.push(if i % 2 == 0 { Dir::In } else { Dir::Out }, "topic/a", text, 0);
        }
        log.selected = Some(19_998);
        let ctx = egui::Context::default();
        draw(&ctx, &mut log);
        let (took, labels) = draw(&ctx, &mut log);
        assert!(labels.len() < 1_000, "{} widgets", labels.len());
        assert!(took < std::time::Duration::from_millis(500), "{took:?}");
        // The newest rows are in view (the list sticks to the bottom).
        assert!(labels.iter().any(|l| l == "topic/a message 19997"), "{labels:?}");
        assert!(!labels.iter().any(|l| l == "topic/a message 1"), "{labels:?}");
        // The selected message in full below the list.
        assert!(labels.iter().any(|l| l == "message 19998"), "{labels:?}");
    }

    /// Searching keeps up with new lines and with the oldest being dropped.
    #[test]
    fn search_follows_the_log() {
        let mut log = Log { cap: 10, ..Default::default() };
        for i in 0..10 {
            log.push(Dir::In, "", format!("m{i}"), 0);
        }
        log.search = "M1".into();
        assert_eq!(log.matching(), Some(vec![1]));
        for i in 10..15 {
            log.push(Dir::In, "", format!("m{i}"), 0);
        }
        // m1 dropped; m10..m14 added (now at indexes 5..9).
        assert_eq!(log.matching(), Some(vec![5, 6, 7, 8, 9]));
        log.search = "m13".into();
        assert_eq!(log.matching(), Some(vec![8]));
        log.clear();
        assert_eq!(log.matching(), Some(vec![]));
        log.push(Dir::In, "m13", "", 0);
        assert_eq!(log.matching(), Some(vec![0]));
        log.search.clear();
        assert_eq!(log.matching(), None);
    }

    /// The Sent/Received filter combines with the search.
    #[test]
    fn filter_by_direction() {
        let mut log = Log::default();
        log.push(Dir::Out, "", "ping", 0);
        log.push(Dir::In, "", "pong", 0);
        log.system("connected");
        log.push(Dir::In, "", "ping back", 0);
        log.filter = LogFilter::Received;
        assert_eq!(log.matching(), Some(vec![1, 3]));
        log.search = "ping".into();
        assert_eq!(log.matching(), Some(vec![3]));
        log.filter = LogFilter::Sent;
        assert_eq!(log.matching(), Some(vec![0]));
        log.filter = LogFilter::All;
        log.search.clear();
        assert_eq!(log.matching(), None);
    }
}
