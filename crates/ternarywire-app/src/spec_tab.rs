//! Local Spec Hub (FR-D1, FR-D2): API specifications kept as files in the
//! workspace's `specs/` folder, edited with highlighting and live validation,
//! and turned into collections (or used to update one).
//!
//! The tab shows the file with its format and validity, a toolbar (view,
//! Validate, Sync, Generate collection), the editor with its problems below
//! (click one to jump to its line) and a documentation preview drawn from
//! the collection the spec turns into.
//!
//! Smithy models may be split over several files: a Smithy file is
//! validated, previewed and imported together with the `.smithy` files and
//! Smithy JSON ASTs of its folder that it is connected to by shape references
//! (in either direction; see `ternarywire_convert::smithy::related_files`), so
//! unrelated models can still share `specs/`. Sibling files are parsed once
//! and re-read only when their modification time or size changes; a sibling
//! open in another spec tab with unsaved changes is read from that tab.
//! With a `smithy-build.json`, the models of its Maven dependencies (jars in
//! the local Maven repository) are assembled after the project's own files.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, pos2, vec2,
};

use ternarywire_convert::smithy::{self, SourceFile};
use ternarywire_core::model::{Collection, Id, Item, RequestItem};

use crate::app::{TabKind, TernaryWireApp};
use crate::highlight::Lang;
use crate::i18n::{tr, trf};
use crate::kit::Tone;
use crate::theme::{self, Palette};
use crate::widgets;
use crate::{icons, kit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecKind {
    OpenApi,
    AsyncApi,
    Protobuf,
    GraphQl,
    Smithy,
    Other,
}

impl SpecKind {
    pub fn detect(path: &Path, text: &str) -> SpecKind {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match ext.as_str() {
            "proto" => return SpecKind::Protobuf,
            "graphql" | "graphqls" | "gql" => return SpecKind::GraphQl,
            "smithy" => return SpecKind::Smithy,
            _ => {}
        }
        let head: String = text.chars().take(4000).collect();
        let json_ast = ext == "json"
            && serde_json::from_str::<serde_json::Value>(text)
                .is_ok_and(|v| ternarywire_convert::smithy::is_json_ast(&v));
        if json_ast || ternarywire_convert::smithy::looks_like_smithy(&head) {
            SpecKind::Smithy
        } else if head.contains("asyncapi") {
            SpecKind::AsyncApi
        } else if head.contains("openapi") || head.contains("swagger") {
            SpecKind::OpenApi
        } else {
            SpecKind::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SpecKind::OpenApi => "OpenAPI",
            SpecKind::AsyncApi => "AsyncAPI",
            SpecKind::Protobuf => "Protocol Buffers",
            SpecKind::GraphQl => "GraphQL SDL",
            SpecKind::Smithy => "Smithy",
            SpecKind::Other => "Text",
        }
    }

    /// The name in pills and lists.
    fn short(self) -> &'static str {
        match self {
            SpecKind::OpenApi => "OpenAPI",
            SpecKind::AsyncApi => "AsyncAPI",
            SpecKind::Protobuf => "Protobuf",
            SpecKind::GraphQl => "GraphQL",
            SpecKind::Smithy => "Smithy",
            SpecKind::Other => tr("Text"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SpecKind::OpenApi => icons::BRACKETS_CURLY,
            SpecKind::AsyncApi => icons::BROADCAST,
            SpecKind::Protobuf => icons::CUBE,
            SpecKind::GraphQl => icons::GRAPH,
            SpecKind::Smithy => icons::TREE_STRUCTURE,
            SpecKind::Other => icons::FILE_TEXT,
        }
    }

    /// The icon of a file of this kind in the APIs list.
    fn file_icon(self) -> &'static str {
        match self {
            SpecKind::OpenApi => icons::FILE_CODE,
            k => k.icon(),
        }
    }

    fn color(self, dark: bool) -> Color32 {
        match self {
            SpecKind::OpenApi => theme::method_color("GET", dark),
            SpecKind::AsyncApi => theme::method_color("WS", dark),
            SpecKind::Protobuf => theme::method_color("PATCH", dark),
            SpecKind::GraphQl => theme::method_color("GQL", dark),
            SpecKind::Smithy => theme::method_color("POST", dark),
            SpecKind::Other => theme::palette(dark).text_muted,
        }
    }

    fn lang(self, path: &Path) -> Lang {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match self {
            SpecKind::GraphQl => Lang::GraphQl,
            SpecKind::Protobuf | SpecKind::Smithy if ext != "json" => Lang::JavaScript,
            _ if ext == "json" => Lang::Json,
            _ if ext == "yaml" || ext == "yml" => Lang::Yaml,
            _ => Lang::Text,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Error,
    Warning,
}

/// One finding of the validation, with its position when it is known.
#[derive(Clone, Debug)]
struct Problem {
    level: Level,
    line: Option<usize>,
    col: Option<usize>,
    message: String,
    /// Another file of a multi-file model (relative to its folder).
    file: Option<String>,
}

impl Problem {
    /// A message that may start with `line:col:` or mention `line N column M`.
    fn parse(level: Level, text: &str) -> Problem {
        if let Some((pos, rest)) = text.split_once(": ") {
            let mut parts = pos.split(':');
            if let (Some(l), Some(c), None) = (parts.next(), parts.next(), parts.next())
                && let (Ok(line), Ok(col)) = (l.trim().parse::<usize>(), c.trim().parse::<usize>())
            {
                return Problem { level, line: Some(line), col: Some(col), message: rest.to_string(), file: None };
            }
        }
        let (line, col) = locate(text);
        Problem { level, line, col, message: text.to_string(), file: None }
    }
}

/// The first `line N` (and `column M`) mentioned in a parser message.
fn locate(text: &str) -> (Option<usize>, Option<usize>) {
    let number_after = |s: &str, word: &str| -> Option<(usize, usize)> {
        let i = s.find(word)?;
        let rest = &s[i + word.len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok().map(|n| (n, i + word.len() + digits.len()))
    };
    let Some((line, end)) = number_after(text, "line ") else { return (None, None) };
    let after = &text[end..];
    let col = number_after(after, "column ").filter(|(_, at)| *at < 16).map(|(c, _)| c);
    (Some(line).filter(|l| *l > 0), col)
}

pub struct Check {
    pub ok: bool,
    pub summary: String,
    /// The findings of the open file as text (`problems` is what is shown).
    #[cfg_attr(not(test), allow(dead_code))]
    pub warnings: Vec<String>,
    problems: Vec<Problem>,
}

impl Check {
    fn new(ok: bool, summary: String, warnings: Vec<String>) -> Check {
        let mut problems = Vec::new();
        if !ok {
            problems.push(Problem::parse(Level::Error, &summary));
        }
        problems.extend(warnings.iter().map(|w| Problem::parse(Level::Warning, w)));
        Check { ok, summary, warnings, problems }
    }
}

/// What the tab shows: the editor, the editor and the docs, or the docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Editor,
    Split,
    Docs,
}

pub struct SpecTab {
    pub path: PathBuf,
    text: String,
    saved: String,
    kind: SpecKind,
    check: Option<Check>,
    last_edit: Option<Instant>,
    target: Option<Id>,
    /// The result of the last sync, shown under the toolbar until closed.
    last_sync: Option<String>,
    /// The collection of the last sync (named in the note, opened by it).
    last_sync_with: Option<Id>,
    /// Automation: open the Sync popover in the next frame.
    open_sync: bool,
    /// Automation: an action for the next frame (see [`SpecTab::automation`]).
    pub auto: Option<String>,
    view: View,
    /// Live documentation preview (FR-D1): the spec as a collection, rebuilt
    /// after edits.
    docs: Option<Result<Collection, String>>,
    /// `info.version` of an OpenAPI spec, for the docs preview.
    docs_version: Option<String>,
    /// A problem was clicked: put the cursor on this line and column.
    jump: Option<(usize, usize)>,
    /// An endpoint of the docs index was clicked: scroll to this request.
    docs_target: Option<Id>,
    /// The editor's widget id (cursor position for the status line).
    editor_id: Option<egui::Id>,
    /// Smithy: the other `.smithy`/JSON files of the folder, by path.
    smithy_files: BTreeMap<PathBuf, CachedSmithy>,
    /// Smithy: the files the open one was last validated with.
    smithy_model: Option<SmithyModel>,
    /// Smithy: the files come from a `smithy-build.json` (all of them form
    /// the model) rather than from the folder.
    smithy_declared: bool,
    /// Smithy: unsaved text of Smithy/JSON files open in other spec tabs, by
    /// path; used instead of what is on disk.
    unsaved_elsewhere: BTreeMap<PathBuf, String>,
    /// Smithy: models of the `maven.dependencies` of the `smithy-build.json`.
    maven: Option<MavenDeps>,
    /// The local Maven repository to read them from (`None`: the user's).
    maven_repository: Option<PathBuf>,
}

/// The Maven dependency models of a `smithy-build.json`, read again only
/// when that file changes (reading them opens jars).
struct MavenDeps {
    build_file: PathBuf,
    stamp: Option<(Option<SystemTime>, u64)>,
    sources: Vec<SourceFile>,
    warnings: Vec<String>,
}

/// A sibling file as last read (`source` is `None` when it is not a Smithy
/// model).
struct CachedSmithy {
    stamp: Stamp,
    source: Option<SourceFile>,
}

/// Where a cached sibling was read from: the file (modification time and
/// size) or the unsaved text of another tab (its hash).
#[derive(PartialEq)]
enum Stamp {
    Disk(Option<SystemTime>, u64),
    Unsaved(u64),
}

/// The other files of the Smithy model the open file belongs to.
struct SmithyModel {
    files: Vec<String>,
    /// Findings in the other files of the folder, `file:line:col: severity: message`
    /// (shown from `Check::problems`).
    #[cfg_attr(not(test), allow(dead_code))]
    problems: Vec<String>,
    /// The folder the file names are relative to.
    base: PathBuf,
}

pub const OPENAPI_TEMPLATE: &str = r#"openapi: 3.0.3
info:
  title: My API
  version: 1.0.0
servers:
  - url: http://localhost:3000
paths:
  /users:
    get:
      summary: List users
      tags: [Users]
      parameters:
        - name: page
          in: query
          schema: { type: integer, example: 1 }
      responses:
        "200":
          description: OK
          content:
            application/json:
              example: [{ id: 1, name: Ada }]
"#;

pub const SMITHY_TEMPLATE: &str = r#"$version: "2"

namespace example.users

use aws.protocols#restJson1

/// Manages users.
@restJson1
@title("Users API")
service Users {
    version: "2024-01-01"
    operations: [ListUsers, GetUser]
}

/// Lists users, one page at a time.
@readonly
@http(method: "GET", uri: "/users")
operation ListUsers {
    input := {
        @httpQuery("page")
        page: Integer
    }
    output := {
        @required
        users: UserList
    }
}

/// Returns one user.
@readonly
@http(method: "GET", uri: "/users/{id}")
operation GetUser {
    input := {
        @required
        @httpLabel
        id: String
    }
    output := {
        @required
        user: User
    }
}

list UserList {
    member: User
}

structure User {
    @required
    id: String
    name: String
}
"#;

/// Automation: turn the live docs preview on.
impl SpecTab {
    pub fn show_preview(&mut self) {
        self.view = View::Split;
        self.rebuild_docs();
    }

    /// Automation: `editor`, `split`, `docs` (the view), `sync` (opens the
    /// Sync popover), `sync:<collection>` (chooses it in the popover),
    /// `synced:<collection>` (syncs the spec into it, as "Spec → collection")
    /// and `spec_sync:<collection>` (all of that plus an unsaved edit).
    fn automation(&mut self, app: &mut TernaryWireApp, action: &str) {
        let find = |name: &str| app.ws.data.collections.iter().find(|c| c.name == name).map(|c| c.id.clone());
        match action.split_once(':') {
            Some(("sync", name)) => {
                self.target = find(name);
                self.open_sync = true;
            }
            Some(("synced", name)) => {
                if let Some(id) = find(name) {
                    self.target = Some(id.clone());
                    self.sync_into(app, &id);
                }
            }
            Some(("spec_sync", name)) => {
                // The fixture's Sync screen: synced, the popover open, and
                // an unsaved edit that gives a warning (a second server).
                if let Some(id) = find(name) {
                    self.target = Some(id.clone());
                    self.sync_into(app, &id);
                    self.open_sync = true;
                }
                if let Some(at) = self.text.find("servers:\n") {
                    let line = "  - url: https://api.staging.bookstore.dev\n";
                    let end = self.text[at..].find("\npaths:").map(|i| at + i + 1).unwrap_or(at + 9);
                    self.text.insert_str(end, line);
                    self.validate();
                }
                self.view = View::Split;
            }
            _ => match action {
                "editor" => self.view = View::Editor,
                "docs" => self.view = View::Docs,
                "sync" => self.open_sync = true,
                _ => self.view = View::Split,
            },
        }
        self.rebuild_docs();
    }

    /// Updates the collection `id` from the spec (Spec → collection) and
    /// notes the result.
    fn sync_into(&mut self, app: &mut TernaryWireApp, id: &Id) {
        match self.import() {
            Ok(spec) => {
                if let Some(c) = app.ws.data.collection_mut(id) {
                    let report = sync_collection(c, &spec);
                    app.ws.mark_collection(id);
                    self.last_sync = Some(report);
                    self.last_sync_with = Some(id.clone());
                }
            }
            Err(e) => app.toast(e, true),
        }
    }
}

/// Spec files in the workspace's `specs/` folder.
pub fn list_specs(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root.join("specs"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

impl SpecTab {
    pub fn open(path: PathBuf) -> Result<SpecTab, String> {
        SpecTab::open_with(path, None)
    }

    /// [`SpecTab::open`] reading Maven dependencies from `maven_repository`.
    fn open_with(path: PathBuf, maven_repository: Option<PathBuf>) -> Result<SpecTab, String> {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let kind = SpecKind::detect(&path, &text);
        let mut tab = SpecTab {
            path,
            saved: text.clone(),
            text,
            kind,
            check: None,
            last_edit: None,
            target: None,
            last_sync: None,
            last_sync_with: None,
            open_sync: false,
            auto: None,
            view: View::Editor,
            docs: None,
            docs_version: None,
            jump: None,
            docs_target: None,
            editor_id: None,
            smithy_files: BTreeMap::new(),
            smithy_model: None,
            smithy_declared: false,
            unsaved_elsewhere: BTreeMap::new(),
            maven: None,
            maven_repository,
        };
        tab.validate();
        Ok(tab)
    }

    pub fn title(&self) -> String {
        self.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "spec".into())
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// Text not written to the file yet (kept in the session file).
    pub fn unsaved_text(&self) -> Option<&str> {
        self.dirty().then_some(self.text.as_str())
    }

    /// Puts back unsaved text from the session file.
    pub fn restore_unsaved(&mut self, text: String) {
        self.text = text;
        self.validate();
    }

    fn validate(&mut self) {
        self.kind = SpecKind::detect(&self.path, &self.text);
        self.check = Some(match self.kind {
            SpecKind::OpenApi => match ternarywire_convert::openapi::import(&self.text) {
                Ok((c, warnings)) => {
                    Check::new(true, trf("Valid {}: {} operations", &[&c.name, &c.request_count()]), warnings)
                }
                Err(e) => Check::new(false, e.to_string(), Vec::new()),
            },
            SpecKind::Smithy => self.check_smithy(),
            SpecKind::AsyncApi => check_asyncapi(&self.text),
            SpecKind::GraphQl => check_braces(&self.text, tr("type definitions")),
            SpecKind::Protobuf => check_braces(&self.text, tr("messages/services")),
            SpecKind::Other => Check::new(true, tr("Not a recognized API specification").into(), Vec::new()),
        });
    }

    /// The spec as a collection (OpenAPI and Smithy, with the other files of
    /// the model).
    fn import(&mut self) -> Result<ternarywire_core::model::Collection, String> {
        match self.kind {
            SpecKind::Smithy => {
                self.refresh_smithy_files();
                let open = SourceFile::parse(self.title(), &self.text);
                smithy::import_sources(&self.smithy_set(&open)).map(|(c, _)| c).map_err(|e| e.to_string())
            }
            _ => ternarywire_convert::openapi::import(&self.text).map(|(c, _)| c).map_err(|e| e.to_string()),
        }
    }

    /// Takes the unsaved text of Smithy/JSON files open in the other spec
    /// tabs, so validation, preview and import see what those tabs show.
    /// Returns true when it changed.
    fn sync_unsaved<'a>(&mut self, others: impl IntoIterator<Item = &'a SpecTab>) -> bool {
        let now: BTreeMap<&Path, &str> = others
            .into_iter()
            .filter(|t| t.path != self.path && is_smithy_candidate(&t.path))
            .filter_map(|t| Some((t.path.as_path(), t.unsaved_text()?)))
            .collect();
        let same = now.len() == self.unsaved_elsewhere.len()
            && now.iter().zip(&self.unsaved_elsewhere).all(|((p, t), (q, u))| *p == q.as_path() && *t == u.as_str());
        if !same {
            self.unsaved_elsewhere = now.into_iter().map(|(p, t)| (p.to_path_buf(), t.to_string())).collect();
        }
        !same
    }

    /// Re-reads the sibling files that are new or changed on disk or in
    /// another tab. Returns the folder their names are relative to.
    fn refresh_smithy_files(&mut self) -> PathBuf {
        let (siblings, base, declared) = match smithy_build_files(&self.path) {
            Some((files, base)) => (files, base, true),
            None => {
                let dir = self.path.parent().map(Path::to_path_buf).unwrap_or_default();
                (smithy_siblings(&dir, &self.path), dir, false)
            }
        };
        if declared != self.smithy_declared {
            self.smithy_files.clear();
            self.smithy_declared = declared;
        }
        self.smithy_files.retain(|p, _| siblings.contains(p));
        for path in siblings {
            let unsaved = self.unsaved_elsewhere.get(&path).map(String::as_str);
            let stamp = match unsaved {
                Some(text) => {
                    let mut h = std::hash::DefaultHasher::new();
                    text.hash(&mut h);
                    Stamp::Unsaved(h.finish())
                }
                None => {
                    let Ok(meta) = std::fs::metadata(&path) else { continue };
                    Stamp::Disk(meta.modified().ok(), meta.len())
                }
            };
            if self.smithy_files.get(&path).is_some_and(|c| c.stamp == stamp) {
                continue;
            }
            let source = read_smithy(&path, &base, unsaved);
            self.smithy_files.insert(path, CachedSmithy { stamp, source });
        }
        self.refresh_maven(declared.then(|| base.join("smithy-build.json")));
        base
    }

    /// Reads the Maven dependency models of `build_file` when it is new or
    /// changed.
    fn refresh_maven(&mut self, build_file: Option<PathBuf>) {
        let Some(build_file) = build_file else {
            self.maven = None;
            return;
        };
        let stamp = std::fs::metadata(&build_file).ok().map(|m| (m.modified().ok(), m.len()));
        if self.maven.as_ref().is_some_and(|m| m.build_file == build_file && m.stamp == stamp) {
            return;
        }
        let resolved = std::fs::read_to_string(&build_file)
            .map_err(|e| e.to_string())
            .and_then(|json| smithy::resolve_maven_dependencies(&json, self.maven_repository.as_deref()));
        let (sources, warnings) = match resolved {
            Ok(m) => (m.sources(), m.warnings),
            Err(e) => (Vec::new(), vec![e]),
        };
        self.maven = Some(MavenDeps { build_file, stamp, sources, warnings });
    }

    /// The open file (first) and the sibling files that form one model with it.
    fn smithy_set<'a>(&'a self, open: &'a SourceFile) -> Vec<&'a SourceFile> {
        let mut all = vec![open];
        all.extend(self.smithy_files.values().filter_map(|c| c.source.as_ref()));
        if self.smithy_declared {
            // Declared in smithy-build.json: one model, broken files included,
            // then its Maven dependencies (the first file sets the namespace).
            all.extend(self.maven.iter().flat_map(|m| m.sources.iter()));
            return all;
        }
        smithy::related_files(&all, 0).into_iter().map(|i| all[i]).collect()
    }

    /// Validates the open text with the other files of its model: the
    /// findings of the open file go to the check, the rest to `smithy_model`.
    fn check_smithy(&mut self) -> Check {
        let base = self.refresh_smithy_files();
        let name = self.title();
        let open = SourceFile::parse(name.clone(), &self.text);
        let set = self.smithy_set(&open);
        let checked = smithy::check_sources(&set);
        let level = |d: &smithy::Diagnostic| if d.is_error() { Level::Error } else { Level::Warning };
        let position = |n: u32| Some(n as usize).filter(|n| *n > 0);
        let warnings =
            checked.in_file(&name).map(|d| format!("{}:{}: {}", d.pos.line, d.pos.column, d.message)).collect();
        let mut found: Vec<Problem> = checked
            .in_file(&name)
            .map(|d| Problem {
                level: level(d),
                line: position(d.pos.line),
                col: position(d.pos.column),
                message: d.message.clone(),
                file: None,
            })
            .collect();
        let mut problems: Vec<String> = Vec::new();
        for d in checked.diagnostics.iter().filter(|d| d.file.as_deref() != Some(name.as_str())) {
            problems.push(d.to_string());
            found.push(Problem {
                level: level(d),
                line: position(d.pos.line),
                col: position(d.pos.column),
                message: d.message.clone(),
                file: d.file.clone(),
            });
        }
        // Broken siblings are not part of the model (their references are
        // unknown) but may well belong to it.
        let in_set = |f: &SourceFile| set.iter().any(|s| std::ptr::eq(*s, f));
        for f in self.smithy_files.values().filter_map(|c| c.source.as_ref()) {
            if let Some(d) = f.syntax_error()
                && !in_set(f)
            {
                problems.push(d.to_string());
                found.push(Problem {
                    level: Level::Error,
                    line: position(d.pos.line),
                    col: position(d.pos.column),
                    message: d.message.clone(),
                    file: Some(d.file.clone().unwrap_or_else(|| f.name.clone())),
                });
            }
        }
        if let Some(m) = &self.maven {
            problems.extend(m.warnings.iter().cloned());
            found.extend(m.warnings.iter().map(|w| Problem {
                level: Level::Warning,
                line: None,
                col: None,
                message: w.clone(),
                file: Some("smithy-build.json".into()),
            }));
        }
        let files = set.iter().skip(1).map(|f| f.name.clone()).collect();
        self.smithy_model = Some(SmithyModel { files, problems, base });
        Check { ok: checked.errors() == 0, summary: checked.summary(), warnings, problems: found }
    }

    fn convertible(&self) -> bool {
        matches!(self.kind, SpecKind::OpenApi | SpecKind::Smithy)
    }

    fn rebuild_docs(&mut self) {
        if self.view == View::Editor || !self.convertible() {
            self.docs = None;
            return;
        }
        self.docs = Some(self.import());
        self.docs_version = (self.kind == SpecKind::OpenApi)
            .then(|| {
                let v: serde_json::Value = if self.text.trim_start().starts_with('{') {
                    serde_json::from_str(&self.text).ok()?
                } else {
                    serde_yaml_ng::from_str(&self.text).ok()?
                };
                let version = v.get("info")?.get("version")?;
                Some(version.as_str().map(str::to_string).unwrap_or_else(|| version.to_string()))
            })
            .flatten();
    }

    pub fn save(&mut self) -> Result<(), String> {
        ternarywire_core::storage::write_atomic(&self.path, self.text.as_bytes()).map_err(|e| e.to_string())?;
        self.saved = self.text.clone();
        Ok(())
    }
}

/// A `.smithy` or `.json` file (possibly part of a Smithy model).
fn is_smithy_candidate(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("smithy") || e.eq_ignore_ascii_case("json"))
}

/// `.smithy` and `.json` files next to `open` (not `open` itself), sorted.
fn smithy_siblings(dir: &Path, open: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().map(|e| e.path()).filter(|p| p != open && p.is_file() && is_smithy_candidate(p)).collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The model files of the nearest `smithy-build.json` (in the open file's
/// folder or above it, not past the workspace root) and its folder: its
/// `sources` and `imports`, folders read recursively (hidden and `build`
/// folders skipped), without the open file.
fn smithy_build_files(open: &Path) -> Option<(Vec<PathBuf>, PathBuf)> {
    let dir = open.ancestors().skip(1).find(|d| {
        d.join("smithy-build.json").is_file() || d.join("ternarywire.json").is_file() || d.parent().is_none()
    })?;
    let text = std::fs::read_to_string(dir.join("smithy-build.json")).ok()?;
    let entries = smithy::build_config_paths(&text).ok()?;
    fn walk(path: &Path, out: &mut Vec<PathBuf>) {
        if path.is_dir() {
            let mut entries: Vec<PathBuf> =
                std::fs::read_dir(path).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
            entries.sort();
            for p in entries {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if p.is_file() || (!name.starts_with('.') && name != "build") {
                    walk(&p, out);
                }
            }
        } else if is_smithy_candidate(path) && path.file_name().is_some_and(|n| n != "smithy-build.json") {
            out.push(path.to_path_buf());
        }
    }
    let mut files = Vec::new();
    for e in entries {
        walk(&dir.join(e), &mut files);
    }
    files.retain(|p| p != open);
    files.dedup();
    Some((files, dir.to_path_buf()))
}

/// A sibling file as a Smithy source: every `.smithy` file (even a broken
/// one), and `.json` files that are Smithy JSON ASTs. Named by its path
/// relative to `base`. Read from disk, or `unsaved` when another tab has it.
fn read_smithy(path: &Path, base: &Path, unsaved: Option<&str>) -> Option<SourceFile> {
    let name = path.strip_prefix(base).unwrap_or(path).display().to_string();
    let is_json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if is_json {
        // Skip other (possibly large) JSON specs without parsing them; Smithy
        // writes the `"smithy"` version first.
        let mut head = Vec::new();
        match unsaved {
            Some(text) => head.extend(text.bytes().take(4096)),
            None => {
                std::fs::File::open(path).ok()?.take(4096).read_to_end(&mut head).ok()?;
            }
        }
        if !String::from_utf8_lossy(&head).contains("\"smithy\"") {
            return None;
        }
    }
    let text = match unsaved {
        Some(text) => Cow::Borrowed(text),
        None => Cow::Owned(std::fs::read_to_string(path).ok()?),
    };
    if is_json { SourceFile::detect(name, &text) } else { Some(SourceFile::parse(name, &text)) }
}

fn check_asyncapi(text: &str) -> Check {
    let value: Result<serde_json::Value, String> = if text.trim_start().starts_with('{') {
        serde_json::from_str(text).map_err(|e| e.to_string())
    } else {
        serde_yaml_ng::from_str(text).map_err(|e| e.to_string())
    };
    match value {
        Ok(v) => {
            let version = v.get("asyncapi").and_then(|x| x.as_str()).unwrap_or("?").to_string();
            let channels = v.get("channels").and_then(|c| c.as_object()).map(|c| c.len()).unwrap_or(0);
            let mut warnings = Vec::new();
            if v.get("info").is_none() {
                warnings.push(tr("missing `info`").into());
            }
            Check::new(true, trf("AsyncAPI {}: {} channels", &[&version, &channels]), warnings)
        }
        Err(e) => Check::new(false, e, Vec::new()),
    }
}

/// Lightweight structural check for SDL/proto files: balanced braces and a
/// count of top-level blocks.
fn check_braces(text: &str, what: &str) -> Check {
    let mut depth: i64 = 0;
    let mut blocks = 0;
    for (line_no, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("").split('#').next().unwrap_or("");
        for ch in code.chars() {
            match ch {
                '{' => {
                    if depth == 0 {
                        blocks += 1;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth < 0 {
                        return Check::new(false, trf("unexpected `}` on line {}", &[&(line_no + 1)]), Vec::new());
                    }
                }
                _ => {}
            }
        }
    }
    if depth != 0 {
        return Check::new(false, trf("{} unclosed `{`", &[&depth]), Vec::new());
    }
    Check::new(true, format!("{blocks} {what}"), Vec::new())
}

fn path_key(r: &RequestItem) -> Option<(String, String)> {
    let h = r.http()?;
    let (base, _, _) = ternarywire_core::urlutil::split(&h.url);
    // Drop scheme/host (often `{{baseUrl}}`) and normalize `:param`/`{param}`.
    let path = match base.find("://") {
        Some(i) => base[i + 3..].find('/').map(|j| &base[i + 3 + j..]).unwrap_or("/"),
        None => base.find('/').map(|j| &base[j..]).unwrap_or("/"),
    };
    let norm: Vec<String> = path
        .trim_matches('/')
        .split('/')
        .map(|s| if s.starts_with(':') || s.starts_with('{') { ":".to_string() } else { s.to_lowercase() })
        .collect();
    Some((h.method.to_uppercase(), norm.join("/")))
}

/// Updates `target` from a collection generated from a spec (FR-D2 manual
/// sync): matching operations (method + path) get the spec's URL, missing
/// params/headers and description; new operations are added; scripts,
/// examples and auth the user added are kept.
pub fn sync_collection(target: &mut Collection, from_spec: &Collection) -> String {
    let mut updated = 0;
    let mut added = 0;
    let existing: Vec<(String, (String, String))> =
        target.requests().into_iter().filter_map(|r| path_key(r).map(|k| (r.id.clone(), k))).collect();
    let mut seen = std::collections::HashSet::new();
    for spec_req in from_spec.requests() {
        let Some(key) = path_key(spec_req) else { continue };
        seen.insert(key.clone());
        match existing.iter().find(|(_, k)| *k == key) {
            Some((id, _)) => {
                if let Some(r) = target.find_request_mut(id) {
                    let spec_http = spec_req.http().cloned().unwrap_or_default();
                    if let Some(h) = r.http_mut() {
                        h.url = spec_http.url.clone();
                        for p in spec_http.params {
                            if !h.params.iter().any(|x| x.key == p.key) {
                                h.params.push(p);
                            }
                        }
                        h.sync_url_from_params();
                        for hd in spec_http.headers {
                            if !h.headers.iter().any(|x| x.key.eq_ignore_ascii_case(&hd.key)) {
                                h.headers.push(hd);
                            }
                        }
                        h.path_vars = ternarywire_core::urlutil::merge_path_vars(&h.url, &h.path_vars);
                    }
                    if r.description.is_empty() {
                        r.description = spec_req.description.clone();
                    }
                    updated += 1;
                }
            }
            None => {
                target.items.push(Item::Request(spec_req.clone()));
                added += 1;
            }
        }
    }
    for v in &from_spec.variables {
        if !target.variables.iter().any(|x| x.key == v.key) {
            target.variables.push(v.clone());
        }
    }
    let missing = existing.iter().filter(|(_, k)| !seen.contains(k)).count();
    trf(
        "{} updated, {} added, {} in the collection but not in the spec (left untouched)",
        &[&updated, &added, &missing],
    )
}

// ---------------------------------------------------------------------------
// Tab
// ---------------------------------------------------------------------------

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut SpecTab) {
    // The other files of a Smithy model may be open in other tabs with
    // unsaved changes (this tab is out of `app.tabs` while shown).
    let others = app.tabs.iter().filter_map(|t| match &t.kind {
        TabKind::Spec(s) => Some(s.as_ref()),
        _ => None,
    });
    if tab.kind == SpecKind::Smithy && tab.sync_unsaved(others) {
        tab.validate();
        tab.rebuild_docs();
    }
    if let Some(a) = tab.auto.take() {
        tab.automation(app, &a);
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let narrow = ui.available_width() < 760.0;
    header(app, ui, tab);
    toolbar(app, ui, tab);
    if let Some(report) = tab.last_sync.clone()
        && sync_note(app, ui, tab, &report)
    {
        tab.last_sync = None;
    }
    let convertible = tab.convertible();
    match tab.view {
        View::Docs if convertible => docs_view(ui, tab, true),
        View::Split if convertible => {
            // The editor (54 %) and the docs on the well fill, side by side
            // or, when narrow, stacked.
            let id = egui::Id::unique(("spec-split", &tab.path));
            let tab = std::cell::RefCell::new(&mut *tab);
            kit::panes(
                ui,
                id,
                !narrow,
                0.54,
                |ui| editor_area(app, ui, &mut tab.borrow_mut()),
                |ui| {
                    let r = ui.max_rect();
                    ui.painter().rect_filled(r, 0.0, theme::pal(ui).well);
                    docs_view(ui, &mut tab.borrow_mut(), false);
                },
            );
        }
        _ => editor_area(app, ui, tab),
    }
    // Re-validate shortly after typing stops.
    if let Some(t) = tab.last_edit {
        if t.elapsed() > Duration::from_millis(400) {
            tab.validate();
            tab.rebuild_docs();
            tab.last_edit = None;
        } else {
            ui.ctx().request_repaint_after(Duration::from_millis(450));
        }
    }
    delete_confirm(app, ui);
}

/// The editor filling the space left, with the problems docked under it
/// (their height follows their count, at most 5 rows).
fn editor_area(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut SpecTab) {
    let rows = problem_rows(tab);
    let shown = rows.len().min(5) as f32;
    let height = 32.0 + 1.0 + shown * 28.0;
    let rect = ui.available_rect_before_wrap();
    let problems = Rect::from_min_max(Pos2::new(rect.left(), (rect.bottom() - height).max(rect.top())), rect.max);
    let editor_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), problems.top()));
    editor(ui, tab, editor_rect);
    let mut child = ui.new_child(UiBuilder::new().max_rect(problems).layout(Layout::top_down(Align::Min)));
    problems_panel(app, &mut child, tab, &rows);
    ui.advance_cursor_after_rect(rect);
}

/// The path of the file shown above its name: its folders inside the
/// workspace (or the folder it is in, outside of it).
fn breadcrumb(app: &TernaryWireApp, path: &Path) -> Vec<String> {
    let parent = path.parent().unwrap_or(Path::new(""));
    match parent.strip_prefix(app.ws.root()) {
        Ok(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect(),
        Err(_) => parent.file_name().map(|n| vec![n.to_string_lossy().to_string()]).unwrap_or_default(),
    }
}

/// The 46 px title row: `specs › file`, unsaved dot, kind and validity
/// pills; Save and more on the right.
fn header(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut SpecTab) {
    let p = theme::pal(ui);
    let crumbs = breadcrumb(app, &tab.path);
    kit::bar(ui, kit::size::VIEW_TITLE, true, |ui| {
        ui.add_space(4.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let more = kit::icon_button(ui, icons::DOTS_THREE, tr("More actions"));
            egui::Popup::menu(&more).width(220.0).align(egui::RectAlign::BOTTOM_END).show(|ui| {
                if tab.convertible()
                    && kit::menu_item(ui, Some(icons::ARROW_SQUARE_OUT), tr("Open docs in browser"), None).clicked()
                {
                    match tab.import() {
                        Ok(c) => crate::dialogs::preview_docs(app, &c),
                        Err(e) => app.toast(e, true),
                    }
                }
                if kit::menu_item(ui, Some(icons::COPY), tr("Copy file path"), None).clicked() {
                    ui.ctx().copy_text(tab.path.display().to_string());
                }
                kit::menu_sep(ui);
                let delete = kit::MenuItem {
                    icon: Some(icons::TRASH),
                    label: tr("Delete…"),
                    danger: true,
                    ..Default::default()
                };
                if kit::menu_item_ex(ui, delete).clicked() {
                    ui.data_mut(|d| d.insert_temp(delete_id(), tab.path.clone()));
                }
            });
            let save = kit::button(
                ui,
                kit::Kind::Secondary,
                kit::Size::Sm,
                tr("Save"),
                kit::ButtonOpts { icon: Some(icons::FLOPPY_DISK), ..Default::default() },
            )
            .on_hover_text(format!("{} (⌘S)", tr("Save")));
            if save.clicked() {
                match tab.save() {
                    Ok(()) => app.toast(tr("Saved"), false),
                    Err(e) => app.toast(trf("Save failed: {}", &[&e]), true),
                }
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let narrow = ui.available_width() < 420.0;
                ui.label(RichText::new(tab.kind.icon()).font(theme::icon_font(13.0)).color(p.text_faint));
                if !narrow {
                    for c in &crumbs {
                        ui.label(RichText::new(c).size(12.5).color(p.text_muted));
                        ui.label(RichText::new(icons::CARET_RIGHT).font(theme::icon_font(11.0)).color(p.text_faint));
                    }
                }
                let pills_w = if narrow { 110.0 } else { 190.0 };
                let title_w = (ui.available_width() - pills_w).max(60.0);
                ui.allocate_ui_with_layout(vec2(title_w, 30.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(tab.title()).font(theme::semibold(14.0)).color(p.text))
                            .truncate(),
                    )
                    .on_hover_text(tab.path.display().to_string());
                });
                if tab.dirty() {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.5, p.accent);
                    resp.on_hover_text(tr("Unsaved changes"));
                }
                kit::pill_ex(
                    ui,
                    tab.kind.short(),
                    Tone::Info,
                    kit::PillOpts { icon: Some(tab.kind.icon()), ..Default::default() },
                )
                .on_hover_text(tab.kind.label());
                validity_pill(ui, tab);
            });
        });
    });
}

