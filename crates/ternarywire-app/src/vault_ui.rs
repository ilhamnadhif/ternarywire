//! Vault dialog: local secrets, names linked to external secret managers
//! (HashiCorp Vault, AWS Secrets Manager, Azure Key Vault, 1Password) and
//! the providers themselves.

use std::sync::Arc;

use egui::{Align, CornerRadius, Key, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use parking_lot::Mutex;

use ternarywire_secrets::{
    AwsCredentialSource, AzureAuth, BrowseEntry, ExternalSecrets, KvVersion, LinkStatus, LinkedSecret, OnePasswordMode,
    ProviderConfig, ProviderKind, ProviderSettings, VaultAuth,
};

use crate::app::TernaryWireApp;
use crate::dialogs::{
    Modal, Outcome, Tone, add_input, banner, body_with, compact_empty, footer, inline_result, line_edit, line_galley,
    number, secret_chip, secret_input, segmented_sm, small_input, tile, trailing, two_lines, vlist, vrow,
};
use crate::i18n::{tr, trf};
use crate::{icons, kit, theme, widgets};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Section {
    #[default]
    Secrets,
    Providers,
}

/// Results of background calls, read by the dialog each frame.
#[derive(Default)]
struct Pending {
    busy: bool,
    message: Option<(String, bool)>,
    browse: Option<Vec<BrowseEntry>>,
}

#[derive(Default)]
struct LinkForm {
    name: String,
    provider: String,
    reference: String,
    ttl_secs: u64,
    browse_path: Vec<String>,
}

struct ProviderForm {
    config: ProviderConfig,
    /// Credential edits (empty = unchanged).
    credentials: Vec<(String, String)>,
    is_new: bool,
}

#[derive(Default)]
pub struct VaultDialog {
    section: Section,
    filter: String,
    new_name: String,
    new_value: String,
    link: Option<LinkForm>,
    provider: Option<ProviderForm>,
    pending: Arc<Mutex<Pending>>,
    /// External providers were set up when the dialog opened (once: a
    /// failure shows a toast, which would come back on every frame).
    attached: bool,
}

impl VaultDialog {
    /// The dialog on its External providers tab (automation).
    pub fn providers() -> VaultDialog {
        VaultDialog { section: Section::Providers, ..Default::default() }
    }

    /// The dialog with the link form open for `name` (automation).
    pub fn linking(name: &str) -> VaultDialog {
        let link = LinkForm { name: name.to_string(), ttl_secs: 300, ..Default::default() };
        VaultDialog { link: Some(link), ..Default::default() }
    }
}

/// Runs `work` in the background; its result replaces `pending`.
fn run<F>(pending: &Arc<Mutex<Pending>>, app: &TernaryWireApp, work: F)
where
    F: Future<Output = Pending> + Send + 'static,
{
    pending.lock().busy = true;
    let pending = pending.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let out = work.await;
        *pending.lock() = out;
        ctx.request_repaint();
    });
}

fn status_text(status: &LinkStatus) -> (String, bool) {
    match status {
        LinkStatus::NotLinked => (tr("local").into(), false),
        LinkStatus::NotFetched => (tr("not fetched yet").into(), false),
        LinkStatus::Fetching => (tr("fetching…").into(), false),
        LinkStatus::Cached { expires_in, .. } => (format!("{} · {} s", tr("cached"), expires_in.as_secs()), false),
        LinkStatus::Failed(e) => (e.message.clone(), true),
    }
}

fn kind_icon(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::HashicorpVault => icons::VAULT,
        ProviderKind::AwsSecretsManager => icons::CLOUD_ARROW_DOWN,
        ProviderKind::AzureKeyVault => icons::KEY,
        ProviderKind::OnePassword => icons::LOCK_KEY,
    }
}

/// Width of a small button with an icon, to lay out fields beside it.
fn small_button_w(ui: &Ui, icon: bool, text: &str) -> f32 {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::medium(12.0), egui::Color32::PLACEHOLDER);
    // Padding 8 and a 1 px border on each side.
    g.size().x + 18.0 + if icon { 13.0 + 5.0 } else { 0.0 }
}

