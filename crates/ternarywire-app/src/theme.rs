//! Design tokens (colors, type, spacing) and the egui style built from them.
//!
//! Every screen draws with these tokens: [`Palette`] for colors (light and
//! dark), the font helpers ([`medium`], [`semibold`], [`mono`]) for type, and
//! the components in `kit.rs`. The accent is the coral red of the app icon.

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, Visuals};

/// Colors of one theme. The values are the tokens of the redesign
/// (`design/redesign/styles.css`, `:root` and `.theme-dark`).
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Content area: the workbench card (request editor, response).
    pub canvas: Color32,
    /// Window chrome: title bar, rail, sidebar, status bar.
    pub chrome: Color32,
    /// Raised surfaces: inputs, cards, dialogs, menus.
    pub surface: Color32,
    /// Quiet panels: code wells, table headers, the tab strip, side panels.
    pub well: Color32,
    /// Hovered rows and quiet button fills.
    pub hover: Color32,
    /// Pressed state and subtle fills (segmented control track, count badges).
    pub subtle: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Text on an accent fill.
    pub on_accent: Color32,
    /// Background of selected rows and accent pills.
    pub accent_soft: Color32,
    /// Accent-colored text on light backgrounds.
    pub accent_text: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
    pub info: Color32,
    /// Soft fills behind `ok`/`warn`/`err`/`info` text (pills, banners).
    pub ok_soft: Color32,
    pub warn_soft: Color32,
    pub err_soft: Color32,
    pub info_soft: Color32,
    /// `{{variable}}` chips: defined (text, fill) and undefined (fill; text is `err`).
    pub var_ok: Color32,
    pub var_ok_bg: Color32,
    pub var_bad_bg: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const LIGHT: Palette = Palette {
    canvas: rgb(255, 255, 255),
    chrome: rgb(244, 244, 246),
    surface: rgb(255, 255, 255),
    well: rgb(248, 248, 250),
    hover: rgb(239, 239, 242),
    subtle: rgb(232, 232, 236),
    border: rgb(228, 228, 233),
    border_strong: rgb(208, 208, 215),
    text: rgb(23, 23, 28),
    text_muted: rgb(98, 98, 109),
    text_faint: rgb(154, 154, 165),
    accent: rgb(225, 77, 42),
    accent_hover: rgb(204, 64, 31),
    on_accent: Color32::WHITE,
    accent_soft: rgb(253, 236, 230),
    accent_text: rgb(190, 58, 26),
    ok: rgb(22, 140, 78),
    warn: rgb(180, 112, 0),
    err: rgb(214, 48, 49),
    info: rgb(37, 99, 220),
    ok_soft: rgb(229, 245, 236),
    warn_soft: rgb(251, 241, 220),
    err_soft: rgb(252, 232, 232),
    info_soft: rgb(231, 238, 252),
    var_ok: rgb(20, 130, 70),
    var_ok_bg: rgb(227, 244, 234),
    var_bad_bg: rgb(252, 232, 232),
};

const DARK: Palette = Palette {
    canvas: rgb(24, 25, 28),
    chrome: rgb(18, 19, 22),
    surface: rgb(32, 33, 37),
    well: rgb(28, 29, 33),
    hover: rgb(38, 39, 44),
    subtle: rgb(45, 46, 52),
    border: rgb(44, 45, 51),
    border_strong: rgb(62, 64, 71),
    text: rgb(232, 232, 236),
    text_muted: rgb(158, 159, 168),
    text_faint: rgb(110, 111, 120),
    accent: rgb(242, 103, 72),
    accent_hover: rgb(250, 125, 96),
    on_accent: Color32::WHITE,
    accent_soft: rgb(63, 36, 30),
    accent_text: rgb(255, 144, 118),
    ok: rgb(74, 205, 128),
    warn: rgb(240, 180, 72),
    err: rgb(248, 104, 100),
    info: rgb(110, 160, 255),
    ok_soft: rgb(23, 51, 38),
    warn_soft: rgb(58, 47, 23),
    err_soft: rgb(61, 31, 31),
    info_soft: rgb(28, 42, 69),
    var_ok: rgb(110, 200, 140),
    var_ok_bg: rgb(27, 51, 38),
    var_bad_bg: rgb(61, 31, 31),
};