/// `Valid`, `2 warnings` or `3 errors`, with the summary as tooltip.
fn validity_pill(ui: &mut Ui, tab: &SpecTab) {
    let Some(c) = &tab.check else { return };
    let errors = c.problems.iter().filter(|x| x.level == Level::Error && x.file.is_none()).count();
    let warnings = c.problems.iter().filter(|x| x.level == Level::Warning && x.file.is_none()).count();
    let (icon, text, tone) = if tab.kind == SpecKind::Other {
        (icons::QUESTION, tr("Not validated").to_string(), Tone::Neutral)
    } else if !c.ok {
        let n = errors.max(1);
        (icons::X_CIRCLE, if n == 1 { tr("1 error").to_string() } else { trf("{} errors", &[&n]) }, Tone::Err)
    } else if warnings > 0 {
        let text = if warnings == 1 { tr("1 warning").to_string() } else { trf("{} warnings", &[&warnings]) };
        (icons::WARNING, text, Tone::Warn)
    } else {
        (icons::CHECK, tr("Valid").to_string(), Tone::Ok)
    };
    kit::pill_ex(ui, &text, tone, kit::PillOpts { icon: Some(icon), ..Default::default() }).on_hover_text(&c.summary);
}

/// The 46 px toolbar: Editor / Split / Docs on the left; Validate, Sync and
/// Generate collection on the right.
fn toolbar(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut SpecTab) {
    let p = theme::pal(ui);
    let convertible = tab.convertible();
    kit::bar(ui, kit::size::VIEW_TITLE, true, |ui| {
        if convertible {
            let mut view = tab.view;
            let items = [
                (View::Editor, Some(icons::CODE), tr("Editor")),
                (View::Split, Some(icons::COLUMNS), tr("Split")),
                (View::Docs, Some(icons::BOOK_OPEN), tr("Docs")),
            ];
            if kit::segmented_ex(ui, &mut view, &items, true) {
                tab.view = view;
                tab.rebuild_docs();
            }
        } else if let Some(c) = &tab.check {
            ui.add(egui::Label::new(RichText::new(&c.summary).size(12.5).color(p.text_muted)).truncate())
                .on_hover_text(tr("Documentation and collections are made from OpenAPI and Smithy specs."));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let small = |icon: &'static str| kit::ButtonOpts { icon: Some(icon), ..Default::default() };
            if convertible {
                let label = if ui.available_width() < 340.0 { tr("Generate") } else { tr("Generate collection") };
                let generate = kit::button(ui, kit::Kind::Primary, kit::Size::Sm, label, small(icons::STACK));
                if generate.on_hover_text(tr("Create a collection with a request for every operation")).clicked() {
                    match tab.import() {
                        Ok(mut c) => {
                            c.id = ternarywire_core::util::new_id();
                            let name = c.name.clone();
                            let id = app.ws.add_collection(c);
                            app.sidebar.expanded.insert(id.clone());
                            tab.target = Some(id);
                            app.toast(trf("Collection “{}” created from the spec", &[&name]), false);
                        }
                        Err(e) => app.toast(e, true),
                    }
                }
                let popup_id = egui::Id::unique(("spec-sync-popover", &tab.path));
                if std::mem::take(&mut tab.open_sync) {
                    egui::Popup::open_id(ui.ctx(), popup_id);
                }
                let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
                let sync = kit::button(
                    ui,
                    kit::Kind::Secondary,
                    kit::Size::Sm,
                    tr("Sync"),
                    kit::ButtonOpts {
                        icon: Some(icons::ARROWS_LEFT_RIGHT),
                        caret: true,
                        on: open,
                        ..Default::default()
                    },
                );
                let sync = sync.on_hover_text(tr("Update a collection from this spec, or the spec from a collection"));
                let style = ui.style().clone();
                egui::Popup::menu(&sync)
                    .id(popup_id)
                    .width(312.0)
                    .align(egui::RectAlign::BOTTOM_END)
                    .gap(6.0)
                    .frame(
                        egui::Frame::menu(&style)
                            .inner_margin(egui::Margin::same(12))
                            .corner_radius(CornerRadius::same(11)),
                    )
                    .layout(Layout::top_down(Align::Min))
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| sync_menu(app, ui, tab));
            }
            let validate =
                kit::button(ui, kit::Kind::Secondary, kit::Size::Sm, tr("Validate"), small(icons::SHIELD_CHECK));
            if validate.on_hover_text(tr("Check the spec again, with the files it uses")).clicked() {
                tab.validate();
                tab.rebuild_docs();
                let n = tab.check.as_ref().map_or(0, |c| c.problems.len());
                if n == 0 {
                    app.toast(tr("No problems found"), false);
                } else {
                    app.toast(crate::flow_tab::problems_text(n), true);
                }
            }
        });
    });
}