/// Two columns 12 px apart (`.grid2`): equal, or the right one `right_w`
/// wide. `add` is called for column 0, then 1.
fn grid2(ui: &mut Ui, right_w: Option<f32>, mut add: impl FnMut(&mut Ui, usize)) {
    let w = ui.available_width();
    let (lw, rw) = match right_w {
        Some(r) => (w - 12.0 - r, r),
        None => ((w - 12.0) / 2.0, (w - 12.0) / 2.0),
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for (col, cw) in [(0, lw), (1, rw)] {
            ui.allocate_ui_with_layout(vec2(cw, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(cw);
                add(ui, col);
            });
        }
    });
}

/// A small text field with its label above it, as wide as the column.
fn text_field(ui: &mut Ui, label: &str, value: &mut String, hint: &str, mono: bool) {
    kit::field(ui, label, "", |ui| {
        let w = ui.available_width();
        small_input(ui, value, hint, label, w, mono);
    });
}

/// The busy spinner or the last result, beside the buttons of a form.
fn pending_inline(ui: &mut Ui, pending: &Arc<Mutex<Pending>>) {
    let (busy, message) = {
        let p = pending.lock();
        (p.busy, p.message.clone())
    };
    if busy {
        ui.add(egui::Spinner::new().size(13.0));
        ui.label(RichText::new(tr("Working…")).size(12.0).color(theme::pal(ui).text_muted));
    } else if let Some((m, err)) = message {
        let w = (ui.available_width() - 150.0).max(60.0);
        ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
            inline_result(ui, !err, &m)
        });
    }
}

pub fn show(app: &mut TernaryWireApp, ctx: &egui::Context, d: &mut VaultDialog) -> Outcome {
    // External providers are set up on first use.
    if !std::mem::replace(&mut d.attached, true) {
        app.attach_external_secrets(true);
    }
    let ext = app.ws.external.clone();
    let backend = app.ws.secrets.backend_description();
    let secret_count = app.ws.vault_names().len();
    let provider_count = ext.as_ref().map(|e| e.providers().len()).unwrap_or(0);
    Modal::new("vault", 820.0)
        .title(tr("Vault"))
        .subtitle(trf("Secrets are kept in {}, never in workspace files.", &[&backend]))
        .icon(icons::LOCK_KEY)
        .show(ctx, |ui| {
            section_tabs(ui, &mut d.section, secret_count, provider_count);
            body_with(ui, 14.0, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                match (d.section, &ext) {
                    (Section::Secrets, _) => secrets(app, ui, d, ext.as_ref()),
                    (Section::Providers, Some(ext)) => providers(app, ui, d, ext),
                    (Section::Providers, None) => {
                        banner(ui, Tone::Err, tr("External secrets are unavailable for this workspace."));
                    }
                }
                // Results of a form are shown beside its buttons; others here.
                let inline = d.link.is_some() || (d.section == Section::Providers && d.provider.is_some());
                let (busy, message) = {
                    let p = d.pending.lock();
                    (p.busy, p.message.clone())
                };
                if !inline {
                    if busy {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(tr("Working…")).color(theme::pal(ui).text_muted));
                        });
                    } else if let Some((m, err)) = message {
                        banner(ui, if err { Tone::Err } else { Tone::Ok }, &m);
                    }
                }
            });
            footer(ui, |ui| {
                let p = theme::pal(ui);
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(tr("Use a secret in any field as")).size(12.0).color(p.text_muted));
                secret_chip(ui, "{{vault:name}}");
                trailing(ui, |ui| {
                    if kit::secondary(ui, None, tr("Close")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        })
}

/// Secrets | External providers, across the dialog with 18 px sides.
/// Secrets | External providers (`.tabs` with 18 px sides): 36 high, a
/// hairline under the whole dialog, the selected tab underlined in the
/// accent, counts in small badges.
fn section_tabs(ui: &mut Ui, section: &mut Section, secrets: usize, providers: usize) {
    let p = theme::pal(ui);
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), kit::size::TABS));
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.border));
    let mut x = rect.left() + 18.0;
    let cy = rect.center().y;
    for (value, label, count) in
        [(Section::Secrets, tr("Secrets"), secrets), (Section::Providers, tr("External providers"), providers)]
    {
        let selected = *section == value;
        let g = ui.painter().layout_no_wrap(label.to_string(), theme::medium(12.5), egui::Color32::PLACEHOLDER);
        let (badge_fg, badge_bg) = if selected { (p.accent_text, p.accent_soft) } else { (p.text_muted, p.subtle) };
        let cg = (count > 0).then(|| ui.painter().layout_no_wrap(count.to_string(), theme::semibold(10.5), badge_fg));
        let badge_w = cg.as_ref().map_or(0.0, |c| (c.size().x + 10.0).max(17.0));
        let w = 18.0 + g.size().x + if cg.is_some() { 6.0 + badge_w } else { 0.0 };
        let tab = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height() - 1.0));
        let resp = ui.interact(tab, ui.scope_id().with(("vault-section", label)), Sense::click());
        let name = if count > 0 { format!("{label} ({count})") } else { label.to_string() };
        widgets::a11y_tab(&resp, &name, selected);
        if resp.clicked() {
            *section = value;
        }
        let color = if selected || resp.hovered() { p.text } else { p.text_muted };
        let gw = g.size().x;
        ui.painter().galley(pos2(tab.left() + 9.0, cy - g.size().y / 2.0), g, color);
        if let Some(cg) = cg {
            let pill = Rect::from_min_size(pos2(tab.left() + 9.0 + gw + 6.0, cy - 8.0), vec2(badge_w, 16.0));
            ui.painter().rect_filled(pill, CornerRadius::same(8), badge_bg);
            ui.painter().galley(pill.center() - cg.size() / 2.0, cg, badge_fg);
        }
        if selected {
            let line =
                Rect::from_min_max(pos2(tab.left() + 8.0, rect.bottom() - 2.0), pos2(tab.right() - 8.0, rect.bottom()));
            ui.painter().rect_filled(line, CornerRadius::same(1), p.accent);
        }
        if resp.has_focus() {
            let ring = tab.shrink(3.0);
            ui.painter().rect_stroke(
                ring,
                CornerRadius::same(theme::RADIUS_SM),
                Stroke::new(1.5, p.accent),
                egui::StrokeKind::Inside,
            );
        }
        x += w + 2.0;
    }
    ui.advance_cursor_after_rect(rect);
}

