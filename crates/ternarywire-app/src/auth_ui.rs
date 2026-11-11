//! Auth editor shared by requests, folders and collections. Forms are built
//! from `ternarywire_auth::fields`, so every scheme gets an editor for free.
//!
//! Layout (the redesign's `http-auth`): a toolbar row with "Auth type" and
//! its drop-down (plus the token state for OAuth 2.0), then the form: the
//! token card, the fields in a two-column grid, switches as full rows and
//! the advanced fields in a collapsed "Advanced (n)".

use egui::{Align, Align2, CornerRadius, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use ternarywire_auth::{FieldKind, FieldSpec};
use ternarywire_core::auth::{Auth, AuthType};
use ternarywire_core::tokens::OAuth2Token;

use crate::i18n::{tr, trf};
use crate::icons;
use crate::kit::{self, ButtonOpts, Kind, PillOpts, Size, Tone};
use crate::theme;
use crate::widgets::{self, VarCtx};

pub struct AuthUiCtx<'a> {
    /// Collection level: `Inherit` is not offered.
    pub is_root: bool,
    /// Where the inherited auth comes from (name, type), for display.
    pub inherited: Option<(String, AuthType)>,
    /// Key used to store OAuth 2.0 tokens for this auth.
    pub owner_id: &'a str,
    pub token: Option<OAuth2Token>,
    pub fetching: bool,
    pub vars: &'a VarCtx,
}

pub enum AuthAction {
    None,
    GetToken,
    ClearToken,
}

/// What [`auth_form`] did.
pub struct AuthOut {
    pub changed: bool,
    pub action: AuthAction,
    /// "Open collection auth" was pressed on the inherited auth.
    pub open_parent: bool,
}

/// What the signers in ternarywire-auth use when a parameter is missing (as in
/// imported collections). New configs get `f.default` written, which can
/// differ from this fallback.
fn missing_value(kind: AuthType, f: &FieldSpec) -> &'static str {
    match (kind, f.key) {
        // Signed into the URL or body unless set.
        (AuthType::OAuth1, "addParamsToHeader") => "false",
        // The redirect is received on a random 127.0.0.1 port.
        (AuthType::OAuth2, "redirect_uri") => "",
        _ => f.default,
    }
}

fn value_text(auth: &Auth, f: &FieldSpec) -> String {
    match auth.params.get(f.key) {
        Some(_) => auth.get_string(f.key),
        None => missing_value(auth.kind, f).to_string(),
    }
}

fn bool_value(auth: &Auth, f: &FieldSpec) -> bool {
    auth.get_bool(f.key, missing_value(auth.kind, f) == "true")
}

/// One sentence about each auth type, shown above its form.
fn describe(kind: AuthType) -> &'static str {
    match kind {
        AuthType::Inherit => tr("Use the auth of the parent folder or collection."),
        AuthType::None => tr("No authorization is sent with this request."),
        AuthType::ApiKey => tr("Send a key and value as a header or query parameter."),
        AuthType::Bearer => tr("Send a token in the Authorization header."),
        AuthType::Basic => tr("Send a username and password in the Authorization header."),
        AuthType::Digest => tr("Answer the server's challenge with a hashed password."),
        AuthType::Jwt => tr("Sign a JSON Web Token and send it with the request."),
        AuthType::OAuth1 => tr("Sign the request with OAuth 1.0 consumer and token credentials."),
        AuthType::OAuth2 => tr("Gets a token from an authorization server and sends it with the request."),
        AuthType::Hawk => tr("Sign the request with Hawk credentials."),
        AuthType::AwsV4 => tr("Sign the request with AWS Signature Version 4."),
        AuthType::Ntlm => tr("Windows NTLM challenge-response authentication."),
        AuthType::EdgeGrid => tr("Sign the request for Akamai EdgeGrid APIs."),
        AuthType::Asap => tr("Sign a token with the Atlassian S2S Authentication Protocol."),
    }
}