/// A collection to sync with, in the Sync popover: a check when chosen.
fn sync_option(ui: &mut Ui, name: &str, detail: &str, on: bool) -> egui::Response {
    let p = theme::pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    widgets::a11y_tab(&resp, name, on);
    if on || resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), p.hover);
    }
    let cy = rect.center().y;
    let mut x = rect.left() + 8.0;
    if on {
        ui.painter().text(
            pos2(x + 8.0, cy),
            Align2::CENTER_CENTER,
            icons::CHECK,
            theme::icon_font(13.0),
            p.accent_text,
        );
    }
    x += 16.0 + 9.0;
    ui.painter().text(pos2(x, cy), Align2::LEFT_CENTER, icons::STACK, theme::icon_font(13.0), p.text_faint);
    x += 13.0 + 9.0;
    let d = ui.painter().layout_no_wrap(detail.to_string(), FontId::proportional(11.0), p.text_faint);
    let right = rect.right() - 8.0 - d.size().x;
    ui.painter().galley(pos2(right, cy - d.size().y / 2.0), d, p.text_faint);
    let g = kit::elide(ui, name, FontId::proportional(12.5), p.text, right - 9.0 - x);
    ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, p.text);
    resp
}

/// One of the two sync actions: a tile, a title and what it does.
fn sync_action(ui: &mut Ui, icon: &str, primary: bool, title: &str, hint: &str) -> egui::Response {
    let p = theme::pal(ui);
    let bg = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 9)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (t, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
            let (fill, fg) = if primary { (p.accent_soft, p.accent_text) } else { (p.subtle, p.text_muted) };
            ui.painter().rect_filled(t, CornerRadius::same(7), fill);
            ui.painter().text(t.center(), Align2::CENTER_CENTER, icon, theme::icon_font(15.0), fg);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(title).font(theme::medium(13.0)).color(p.text));
                ui.add(egui::Label::new(RichText::new(hint).size(12.0).color(p.text_muted)).wrap());
            });
        });
    });
    let rect = inner.response.rect;
    let resp = ui.interact(rect, ui.make_persistent_id(("sync-action", title)), Sense::click());
    widgets::a11y_button(&resp, title);
    let hovered = resp.hovered();
    ui.painter().set(
        bg,
        egui::Shape::rect_filled(rect, CornerRadius::same(8), if hovered { p.well } else { Color32::TRANSPARENT }),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, if hovered { p.border_strong } else { p.border }),
        StrokeKind::Inside,
    );
    resp
}

