//! Left sidebar: collections tree (FR-C1, FR-C5), environments and history
//! (FR-R9).

use std::collections::HashSet;

use egui::{Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2};

use ternarywire_core::model::{Collection, Folder, HttpRequest, Id, Item, RequestItem, RequestKind, WsRequest};
use ternarywire_core::storage::Workspace;

use crate::app::{ItemRef, SidebarTab, TabKind, TernaryWireApp};
use crate::dialogs::Dialog;
use crate::i18n::{tr, trf};

use crate::widgets;
use crate::{icons, kit, theme};

#[derive(Default)]
pub struct SidebarState {
    pub filter: String,
    pub expanded: HashSet<Id>,
    pub renaming: Option<(Id, Id, String)>,
    pub history_filter: String,
    pub focus_search: bool,
    /// Flows in `flows/` (id, name, path); reloaded when `flows_dirty`.
    pub flows: Option<Vec<(Id, String, std::path::PathBuf)>>,
    pub flows_dirty: bool,
    /// Scrolls the tree to this offset in the next frame (automation).
    pub scroll_to: Option<f32>,
    /// The row whose hover actions stay up while their menu is open.
    pub actions_open: Option<Id>,
    /// History entry each tab was opened from (tab id → entry id), to mark it in History.
    pub history_tabs: std::collections::HashMap<u64, Id>,
}

impl SidebarState {
    pub fn expand_to(&mut self, collection: &str, folder: Option<&str>) {
        self.expanded.insert(collection.to_string());
        if let Some(f) = folder {
            self.expanded.insert(f.to_string());
        }
    }
}

#[derive(Clone, Debug)]
struct DragItem {
    collection: Id,
    item: Id,
}

pub enum ExportFormat {
    Postman,
    OpenApi,
}

enum Act {
    Open(ItemRef),
    OpenContainer(Id, Option<Id>),
    NewRequest(Id, Option<Id>),
    NewWs(Id, Option<Id>),
    NewProto(Id, Option<Id>, String, Box<RequestKind>),
    NewFolder(Id, Option<Id>),
    CommitRename(Id, Id, String),
    Duplicate(Id, Id),
    DuplicateCollection(Id),
    Delete(Id, Option<Id>),
    Run(Id, Option<Id>),
    Mock(Id),
    Export(Id, ExportFormat),
    Move {
        from: Id,
        item: Id,
        to: Id,
        parent: Option<Id>,
        index: usize,
    },
    CopyCurl(Id, Id),
    /// None = preview, Some(markdown) = export.
    Docs(Id, Option<bool>),
    Fork(Id),
    /// Pull parent changes (true) or merge into the parent (false).
    ForkSync(Id, bool),
}

fn matches_filter(item: &Item, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    match item {
        Item::Folder(f) => f.name.to_lowercase().contains(q) || f.items.iter().any(|i| matches_filter(i, q)),
        Item::Request(r) => {
            r.name.to_lowercase().contains(q)
                || r.request.url().to_lowercase().contains(q)
                || r.http()
                    .map(|h| h.body.raw.to_lowercase().contains(q) || h.body.graphql.query.to_lowercase().contains(q))
                    .unwrap_or(false)
                || r.scripts.pre_request.to_lowercase().contains(q)
                || r.scripts.post_response.to_lowercase().contains(q)
                || r.description.to_lowercase().contains(q)
        }
    }
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui) {
    let p = crate::theme::pal(ui);
    let title = match app.sidebar_tab {
        SidebarTab::Collections => tr("Collections"),
        SidebarTab::Environments => tr("Environments"),
        SidebarTab::History => tr("History"),
        SidebarTab::Apis => tr("API specs"),
        SidebarTab::Flows => tr("Flows"),
        SidebarTab::Servers => tr("Local servers"),
    };
    ui.spacing_mut().item_spacing.y = 0.0;
    // A 34 px header: the section's title, then its actions (small icon buttons).
    kit::bar_ex(ui, 34.0, 0.0, false, |ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(title).font(crate::theme::semibold(13.0)).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(2.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            match app.sidebar_tab {
                SidebarTab::Collections => {
                    let more = kit::icon_button_sm(ui, icons::DOTS_THREE, tr("More actions"));
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_min_width(200.0);
                        if kit::menu_item(ui, Some(icons::ARROWS_OUT_LINE_VERTICAL), tr("Expand all"), None).clicked() {
                            expand_all(&app.ws.data, &mut app.sidebar);
                        }
                        if kit::menu_item(ui, Some(icons::ARROWS_IN_LINE_VERTICAL), tr("Collapse all"), None).clicked()
                        {
                            app.sidebar.expanded.clear();
                        }
                    });
                    if kit::icon_button_sm(ui, icons::DOWNLOAD_SIMPLE, tr("Import")).clicked() {
                        app.dialog = Some(Dialog::import());
                    }
                    if kit::icon_button_sm(ui, icons::PLUS, tr("New Collection")).clicked() {
                        app.dialog = Some(Dialog::prompt_new_collection());
                    }
                }
                SidebarTab::Environments => {
                    if kit::icon_button_sm(ui, icons::PLUS, tr("New Environment")).clicked() {
                        app.dialog = Some(Dialog::prompt_new_environment());
                    }
                }
                SidebarTab::History => {
                    if !app.ws.history.is_empty()
                        && kit::icon_button_sm(ui, icons::BROOM, tr("Clear history")).clicked()
                    {
                        app.ws.clear_history();
                    }
                }
                SidebarTab::Apis => crate::spec_tab::sidebar_actions(app, ui),
                SidebarTab::Flows => crate::flow_tab::sidebar_actions(app, ui),
                SidebarTab::Servers => {
                    let add = kit::icon_button_sm(ui, icons::PLUS, tr("New server"));
                    egui::Popup::menu(&add).show(|ui| new_server_menu(app, ui));
                }
            }
        });
    });
    match app.sidebar_tab {
        SidebarTab::Collections => collections(app, ui),
        SidebarTab::Environments => crate::env_tab::sidebar_list(app, ui),
        SidebarTab::History => history(app, ui),
        SidebarTab::Apis => crate::spec_tab::sidebar_list(app, ui),
        SidebarTab::Flows => crate::flow_tab::sidebar_list(app, ui),
        SidebarTab::Servers => servers(app, ui),
    }
}