fn secrets(app: &mut TernaryWireApp, ui: &mut Ui, d: &mut VaultDialog, ext: Option<&ExternalSecrets>) {
    let p = theme::pal(ui);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), Layout::left_to_right(Align::Center), |ui| {
        kit::search_field_sm(ui, "vault-filter", &mut d.filter, tr("Filter secrets"), 240.0);
        trailing(ui, |ui| {
            if let Some(e) = ext
                && !e.providers().is_empty()
                && kit::button_ex(ui, kit::Kind::Secondary, Some(icons::LINK), tr("Link external secret…"), true)
                    .clicked()
                && d.link.is_none()
            {
                d.link = Some(LinkForm { ttl_secs: 300, ..Default::default() });
            }
        });
    });
    let names = app.ws.vault_names();
    let needle = d.filter.trim().to_lowercase();
    let shown: Vec<&String> = names.iter().filter(|n| n.to_lowercase().contains(&needle)).collect();
    let mut remove = None;
    let mut unlink = None;
    let editing = d.link.as_ref().map(|l| l.name.clone());
    vlist(ui, |ui| {
        if names.is_empty() {
            egui::Frame::new().inner_margin(Margin::symmetric(12, 20)).show(ui, |ui| {
                compact_empty(
                    ui,
                    icons::KEY,
                    tr("No secrets yet"),
                    tr("Add one below, then use it in any field without putting it in a file."),
                );
            });
        } else if shown.is_empty() {
            egui::Frame::new().inner_margin(Margin::symmetric(14, 14)).show(ui, |ui| {
                ui.label(RichText::new(tr("No secrets match the filter.")).size(12.5).color(p.text_muted));
            });
        }
        for (i, n) in shown.iter().enumerate() {
            let linked = ext.and_then(|e| e.linked(n));
            let selected = editing.as_deref() == Some(n.as_str());
            vrow(ui, (i, shown.len()), 44.0, selected, |ui, hovered| {
                tile(ui, icons::KEY, false);
                ui.label(RichText::new(format!("{{{{vault:{n}}}}}")).font(theme::mono(12.0)).color(p.text));
                match (&linked, ext) {
                    (Some(l), Some(e)) => {
                        let provider = e.provider(&l.provider_id).map(|p| p.name).unwrap_or_default();
                        let o = kit::PillOpts { icon: Some(icons::LINK_SIMPLE), ..Default::default() };
                        kit::pill_ex(ui, &provider, kit::Tone::Info, o).on_hover_text(&l.reference);
                        let (status, err) = status_text(&e.status(n));
                        let w = (ui.available_width() - 64.0).max(20.0);
                        ui.allocate_ui_with_layout(vec2(w, 20.0), Layout::left_to_right(Align::Center), |ui| {
                            let color = if err { p.err } else { p.text_faint };
                            ui.add(egui::Label::new(RichText::new(status).size(12.0).color(color)).truncate());
                        });
                    }
                    _ => {
                        kit::tone_pill(ui, tr("Local"), kit::Tone::Neutral);
                    }
                }
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if !hovered {
                        ui.set_invisible();
                    }
                    if linked.is_some() {
                        if kit::icon_button_sm(ui, icons::X, tr("Unlink")).clicked() {
                            unlink = Some(n.to_string());
                        }
                    } else {
                        if kit::icon_button_sm(ui, icons::TRASH, tr("Delete")).clicked() {
                            remove = Some(n.to_string());
                        }
                        if ext.is_some()
                            && kit::icon_button_sm(ui, icons::LINK, tr("Link to an external secret")).clicked()
                        {
                            d.link = Some(LinkForm { name: n.to_string(), ttl_secs: 300, ..Default::default() });
                        }
                    }
                    if kit::icon_button_sm(ui, icons::COPY, tr("Copy the reference")).clicked() {
                        ui.ctx().copy_text(format!("{{{{vault:{n}}}}}"));
                    }
                });
            });
        }
    });
    if let Some(n) = remove {
        app.ws.secrets.vault_remove(&n);
    }
    if let (Some(n), Some(e)) = (unlink, ext) {
        let _ = e.unlink(&n);
    }
    match (ext, d.link.is_some()) {
        (Some(e), true) => link_form(app, ui, d, e),
        _ => add_secret(app, ui, d),
    }
}