/// The Sync popover: choose a collection, then update it from the spec (or
/// the spec from it).
fn sync_menu(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut SpecTab) {
    let p = theme::pal(ui);
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.label(RichText::new(tr("Sync with a collection")).font(theme::semibold(13.0)).color(p.text));
    ui.add_space(2.0);
    ui.add(
        egui::Label::new(kit::hint(
            ui,
            tr("Operations match by method and path. Your scripts, auth and examples are kept."),
        ))
        .wrap(),
    );
    ui.add_space(10.0);
    if app.ws.data.collections.is_empty() {
        ui.label(RichText::new(tr("No collections yet: generate one from the spec first.")).color(p.text_muted));
        return;
    }
    ui.label(kit::eyebrow(ui, tr("Collection")));
    ui.add_space(4.0);
    let forks: Vec<&str> = app.ws.forks.iter().map(|f| f.fork.as_str()).collect();
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for c in &app.ws.data.collections {
            let on = tab.target.as_deref() == Some(c.id.as_str());
            let detail = if forks.contains(&c.id.as_str()) {
                tr("fork").to_string()
            } else {
                trf("{} requests", &[&c.request_count()])
            };
            if sync_option(ui, &c.name, &detail, on).clicked() {
                tab.target = Some(c.id.clone());
            }
        }
    });
    ui.add_space(10.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.border));
    ui.add_space(11.0);
    let Some(target) = tab.target.clone() else {
        ui.label(kit::hint(ui, tr("Choose the collection to sync with.")));
        return;
    };
    ui.spacing_mut().item_spacing.y = 6.0;
    if sync_action(ui, icons::ARROW_RIGHT, true, tr("Spec → collection"), tr("Update the collection from this spec"))
        .clicked()
    {
        tab.sync_into(app, &target);
        ui.close();
    }
    if tab.kind == SpecKind::OpenApi
        && sync_action(
            ui,
            icons::ARROW_LEFT,
            false,
            tr("Collection → spec"),
            tr("Replace this spec with one generated from the collection (not saved until you press Save)"),
        )
        .clicked()
        && let Some(c) = app.ws.data.collection(&target)
    {
        let v = ternarywire_convert::openapi::export(c);
        let is_json = tab.path.extension().map(|e| e == "json").unwrap_or(false);
        tab.text = if is_json {
            serde_json::to_string_pretty(&v).unwrap_or_default()
        } else {
            serde_yaml_ng::to_string(&v).unwrap_or_default()
        };
        tab.last_edit = Some(Instant::now() - Duration::from_secs(1));
        tab.last_sync = Some(tr("Spec regenerated from the collection (not saved yet)").into());
        tab.last_sync_with = Some(target.clone());
        ui.close();
    }
}

/// The result of the last sync: a 36 px info bar under the toolbar with
/// "Open collection" and a close button. Returns true when closed.
fn sync_note(app: &mut TernaryWireApp, ui: &mut Ui, tab: &SpecTab, report: &str) -> bool {
    let p = theme::pal(ui);
    let with =
        tab.last_sync_with.as_ref().and_then(|id| app.ws.data.collection(id)).map(|c| (c.id.clone(), c.name.clone()));
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(Rect::from_min_size(full.min, vec2(full.width(), 36.0)), 0.0, p.info_soft);
    let mut close = false;
    let mut open = None;
    kit::bar(ui, 36.0, true, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(icons::CHECK_CIRCLE).font(theme::icon_font(14.0)).color(p.info));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            close = kit::icon_button_sm(ui, icons::X, tr("Dismiss")).clicked();
            if let Some((id, _)) = &with
                && kit::ghost(ui, None, tr("Open collection")).clicked()
            {
                open = Some(id.clone());
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let text = TextFormat::simple(FontId::proportional(12.5), p.text);
                let mut job = LayoutJob::default();
                match &with {
                    Some((_, name)) if report != tr("Spec regenerated from the collection (not saved yet)") => {
                        let s = tr("Synced with {}: {}");
                        let mut parts = s.splitn(3, "{}");
                        job.append(parts.next().unwrap_or(""), 0.0, text.clone());
                        job.append(name, 0.0, TextFormat::simple(theme::medium(12.5), p.text));
                        job.append(parts.next().unwrap_or(""), 0.0, text.clone());
                        job.append(report, 0.0, text.clone());
                        job.append(parts.next().unwrap_or(""), 0.0, text);
                    }
                    _ => job.append(report, 0.0, text),
                }
                job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width());
                ui.add(egui::Label::new(job).truncate()).on_hover_text(report);
            });
        });
    });
    if let Some(id) = open {
        app.open_container(id, None);
    }
    close
}

/// The editor: frameless with its line-number gutter, filling `rect`.
fn editor(ui: &mut Ui, tab: &mut SpecTab, rect: Rect) {
    let p = theme::pal(ui);
    ui.painter().rect_filled(rect, 0.0, p.surface);
    let lang = tab.kind.lang(&tab.path);
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt("spec-editor").auto_shrink([false; 2]).show(&mut child, |ui| {
        let editor_id = ui.make_persistent_id(("spec", &tab.path));
        let mut scroll_to = None;
        if tab.editor_id == Some(editor_id)
            && let Some((line, col)) = tab.jump.take()
        {
            select_line(ui.ctx(), editor_id, &tab.text, line, col);
            ui.memory_mut(|m| m.request_focus(editor_id));
            scroll_to = Some(line);
        }
        let rows = crate::env_tab::editor_rows(ui, rect.height());
        let r = widgets::code_editor_gutter(ui, ("spec", &tab.path), &mut tab.text, lang, None, rows, false, None);
        widgets::a11y_name(&r, &tab.title());
        if r.changed() {
            tab.last_edit = Some(Instant::now());
        }
        if tab.jump.is_some() {
            ui.ctx().request_repaint();
        }
        tab.editor_id = Some(editor_id);
        if let Some(line) = scroll_to {
            let y = line_offset(ui, &tab.text, line, r.rect.width() - 60.0);
            let target =
                Rect::from_min_size(Pos2::new(r.rect.left(), r.rect.top() + 8.0 + y), vec2(r.rect.width(), 20.0));
            ui.scroll_to_rect(target, Some(Align::Center));
        }
    });
}

