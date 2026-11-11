//! Environments and globals (FR-V1..V4): the title-bar selector, the
//! sidebar list and the variable editor (also used for collection and
//! folder variables).
//!
//! Also the pieces the workspace views share (collection settings,
//! environments, Spec Hub): the 46 px title row with its tile, the hover
//! menu of a sidebar row and the sidebar footer.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    pos2, vec2,
};

use ternarywire_core::model::Id;
use ternarywire_core::variables::{VarKind, Variable};

use crate::app::TernaryWireApp;
use crate::dialogs::Dialog;
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::{self, MenuItem, Tone, TreeRow, size};
use crate::widgets::{self, VarCtx};
use crate::{icons, theme};

#[derive(Default)]
pub struct EnvTab {
    /// `None` = globals.
    pub env: Option<Id>,
    pub search: String,
}

// ---------------------------------------------------------------------------
// Shared view pieces
// ---------------------------------------------------------------------------

/// The color of the environment at `index` in the list. Environments have
/// no color in the model, so it is derived from their position, cycling
/// through palette tokens in this order: ok, warn, accent, info, then the
/// PATCH, HEAD and OPTIONS method colors. It stays the same as long as the
/// order of the list does.
pub fn env_color(ui: &Ui, index: usize) -> Color32 {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let colors = [
        p.ok,
        p.warn,
        p.accent,
        p.info,
        theme::method_color("PATCH", dark),
        theme::method_color("HEAD", dark),
        theme::method_color("OPTIONS", dark),
    ];
    colors[index % colors.len()]
}

/// The 26 px rounded tile with the icon of a view: accent soft, or a soft
/// fill of `tint` (an environment's color).
pub(crate) fn tile(ui: &mut Ui, icon: &str, tint: Option<Color32>) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
    let (bg, fg) = match tint {
        Some(c) => (theme::soft(c, 0.15), c),
        None => (p.accent_soft, p.accent_text),
    };
    ui.painter().rect_filled(rect, CornerRadius::same(7), bg);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::icon_font(15.0), fg);
    resp
}

/// The 46 px title row of a workspace view (the prototype's `.w-title`):
/// the tile, the name drawn by `name` (see [`title_label`]), a muted
/// `· subtitle`, and on the right the controls `right` adds (right to left,
/// 10 apart). A hairline under the whole width.
pub(crate) fn title_row<R>(
    ui: &mut Ui,
    icon: &str,
    tint: Option<Color32>,
    sub: &str,
    right: impl FnOnce(&mut Ui),
    name: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = theme::pal(ui);
    kit::bar(ui, size::VIEW_TITLE, true, |ui| {
        // 16 px on the left, 12 on the right.
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 10.0;
        tile(ui, icon, tint);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            right(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let sub = (!sub.is_empty()).then(|| format!("· {sub}"));
                let sub_w = sub.as_ref().map_or(0.0, |s| {
                    ui.painter().layout_no_wrap(s.clone(), FontId::proportional(12.0), p.text_muted).size().x + 10.0
                });
                let name_w = (ui.available_width() - sub_w).max(60.0);
                let r = ui
                    .allocate_ui_with_layout(vec2(name_w, 30.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_max_width(name_w);
                        name(ui)
                    })
                    .inner;
                if let Some(s) = sub
                    && ui.available_width() > 24.0
                {
                    ui.add(egui::Label::new(RichText::new(s).size(12.0).color(p.text_muted)).truncate());
                }
                r
            })
            .inner
        })
        .inner
    })
}

/// The name in a [`title_row`]: 14 semibold, cut with an ellipsis;
/// clickable so a double-click can rename.
pub(crate) fn title_label(ui: &mut Ui, text: &str) -> egui::Response {
    let p = theme::pal(ui);
    ui.add(
        egui::Label::new(RichText::new(text).font(theme::semibold(14.0)).color(p.text))
            .truncate()
            .sense(Sense::click()),
    )
}

/// Rows a code editor (`widgets::code_editor_gutter`, 8 px above and below)
/// asks for to fill `height`: its minimum height counts rows of the font.
pub(crate) fn editor_rows(ui: &Ui, height: f32) -> usize {
    let size = ui.style().text_styles.get(&egui::TextStyle::Monospace).map(|f| f.size).unwrap_or(12.5);
    let row = ui.fonts_mut(|f| f.row_height(&FontId::monospace(size)));
    ((height - 16.0) / row.max(1.0)).floor().max(4.0) as usize
}

/// Whether the hover menu `popup_id` of a sidebar row is open (pass it as
/// `TreeRow::force_actions`, so the row keeps showing its button).
pub(crate) fn row_menu_open(ui: &Ui, popup_id: egui::Id) -> bool {
    egui::Popup::is_id_open(ui.ctx(), popup_id)
}

/// The `⋯` button of a sidebar row in `actions` (the rect `kit::tree_row`
/// returns while the row is hovered or its menu open), with `menu` in a
/// popup.
pub(crate) fn row_menu(ui: &mut Ui, actions: Option<Rect>, popup_id: egui::Id, width: f32, menu: impl FnOnce(&mut Ui)) {
    let Some(rect) = actions else { return };
    let open = row_menu_open(ui, popup_id);
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::right_to_left(Align::Center)));
    let b = kit::icon_button_ex(&mut child, icons::DOTS_THREE, tr("More actions"), open, 22.0);
    egui::Popup::menu(&b).id(popup_id).width(width).show(menu);
}

// ---------------------------------------------------------------------------
// Title bar selector
// ---------------------------------------------------------------------------

fn selector_menu_id() -> egui::Id {
    egui::Id::unique("env-selector-menu-auto")
}

/// Automation: opens the menu of the title bar's environment selector.
pub fn automation_selector_menu(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(selector_menu_id(), true));
}

