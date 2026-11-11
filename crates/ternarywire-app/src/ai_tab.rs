//! AI request tab (FR-X9): chat requests to OpenAI, Anthropic, Gemini or an
//! OpenAI-compatible endpoint, streamed, with tools from MCP requests.

use std::time::Instant;

use egui::{Color32, CornerRadius, RichText, Stroke, Ui};

use ternarywire_ai::ternarywire_mcp::{McpClient, McpConfig};
use ternarywire_ai::{AiConfig, AiEvent, AiResponse, CancellationToken};
use ternarywire_core::auth::{Auth, AuthType};
use ternarywire_core::model::RequestKind;
use ternarywire_core::protocols::{AiMessage, AiProvider, AiRequest};

use crate::app::{Msg, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tab;
use crate::proto_common::{self as pc, Conn, ItemSlot, ScriptState, header, resolver};
use crate::widgets::{self, VarCtx};
use crate::{icons, kit, theme};
use ternarywire_core::script::ScriptEvent;
use ternarywire_core::script::protocol as proto;

pub enum AiMsg {
    Event(AiEvent),
    Models(Result<Vec<String>, String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Messages,
    Settings,
    Auth,
    Tools,
    Scripts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Chat,
    Raw,
}

pub struct AiTab {
    /// Number of the current send (see `app::next_epoch`).
    pub epoch: u64,
    pub slot: ItemSlot,
    section: Section,
    view: View,
    cancel: Option<CancellationToken>,
    text: String,
    /// Tool calls and results of the last run, in order: (call, name, body).
    steps: Vec<(bool, String, String)>,
    usage: Option<(u64, u64)>,
    done: Option<AiResponse>,
    error: Option<String>,
    models: Vec<String>,
    fetching_models: bool,
    /// When the current send started.
    started: Option<Instant>,
    /// Automation: `send`.
    pub auto: Option<String>,
    pub scripts: ScriptState,
    /// Post-response script input, run on the next frame.
    post: Option<serde_json::Value>,
}

impl AiTab {
    pub fn new(slot: ItemSlot) -> Self {
        AiTab {
            epoch: 0,
            slot,
            section: Section::Messages,
            view: View::Chat,
            cancel: None,
            text: String::new(),
            steps: Vec::new(),
            usage: None,
            done: None,
            error: None,
            models: Vec::new(),
            fetching_models: false,
            started: None,
            auto: None,
            scripts: ScriptState::default(),
            post: None,
        }
    }

    /// Generating, or waiting for the pre-request script.
    pub fn is_running(&self) -> bool {
        self.cancel.is_some() || self.scripts.is_waiting()
    }

    pub fn cancel(&mut self) {
        self.scripts.stop_waiting();
        if let Some(c) = self.cancel.take() {
            c.cancel();
            // What the stopped run still sends (text, an error) is dropped.
            self.epoch = crate::app::next_epoch();
        }
        self.started = None;
    }

    pub fn on_msg(&mut self, m: AiMsg) {
        match m {
            AiMsg::Event(e) => match e {
                AiEvent::Started => {}
                AiEvent::Delta(d) => self.text.push_str(&d),
                AiEvent::ToolCall { name, arguments } => {
                    self.steps.push((true, name, serde_json::to_string_pretty(&arguments).unwrap_or_default()))
                }
                AiEvent::ToolResult { name, result } => self.steps.push((false, name, result)),
                AiEvent::Usage { input_tokens, output_tokens } => self.usage = Some((input_tokens, output_tokens)),
                AiEvent::Done(r) => {
                    self.cancel = None;
                    self.started = None;
                    if self.text.is_empty() {
                        self.text = r.text.clone();
                    }
                    let usage = serde_json::json!({"inputTokens": r.input_tokens, "outputTokens": r.output_tokens});
                    let finish = r.finish_reason.clone().unwrap_or_default();
                    self.post = Some(proto::ai_response(&self.text, &r.model, &finish, usage, 200, r.latency_ms));
                    self.done = Some(r);
                }
                AiEvent::Error(e) => {
                    self.cancel = None;
                    self.started = None;
                    self.error = Some(e);
                }
            },
            AiMsg::Models(r) => {
                self.fetching_models = false;
                match r {
                    Ok(m) => self.models = m,
                    Err(e) => self.error = Some(trf("Could not list models: {}", &[&e])),
                }
            }
        }
    }
}

/// The API key (Bearer token) with variables resolved.
fn api_key(req: &AiRequest, resolve: &dyn Fn(&str) -> String) -> Auth {
    let mut auth = req.auth.clone();
    for key in ["token", "value"] {
        let v = auth.get_string(key);
        if !v.is_empty() {
            auth.set(key, resolve(&v));
        }
    }
    auth
}

/// Who wrote a chat bubble.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Who {
    User,
    Assistant,
    Tool,
}

/// A chat bubble (`.bubrow`): an avatar and the bubble, the user's on the
/// right. `label` is the small title in the bubble (`tool` ones keep their
/// case and have an icon); `fill` stretches the bubble to its maximum width
/// (editors). `add` draws the content and returns its result and whether
/// the bubble has focus (drawn with a ring). Returns that result and the
/// bubble's rect.
fn bubble<R>(
    ui: &mut Ui,
    who: Who,
    label: (&str, Option<&str>),
    max: f32,
    fit: Option<f32>,
    add: impl FnOnce(&mut Ui) -> (R, bool),
) -> (R, egui::Rect) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let ai = theme::method_color("AI", dark);
    let max_w = (ui.available_width() * max).min(ui.available_width() - 34.0).max(160.0);
    let layout = if who == Who::User {
        egui::Layout::right_to_left(egui::Align::Min)
    } else {
        egui::Layout::left_to_right(egui::Align::Min)
    };
    let (fill_c, stroke) = match who {
        Who::User => (p.accent_soft, Stroke::NONE),
        Who::Assistant => (p.surface, Stroke::new(1.0, p.border)),
        Who::Tool => (p.well, Stroke::new(1.0, p.border)),
    };
    let (icon, fg, bg) = match who {
        Who::User => (icons::USER, p.accent_text, p.accent_soft),
        _ => (icons::SPARKLE, ai, theme::soft(ai, 0.16)),
    };
    ui.with_layout(layout, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(8), bg);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, theme::icon_font(14.0), fg);
        let inner = egui::Frame::new()
            .fill(fill_c)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 10 })
            .show(ui, |ui| {
                ui.set_max_width(max_w - 24.0);
                if let Some(w) = fit {
                    // Editors are as wide as their text (within the bubble's maximum).
                    let w = w.clamp(150.0, max_w - 24.0);
                    ui.set_min_width(w);
                    ui.set_max_width(w);
                }
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let label_h = if label.1.is_some() { 18.0 } else { 15.0 };
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), label_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            match label.1 {
                                Some(icon) => {
                                    ui.label(RichText::new(icon).font(theme::icon_font(12.0)).color(p.text_muted));
                                    ui.label(RichText::new(label.0).font(theme::semibold(11.5)).color(p.text_muted));
                                }
                                None => {
                                    ui.label(kit::eyebrow(ui, label.0).size(10.5));
                                }
                            }
                        },
                    );
                    ui.spacing_mut().item_spacing.y = 4.0;
                    add(ui)
                })
                .inner
            });
        let (r, focused) = inner.inner;
        if focused {
            crate::kit::focus_ring(ui, inner.response.rect, 12);
        }
        (r, inner.response.rect)
    })
    .inner
}