fn mono_size(ui: &Ui) -> f32 {
    ui.style().text_styles.get(&egui::TextStyle::Monospace).map(|f| f.size).unwrap_or(12.5)
}

/// Height of the text above `line` (1-based) when wrapped at `width`, in
/// the editor's 20 px lines.
fn line_offset(ui: &Ui, text: &str, line: usize, width: f32) -> f32 {
    if line <= 1 {
        return 0.0;
    }
    let before: String = text.split('\n').take(line - 1).collect::<Vec<_>>().join("\n");
    let mut job = LayoutJob::simple(before, FontId::monospace(mono_size(ui)), Color32::PLACEHOLDER, width.max(50.0));
    for s in &mut job.sections {
        s.format.line_height = Some(20.0);
    }
    ui.painter().layout_job(job).size().y
}

/// Puts the editor's cursor on `line` and selects the line from `col`.
fn select_line(ctx: &egui::Context, id: egui::Id, text: &str, line: usize, col: usize) {
    let mut start = 0;
    let mut current = 1;
    let mut line_len = 0;
    for (i, l) in text.split('\n').enumerate() {
        current = i + 1;
        line_len = l.chars().count();
        if current == line {
            break;
        }
        start += line_len + 1;
    }
    if current != line {
        line_len = 0;
    }
    let from = start + col.saturating_sub(1).min(line_len);
    let to = start + line_len;
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(from), CCursor::new(to.max(from)))));
    state.store(ctx, id);
}

/// `(line, column)` of the editor's cursor, while it has focus.
fn cursor_position(ctx: &egui::Context, id: Option<egui::Id>, text: &str) -> Option<(usize, usize)> {
    let id = id?;
    if !ctx.memory(|m| m.has_focus(id)) {
        return None;
    }
    let index = egui::text_edit::TextEditState::load(ctx, id)?.cursor.char_range()?.primary.index;
    let (mut line, mut col) = (1, 1);
    for c in text.chars().take(index.0) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Some((line, col))
}

/// The problems to list: errors first, then by line.
fn problem_rows(tab: &SpecTab) -> Vec<Problem> {
    let mut rows: Vec<Problem> = tab.check.as_ref().map(|c| c.problems.clone()).unwrap_or_default();
    rows.sort_by_key(|r| (r.file.is_some(), r.level, r.line));
    rows
}

/// The problems under the editor (the prototype's `.w-prob`): a 32 px
/// header with the counts and the cursor position, then a 28 px row per
/// problem (click: jump to it).
fn problems_panel(app: &mut TernaryWireApp, ui: &mut Ui, tab: &mut SpecTab, rows: &[Problem]) {
    let p = theme::pal(ui);
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, p.well);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.add_space(1.0);
    let errors = rows.iter().filter(|r| r.level == Level::Error).count();
    let warnings = rows.len() - errors;
    kit::bar(ui, 32.0, false, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (icon, color, title) = if errors > 0 {
            (icons::X_CIRCLE, p.err, tr("Problems"))
        } else if warnings > 0 {
            (icons::WARNING, p.warn, tr("Problems"))
        } else if tab.kind == SpecKind::Other {
            (icons::CHECK_CIRCLE, p.ok, tr("Not an API spec: nothing to check"))
        } else {
            (icons::CHECK_CIRCLE, p.ok, tr("No problems"))
        };
        ui.label(RichText::new(icon).font(theme::icon_font(13.0)).color(color));
        ui.label(RichText::new(title).font(theme::medium(12.0)).color(p.text));
        if !rows.is_empty() {
            let text = if errors == 1 { tr("1 error").to_string() } else { trf("{} errors", &[&errors]) };
            kit::tone_pill(ui, &text, if errors > 0 { Tone::Err } else { Tone::Neutral });
            let text = if warnings == 1 { tr("1 warning").to_string() } else { trf("{} warnings", &[&warnings]) };
            kit::tone_pill(ui, &text, if warnings > 0 { Tone::Warn } else { Tone::Neutral });
        }
        if let Some(m) = tab.smithy_model.as_ref().filter(|m| tab.kind == SpecKind::Smithy && !m.files.is_empty()) {
            let why = tr("Smithy files of this folder that use this file or are used by it form one model.");
            let text = trf("Model of {} files", &[&(m.files.len() + 1)]);
            kit::pill_ex(ui, &text, Tone::Info, kit::PillOpts { icon: Some(icons::FILES), ..Default::default() })
                .on_hover_text(format!("{why}\n\n{}", m.files.join("\n")));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let size = ternarywire_core::util::human_bytes(tab.text.len() as u64);
            let mut status = format!("{} · {size}", tab.kind.short());
            if let Some((l, c)) = cursor_position(ui.ctx(), tab.editor_id, &tab.text) {
                status = format!("{} · {status}", trf("Ln {}, Col {}", &[&l, &c]));
            }
            let g = ui.painter().layout_no_wrap(status.clone(), theme::mono(11.0), p.text_faint);
            if ui.available_width() > g.size().x + 16.0 {
                ui.label(RichText::new(status).font(theme::mono(11.0)).color(p.text_faint));
            }
        });
    });
    if rows.is_empty() {
        return;
    }
    let mut open_file: Option<(PathBuf, Option<(usize, usize)>)> = None;
    egui::ScrollArea::vertical().id_salt("spec-problems").auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for prob in rows {
            let r = problem_line(ui, prob);
            if r.clicked() {
                match (&prob.file, prob.line) {
                    (None, Some(line)) => tab.jump = Some((line, prob.col.unwrap_or(1))),
                    (Some(file), at) => {
                        let base = tab
                            .smithy_model
                            .as_ref()
                            .map(|m| m.base.clone())
                            .or_else(|| tab.path.parent().map(Path::to_path_buf))
                            .unwrap_or_default();
                        open_file = Some((base.join(file), at.map(|l| (l, prob.col.unwrap_or(1)))));
                    }
                    _ => {}
                }
            }
        }
    });
    if let Some((path, at)) = open_file {
        if !path.is_file() {
            return;
        }
        open_spec(app, path.clone());
        if let Some(TabKind::Spec(t)) = app.tabs.get_mut(app.active).map(|t| &mut t.kind)
            && t.path == path
        {
            t.jump = at;
        }
    }
}

/// A problem: its icon, its position as an outlined pill, the message.
fn problem_line(ui: &mut Ui, prob: &Problem) -> egui::Response {
    let p = theme::pal(ui);
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    let clickable = prob.line.is_some() || prob.file.is_some();
    let resp = resp.on_hover_text(&prob.message);
    widgets::a11y_button(&resp, &prob.message);
    let painter = ui.painter();
    if clickable && resp.hovered() {
        painter.rect_filled(rect, 0.0, p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    let (icon, color) = match prob.level {
        Level::Error => (icons::X_CIRCLE, p.err),
        Level::Warning => (icons::WARNING, p.warn),
    };
    let cy = rect.center().y + 0.5;
    let mut x = rect.left() + 12.0;
    painter.text(Pos2::new(x, cy), Align2::LEFT_CENTER, icon, theme::icon_font(13.0), color);
    x += 13.0 + 8.0;
    let loc = match (&prob.file, prob.line, prob.col) {
        (Some(f), Some(l), Some(c)) => Some(format!("{f}:{l}:{c}")),
        (Some(f), Some(l), None) => Some(format!("{f}:{l}")),
        (Some(f), None, _) => Some(f.clone()),
        (None, Some(l), Some(c)) => Some(format!("{l}:{c}")),
        (None, Some(l), None) => Some(l.to_string()),
        (None, None, _) => None,
    };
    if let Some(loc) = loc {
        let g = painter.layout_no_wrap(loc, theme::mono(11.0), p.text_muted);
        let pill = Rect::from_min_size(Pos2::new(x, cy - 10.0), vec2(g.size().x + 14.0, 20.0));
        painter.rect_stroke(pill, CornerRadius::same(10), Stroke::new(1.0, p.border), StrokeKind::Inside);
        painter.galley(pill.center() - g.size() / 2.0, g, p.text_muted);
        x = pill.right() + 8.0;
    }
    let mut job = LayoutJob::single_section(
        prob.message.replace('\n', " "),
        TextFormat::simple(FontId::proportional(12.0), p.text),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - 12.0 - x).max(10.0));
    let g = painter.layout_job(job);
    painter.galley(Pos2::new(x, cy - g.size().y / 2.0), g, p.text);
    resp
}

// ---------------------------------------------------------------------------
// Documentation preview
// ---------------------------------------------------------------------------

/// The docs preview: on the well beside the editor (split), or centered,
/// at most 780 wide, on the canvas (`page`).
fn docs_view(ui: &mut Ui, tab: &mut SpecTab, page: bool) {
    let p = theme::pal(ui);
    if !tab.convertible() {
        kit::empty_state(
            ui,
            icons::BOOK_OPEN,
            tr("No preview for this format"),
            &trf(
                "Documentation previews are made from OpenAPI and Smithy specs; this file is {}.",
                &[&tab.kind.short()],
            ),
            |_| {},
        );
        return;
    }
    if tab.docs.is_none() {
        tab.rebuild_docs();
    }
    match &tab.docs {
        None => {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(20.0).color(p.accent));
                ui.label(RichText::new(tr("Building the preview…")).color(p.text_muted));
            });
        }
        Some(Err(e)) => {
            let e = e.clone();
            ui.add_space((ui.available_height() * 0.2).clamp(12.0, 120.0));
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                ui.painter().rect(rect, CornerRadius::same(14), p.err_soft, Stroke::NONE, StrokeKind::Inside);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    icons::WARNING_CIRCLE,
                    theme::icon_font(24.0),
                    p.err,
                );
                ui.add_space(14.0);
                ui.label(RichText::new(tr("The preview could not be built")).font(theme::semibold(14.0)).color(p.text));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr("Fix the problems under the editor; the preview follows as you type."))
                        .size(12.5)
                        .color(p.text_muted),
                );
                ui.add_space(12.0);
                egui::Frame::new()
                    .fill(p.err_soft)
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(e).font(theme::mono(12.0)).color(p.err)).wrap());
                    });
            });
        }
        Some(Ok(c)) => {
            let c = Arc::new(c.clone());
            let version = tab.docs_version.clone();
            let mut target = tab.docs_target.take();
            egui::ScrollArea::vertical().id_salt(("spec-docs-scroll", &tab.path)).auto_shrink([false; 2]).show(
                ui,
                |ui| {
                    // 22 26 40 padding; on the page, at most 780 wide and centered.
                    let avail = ui.available_width();
                    let outer = if page { avail.min(780.0) } else { avail };
                    let side = ((avail - outer) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(side);
                        ui.allocate_ui_with_layout(vec2(outer, 0.0), Layout::top_down(Align::Min), |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin { left: 26, right: 26, top: 22, bottom: 40 })
                                .show(ui, |ui| {
                                    ui.set_width((outer - 52.0).max(120.0));
                                    ui.spacing_mut().item_spacing.y = 14.0;
                                    docs_page(ui, &c, version.as_deref(), &mut target);
                                });
                        });
                    });
                },
            );
            tab.docs_target = target;
        }
    }
}

fn docs_page(ui: &mut Ui, c: &Collection, version: Option<&str>, target: &mut Option<Id>) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(RichText::new(&c.name).font(theme::semibold(21.0)).color(p.text));
        if !c.description.trim().is_empty() {
            ui.add(egui::Label::new(RichText::new(c.description.trim()).color(p.text_muted)).wrap());
        }
    });
    let requests = c.requests();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let ops =
            if requests.len() == 1 { tr("1 operation").to_string() } else { trf("{} operations", &[&requests.len()]) };
        kit::pill_ex(ui, &ops, Tone::Info, kit::PillOpts { icon: Some(icons::LIST), ..Default::default() });
        if let Some(v) = version {
            kit::pill_ex(ui, &format!("v{v}"), Tone::Neutral, kit::PillOpts { outline: true, ..Default::default() });
        }
        for v in c.variables.iter().filter(|v| !v.key.is_empty() && !v.is_secret()) {
            variable_chip(ui, &v.key, &v.value);
        }
    });
    if requests.is_empty() {
        ui.label(RichText::new(tr("The spec has no operations yet.")).color(p.text_muted));
        return;
    }
    // Index of the endpoints; a click scrolls to one.
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.label(kit::eyebrow(ui, tr("Endpoints")));
        egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, r) in requests.iter().enumerate() {
                    if index_row(ui, r, i == 0, i + 1 == requests.len()).clicked() {
                        *target = Some(r.id.clone());
                    }
                }
            });
    });
    docs_items(ui, &c.items, 0, target);
}