/// The sidebar's search field: small, full width, 4 px above and 8 below.
fn sidebar_search(ui: &mut Ui, id: &str, text: &mut String, hint: &str) -> egui::Response {
    ui.add_space(4.0);
    let r = kit::search_field_sm(ui, id, text, hint, ui.available_width());
    ui.add_space(8.0);
    r
}

/// Height [`sidebar_footer`] takes for `text` in a sidebar `width` wide.
pub fn sidebar_footer_height(ui: &Ui, text: &str, width: f32) -> f32 {
    let g = ui.painter().layout(text.to_string(), FontId::proportional(11.5), Color32::PLACEHOLDER, width - 12.0);
    9.0 + g.size().y
}

/// The hint at the bottom of a sidebar (the prototype's `.sb-foot`): a
/// hairline, then faint 11.5 text, pinned to the bottom of the space left.
pub fn sidebar_footer(ui: &mut Ui, text: &str) {
    let p = theme::pal(ui);
    let rect = ui.available_rect_before_wrap();
    let h = sidebar_footer_height(ui, text, rect.width());
    let top = (rect.bottom() - h).max(rect.top());
    let g = ui.painter().layout(text.to_string(), FontId::proportional(11.5), p.text_faint, rect.width() - 12.0);
    ui.painter().hline(rect.x_range(), top + 0.5, Stroke::new(1.0, p.border));
    ui.painter().galley(pos2(rect.left() + 6.0, top + 9.0), g, p.text_faint);
    ui.allocate_rect(Rect::from_min_max(pos2(rect.left(), rect.top()), rect.max), Sense::hover());
}

fn expand_all(data: &Workspace, state: &mut SidebarState) {
    fn walk(items: &[Item], state: &mut SidebarState) {
        for i in items {
            if let Item::Folder(f) = i {
                state.expanded.insert(f.id.clone());
                walk(&f.items, state);
            }
        }
    }
    for c in &data.collections {
        state.expanded.insert(c.id.clone());
        walk(&c.items, state);
    }
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

/// The "+" menu of Local servers.
fn new_server_menu(app: &mut TernaryWireApp, ui: &mut Ui) {
    ui.set_min_width(230.0);
    let mock = kit::menu_item_ex(
        ui,
        kit::MenuItem { icon: Some(icons::CUBE), label: tr("Mock server…"), submenu: true, ..Default::default() },
    );
    let collections: Vec<(Id, String)> =
        app.ws.data.collections.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
    egui::Popup::menu(&mock).show(|ui| {
        ui.set_min_width(200.0);
        if collections.is_empty() {
            ui.label(kit::hint(ui, tr("No collections yet")));
        }
        for (id, name) in &collections {
            if kit::menu_item(ui, Some(icons::STACK), name, None).clicked() {
                app.open_mock(id.clone());
            }
        }
    });
    if kit::menu_item(ui, Some(icons::WEBHOOKS_LOGO), tr("Webhook listener"), None).clicked() {
        app.create_new(crate::app::NewItem::Webhook);
    }
    if kit::menu_item(ui, Some(icons::BROADCAST), tr("Capture traffic (proxy)"), None).clicked() {
        app.create_new(crate::app::NewItem::Capture);
    }
}

/// Local servers: the open mock, webhook and capture tabs, grouped, with
/// their state; a click shows the tab.
fn servers(app: &mut TernaryWireApp, ui: &mut Ui) {
    let p = crate::theme::pal(ui);
    ui.add_space(4.0);
    #[derive(Clone)]
    struct Srv {
        idx: usize,
        icon: &'static str,
        name: String,
        state: Option<String>,
        tone: kit::Tone,
        tip: String,
    }
    let mut groups: [(&str, Vec<Srv>); 3] =
        [(tr("Mock servers"), vec![]), (tr("Webhook listeners"), vec![]), (tr("Capture proxy"), vec![])];
    for (idx, t) in app.tabs.iter().enumerate() {
        let port = |a: Option<std::net::SocketAddr>| a.map(|a| format!(":{}", a.port()));
        match &t.kind {
            TabKind::Mock(m) => {
                let name = app.ws.data.collection(&m.collection).map(|c| c.name.clone()).unwrap_or_default();
                let addr = m.address();
                groups[0].1.push(Srv {
                    idx,
                    icon: icons::CUBE,
                    name,
                    state: port(addr),
                    tone: kit::Tone::Ok,
                    tip: addr.map(|a| format!("http://{a}")).unwrap_or_default(),
                });
            }
            TabKind::Webhook(w) => {
                let addr = w.address();
                let n = w.request_count();
                groups[1].1.push(Srv {
                    idx,
                    icon: icons::WEBHOOKS_LOGO,
                    name: tr("Webhook listener").to_string(),
                    state: port(addr).map(|s| if n > 0 { format!("{s} · {n}") } else { s }),
                    tone: kit::Tone::Ok,
                    tip: addr.map(|a| format!("http://{a}")).unwrap_or_default(),
                });
            }
            TabKind::Capture(c) => {
                let addr = c.address();
                groups[2].1.push(Srv {
                    idx,
                    icon: icons::BROADCAST,
                    name: tr("Capture traffic").to_string(),
                    state: port(addr),
                    tone: if c.is_paused() { kit::Tone::Warn } else { kit::Tone::Ok },
                    tip: addr.map(|a| a.to_string()).unwrap_or_default(),
                });
            }
            _ => {}
        }
    }
    if groups.iter().all(|g| g.1.is_empty()) {
        kit::empty_state(
            ui,
            icons::BROADCAST,
            tr("No local servers"),
            tr("Start a mock server, a webhook listener or a capture proxy; they show here while their tab is open."),
            |ui| {
                ui.vertical_centered(|ui| {
                    if kit::secondary(ui, Some(icons::WEBHOOKS_LOGO), tr("Webhook listener")).clicked() {
                        app.create_new(crate::app::NewItem::Webhook);
                    }
                    ui.add_space(6.0);
                    if kit::secondary(ui, Some(icons::BROADCAST), tr("Capture traffic")).clicked() {
                        app.create_new(crate::app::NewItem::Capture);
                    }
                });
            },
        );
        return;
    }
    let mut activate = None;
    let foot = tr("Servers run on this machine only. Stopped servers keep their settings.");
    let list_h = (ui.available_height() - sidebar_footer_height(ui, foot, ui.available_width())).max(40.0);
    egui::ScrollArea::vertical().id_salt("servers").max_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
        for (title, rows) in &groups {
            if rows.is_empty() {
                continue;
            }
            kit::sidebar_group(ui, title);
            for s in rows {
                let stopped = tr("stopped");
                let (r, _) = kit::tree_row(
                    ui,
                    kit::TreeRow {
                        pad_left: 4.0,
                        icon: Some(s.icon),
                        label: &s.name,
                        selected: s.idx == app.active,
                        meta: s.state.is_none().then_some(stopped),
                        ..Default::default()
                    },
                    |ui| {
                        if let Some(st) = &s.state {
                            kit::pill_ex(ui, st, s.tone, kit::PillOpts { dot: true, live: true, ..Default::default() });
                        }
                    },
                );
                let r = if s.tip.is_empty() { r } else { r.on_hover_text(&s.tip) };
                widgets::a11y_button(&r, &s.name);
                if r.clicked() {
                    activate = Some(s.idx);
                }
            }
        }
    });
    if let Some(i) = activate {
        app.active = i;
    }
    let _ = p;
    sidebar_footer(ui, foot);
}

