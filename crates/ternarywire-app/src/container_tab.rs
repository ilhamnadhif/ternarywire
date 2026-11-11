//! Collection and folder settings (FR-C2): overview, auth, scripts and
//! variables that requests inside inherit.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

use ternarywire_core::auth::AuthType;
use ternarywire_core::model::{Folder, HttpRequest, Id, Item, RequestItem, RequestKind, WsRequest};
use ternarywire_core::script::ScriptEvent;
use ternarywire_core::tokens::TokenStore;

use crate::app::{ItemRef, TernaryWireApp};
use crate::dialogs::Dialog;
use crate::env_tab::{title_label, title_row};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::{self, MenuItem, Tab};
use crate::{icons, theme, widgets};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Overview,
    Auth,
    Scripts,
    Variables,
}

pub struct ContainerTab {
    pub collection: Id,
    pub folder: Option<Id>,
    section: Section,
    event: ScriptEvent,
    filter: String,
    /// The description shows its Markdown rendered.
    preview: bool,
    /// The name being edited in the title row (double-click on it).
    renaming: Option<String>,
}

impl ContainerTab {
    pub fn new(collection: Id, folder: Option<Id>) -> Self {
        ContainerTab {
            collection,
            folder,
            section: Section::Overview,
            event: ScriptEvent::PreRequest,
            filter: String::new(),
            preview: false,
            renaming: None,
        }
    }

    /// Automation: shows `overview`, `auth`, `vars` or `scripts`.
    pub fn automation(&mut self, action: &str) {
        self.section = match action {
            "auth" => Section::Auth,
            "vars" | "variables" => Section::Variables,
            "scripts" => Section::Scripts,
            _ => Section::Overview,
        };
    }
}

fn count_folders(items: &[Item]) -> usize {
    items
        .iter()
        .map(|i| match i {
            Item::Folder(f) => 1 + count_folders(&f.items),
            Item::Request(_) => 0,
        })
        .sum()
}

fn count_requests(items: &[Item]) -> usize {
    items
        .iter()
        .map(|i| match i {
            Item::Folder(f) => count_requests(&f.items),
            Item::Request(_) => 1,
        })
        .sum()
}

/// Requests that get the auth of the container holding `items`: they
/// inherit it, through folders that inherit too.
fn count_inheriting(items: &[Item]) -> usize {
    items
        .iter()
        .map(|i| match i {
            Item::Folder(f) if f.auth.kind == AuthType::Inherit => count_inheriting(&f.items),
            Item::Folder(_) => 0,
            Item::Request(r) => usize::from(r.request.auth().kind == AuthType::Inherit),
        })
        .sum()
}

/// What the title row's menu and the Actions card ask for.
enum Act {
    Run,
    Mock,
    Rename,
    Duplicate,
    Delete,
    Fork,
    ForkSync(bool),
    Export(crate::sidebar::ExportFormat),
    /// None = preview, Some(markdown) = export.
    Docs(Option<bool>),
    NewRequest,
    NewWs,
    NewProto(String, Box<RequestKind>),
    NewFolder,
}

/// Numbers and names shown in the overview, read before borrowing the
/// container mutably.
struct Facts {
    collection_name: String,
    requests: usize,
    folders: usize,
    folder_names: String,
    inheriting: usize,
    inherited: Option<(String, AuthType)>,
    fork_parent: Option<String>,
    dir: Option<String>,
}