/// A field-like button that opens a menu (`Popup::menu` on the response).
#[allow(dead_code)] // Shared with the other views.
pub(crate) fn select_button(ui: &mut Ui, icon: Option<&str>, text: &str, width: f32) -> egui::Response {
    kit::select_box(ui, icon, text, width, false)
}

/// An on/off switch. Returns a response that is `changed()` when toggled.
pub(crate) fn switch(ui: &mut Ui, on: &mut bool, name: &str) -> egui::Response {
    kit::switch(ui, on, name)
}

/// A masked single-line input with a show/hide eye inside it. `framed`
/// false draws no frame (table cells); framed it is 26 high (`.input.sm`).
pub(crate) fn secret_input(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    width: f32,
    framed: bool,
    hint: &str,
) -> egui::Response {
    let p = theme::pal(ui);
    let id = ui.make_persistent_id(id);
    let reveal_id = id.with("reveal");
    let mut reveal: bool = ui.data(|d| d.get_temp(reveal_id).unwrap_or(false));
    let mut edit = egui::TextEdit::singleline(text)
        .id(id)
        .password(!reveal)
        .hint_text(RichText::new(hint).color(p.text_faint))
        .desired_width(width)
        .font(theme::mono(12.0));
    if framed {
        edit = edit
            .margin(egui::Margin { left: 8, right: 30, top: 0, bottom: 0 })
            .min_size(vec2(0.0, 26.0))
            .align(Align2::LEFT_CENTER);
    } else {
        let h = ui.available_height().clamp(20.0, 34.0);
        edit = edit
            // egui ignores `margin` when a frame is given: it goes in the frame.
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 6, right: 28, top: 2, bottom: 2 }))
            .min_size(vec2(0.0, h))
            .align(Align2::LEFT_CENTER);
    }
    let r = ui.scope(|ui| {
        let radius = CornerRadius::same(theme::RADIUS_SM);
        let w = &mut ui.visuals_mut().widgets;
        for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            v.corner_radius = radius;
        }
        ui.add(edit)
    });
    let r = r.inner;
    let eye = Rect::from_center_size(pos2(r.rect.right() - 13.0, r.rect.center().y), vec2(22.0, 22.0));
    let e = ui.interact(eye, id.with("eye"), Sense::click());
    let label = if reveal { tr("Hide value") } else { tr("Show value") };
    widgets::a11y_button(&e, label);
    if e.hovered() {
        ui.painter().rect_filled(eye, CornerRadius::same(5), p.hover);
    }
    let color = if e.hovered() { p.text } else { p.text_muted };
    ui.painter().text(
        eye.center(),
        Align2::CENTER_CENTER,
        if reveal { icons::EYE_SLASH } else { icons::EYE },
        theme::icon_font(13.0),
        color,
    );
    if e.on_hover_text(label).clicked() {
        reveal = !reveal;
        ui.data_mut(|d| d.insert_temp(reveal_id, reveal));
    }
    r
}

/// A 26 px framed input with `{{variable}}` chips (`.input.sm.mono`).
fn var_input(ui: &mut Ui, id: &str, text: &mut String, vars: &VarCtx, width: f32) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface);
    // `.input.sm.mono`: 12 px, 8 px padding (text is cut before the right one).
    let text_rect = rect.with_max_x(rect.right() - 8.0);
    let mut cell =
        ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(Layout::left_to_right(Align::Center)));
    cell.set_clip_rect(text_rect.intersect(ui.clip_rect()));
    cell.style_mut().text_styles.insert(egui::TextStyle::Monospace, theme::mono(12.0));
    let r = widgets::var_cell(&mut cell, id, text, vars, "", text_rect.width());
    if r.has_focus() {
        kit::focus_ring(ui, rect, theme::RADIUS_SM);
    } else {
        let c = if ui.rect_contains_pointer(rect) { p.text_faint } else { p.border_strong };
        ui.painter().rect_stroke(rect, CornerRadius::same(theme::RADIUS_SM), Stroke::new(1.0, c), StrokeKind::Inside);
    }
    r
}