/// The environment selector of the title bar: a small select box (color dot
/// and name) and the quick-look eye button, 2 apart. Laid out in the
/// direction of `ui` (the title bar adds it right to left).
pub fn environment_selector(app: &mut TernaryWireApp, ui: &mut Ui) {
    let active = app
        .ws
        .data
        .active_environment
        .as_deref()
        .and_then(|id| app.ws.data.environments.iter().position(|e| e.id == id));
    let name = active.map(|i| app.ws.data.environments[i].name.clone());
    let label = name.clone().unwrap_or_else(|| tr("No environment").to_string());
    let dot = active.map(|i| env_color(ui, i));
    let rtl = ui.layout().main_dir() == egui::Direction::RightToLeft;
    let mut quick_look = false;
    let select = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut eye = |ui: &mut Ui| {
                if kit::icon_button(ui, icons::EYE, tr("Environment quick look")).clicked() {
                    quick_look = true;
                }
            };
            let select = |ui: &mut Ui| {
                let tw = ui.painter().layout_no_wrap(label.clone(), FontId::proportional(12.0), Color32::PLACEHOLDER);
                let w = (8.0 + if dot.is_some() { 14.0 } else { 0.0 } + tw.size().x + 26.0).clamp(132.0, 220.0);
                kit::select_box_ex(ui, None, dot, &label, w, true, false)
            };
            if rtl {
                eye(ui);
                select(ui)
            } else {
                let r = select(ui);
                eye(ui);
                r
            }
        })
        .inner;
    let select = select.on_hover_text(format!("{} (⌘E)", tr("Switch environment")));
    widgets::a11y_name(&select, &format!("{}: {}", tr("Environment"), label));
    let mut choice: Option<Option<Id>> = None;
    let mut manage = false;
    if ui.data_mut(|d| d.remove_temp::<bool>(selector_menu_id())).unwrap_or(false) {
        egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&select));
    }
    egui::Popup::menu(&select).width(240.0).align(egui::RectAlign::BOTTOM_END).gap(4.0).show(|ui| {
        let none = MenuItem { checked: Some(active.is_none()), label: tr("No environment"), ..Default::default() };
        if kit::menu_item_ex(ui, none).clicked() {
            choice = Some(None);
        }
        for (i, e) in app.ws.data.environments.iter().enumerate() {
            let detail = trf("{} variables", &[&e.variables.len()]);
            let item = MenuItem {
                dot: Some(env_color(ui, i)),
                label: &e.name,
                detail: Some(&detail),
                selected: active == Some(i),
                ..Default::default()
            };
            if kit::menu_item_ex(ui, item).clicked() {
                choice = Some(Some(e.id.clone()));
            }
        }
        kit::menu_sep(ui);
        if kit::menu_item(ui, Some(icons::EYE), tr("Quick look"), None).clicked() {
            quick_look = true;
        }
        if kit::menu_item(ui, Some(icons::SLIDERS_HORIZONTAL), tr("Manage environments"), None).clicked() {
            manage = true;
        }
    });
    if quick_look {
        app.dialog = Some(Dialog::quick_look());
    }
    if let Some(c) = choice {
        app.ws.set_active_environment(c);
    }
    if manage {
        app.sidebar_tab = crate::app::SidebarTab::Environments;
        app.settings.ui.sidebar_visible = true;
        if let Some(id) = app.ws.data.active_environment.clone() {
            app.open_environment(Some(id));
        }
    }
}

// ---------------------------------------------------------------------------
// Environment actions
// ---------------------------------------------------------------------------

/// Something to do with an environment, from its menu.
enum EnvAct {
    Open(Option<Id>),
    Activate(Option<Id>),
    Rename(Id, String),
    Duplicate(Id),
    Export(Id),
    Delete(Id),
}

fn apply(app: &mut TernaryWireApp, act: EnvAct) {
    match act {
        EnvAct::Open(id) => app.open_environment(id),
        EnvAct::Activate(id) => app.ws.set_active_environment(id),
        EnvAct::Rename(id, name) => app.dialog = Some(Dialog::rename_env(id, name)),
        EnvAct::Duplicate(id) => {
            if let Some(e) = app.ws.data.environment(&id).cloned() {
                let mut copy = e.clone();
                copy.id = ternarywire_core::util::new_id();
                copy.name = trf("{} Copy", &[&e.name]);
                app.ws.add_environment(copy);
            }
        }
        EnvAct::Export(id) => crate::dialogs::export_environment(app, &id),
        EnvAct::Delete(id) => {
            let name = app.ws.data.environment(&id).map(|e| e.name.clone()).unwrap_or_default();
            app.dialog = Some(Dialog::confirm_delete_env(id, name));
        }
    }
}

/// Menu entries of an environment (row menu, context menu, tab header).
fn env_menu(ui: &mut Ui, id: &Id, name: &str, active: bool, acts: &mut Vec<EnvAct>) {
    let (icon, label) =
        if active { (icons::CIRCLE, tr("Deactivate")) } else { (icons::CHECK_CIRCLE, tr("Set active")) };
    if kit::menu_item(ui, Some(icon), label, None).clicked() {
        acts.push(EnvAct::Activate(if active { None } else { Some(id.clone()) }));
    }
    if kit::menu_item(ui, Some(icons::PENCIL_SIMPLE), tr("Rename"), None).clicked() {
        acts.push(EnvAct::Rename(id.clone(), name.to_string()));
    }
    if kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked() {
        acts.push(EnvAct::Duplicate(id.clone()));
    }
    if kit::menu_item(ui, Some(icons::EXPORT), &trf("{} (Postman)", &[&tr("Export")]), None).clicked() {
        acts.push(EnvAct::Export(id.clone()));
    }
    kit::menu_sep(ui);
    let delete = MenuItem { icon: Some(icons::TRASH), label: tr("Delete…"), danger: true, ..Default::default() };
    if kit::menu_item_ex(ui, delete).clicked() {
        acts.push(EnvAct::Delete(id.clone()));
    }
}