fn facts(app: &TernaryWireApp, tab: &ContainerTab) -> Option<Facts> {
    let c = app.ws.data.collection(&tab.collection)?;
    let items: &[Item] = match &tab.folder {
        None => &c.items,
        Some(f) => match c.find(f) {
            Some(Item::Folder(folder)) => &folder.items,
            _ => &[],
        },
    };
    let inherited = tab.folder.as_ref().and_then(|f| {
        let chain = c.ancestors(f)?;
        ternarywire_core::model::effective_from_chain(&chain).map(|e| (e.owner_name.to_string(), e.auth.kind))
    });
    let folder_names = items
        .iter()
        .filter_map(|i| match i {
            Item::Folder(f) => Some(f.name.as_str()),
            Item::Request(_) => None,
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let fork_parent = app
        .ws
        .forks
        .iter()
        .find(|f| f.fork == tab.collection)
        .map(|f| app.ws.data.collection(&f.parent).map(|c| c.name.clone()).unwrap_or_default());
    let dir = app.ws.store.collection_dir(&tab.collection).map(|d| {
        let rel = d.strip_prefix(app.ws.root()).map(|r| r.to_path_buf()).unwrap_or(d);
        format!("{}/", rel.display())
    });
    Some(Facts {
        collection_name: c.name.clone(),
        requests: count_requests(items),
        folders: count_folders(items),
        folder_names,
        inheriting: count_inheriting(items),
        inherited,
        fork_parent,
        dir,
    })
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut ContainerTab) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let vars = app.var_ctx(Some(&tab.collection));
    let owner_id = tab.folder.clone().unwrap_or_else(|| tab.collection.clone());
    let token = TokenStore::get(app.ws.secrets.as_ref(), &owner_id);
    let fetching = app.pending_tokens.contains(&owner_id);
    let Some(facts) = facts(app, tab) else {
        kit::empty_state(
            ui,
            icons::STACK,
            tr("This collection was deleted."),
            tr("Close this tab or pick another collection."),
            |_| {},
        );
        return;
    };
    let mut get_token = false;
    let mut clear_token = false;
    let mut acts: Vec<Act> = Vec::new();
    let cid = tab.collection.clone();
    let Some(c) = app.ws.data.collection_mut(&cid) else { return };
    let mut changed = false;
    // Borrow the common fields of either a collection or a folder.
    let (name, description, auth, scripts, variables) = match &tab.folder {
        None => (&mut c.name, &mut c.description, &mut c.auth, &mut c.scripts, &mut c.variables),
        Some(fid) => match c.find_mut(fid) {
            Some(Item::Folder(f)) => (&mut f.name, &mut f.description, &mut f.auth, &mut f.scripts, &mut f.variables),
            _ => {
                kit::empty_state(
                    ui,
                    icons::FOLDER_SIMPLE,
                    tr("This folder was deleted."),
                    tr("Close this tab or pick another folder."),
                    |_| {},
                );
                return;
            }
        },
    };
    let is_folder = tab.folder.is_some();
    let (icon, subtitle) = if is_folder {
        (icons::FOLDER_SIMPLE, trf("Folder in {} · {} requests", &[&facts.collection_name, &facts.requests]))
    } else {
        (icons::STACK, trf("Collection · {} requests", &[&facts.requests]))
    };
    // Title row: name (double-click renames), Mock server, Run, more.
    let fork_parent = facts.fork_parent.clone();
    let title = title_row(
        ui,
        icon,
        None,
        &subtitle,
        |ui| {
            let more = kit::icon_button(ui, icons::DOTS_THREE, tr("More actions"));
            egui::Popup::menu(&more)
                .width(220.0)
                .align(egui::RectAlign::BOTTOM_END)
                .show(|ui| container_menu(ui, is_folder, fork_parent.as_deref(), &mut acts));
            let run = kit::button(
                ui,
                kit::Kind::Primary,
                kit::Size::Sm,
                tr("Run"),
                kit::ButtonOpts { icon: Some(icons::PLAY), ..Default::default() },
            );
            if run.on_hover_text(tr("Open the Collection Runner")).clicked() {
                acts.push(Act::Run);
            }
            if !is_folder
                && kit::button(
                    ui,
                    kit::Kind::Secondary,
                    kit::Size::Sm,
                    tr("Mock server"),
                    kit::ButtonOpts { icon: Some(icons::CUBE), ..Default::default() },
                )
                .clicked()
            {
                acts.push(Act::Mock);
            }
        },
        |ui| match &mut tab.renaming {
            Some(text) => {
                let id = ui.make_persistent_id("container-rename");
                let r = ui.add(
                    egui::TextEdit::singleline(text)
                        .id(id)
                        .font(theme::semibold(14.0))
                        .desired_width(ui.available_width().min(320.0))
                        .min_size(vec2(0.0, 26.0))
                        .margin(egui::Margin::symmetric(6, 0))
                        .align(Align2::LEFT_CENTER),
                );
                widgets::a11y_name(&r, tr("Name"));
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                // Done: Some(keep the new name).
                ((r.lost_focus() || cancel).then_some(!cancel), false)
            }
            None => {
                let r = title_label(ui, name).on_hover_text(tr("Double-click to rename"));
                (None, r.double_clicked())
            }
        },
    );
    let (done, rename) = title;
    if rename {
        acts.push(Act::Rename);
    }
    if let Some(keep) = done {
        let text = tab.renaming.take().unwrap_or_default();
        let text = text.trim();
        if keep && !text.is_empty() && name != text {
            *name = text.to_string();
            changed = true;
        }
    }
    let script_count = [&scripts.pre_request, &scripts.post_response].iter().filter(|s| !s.trim().is_empty()).count();
    let auth_set = !matches!(auth.kind, AuthType::Inherit | AuthType::None);
    kit::tabs(
        ui,
        &mut tab.section,
        &[
            Tab::new(Section::Overview, tr("Overview")),
            Tab::new(Section::Auth, tr("Authorization")).dot(auth_set),
            Tab::new(Section::Variables, tr("Variables")).count(variables.len()),
            Tab::new(Section::Scripts, tr("Scripts")).count(script_count),
        ],
    );
    match tab.section {
        Section::Overview => {
            let secrets = variables.iter().filter(|v| v.is_secret()).count();
            egui::ScrollArea::vertical().id_salt("container-overview").auto_shrink([false; 2]).show(ui, |ui| {
                let avail = ui.available_width();
                let width = (avail - 40.0).clamp(200.0, 940.0);
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 18, bottom: 28 }).show(
                    ui,
                    |ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing.y = 18.0;
                        overview_stats(ui, &facts, variables.len(), secrets, auth.kind);
                        let side = width >= 620.0;
                        let mut main = |ui: &mut Ui, changed: &mut bool| {
                            ui.spacing_mut().item_spacing.y = 18.0;
                            kit::group(ui, tr("General"), "", |ui, rows| {
                                let help = if is_folder {
                                    tr("Shown in the sidebar and in run reports.")
                                } else {
                                    tr("Also the name of the collection file.")
                                };
                                // Label and help on the left, a 280 wide input (26 high) on the right.
                                rows.block(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let input_w = 280.0_f32.min(ui.available_width() * 0.6);
                                        let label_w = ui.available_width() - input_w - 14.0;
                                        ui.allocate_ui_with_layout(
                                            vec2(label_w, 0.0),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                ui.set_width(label_w);
                                                ui.spacing_mut().item_spacing.y = 1.0;
                                                let p = theme::pal(ui);
                                                ui.label(
                                                    RichText::new(tr("Name")).font(theme::medium(13.0)).color(p.text),
                                                );
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(help).size(12.0).color(p.text_muted),
                                                    )
                                                    .wrap(),
                                                );
                                            },
                                        );
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.spacing_mut().interact_size.y = 26.0;
                                            let r = ui.add(
                                                egui::TextEdit::singleline(name)
                                                    .font(FontId::proportional(12.0))
                                                    .desired_width(input_w)
                                                    .min_size(vec2(0.0, 26.0))
                                                    .margin(egui::Margin::symmetric(8, 0))
                                                    .align(Align2::LEFT_CENTER),
                                            );
                                            widgets::a11y_name(&r, tr("Name"));
                                            *changed |= r.changed();
                                        });
                                    });
                                });
                            });
                            *changed |= description_ui(ui, &mut tab.preview, description);
                        };
                        if side {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 20.0;
                                let main_w = width - 250.0 - 20.0;
                                ui.allocate_ui_with_layout(vec2(main_w, 0.0), Layout::top_down(Align::Min), |ui| {
                                    ui.set_width(main_w);
                                    main(ui, &mut changed);
                                });
                                ui.allocate_ui_with_layout(vec2(250.0, 0.0), Layout::top_down(Align::Min), |ui| {
                                    ui.set_width(250.0);
                                    actions_card(ui, is_folder, &facts, &mut acts);
                                });
                            });
                        } else {
                            main(ui, &mut changed);
                            actions_card(ui, is_folder, &facts, &mut acts);
                        }
                    },
                );
            });
        }
        Section::Auth => {
            egui::ScrollArea::vertical().id_salt("container-auth").auto_shrink([false; 2]).show(ui, |ui| {
                let width = ui.available_width().min(820.0);
                ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(width);
                    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 16 }).show(
                        ui,
                        |ui| {
                            ui.set_width(ui.available_width());
                            inherit_banner(ui, &facts);
                            ui.add_space(12.0);
                            let (ch, action) = crate::auth_ui::auth_editor(
                                ui,
                                auth,
                                crate::auth_ui::AuthUiCtx {
                                    is_root: !is_folder,
                                    inherited: facts.inherited.clone(),
                                    owner_id: &owner_id,
                                    token: token.clone(),
                                    fetching,
                                    vars: &vars,
                                },
                            );
                            changed |= ch;
                            match action {
                                crate::auth_ui::AuthAction::GetToken => get_token = true,
                                crate::auth_ui::AuthAction::ClearToken => clear_token = true,
                                crate::auth_ui::AuthAction::None => {}
                            }
                        },
                    );
                });
            });
        }
        Section::Scripts => {
            let mut event = tab.event;
            kit::toolbar(ui, |ui| {
                script_events(ui, &mut event, scripts);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let code = match event {
                        ScriptEvent::PostResponse => &mut scripts.post_response,
                        _ => &mut scripts.pre_request,
                    };
                    changed |= snippets_button(ui, event, code);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(kit::hint(
                                ui,
                                tr("Runs for every request inside, before the request's own script."),
                            ))
                            .truncate(),
                        );
                    });
                });
            });
            tab.event = event;
            let code = match tab.event {
                ScriptEvent::PostResponse => &mut scripts.post_response,
                _ => &mut scripts.pre_request,
            };
            changed |= script_well(ui, tab.event, code, &vars);
        }
        Section::Variables => {
            let table_id = ("container-vars", &tab.collection, &tab.folder);
            kit::toolbar(ui, |ui| {
                let w = (ui.available_width() * 0.4).clamp(140.0, 260.0);
                let r =
                    kit::search_field_sm(ui, ("container-filter", &cid), &mut tab.filter, tr("Filter variables"), w);
                widgets::a11y_name(&r, tr("Filter variables"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    crate::env_tab::bulk_toggle(ui, table_id);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let help = if is_folder {
                            tr("Folder variables override collection variables for requests in this folder.")
                        } else {
                            tr("Collection variables are available to every request in the collection.")
                        };
                        ui.add(egui::Label::new(kit::hint(ui, help)).truncate());
                    });
                });
            });
            egui::ScrollArea::vertical().id_salt("container-vars").auto_shrink([false; 2]).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 4, bottom: 10 }).show(
                    ui,
                    |ui| {
                        ui.set_width(ui.available_width());
                        changed |= crate::env_tab::variables_table(ui, table_id, variables, &tab.filter, &vars);
                    },
                );
                variables_hint(ui);
            });
        }
    }
    let resolved_auth = if get_token { Some(auth.clone()) } else { None };
    if changed {
        app.ws.mark_collection(&cid);
    }
    if let Some(a) = resolved_auth {
        let scopes = app.ws.scopes_for(Some(&cid));
        app.fetch_token(a, scopes, owner_id.clone());
    }
    if clear_token {
        TokenStore::set(app.ws.secrets.as_ref(), &owner_id, None);
    }
    for act in acts {
        apply(app, tab, act);
    }
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