/// The control of one field, `width` wide.
fn field_control(ui: &mut Ui, auth: &mut Auth, f: &FieldSpec, vars: &VarCtx, width: f32) -> bool {
    let mut changed = false;
    match f.kind {
        FieldKind::Text => {
            let mut v = value_text(auth, f);
            let r = var_input(ui, f.key, &mut v, vars, width);
            widgets::a11y_name(&r, tr(f.label));
            if r.changed() {
                auth.set(f.key, v);
                changed = true;
            }
        }
        FieldKind::Secret => {
            let mut v = value_text(auth, f);
            let is_ref = ternarywire_core::variables::is_pure_reference(&v);
            let r = if is_ref {
                var_input(ui, f.key, &mut v, vars, width)
            } else {
                secret_input(ui, f.key, &mut v, width, true, "")
            };
            widgets::a11y_name(&r, tr(f.label));
            if r.changed() {
                auth.set(f.key, v);
                changed = true;
            }
        }
        FieldKind::Multiline => {
            let mut v = value_text(auth, f);
            let lang = if f.key.to_lowercase().contains("key") {
                crate::highlight::Lang::Text
            } else {
                crate::highlight::Lang::Json
            };
            ui.scope(|ui| {
                ui.set_width(width);
                kit::well(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if widgets::code_editor_gutter(ui, f.key, &mut v, lang, Some(vars), 3, false, None).changed() {
                        auth.set(f.key, v);
                        changed = true;
                    }
                });
            });
        }
        FieldKind::Bool => {
            let mut b = bool_value(auth, f);
            if switch(ui, &mut b, tr(f.label)).changed() {
                auth.set(f.key, b);
                changed = true;
            }
        }
        FieldKind::Select(options) => {
            let current = value_text(auth, f);
            // A few short options are a segmented control (on a track as
            // wide as the field), others a drop-down.
            let labels: Vec<&str> = options.iter().map(|(v, l)| option_label(f.key, v, l)).collect();
            let short = options.len() <= 3 && labels.iter().all(|l| l.chars().count() <= 12);
            if short {
                let indexed: Vec<(usize, &str)> = labels.iter().enumerate().map(|(i, l)| (i, *l)).collect();
                let mut sel = options.iter().position(|(v, _)| *v == current).unwrap_or(0);
                let p = theme::pal(ui);
                let track = Rect::from_min_size(ui.cursor().min, vec2(width, 26.0));
                ui.painter().rect_filled(track, CornerRadius::same(8), p.subtle);
                let r = ui.scope(|ui| kit::segmented_sm(ui, &mut sel, &indexed));
                widgets::a11y_name(&r.response, tr(f.label));
                if r.inner {
                    auth.set(f.key, options[sel].0.to_string());
                    changed = true;
                }
            } else {
                let label = options
                    .iter()
                    .find(|(v, _)| *v == current)
                    .map(|(_, l)| tr(l))
                    .unwrap_or(current.as_str())
                    .to_string();
                let r = kit::select_box(ui, None, &label, width, true);
                widgets::a11y_name(&r, tr(f.label));
                egui::Popup::menu(&r).width(width.max(200.0)).show(|ui| {
                    for (v, l) in options.iter() {
                        let item = kit::MenuItem { checked: Some(current == *v), label: tr(l), ..Default::default() };
                        if kit::menu_item_ex(ui, item).clicked() {
                            auth.set(f.key, v.to_string());
                            changed = true;
                        }
                    }
                });
            }
        }
    }
    changed
}