/// The environment open in the active tab (`Some(None)` = globals).
fn open_in_tab(app: &TernaryWireApp) -> Option<Option<Id>> {
    match app.tabs.get(app.active).map(|t| &t.kind) {
        Some(crate::app::TabKind::Environment(t)) => Some(t.env.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

/// The Environments sidebar under its header (drawn by the shell): filter,
/// Globals, the environments and a hint at the bottom.
pub fn sidebar_list(app: &mut TernaryWireApp, ui: &mut Ui) {
    let p = theme::pal(ui);
    let open = open_in_tab(app);
    let mut acts: Vec<EnvAct> = Vec::new();
    let filter_id = egui::Id::unique("env-list-filter");
    let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
    ui.add_space(4.0);
    let r = kit::search_field_sm(ui, "env-list-search", &mut filter, tr("Filter environments"), ui.available_width());
    widgets::a11y_name(&r, tr("Filter environments"));
    ui.add_space(8.0);
    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    let q = filter.trim().to_lowercase();
    let foot = tr(
        "Double-click an environment to make it active. Initial values are committed; current values stay on this machine.",
    );
    let list_h =
        (ui.available_height() - crate::sidebar::sidebar_footer_height(ui, foot, ui.available_width())).max(40.0);
    let mut new_env = false;
    egui::ScrollArea::vertical().id_salt("env-list").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let globals_name = tr("Globals");
        if q.is_empty() || globals_name.to_lowercase().contains(&q) {
            let count = app.ws.data.globals.len().to_string();
            let row = TreeRow {
                pad_left: 4.0,
                icon: Some(icons::GLOBE_SIMPLE),
                label: globals_name,
                meta: Some(&count),
                selected: open == Some(None),
                ..Default::default()
            };
            let (r, _) = kit::tree_row(ui, row, |_| {});
            widgets::a11y_button(&r, globals_name);
            if r.on_hover_text(tr("Variables available in every request and environment")).clicked() {
                acts.push(EnvAct::Open(None));
            }
        }
        kit::sidebar_group(ui, tr("Environments"));
        if app.ws.data.environments.is_empty() {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(RichText::new(tr("No environments yet")).font(theme::medium(13.0)).color(p.text));
                ui.label(kit::hint(ui, tr("Keep values like base URLs and tokens per stage.")));
                ui.add_space(6.0);
                if kit::secondary(ui, Some(icons::PLUS), tr("New Environment")).clicked() {
                    new_env = true;
                }
            });
        }
        let mut shown = 0;
        for (i, e) in app.ws.data.environments.iter().enumerate() {
            if !q.is_empty() && !e.name.to_lowercase().contains(&q) {
                continue;
            }
            shown += 1;
            let active = app.ws.data.active_environment.as_deref() == Some(&e.id);
            let selected = open.as_ref() == Some(&Some(e.id.clone()));
            let popup_id = egui::Id::unique(("env-row-menu", &e.id));
            let menu_open = row_menu_open(ui, popup_id);
            let count = e.variables.len().to_string();
            let row = TreeRow {
                pad_left: 4.0,
                // The active one has a check; the others their color dot,
                // painted over a clear icon that keeps the label in place.
                icon: Some(if active { icons::CHECK_CIRCLE } else { icons::CIRCLE }),
                icon_color: Some(if active { p.ok } else { Color32::TRANSPARENT }),
                label: &e.name,
                meta: Some(count.as_str()),
                selected,
                actions_width: 22.0,
                force_actions: menu_open,
                ..Default::default()
            };
            let (resp, actions) = kit::tree_row(ui, row, |ui| {
                if active {
                    kit::tone_pill(ui, tr("Active"), Tone::Ok);
                }
            });
            widgets::a11y_button(&resp, &e.name);
            let lead = Rect::from_center_size(pos2(resp.rect.left() + 11.5, resp.rect.center().y), vec2(20.0, 20.0));
            let d = ui.interact(lead, egui::Id::unique(("env-active", &e.id)), Sense::click());
            widgets::a11y_button(&d, if active { tr("Deactivate") } else { tr("Set active") });
            if !active {
                let (icon, color) = if d.hovered() {
                    (Some(icons::CHECK_CIRCLE), p.ok)
                } else {
                    (None, env_color(ui, i).gamma_multiply(0.9))
                };
                match icon {
                    Some(icon) => {
                        ui.painter().text(lead.center(), Align2::CENTER_CENTER, icon, theme::icon_font(15.0), color);
                    }
                    None => {
                        ui.painter().circle_filled(lead.center(), 4.0, color);
                    }
                }
            }
            if d.on_hover_text(if active { tr("Active — click to deactivate") } else { tr("Set active") }).clicked() {
                acts.push(EnvAct::Activate(if active { None } else { Some(e.id.clone()) }));
            }
            row_menu(ui, actions, popup_id, 200.0, |ui| env_menu(ui, &e.id, &e.name, active, &mut acts));
            if resp.clicked() {
                acts.push(EnvAct::Open(Some(e.id.clone())));
            }
            if resp.double_clicked() {
                acts.push(EnvAct::Activate(Some(e.id.clone())));
            }
            resp.context_menu(|ui| env_menu(ui, &e.id, &e.name, active, &mut acts));
        }
        if shown == 0 && !q.is_empty() && !app.ws.data.environments.is_empty() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(kit::hint(ui, &trf("No environments match “{}”", &[&filter.trim()])));
            });
        }
        ui.add_space(8.0);
    });
    crate::sidebar::sidebar_footer(ui, foot);
    if new_env {
        app.dialog = Some(Dialog::prompt_new_environment());
    }
    for a in acts {
        apply(app, a);
    }
}

// ---------------------------------------------------------------------------
// Variables table
// ---------------------------------------------------------------------------

/// Editable variables table (the prototype's `.vt`): enable box, name, type
/// (default/secret), initial value (shared: saved in the workspace) and
/// current value (local), row actions on hover, and an empty last row that
/// adds a variable. `ctx` colors the `{{variables}}` in values. Shows the
/// bulk editor instead while it is on (see [`bulk_toggle`]). Returns true
/// when changed.
pub fn variables_table(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    vars: &mut Vec<Variable>,
    filter: &str,
    ctx: &VarCtx,
) -> bool {
    let salt = egui::Id::unique(&id);
    ui.push_id(salt, |ui| {
        if bulk_on(ui, salt) { bulk_editor(ui, salt, vars, ctx) } else { variables_grid(ui, salt, vars, filter, ctx) }
    })
    .inner
}

fn bulk_on(ui: &Ui, salt: egui::Id) -> bool {
    ui.data(|d| d.get_temp(salt.with("bulk")).unwrap_or(false))
}

fn set_bulk(ui: &Ui, salt: egui::Id, on: bool) {
    ui.data_mut(|d| {
        d.insert_temp(salt.with("bulk"), on);
        d.remove_temp::<String>(salt.with("bulk-text"));
    });
}

/// The "Bulk edit" / "Key-value edit" button of the table `id` (toolbars).
pub(crate) fn bulk_toggle(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug) {
    let salt = egui::Id::unique(&id);
    let on = bulk_on(ui, salt);
    let (icon, label) =
        if on { (icons::TABLE, tr("Key-value edit")) } else { (icons::TEXT_ALIGN_LEFT, tr("Bulk edit")) };
    if kit::ghost(ui, Some(icon), label).clicked() {
        set_bulk(ui, salt, !on);
    }
}