fn active_ref(app: &TernaryWireApp) -> Option<ItemRef> {
    app.tabs.get(app.active).and_then(|t| t.kind.location().cloned())
}

fn collections(app: &mut TernaryWireApp, ui: &mut Ui) {
    let mut acts: Vec<Act> = Vec::new();
    let r = sidebar_search(ui, "tree-search", &mut app.sidebar.filter, tr("Filter requests"));
    widgets::a11y_name(&r, tr("Search requests"));
    if app.sidebar.focus_search {
        r.request_focus();
        app.sidebar.focus_search = false;
    }
    let active = active_ref(app);
    let filter = app.sidebar.filter.to_lowercase();
    if app.ws.data.collections.is_empty() {
        kit::empty_state(
            ui,
            icons::STACK,
            tr("No collections yet"),
            tr("Group requests into collections to save and share them."),
            |ui| {
                if kit::primary_icon(ui, icons::PLUS, tr("New Collection")).clicked() {
                    app.dialog = Some(Dialog::prompt_new_collection());
                }
                ui.add_space(4.0);
                if kit::secondary(ui, Some(icons::DOWNLOAD_SIMPLE), tr("Import")).clicked() {
                    app.dialog = Some(Dialog::import());
                }
            },
        );
    }
    let data: &Workspace = &app.ws.data;
    let fork_parents: std::collections::HashMap<&str, String> = app
        .ws
        .forks
        .iter()
        .map(|f| (f.fork.as_str(), data.collection(&f.parent).map(|c| c.name.clone()).unwrap_or_default()))
        .collect();
    let state = &mut app.sidebar;
    // Only the visible rows are laid out, so a 10,000-request tree costs the
    // same per frame as a small one.
    let rows = flatten(data, state, &filter);
    const ROW: f32 = 28.0;
    let mut area = egui::ScrollArea::vertical().id_salt("tree").auto_shrink([false; 2]);
    if let Some(y) = state.scroll_to.take() {
        area = area.vertical_scroll_offset(y);
    }
    kit::show_rows(area, ui, ROW, rows.len(), |ui, range| {
        for row in &rows[range] {
            match row.kind {
                RowKind::Collection => {
                    let parent = fork_parents.get(row.collection.id.as_str()).map(String::as_str);
                    collection_row(ui, state, row, parent, &mut acts);
                }
                RowKind::Folder(f) => folder_row(ui, state, row, f, &mut acts),
                RowKind::Request(r) => request_row(ui, state, row, r, active.as_ref(), &mut acts),
                RowKind::Empty => {
                    ui.horizontal(|ui| {
                        ui.set_height(ROW);
                        ui.add_space(4.0 + row.depth as f32 * 14.0 + 22.0);
                        ui.label(kit::hint(ui, tr("Empty collection")));
                        if kit::ghost(ui, Some(icons::PLUS), tr("Add request")).clicked() {
                            acts.push(Act::NewRequest(row.collection.id.clone(), None));
                        }
                    });
                }
            }
        }
    });
    // Hover actions stay up while their menu is open.
    if !egui::Popup::is_any_open(ui.ctx()) && !ui.input(|i| i.pointer.any_click()) {
        state.actions_open = None;
    }
    apply(app, acts);
}