/// A number card with an icon before its label (the prototype's `.stat`).
fn stat(ui: &mut Ui, width: f32, icon: &str, label: &str, value: &str, value_size: f32, sub: &str) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(width - 30.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // Rows of 17 / 2 + 32 / 17 (the prototype's line heights).
                let line = |ui: &mut Ui, h: f32, add: &mut dyn FnMut(&mut Ui)| {
                    ui.allocate_ui_with_layout(vec2(width - 30.0, h), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_min_height(h);
                        add(ui);
                    });
                };
                line(ui, 17.0, &mut |ui| {
                    let mut job = LayoutJob::default();
                    job.append(icon, 0.0, TextFormat::simple(theme::icon_font(11.5), p.text_muted));
                    job.append(label, 6.0, TextFormat::simple(FontId::proportional(11.5), p.text_muted));
                    ui.add(egui::Label::new(job).truncate());
                });
                ui.add_space(2.0);
                line(ui, 32.0, &mut |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(value).font(theme::semibold(value_size)).color(p.text))
                            .truncate(),
                    );
                });
                line(ui, 17.0, &mut |ui| {
                    ui.add(egui::Label::new(RichText::new(sub).size(11.5).color(p.text_faint)).truncate());
                });
            });
        });
}

fn overview_stats(ui: &mut Ui, f: &Facts, variables: usize, secrets: usize, auth: AuthType) {
    let width = ui.available_width();
    let gap = 10.0;
    let per_row = if width >= 560.0 { 4 } else { 2 };
    let w = ((width - gap * (per_row as f32 - 1.0)) / per_row as f32).floor();
    let auth_label = match auth {
        AuthType::Inherit => tr("Inherited"),
        k => tr(k.label()),
    };
    let auth_sub = if matches!(auth, AuthType::Inherit | AuthType::None) {
        String::new()
    } else {
        trf("inherited by {} requests", &[&f.inheriting])
    };
    let requests_sub = if f.folders > 0 { trf("in {} folders", &[&f.folders]) } else { String::new() };
    let secrets_sub = match secrets {
        0 => String::new(),
        1 => tr("1 secret").to_string(),
        n => trf("{} secrets", &[&n]),
    };
    let cards = [
        (icons::GLOBE, tr("Requests"), f.requests.to_string(), 22.0, requests_sub),
        (icons::FOLDER_SIMPLE, tr("Folders"), f.folders.to_string(), 22.0, f.folder_names.clone()),
        (icons::BRACKETS_CURLY, tr("Variables"), variables.to_string(), 22.0, secrets_sub),
        (icons::KEY, tr("Authorization"), auth_label.to_string(), 17.0, auth_sub),
    ];
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = gap;
        for chunk in cards.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (icon, label, value, size, sub) in chunk {
                    stat(ui, w, icon, label, value, *size, sub);
                }
            });
        }
    });
}