/// Bulk edit text: one `key:value` line per variable, disabled ones
/// prefixed with `//`. Secrets are left out (their values are not shown).
fn bulk_format(vars: &[Variable]) -> String {
    vars.iter()
        .filter(|v| !v.is_secret())
        .map(|v| format!("{}{}:{}", if v.enabled { "" } else { "//" }, v.key, v.value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bulk_parse(text: &str) -> Vec<(bool, String, String)> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (enabled, l) = match l.trim_start().strip_prefix("//") {
                Some(rest) => (false, rest),
                None => (true, l),
            };
            let (k, v) = l.split_once(':').unwrap_or((l, ""));
            (enabled, k.trim().to_string(), v.to_string())
        })
        .collect()
}

/// The variables of the bulk text: the parsed lines (keeping the current
/// value, type and description of names that existed), with the secrets
/// back where they were.
fn bulk_merge(text: &str, old: &[Variable]) -> Vec<Variable> {
    let mut out: Vec<Variable> = bulk_parse(text)
        .into_iter()
        .map(|(enabled, key, value)| {
            let mut v = old
                .iter()
                .find(|o| !o.is_secret() && o.key == key)
                .cloned()
                .unwrap_or_else(|| Variable::new(key.clone(), ""));
            v.key = key;
            v.value = value;
            v.enabled = enabled;
            v
        })
        .collect();
    for (i, s) in old.iter().enumerate().filter(|(_, v)| v.is_secret()) {
        out.insert(i.min(out.len()), s.clone());
    }
    out
}

fn bulk_editor(ui: &mut Ui, salt: egui::Id, vars: &mut Vec<Variable>, ctx: &VarCtx) -> bool {
    let text_id = salt.with("bulk-text");
    let stored: Option<String> = ui.data(|d| d.get_temp(text_id));
    // The text being edited, unless the variables were changed elsewhere.
    let mut text = match stored {
        Some(s) if bulk_format(&bulk_merge(&s, vars)) == bulk_format(vars) => s,
        _ => bulk_format(vars),
    };
    let mut changed = false;
    let r = kit::well(ui)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::code_editor_gutter(ui, salt.with("bulk-editor"), &mut text, Lang::Text, Some(ctx), 8, false, None)
        })
        .inner;
    if r.changed() {
        *vars = bulk_merge(&text, vars);
        changed = true;
    }
    ui.data_mut(|d| d.insert_temp(text_id, text));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(kit::hint(
                ui,
                tr("One entry per line as key:value. Prefix with // to disable. Secrets are kept and not shown here."),
            ))
            .wrap(),
        );
    });
    changed
}

/// A small upper-case tag in a column header (`SHARED`, `LOCAL`).
fn header_tag(ui: &Ui, at: egui::Pos2, icon: &str, text: &str) -> Rect {
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    job.append(icon, 0.0, TextFormat::simple(theme::icon_font(10.0), p.text_faint));
    let mut f = TextFormat::simple(theme::semibold(10.0), p.text_faint);
    f.extra_letter_spacing = 0.2;
    job.append(&format!(" {text}"), 0.0, f);
    let g = ui.painter().layout_job(job);
    let rect = Rect::from_min_size(pos2(at.x, at.y - 8.0), vec2(g.size().x + 10.0, 16.0));
    ui.painter().rect_filled(rect, CornerRadius::same(4), p.subtle);
    ui.painter().galley(pos2(rect.left() + 5.0, rect.center().y - g.size().y / 2.0), g, p.text_faint);
    rect
}

/// A masked secret value: eight dots and a lock.
fn paint_mask(ui: &Ui, at: egui::Pos2, faint: bool) {
    let p = theme::pal(ui);
    let mut f = TextFormat::simple(theme::mono(12.0), if faint { p.text_faint } else { p.text_muted });
    f.extra_letter_spacing = 2.0;
    let g = ui.painter().layout_job(LayoutJob::single_section("••••••••".into(), f));
    ui.painter().galley(pos2(at.x, at.y - g.size().y / 2.0), g.clone(), p.text_muted);
    ui.painter().text(
        pos2(at.x + g.size().x + 4.0, at.y),
        Align2::LEFT_CENTER,
        icons::LOCK_SIMPLE,
        theme::icon_font(12.0),
        p.text_faint,
    );
}

/// The cell editor of a secret value: masked (dots and a lock) until it is
/// revealed with its eye button or focused. `fallback` is shown masked and
/// faint when the value is empty (the current value falls back to the
/// initial one).
fn secret_cell(ui: &mut Ui, salt: &str, text: &mut String, cell: Rect, fallback: bool) -> egui::Response {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id(("secret", salt));
    let reveal_id = id.with("reveal");
    let mut reveal: bool = ui.data(|d| d.get_temp(reveal_id).unwrap_or(false));
    let eye = Rect::from_center_size(pos2(cell.right() - 10.0 - 11.0, cell.center().y), vec2(22.0, 22.0));
    let edit_rect = Rect::from_min_max(cell.min, pos2(eye.left() - 2.0, cell.max.y));
    let focused = ui.memory(|m| m.has_focus(id));
    let masked = !reveal && !focused;
    let r = ui.put(
        edit_rect,
        egui::TextEdit::singleline(text)
            .id(id)
            .password(!reveal)
            .font(theme::mono(12.0))
            .text_color(if masked { Color32::TRANSPARENT } else { p.text })
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(9, 0)))
            .min_size(vec2(0.0, edit_rect.height()))
            .align(Align2::LEFT_CENTER)
            .desired_width(edit_rect.width()),
    );
    if masked && (!text.is_empty() || fallback) {
        paint_mask(ui, pos2(cell.left() + 10.0, cell.center().y), text.is_empty());
    }
    let label = if reveal { tr("Hide value") } else { tr("Show value") };
    let e =
        ui.put(eye, |ui: &mut Ui| kit::icon_button_sm(ui, if reveal { icons::EYE_SLASH } else { icons::EYE }, label));
    if e.clicked() {
        reveal = !reveal;
        ui.data_mut(|d| d.insert_temp(reveal_id, reveal));
    }
    r
}