/// Which status color a pill, banner or badge uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Ok,
    Warn,
    Err,
    Info,
}

impl Palette {
    /// Barely visible fill for hovered table rows.
    pub fn faint_row(&self) -> Color32 {
        self.well
    }

    /// `(text, fill)` of a tone.
    pub fn tone(&self, tone: Tone) -> (Color32, Color32) {
        match tone {
            Tone::Neutral => (self.text_muted, self.subtle),
            Tone::Accent => (self.accent_text, self.accent_soft),
            Tone::Ok => (self.ok, self.ok_soft),
            Tone::Warn => (self.warn, self.warn_soft),
            Tone::Err => (self.err, self.err_soft),
            Tone::Info => (self.info, self.info_soft),
        }
    }
}

/// Tone of an HTTP status code.
pub fn status_tone(status: u16) -> Tone {
    match status {
        200..=299 => Tone::Ok,
        300..=399 => Tone::Info,
        400..=499 => Tone::Warn,
        _ => Tone::Err,
    }
}

/// Fixed sizes of the layout grid (points). Rows that sit side by side in
/// the request and response panes have the same height, so their hairlines
/// line up across the window.
pub mod size {
    /// Title bar with the workspace switcher, search and environment.
    pub const TITLE_BAR: f32 = 44.0;
    /// Tab strip at the top of the workbench card.
    pub const TAB_STRIP: f32 = 40.0;
    pub const STATUS_BAR: f32 = 28.0;
    pub const RAIL: f32 = 52.0;
    /// Title row of a view (breadcrumb, name, secondary actions).
    pub const VIEW_TITLE: f32 = 46.0;
    /// First row of a pane: URL bar + Send, or the response status.
    pub const ACTION_ROW: f32 = 56.0;
    /// Tabs inside a pane.
    pub const TABS: f32 = 36.0;
    /// Toolbar above a pane's content.
    pub const TOOLBAR: f32 = 40.0;
    /// Horizontal padding of pane rows and content.
    pub const PAD: f32 = 12.0;
    /// Table and key/value rows.
    pub const ROW: f32 = 32.0;
    pub const TABLE_HEAD: f32 = 28.0;
    /// Sidebar tree rows.
    pub const TREE_ROW: f32 = 28.0;
    /// Gutter between two panes (1 px line in its middle).
    pub const GUTTER: f32 = 9.0;
    /// Side by side needs two usable panes; narrower work areas stack them.
    pub const SIDE_BY_SIDE_MIN: f32 = 880.0;
}

pub fn palette(dark: bool) -> Palette {
    if dark { DARK } else { LIGHT }
}

/// The palette of the theme `ui` is drawn with.
pub fn pal(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}

/// Accent used for primary actions.
pub fn accent(dark: bool) -> Color32 {
    palette(dark).accent
}

pub fn method_color(method: &str, dark: bool) -> Color32 {
    let (d, l) = match method.to_ascii_uppercase().as_str() {
        "GET" => ((74, 205, 128), (22, 140, 78)),
        "POST" => ((246, 184, 70), (184, 112, 0)),
        "PUT" => ((100, 160, 255), (37, 99, 220)),
        "PATCH" => ((180, 140, 250), (124, 72, 210)),
        "DELETE" | "DEL" => ((248, 104, 100), (214, 48, 49)),
        // Protocol badges share the hues of the rarer methods.
        "HEAD" | "SIO" | "SOCKET.IO" => ((96, 205, 205), (14, 128, 132)),
        "OPTIONS" | "OPT" | "AI" => ((240, 125, 200), (180, 45, 135)),
        "WS" => ((110, 180, 255), (20, 110, 200)),
        "GQL" | "GRPC" => ((236, 100, 190), (196, 30, 140)),
        "MQTT" => ((180, 140, 250), (124, 72, 210)),
        "MCP" => ((246, 184, 70), (184, 112, 0)),
        _ => ((158, 159, 168), (104, 104, 116)),
    };
    let (r, g, b) = if dark { d } else { l };
    Color32::from_rgb(r, g, b)
}

pub fn status_color(status: u16, dark: bool) -> Color32 {
    match status {
        200..=299 => ok(dark),
        300..=399 => palette(dark).info,
        400..=499 => warn(dark),
        _ => err(dark),
    }
}