/// The label of an option in a segmented control: short forms of the long
/// names the drop-downs use.
fn option_label(key: &str, value: &str, label: &'static str) -> &'static str {
    match (key, value) {
        ("addTokenTo", "header") => tr("Header"),
        ("addTokenTo", "queryParams") => tr("URL"),
        _ => tr(label),
    }
}

/// The order of the OAuth 2.0 fields: the authorization server first, then
/// the token as sent. Credentials of one grant show only with that grant
/// (or when they are set).
fn oauth2_fields<'a>(auth: &Auth, fields: Vec<&'a FieldSpec>) -> Vec<&'a FieldSpec> {
    const ORDER: &[&str] = &[
        "grant_type",
        "addTokenTo",
        "authUrl",
        "accessTokenUrl",
        "clientId",
        "clientSecret",
        "scope",
        "redirect_uri",
        "challengeAlgorithm",
        "username",
        "password",
        "accessToken",
        "headerPrefix",
        "tokenName",
    ];
    let grant = auth.get_string("grant_type");
    let shown = |f: &FieldSpec| match f.key {
        "username" | "password" => grant.starts_with("password") || !auth.get_string(f.key).is_empty(),
        "challengeAlgorithm" => grant.ends_with("pkce") || auth.params.contains_key(f.key),
        _ => true,
    };
    let rank = |f: &FieldSpec| ORDER.iter().position(|k| *k == f.key).unwrap_or(ORDER.len());
    let mut out: Vec<&FieldSpec> = fields.into_iter().filter(|f| shown(f)).collect();
    out.sort_by_key(|f| rank(f));
    out
}

/// One field: label above the control, help under it (`.field`).
fn field_ui(ui: &mut Ui, auth: &mut Auth, f: &FieldSpec, vars: &VarCtx, width: f32) -> bool {
    ui.push_id(f.key, |ui| {
        if f.kind == FieldKind::Bool {
            return switch_row(ui, auth, f, width);
        }
        kit::field(ui, tr(f.label), "", |ui| {
            let changed = field_control(ui, auth, f, vars, width);
            if !f.help.is_empty() {
                ui.scope(|ui| {
                    ui.set_max_width(width);
                    ui.add(egui::Label::new(help_text(ui, tr(f.help))).wrap());
                });
            }
            changed
        })
    })
    .inner
}

/// Field help (11.5, muted) with `code` in the mono font.
fn help_text(ui: &Ui, text: &str) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let muted = theme::pal(ui).text_muted;
    let plain = TextFormat::simple(egui::FontId::proportional(11.5), muted);
    let mono = TextFormat { valign: Align::Center, ..TextFormat::simple(theme::mono(12.5), muted) };
    let mut job = LayoutJob::default();
    for (i, part) in text.split('`').enumerate() {
        job.append(part, 0.0, if i % 2 == 1 { mono.clone() } else { plain.clone() });
    }
    job
}

/// An on/off field as a full row: label and help on the left, the switch
/// on the right.
fn switch_row(ui: &mut Ui, auth: &mut Auth, f: &FieldSpec, width: f32) -> bool {
    let p = theme::pal(ui);
    let mut changed = false;
    // The switch centers on the text (its height from the last frame).
    let h_id = ui.scope_id().with((f.key, "switch-row-h"));
    let h: f32 = ui.data(|d| d.get_temp(h_id).unwrap_or(13.0 * 1.45));
    ui.allocate_ui_with_layout(vec2(width, h), Layout::left_to_right(Align::Center), |ui| {
        ui.set_width(width);
        ui.set_min_height(h);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut b = bool_value(auth, f);
            if switch(ui, &mut b, tr(f.label)).changed() {
                auth.set(f.key, b);
                changed = true;
            }
            ui.add_space(8.0);
            // Lines 1.45 high, as in the prototype.
            let text = ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let label = RichText::new(tr(f.label)).font(theme::medium(13.0)).color(p.text);
                ui.label(label.line_height(Some(13.0 * 1.45)));
                if !f.help.is_empty() {
                    ui.add(egui::Label::new(kit::hint_block(ui, tr(f.help))).wrap());
                }
            });
            let text_h = text.response.rect.height();
            if (text_h - h).abs() > 0.5 {
                ui.data_mut(|d| d.insert_temp(h_id, text_h));
                ui.ctx().request_discard("switch row height");
            }
        });
    });
    changed
}