/// "Add a secret": name, masked value and Add, in a card under the list.
fn add_secret(app: &mut TernaryWireApp, ui: &mut Ui, d: &mut VaultDialog) {
    kit::card(ui).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(kit::section_title(tr("Add a secret")).color(theme::pal(ui).text));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            small_input(ui, &mut d.new_name, tr("Name"), tr("Name"), 200.0, true);
            let w = ui.available_width() - small_button_w(ui, true, tr("Add")) - 8.0;
            let v = secret_input(ui, "vault-new-value", &mut d.new_value, tr("Value"), w);
            widgets::a11y_name(&v, tr("Value"));
            let enter = v.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let ok = !d.new_name.trim().is_empty();
            let add = ui
                .add_enabled_ui(ok, |ui| kit::button_ex(ui, kit::Kind::Secondary, Some(icons::PLUS), tr("Add"), true))
                .inner
                .clicked();
            if (add || enter) && ok {
                app.ws.secrets.vault_set(d.new_name.trim(), &d.new_value);
                d.new_name.clear();
                d.new_value.clear();
            }
        });
    });
}

/// The link form, in place of "Add a secret": name, provider, reference and
/// cache, a browser of the provider's secrets, Preview and Save link.
fn link_form(app: &TernaryWireApp, ui: &mut Ui, d: &mut VaultDialog, ext: &ExternalSecrets) {
    let providers = ext.providers();
    if providers.is_empty() {
        banner(ui, Tone::Info, tr("Add a provider under External providers first."));
        return;
    }
    let Some(form) = d.link.as_mut() else { return };
    if form.provider.is_empty() {
        form.provider = providers[0].id.clone();
    }
    let provider = providers.iter().find(|p| p.id == form.provider).cloned().unwrap_or_else(|| providers[0].clone());
    let mut save = false;
    let mut cancel = false;
    let mut browse: Option<String> = None;
    let mut preview = false;
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.accent))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new(icons::LINK).font(theme::icon_font(13.0)).color(p.accent_text));
                ui.label(kit::section_title(tr("Link a vault name to an external secret")).color(p.text));
                trailing(ui, |ui| {
                    if kit::icon_button_sm(ui, icons::X, tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
            grid2(ui, None, |ui, col| {
                if col == 0 {
                    text_field(ui, tr("Name"), &mut form.name, "db_password", true);
                    return;
                }
                kit::field(ui, tr("Provider"), "", |ui| {
                    let label = format!("{} ({})", provider.name, provider.settings.kind().label());
                    let icon = kind_icon(provider.settings.kind());
                    let w = ui.available_width();
                    let r = kit::select_box(ui, Some(icon), &label, w, true);
                    widgets::a11y_name(&r, tr("Provider"));
                    egui::Popup::menu(&r).width(w).show(|ui| {
                        for pr in &providers {
                            let label = format!("{} ({})", pr.name, pr.settings.kind().label());
                            let item = kit::MenuItem {
                                icon: Some(kind_icon(pr.settings.kind())),
                                label: &label,
                                selected: pr.id == form.provider,
                                ..Default::default()
                            };
                            if kit::menu_item_ex(ui, item).clicked() {
                                form.provider = pr.id.clone();
                                form.browse_path.clear();
                            }
                        }
                    });
                });
            });
            grid2(ui, Some(150.0), |ui, col| {
                if col == 0 {
                    text_field(ui, tr("Reference"), &mut form.reference, provider.settings.reference_hint(), true);
                    return;
                }
                kit::field(ui, tr("Cache"), tr("0 fetches on every use"), |ui| {
                    ui.horizontal(|ui| number(ui, tr("Cache"), &mut form.ttl_secs, 0..=86_400, "s", 80.0));
                });
            });
            if let Err(e) = provider.settings.check_reference(&form.reference)
                && !form.reference.is_empty()
            {
                banner(ui, Tone::Err, &e.message);
            }
            let entries = d.pending.lock().browse.clone();
            if let Some(entries) = entries {
                kit::well(ui).inner_margin(Margin::same(4)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("vault-browse").max_height(180.0).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        if !form.browse_path.is_empty()
                            && browse_row(ui, icons::ARROW_BEND_UP_LEFT, tr("Up"), false).0.clicked()
                        {
                            form.browse_path.pop();
                            browse = Some(form.browse_path.last().cloned().unwrap_or_default());
                        }
                        if entries.is_empty() {
                            egui::Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                                ui.label(RichText::new(tr("Nothing here.")).size(12.5).color(p.text_muted));
                            });
                        }
                        for e in entries {
                            if let Some(path) = &e.path {
                                if browse_row(ui, icons::FOLDER_SIMPLE, &e.name, false).0.clicked() {
                                    form.browse_path.push(path.clone());
                                    browse = Some(path.clone());
                                }
                            } else {
                                let usable = e.reference.is_some();
                                let (r, used) = browse_row(ui, icons::KEY, &e.name, usable);
                                if let Some(reference) = &e.reference
                                    && (used || r.double_clicked())
                                {
                                    form.reference = reference.clone();
                                    if form.name.is_empty() {
                                        form.name = e.name.clone();
                                    }
                                }
                            }
                        }
                    });
                });
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if kit::button_ex(ui, kit::Kind::Secondary, Some(icons::FOLDER_OPEN), tr("Browse…"), true).clicked() {
                    browse = Some(form.browse_path.last().cloned().unwrap_or_default());
                }
                if kit::button_ex(ui, kit::Kind::Secondary, Some(icons::EYE), tr("Preview"), true)
                    .on_hover_text(tr("Fetch once without showing the value"))
                    .clicked()
                {
                    preview = true;
                }
                pending_inline(ui, &d.pending);
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let ok = !form.name.trim().is_empty() && !form.reference.trim().is_empty();
                    let r = ui.add_enabled_ui(ok, |ui| {
                        kit::button(ui, kit::Kind::Primary, kit::Size::Sm, tr("Save link"), Default::default())
                    });
                    if r.inner.clicked() {
                        save = true;
                    }
                    if kit::button_ex(ui, kit::Kind::Secondary, None, tr("Cancel"), true).clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if let Some(path) = browse {
        let (ext, id) = (ext.clone(), provider.id.clone());
        run(&d.pending, app, async move {
            match ext.browse(&id, &path).await {
                Ok(list) => Pending { browse: Some(list), ..Default::default() },
                Err(e) => Pending { message: Some((e.message, true)), ..Default::default() },
            }
        });
    }
    if preview {
        let (ext, id, reference) = (ext.clone(), provider.id.clone(), form.reference.clone());
        let keep = d.pending.lock().browse.clone();
        run(&d.pending, app, async move {
            let message = match ext.resolve_reference(&id, &reference).await {
                Ok(v) => (format!("{} ({} {})", tr("Resolved"), v.chars().count(), tr("characters")), false),
                Err(e) => (e.message, true),
            };
            Pending { message: Some(message), browse: keep, ..Default::default() }
        });
    }
    if save {
        let link = LinkedSecret::new(form.name.trim(), &provider.id, form.reference.trim()).with_ttl(form.ttl_secs);
        match ext.link(link) {
            Ok(()) => {
                let (ext2, name) = (ext.clone(), form.name.trim().to_string());
                d.link = None;
                run(&d.pending, app, async move {
                    let errs = ext2.prefetch(std::slice::from_ref(&name)).await;
                    let message = match errs.into_iter().next() {
                        Some((_, e)) => (e.message, true),
                        None => (format!("{{{{vault:{name}}}}} {}", tr("is linked")), false),
                    };
                    Pending { message: Some(message), ..Default::default() }
                });
            }
            Err(e) => d.pending.lock().message = Some((e.message, true)),
        }
    } else if cancel {
        d.link = None;
        let mut pending = d.pending.lock();
        pending.browse = None;
        pending.message = None;
    }
}

