//! Review dialog for pulling a fork's parent changes or merging a fork back.

use egui::{Align, Align2, CornerRadius, FontId, Layout, Margin, Rect, RichText, Sense, Ui, pos2, vec2};

use ternarywire_core::fork::{self, ChangeKind, DiffOp, Plan};
use ternarywire_core::model::Id;

use crate::app::TernaryWireApp;
use crate::dialogs::{
    Modal, Outcome, body, compact_empty, footer, line_galley, pane_bar, pane_head, room, segmented_sm, split, trailing,
};
use crate::i18n::{tr, trf};
use crate::{icons, kit, theme};

pub struct ForkSync {
    pub fork: Id,
    /// Pull parent → fork, or merge fork → parent.
    pub pull: bool,
    pub plan: Plan,
    pub selected: usize,
}

impl ForkSync {
    pub fn new(app: &TernaryWireApp, fork_id: &str, pull: bool) -> Option<ForkSync> {
        let link = app.ws.fork_of(fork_id)?;
        let parent = app.ws.data.collection(&link.parent)?;
        let fork = app.ws.data.collection(fork_id)?;
        let plan = if pull { fork::plan_pull(link, parent, fork) } else { fork::plan_merge(link, parent, fork) };
        Some(ForkSync { fork: fork_id.to_string(), pull, plan, selected: 0 })
    }
}

/// Creates a fork of `parent` named `name`.
pub fn create(app: &mut TernaryWireApp, parent: &str, name: &str) {
    let Some(p) = app.ws.data.collection(parent) else { return };
    let (fork, link) = fork::create_fork(p, name, chrono::Utc::now().timestamp_millis());
    app.ws.add_collection(fork);
    app.ws.forks.push(link);
    app.ws.save_forks();
    app.toast(tr("Fork created"), false);
}

/// Label and tone of a change kind.
fn kind_style(kind: ChangeKind) -> (&'static str, kit::Tone) {
    match kind {
        ChangeKind::Added => (tr("added"), kit::Tone::Ok),
        ChangeKind::Modified => (tr("changed"), kit::Tone::Info),
        ChangeKind::Removed => (tr("removed"), kit::Tone::Err),
        ChangeKind::Conflict => (tr("conflict"), kit::Tone::Warn),
    }
}