/// Fields in a two-column grid (12 apart); switches and multi-line fields
/// take a whole row. One column when narrow.
fn fields_grid(ui: &mut Ui, auth: &mut Auth, fields: &[&FieldSpec], vars: &VarCtx) -> bool {
    let mut changed = false;
    let width = ui.available_width();
    let two = width >= 420.0 && fields.iter().filter(|f| is_cell(f)).count() > 1;
    let col = if two { ((width - 12.0) / 2.0).floor() } else { width };
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 12.0;
        let mut pending: Vec<&FieldSpec> = Vec::new();
        let flush = |ui: &mut Ui, auth: &mut Auth, pending: &mut Vec<&FieldSpec>| {
            let mut changed = false;
            for pair in pending.chunks(if two { 2 } else { 1 }) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for f in pair {
                        ui.allocate_ui_with_layout(vec2(col, 0.0), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(col);
                            changed |= field_ui(ui, auth, f, vars, col);
                        });
                    }
                });
            }
            pending.clear();
            changed
        };
        for f in fields {
            if is_cell(f) {
                pending.push(f);
            } else {
                let after_grid = !pending.is_empty();
                changed |= flush(ui, auth, &mut pending);
                // A full row is a block of its own, 14 under the grid.
                if after_grid {
                    ui.add_space(2.0);
                }
                changed |= field_ui(ui, auth, f, vars, width);
            }
        }
        changed |= flush(ui, auth, &mut pending);
    });
    changed
}

/// Fields that sit in a grid cell (the others take a whole row).
fn is_cell(f: &FieldSpec) -> bool {
    !matches!(f.kind, FieldKind::Bool | FieldKind::Multiline)
}

/// "Auth type" and its drop-down, then (OAuth 2.0) the token state on the
/// right: the content of the auth toolbar. Returns true when the type
/// changed.
pub(crate) fn type_bar(ui: &mut Ui, auth: &mut Auth, ctx: &AuthUiCtx<'_>) -> bool {
    let mut changed = false;
    ui.label(kit::section_title(tr("Auth type")));
    let label = tr(auth.kind.label());
    let tw = ui
        .painter()
        .layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), egui::Color32::PLACEHOLDER)
        .size()
        .x;
    let r = kit::select_box(ui, Some(icons::KEY), label, (8.0 + 14.0 + 6.0 + tw + 26.0).max(190.0), true);
    widgets::a11y_name(&r, tr("Auth type"));
    egui::Popup::menu(&r).width(220.0).show(|ui| {
        for t in AuthType::ALL {
            if ctx.is_root && t == AuthType::Inherit {
                continue;
            }
            let item = kit::MenuItem { checked: Some(auth.kind == t), label: tr(t.label()), ..Default::default() };
            if kit::menu_item_ex(ui, item).on_hover_text(describe(t)).clicked() && auth.kind != t {
                let mut fresh = Auth::new(t);
                // Sensible defaults for new configs.
                for f in ternarywire_auth::fields(t) {
                    if !f.default.is_empty() {
                        match f.kind {
                            FieldKind::Bool => fresh.set(f.key, f.default == "true"),
                            _ => fresh.set(f.key, f.default.to_string()),
                        }
                    }
                }
                *auth = fresh;
                changed = true;
            }
            if t == AuthType::None {
                kit::menu_sep(ui);
            }
        }
    });
    if auth.kind == AuthType::OAuth2 {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| token_pill(ui, ctx));
    }
    changed
}

/// The token state in the auth toolbar: active (with the time left) or expired.
fn token_pill(ui: &mut Ui, ctx: &AuthUiCtx<'_>) {
    let dot = PillOpts { dot: true, ..Default::default() };
    if ctx.fetching {
        kit::pill_ex(ui, tr("Getting a token…"), Tone::Info, dot);
        return;
    }
    let Some(t) = &ctx.token else { return };
    let now = chrono::Utc::now().timestamp();
    match t.expires_at {
        Some(e) if e <= now => {
            kit::pill_ex(ui, tr("Token expired"), Tone::Err, dot);
        }
        Some(e) => {
            kit::pill_ex(ui, &trf("Token active · {}", &[&short_duration(e - now)]), Tone::Ok, dot);
        }
        None => {
            kit::pill_ex(ui, tr("Token active"), Tone::Ok, dot);
        }
    }
}