fn variables_grid(ui: &mut Ui, salt: egui::Id, vars: &mut Vec<Variable>, filter: &str, ctx: &VarCtx) -> bool {
    let p = theme::pal(ui);
    let mut changed = false;
    let mut remove = None;
    // Columns 34 / 1fr / 128 / 1.25fr / 1.25fr / 64; header 30, rows 36.
    const CHECK: f32 = 34.0;
    const KIND: f32 = 128.0;
    const ACT: f32 = 64.0;
    const HEAD: f32 = 30.0;
    const ROW: f32 = 36.0;
    let width = ui.available_width().max(420.0);
    let left = ui.cursor().left();
    let top = ui.cursor().top();
    let fr = (width - CHECK - KIND - ACT) / 3.5;
    let xs = [
        left + CHECK,
        left + CHECK + fr,
        left + CHECK + fr + KIND,
        left + CHECK + fr + KIND + fr * 1.25,
        left + width - ACT,
    ];
    let right = left + width;
    let bg = ui.painter().add(egui::Shape::Noop);
    let mono = theme::mono(12.0);

    // Header: well fill, 11.5 medium muted labels.
    let head = Rect::from_min_size(pos2(left, top), vec2(width, HEAD));
    ui.painter().rect_filled(head, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, p.well);
    if !vars.is_empty() {
        let all = vars.iter().all(|v| v.enabled);
        let some = vars.iter().any(|v| v.enabled);
        let c = Rect::from_center_size(pos2(left + CHECK / 2.0, head.center().y), vec2(15.0, 15.0));
        let r = ui.interact(c, salt.with("all"), Sense::click());
        widgets::a11y_name(&r, tr("Enabled"));
        kit::paint_check(ui, c, all, some && !all, r.hovered(), ui.is_enabled());
        if r.clicked() {
            for v in vars.iter_mut() {
                v.enabled = !all;
            }
            changed = true;
        }
    }
    let heads = [
        (xs[0], xs[1], tr("Variable"), "", None),
        (xs[1], xs[2], tr("Type"), tr("Secret values are masked and never written to the workspace files"), None),
        (
            xs[2],
            xs[3],
            tr("Initial value"),
            tr("Shared: saved to the workspace files (secrets excepted)"),
            Some((icons::SHARE, tr("SHARED"))),
        ),
        (
            xs[3],
            xs[4],
            tr("Current value"),
            tr("Local: used when sending, never committed"),
            Some((icons::DESKTOP, tr("LOCAL"))),
        ),
    ];
    for (i, (x0, x1, label, help, tag)) in heads.iter().enumerate() {
        let g = ui.painter().layout_no_wrap(label.to_string(), theme::medium(11.5), p.text_muted);
        let w = g.size().x;
        ui.painter().galley(pos2(x0 + 10.0, head.center().y - g.size().y / 2.0), g, p.text_muted);
        if let Some((icon, text)) = tag {
            header_tag(ui, pos2(x0 + 10.0 + w + 6.0, head.center().y), icon, text);
        }
        if !help.is_empty() {
            let r = Rect::from_min_max(pos2(*x0, head.top()), pos2(*x1, head.bottom()));
            ui.interact(r, salt.with(("head", i)), Sense::hover()).on_hover_text(*help);
        }
    }
    let opts = Rect::from_center_size(pos2(right - 6.0 - 11.0, head.center().y), vec2(22.0, 22.0));
    let o = ui.put(opts, |ui: &mut Ui| kit::icon_button_sm(ui, icons::DOTS_THREE, tr("Table options")));
    egui::Popup::menu(&o).width(250.0).show(|ui| {
        if kit::menu_item(ui, Some(icons::TEXT_ALIGN_LEFT), tr("Bulk edit"), None).clicked() {
            set_bulk(ui, salt, true);
        }
        kit::menu_sep(ui);
        let any_current = vars.iter().any(|v| v.current.is_some());
        ui.add_enabled_ui(any_current, |ui| {
            if kit::menu_item(ui, Some(icons::ARROW_LINE_DOWN), tr("Persist all current values as initial"), None)
                .clicked()
            {
                for v in vars.iter_mut() {
                    if let Some(c) = v.current.take() {
                        v.value = c;
                    }
                }
                changed = true;
            }
            if kit::menu_item(ui, Some(icons::ARROW_COUNTER_CLOCKWISE), tr("Reset all current values"), None).clicked()
            {
                for v in vars.iter_mut() {
                    v.current = None;
                }
                changed = true;
            }
        });
    });

    let mut y = head.bottom();
    let auto_menu: Option<String> = ui.data(|d| d.get_temp(auto_menu_id()));
    let needle = filter.to_lowercase();
    let mut shown = 0;
    let n = vars.len();
    // The cell being edited: a 1.5 px accent ring on the surface, inset.
    let ring = |ui: &Ui, slot: egui::layers::ShapeIdx, cell: Rect| {
        let r = Rect::from_min_max(
            pos2(cell.left() + 5.0, cell.top() + 3.0),
            pos2(cell.right() - 5.0, cell.bottom() - 4.0),
        );
        ui.painter().set(slot, egui::Shape::rect_filled(r, CornerRadius::same(4), p.surface));
        ui.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.5, p.accent), StrokeKind::Inside);
    };
    for i in 0..=n {
        let is_new = i == n;
        if !is_new && !needle.is_empty() && !vars[i].key.to_lowercase().contains(&needle) {
            continue;
        }
        if !is_new {
            shown += 1;
        }
        let mut v = if is_new { Variable::new("", "") } else { vars[i].clone() };
        let mut row_changed = false;
        let row_rect = Rect::from_min_size(pos2(left, y), vec2(width, ROW));
        ui.painter().hline(left..=right, y, Stroke::new(1.0, p.border));
        let menu_id = salt.with(("row-menu", i));
        if !is_new && auto_menu.as_deref() == Some(v.key.as_str()) {
            egui::Popup::open_id(ui.ctx(), menu_id);
            ui.data_mut(|d| d.remove_temp::<String>(auto_menu_id()));
        }
        let menu_open = !is_new && egui::Popup::is_id_open(ui.ctx(), menu_id);
        let hovered = !is_new && (ui.rect_contains_pointer(row_rect) || menu_open);
        if hovered {
            ui.painter().rect_filled(row_rect.shrink2(vec2(1.0, 0.0)).with_min_y(y + 0.5), 0.0, p.well);
        }
        // A cell spans its column between the hairlines.
        let cell = |x0: f32, x1: f32| Rect::from_min_max(pos2(x0 + 1.0, y + 1.0), pos2(x1, y + ROW));
        let cy = row_rect.center().y + 0.5;
        ui.push_id(i, |ui| {
            if is_new {
                ui.painter().text(
                    pos2(left + CHECK / 2.0, cy),
                    Align2::CENTER_CENTER,
                    icons::PLUS,
                    theme::icon_font(13.0),
                    p.text_faint,
                );
            } else {
                let c = Rect::from_center_size(pos2(left + CHECK / 2.0, cy), vec2(15.0, 15.0));
                let r = ui.put(c, |ui: &mut Ui| kit::check(ui, &mut v.enabled, ""));
                widgets::a11y_name(&r, &format!("{} {}", tr("Enabled"), v.key));
                row_changed |= r.changed();
            }
            let faded = !is_new && !v.enabled;
            // Name
            let key_rect = cell(xs[0], xs[1]);
            let slot = ui.painter().add(egui::Shape::Noop);
            let hint = if is_new {
                RichText::new(tr("Add a new variable")).font(FontId::proportional(13.0)).color(p.text_faint)
            } else {
                RichText::new("")
            };
            let r = ui
                .scope_builder(UiBuilder::new().max_rect(key_rect), |ui| {
                    if faded {
                        ui.multiply_opacity(0.6);
                    }
                    let color = if faded { p.text_faint } else { p.text };
                    ui.add(frameless(&mut v.key, key_rect, &mono, hint, color))
                })
                .inner;
            widgets::a11y_name(&r, if is_new { tr("Add a new variable") } else { tr("Variable") });
            if r.has_focus() {
                ring(ui, slot, key_rect);
            }
            row_changed |= r.changed();
            if !is_new {
                row_changed |= variable_cells(ui, &mut v, &xs, y, faded, ctx);
                // Row actions: more, delete.
                if hovered {
                    let trash = Rect::from_center_size(pos2(right - 6.0 - 11.0, cy - 0.5), vec2(22.0, 22.0));
                    let dots = trash.translate(vec2(-22.0, 0.0));
                    let m = ui.put(dots, |ui: &mut Ui| {
                        kit::icon_button_ex(ui, icons::DOTS_THREE, tr("More actions"), menu_open, 22.0)
                    });
                    egui::Popup::menu(&m).id(menu_id).width(250.0).show(|ui| {
                        row_changed |= variable_menu(ui, &mut v);
                    });
                    let d = ui.put(trash, |ui: &mut Ui| kit::icon_button_sm(ui, icons::TRASH, tr("Delete")));
                    if d.clicked() {
                        remove = Some(i);
                    }
                }
            }
        });
        y += ROW;
        if row_changed {
            changed = true;
            if is_new {
                if !v.key.is_empty() {
                    vars.push(v);
                }
            } else {
                vars[i] = v;
            }
        }
    }
    let table = Rect::from_min_max(pos2(left, top), pos2(right, y));
    ui.painter().set(bg, egui::Shape::rect_filled(table, CornerRadius::same(9), p.surface));
    for x in &xs[..4] {
        ui.painter().vline(*x, table.y_range(), Stroke::new(1.0, p.border));
    }
    ui.painter().rect_stroke(table, CornerRadius::same(9), Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.advance_cursor_after_rect(table);
    if !needle.is_empty() && shown == 0 {
        ui.add_space(8.0);
        ui.label(widgets::muted(ui, trf("No variables match “{}”.", &[&filter])));
    }
    if let Some(i) = remove {
        vars.remove(i);
        changed = true;
    }
    changed
}