/// `name value` in a small rounded chip (a variable of the spec).
fn variable_chip(ui: &mut Ui, key: &str, value: &str) {
    let p = theme::pal(ui);
    let mut job = LayoutJob::default();
    job.append(key, 0.0, TextFormat::simple(FontId::proportional(11.5), p.text_faint));
    job.append(value, 6.0, TextFormat::simple(theme::mono(11.5), p.text_muted));
    let g = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 22.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(11), p.surface, Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().galley(pos2(rect.left() + 10.0, rect.center().y - g.size().y / 2.0), g, p.text_muted);
    resp.on_hover_text(format!("{{{{{key}}}}} = {value}"));
}

/// A small upper-case label above a docs block.
fn docs_label(ui: &mut Ui, text: &str) {
    ui.label(kit::eyebrow(ui, text));
}

/// A read-only block of code (a request body or an example) in a well.
fn code_block(ui: &mut Ui, text: &str) {
    let dark = ui.visuals().dark_mode;
    let start = text.trim_start();
    let is_json = (start.starts_with('{') || start.starts_with('['))
        && (text.len() > 64 * 1024 || serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok());
    let lang = if is_json { Lang::Json } else { Lang::Text };
    let key = crate::highlight::HlKey { text, lang, dark, search: "", vars: None, font_size: 12.5, wrap: true };
    let mut job = crate::highlight::cached(ui.ctx(), key);
    for s in &mut job.sections {
        s.format.line_height = Some(20.0);
    }
    kit::well(ui).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        job.wrap.max_width = ui.available_width();
        ui.add(egui::Label::new(job).wrap());
    });
}

/// `GET /books/:id` without the `{{baseUrl}}`, scheme, host and query.
fn docs_path(url: &str) -> String {
    let mut s = url.split('?').next().unwrap_or("").trim();
    if s.starts_with("{{")
        && let Some(end) = s.find("}}")
    {
        s = &s[end + 2..];
    } else if let Some(i) = s.find("://") {
        s = s[i + 3..].find('/').map(|j| &s[i + 3 + j..]).unwrap_or("/");
    }
    if s.is_empty() { "/".into() } else { s.to_string() }
}

/// A path with its parameters (`:id`, `{id}`) in the accent color.
fn path_job(path: &str, p: &Palette, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, seg) in path.split('/').enumerate() {
        if i > 0 {
            job.append("/", 0.0, TextFormat::simple(theme::mono(size), p.text_muted));
        }
        let color = if seg.starts_with(':') || seg.starts_with('{') { p.accent_text } else { p.text };
        job.append(seg, 0.0, TextFormat::simple(theme::mono(size), color));
    }
    job
}

fn method_of(r: &RequestItem) -> String {
    r.http().map(|h| h.method.to_uppercase()).unwrap_or_else(|| r.request.badge())
}

/// A row of the Endpoints index (32 high): method, path, name.
fn index_row(ui: &mut Ui, r: &RequestItem, first: bool, last: bool) -> egui::Response {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    widgets::a11y_button(&resp, &r.name);
    let painter = ui.painter();
    if resp.hovered() {
        let radius = CornerRadius {
            nw: if first { 8 } else { 0 },
            ne: if first { 8 } else { 0 },
            sw: if last { 8 } else { 0 },
            se: if last { 8 } else { 0 },
        };
        painter.rect_filled(rect, radius, p.well);
    }
    if !first {
        painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    }
    let method = method_of(r);
    let mc = theme::method_color(&method, dark);
    let cy = rect.center().y;
    painter.text(Pos2::new(rect.left() + 12.0, cy), Align2::LEFT_CENTER, &method, theme::semibold(11.0), mc);
    let x = rect.left() + 12.0 + 64.0 + 8.0;
    let name = painter.layout_no_wrap(r.name.clone(), FontId::proportional(12.5), p.text_muted);
    let name_w = name.size().x.min(w * 0.4);
    let mut job = path_job(&docs_path(r.request.url()), &p, 12.0);
    job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - 12.0 - 8.0 - name_w - x).max(30.0));
    let g = painter.layout_job(job);
    painter.galley(Pos2::new(x, cy - g.size().y / 2.0), g, p.text);
    let mut job =
        LayoutJob::single_section(r.name.clone(), TextFormat::simple(FontId::proportional(12.5), p.text_muted));
    job.wrap = egui::text::TextWrapping::truncate_at_width(name_w);
    let g = painter.layout_job(job);
    painter.galley(Pos2::new(rect.right() - 12.0 - g.size().x, cy - g.size().y / 2.0), g, p.text_muted);
    resp
}

fn docs_items(ui: &mut Ui, items: &[Item], depth: usize, target: &mut Option<Id>) {
    let p = theme::pal(ui);
    for item in items {
        match item {
            Item::Request(r) => request_card(ui, r, target),
            Item::Folder(f) => {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let size = if depth == 0 { 17.0 } else { 15.0 };
                        ui.label(RichText::new(&f.name).font(theme::semibold(size)).color(p.text));
                        kit::tone_pill(ui, &count_requests(&f.items).to_string(), Tone::Neutral);
                    });
                    if !f.description.trim().is_empty() {
                        ui.add(egui::Label::new(RichText::new(f.description.trim()).color(p.text_muted)).wrap());
                    }
                });
                // A hairline, 14 above the operations of the folder.
                let r = ui.available_rect_before_wrap();
                ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.border));
                ui.add_space(15.0);
                docs_items(ui, &f.items, depth + 1, target);
            }
        }
    }
}

fn count_requests(items: &[Item]) -> usize {
    items
        .iter()
        .map(|i| match i {
            Item::Request(_) => 1,
            Item::Folder(f) => count_requests(&f.items),
        })
        .sum()
}

/// One parameter row of the docs.
struct Param<'a> {
    name: &'a str,
    place: &'static str,
    value: &'a str,
    description: &'a str,
}

/// An operation: name, method and path, parameters, body and responses.
fn request_card(ui: &mut Ui, r: &RequestItem, target: &mut Option<Id>) {
    let p = theme::pal(ui);
    let dark = ui.visuals().dark_mode;
    let resp = kit::card(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.label(RichText::new(&r.name).font(theme::semibold(14.0)).color(p.text));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let method = method_of(r);
            kit::pill(ui, &method, theme::method_color(&method, dark));
            ui.label(path_job(&docs_path(r.request.url()), &p, 12.5));
        });
        if !r.description.trim().is_empty() {
            ui.add(egui::Label::new(RichText::new(r.description.trim()).color(p.text_muted)).wrap());
        }
        if let Some(h) = r.http() {
            let mut params: Vec<Param> = Vec::new();
            for kv in h.path_vars.iter().filter(|k| !k.key.is_empty()) {
                params.push(Param { name: &kv.key, place: "path", value: &kv.value, description: &kv.description });
            }
            for kv in h.params.iter().filter(|k| !k.key.is_empty()) {
                params.push(Param { name: &kv.key, place: "query", value: &kv.value, description: &kv.description });
            }
            for kv in h.headers.iter().filter(|k| !k.key.is_empty()) {
                params.push(Param { name: &kv.key, place: "header", value: &kv.value, description: &kv.description });
            }
            if !params.is_empty() {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    docs_label(ui, tr("Parameters"));
                    param_table(ui, &params);
                });
            }
            let body = h.body.raw.trim();
            if !body.is_empty() && h.body.mode != ternarywire_core::model::BodyMode::None {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    docs_label(ui, tr("Request body"));
                    let pretty = serde_json::from_str::<serde_json::Value>(body)
                        .ok()
                        .and_then(|v| serde_json::to_string_pretty(&v).ok())
                        .unwrap_or_else(|| body.to_string());
                    code_block(ui, &crate::dialogs::ellipsize(&pretty, 4000));
                });
            }
        }
        if !r.examples.is_empty() {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                docs_label(ui, tr("Responses"));
                for e in &r.examples {
                    let open_id = ui.make_persistent_id(("docs-example", &r.id, &e.id));
                    let mut open: bool = ui.data(|d| d.get_temp(open_id).unwrap_or(false));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        kit::tone_pill(ui, &e.status.to_string(), theme::status_tone(e.status));
                        let name = if e.name.trim().is_empty() { e.status_text.as_str() } else { e.name.as_str() };
                        ui.label(RichText::new(name).color(p.text));
                        if !e.body.trim().is_empty() {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let icon = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
                                if kit::ghost(ui, Some(icon), tr("Example")).clicked() {
                                    open = !open;
                                    ui.data_mut(|d| d.insert_temp(open_id, open));
                                }
                            });
                        }
                    });
                    if open && !e.body.trim().is_empty() {
                        code_block(ui, &crate::dialogs::ellipsize(&e.body, 4000));
                    }
                }
            });
        }
    });
    if target.as_deref() == Some(r.id.as_str()) {
        ui.scroll_to_rect(resp.response.rect, Some(Align::TOP));
        *target = None;
    }
}

/// Parameters as a table (the prototype's `.tbl`): name with its
/// description under it, where it goes, an example.
fn param_table(ui: &mut Ui, rows: &[Param]) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = ui.available_width();
            let name_w = w * 0.34;
            let place_w = w * 0.18;
            let head = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover()).0;
            let painter = ui.painter();
            painter.rect_filled(head, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, p.well);
            painter.hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, p.border));
            for (x, text) in [(12.0, tr("Name")), (12.0 + name_w, tr("In")), (12.0 + name_w + place_w, tr("Example"))] {
                painter.text(
                    Pos2::new(head.left() + x, head.center().y),
                    Align2::LEFT_CENTER,
                    text,
                    theme::medium(11.5),
                    p.text_muted,
                );
            }
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    let r = ui.available_rect_before_wrap();
                    ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.border));
                }
                ui.add_space(8.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(12.0);
                    ui.allocate_ui_with_layout(vec2(name_w - 12.0, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(name_w - 24.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let mut job = LayoutJob::default();
                        job.append(row.name, 0.0, TextFormat::simple(theme::mono(12.5), p.text));
                        if row.place == "path" {
                            job.append(" *", 0.0, TextFormat::simple(FontId::proportional(12.5), p.err));
                        }
                        let name = ui.add(egui::Label::new(job).truncate());
                        if row.place == "path" {
                            name.on_hover_text(tr("Required"));
                        }
                        if !row.description.trim().is_empty() {
                            ui.add(egui::Label::new(kit::hint(ui, row.description.trim())).wrap());
                        }
                    });
                    ui.add_space(12.0);
                    ui.allocate_ui_with_layout(vec2(place_w, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(place_w - 12.0);
                        ui.label(RichText::new(row.place).size(12.5).color(p.text_muted));
                    });
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width() - 12.0, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(row.value).font(theme::mono(12.5)).color(p.text)).wrap(),
                            );
                        },
                    );
                });
                ui.add_space(8.0);
            }
        });
}

// ---------------------------------------------------------------------------
// Sidebar list
// ---------------------------------------------------------------------------

/// Format of a spec file for the list, read again when the file changes.
fn list_kind(ui: &Ui, path: &Path) -> SpecKind {
    let cache_id = egui::Id::unique("spec-list-kinds");
    let meta = std::fs::metadata(path).ok();
    let stamp = (meta.as_ref().and_then(|m| m.modified().ok()), meta.as_ref().map_or(0, |m| m.len()));
    let cached: Option<(Option<SystemTime>, u64, SpecKind)> = ui.data(|d| {
        d.get_temp::<Arc<HashMap<PathBuf, (Option<SystemTime>, u64, SpecKind)>>>(cache_id)
            .and_then(|m| m.get(path).copied())
    });
    if let Some((t, n, k)) = cached
        && (t, n) == stamp
    {
        return k;
    }
    let text = if stamp.1 <= 512 * 1024 {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        let mut head = Vec::new();
        if let Ok(f) = std::fs::File::open(path) {
            let _ = f.take(8192).read_to_end(&mut head);
        }
        String::from_utf8_lossy(&head).into_owned()
    };
    let kind = SpecKind::detect(path, &text);
    ui.data_mut(|d| {
        let map = d.get_temp_mut_or_default::<Arc<HashMap<PathBuf, (Option<SystemTime>, u64, SpecKind)>>>(cache_id);
        Arc::make_mut(map).insert(path.to_path_buf(), (stamp.0, stamp.1, kind));
    });
    kind
}