/// Description: a Markdown editor (Write) or its rendering (Preview).
fn description_ui(ui: &mut Ui, preview: &mut bool, text: &mut String) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            ui.label(kit::section_title(tr("Description")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut mode = *preview;
                kit::segmented_sm(ui, &mut mode, &[(false, tr("Write")), (true, tr("Preview"))]);
                *preview = mode;
            });
        });
        if *preview {
            kit::well(ui).inner_margin(egui::Margin::symmetric(14, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(120.0);
                if text.trim().is_empty() {
                    ui.label(kit::hint(ui, tr("No description yet.")));
                } else {
                    markdown(ui, text);
                }
            });
        } else {
            kit::well(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let rows = text.lines().count().clamp(8, 30);
                let r = widgets::code_editor_gutter(ui, "desc", text, Lang::Text, None, rows, false, None);
                widgets::a11y_name(&r, tr("Description"));
                changed |= r.changed();
            });
        }
        ui.label(kit::hint(ui, tr("Markdown. Shown in the generated collection documentation.")));
    });
    changed
}

/// One line of Markdown with `**bold**`, `*italic*` and `` `code` ``.
fn inline_job(ui: &Ui, text: &str, size: f32, color: Color32) -> LayoutJob {
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    let (mut bold, mut italic, mut code) = (false, false, false);
    let mut buf = String::new();
    let flush = |job: &mut LayoutJob, buf: &mut String, bold: bool, italic: bool, code: bool| {
        if buf.is_empty() {
            return;
        }
        let mut f = if code {
            let mut f = TextFormat::simple(theme::mono(size - 1.0), p.text);
            f.background = p.subtle;
            f
        } else if bold {
            TextFormat::simple(theme::semibold(size), color)
        } else {
            TextFormat::simple(FontId::proportional(size), color)
        };
        f.italics = italic && !code;
        job.append(buf, 0.0, f);
        buf.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '`' => {
                flush(&mut job, &mut buf, bold, italic, code);
                code = !code;
            }
            '*' if !code && chars.peek() == Some(&'*') => {
                chars.next();
                flush(&mut job, &mut buf, bold, italic, code);
                bold = !bold;
            }
            '*' | '_' if !code => {
                flush(&mut job, &mut buf, bold, italic, code);
                italic = !italic;
            }
            c => buf.push(c),
        }
    }
    flush(&mut job, &mut buf, bold, italic, code);
    job
}