fn auto_menu_id() -> egui::Id {
    egui::Id::unique("var-row-menu-auto")
}

/// Automation: opens the menu of the variable row `name` of the variables
/// table shown next.
pub fn automation_row_menu(ctx: &egui::Context, name: String) {
    ctx.data_mut(|d| d.insert_temp(auto_menu_id(), name));
}

/// Type, initial and current value of a row of the variables table.
fn variable_cells(ui: &mut Ui, v: &mut Variable, xs: &[f32; 5], y: f32, faded: bool, ctx: &VarCtx) -> bool {
    let p = theme::pal(ui);
    const ROW: f32 = 36.0;
    let mut changed = false;
    let cell = |x0: f32, x1: f32| Rect::from_min_max(pos2(x0 + 1.0, y + 1.0), pos2(x1, y + ROW));
    let ring = |ui: &Ui, slot: egui::layers::ShapeIdx, cell: Rect| {
        let r = Rect::from_min_max(
            pos2(cell.left() + 5.0, cell.top() + 3.0),
            pos2(cell.right() - 5.0, cell.bottom() - 4.0),
        );
        ui.painter().set(slot, egui::Shape::rect_filled(r, CornerRadius::same(4), p.surface));
        ui.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.5, p.accent), StrokeKind::Inside);
    };
    // Type: a quiet select box filling the cell.
    let secret = v.kind == VarKind::Secret;
    let kind_rect = Rect::from_min_max(pos2(xs[1] + 10.0, y), pos2(xs[2] - 10.0, y + ROW));
    let (icon, label) = if secret { (icons::LOCK_SIMPLE, tr("secret")) } else { (icons::TEXT_AA, tr("default")) };
    let r = ui
        .scope_builder(UiBuilder::new().max_rect(kind_rect).layout(Layout::left_to_right(Align::Center)), |ui| {
            if faded {
                ui.multiply_opacity(0.6);
            }
            kit::select_box_ex(ui, Some(icon), None, label, kind_rect.width(), true, true)
        })
        .inner;
    widgets::a11y_name(&r, &format!("{}: {label}", tr("Type")));
    egui::Popup::menu(&r).width(kind_rect.width().max(150.0)).show(|ui| {
        for (k, icon, label) in
            [(VarKind::Default, icons::TEXT_AA, tr("default")), (VarKind::Secret, icons::LOCK_SIMPLE, tr("secret"))]
        {
            let item = MenuItem { icon: Some(icon), label, selected: v.kind == k, ..Default::default() };
            if kit::menu_item_ex(ui, item).clicked() && v.kind != k {
                v.kind = k;
                changed = true;
            }
        }
    });
    // Initial value
    let init_rect = cell(xs[2], xs[3]);
    let slot = ui.painter().add(egui::Shape::Noop);
    let r = ui
        .scope_builder(UiBuilder::new().max_rect(init_rect), |ui| {
            if faded {
                ui.multiply_opacity(0.35);
            }
            if secret {
                secret_cell(ui, "init", &mut v.value, init_rect, false)
            } else {
                ui.style_mut().text_styles.insert(egui::TextStyle::Monospace, theme::mono(12.0));
                widgets::var_cell(ui, "init", &mut v.value, ctx, "", init_rect.width())
            }
        })
        .inner;
    widgets::a11y_name(&r, tr("Initial value"));
    if r.has_focus() {
        ring(ui, slot, init_rect);
    }
    changed |= r.changed();
    // Current value: the initial one as placeholder; a dot when it differs.
    let mut cur = v.current.clone().unwrap_or_default();
    let differs = !secret && !cur.is_empty() && cur != v.value;
    let cur_cell = cell(xs[3], xs[4]);
    let cur_rect = if differs { cur_cell.with_max_x(cur_cell.right() - 16.0) } else { cur_cell };
    let slot = ui.painter().add(egui::Shape::Noop);
    let r = ui
        .scope_builder(UiBuilder::new().max_rect(cur_rect), |ui| {
            if faded {
                ui.multiply_opacity(0.35);
            }
            if secret {
                secret_cell(ui, "cur", &mut cur, cur_rect, !v.value.is_empty())
            } else {
                ui.style_mut().text_styles.insert(egui::TextStyle::Monospace, theme::mono(12.0));
                let r = widgets::var_cell(ui, "cur", &mut cur, ctx, "", cur_rect.width());
                // The initial value as placeholder, in the font of the values.
                if cur.is_empty() && !r.has_focus() && !v.value.is_empty() {
                    let g = kit::elide(ui, &v.value, theme::mono(12.0), p.text_faint, cur_rect.width() - 20.0);
                    let at = pos2(cur_rect.left() + 9.0, cur_rect.center().y - g.size().y / 2.0);
                    ui.painter().galley(at, g, p.text_faint);
                }
                r
            }
        })
        .inner;
    widgets::a11y_name(&r, tr("Current value"));
    if r.has_focus() {
        ring(ui, slot, cur_cell);
    }
    if r.changed() {
        v.current = if cur.is_empty() { None } else { Some(cur) };
        changed = true;
    }
    if differs {
        let c = pos2(cur_cell.right() - 10.0 - 3.0, cur_cell.center().y);
        ui.painter().circle_filled(c, 3.0, p.accent);
        ui.interact(Rect::from_center_size(c, vec2(12.0, 12.0)), ui.scope_id().with("diff"), Sense::hover())
            .on_hover_text(tr("Differs from the initial value"));
    }
    changed
}