/// The answer as text, with `**bold**` spans set in semibold.
fn answer_job(text: &str, p: &theme::Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let regular = egui::TextFormat {
        line_height: Some(20.0),
        ..egui::TextFormat::simple(egui::FontId::proportional(13.0), p.text)
    };
    let bold = egui::TextFormat { font_id: theme::semibold(13.0), ..regular.clone() };
    for (i, part) in text.split("**").enumerate() {
        if !part.is_empty() {
            job.append(part, 0.0, if i % 2 == 1 { bold.clone() } else { regular.clone() });
        }
    }
    job
}

/// Three dots that pulse while the answer streams.
fn typing_dots(ui: &mut Ui) {
    let p = theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 16.0), egui::Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = (t * 4.0 - i as f32 * 0.7).sin() * 0.5 + 0.5;
        let c = egui::pos2(rect.left() + 4.0 + i as f32 * 8.0, rect.center().y);
        ui.painter().circle_filled(c, 2.5, theme::soft(p.text_faint, 0.35 + phase * 0.65));
    }
    ui.ctx().request_repaint();
}

/// A slider like the prototype's (`.pslider`): a 4 px track, the value in
/// accent and a 14 px knob. Returns true when the value changed.
fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32, name: &str) -> bool {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(150.0, 18.0), egui::Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let mut changed = false;
    if let Some(pos) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let v = ((lo + t * (hi - lo)) / step).round() * step;
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            changed = true;
        }
    }
    if resp.has_focus() {
        let (left, right) = ui.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight)));
        if left || right {
            *value = (*value + if right { step } else { -step }).clamp(lo, hi);
            changed = true;
        }
    }
    let v = *value;
    resp.widget_info(|| egui::WidgetInfo::slider(true, v as f64, name));
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
    ui.painter().rect_filled(track, CornerRadius::same(2), p.subtle);
    let fill = egui::Rect::from_min_max(track.min, egui::pos2(track.left() + track.width() * t, track.bottom()));
    ui.painter().rect_filled(fill, CornerRadius::same(2), p.accent);
    let knob = egui::pos2(track.left() + track.width() * t, rect.center().y);
    ui.painter().circle_filled(knob + egui::vec2(0.0, 1.0), 7.5, Color32::from_black_alpha(40));
    ui.painter().circle(knob, 7.0, Color32::WHITE, Stroke::new(1.0, p.border_strong));
    if resp.has_focus() {
        ui.painter().circle_stroke(knob, 9.0, Stroke::new(1.5, p.accent));
    }
    changed
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut AiTab) {
    let Some(original) = tab.slot.load(app) else {
        ui.label(tr("This request no longer exists."));
        return;
    };
    let mut item = original.clone();
    // The rows of the view are edge to edge.
    ui.spacing_mut().item_spacing.y = 0.0;
    let collection = tab.slot.collection_id();
    let vars = app.var_ctx(collection.as_deref());
    let running = tab.is_running();
    // MCP requests whose tools can be offered, by collection.
    let mcp: Vec<(String, String, String)> = app
        .ws
        .data
        .collections
        .iter()
        .flat_map(|c| {
            c.requests()
                .into_iter()
                .filter(|r| matches!(r.request, RequestKind::Mcp(_)))
                .map(move |r| (r.id.clone(), r.name.clone(), c.name.clone()))
        })
        .collect();
    header(ui, app, tab_id, &tab.slot, &mut item, |_| {});
    let scripts = &mut item.scripts;
    let RequestKind::Ai(req) = &mut item.request else { return };
    let mut send = false;
    let mut stop = false;
    let mut retry = false;
    let mut add_answer = false;
    let mut fetch_models = false;
    let suggestions: Vec<String> = if tab.models.is_empty() {
        ternarywire_ai::suggested_models(req.provider).iter().map(|s| s.to_string()).collect()
    } else {
        tab.models.clone()
    };
    let fetching = tab.fetching_models;
    let mut clear_models = false;
    let panes = ui.make_persistent_id("ai-panes");
    let section = &mut tab.section;
    let view = &mut tab.view;
    let scripts_state = &mut tab.scripts;
    let (text, steps, done, error, started, usage) =
        (&tab.text, &tab.steps, &tab.done, &tab.error, tab.started, tab.usage);
    pc::view_panes(
        ui,
        panes,
        |ui| {
            let clicked = pc::connect_bar(
                ui,
                "AI",
                96.0,
                |ui| {
                    let label = req.provider.label();
                    let w = pc::bar_combo_width(ui, label);
                    let r = pc::bar_combo(ui, tr("Provider"), w, |ui, rect| {
                        let p = theme::pal(ui);
                        ui.painter().text(
                            rect.left_center(),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(12.5),
                            p.text,
                        );
                    });
                    let r = r.on_hover_text(tr("Provider"));
                    egui::Popup::menu(&r).width(200.0).show(|ui| {
                        for p in AiProvider::ALL {
                            let item = kit::MenuItem {
                                checked: Some(req.provider == p),
                                label: p.label(),
                                ..Default::default()
                            };
                            if kit::menu_item_ex(ui, item).clicked() && req.provider != p {
                                req.provider = p;
                                req.model.clear();
                                clear_models = true;
                            }
                        }
                    });
                    pc::bar_divider(ui);
                    let w = (ui.available_width() - 33.0).max(80.0);
                    let r = pc::bar_var(ui, "ai-model", &mut req.model, &vars, tr("model"), w);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !running {
                        send = true;
                    }
                    let focused = r.has_focus();
                    r.on_hover_text(tr("Model"));
                    pc::bar_divider(ui);
                    let more = pc::bar_combo(ui, tr("Choose a model"), ui.available_width(), |_, _| {});
                    let more = more.on_hover_text(tr("Choose a model"));
                    egui::Popup::menu(&more).width(260.0).show(|ui| {
                        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            for m in &suggestions {
                                let item =
                                    kit::MenuItem { checked: Some(&req.model == m), label: m, ..Default::default() };
                                if kit::menu_item_ex(ui, item).clicked() {
                                    req.model = m.clone();
                                }
                            }
                        });
                        kit::menu_sep(ui);
                        if fetching {
                            ui.horizontal(|ui| {
                                ui.add_space(9.0);
                                ui.spinner();
                                ui.label(tr("Fetching models…"));
                            });
                        } else if kit::menu_item(
                            ui,
                            Some(icons::ARROWS_CLOCKWISE),
                            tr("Fetch models from provider"),
                            None,
                        )
                        .clicked()
                        {
                            fetch_models = true;
                        }
                    });
                    focused
                },
                |ui| {
                    if running {
                        stop = kit::action_button(ui, kit::Kind::Secondary, icons::STOP, tr("Stop")).clicked();
                        false
                    } else {
                        kit::action_button(ui, kit::Kind::Primary, icons::PAPER_PLANE_RIGHT, tr("Send"))
                            .on_hover_text(format!("{} (⇧⌘↩)", tr("Send")))
                            .clicked()
                    }
                },
            );
            send |= clicked;
            let tabs = [
                Tab::new(Section::Messages, tr("Conversation")).count(req.messages.len()),
                Tab::new(Section::Settings, tr("Settings")),
                Tab::new(Section::Auth, tr("Auth")),
                Tab::new(Section::Tools, tr("MCP tools")).count(req.mcp_servers.len()),
                Tab::new(Section::Scripts, tr("Scripts")).count(pc::scripts_count(scripts)),
            ];
            kit::tabs(ui, section, &tabs);
            match *section {
                Section::Messages => conversation(ui, req, &vars, running),
                Section::Settings => settings(ui, req, &vars),
                Section::Auth => {
                    pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("API key"), None), |_| {});
                    pc::body(ui, "ai-auth", 0.0, 14.0, |ui| {
                        ui.label(kit::hint(
                            ui,
                            tr("API key, sent as the provider expects. Keep it in the vault, e.g. {{vault:openai}}."),
                        ));
                        // Other auth (Inherit, …) sends no key; it becomes a Bearer
                        // token once one is typed, not by looking at the section.
                        let keyed = matches!(req.auth.kind, AuthType::Bearer | AuthType::ApiKey);
                        let key = if req.auth.kind == AuthType::ApiKey { "value" } else { "token" };
                        let mut v = if keyed { req.auth.get_string(key) } else { String::new() };
                        kit::field(ui, tr("API key"), "", |ui| {
                            let w = ui.available_width();
                            let r = if v.is_empty() || ternarywire_core::variables::is_pure_reference(&v) {
                                pc::var_input_sm(ui, "ai-key", &mut v, &vars, "{{vault:openai}}", w)
                            } else {
                                widgets::secret_field(ui, "ai-key", &mut v, w)
                            };
                            if r.changed() {
                                if !keyed {
                                    req.auth = Auth::bearer("");
                                }
                                req.auth.set(key, v.clone());
                            }
                        });
                        if req.provider == AiProvider::OpenAiCompatible {
                            ui.label(kit::hint(
                                ui,
                                tr("Local servers such as Ollama or LM Studio usually need no key."),
                            ));
                        }
                    });
                }
                Section::Scripts => pc::scripts_ui(
                    ui,
                    scripts,
                    &[(ScriptEvent::PreRequest, "Before send"), (ScriptEvent::PostResponse, "After response")],
                    scripts_state,
                    &vars,
                ),
                Section::Tools => {
                    pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("MCP tools"), None), |_| {});
                    pc::body(ui, "ai-tools", 0.0, 14.0, |ui| {
                        ui.label(kit::hint(
                            ui,
                            tr("Tools of the selected MCP requests are offered to the model; calls run through those servers."),
                        ));
                        if mcp.is_empty() {
                            kit::empty_state(
                                ui,
                                icons::PUZZLE_PIECE,
                                tr("No MCP servers yet"),
                                tr("No MCP requests saved in this workspace yet."),
                                |_| {},
                            );
                            return;
                        }
                        let p = theme::pal(ui);
                        pc::list_box(ui, mcp.len(), 40.0, |ui, i| {
                            let (id, name, coll) = &mcp[i];
                            let mut on = req.mcp_servers.contains(id);
                            if kit::check(ui, &mut on, name).changed() {
                                if on {
                                    req.mcp_servers.push(id.clone());
                                } else {
                                    req.mcp_servers.retain(|x| x != id);
                                }
                            }
                            ui.label(
                                RichText::new(icons::PUZZLE_PIECE).font(theme::icon_font(15.0)).color(p.text_faint),
                            );
                            ui.label(RichText::new(name).font(theme::medium(13.0)).color(p.text));
                            ui.add(egui::Label::new(RichText::new(coll).size(11.0).color(p.text_faint)).truncate());
                        });
                    });
                }
            }
        },
        |ui| {
            let p = theme::pal(ui);
            pc::status_row(
                ui,
                |ui| {
                    if running {
                        pc::status_pill(ui, Conn::Busy, tr("Generating…"));
                        if let Some(s) = started {
                            pc::metrics(ui, |ui| {
                                kit::metric(
                                    ui,
                                    icons::TIMER,
                                    &ternarywire_core::util::human_ms(s.elapsed().as_secs_f64() * 1000.0),
                                );
                            });
                            if let Some((i, o)) = usage {
                                kit::tone_pill(ui, &trf("{} out", &[&o]), kit::Tone::Neutral)
                                    .on_hover_text(tr("Output tokens"));
                                kit::tone_pill(ui, &trf("{} in", &[&i]), kit::Tone::Neutral)
                                    .on_hover_text(tr("Input tokens"));
                            }
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                        }
                    } else if error.is_some() {
                        pc::status_pill(ui, Conn::Failed, tr("Failed"));
                    } else if let Some(r) = done {
                        let opts = kit::PillOpts { large: true, dot: true, ..Default::default() };
                        kit::pill_ex(ui, tr("Completed"), kit::Tone::Ok, opts).on_hover_text(&r.model);
                        let total = ternarywire_core::util::human_ms(r.latency_ms);
                        pc::metrics(ui, |ui| {
                            kit::metric(ui, icons::TIMER, &total).on_hover_ui(|ui| {
                                if let Some(f) = r.first_token_ms {
                                    ui.label(trf("first token {}", &[&ternarywire_core::util::human_ms(f)]));
                                }
                                ui.label(trf("Total: {}", &[&total]));
                                ui.label(RichText::new(&r.model).monospace());
                            });
                            if let Some(f) = r.first_token_ms {
                                pc::metric_text(ui, &format!("TTFT {}", ternarywire_core::util::human_ms(f)))
                                    .on_hover_text(tr("Time to first token"));
                            }
                        });
                        if let Some(o) = r.output_tokens {
                            kit::tone_pill(ui, &trf("{} out", &[&o]), kit::Tone::Neutral)
                                .on_hover_text(tr("Output tokens"));
                        }
                        if let Some(i) = r.input_tokens {
                            kit::tone_pill(ui, &trf("{} in", &[&i]), kit::Tone::Neutral)
                                .on_hover_text(tr("Input tokens"));
                        }
                        if let Some(f) = &r.finish_reason {
                            let ok = f == "stop" || f == "end_turn" || f == "STOP";
                            kit::tone_pill(ui, f, if ok { kit::Tone::Ok } else { kit::Tone::Warn })
                                .on_hover_text(tr("Finish reason"));
                        }
                    } else {
                        pc::status_pill(ui, Conn::Idle, tr("Not sent yet"));
                    }
                },
                |ui| pc::pane_actions(ui, panes),
            );
            kit::tabs(ui, view, &[Tab::new(View::Chat, tr("Answer")), Tab::new(View::Raw, tr("Raw response"))]);
            if *view == View::Raw {
                match done.as_ref().map(|r| serde_json::to_string_pretty(&r.raw).unwrap_or_default()) {
                    Some(raw) => {
                        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                            let h = ui.available_height();
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
                            ui.painter().rect(
                                rect,
                                CornerRadius::same(8),
                                p.surface,
                                Stroke::new(1.0, p.border),
                                egui::StrokeKind::Inside,
                            );
                            let inner = rect.shrink(1.0);
                            pc::inside(ui, egui::UiBuilder::new().max_rect(inner), |ui| {
                                egui::ScrollArea::vertical().id_salt("ai-raw").auto_shrink([false; 2]).show(ui, |ui| {
                                    let mut text = raw.clone();
                                    widgets::code_editor_gutter(
                                        ui,
                                        "ai-raw-text",
                                        &mut text,
                                        Lang::Json,
                                        None,
                                        1,
                                        true,
                                        None,
                                    );
                                });
                            });
                        });
                    }
                    None => kit::empty_state(
                        ui,
                        icons::BRACKETS_CURLY,
                        tr("No response yet"),
                        tr("The provider's response appears here after a send."),
                        |_| {},
                    ),
                }
                return;
            }
            let (a, r) = answer(ui, running, text, steps, done.is_some(), error.as_deref());
            add_answer = a;
            retry = r;
        },
    );
    send |= retry;
    if add_answer {
        req.messages.push(AiMessage { role: "assistant".into(), content: tab.text.clone() });
        req.messages.push(AiMessage { role: "user".into(), content: String::new() });
        tab.text.clear();
        tab.steps.clear();
        tab.done = None;
    }
    if clear_models {
        tab.models.clear();
    }
    if stop {
        tab.cancel();
    }
    if !running && pc::send_shortcut(ui) {
        send = true;
    }
    if tab.auto.as_deref() == Some("send") {
        send = !tab.is_running();
        tab.auto = None;
    }
    let req_clone = req.clone();
    let item_now = item.clone();
    tab.slot.store(app, &original, item);
    if fetch_models {
        let r = resolver(app, collection.as_deref());
        let auth = api_key(&req_clone, &r);
        let key = match auth.kind {
            AuthType::Bearer => Some(auth.get_string("token")),
            AuthType::ApiKey => Some(auth.get_string("value")),
            _ => None,
        };
        let base = r(&req_clone.base_url);
        let provider = req_clone.provider;
        match app.http.default_client() {
            Ok(http) => {
                tab.fetching_models = true;
                let tx = app.tx.clone();
                let ctx = app.ctx.clone();
                app.rt.spawn(async move {
                    let res = ternarywire_ai::list_models(provider, &base, key.as_deref(), &http)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Msg::Ai { tab: tab_id, epoch: 0, event: AiMsg::Models(res) });
                    ctx.request_repaint();
                });
            }
            Err(e) => tab.error = Some(e.message),
        }
    }
    if send && !tab.is_running() {
        tab.scripts.clear_results();
        tab.view = View::Chat;
        if let Some(run) = crate::proto_common::run_scripts(
            app,
            tab_id,
            &tab.slot,
            &item_now,
            ScriptEvent::PreRequest,
            proto::ai(&req_clone),
        ) {
            tab.scripts.wait_for(run);
        } else {
            start(app, tab_id, tab, req_clone.clone(), collection.as_deref());
        }
    }
    if let Some(pre) = tab.scripts.take_pre() {
        match pre {
            None => tab.error = Some(tr("Skipped by pm.execution.skipRequest()").into()),
            Some(v) => {
                let mut sent = req_clone.clone();
                proto::apply_ai(&mut sent, &v);
                start(app, tab_id, tab, sent, collection.as_deref());
            }
        }
    }
    if let Some(v) = tab.post.take() {
        crate::proto_common::run_scripts(
            app,
            tab_id,
            &tab.slot,
            &item_now,
            ScriptEvent::PostResponse,
            proto::ai(&req_clone).with_response(v),
        );
    }
}