/// Markdown drawn simply: headings, bullets and paragraphs.
fn markdown(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.y = 4.0;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            ui.add_space(4.0);
        } else if let Some(h) = t.strip_prefix('#') {
            let level = 1 + h.chars().take_while(|c| *c == '#').count();
            let size = match level {
                1 => 17.0,
                2 => 14.5,
                _ => 13.0,
            };
            let h = h.trim_start_matches('#').trim();
            ui.add_space(4.0);
            ui.label(RichText::new(h).font(theme::semibold(size)).color(p.text));
        } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("•").color(p.text_muted));
                ui.add(egui::Label::new(inline_job(ui, b, 13.0, p.text)).wrap());
            });
        } else {
            ui.add(egui::Label::new(inline_job(ui, t, 13.0, p.text)).wrap());
        }
    }
}

/// One row of the Actions card: a faint icon and a label, 30 high.
fn action_row(ui: &mut Ui, icon: &str, label: &str) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    widgets::a11y_button(&resp, label);
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
        }
        let x = rect.left() + 9.0;
        ui.painter().text(
            pos2(x + 7.0, rect.center().y),
            Align2::CENTER_CENTER,
            icon,
            theme::icon_font(13.0),
            p.text_faint,
        );
        let g = kit::elide(ui, label, FontId::proportional(13.0), p.text, rect.right() - x - 30.0);
        ui.painter().galley(pos2(x + 22.0, rect.center().y - g.size().y / 2.0), g, p.text);
    }
    resp
}

/// The Actions card: what the sidebar's context menu offers for a
/// collection (run, mock, docs, export, fork), and where its files are.
fn actions_card(ui: &mut Ui, is_folder: bool, f: &Facts, acts: &mut Vec<Act>) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(kit::section_title(tr("Actions")));
        egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                if is_folder {
                    if action_row(ui, icons::PLAY, tr("Run folder")).clicked() {
                        acts.push(Act::Run);
                    }
                    return;
                }
                let rows: Vec<(&str, String, Act)> = vec![
                    (icons::PLAY, tr("Run collection").into(), Act::Run),
                    (icons::CUBE, tr("Mock server").into(), Act::Mock),
                    (icons::BOOK_OPEN, tr("Preview documentation").into(), Act::Docs(None)),
                    (
                        icons::EXPORT,
                        trf("{} (Postman v2.1)", &[&tr("Export")]),
                        Act::Export(crate::sidebar::ExportFormat::Postman),
                    ),
                    (
                        icons::EXPORT,
                        trf("{} (OpenAPI 3)", &[&tr("Export")]),
                        Act::Export(crate::sidebar::ExportFormat::OpenApi),
                    ),
                    (icons::FILE_TEXT, tr("Export documentation (HTML)").into(), Act::Docs(Some(false))),
                    (icons::GIT_FORK, format!("{}…", tr("Fork")), Act::Fork),
                ];
                for (icon, label, act) in rows {
                    if action_row(ui, icon, &label).clicked() {
                        acts.push(act);
                    }
                }
                if let Some(parent) = &f.fork_parent {
                    if action_row(ui, icons::ARROW_LINE_DOWN, &format!("{} “{parent}”…", tr("Pull changes from")))
                        .clicked()
                    {
                        acts.push(Act::ForkSync(true));
                    }
                    if action_row(ui, icons::ARROW_UP, &format!("{} “{parent}”…", tr("Merge into"))).clicked() {
                        acts.push(Act::ForkSync(false));
                    }
                }
            });
        if let (Some(dir), false) = (&f.dir, is_folder) {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 4.0;
                let mut job = LayoutJob::default();
                job.append(icons::FOLDER_SIMPLE, 0.0, TextFormat::simple(theme::icon_font(12.0), p.text_muted));
                job.append(dir, 4.0, TextFormat::simple(theme::mono(11.0), p.text_muted));
                job.append(
                    &format!(" · {}", tr("one file per request")),
                    0.0,
                    TextFormat::simple(FontId::proportional(12.0), p.text_muted),
                );
                ui.add(egui::Label::new(job).wrap());
            });
        }
    });
}

// ---------------------------------------------------------------------------
// Authorization, scripts, variables
// ---------------------------------------------------------------------------

/// How many requests inside get this auth (info banner over the editor).
fn inherit_banner(ui: &mut Ui, f: &Facts) {
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    let text = TextFormat::simple(FontId::proportional(12.5), p.text);
    job.append(tr("Requests that use “Inherit auth from parent” get this auth."), 0.0, text.clone());
    if f.requests > 0 {
        job.append(" ", 0.0, text.clone());
        job.append(
            &trf("{} of {} requests inherit it.", &[&f.inheriting, &f.requests]),
            0.0,
            TextFormat::simple(theme::medium(12.5), p.text),
        );
    }
    egui::Frame::new()
        .fill(p.info_soft)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(RichText::new(icons::INFO).font(theme::icon_font(16.0)).color(p.info));
                ui.add(egui::Label::new(job).wrap());
            });
        });
}