/// The menu of a variable row: persist or reset the current value, copy
/// the reference. Returns true when the variable changed.
fn variable_menu(ui: &mut Ui, v: &mut Variable) -> bool {
    let mut changed = false;
    kit::menu_header(ui, &v.key);
    ui.add_enabled_ui(v.current.is_some(), |ui| {
        if kit::menu_item(ui, Some(icons::ARROW_LINE_DOWN), tr("Persist current value as initial"), None).clicked()
            && let Some(c) = v.current.take()
        {
            v.value = c;
            changed = true;
        }
        let back: String = v.value.chars().take(18).collect();
        let detail = (!v.is_secret() && !back.is_empty()).then(|| trf("back to {}", &[&back]));
        let reset = MenuItem {
            icon: Some(icons::ARROW_COUNTER_CLOCKWISE),
            label: tr("Reset current value"),
            detail: detail.as_deref(),
            ..Default::default()
        };
        if kit::menu_item_ex(ui, reset).clicked() {
            v.current = None;
            changed = true;
        }
    });
    kit::menu_sep(ui);
    let reference = format!("{{{{{}}}}}", v.key);
    if kit::menu_item(ui, Some(icons::COPY), &trf("Copy {}", &[&reference]), None).clicked() {
        ui.ctx().copy_text(reference);
    }
    changed
}

/// A frameless single-line cell editor.
fn frameless<'t>(
    text: &'t mut String,
    rect: Rect,
    font: &FontId,
    hint: impl Into<egui::WidgetText>,
    color: Color32,
) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .hint_text(hint)
        .font(font.clone())
        .text_color(color)
        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(9, 0)))
        .min_size(vec2(0.0, rect.height()))
        .align(Align2::LEFT_CENTER)
        .desired_width(rect.width())
}

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

/// A hint with a `{{name}}` chip in the secret style (info on info soft)
/// in place of the `{}` of `text`.
fn chip_hint(ui: &Ui, text: &str, chip: &str) -> LayoutJob {
    let p = theme::pal(ui);
    let (before, after) = text.split_once("{}").unwrap_or((text, ""));
    let mut job = LayoutJob::default();
    let text = TextFormat::simple(FontId::proportional(12.0), p.text_muted);
    job.append(before, 0.0, text.clone());
    let mut c = TextFormat::simple(theme::mono(11.5), p.info);
    c.background = p.info_soft;
    job.append(chip, 0.0, c);
    job.append(after, 0.0, text);
    job
}