pub fn ok(dark: bool) -> Color32 {
    palette(dark).ok
}

pub fn warn(dark: bool) -> Color32 {
    palette(dark).warn
}

pub fn err(dark: bool) -> Color32 {
    palette(dark).err
}

pub fn muted(dark: bool) -> Color32 {
    palette(dark).text_muted
}

pub fn var_defined(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(110, 200, 140) } else { Color32::from_rgb(20, 130, 70) }
}

pub fn var_undefined(dark: bool) -> Color32 {
    err(dark)
}

/// A color at `alpha` (0–1) opacity, for soft fills behind colored text.
pub fn soft(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

// ---------------------------------------------------------------------------
// Type
// ---------------------------------------------------------------------------

const MEDIUM: &str = "medium";
const SEMIBOLD: &str = "semibold";
const ICONS: &str = "icons";

/// Body text size for the user's font size setting (13 by default).
fn body(font_size: f32) -> f32 {
    font_size
}

/// Inter Medium (labels that need a little weight: tabs, buttons).
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// Inter SemiBold (titles, headings).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// The icon font alone (icons also render inside ordinary text).
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Inter (regular, medium, semibold), JetBrains Mono and the Phosphor icons,
/// with egui's default fonts as fallbacks for scripts and emoji the bundled
/// subsets leave out.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font = |bytes: &'static [u8]| std::sync::Arc::new(egui::FontData::from_static(bytes));
    fonts.font_data.insert("Inter".into(), font(include_bytes!("../assets/fonts/Inter-Regular.ttf")));
    fonts.font_data.insert("Inter-Medium".into(), font(include_bytes!("../assets/fonts/Inter-Medium.ttf")));
    fonts.font_data.insert("Inter-SemiBold".into(), font(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")));
    fonts.font_data.insert("JetBrainsMono".into(), font(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")));
    let phosphor_bytes: &'static [u8] = include_bytes!("../assets/fonts/Phosphor.ttf");
    let mut phosphor = egui::FontData::from_static(phosphor_bytes);
    // Icons inside text sit a little high next to Inter's x-height.
    phosphor.tweak.y_offset_factor = 0.08;
    fonts.font_data.insert("Phosphor".into(), std::sync::Arc::new(phosphor));
    // A standalone icon (`icon_font`) is centered in its box like the
    // prototype's flex-centered `.i`, without that nudge.
    fonts.font_data.insert("Phosphor-Icon".into(), font(phosphor_bytes));

    let fallbacks: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_fallbacks: Vec<String> = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let with = |first: &str| {
        let mut v = vec![first.to_string(), "Phosphor".to_string()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, with("Inter"));
    fonts.families.insert(FontFamily::Name(MEDIUM.into()), with("Inter-Medium"));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), with("Inter-SemiBold"));
    let mut mono = vec!["JetBrainsMono".to_string()];
    mono.extend(mono_fallbacks);
    mono.push("Phosphor".into());
    fonts.families.insert(FontFamily::Monospace, mono);
    fonts.families.insert(FontFamily::Name(ICONS.into()), vec!["Phosphor-Icon".into(), "Inter".into()]);
    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

/// Controls (buttons, inputs); small controls use 6.
pub const RADIUS: u8 = 7;
pub const RADIUS_SM: u8 = 6;
/// Cards.
pub const RADIUS_CARD: u8 = 9;
/// The workbench card and toasts.
pub const RADIUS_LG: u8 = 10;
/// Dialogs.
pub const RADIUS_DIALOG: u8 = 14;

pub fn apply(ctx: &egui::Context, font_size: f32) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            let dark = theme == egui::Theme::Dark;
            let p = palette(dark);
            let s = &mut style.spacing;
            s.item_spacing = egui::vec2(8.0, 6.0);
            s.button_padding = egui::vec2(9.0, 4.0);
            // Inputs, selects and menu items are 30 high.
            s.interact_size = egui::vec2(24.0, 30.0);
            s.window_margin = egui::Margin::same(18);
            s.menu_margin = egui::Margin::same(5);
            s.indent = 16.0;
            s.icon_width = 15.0;
            s.icon_width_inner = 9.0;
            s.icon_spacing = 6.0;
            s.combo_height = 320.0;
            s.tooltip_width = 360.0;
            s.menu_width = 220.0;
            s.scroll = egui::style::ScrollStyle::floating();
            s.scroll.bar_width = 8.0;
            s.scroll.floating_width = 3.0;
            s.scroll.floating_allocated_width = 0.0;

            let v: &mut Visuals = &mut style.visuals;
            v.panel_fill = p.canvas;
            v.window_fill = p.surface;
            v.extreme_bg_color = p.surface;
            v.text_edit_bg_color = Some(p.surface);
            v.faint_bg_color = p.well;
            v.code_bg_color = p.subtle;
            v.hyperlink_color = p.accent_text;
            v.warn_fg_color = p.warn;
            v.error_fg_color = p.err;
            v.override_text_color = None;
            // Placeholders of text fields (`.input::placeholder`).
            v.weak_text_color = Some(p.text_faint);
            v.selection.bg_fill = if dark { soft(p.accent, 0.32) } else { soft(p.accent, 0.18) };
            // Also the border of a focused text field: accent in both themes.
            v.selection.stroke = Stroke::new(1.0, if dark { p.accent } else { p.accent_text });
            v.text_cursor.stroke = Stroke::new(1.5, p.accent);
            v.window_corner_radius = CornerRadius::same(RADIUS_DIALOG);
            v.menu_corner_radius = CornerRadius::same(RADIUS_LG);
            v.window_stroke = Stroke::new(1.0, p.border);
            v.window_shadow = Shadow {
                offset: [0, 24],
                blur: 64,
                spread: 0,
                color: Color32::from_black_alpha(if dark { 153 } else { 56 }),
            };
            v.popup_shadow = Shadow {
                offset: [0, 6],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(if dark { 128 } else { 31 }),
            };
            v.window_highlight_topmost = false;
            v.collapsing_header_frame = false;
            v.indent_has_left_vline = true;
            v.striped = false;
            v.slider_trailing_fill = true;
            v.handle_shape = egui::style::HandleShape::Circle;
            v.interact_cursor = Some(egui::CursorIcon::PointingHand);
            v.disabled_alpha = 0.45;

            let radius = CornerRadius::same(RADIUS);
            let w = &mut v.widgets;
            // Labels, separators, frames.
            w.noninteractive.bg_fill = p.surface;
            w.noninteractive.weak_bg_fill = p.surface;
            w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
            w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
            w.noninteractive.corner_radius = radius;
            // Buttons, inputs, checkboxes at rest: outlined surface.
            w.inactive.bg_fill = p.subtle;
            w.inactive.weak_bg_fill = p.surface;
            w.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
            w.inactive.fg_stroke = Stroke::new(1.0, p.text);
            w.inactive.corner_radius = radius;
            w.inactive.expansion = 0.0;
            w.hovered.bg_fill = p.subtle;
            w.hovered.weak_bg_fill = p.hover;
            w.hovered.bg_stroke =
                Stroke::new(1.0, if dark { Color32::from_rgb(80, 82, 90) } else { Color32::from_rgb(190, 190, 200) });
            w.hovered.fg_stroke = Stroke::new(1.0, p.text);
            w.hovered.corner_radius = radius;
            w.hovered.expansion = 0.0;
            w.active.bg_fill = p.subtle;
            w.active.weak_bg_fill = p.subtle;
            w.active.bg_stroke = Stroke::new(1.0, p.accent);
            w.active.fg_stroke = Stroke::new(1.0, p.text);
            w.active.corner_radius = radius;
            w.active.expansion = 0.0;
            w.open.bg_fill = p.subtle;
            w.open.weak_bg_fill = p.hover;
            w.open.bg_stroke = Stroke::new(1.0, p.border_strong);
            w.open.fg_stroke = Stroke::new(1.0, p.text);
            w.open.corner_radius = radius;

            for (ts, font) in style.text_styles.iter_mut() {
                *font = match ts {
                    egui::TextStyle::Heading => semibold(font_size + 3.0),
                    egui::TextStyle::Small => FontId::proportional(font_size - 1.0),
                    egui::TextStyle::Monospace => mono(font_size - 0.5),
                    egui::TextStyle::Button => FontId::proportional(font_size - 0.5),
                    _ => FontId::proportional(body(font_size)),
                };
            }
        });
    }
}