/// The conversation to send: the system prompt, then the request's
/// messages as editable bubbles (swap the role, remove), and buttons to add
/// one.
fn conversation(ui: &mut Ui, req: &mut AiRequest, vars: &VarCtx, running: bool) {
    let p = theme::pal(ui);
    pc::body(ui, "ai-chat", 12.0, 12.0, |ui| {
        // System prompt.
        egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(icons::GEAR).font(theme::icon_font(14.0)).color(p.text_faint));
                    ui.label(kit::section_title(tr("System prompt")));
                });
                egui::Frame::new()
                    .fill(p.well)
                    .corner_radius(CornerRadius::same(theme::RADIUS))
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let r = pc::plain_editor(ui, "ai-system", &mut req.system, Lang::Text, Some(vars), 2);
                        if req.system.is_empty() && !r.has_focus() {
                            ui.painter().text(
                                r.rect.left_top() + egui::vec2(0.0, 2.0),
                                egui::Align2::LEFT_TOP,
                                tr("Optional instructions for the model"),
                                theme::mono(12.0),
                                p.text_faint,
                            );
                        }
                    });
            });
        // Messages.
        let mut remove = None;
        let mut flip = None;
        let n = req.messages.len();
        for (i, m) in req.messages.iter_mut().enumerate() {
            let who = if m.role == "assistant" { Who::Assistant } else { Who::User };
            let label = if who == Who::User { tr("User") } else { tr("Assistant") };
            ui.push_id(("ai-msg", i), |ui| {
                let wrap = (ui.available_width() * 0.86 - 34.0 - 24.0).max(100.0);
                let text_w = ui
                    .painter()
                    .layout(m.content.clone(), theme::mono(12.5), Color32::PLACEHOLDER, wrap - 10.0)
                    .size()
                    .x;
                let (focused, rect) = bubble(ui, who, (label, None), 0.86, Some(text_w + 8.0), |ui| {
                    let rows = if i + 1 == n { 2 } else { 1 };
                    let r = pc::plain_editor(ui, ("ai-msg", i), &mut m.content, Lang::Text, Some(vars), rows);
                    if m.content.is_empty() && !r.has_focus() {
                        ui.painter().text(
                            r.rect.left_top() + egui::vec2(0.0, 2.0),
                            egui::Align2::LEFT_TOP,
                            tr("Write a message…"),
                            theme::mono(12.0),
                            p.text_faint,
                        );
                    }
                    (r.has_focus(), r.has_focus())
                });
                // Swap and remove at the top right of the bubble, on hover.
                if focused || ui.rect_contains_pointer(rect) {
                    let at = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 4.0 - 44.0, rect.top() + 4.0),
                        egui::vec2(44.0, 22.0),
                    );
                    pc::inside(
                        ui,
                        egui::UiBuilder::new().max_rect(at).layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let other = if who == Who::User { tr("Make assistant") } else { tr("Make user") };
                            if kit::icon_button_sm(ui, icons::ARROWS_DOWN_UP, other).clicked() {
                                flip = Some(i);
                            }
                            if kit::icon_button_sm(ui, icons::TRASH, tr("Remove message")).clicked() {
                                remove = Some(i);
                            }
                        },
                    );
                }
            });
        }
        if let Some(i) = flip {
            let m = &mut req.messages[i];
            m.role = if m.role == "assistant" { "user".into() } else { "assistant".into() };
        }
        if let Some(i) = remove {
            req.messages.remove(i);
        }
        ui.horizontal(|ui| {
            if kit::ghost(ui, Some(icons::PLUS), tr("User message")).clicked() {
                req.messages.push(AiMessage { role: "user".into(), content: String::new() });
            }
            if kit::ghost(ui, Some(icons::PLUS), tr("Assistant message")).clicked() {
                req.messages.push(AiMessage { role: "assistant".into(), content: String::new() });
            }
            if !running {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(kit::hint(ui, tr("to send")));
                    kit::kbd(ui, "⇧⌘↩");
                });
            }
        });
    });
}