/// A row of the provider browser (`.dl-lrow`): folders open, secrets have
/// a Use button (`usable`). Returns the row and whether Use was pressed.
fn browse_row(ui: &mut Ui, icon: &str, label: &str, usable: bool) -> (egui::Response, bool) {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    widgets::a11y_button(&resp, label);
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), p.hover);
    }
    let cy = rect.center().y;
    let ig = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(13.0), p.text_muted);
    let iw = ig.size().x;
    ui.painter().galley(pos2(rect.left() + 10.0, cy - ig.size().y / 2.0), ig, p.text_muted);
    let g = line_galley(ui, label, theme::mono(12.0), rect.width() - 100.0);
    ui.painter().galley(pos2(rect.left() + 18.0 + iw, cy - g.size().y / 2.0), g, p.text);
    let mut used = false;
    if usable {
        let w = small_button_w(ui, false, tr("Use"));
        let r = Rect::from_min_size(pos2(rect.right() - 6.0 - w, cy - 12.0), vec2(w, 24.0));
        let use_button =
            |ui: &mut Ui| kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Use"), Default::default());
        used = crate::dialogs::place(ui, r, use_button).clicked();
    }
    (resp, used)
}

fn providers(app: &mut TernaryWireApp, ui: &mut Ui, d: &mut VaultDialog, ext: &ExternalSecrets) {
    ui.add(
        egui::Label::new(kit::hint(
            ui,
            tr("Providers keep their settings in .ternarywire/external-secrets.json; tokens and keys go to the secret store."),
        ))
        .wrap(),
    );
    let list = ext.providers();
    // Master–detail: one provider is always open in the form.
    if d.provider.is_none()
        && let Some(first) = list.first()
    {
        d.provider = Some(ProviderForm { config: first.clone(), credentials: Vec::new(), is_new: false });
    }
    let mut remove = None;
    let mut open = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        ui.allocate_ui_with_layout(vec2(270.0, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(270.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            if !list.is_empty() {
                let editing = d.provider.as_ref().filter(|f| !f.is_new).map(|f| f.config.id.clone());
                vlist(ui, |ui| {
                    for (i, pr) in list.iter().enumerate() {
                        let selected = editing.as_deref() == Some(pr.id.as_str());
                        let (_, row) = vrow(ui, (i, list.len()), 44.0, selected, |ui, _| {
                            tile(ui, kind_icon(pr.settings.kind()), true);
                            let w = ui.available_width() - 54.0;
                            two_lines(ui, &pr.name, theme::medium(13.0), pr.settings.kind().label(), w);
                            trailing(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if kit::icon_button_sm(ui, icons::TRASH, tr("Remove")).clicked() {
                                    remove = Some(pr.id.clone());
                                }
                                if kit::icon_button_sm(ui, icons::PENCIL_SIMPLE, tr("Edit")).clicked() {
                                    open = Some(pr.clone());
                                }
                            });
                        });
                        if row.clicked() {
                            open = Some(pr.clone());
                        }
                    }
                });
            }
            let o = kit::ButtonOpts { icon: Some(icons::PLUS), caret: true, min_width: 270.0, ..Default::default() };
            let add = kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Add provider"), o);
            egui::Popup::menu(&add).width(220.0).show(|ui| {
                for kind in ProviderKind::ALL {
                    if kit::menu_item(ui, Some(kind_icon(kind)), kind.label(), None).clicked() {
                        d.provider = Some(ProviderForm {
                            config: ProviderConfig::new(kind.label(), kind.default_settings()),
                            credentials: Vec::new(),
                            is_new: true,
                        });
                        d.pending.lock().message = None;
                    }
                }
            });
        });
        let w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(w);
            if d.provider.is_some() {
                provider_form(app, ui, d, ext);
            } else {
                kit::card(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_space(12.0);
                    compact_empty(
                        ui,
                        icons::VAULT,
                        tr("No external providers"),
                        tr("Connect HashiCorp Vault, AWS Secrets Manager, Azure Key Vault or 1Password."),
                    );
                    ui.add_space(12.0);
                });
            }
        });
    });
    if let Some(pr) = open
        && d.provider.as_ref().is_none_or(|f| f.is_new || f.config.id != pr.id)
    {
        d.provider = Some(ProviderForm { config: pr, credentials: Vec::new(), is_new: false });
        d.pending.lock().message = None;
    }
    if let Some(id) = remove {
        if d.provider.as_ref().is_some_and(|f| f.config.id == id) {
            d.provider = None;
        }
        match ext.remove_provider(&id) {
            Ok(unlinked) if !unlinked.is_empty() => {
                d.pending.lock().message = Some((format!("{}: {}", tr("Unlinked"), unlinked.join(", ")), false))
            }
            Ok(_) => {}
            Err(e) => d.pending.lock().message = Some((e.message, true)),
        }
    }
}