/// The form of the chosen auth type (everything under the toolbar). Lays
/// its parts out 14 apart.
pub(crate) fn auth_form(ui: &mut Ui, auth: &mut Auth, ctx: &AuthUiCtx<'_>, can_open_parent: bool) -> AuthOut {
    let mut out = AuthOut { changed: false, action: AuthAction::None, open_parent: false };
    ui.spacing_mut().item_spacing.y = 14.0;
    match auth.kind {
        AuthType::Inherit => {
            out.open_parent = inherited_card(ui, ctx.inherited.as_ref(), can_open_parent);
            return out;
        }
        AuthType::None => {
            kit::banner(ui, Tone::Neutral, icons::LOCK_SIMPLE, tr("This request does not use any authorization."));
            return out;
        }
        _ => {}
    }
    // ASAP used to store `issuer`/`subject`/`audience`; Postman's names win now.
    if auth.kind == AuthType::Asap {
        for (old, new) in [("issuer", "iss"), ("subject", "sub"), ("audience", "aud")] {
            if let Some(v) = auth.params.shift_remove(old)
                && !auth.params.contains_key(new)
            {
                auth.params.insert(new.to_string(), v);
                out.changed = true;
            }
        }
    }
    // Token and username/password forms explain themselves (their field
    // help says how the value is sent).
    if !matches!(auth.kind, AuthType::Bearer | AuthType::Basic) {
        ui.add(egui::Label::new(kit::hint_block(ui, describe(auth.kind))).wrap());
    }
    if auth.kind == AuthType::OAuth2 {
        token_card(ui, ctx, &mut out.action);
    }
    let fields = ternarywire_auth::fields(auth.kind);
    // OAuth 2.0 keeps the token as sent (a pasted token, its prefix and
    // name) and the PKCE method with the other rarely changed settings.
    let tucked = |f: &FieldSpec| {
        auth.kind == AuthType::OAuth2
            && matches!(f.key, "accessToken" | "headerPrefix" | "tokenName" | "challengeAlgorithm")
    };
    let mut basic: Vec<&FieldSpec> = fields.iter().filter(|f| !f.is_advanced() && !tucked(f)).collect();
    if auth.kind == AuthType::OAuth2 {
        basic = oauth2_fields(auth, basic);
    }
    let advanced: Vec<&FieldSpec> = fields.iter().filter(|f| f.is_advanced() || tucked(f)).collect();
    if !basic.is_empty() {
        out.changed |= fields_grid(ui, auth, &basic, ctx.vars);
    }
    if !advanced.is_empty() {
        let id = ui.make_persistent_id(("auth-adv", auth.kind.as_str()));
        if advanced_header(ui, id, advanced.len()) {
            out.changed |= fields_grid(ui, auth, &advanced, ctx.vars);
        }
    }
    out
}

/// "Advanced (n)" disclosure with a note on the right. Returns whether it
/// is open.
fn advanced_header(ui: &mut Ui, id: egui::Id, count: usize) -> bool {
    let p = theme::pal(ui);
    let mut open: bool = ui.data(|d| d.get_temp(id).unwrap_or(false));
    let row = crate::request_tab::hrow(ui, 20.0, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
        ui.add(
            egui::Label::new(RichText::new(caret).font(theme::icon_font(12.0)).color(p.text_faint)).selectable(false),
        );
        ui.add(egui::Label::new(kit::section_title(tr("Advanced"))).selectable(false));
        kit::tone_pill(ui, &count.to_string(), Tone::Neutral);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(tr("Most servers work with the defaults")).size(11.0).color(p.text_faint),
                )
                .selectable(false),
            );
        });
    });
    let r = ui.interact(row.response.rect, id.with("toggle"), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tr("Advanced")));
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