/// Pre-request / Post-response, small, with a green dot after the ones
/// that have code.
fn script_events(ui: &mut Ui, event: &mut ScriptEvent, scripts: &ternarywire_core::model::Scripts) {
    let p = theme::pal(ui);
    let items = [
        (ScriptEvent::PreRequest, tr("Pre-request"), !scripts.pre_request.trim().is_empty()),
        (ScriptEvent::PostResponse, tr("Post-response"), !scripts.post_response.trim().is_empty()),
    ];
    let parts: Vec<_> = items
        .iter()
        .map(|(_, label, dot)| {
            let g = ui.painter().layout_no_wrap(label.to_string(), theme::medium(11.5), Color32::PLACEHOLDER);
            let w = g.size().x + if *dot { 13.0 } else { 0.0 } + 16.0;
            (g, w)
        })
        .collect();
    let total = parts.iter().map(|p| p.1).sum::<f32>() + 1.0 + 4.0;
    let (rect, base) = ui.allocate_exact_size(vec2(total, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), p.subtle);
    let dark = ui.visuals().dark_mode;
    let mut x = rect.left() + 2.0;
    for (i, ((value, label, dot), (g, w))) in items.iter().zip(parts).enumerate() {
        let seg = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(w, 22.0));
        x += w + 1.0;
        let resp = ui.interact(seg, base.id.with(("script-event", i)), Sense::click());
        let selected = *event == *value;
        widgets::a11y_tab(&resp, label, selected);
        if resp.clicked() {
            *event = *value;
        }
        let painter = ui.painter();
        if selected {
            if dark {
                painter.rect(
                    seg,
                    CornerRadius::same(6),
                    p.hover,
                    Stroke::new(1.0, p.border_strong),
                    egui::StrokeKind::Inside,
                );
            } else {
                let shadow = egui::Shadow { offset: [0, 1], blur: 2, spread: 0, color: Color32::from_black_alpha(20) };
                painter.add(shadow.as_shape(seg, CornerRadius::same(6)));
                painter.rect(
                    seg,
                    CornerRadius::same(6),
                    p.surface,
                    Stroke::new(0.5, p.border),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let color = if selected || resp.hovered() { p.text } else { p.text_muted };
        let gx = seg.left() + 8.0;
        painter.galley(pos2(gx, seg.center().y - g.size().y / 2.0), g.clone(), color);
        if *dot {
            painter.circle_filled(pos2(gx + g.size().x + 7.0 + 3.0, seg.center().y), 3.0, p.ok);
        }
    }
}

fn insert_snippet(code: &mut String, snippet: &str) {
    if !code.is_empty() && !code.ends_with('\n') {
        code.push('\n');
    }
    code.push_str(snippet);
}

/// The Snippets drop-down: snippets for the event and the pm API reference.
fn snippets_button(ui: &mut Ui, event: ScriptEvent, code: &mut String) -> bool {
    let mut changed = false;
    let r = kit::button(
        ui,
        kit::Kind::Secondary,
        kit::Size::Sm,
        tr("Snippets"),
        kit::ButtonOpts { icon: Some(icons::LIGHTNING), caret: true, ..Default::default() },
    );
    egui::Popup::menu(&r).width(280.0).align(egui::RectAlign::BOTTOM_END).show(|ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for s in crate::snippets::SNIPPETS.iter().filter(|s| event == ScriptEvent::PostResponse || !s.post_only) {
                if kit::menu_item(ui, Some(icons::PLUS), s.title, None)
                    .on_hover_text(RichText::new(s.code).monospace())
                    .clicked()
                {
                    insert_snippet(code, s.code);
                    changed = true;
                }
            }
            kit::menu_sep(ui);
            kit::menu_header(ui, tr("pm API"));
            for (sig, doc) in crate::snippets::PM_COMPLETIONS {
                ui.horizontal(|ui| {
                    ui.add_space(9.0);
                    ui.add(
                        egui::Label::new(RichText::new(*sig).font(theme::mono(11.0)).color(theme::pal(ui).text_muted))
                            .truncate(),
                    )
                    .on_hover_text(*doc);
                });
            }
        });
    });
    changed
}

/// The script editor filling the space left, in a well (12 from the edges).
fn script_well(ui: &mut Ui, event: ScriptEvent, code: &mut String, vars: &widgets::VarCtx) -> bool {
    let avail = ui.available_rect_before_wrap();
    let rect =
        Rect::from_min_max(pos2(avail.left() + 12.0, avail.top()), pos2(avail.right() - 12.0, avail.bottom() - 12.0));
    let mut changed = false;
    ui.scope_builder(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)), |ui| {
        kit::well(ui).show(ui, |ui| {
            ui.set_min_size(rect.size() - vec2(2.0, 2.0));
            egui::ScrollArea::vertical()
                .id_salt(("container-script", event == ScriptEvent::PreRequest))
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let rows = crate::env_tab::editor_rows(ui, rect.height() - 2.0);
                    let r = widgets::code_editor_gutter(
                        ui,
                        ("script", event == ScriptEvent::PreRequest),
                        code,
                        Lang::JavaScript,
                        Some(vars),
                        rows,
                        false,
                        Some(&widgets::pm_completions),
                    );
                    changed |= r.changed();
                });
        });
    });
    ui.advance_cursor_after_rect(avail);
    changed
}