fn rename_field(ui: &mut Ui, state: &mut SidebarState, acts: &mut Vec<Act>) -> bool {
    let Some((cid, target, buf)) = &mut state.renaming else { return false };
    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width() - 8.0));
    r.request_focus();
    let done = r.lost_focus();
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if done || escape {
        if !escape && !buf.trim().is_empty() {
            acts.push(Act::CommitRename(cid.clone(), target.clone(), buf.trim().to_string()));
        }
        state.renaming = None;
    }
    true
}

#[derive(Clone, Copy)]
enum RowKind<'a> {
    Collection,
    Folder(&'a Folder),
    Request(&'a RequestItem),
    Empty,
}

struct Row<'a> {
    depth: usize,
    collection: &'a Collection,
    kind: RowKind<'a>,
    parent: Option<&'a str>,
    index: usize,
    open: bool,
}

/// Visible rows of the tree in display order.
fn flatten<'a>(data: &'a Workspace, state: &SidebarState, filter: &str) -> Vec<Row<'a>> {
    fn walk<'a>(
        c: &'a Collection,
        items: &'a [Item],
        parent: Option<&'a str>,
        depth: usize,
        state: &SidebarState,
        filter: &str,
        out: &mut Vec<Row<'a>>,
    ) {
        for (index, item) in items.iter().enumerate() {
            if !matches_filter(item, filter) {
                continue;
            }
            match item {
                Item::Folder(f) => {
                    let open = state.expanded.contains(&f.id) || !filter.is_empty();
                    out.push(Row { depth, collection: c, kind: RowKind::Folder(f), parent, index, open });
                    if open {
                        walk(c, &f.items, Some(&f.id), depth + 1, state, filter, out);
                    }
                }
                Item::Request(r) => {
                    out.push(Row { depth, collection: c, kind: RowKind::Request(r), parent, index, open: false });
                }
            }
        }
    }
    let mut out = Vec::new();
    for c in &data.collections {
        if !filter.is_empty()
            && !c.name.to_lowercase().contains(filter)
            && !c.items.iter().any(|i| matches_filter(i, filter))
        {
            continue;
        }
        let open = state.expanded.contains(&c.id) || !filter.is_empty();
        out.push(Row { depth: 0, collection: c, kind: RowKind::Collection, parent: None, index: 0, open });
        if open {
            if c.items.is_empty() {
                out.push(Row { depth: 1, collection: c, kind: RowKind::Empty, parent: None, index: 0, open: false });
            }
            walk(c, &c.items, None, 1, state, filter, &mut out);
        }
    }
    out
}

/// The inline rename field in place of a row.
fn rename_row(ui: &mut Ui, depth: usize, state: &mut SidebarState, acts: &mut Vec<Act>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let field = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 4.0 + depth as f32 * 14.0 + 22.0, rect.top() + 2.0),
        rect.max - egui::vec2(4.0, 2.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    rename_field(&mut child, state, acts);
}

fn is_renaming(state: &SidebarState, id: &str) -> bool {
    matches!(&state.renaming, Some((_, t, _)) if t == id)
}

/// Shows a row's hover actions in `rect` (right to left), keeping them up
/// while their menu is open.
fn row_actions(ui: &mut Ui, key: &str, rect: Option<egui::Rect>, add: impl FnOnce(&mut Ui) -> bool) -> bool {
    let Some(rect) = rect else { return false };
    // A child that doesn't move the list's cursor (the row is already allocated).
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .id_salt(("tree-actions", key))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 2.0;
    add(&mut child)
}

fn collection_row(
    ui: &mut Ui,
    state: &mut SidebarState,
    row: &Row<'_>,
    fork_parent: Option<&str>,
    acts: &mut Vec<Act>,
) {
    let c = row.collection;
    if is_renaming(state, &c.id) {
        rename_row(ui, row.depth, state, acts);
        return;
    }
    let count = count_requests(&c.items);
    let meta = if count > 0 { count.to_string() } else { String::new() };
    let info = crate::theme::pal(ui).info;
    let (r, actions) = kit::tree_row(
        ui,
        kit::TreeRow {
            depth: row.depth,
            caret: Some(row.open),
            icon: Some(icons::STACK),
            label: &c.name,
            strong: true,
            meta: (count > 0).then_some(meta.as_str()),
            actions_width: 46.0,
            force_actions: state.actions_open.as_deref() == Some(c.id.as_str()),
            ..Default::default()
        },
        |ui| {
            if let Some(p) = fork_parent {
                kit::pill(ui, tr("fork"), info).on_hover_text(format!("{} {p}", tr("Forked from")));
            }
        },
    );
    widgets::a11y_button(&r, &format!("{} {}", tr("Collection"), c.name));
    if r.clicked() {
        toggle(state, &c.id);
    }
    if r.double_clicked() {
        acts.push(Act::OpenContainer(c.id.clone(), None));
    }
    // Dropping onto the collection header moves the item to the root.
    if let Some(p) = r.dnd_release_payload::<DragItem>() {
        acts.push(Act::Move {
            from: p.collection.clone(),
            item: p.item.clone(),
            to: c.id.clone(),
            parent: None,
            index: usize::MAX,
        });
    }
    r.context_menu(|ui| collection_menu(ui, state, c, fork_parent, acts));
    let key = c.id.clone();
    if row_actions(ui, &key, actions, |ui| {
        let more = kit::icon_button_sm(ui, icons::DOTS_THREE, tr("More actions"));
        egui::Popup::menu(&more).show(|ui| collection_menu(ui, state, c, fork_parent, acts));
        let add = kit::icon_button_sm(ui, icons::PLUS, tr("Add request"));
        egui::Popup::menu(&add).show(|ui| add_menu(ui, c, None, acts));
        more.clicked() || add.clicked()
    }) {
        state.actions_open = Some(key);
    }
}