/// Model settings: base URL, streaming, temperature, max tokens and the
/// extra body.
fn settings(ui: &mut Ui, req: &mut AiRequest, vars: &VarCtx) {
    let p = theme::pal(ui);
    pc::toolbar(ui, |ui| pc::toolbar_title(ui, tr("Model settings"), None), |_| {});
    pc::body(ui, "ai-settings", 0.0, 14.0, |ui| {
        kit::group(ui, "", "", |ui, rows| {
            rows.row(ui, tr("Base URL"), "", |ui| {
                pc::var_input_sm(
                    ui,
                    "ai-base",
                    &mut req.base_url,
                    vars,
                    ternarywire_ai::default_base_url(req.provider),
                    220.0,
                );
            });
            rows.row(ui, tr("Stream"), tr("Stream the response as it is generated"), |ui| {
                kit::check(ui, &mut req.stream, tr("Stream"));
            });
            rows.block(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut on = req.temperature.is_some();
                    if kit::checkbox(ui, &mut on, tr("Temperature")).changed() {
                        req.temperature = on.then_some(1.0);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &mut req.temperature {
                        Some(t) => {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            ui.label(RichText::new(format!("{t:.2}")).font(theme::mono(12.0)).color(p.text));
                            slider(ui, t, 0.0..=2.0, 0.05, tr("Temperature"));
                        }
                        None => {
                            ui.label(RichText::new(tr("provider default")).size(12.0).color(p.text_faint));
                        }
                    });
                });
            });
            rows.block(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut on = req.max_tokens.is_some();
                    if kit::checkbox(ui, &mut on, tr("Max tokens")).changed() {
                        req.max_tokens = on.then_some(1024);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &mut req.max_tokens {
                        Some(t) => {
                            pc::number_input(ui, "ai-max-tokens", t, 90.0, tr("Max tokens"));
                        }
                        None => {
                            ui.label(RichText::new(tr("provider default")).size(12.0).color(p.text_faint));
                        }
                    });
                });
            });
        });
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(kit::section_title(tr("Extra body")));
            ui.label(kit::hint(ui, tr("JSON merged into the provider request")));
            let r = pc::editor_well(
                ui,
                "ai-extra",
                &mut req.extra,
                Lang::Json,
                Some(vars),
                egui::Margin::ZERO,
                Some(80.0),
                None,
            );
            pc::well_placeholder(ui, &r, &req.extra, "{ \"top_p\": 0.9 }");
        });
    });
}