/// Returns (changed, action).
pub fn auth_editor(ui: &mut Ui, auth: &mut Auth, ctx: AuthUiCtx<'_>) -> (bool, AuthAction) {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_height(kit::size::TOOLBAR);
        ui.spacing_mut().item_spacing.x = 8.0;
        changed |= type_bar(ui, auth, &ctx);
    });
    let out = ui.vertical(|ui| auth_form(ui, auth, &ctx, false)).inner;
    (changed || out.changed, out.action)
}

/// Where an inherited auth comes from. Returns true when "Open collection
/// auth" was pressed.
fn inherited_card(ui: &mut Ui, inherited: Option<&(String, AuthType)>, can_open: bool) -> bool {
    let p = theme::pal(ui);
    let mut open = false;
    kit::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            kit::icon_tile(ui, icons::SHIELD_CHECK, p.accent, 34.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                match inherited {
                    Some((name, kind)) => {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(
                                RichText::new(trf("Inherited from “{}”", &[name]))
                                    .font(theme::semibold(13.0))
                                    .color(p.text),
                            );
                            kit::tone_pill(ui, tr(kind.label()), Tone::Accent);
                        });
                        let text = format!(
                            "{} {}",
                            trf("This request uses {} from “{}”.", &[&tr(kind.label()), name]),
                            tr("Change it in the folder or collection settings.")
                        );
                        ui.add(egui::Label::new(kit::hint(ui, &text)).wrap());
                        if can_open {
                            ui.add_space(4.0);
                            let opts = ButtonOpts { icon: Some(icons::ARROW_SQUARE_OUT), ..Default::default() };
                            open =
                                kit::button(ui, Kind::Secondary, Size::Sm, tr("Open collection auth"), opts).clicked();
                        }
                    }
                    None => {
                        ui.label(RichText::new(tr("Nothing to inherit")).font(theme::semibold(13.0)).color(p.text));
                        ui.add(
                            egui::Label::new(kit::hint(
                                ui,
                                tr("No auth is set on any parent; the request is sent without auth."),
                            ))
                            .wrap(),
                        );
                    }
                }
            });
        });
    });
    open
}

/// Seconds as `45 s`, `12 min`, `3 h`, `2 d`.
fn short_duration(secs: i64) -> String {
    match secs {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86_400),
    }
}