/// Under the variables: where a variable's value comes from.
fn variables_hint(ui: &mut Ui) {
    let p = theme::pal(ui);
    let text = tr("Hover a {} anywhere to see its value and the scope it comes from.");
    let (before, after) = text.split_once("{}").unwrap_or((text, ""));
    let mut job = LayoutJob::default();
    let f = TextFormat::simple(FontId::proportional(12.0), p.text_muted);
    job.append(icons::INFO, 0.0, TextFormat::simple(theme::icon_font(13.0), p.text_muted));
    job.append(before.trim_end(), 14.0, f.clone());
    let mut chip = TextFormat::simple(theme::mono(11.5), p.var_ok);
    chip.background = p.var_ok_bg;
    job.append("{{variable}}", 14.0, chip);
    job.append(after.trim_start(), 14.0, f);
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        ui.add(egui::Label::new(job).wrap());
    });
}

// ---------------------------------------------------------------------------
// Menu and actions
// ---------------------------------------------------------------------------

/// The `⋯` menu of the title row: the collection's (or folder's) context
/// menu of the sidebar.
fn container_menu(ui: &mut Ui, is_folder: bool, fork_parent: Option<&str>, acts: &mut Vec<Act>) {
    if kit::menu_item(ui, Some(icons::GLOBE), tr("Add request"), None).clicked() {
        acts.push(Act::NewRequest);
    }
    let other = kit::menu_item_ex(
        ui,
        MenuItem { icon: Some(icons::PLUS), label: tr("Add other request"), submenu: true, ..Default::default() },
    );
    egui::containers::menu::SubMenu::new().show(ui, &other, |ui| {
        ui.set_min_width(200.0);
        if kit::menu_item(ui, None, tr("Add WebSocket request"), None).clicked() {
            acts.push(Act::NewWs);
        }
        for (label, name, kind) in crate::app::protocol_templates() {
            if kit::menu_item(ui, None, &label.replace("New", "Add"), None).clicked() {
                acts.push(Act::NewProto(name.to_string(), Box::new(kind)));
            }
        }
    });
    if kit::menu_item(ui, Some(icons::FOLDER_PLUS), tr("Add folder"), None).clicked() {
        acts.push(Act::NewFolder);
    }
    kit::menu_sep(ui);
    let run = if is_folder { tr("Run folder") } else { tr("Run collection") };
    if kit::menu_item(ui, Some(icons::PLAY), run, None).clicked() {
        acts.push(Act::Run);
    }
    if !is_folder && kit::menu_item(ui, Some(icons::CUBE), tr("Mock server"), None).clicked() {
        acts.push(Act::Mock);
    }
    kit::menu_sep(ui);
    if kit::menu_item(ui, Some(icons::PENCIL_SIMPLE), tr("Rename"), None).clicked() {
        acts.push(Act::Rename);
    }
    if kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked() {
        acts.push(Act::Duplicate);
    }
    if !is_folder {
        if kit::menu_item(ui, Some(icons::GIT_FORK), &format!("{}…", tr("Fork")), None).clicked() {
            acts.push(Act::Fork);
        }
        if let Some(parent) = fork_parent {
            if kit::menu_item(
                ui,
                Some(icons::ARROW_LINE_DOWN),
                &format!("{} “{parent}”…", tr("Pull changes from")),
                None,
            )
            .clicked()
            {
                acts.push(Act::ForkSync(true));
            }
            if kit::menu_item(ui, Some(icons::ARROW_UP), &format!("{} “{parent}”…", tr("Merge into")), None).clicked()
            {
                acts.push(Act::ForkSync(false));
            }
        }
        let export = kit::menu_item_ex(
            ui,
            MenuItem { icon: Some(icons::EXPORT), label: tr("Export"), submenu: true, ..Default::default() },
        );
        egui::containers::menu::SubMenu::new().show(ui, &export, |ui| {
            ui.set_min_width(200.0);
            if kit::menu_item(ui, None, &trf("{} (Postman v2.1)", &[&tr("Export")]), None).clicked() {
                acts.push(Act::Export(crate::sidebar::ExportFormat::Postman));
            }
            if kit::menu_item(ui, None, &trf("{} (OpenAPI 3)", &[&tr("Export")]), None).clicked() {
                acts.push(Act::Export(crate::sidebar::ExportFormat::OpenApi));
            }
        });
        let docs = kit::menu_item_ex(
            ui,
            MenuItem { icon: Some(icons::BOOK_OPEN), label: tr("Documentation"), submenu: true, ..Default::default() },
        );
        egui::containers::menu::SubMenu::new().show(ui, &docs, |ui| {
            ui.set_min_width(220.0);
            if kit::menu_item(ui, None, tr("Preview documentation"), None).clicked() {
                acts.push(Act::Docs(None));
            }
            if kit::menu_item(ui, None, tr("Export documentation (HTML)"), None).clicked() {
                acts.push(Act::Docs(Some(false)));
            }
            if kit::menu_item(ui, None, tr("Export documentation (Markdown)"), None).clicked() {
                acts.push(Act::Docs(Some(true)));
            }
        });
    }
    kit::menu_sep(ui);
    let delete = MenuItem { icon: Some(icons::TRASH), label: tr("Delete…"), danger: true, ..Default::default() };
    if kit::menu_item_ex(ui, delete).clicked() {
        acts.push(Act::Delete);
    }
}