enum ListAct {
    New(SpecKind),
    OpenFile,
    Open(PathBuf),
    Delete(PathBuf),
}

/// Where a "Delete…" asks for confirmation (the path to delete).
fn delete_id() -> egui::Id {
    egui::Id::unique("spec-list-delete")
}

/// The confirmation of a "Delete…" (from the sidebar or the tab), drawn
/// once per frame by whichever of them runs first.
fn delete_confirm(app: &mut TernaryWireApp, ui: &mut Ui) {
    let Some(path) = ui.data(|d| d.get_temp::<PathBuf>(delete_id())) else { return };
    let frame = ui.ctx().cumulative_frame_nr();
    let drawn_id = delete_id().with("frame");
    if ui.data(|d| d.get_temp::<u64>(drawn_id)) == Some(frame) {
        return;
    }
    ui.data_mut(|d| d.insert_temp(drawn_id, frame));
    let name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    match crate::flow_tab::confirm_dialog(
        ui.ctx(),
        "spec-delete-confirm",
        &trf("Delete “{}”?", &[&name]),
        tr("The file is removed from specs/. This cannot be undone."),
        tr("Delete file"),
    ) {
        Some(true) => {
            ui.data_mut(|d| d.remove_temp::<PathBuf>(delete_id()));
            if let Some(i) = app.tabs.iter().position(|t| matches!(&t.kind, TabKind::Spec(s) if s.path == path)) {
                app.close_tab(i);
            }
            if let Err(e) = std::fs::remove_file(&path) {
                app.toast(e.to_string(), true);
            }
        }
        Some(false) => {
            ui.data_mut(|d| d.remove_temp::<PathBuf>(delete_id()));
        }
        None => {}
    }
}

/// Creates `specs/openapi.yaml` (or `specs/model.smithy` when `smithy`),
/// numbered when it exists (`openapi-2.yaml`, …), from a template and opens
/// it. The "New OpenAPI spec" / "New Smithy model" of the APIs sidebar and
/// the title bar's New menu.
pub fn new_spec(app: &mut TernaryWireApp, smithy: bool) {
    let dir = app.ws.root().join("specs");
    let (stem, ext, text) =
        if smithy { ("model", "smithy", SMITHY_TEMPLATE) } else { ("openapi", "yaml", OPENAPI_TEMPLATE) };
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    match ternarywire_core::storage::write_atomic(&path, text.as_bytes()) {
        Ok(()) => open_spec(app, path),
        Err(e) => app.toast(e.to_string(), true),
    }
}

/// Opens a spec file picked in a file dialog.
pub fn open_spec_file(app: &mut TernaryWireApp) {
    if let Some(p) = rfd::FileDialog::new()
        .add_filter(tr("API specs"), &["yaml", "yml", "json", "proto", "graphql", "graphqls", "smithy"])
        .pick_file()
    {
        open_spec(app, p);
    }
}

/// The entries of the "New spec" menu: New OpenAPI spec, New Smithy model,
/// Open file….
pub fn new_spec_menu(app: &mut TernaryWireApp, ui: &mut Ui) {
    if kit::menu_item(ui, Some(icons::FILE_CODE), tr("New OpenAPI spec"), None).clicked() {
        new_spec(app, false);
    }
    if kit::menu_item(ui, Some(icons::TREE_STRUCTURE), tr("New Smithy model"), None).clicked() {
        new_spec(app, true);
    }
    kit::menu_sep(ui);
    if kit::menu_item(ui, Some(icons::FOLDER_OPEN), tr("Open file…"), None).clicked() {
        open_spec_file(app);
    }
}

/// The actions of the APIs sidebar header (the shell draws the header and
/// calls this in a right-to-left layout): "+" with the New spec menu.
pub fn sidebar_actions(app: &mut TernaryWireApp, ui: &mut Ui) {
    let add = kit::icon_button_sm(ui, icons::PLUS, tr("New spec"));
    egui::Popup::menu(&add).width(210.0).show(|ui| new_spec_menu(app, ui));
}

/// The APIs sidebar under its header: filter, the files of `specs/` and a
/// hint at the bottom.
pub fn sidebar_list(app: &mut TernaryWireApp, ui: &mut Ui) {
    let dark = ui.visuals().dark_mode;
    let root = app.ws.root().to_path_buf();
    let specs = list_specs(&root);
    let mut act: Option<ListAct> = None;
    if specs.is_empty() {
        kit::empty_state(
            ui,
            icons::FILE_CODE,
            tr("No API specs yet"),
            tr("Write OpenAPI or Smithy specs next to your collections and turn them into requests."),
            |ui| {
                if kit::primary_icon(ui, icons::PLUS, tr("New OpenAPI spec")).clicked() {
                    act = Some(ListAct::New(SpecKind::OpenApi));
                }
                ui.add_space(4.0);
                if kit::secondary(ui, Some(icons::TREE_STRUCTURE), tr("New Smithy model")).clicked() {
                    act = Some(ListAct::New(SpecKind::Smithy));
                }
                ui.add_space(2.0);
                if kit::ghost(ui, Some(icons::FOLDER_OPEN), tr("Open file…")).clicked() {
                    act = Some(ListAct::OpenFile);
                }
            },
        );
    } else {
        let filter_id = egui::Id::unique("spec-list-filter");
        let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
        ui.add_space(4.0);
        let r = kit::search_field_sm(ui, "spec-filter", &mut filter, tr("Filter specs"), ui.available_width());
        widgets::a11y_name(&r, tr("Filter specs"));
        ui.add_space(8.0);
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        let active = app.tabs.get(app.active).and_then(|t| match &t.kind {
            TabKind::Spec(s) => Some(s.path.clone()),
            _ => None,
        });
        let dirty: Vec<PathBuf> = app
            .tabs
            .iter()
            .filter_map(|t| match &t.kind {
                TabKind::Spec(s) if s.dirty() => Some(s.path.clone()),
                _ => None,
            })
            .collect();
        let q = filter.trim().to_lowercase();
        let foot = tr("Specs live in specs/ inside the workspace and can be committed to Git.");
        let list_h =
            (ui.available_height() - crate::sidebar::sidebar_footer_height(ui, foot, ui.available_width())).max(40.0);
        egui::ScrollArea::vertical().id_salt("spec-list").max_height(list_h).auto_shrink([false, false]).show(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut shown = 0;
                for path in &specs {
                    let name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    if !q.is_empty() && !name.to_lowercase().contains(&q) {
                        continue;
                    }
                    shown += 1;
                    let kind = list_kind(ui, path);
                    let popup_id = egui::Id::unique(("spec-row-menu", path));
                    let menu_open = crate::env_tab::row_menu_open(ui, popup_id);
                    let row = kit::TreeRow {
                        pad_left: 4.0,
                        icon: Some(kind.file_icon()),
                        icon_color: Some(kind.color(dark)),
                        label: &name,
                        meta: Some(kind.short()),
                        unsaved: dirty.contains(path),
                        selected: active.as_ref() == Some(path),
                        actions_width: 22.0,
                        force_actions: menu_open,
                        ..Default::default()
                    };
                    let (row, actions) = kit::tree_row(ui, row, |_| {});
                    widgets::a11y_button(&row, &name);
                    let row = row.on_hover_text(path.display().to_string());
                    if row.clicked() {
                        act = Some(ListAct::Open(path.clone()));
                    }
                    let mut menu = |ui: &mut Ui| {
                        if kit::menu_item(ui, Some(icons::FOLDER_OPEN), tr("Open"), None).clicked() {
                            act = Some(ListAct::Open(path.clone()));
                        }
                        if kit::menu_item(ui, Some(icons::COPY), tr("Copy file path"), None).clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                        }
                        kit::menu_sep(ui);
                        let delete = kit::MenuItem {
                            icon: Some(icons::TRASH),
                            label: tr("Delete…"),
                            danger: true,
                            ..Default::default()
                        };
                        if kit::menu_item_ex(ui, delete).clicked() {
                            act = Some(ListAct::Delete(path.clone()));
                        }
                    };
                    crate::env_tab::row_menu(ui, actions, popup_id, 200.0, &mut menu);
                    row.context_menu(&mut menu);
                }
                if shown == 0 {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(kit::hint(ui, &trf("No specs match “{}”", &[&filter.trim()])));
                    });
                }
                ui.add_space(8.0);
            },
        );
        crate::sidebar::sidebar_footer(ui, foot);
    }

    match act {
        None => {}
        Some(ListAct::New(kind)) => new_spec(app, kind == SpecKind::Smithy),
        Some(ListAct::OpenFile) => open_spec_file(app),
        Some(ListAct::Open(path)) => open_spec(app, path),
        Some(ListAct::Delete(path)) => {
            ui.data_mut(|d| d.insert_temp(delete_id(), path));
        }
    }
    delete_confirm(app, ui);
}