/// The two help cards under the table: the vault, and switching quickly.
fn help_cards(app: &mut TernaryWireApp, ui: &mut Ui) {
    let p = theme::pal(ui);
    let gap = 10.0;
    let side = ui.available_width() >= 560.0;
    let w = if side { ((ui.available_width() - gap) / 2.0).floor() } else { ui.available_width() };
    let h_id = ui.scope_id().with("env-help-h");
    let h: f32 = ui.data(|d| d.get_temp(h_id).unwrap_or(0.0));
    // A card: its icon (14, muted) centered on the content, 10 before it.
    let card = |ui: &mut Ui, icon: &str, add: &mut dyn FnMut(&mut Ui)| -> f32 {
        kit::card(ui)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(w - 30.0);
                let row = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.add_space(24.0);
                        add(ui)
                    })
                    .response
                    .rect;
                let at = pos2(row.left() + 7.0, row.center().y);
                ui.painter().text(at, Align2::CENTER_CENTER, icon, theme::icon_font(14.0), p.text_muted);
            })
            .response
            .rect
            .height()
    };
    let mut open_vault = false;
    let mut vault = |ui: &mut Ui| {
        let text_w = (ui.available_width() - 100.0).max(80.0);
        ui.allocate_ui_with_layout(vec2(text_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(tr("Secrets live in the Keychain")).font(theme::medium(13.0)).color(p.text));
            let job = chip_hint(ui, tr("Use {} for values shared across environments."), "{{vault:name}}");
            ui.add(egui::Label::new(job).wrap());
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Open vault"), Default::default()).clicked() {
                open_vault = true;
            }
        });
    };
    let mut switch = |ui: &mut Ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(tr("Switch quickly")).font(theme::medium(13.0)).color(p.text));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                kit::kbd(ui, "⌘");
                kit::kbd(ui, "E");
                ui.add_space(2.0);
                ui.label(kit::hint(ui, tr("anywhere, or double-click an environment in the sidebar.")));
            });
        });
    };
    if side {
        // Vertically centered on each other, like the prototype's row.
        let top = ui.cursor().top();
        let left = ui.cursor().left();
        let mut heights = [0.0f32; 2];
        let cards = [(icons::LOCK_KEY, &mut vault as &mut dyn FnMut(&mut Ui)), (icons::ARROWS_LEFT_RIGHT, &mut switch)];
        for (k, (icon, add)) in cards.into_iter().enumerate() {
            let prev: f32 = ui.data(|d| d.get_temp(h_id.with(k)).unwrap_or(h));
            let dy = ((h - prev) / 2.0).max(0.0);
            let rect = Rect::from_min_size(pos2(left + k as f32 * (w + gap), top + dy), vec2(w, 400.0));
            heights[k] = ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| card(ui, icon, add)).inner;
            ui.data_mut(|d| d.insert_temp(h_id.with(k), heights[k]));
        }
        let max = heights[0].max(heights[1]);
        if (max - h).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(h_id, max));
            ui.ctx().request_repaint();
        }
        ui.advance_cursor_after_rect(Rect::from_min_size(pos2(left, top), vec2(w * 2.0 + gap, max)));
    } else {
        card(ui, icons::LOCK_KEY, &mut vault);
        ui.add_space(gap);
        card(ui, icons::ARROWS_LEFT_RIGHT, &mut switch);
    }
    if open_vault {
        app.dialog = Some(Dialog::vault());
    }
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut EnvTab) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.y = 0.0;
    let index = tab.env.as_ref().and_then(|id| app.ws.data.environments.iter().position(|e| &e.id == id));
    let (title, count) = match (&tab.env, index) {
        (None, _) => (tr("Globals").to_string(), app.ws.data.globals.len()),
        (Some(_), Some(i)) => (app.ws.data.environments[i].name.clone(), app.ws.data.environments[i].variables.len()),
        (Some(_), None) => {
            kit::empty_state(
                ui,
                icons::GLOBE,
                tr("This environment was deleted."),
                tr("Pick another one in the Environments list."),
                |_| {},
            );
            return;
        }
    };
    let active = tab.env.as_ref().is_some_and(|id| app.ws.data.active_environment.as_deref() == Some(id.as_str()));
    let mut acts: Vec<EnvAct> = Vec::new();
    let subtitle = match &tab.env {
        None => trf("Global variables · {} variables", &[&count]),
        Some(_) => trf("Environment · {} variables", &[&count]),
    };
    let tint = index.map(|i| env_color(ui, i));
    let name = title_row(
        ui,
        icons::GLOBE_SIMPLE,
        tint,
        &subtitle,
        |ui| {
            if let Some(id) = &tab.env {
                let more = kit::icon_button(ui, icons::DOTS_THREE, tr("More actions"));
                egui::Popup::menu(&more)
                    .width(200.0)
                    .align(egui::RectAlign::BOTTOM_END)
                    .show(|ui| env_menu(ui, id, &title, active, &mut acts));
                if active {
                    kit::pill_ex(
                        ui,
                        tr("Active"),
                        Tone::Ok,
                        kit::PillOpts { icon: Some(icons::CHECK), ..Default::default() },
                    )
                    .on_hover_text(tr("Requests resolve variables from this environment"));
                } else if kit::button(
                    ui,
                    kit::Kind::Secondary,
                    kit::Size::Sm,
                    tr("Set active"),
                    kit::ButtonOpts { icon: Some(icons::CHECK_CIRCLE), ..Default::default() },
                )
                .clicked()
                {
                    acts.push(EnvAct::Activate(Some(id.clone())));
                }
            }
        },
        |ui| title_label(ui, &title),
    );
    if let Some(id) = &tab.env {
        let name = name.on_hover_text(tr("Double-click to rename"));
        if name.double_clicked() {
            acts.push(EnvAct::Rename(id.clone(), title.clone()));
        }
    }
    // Filter, and where the values go (one line of help, always shown).
    kit::toolbar(ui, |ui| {
        let w = (ui.available_width() * 0.4).clamp(140.0, 280.0);
        let r = kit::search_field_sm(ui, "env-filter", &mut tab.search, tr("Filter variables"), w);
        widgets::a11y_name(&r, tr("Filter variables"));
        let mut job = LayoutJob::default();
        let mut icon = TextFormat::simple(theme::icon_font(13.0), p.text_muted);
        icon.valign = Align::Center;
        job.append(icons::INFO, 0.0, icon);
        job.append(
            tr("Initial values are saved in the workspace and can be committed to Git; current values and secrets stay on this machine."),
            6.0,
            TextFormat::simple(FontId::proportional(12.0), p.text_muted),
        );
        job.wrap.max_rows = 2;
        job.wrap.max_width = ui.available_width();
        ui.add(egui::Label::new(job).wrap());
    });
    let ctx = app.var_ctx(None);
    egui::ScrollArea::vertical().id_salt("env-vars").auto_shrink([false; 2]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 4, bottom: 16 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &tab.env {
                None => {
                    let mut vars = app.ws.data.globals.clone();
                    if variables_table(ui, "globals", &mut vars, &tab.search, &ctx) {
                        app.ws.data.globals = vars;
                        app.ws.mark_globals();
                    }
                }
                Some(id) => {
                    let id = id.clone();
                    if let Some(env) = app.ws.data.environments.iter_mut().find(|e| e.id == id) {
                        let mut vars = env.variables.clone();
                        if variables_table(ui, ("env", &id), &mut vars, &tab.search, &ctx) {
                            env.variables = vars;
                            app.ws.mark_env(&id);
                        }
                    }
                }
            }
            ui.add_space(14.0);
            help_cards(app, ui);
        });
    });
    for a in acts {
        apply(app, a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_edit_keeps_secrets_and_current_values() {
        let mut a = Variable::new("a", "1");
        a.current = Some("2".into());
        let s = Variable::secret("token", "x");
        let mut c = Variable::new("c", "3");
        c.enabled = false;
        let vars = vec![a, s, c];
        let text = bulk_format(&vars);
        assert_eq!(text, "a:1\n//c:3");
        let merged = bulk_merge("a:5\n//c:3\nd:4", &vars);
        let keys: Vec<&str> = merged.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(keys, ["a", "token", "c", "d"]);
        assert_eq!(merged[0].value, "5");
        assert_eq!(merged[0].current.as_deref(), Some("2"));
        assert_eq!(merged[1].value, "x");
        assert!(!merged[2].enabled);
    }
}