fn collection_menu(
    ui: &mut Ui,
    state: &mut SidebarState,
    c: &Collection,
    fork_parent: Option<&str>,
    acts: &mut Vec<Act>,
) {
    ui.set_min_width(230.0);
    container_menu(ui, state, c, None, &c.name, acts);
    kit::menu_sep(ui);
    if kit::menu_item(ui, Some(icons::EXPORT), &trf("{} (Postman v2.1)", &[&tr("Export")]), None).clicked() {
        acts.push(Act::Export(c.id.clone(), ExportFormat::Postman));
    }
    if kit::menu_item(ui, Some(icons::EXPORT), &trf("{} (OpenAPI 3)", &[&tr("Export")]), None).clicked() {
        acts.push(Act::Export(c.id.clone(), ExportFormat::OpenApi));
    }
    if kit::menu_item(ui, Some(icons::BOOK_OPEN), tr("Preview documentation"), None).clicked() {
        acts.push(Act::Docs(c.id.clone(), None));
    }
    if kit::menu_item(ui, Some(icons::FILE_TEXT), tr("Export documentation (HTML)"), None).clicked() {
        acts.push(Act::Docs(c.id.clone(), Some(false)));
    }
    if kit::menu_item(ui, Some(icons::FILE_TEXT), tr("Export documentation (Markdown)"), None).clicked() {
        acts.push(Act::Docs(c.id.clone(), Some(true)));
    }
    if kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked() {
        acts.push(Act::DuplicateCollection(c.id.clone()));
    }
    if kit::menu_item(ui, Some(icons::GIT_FORK), &format!("{}…", tr("Fork")), None).clicked() {
        acts.push(Act::Fork(c.id.clone()));
    }
    if let Some(p) = fork_parent {
        if kit::menu_item(ui, Some(icons::ARROW_DOWN), &format!("{} “{p}”…", tr("Pull changes from")), None).clicked()
        {
            acts.push(Act::ForkSync(c.id.clone(), true));
        }
        if kit::menu_item(ui, Some(icons::GIT_MERGE), &format!("{} “{p}”…", tr("Merge into")), None).clicked() {
            acts.push(Act::ForkSync(c.id.clone(), false));
        }
    }
    if kit::menu_item(ui, Some(icons::CUBE), tr("Mock server"), None).clicked() {
        acts.push(Act::Mock(c.id.clone()));
    }
}

fn folder_row(ui: &mut Ui, state: &mut SidebarState, row: &Row<'_>, f: &Folder, acts: &mut Vec<Act>) {
    let c = row.collection;
    if is_renaming(state, &f.id) {
        rename_row(ui, row.depth, state, acts);
        return;
    }
    let (r, actions) = kit::tree_row(
        ui,
        kit::TreeRow {
            depth: row.depth,
            caret: Some(row.open),
            icon: Some(if row.open { icons::FOLDER_OPEN } else { icons::FOLDER_SIMPLE }),
            label: &f.name,
            actions_width: 46.0,
            force_actions: state.actions_open.as_deref() == Some(f.id.as_str()),
            ..Default::default()
        },
        |_| {},
    );
    widgets::a11y_button(&r, &format!("{} {}", tr("Folder"), f.name));
    if r.drag_started() {
        r.dnd_set_drag_payload(DragItem { collection: c.id.clone(), item: f.id.clone() });
    }
    if r.clicked() {
        toggle(state, &f.id);
    }
    if r.double_clicked() {
        acts.push(Act::OpenContainer(c.id.clone(), Some(f.id.clone())));
    }
    drop_target(ui, &r, c, Some(&f.id), usize::MAX, acts, true);
    r.context_menu(|ui| {
        ui.set_min_width(230.0);
        container_menu(ui, state, c, Some(f), &f.name, acts)
    });
    let key = f.id.clone();
    if row_actions(ui, &key, actions, |ui| {
        let more = kit::icon_button_sm(ui, icons::DOTS_THREE, tr("More actions"));
        egui::Popup::menu(&more).show(|ui| {
            ui.set_min_width(230.0);
            container_menu(ui, state, c, Some(f), &f.name, acts)
        });
        let add = kit::icon_button_sm(ui, icons::PLUS, tr("Add request"));
        egui::Popup::menu(&add).show(|ui| add_menu(ui, c, Some(f), acts));
        more.clicked() || add.clicked()
    }) {
        state.actions_open = Some(key);
    }
}

#[allow(clippy::too_many_arguments)]
fn request_row(
    ui: &mut Ui,
    state: &mut SidebarState,
    row: &Row<'_>,
    r: &RequestItem,
    active: Option<&ItemRef>,
    acts: &mut Vec<Act>,
) {
    let c = row.collection;
    if is_renaming(state, &r.id) {
        rename_row(ui, row.depth, state, acts);
        return;
    }
    let is_active = active.map(|a| a.item == r.id && a.collection == c.id).unwrap_or(false);
    let badge = r.request.badge();
    let (resp, actions) = kit::tree_row(
        ui,
        kit::TreeRow {
            depth: row.depth,
            indent_caret: true,
            method: Some(&badge),
            label: &r.name,
            selected: is_active,
            actions_width: 22.0,
            force_actions: state.actions_open.as_deref() == Some(r.id.as_str()),
            ..Default::default()
        },
        |_| {},
    );
    widgets::a11y_button(&resp, &format!("{badge} {}", r.name));
    if resp.drag_started() {
        resp.dnd_set_drag_payload(DragItem { collection: c.id.clone(), item: r.id.clone() });
    }
    if resp.clicked() {
        acts.push(Act::Open(ItemRef { collection: c.id.clone(), item: r.id.clone() }));
    }
    let resp = resp.on_hover_text(r.request.url());
    drop_target(ui, &resp, c, row.parent, row.index, acts, false);
    resp.context_menu(|ui| request_menu(ui, state, c, r, acts));
    let key = r.id.clone();
    if row_actions(ui, &key, actions, |ui| {
        let more = kit::icon_button_sm(ui, icons::DOTS_THREE, tr("More actions"));
        egui::Popup::menu(&more).show(|ui| request_menu(ui, state, c, r, acts));
        more.clicked()
    }) {
        state.actions_open = Some(key);
    }
}