/// The answer of the last run: tool calls and results, the streamed answer
/// with Add to conversation and Copy, or the error. Returns (add the answer
/// to the conversation, retry).
fn answer(
    ui: &mut Ui,
    running: bool,
    text: &str,
    steps: &[(bool, String, String)],
    done: bool,
    error: Option<&str>,
) -> (bool, bool) {
    let p = theme::pal(ui);
    let mut add = false;
    let mut retry = false;
    if !running && text.is_empty() && error.is_none() && !done && steps.is_empty() {
        kit::empty_state(
            ui,
            icons::SPARKLE,
            tr("No answer yet"),
            tr("Send the request to see the model's answer."),
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    kit::kbd(ui, "⇧⌘↩");
                    ui.label(kit::hint(ui, tr("to send")));
                });
            },
        );
        return (false, false);
    }
    egui::ScrollArea::vertical().id_salt("ai-answer").stick_to_bottom(running).auto_shrink([false; 2]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 12, bottom: 16 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            for (i, (is_call, name, body)) in steps.iter().enumerate() {
                let (label, icon) = if *is_call {
                    (trf("Tool call · {}", &[name]), icons::PUZZLE_PIECE)
                } else {
                    (trf("Tool result · {}", &[name]), icons::CHECK_CIRCLE)
                };
                ui.push_id(("ai-step", i), |ui| {
                    bubble(ui, Who::Tool, (&label, Some(icon)), 0.86, None, |ui| {
                        let short = if *is_call {
                            format!("{name}({})", crate::dialogs::ellipsize(&pc::preview_text(body), 150))
                        } else {
                            crate::dialogs::ellipsize(&pc::preview_text(body), 160)
                        };
                        ui.add(egui::Label::new(RichText::new(short).font(theme::mono(11.5)).color(p.text)).wrap());
                        let id = ui.make_persistent_id(("ai-step-full", i));
                        if pc::disclosure(ui, id, tr("Details"), None, false, |_| {}) {
                            pc::code_block(ui, ("ai-step-text", i), body);
                        }
                        ((), false)
                    });
                });
            }
            if running || !text.is_empty() {
                bubble(ui, Who::Assistant, (tr("Assistant"), None), 0.92, None, |ui| {
                    if !text.is_empty() {
                        ui.add(egui::Label::new(answer_job(text, &p)).selectable(true).wrap());
                    }
                    if running {
                        typing_dots(ui);
                    }
                    ((), false)
                });
                if !running && !text.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(34.0);
                        let opts = kit::ButtonOpts { icon: Some(icons::PLUS), ..Default::default() };
                        if kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Add to conversation"), opts)
                            .on_hover_text(tr("Append the answer as an assistant message"))
                            .clicked()
                        {
                            add = true;
                        }
                        if kit::icon_button_sm(ui, icons::COPY, tr("Copy answer")).clicked() {
                            ui.ctx().copy_text(text.to_string());
                        }
                    });
                }
            }
            if let Some(e) = error {
                pc::error_banner(ui, tr("The model could not answer"), e, |ui| {
                    let opts = kit::ButtonOpts { icon: Some(icons::ARROW_CLOCKWISE), ..Default::default() };
                    retry = kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Retry"), opts).clicked();
                });
            }
        });
    });
    (add, retry)
}