pub fn show(app: &mut TernaryWireApp, ctx: &egui::Context, s: &mut ForkSync) -> Outcome {
    let Some(link) = app.ws.fork_of(&s.fork).cloned() else { return Outcome::Close };
    let parent_name = app.ws.data.collection(&link.parent).map(|c| c.name.clone()).unwrap_or_default();
    let fork_name = app.ws.data.collection(&s.fork).map(|c| c.name.clone()).unwrap_or_default();
    let (source, target) = if s.pull { (&parent_name, &fork_name) } else { (&fork_name, &parent_name) };
    let (title, icon) = if s.pull {
        (trf("Pull changes from “{}”", &[&parent_name]), icons::GIT_FORK)
    } else {
        (trf("Merge into “{}”", &[&parent_name]), icons::GIT_MERGE)
    };
    if s.plan.is_empty() {
        return Modal::new("fork-sync", 460.0).title(title).icon(icon).center().show(ctx, |ui| {
            body(ui, |ui| {
                ui.add_space(8.0);
                compact_empty(ui, icons::CHECK_CIRCLE, tr("Already up to date."), tr("There are no changes to apply."));
                ui.add_space(8.0);
            });
            footer(ui, |ui| {
                trailing(ui, |ui| {
                    if kit::secondary(ui, None, tr("Close")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    }
    let total = s.plan.entries.len();
    let conflicts = s.plan.conflicts();
    let changes = crate::dialogs::plural(total, "{} change", "{} changes");
    let subtitle = match conflicts {
        0 => trf("{} from “{}” to “{}”", &[&changes, source, target]),
        1 => trf("{} from “{}” to “{}”, 1 conflict: choose a side to include it", &[&changes, source, target]),
        n => trf("{} from “{}” to “{}”, {} conflicts: choose a side to include them", &[&changes, source, target, &n]),
    };
    let mut apply = false;
    let out =
        Modal::new("fork-sync", 1000.0).title(title).subtitle(subtitle).icon(icon).sticky().center().show(ctx, |ui| {
            let current = s.selected;
            let mut select = None;
            let n = s.plan.entries.iter().filter(|e| e.apply).count();
            let entries = &mut s.plan.entries;
            let shown =
                entries.get(current).map(|e| (e.name.clone(), e.kind, e.target_text.clone(), e.source_text.clone()));
            let height = room(ui).min(440.0);
            split(
                ui,
                height,
                379.0,
                |ui| {
                    pane_head(ui, tr("Changes"), &trf("{} of {} selected", &[&n, &total]));
                    egui::ScrollArea::vertical().id_salt("fork-list").auto_shrink([false; 2]).show(ui, |ui| {
                        egui::Frame::new().inner_margin(Margin::same(6)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for (i, e) in entries.iter_mut().enumerate() {
                                ui.push_id(i, |ui| {
                                    if change_row(ui, e, i == current, source, target) {
                                        select = Some(i);
                                    }
                                });
                            }
                        });
                    });
                },
                |ui| {
                    let Some((name, kind, old, new)) = &shown else { return };
                    pane_bar(ui, 48.0, 16.0, |ui| {
                        let p = theme::pal(ui);
                        ui.label(RichText::new(name).font(theme::semibold(13.0)).color(p.text));
                        let (label, tone) = kind_style(*kind);
                        kit::tone_pill(ui, label, tone);
                        trailing(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(RichText::new(format!("− {target}")).size(12.0).color(p.err));
                            ui.label(RichText::new(format!("+ {source}")).size(12.0).color(p.ok));
                        });
                    });
                    diff_view(ui, old, new);
                },
            );
            if let Some(i) = select {
                s.selected = i;
            }
            footer(ui, |ui| {
                let p = theme::pal(ui);
                ui.label(RichText::new(trf("{} of {} selected", &[&n, &total])).size(12.0).color(p.text_muted));
                if ui.add_enabled_ui(n < total, |ui| kit::ghost(ui, None, tr("Select all"))).inner.clicked() {
                    s.plan.entries.iter_mut().for_each(|e| e.apply = true);
                }
                if ui.add_enabled_ui(n > 0, |ui| kit::ghost(ui, None, tr("Select none"))).inner.clicked() {
                    s.plan.entries.iter_mut().for_each(|e| e.apply = false);
                }
                trailing(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let label = format!("{} ({n})", tr("Apply"));
                    if ui.add_enabled_ui(n > 0, |ui| kit::primary_icon(ui, icons::GIT_MERGE, &label)).inner.clicked() {
                        apply = true;
                    }
                    if kit::secondary(ui, None, tr("Cancel")).clicked() { Outcome::Close } else { Outcome::Keep }
                })
            })
        });
    if !apply {
        return out;
    }
    let Some(mut link) = app.ws.fork_of(&s.fork).cloned() else { return Outcome::Close };
    let (Some(parent), Some(fork_c)) =
        (app.ws.data.collection(&link.parent).cloned(), app.ws.data.collection(&s.fork).cloned())
    else {
        return Outcome::Close;
    };
    if s.pull {
        let mut updated = fork_c;
        fork::apply_pull(&mut link, &s.plan, &parent, &mut updated);
        if let Some(c) = app.ws.data.collection_mut(&s.fork) {
            *c = updated;
        }
        app.ws.mark_collection(&s.fork);
    } else {
        let mut updated = parent;
        fork::apply_merge(&mut link, &s.plan, &fork_c, &mut updated);
        let pid = link.parent.clone();
        if let Some(c) = app.ws.data.collection_mut(&pid) {
            *c = updated;
        }
        app.ws.mark_collection(&pid);
    }
    if let Some(l) = app.ws.forks.iter_mut().find(|l| l.fork == s.fork) {
        *l = link;
    }
    app.ws.save_forks();
    app.toast(tr("Changes applied"), false);
    Outcome::Close
}

/// One change (`.dl-lrow`): include checkbox, kind pill, icon and name; a
/// conflict that is included gets "Use … / Keep …" under it. The row shown
/// in the diff is raised. Returns true when the row (not a control in it)
/// was clicked.
fn change_row(ui: &mut Ui, e: &mut fork::PlanEntry, selected: bool, source: &str, target: &str) -> bool {
    let p = theme::pal(ui);
    let conflict = e.kind == ChangeKind::Conflict && e.apply;
    let h = if conflict { 6.0 + 22.0 + 8.0 + 26.0 + 6.0 } else { 36.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    crate::widgets::a11y_button(&resp, &e.name);
    let radius = CornerRadius::same(theme::RADIUS);
    if selected {
        if !ui.visuals().dark_mode {
            let shadow =
                egui::Shadow { offset: [0, 1], blur: 2, spread: 0, color: egui::Color32::from_black_alpha(10) };
            ui.painter().add(shadow.as_shape(rect, radius));
        }
        ui.painter().rect(rect, radius, p.surface, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Outside);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, radius, p.hover);
    }
    let cy = if conflict { rect.top() + 6.0 + 11.0 } else { rect.center().y };
    let mut x = rect.left() + 10.0;
    let check = Rect::from_min_size(pos2(x, cy - 7.5), vec2(15.0, 15.0));
    let name = e.name.clone();
    crate::dialogs::place(ui, check, |ui: &mut Ui| kit::check(ui, &mut e.apply, &name))
        .on_hover_text(tr("Include this change"));
    x += 15.0 + 8.0;
    let (label, tone) = kind_style(e.kind);
    let (fg, bg) = p.tone(tone);
    let g = ui.painter().layout_no_wrap(label.to_string(), theme::medium(11.5), fg);
    let pill = Rect::from_min_size(pos2(x, cy - 10.0), vec2(g.size().x + 14.0, 20.0));
    ui.painter().rect_filled(pill, CornerRadius::same(10), bg);
    ui.painter().galley(pill.center() - g.size() / 2.0, g, fg);
    x = pill.right() + 8.0;
    let icon = if e.id == fork::ROOT {
        icons::SLIDERS
    } else if e.is_folder {
        icons::FOLDER_SIMPLE
    } else {
        icons::FILE_TEXT
    };
    let ig = ui.painter().layout_no_wrap(icon.to_string(), theme::icon_font(13.0), p.text_muted);
    let iw = ig.size().x;
    ui.painter().galley(pos2(x, cy - ig.size().y / 2.0), ig, p.text_muted);
    x += iw + 8.0;
    let ng = line_galley(ui, &e.name, FontId::proportional(12.5), rect.right() - 10.0 - x);
    ui.painter().galley(pos2(x, cy - ng.size().y / 2.0), ng, if e.apply { p.text } else { p.text_faint });
    if conflict {
        let seg = Rect::from_min_max(pos2(rect.left() + 10.0 + 23.0, cy + 11.0 + 8.0), rect.max - vec2(10.0, 6.0));
        crate::dialogs::in_rect(ui, seg, Layout::left_to_right(Align::Center), |ui| {
            segmented_sm(
                ui,
                &mut e.take_source,
                &[(true, trf("Use {}", &[&source])), (false, trf("Keep {}", &[&target]))],
            );
        });
    }
    resp.clicked()
}

/// The line diff (`.dl-diffv`): 22 px mono rows with the old and new line
/// numbers, a +/− column, added lines on the ok tint and removed ones on
/// the err tint.
fn diff_view(ui: &mut Ui, old: &str, new: &str) {
    let p = theme::pal(ui);
    let mut rows = Vec::new();
    let (mut a, mut b) = (0usize, 0usize);
    for (op, line) in fork::line_diff(old, new) {
        let (na, nb) = match op {
            DiffOp::Same => {
                a += 1;
                b += 1;
                (Some(a), Some(b))
            }
            DiffOp::Removed => {
                a += 1;
                (Some(a), None)
            }
            DiffOp::Added => {
                b += 1;
                (None, Some(b))
            }
        };
        rows.push((op, na, nb, line));
    }
    let longest = rows.iter().map(|r| r.3.chars().count()).max().unwrap_or(0) as f32;
    egui::ScrollArea::both().id_salt("fork-diff").auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width().max(34.0 * 2.0 + 26.0 + longest * 7.3 + 16.0);
        ui.add_space(8.0);
        for (op, na, nb, line) in &rows {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let (sign, sign_c, bg) = match op {
                DiffOp::Same => ("", p.text_faint, None),
                DiffOp::Removed => ("−", p.err, Some(p.err_soft)),
                DiffOp::Added => ("+", p.ok, Some(p.ok_soft)),
            };
            if let Some(bg) = bg {
                ui.painter().rect_filled(rect, 0.0, bg);
            }
            let cy = rect.center().y;
            let font = theme::mono(12.0);
            for (i, n) in [na, nb].into_iter().enumerate() {
                if let Some(n) = n {
                    let x = rect.left() + 34.0 * (i + 1) as f32 - 6.0;
                    ui.painter().text(pos2(x, cy), Align2::RIGHT_CENTER, n.to_string(), font.clone(), p.text_faint);
                }
            }
            ui.painter().text(pos2(rect.left() + 68.0 + 13.0, cy), Align2::CENTER_CENTER, sign, font.clone(), sign_c);
            // Not wrapped: indentation matters.
            ui.painter().text(pos2(rect.left() + 68.0 + 26.0, cy), Align2::LEFT_CENTER, line, font, p.text);
        }
        ui.add_space(8.0);
    });
}