fn request_menu(ui: &mut Ui, state: &mut SidebarState, c: &Collection, r: &RequestItem, acts: &mut Vec<Act>) {
    ui.set_min_width(210.0);
    if kit::menu_item(ui, Some(icons::ARROW_SQUARE_OUT), tr("Open"), None).clicked() {
        acts.push(Act::Open(ItemRef { collection: c.id.clone(), item: r.id.clone() }));
    }
    if kit::menu_item(ui, Some(icons::PENCIL_SIMPLE), tr("Rename"), None).clicked() {
        state.renaming = Some((c.id.clone(), r.id.clone(), r.name.clone()));
    }
    if kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked() {
        acts.push(Act::Duplicate(c.id.clone(), r.id.clone()));
    }
    if r.http().is_some() && kit::menu_item(ui, Some(icons::TERMINAL), tr("Copy as cURL"), None).clicked() {
        acts.push(Act::CopyCurl(c.id.clone(), r.id.clone()));
    }
    kit::menu_sep(ui);
    let del = kit::MenuItem { icon: Some(icons::TRASH), label: tr("Delete"), danger: true, ..Default::default() };
    if kit::menu_item_ex(ui, del).clicked() {
        acts.push(Act::Delete(c.id.clone(), Some(r.id.clone())));
    }
}

/// The "+" menu of a collection or folder row: the request types and a folder.
fn add_menu(ui: &mut Ui, c: &Collection, folder: Option<&Folder>, acts: &mut Vec<Act>) {
    ui.set_min_width(220.0);
    let fid = folder.map(|f| f.id.clone());
    if kit::menu_item(ui, Some(icons::GLOBE), tr("Add request"), None).clicked() {
        acts.push(Act::NewRequest(c.id.clone(), fid.clone()));
    }
    if kit::menu_item(ui, Some(icons::PLUGS_CONNECTED), tr("Add WebSocket request"), None).clicked() {
        acts.push(Act::NewWs(c.id.clone(), fid.clone()));
    }
    for (label, name, kind) in crate::app::protocol_templates() {
        if kit::menu_item(ui, Some(crate::app::protocol_icon(name)), &label.replace("New", "Add"), None).clicked() {
            acts.push(Act::NewProto(c.id.clone(), fid.clone(), name.to_string(), Box::new(kind)));
        }
    }
    kit::menu_sep(ui);
    if kit::menu_item(ui, Some(icons::FOLDER_PLUS), tr("Add folder"), None).clicked() {
        acts.push(Act::NewFolder(c.id.clone(), fid));
    }
}

fn container_menu(
    ui: &mut Ui,
    state: &mut SidebarState,
    c: &Collection,
    folder: Option<&Folder>,
    name: &str,
    acts: &mut Vec<Act>,
) {
    let fid = folder.map(|f| f.id.clone());
    add_menu(ui, c, folder, acts);
    kit::menu_sep(ui);
    let run = if folder.is_some() { tr("Run folder") } else { tr("Run collection") };
    if kit::menu_item(ui, Some(icons::PLAY), run, None).clicked() {
        acts.push(Act::Run(c.id.clone(), fid.clone()));
    }
    if kit::menu_item(ui, Some(icons::SLIDERS_HORIZONTAL), tr("Settings"), None).clicked() {
        acts.push(Act::OpenContainer(c.id.clone(), fid.clone()));
    }
    if kit::menu_item(ui, Some(icons::PENCIL_SIMPLE), tr("Rename"), None).clicked() {
        state.renaming = Some((c.id.clone(), fid.clone().unwrap_or_else(|| c.id.clone()), name.to_string()));
    }
    if let Some(f) = folder
        && kit::menu_item(ui, Some(icons::COPY), tr("Duplicate"), None).clicked()
    {
        acts.push(Act::Duplicate(c.id.clone(), f.id.clone()));
    }
    let del = kit::MenuItem { icon: Some(icons::TRASH), label: tr("Delete"), danger: true, ..Default::default() };
    if kit::menu_item_ex(ui, del).clicked() {
        acts.push(Act::Delete(c.id.clone(), fid));
    }
}

fn toggle(state: &mut SidebarState, id: &str) {
    if !state.expanded.remove(id) {
        state.expanded.insert(id.to_string());
    }
}

/// Handles drops on a row: onto a folder moves into it, onto a request moves
/// before it.
fn drop_target(
    ui: &Ui,
    resp: &egui::Response,
    c: &Collection,
    parent: Option<&str>,
    index: usize,
    acts: &mut Vec<Act>,
    into_folder: bool,
) {
    if let Some(hover) = resp.dnd_hover_payload::<DragItem>()
        && hover.item != parent.unwrap_or("")
    {
        let stroke = egui::Stroke::new(2.0, crate::theme::accent(ui.visuals().dark_mode));
        if into_folder {
            ui.painter().rect_stroke(resp.rect, 3.0, stroke, egui::StrokeKind::Inside);
        } else {
            ui.painter().hline(resp.rect.x_range(), resp.rect.top(), stroke);
        }
    }
    if let Some(p) = resp.dnd_release_payload::<DragItem>() {
        acts.push(Act::Move {
            from: p.collection.clone(),
            item: p.item.clone(),
            to: c.id.clone(),
            parent: parent.map(str::to_string),
            index,
        });
    }
}