pub fn start(app: &mut TernaryWireApp, tab_id: u64, tab: &mut AiTab, mut req: AiRequest, collection: Option<&str>) {
    let r = resolver(app, collection);
    req.auth = api_key(&req, &r);
    tab.text.clear();
    tab.steps.clear();
    tab.usage = None;
    tab.done = None;
    tab.error = None;
    let cfg = match AiConfig::from_request(&req, &r) {
        Ok(c) => c,
        Err(e) => {
            tab.error = Some(e.to_string());
            return;
        }
    };
    let http = match app.http.default_client() {
        Ok(c) => c,
        Err(e) => {
            tab.error = Some(e.message);
            return;
        }
    };
    // MCP servers whose tools are offered, with their own variable scopes.
    let mut servers = Vec::new();
    for id in &req.mcp_servers {
        for c in &app.ws.data.collections {
            if let Some(item) = c.find_request(id)
                && let RequestKind::Mcp(m) = &item.request
            {
                let resolve = resolver(app, Some(&c.id));
                let m = crate::mcp_tab::with_inherited_auth(app, Some(&c.id), &item.id, m);
                servers.push((item.name.clone(), McpConfig::from_request(&m, &resolve)));
            }
        }
    }
    let cancel = CancellationToken::new();
    tab.cancel = Some(cancel.clone());
    tab.started = Some(Instant::now());
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    tab.epoch = crate::app::next_epoch();
    let epoch = tab.epoch;
    app.rt.spawn(async move {
        let emit = {
            let tx = tx.clone();
            let ctx = ctx.clone();
            move |e: AiEvent| {
                let _ = tx.send(Msg::Ai { tab: tab_id, epoch, event: AiMsg::Event(e) });
                ctx.request_repaint();
            }
        };
        let mut clients = Vec::new();
        let mut tools = Vec::new();
        for (name, cfg) in servers {
            match McpClient::connect_with_http(cfg, http.clone(), |_| {}).await {
                Ok(client) => {
                    match client.list_tools().await {
                        Ok(list) => tools.extend(list.into_iter().map(|t| (client.clone(), t))),
                        Err(e) => {
                            emit(AiEvent::ToolResult { name: format!("{name} (tools/list)"), result: e.to_string() })
                        }
                    }
                    clients.push(client);
                }
                Err(e) => emit(AiEvent::ToolResult { name: format!("{name} (connect)"), result: e.to_string() }),
            }
        }
        let _ = ternarywire_ai::run(cfg, &http, tools, emit, cancel).await;
        for c in clients {
            c.close().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::TabKind;
    use ternarywire_core::model::{HttpRequest, RequestItem};

    fn ai_tab(app: &mut TernaryWireApp, auth: Auth) -> u64 {
        let mut item = RequestItem::new("ai", HttpRequest::default());
        item.request = RequestKind::Ai(AiRequest { model: "m".into(), auth, ..Default::default() });
        let mut tab = AiTab::new(ItemSlot { location: None, scratch: Some(item) });
        tab.section = Section::Auth;
        app.add_tab(TabKind::Ai(Box::new(tab)))
    }

    fn auth(app: &TernaryWireApp) -> Auth {
        match &app.tabs[app.active].kind.scratch().unwrap().request {
            RequestKind::Ai(r) => r.auth.clone(),
            _ => panic!("AI request"),
        }
    }

    /// Showing the Authorization section changes nothing: an inherited (or
    /// any other) auth used to be replaced with an empty Bearer token.
    #[test]
    fn viewing_the_auth_section_keeps_the_auth() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        ai_tab(&mut app, Auth::default());
        app.draw_frame(Vec::new());
        app.draw_frame(Vec::new());
        assert_eq!(auth(&app), Auth::default());
        let basic = Auth::basic("u", "p");
        ai_tab(&mut app, basic.clone());
        app.draw_frame(Vec::new());
        assert_eq!(auth(&app), basic);
    }

    /// Text the model still streams after Stop does not show up.
    #[test]
    fn stopped_answers_stay_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let id = ai_tab(&mut app, Auth::default());
        let TabKind::Ai(t) = &mut app.tabs[0].kind else { panic!("AI tab") };
        t.epoch = crate::app::next_epoch();
        t.cancel = Some(CancellationToken::new());
        let running = t.epoch;
        t.cancel();
        for event in [AiEvent::Delta("late".into()), AiEvent::Error("cancelled".into())] {
            app.tx.send(Msg::Ai { tab: id, epoch: running, event: AiMsg::Event(event) }).unwrap();
        }
        app.pump_messages();
        let TabKind::Ai(t) = &app.tabs[0].kind else { panic!("AI tab") };
        assert_eq!((t.text.as_str(), t.error.as_deref()), ("", None));
    }
}