/// The stored OAuth 2.0 token: state, value, expiry, scopes and the actions.
fn token_card(ui: &mut Ui, ctx: &AuthUiCtx<'_>, action: &mut AuthAction) {
    let p = theme::pal(ui);
    kit::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let now = chrono::Utc::now().timestamp();
        let expired = ctx.token.as_ref().and_then(|t| t.expires_at).is_some_and(|e| e <= now);
        crate::request_tab::hrow(ui, 20.0, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.label(kit::section_title(tr("Current token")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let dot = PillOpts { dot: true, ..Default::default() };
                if ctx.fetching {
                    ui.add(egui::Spinner::new().size(14.0).color(p.info));
                    kit::pill_ex(ui, tr("Waiting for the authorization server…"), Tone::Info, dot);
                } else if ctx.token.is_none() {
                    kit::pill_ex(ui, tr("No token"), Tone::Neutral, PillOpts::default());
                } else if expired {
                    kit::pill_ex(ui, tr("Expired"), Tone::Err, dot);
                } else {
                    kit::pill_ex(ui, tr("Active"), Tone::Ok, dot);
                }
            });
        });
        match &ctx.token {
            Some(t) => {
                let kind = if t.token_type.is_empty() { "Bearer" } else { &t.token_type };
                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(p.well)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        crate::request_tab::hrow(ui, 22.0, |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(RichText::new(kind).font(theme::mono(12.0)).color(p.text_faint));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if kit::icon_button_sm(ui, icons::COPY, tr("Copy token")).clicked() {
                                    ui.ctx().copy_text(t.access_token.clone());
                                }
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&t.access_token).font(theme::mono(12.0)).color(p.text),
                                        )
                                        .truncate(),
                                    );
                                });
                            });
                        });
                    });
                ui.add_space(10.0);
                // 12 px text with its icon a space before it (`.row.small`).
                crate::request_tab::hrow(ui, 17.4, |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let line = |ui: &mut Ui, icon: &str, text: &str| {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.5;
                            ui.label(RichText::new(icon).font(theme::icon_font(12.0)).color(p.text_muted));
                            ui.label(RichText::new(text).size(12.0).color(p.text_muted))
                        })
                        .inner
                    };
                    if let Some(e) = t.expires_at {
                        let left = e - now;
                        let text = if left <= 0 {
                            tr("Expired").to_string()
                        } else {
                            trf("Expires in {}", &[&short_duration(left)])
                        };
                        line(ui, icons::CLOCK, &text);
                    }
                    if t.refresh_token.is_some() {
                        line(ui, icons::ARROWS_CLOCKWISE, tr("Renewed automatically"))
                            .on_hover_text(tr("refresh token available: renewed automatically when it expires"));
                    }
                });
                if let Some(s) = t.scope.as_ref().filter(|s| !s.trim().is_empty()) {
                    ui.add_space(8.0);
                    crate::request_tab::hrow(ui, 20.0, |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for scope in s.split_whitespace() {
                            kit::pill_ex(ui, scope, Tone::Neutral, PillOpts { outline: true, ..Default::default() })
                                .on_hover_text(tr("Scopes"));
                        }
                    });
                }
            }
            None => {
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(kit::hint(
                        ui,
                        tr("No token stored. Tokens are kept in the Keychain, never in collection files."),
                    ))
                    .wrap(),
                );
            }
        }
        ui.add_space(12.0);
        crate::request_tab::hrow(ui, 24.0, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_enabled_ui(!ctx.fetching, |ui| {
                let opts = ButtonOpts { icon: Some(icons::ARROWS_CLOCKWISE), ..Default::default() };
                if kit::button(ui, Kind::Secondary, Size::Sm, tr("Get new access token"), opts).clicked() {
                    *action = AuthAction::GetToken;
                }
            });
            if ctx.token.is_some() {
                let opts = ButtonOpts { icon: Some(icons::TRASH), ..Default::default() };
                if kit::button(ui, Kind::Ghost, Size::Sm, tr("Clear token"), opts).clicked() {
                    *action = AuthAction::ClearToken;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(tr("Kept in the Keychain")).size(11.0).color(p.text_faint))
                    .on_hover_text(trf("Token key: {}", &[&ctx.owner_id]));
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ternarywire_core::model::HttpRequest;
    use ternarywire_core::prepared::{PrepareContext, prepare};
    use ternarywire_core::variables::{Resolver, VariableScopes};

    fn spec(kind: AuthType, key: &str) -> &'static FieldSpec {
        ternarywire_auth::fields(kind).iter().find(|f| f.key == key).unwrap()
    }

    /// Imported auth often leaves parameters out; the editor must then show
    /// what is actually sent, not the default a new config would get.
    #[test]
    fn missing_params_show_what_is_sent() {
        let mut auth = Auth::new(AuthType::OAuth1);
        auth.set("consumerKey", "ck");
        auth.set("consumerSecret", "cs");
        let http = HttpRequest { auth: auth.clone(), ..HttpRequest::new("GET", "https://api.test/x") };
        let scopes = VariableScopes::default();
        let defaults = ternarywire_core::settings::RequestDefaults::default();
        let ctx = PrepareContext {
            resolver: Resolver::new(&scopes),
            auth: &auth,
            auth_owner_id: None,
            defaults: &defaults,
            base_dir: None,
        };
        let signed = ternarywire_auth::preview(&prepare(&http, &ctx)).unwrap().request;
        let in_header =
            signed.headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v.starts_with("OAuth"));
        assert_eq!(bool_value(&auth, spec(AuthType::OAuth1, "addParamsToHeader")), in_header);

        // Without a callback URL the redirect is received on a random
        // 127.0.0.1 port, not on http://localhost:5555/callback.
        let auth = Auth::new(AuthType::OAuth2);
        assert_eq!(value_text(&auth, spec(AuthType::OAuth2, "redirect_uri")), "");
    }
}