fn apply(app: &mut TernaryWireApp, acts: Vec<Act>) {
    for act in acts {
        match act {
            Act::Open(r) => app.open_item(r),
            Act::OpenContainer(c, f) => app.open_container(c, f),
            Act::NewRequest(cid, parent) => {
                let item = RequestItem::new("New Request", HttpRequest::default());
                add_item(app, &cid, parent.as_deref(), Item::Request(item), true);
            }
            Act::NewWs(cid, parent) => {
                let mut item = RequestItem::new("New WebSocket", HttpRequest::default());
                item.request = RequestKind::WebSocket(WsRequest::default());
                add_item(app, &cid, parent.as_deref(), Item::Request(item), true);
            }
            Act::NewProto(cid, parent, name, kind) => {
                let mut item = RequestItem::new(name, HttpRequest::default());
                item.request = *kind;
                add_item(app, &cid, parent.as_deref(), Item::Request(item), true);
            }
            Act::NewFolder(cid, parent) => {
                let f = Folder::new("New Folder");
                let fid = f.id.clone();
                add_item(app, &cid, parent.as_deref(), Item::Folder(f), false);
                app.sidebar.renaming = Some((cid, fid, "New Folder".into()));
            }
            Act::CommitRename(cid, target, name) => {
                if let Some(c) = app.ws.data.collection_mut(&cid) {
                    if target == cid {
                        c.name = name;
                    } else if let Some(item) = c.find_mut(&target) {
                        item.set_name(name);
                    }
                    app.ws.mark_collection(&cid);
                }
            }
            Act::Duplicate(cid, item_id) => {
                if let Some(c) = app.ws.data.collection_mut(&cid)
                    && let Some(item) = c.find(&item_id).cloned()
                {
                    let parent = c.parent_of(&item_id).map(str::to_string);
                    let mut copy = item.duplicate();
                    copy.set_name(trf("{} Copy", &[&item.name()]));
                    let idx = c
                        .children_mut(parent.as_deref())
                        .and_then(|v| v.iter().position(|i| i.id() == item_id))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    c.insert(parent.as_deref(), idx, copy);
                    app.ws.mark_collection(&cid);
                }
            }
            Act::DuplicateCollection(cid) => {
                if let Some(c) = app.ws.data.collection(&cid).cloned() {
                    let mut copy = c.clone();
                    copy.id = ternarywire_core::util::new_id();
                    copy.name = trf("{} Copy", &[&c.name]);
                    copy.items = copy.items.iter().map(|i| i.duplicate()).collect();
                    app.ws.add_collection(copy);
                }
            }
            Act::Delete(cid, item) => {
                let name = match &item {
                    Some(i) => app.ws.data.collection(&cid).and_then(|c| c.find(i)).map(|i| i.name().to_string()),
                    None => app.ws.data.collection(&cid).map(|c| c.name.clone()),
                };
                if let Some(name) = name {
                    app.dialog = Some(Dialog::confirm_delete(cid, item, name));
                }
            }
            Act::Run(cid, folder) => app.open_runner(cid, folder),
            Act::Mock(cid) => app.open_mock(cid),
            Act::Fork(cid) => {
                let name = app.ws.data.collection(&cid).map(|c| format!("{} (fork)", c.name)).unwrap_or_default();
                app.dialog = Some(Dialog::Prompt { kind: crate::dialogs::Prompt::Fork(cid), text: name });
            }
            Act::ForkSync(cid, pull) => {
                if let Some(s) = crate::fork_ui::ForkSync::new(app, &cid, pull) {
                    app.dialog = Some(Dialog::ForkSync(Box::new(s)));
                }
            }
            Act::Export(cid, format) => crate::dialogs::export_collection(app, &cid, format),
            Act::Move { from, item, to, parent, index } => move_item(app, &from, &item, &to, parent.as_deref(), index),
            Act::Docs(cid, mode) => match mode {
                None => {
                    if let Some(c) = app.ws.data.collection(&cid).cloned() {
                        crate::dialogs::preview_docs(app, &c);
                    }
                }
                Some(md) => crate::dialogs::export_docs(app, &cid, md),
            },
            Act::CopyCurl(cid, rid) => {
                if let Some(code) =
                    crate::dialogs::codegen_for(app, &cid, &rid, ternarywire_convert::codegen::Language::Curl)
                {
                    app.ctx.copy_text(code);
                    app.toast(tr("Copied"), false);
                }
            }
        }
    }
}

fn add_item(app: &mut TernaryWireApp, cid: &str, parent: Option<&str>, item: Item, open: bool) {
    let item_id = item.id().to_string();
    let name = item.name().to_string();
    if let Some(c) = app.ws.data.collection_mut(cid) {
        let at = c.children_mut(parent).map(|v| v.len()).unwrap_or(0);
        c.insert(parent, at, item);
        app.ws.mark_collection(cid);
        app.sidebar.expand_to(cid, parent);
        if open {
            app.open_item(ItemRef { collection: cid.to_string(), item: item_id.clone() });
            app.sidebar.renaming = Some((cid.to_string(), item_id, name));
        }
    }
}