pub fn open_spec(app: &mut TernaryWireApp, path: PathBuf) {
    if let Some(i) = app.tabs.iter().position(|t| matches!(&t.kind, TabKind::Spec(s) if s.path == path)) {
        app.active = i;
        return;
    }
    match SpecTab::open(path) {
        Ok(tab) => {
            app.add_tab(TabKind::Spec(Box::new(tab)));
        }
        Err(e) => app.toast(e, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⌘S on a spec tab writes the spec file (it used to only flush the
    /// collections and still say "Saved").
    #[test]
    fn cmd_s_saves_the_open_spec() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(&dir.path().join("ws"));
        let path = dir.path().join("api.yaml");
        std::fs::write(&path, OPENAPI_TEMPLATE).unwrap();
        open_spec(&mut app, path.clone());
        let crate::app::TabKind::Spec(t) = &mut app.tabs[0].kind else { panic!("spec tab") };
        t.text.push_str("\n# edited\n");
        app.save_active();
        assert!(std::fs::read_to_string(&path).unwrap().ends_with("# edited\n"));
        let crate::app::TabKind::Spec(t) = &app.tabs[0].kind else { panic!("spec tab") };
        assert!(!t.dirty());
    }

    /// Unsaved spec edits survive a force-quit through the session file,
    /// like unsaved requests do.
    #[test]
    fn unsaved_spec_edits_are_restored_from_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(&dir.path().join("ws"));
        let path = dir.path().join("api.yaml");
        std::fs::write(&path, OPENAPI_TEMPLATE).unwrap();
        open_spec(&mut app, path.clone());
        let crate::app::TabKind::Spec(t) = &mut app.tabs[0].kind else { panic!("spec tab") };
        t.text.push_str("\n# not saved yet\n");
        crate::dialogs::save_session(&mut app);
        app.tabs.clear();
        crate::dialogs::restore_session(&mut app);
        let crate::app::TabKind::Spec(t) = &app.tabs[0].kind else { panic!("spec tab") };
        assert!(t.text.ends_with("# not saved yet\n"));
        assert!(t.dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), OPENAPI_TEMPLATE);
    }

    #[test]
    fn detects_and_validates() {
        assert_eq!(SpecKind::detect(Path::new("a.yaml"), OPENAPI_TEMPLATE), SpecKind::OpenApi);
        assert_eq!(SpecKind::detect(Path::new("a.proto"), ""), SpecKind::Protobuf);
        assert!(check_braces("message A { int32 x = 1; }\nservice S { }", "x").ok);
        assert!(!check_braces("message A { ", "x").ok);
        assert!(check_asyncapi("asyncapi: 2.6.0\ninfo: {title: t, version: '1'}\nchannels: {a: {}}").ok);
    }

    #[test]
    fn smithy_files_of_a_folder_form_one_model() {
        let dir = std::env::temp_dir().join(format!("ternarywire-spec-smithy-{}", std::process::id()));
        let specs = dir.join("specs");
        std::fs::create_dir_all(&specs).unwrap();
        let shop: [(&str, &str); 5] = [
            ("main.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/main.smithy")),
            ("operations.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/operations.smithy")),
            ("common.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/common.smithy")),
            ("traits.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/traits.smithy")),
            ("item.json", include_str!("../../ternarywire-convert/testdata/smithy-multi/item.json")),
        ];
        for (name, text) in shop {
            std::fs::write(specs.join(name), text).unwrap();
        }
        // Unrelated neighbours: another Smithy model and an OpenAPI spec.
        std::fs::write(specs.join("model.smithy"), SMITHY_TEMPLATE).unwrap();
        std::fs::write(specs.join("openapi.json"), "{\"openapi\": \"3.0.3\", \"paths\": {}}").unwrap();

        let mut tab = SpecTab::open(specs.join("operations.smithy")).unwrap();
        let check = tab.check.as_ref().unwrap();
        assert!(check.ok, "{} {:?}", check.summary, tab.smithy_model.as_ref().map(|m| &m.problems));
        assert_eq!(check.summary, "Smithy 2.0: 1 service, 3 operations in 5 files");
        let m = tab.smithy_model.as_ref().unwrap();
        assert_eq!(m.files, ["common.smithy", "item.json", "main.smithy", "traits.smithy"]);
        assert!(m.problems.is_empty(), "{:?}", m.problems);
        assert_eq!(tab.import().unwrap().request_count(), 3);

        // A sibling changed on disk is re-read; its findings are listed apart.
        let common = shop[2].1.replace("currency: Currency\n", "currency: Currencyy\n");
        std::fs::write(specs.join("common.smithy"), common).unwrap();
        tab.validate();
        let check = tab.check.as_ref().unwrap();
        assert!(!check.ok);
        assert!(check.warnings.is_empty(), "{:?}", check.warnings);
        let m = tab.smithy_model.as_ref().unwrap();
        assert_eq!(m.problems, ["common.smithy:13:5: error: unknown target `Currencyy` for member `currency`"]);
        // Diagnostics of the open file are its own.
        tab.text = tab.text.replace("errors: [NotFound]", "errors: [Missing]");
        tab.validate();
        let check = tab.check.as_ref().unwrap();
        assert_eq!(check.warnings, ["21:14: unknown error `Missing`"]);

        // The template model does not pull in the shop files.
        let other = SpecTab::open(specs.join("model.smithy")).unwrap();
        assert!(other.check.as_ref().unwrap().ok);
        assert!(other.smithy_model.as_ref().unwrap().files.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn smithy_build_json_declares_the_model() {
        let dir = std::env::temp_dir().join(format!("ternarywire-spec-smithy-build-{}", std::process::id()));
        let project = dir.join("specs").join("shop");
        let model = project.join("model");
        let shared = project.join("shared");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(dir.join("ternarywire.json"), "{}").unwrap();
        std::fs::write(
            model.join("main.smithy"),
            include_str!("../../ternarywire-convert/testdata/smithy-multi/main.smithy"),
        )
        .unwrap();
        std::fs::write(
            model.join("operations.smithy"),
            include_str!("../../ternarywire-convert/testdata/smithy-multi/operations.smithy"),
        )
        .unwrap();
        std::fs::write(
            model.join("item.json"),
            include_str!("../../ternarywire-convert/testdata/smithy-multi/item.json"),
        )
        .unwrap();
        std::fs::write(
            shared.join("common.smithy"),
            include_str!("../../ternarywire-convert/testdata/smithy-multi/common.smithy"),
        )
        .unwrap();
        std::fs::write(
            shared.join("traits.smithy"),
            include_str!("../../ternarywire-convert/testdata/smithy-multi/traits.smithy"),
        )
        .unwrap();
        // Not a source: ignored even though it is next to the build file.
        std::fs::write(project.join("scratch.smithy"), "namespace other\nstring Broken {").unwrap();
        std::fs::write(
            project.join("smithy-build.json"),
            r#"{"version": "1.0", "sources": ["model"], "imports": ["shared"]}"#,
        )
        .unwrap();

        let mut tab = SpecTab::open(model.join("operations.smithy")).unwrap();
        let check = tab.check.as_ref().unwrap();
        assert!(check.ok, "{} {:?}", check.summary, tab.smithy_model.as_ref().map(|m| &m.problems));
        assert_eq!(check.summary, "Smithy 2.0: 1 service, 3 operations in 5 files");
        let m = tab.smithy_model.as_ref().unwrap();
        assert_eq!(m.files, ["model/item.json", "model/main.smithy", "shared/common.smithy", "shared/traits.smithy"]);
        assert_eq!(tab.import().unwrap().request_count(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn spec_tab<'a>(app: &'a mut TernaryWireApp, name: &str) -> &'a mut SpecTab {
        app.tabs
            .iter_mut()
            .find_map(|t| match &mut t.kind {
                crate::app::TabKind::Spec(s) if s.path.ends_with(name) => Some(s.as_mut()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no tab for {name}"))
    }

    /// The other files of a Smithy model that are open in other tabs with
    /// unsaved changes are used as the tabs show them, not as they are on
    /// disk (validation, preview and import).
    #[test]
    fn smithy_model_uses_unsaved_text_of_other_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = TernaryWireApp::for_test(&dir.path().join("ws"));
        let specs = app.ws.root().join("specs");
        std::fs::create_dir_all(&specs).unwrap();
        let shop: [(&str, &str); 5] = [
            ("main.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/main.smithy")),
            ("operations.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/operations.smithy")),
            ("common.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/common.smithy")),
            ("traits.smithy", include_str!("../../ternarywire-convert/testdata/smithy-multi/traits.smithy")),
            ("item.json", include_str!("../../ternarywire-convert/testdata/smithy-multi/item.json")),
        ];
        for (name, text) in shop {
            std::fs::write(specs.join(name), text).unwrap();
        }
        open_spec(&mut app, specs.join("main.smithy"));
        open_spec(&mut app, specs.join("common.smithy"));
        open_spec(&mut app, specs.join("operations.smithy"));
        let main = spec_tab(&mut app, "main.smithy");
        main.text = main.text.replace("[GetItem, ListItems, PutPrice]", "[GetItem, ListItems]");
        let common = spec_tab(&mut app, "common.smithy");
        common.text = common.text.replace("currency: Currency\n", "currency: Currencyy\n");
        app.active =
            app.tabs.iter().position(|t| crate::dialogs::tab_title(t, &app.ws).0.contains("operations")).unwrap();
        app.draw_frame(Vec::new());
        let ops = spec_tab(&mut app, "operations.smithy");
        let m = ops.smithy_model.as_ref().unwrap();
        assert_eq!(m.problems, ["common.smithy:13:5: error: unknown target `Currencyy` for member `currency`"]);
        assert_eq!(ops.import().unwrap().request_count(), 2);
        assert_eq!(std::fs::read_to_string(specs.join("main.smithy")).unwrap(), shop[0].1);

        // Closing the other tabs without saving: back to the files.
        while app.tabs.len() > 1 {
            let other = app.tabs.iter().position(|t| !crate::dialogs::tab_title(t, &app.ws).0.contains("operations"));
            app.close_tab(other.unwrap());
        }
        app.draw_frame(Vec::new());
        let ops = spec_tab(&mut app, "operations.smithy");
        assert!(ops.check.as_ref().unwrap().ok, "{:?}", ops.smithy_model.as_ref().map(|m| &m.problems));
        assert!(ops.smithy_model.as_ref().unwrap().problems.is_empty());
        assert_eq!(ops.import().unwrap().request_count(), 3);
    }

    /// A jar (zip) with uncompressed entries.
    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let crc32 = |data: &[u8]| {
            let mut crc = !0u32;
            for b in data {
                crc ^= u32::from(*b);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            !crc
        };
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data) in entries {
            let mut sizes = Vec::new();
            sizes.extend(crc32(data).to_le_bytes());
            sizes.extend((data.len() as u32).to_le_bytes());
            sizes.extend((data.len() as u32).to_le_bytes());
            sizes.extend((name.len() as u16).to_le_bytes());
            let offset = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend(&sizes);
            out.extend([0; 2]);
            out.extend(name.as_bytes());
            out.extend(*data);
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend(&sizes);
            central.extend([0; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let (cd_offset, cd_size) = (out.len() as u32, central.len() as u32);
        out.extend(central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0; 4]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend(cd_size.to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend([0; 2]);
        out
    }

    /// With a `smithy-build.json`, the models of its Maven dependencies (from
    /// jars in the local repository) are assembled after the project's own
    /// files, missing jars are listed as problems, and the jars are read
    /// again only when the build file changes.
    #[test]
    fn smithy_build_json_maven_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("m2");
        let jar_dir = repo.join("com/example/shapes/1.0.0");
        std::fs::create_dir_all(&jar_dir).unwrap();
        std::fs::write(
            jar_dir.join("shapes-1.0.0.jar"),
            jar(&[
                ("META-INF/smithy/manifest", b"shapes.smithy\n"),
                ("META-INF/smithy/shapes.smithy", b"$version: \"2\"\nnamespace com.example.shared\nstring Id\n"),
            ]),
        )
        .unwrap();
        let project = dir.path().join("ws").join("specs");
        std::fs::create_dir_all(project.join("model")).unwrap();
        std::fs::write(dir.path().join("ws").join("ternarywire.json"), "{}").unwrap();
        let main =
            "$version: \"2\"\nnamespace ex.app\n\nuse com.example.shared#Id\n\nstructure Thing {\n    id: Id\n}\n";
        std::fs::write(project.join("model/main.smithy"), main).unwrap();
        std::fs::write(project.join("model/more.smithy"), "$version: \"2\"\nnamespace ex.app\nstring Name\n").unwrap();
        let build = r#"{"version": "1.0", "sources": ["model"],
            "maven": {"dependencies": ["com.example:shapes:1.0.0", "com.example:missing:2.0"]}}"#;
        std::fs::write(project.join("smithy-build.json"), build).unwrap();

        let mut tab = SpecTab::open_with(project.join("model/main.smithy"), Some(repo.clone())).unwrap();
        let check = tab.check.as_ref().unwrap();
        assert!(check.warnings.is_empty(), "{:?}", check.warnings);
        let m = tab.smithy_model.as_ref().unwrap();
        assert_eq!(m.files, ["model/more.smithy", "shapes-1.0.0.jar!/META-INF/smithy/shapes.smithy"]);
        assert_eq!(m.problems.len(), 1, "{:?}", m.problems);
        assert!(m.problems[0].contains("com.example:missing:2.0"), "{:?}", m.problems);

        // Cached: the jar is not read again while smithy-build.json is unchanged.
        std::fs::remove_dir_all(&repo).unwrap();
        tab.validate();
        assert!(tab.check.as_ref().unwrap().warnings.is_empty());
        // A changed build file is read again (now the jar is gone too).
        std::fs::write(project.join("smithy-build.json"), build.replace("\"1.0\"", "\"1.0\" ")).unwrap();
        tab.validate();
        let check = tab.check.as_ref().unwrap();
        assert!(check.warnings.iter().any(|w| w.contains("com.example.shared#Id")), "{:?}", check.warnings);
        assert_eq!(tab.smithy_model.as_ref().unwrap().problems.len(), 2);
    }

    /// Problems carry the line and column the editor jumps to.
    #[test]
    fn problems_have_positions() {
        let p = Problem::parse(Level::Warning, "21:14: unknown error `Missing`");
        assert_eq!((p.line, p.col, p.message.as_str()), (Some(21), Some(14), "unknown error `Missing`"));
        let yaml = "invalid YAML: did not find expected key at line 12 column 6, while parsing a block mapping at line 7 column 5";
        let p = Problem::parse(Level::Error, yaml);
        assert_eq!((p.line, p.col), (Some(12), Some(6)));
        assert_eq!(locate("unexpected `}` on line 3"), (Some(3), None));
        assert_eq!(locate("missing `info`"), (None, None));
        let check = check_braces("message A {\n}\n}", "x");
        assert_eq!(check.problems[0].line, Some(3));
        assert!(Check::new(true, "ok".into(), Vec::new()).problems.is_empty());

        // The cursor lands on the line, selected from the column.
        let ctx = egui::Context::default();
        let id = egui::Id::unique("editor");
        select_line(&ctx, id, "a\nbcdef\ng", 2, 3);
        let range = egui::text_edit::TextEditState::load(&ctx, id).unwrap().cursor.char_range().unwrap();
        assert_eq!((range.secondary.index.0, range.primary.index.0), (4, 7));
        assert_eq!(docs_path("{{baseUrl}}/books/:id?x=1"), "/books/:id");
        assert_eq!(docs_path("https://api.example.com/v1/items"), "/v1/items");
    }

    #[test]
    fn sync_updates_and_adds() {
        let (spec, _) = ternarywire_convert::openapi::import(OPENAPI_TEMPLATE).unwrap();
        let mut target = Collection::new("Mine");
        let mut r = RequestItem::new("Users", ternarywire_core::model::HttpRequest::new("GET", "{{baseUrl}}/users"));
        r.scripts.post_response = "pm.test('x', () => {});".into();
        target.items.push(Item::Request(r));
        let report = sync_collection(&mut target, &spec);
        assert!(report.starts_with("1 updated, 0 added"), "{report}");
        let users = &target.requests()[0];
        assert!(users.http().unwrap().params.iter().any(|p| p.key == "page"));
        assert!(!users.scripts.post_response.is_empty(), "user scripts are kept");
    }
}