/// Adds an item to the container (at its end) and, for requests, opens it.
fn add_item(app: &mut TernaryWireApp, cid: &str, parent: Option<&str>, item: Item, open: bool) {
    let item_id = item.id().to_string();
    let name = item.name().to_string();
    let Some(c) = app.ws.data.collection_mut(cid) else { return };
    let at = c.children_mut(parent).map(|v| v.len()).unwrap_or(0);
    c.insert(parent, at, item);
    app.ws.mark_collection(cid);
    app.sidebar.expand_to(cid, parent);
    if open {
        app.open_item(ItemRef { collection: cid.to_string(), item: item_id.clone() });
    }
    app.sidebar.renaming = Some((cid.to_string(), item_id, name));
}

fn apply(app: &mut TernaryWireApp, tab: &mut ContainerTab, act: Act) {
    let cid = tab.collection.clone();
    let fid = tab.folder.clone();
    match act {
        Act::Run => app.open_runner(cid, fid),
        Act::Mock => app.open_mock(cid),
        Act::Rename => {
            let name = app.ws.data.collection(&cid).map(|c| match &fid {
                Some(f) => c.find(f).map(|i| i.name().to_string()).unwrap_or_default(),
                None => c.name.clone(),
            });
            tab.renaming = name;
        }
        Act::Duplicate => match &fid {
            None => {
                if let Some(c) = app.ws.data.collection(&cid).cloned() {
                    let mut copy = c.clone();
                    copy.id = ternarywire_core::util::new_id();
                    copy.name = trf("{} Copy", &[&c.name]);
                    copy.items = copy.items.iter().map(|i| i.duplicate()).collect();
                    app.ws.add_collection(copy);
                }
            }
            Some(f) => {
                if let Some(c) = app.ws.data.collection_mut(&cid)
                    && let Some(item) = c.find(f).cloned()
                {
                    let parent = c.parent_of(f).map(str::to_string);
                    let mut copy = item.duplicate();
                    copy.set_name(trf("{} Copy", &[&item.name()]));
                    let idx = c
                        .children_mut(parent.as_deref())
                        .and_then(|v| v.iter().position(|i| i.id() == f))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    c.insert(parent.as_deref(), idx, copy);
                    app.ws.mark_collection(&cid);
                }
            }
        },
        Act::Delete => {
            let name = app.ws.data.collection(&cid).map(|c| match &fid {
                Some(f) => c.find(f).map(|i| i.name().to_string()).unwrap_or_default(),
                None => c.name.clone(),
            });
            if let Some(name) = name {
                app.dialog = Some(Dialog::confirm_delete(cid, fid, name));
            }
        }
        Act::Fork => {
            let name = app.ws.data.collection(&cid).map(|c| format!("{} (fork)", c.name)).unwrap_or_default();
            app.dialog = Some(Dialog::Prompt { kind: crate::dialogs::Prompt::Fork(cid), text: name });
        }
        Act::ForkSync(pull) => {
            if let Some(s) = crate::fork_ui::ForkSync::new(app, &cid, pull) {
                app.dialog = Some(Dialog::ForkSync(Box::new(s)));
            }
        }
        Act::Export(format) => crate::dialogs::export_collection(app, &cid, format),
        Act::Docs(None) => {
            if let Some(c) = app.ws.data.collection(&cid).cloned() {
                crate::dialogs::preview_docs(app, &c);
            }
        }
        Act::Docs(Some(md)) => crate::dialogs::export_docs(app, &cid, md),
        Act::NewRequest => {
            let item = RequestItem::new("New Request", HttpRequest::default());
            add_item(app, &cid, fid.as_deref(), Item::Request(item), true);
        }
        Act::NewWs => {
            let mut item = RequestItem::new("New WebSocket", HttpRequest::default());
            item.request = RequestKind::WebSocket(WsRequest::default());
            add_item(app, &cid, fid.as_deref(), Item::Request(item), true);
        }
        Act::NewProto(name, kind) => {
            let mut item = RequestItem::new(name, HttpRequest::default());
            item.request = *kind;
            add_item(app, &cid, fid.as_deref(), Item::Request(item), true);
        }
        Act::NewFolder => {
            let f = Folder::new("New Folder");
            add_item(app, &cid, fid.as_deref(), Item::Folder(f), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_requests_that_inherit_the_auth() {
        use ternarywire_core::auth::Auth;
        let mut own = RequestItem::new("Login", HttpRequest::default());
        own.request.auth_mut().kind = AuthType::None;
        let inherits = RequestItem::new("List", HttpRequest::default());
        let mut folder = Folder::new("Books");
        folder.items.push(Item::Request(inherits.clone()));
        let mut locked = Folder::new("Admin");
        locked.auth = Auth { kind: AuthType::Bearer, ..Default::default() };
        locked.items.push(Item::Request(inherits.clone()));
        let items = vec![Item::Request(own), Item::Folder(folder), Item::Folder(locked), Item::Request(inherits)];
        assert_eq!(count_requests(&items), 4);
        assert_eq!(count_folders(&items), 2);
        assert_eq!(count_inheriting(&items), 2);
    }
}