fn provider_form(app: &TernaryWireApp, ui: &mut Ui, d: &mut VaultDialog, ext: &ExternalSecrets) {
    let Some(form) = d.provider.as_mut() else { return };
    let mut save = false;
    let mut test = false;
    let mut cancel = false;
    kit::card(ui).inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        let kind = form.config.settings.kind();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            tile(ui, kind_icon(kind), true);
            let title = if form.is_new { trf("New {} provider", &[&kind.label()]) } else { form.config.name.clone() };
            let w = ui.available_width();
            two_lines(ui, &title, theme::semibold(13.0), kind.label(), w);
        });
        ui.add_space(2.0);
        text_field(ui, tr("Name"), &mut form.config.name, "", false);
        match &mut form.config.settings {
            ProviderSettings::HashicorpVault(s) => {
                grid2(ui, None, |ui, col| match col {
                    0 => text_field(ui, tr("Address"), &mut s.address, "https://vault.example.com:8200", true),
                    _ => text_field(ui, tr("Namespace"), &mut s.namespace, tr("optional (Vault Enterprise)"), false),
                });
                grid2(ui, None, |ui, col| match col {
                    0 => kit::field(ui, tr("Authentication"), "", |ui| {
                        segmented_sm(
                            ui,
                            &mut s.auth,
                            &[(VaultAuth::Token, "Token".into()), (VaultAuth::AppRole, "AppRole".into())],
                        );
                    }),
                    _ => kit::field(ui, tr("KV version"), "", |ui| {
                        segmented_sm(
                            ui,
                            &mut s.kv_version,
                            &[
                                (KvVersion::Auto, tr("Auto").into()),
                                (KvVersion::V1, "v1".into()),
                                (KvVersion::V2, "v2".into()),
                            ],
                        );
                    }),
                });
                if s.auth == VaultAuth::AppRole {
                    grid2(ui, None, |ui, col| match col {
                        0 => text_field(ui, "Role ID", &mut s.role_id, "", true),
                        _ => text_field(ui, tr("AppRole mount"), &mut s.approle_mount, "approle", true),
                    });
                }
                text_field(ui, tr("KV mount"), &mut s.mount, tr("optional: first path segment"), true);
            }
            ProviderSettings::AwsSecretsManager(s) => {
                grid2(ui, None, |ui, col| match col {
                    0 => text_field(ui, tr("Region"), &mut s.region, "eu-west-1", true),
                    _ => kit::field(ui, tr("Credentials"), "", |ui| {
                        segmented_sm(
                            ui,
                            &mut s.credentials,
                            &[
                                (AwsCredentialSource::DefaultChain, tr("Default chain (env, ~/.aws)").into()),
                                (AwsCredentialSource::Static, tr("Access keys").into()),
                            ],
                        );
                    }),
                });
                if s.credentials == AwsCredentialSource::DefaultChain {
                    text_field(ui, tr("Profile"), &mut s.profile, "default", false);
                }
                text_field(ui, tr("Endpoint"), &mut s.endpoint, tr("optional (e.g. LocalStack)"), true);
            }
            ProviderSettings::AzureKeyVault(s) => {
                text_field(ui, tr("Vault URL"), &mut s.vault_url, "https://my-vault.vault.azure.net", true);
                kit::field(ui, tr("Authentication"), "", |ui| {
                    segmented_sm(
                        ui,
                        &mut s.auth,
                        &[
                            (AzureAuth::ClientSecret, tr("Service principal").into()),
                            (AzureAuth::AzureCli, "Azure CLI".into()),
                        ],
                    );
                });
                if s.auth == AzureAuth::ClientSecret {
                    grid2(ui, None, |ui, col| match col {
                        0 => text_field(ui, tr("Tenant ID"), &mut s.tenant_id, "", true),
                        _ => text_field(ui, tr("Client ID"), &mut s.client_id, "", true),
                    });
                    text_field(
                        ui,
                        tr("Authority host"),
                        &mut s.authority_host,
                        "https://login.microsoftonline.com",
                        true,
                    );
                } else {
                    text_field(ui, tr("az path"), &mut s.cli_path, tr("optional"), true);
                }
                text_field(ui, tr("API version"), &mut s.api_version, "7.5", true);
            }
            ProviderSettings::OnePassword(s) => {
                kit::field(ui, tr("Mode"), "", |ui| {
                    segmented_sm(
                        ui,
                        &mut s.mode,
                        &[
                            (OnePasswordMode::Cli, tr("op CLI").into()),
                            (OnePasswordMode::Connect, tr("Connect server").into()),
                        ],
                    );
                });
                match s.mode {
                    OnePasswordMode::Cli => grid2(ui, None, |ui, col| match col {
                        0 => text_field(
                            ui,
                            tr("Account"),
                            &mut s.account,
                            tr("optional: sign-in address or email"),
                            false,
                        ),
                        _ => text_field(ui, tr("op path"), &mut s.cli_path, tr("optional"), true),
                    }),
                    OnePasswordMode::Connect => {
                        text_field(ui, tr("Connect URL"), &mut s.connect_url, "http://localhost:8080", true)
                    }
                }
            }
        }
        for f in form.config.settings.credential_fields() {
            let set = ext.has_credential(&form.config.id, f.key);
            let idx = match form.credentials.iter().position(|(k, _)| k == f.key) {
                Some(i) => i,
                None => {
                    form.credentials.push((f.key.to_string(), String::new()));
                    form.credentials.len() - 1
                }
            };
            let label = format!("{}{}", tr(f.label), if f.required { " *" } else { "" });
            kit::field(ui, &label, "", |ui| {
                let hint = if set { tr("set — type to replace") } else { tr("not set") };
                let w = ui.available_width();
                let edit =
                    line_edit(ui, &mut form.credentials[idx].1, hint, true, false).password(true).desired_width(w);
                let r = add_input(ui, edit, true);
                widgets::a11y_name(&r, tr(f.label));
            });
        }
        if let Err(e) = form.config.settings.validate() {
            banner(ui, Tone::Err, &e.message);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let o = kit::ButtonOpts { icon: Some(icons::PLUGS_CONNECTED), ..Default::default() };
            if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Test connection"), o).clicked() {
                save = true;
                test = true;
            }
            pending_inline(ui, &d.pending);
            trailing(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if kit::button(ui, kit::Kind::Primary, kit::Size::Sm, tr("Save"), Default::default()).clicked() {
                    save = true;
                }
                if kit::button_ex(ui, kit::Kind::Secondary, None, tr("Cancel"), true).clicked() {
                    cancel = true;
                }
            });
        });
    });
    if cancel {
        d.provider = None;
        d.pending.lock().message = None;
        return;
    }
    if !save {
        return;
    }
    let config = form.config.clone();
    if let Err(e) = ext.upsert_provider(config.clone()) {
        d.pending.lock().message = Some((e.message, true));
        return;
    }
    for (k, v) in form.credentials.drain(..).filter(|(_, v)| !v.is_empty()) {
        ext.set_credential(&config.id, &k, &v);
    }
    form.is_new = false;
    if test {
        let ext = ext.clone();
        run(&d.pending, app, async move {
            let message = match ext.test_provider(&config).await {
                Ok(summary) => (summary, false),
                Err(e) => (e.message, true),
            };
            Pending { message: Some(message), ..Default::default() }
        });
    } else {
        d.pending.lock().message = Some((tr("Saved").into(), false));
    }
}