fn move_item(app: &mut TernaryWireApp, from: &str, item: &str, to: &str, parent: Option<&str>, index: usize) {
    if from == to {
        if let Some(c) = app.ws.data.collection_mut(from) {
            let idx = if index == usize::MAX { c.children_mut(parent).map(|v| v.len()).unwrap_or(0) } else { index };
            if c.move_item(item, parent, idx) {
                app.ws.mark_collection(from);
            }
        }
        return;
    }
    let Some(moved) = app.ws.data.collection_mut(from).and_then(|c| c.remove(item)) else { return };
    let ok = app
        .ws
        .data
        .collection_mut(to)
        .map(|c| {
            let idx = if index == usize::MAX { c.children_mut(parent).map(|v| v.len()).unwrap_or(0) } else { index };
            c.insert(parent, idx, moved.clone())
        })
        .unwrap_or(false);
    if ok {
        app.ws.mark_collection(from);
        app.ws.mark_collection(to);
        // Keep open tabs pointing at the right collection.
        for tab in &mut app.tabs {
            let loc = tab.kind.location_mut().and_then(|l| l.as_mut());
            if let Some(l) = loc
                && l.collection == from
                && (l.item == item || app.ws.data.collection(to).and_then(|c| c.find(&l.item)).is_some())
            {
                l.collection = to.to_string();
            }
        }
    } else if let Some(c) = app.ws.data.collection_mut(from) {
        c.items.push(moved);
    }
}

fn history(app: &mut TernaryWireApp, ui: &mut Ui) {
    let dark = ui.visuals().dark_mode;
    sidebar_search(ui, "history-search", &mut app.sidebar.history_filter, tr("Search history"));
    let q = app.sidebar.history_filter.clone();
    let mut open = None;
    let mut remove = None;
    let today = chrono::Local::now().date_naive();
    let selected = app.tabs.get(app.active).and_then(|t| app.sidebar.history_tabs.get(&t.id).cloned());
    egui::ScrollArea::vertical().id_salt("history").auto_shrink([false; 2]).show(ui, |ui| {
        let mut last_day = None;
        for h in app.ws.history.iter().filter(|h| h.matches(&q)) {
            let dt = chrono::DateTime::from_timestamp_millis(h.timestamp).map(|d| d.with_timezone(&chrono::Local));
            let day = dt.map(|d| d.date_naive());
            if day != last_day {
                let label = match day {
                    Some(d) if d == today => tr("Today").to_string(),
                    Some(d) if Some(d) == today.pred_opt() => tr("Yesterday").to_string(),
                    Some(d) => d.format("%-d %b %Y").to_string(),
                    None => String::new(),
                };
                kit::sidebar_group(ui, &label);
                last_day = day;
            }
            let url = if h.resolved_url.is_empty() { &h.request.url } else { &h.resolved_url };
            let shown = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
            let path = shown.find('/').map(|i| &shown[i..]).unwrap_or(shown);
            let (status, color) = match (h.status, &h.error) {
                (Some(code), _) => (code.to_string(), crate::theme::status_color(code, dark)),
                (None, _) => (tr("Error").to_string(), crate::theme::err(dark)),
            };
            let (r, actions) = kit::tree_row(
                ui,
                kit::TreeRow {
                    pad_left: 2.0,
                    method: Some(&h.request.method),
                    label: path,
                    mono: true,
                    meta: Some(&status),
                    meta_color: Some(color),
                    selected: selected.as_deref() == Some(h.id.as_str()),
                    actions_width: 22.0,
                    ..Default::default()
                },
                |_| {},
            );
            widgets::a11y_button(&r, &format!("{} {url}", h.request.method));
            let r = r.on_hover_ui(|ui| {
                ui.label(RichText::new(url).monospace());
                if let Some(n) = &h.name {
                    ui.label(n);
                }
                ui.label(format!(
                    "{} · {} · {}",
                    h.status.map(|s| s.to_string()).or(h.error.clone()).unwrap_or_default(),
                    ternarywire_core::util::human_ms(h.elapsed_ms),
                    dt.map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_default()
                ));
            });
            if r.clicked() {
                open = Some(h.id.clone());
            }
            if let Some(rect) = actions {
                let mut child = ui.new_child(
                    egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                if kit::icon_button_sm(&mut child, icons::ARROW_SQUARE_OUT, tr("Open in a new tab")).clicked() {
                    open = Some(h.id.clone());
                }
            }
            r.context_menu(|ui| {
                let del =
                    kit::MenuItem { icon: Some(icons::TRASH), label: tr("Delete"), danger: true, ..Default::default() };
                if kit::menu_item_ex(ui, del).clicked() {
                    remove = Some(h.id.clone());
                }
            });
        }
        if app.ws.history.is_empty() {
            kit::empty_state(
                ui,
                icons::CLOCK_COUNTER_CLOCKWISE,
                tr("No history yet"),
                tr("Requests you send appear here."),
                |_| {},
            );
        }
    });
    if let Some(id) = open {
        app.open_history(&id);
    }
    if let Some(id) = remove {
        app.ws.remove_history(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inline rename that follows "Add …" starts from the new item's own
    /// name, so leaving the field unchanged keeps it.
    #[test]
    fn new_items_keep_their_name_after_the_inline_rename() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(dir.path());
        let cid = app.new_collection("C");
        let (_, name, kind) = crate::app::protocol_templates().remove(0);
        apply(
            &mut app,
            vec![Act::NewWs(cid.clone(), None), Act::NewProto(cid.clone(), None, name.to_string(), Box::new(kind))],
        );
        let c = app.ws.data.collection(&cid).unwrap();
        let names: Vec<String> = c.items.iter().map(|i| i.name().to_string()).collect();
        assert_eq!(names, ["New WebSocket", name]);
        let (_, target, buf) = app.sidebar.renaming.clone().unwrap();
        assert_eq!(target, c.items[1].id());
        assert_eq!(buf, name);
    }
}
