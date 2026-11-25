//! Regression tests covering security, correctness, and DoS hardening fixes
//! across `ternarywire-core`.

use std::time::Instant;

#[cfg(unix)]
use indexmap::IndexMap;
use serde_json::json;

use ternarywire_core::auth::{Auth, AuthType};
use ternarywire_core::environment::Environment;
use ternarywire_core::fork::{self, ChangeKind};
#[cfg(unix)]
use ternarywire_core::history::{HistoryEntry, HistoryStore};
use ternarywire_core::jsonpath;
use ternarywire_core::model::{BodyMode, Collection, Folder, FormField, HttpRequest, Item, KeyValue, RequestItem};
use ternarywire_core::postman::{self, ExportOptions};
use ternarywire_core::prepared::{PrepareContext, PreparedBody, prepare};
use ternarywire_core::protocols::{McpRequest, McpTransport, SocketIoArg};
use ternarywire_core::response::BodyLanguage;
use ternarywire_core::script::protocol as proto;
use ternarywire_core::secrets::SecretStore;
#[cfg(unix)]
use ternarywire_core::secrets::{FileBackend, SecretBackend};
use ternarywire_core::settings::{ClientCertificate, RequestDefaults};
use ternarywire_core::storage::WorkspaceStore;
use ternarywire_core::urlutil;
use ternarywire_core::variables::{Resolver, Variable, VariableScopes};

#[test]
fn apply_mcp_cannot_overwrite_stdio_command_args_env_or_cwd() {
    let mut req = McpRequest {
        transport: McpTransport::Stdio {
            command: "npx".into(),
            args: vec!["-y".into(), "safe-server".into()],
            env: vec![KeyValue::new("SAFE", "1")],
            cwd: "/safe".into(),
        },
        ..Default::default()
    };
    let attack = json!({
        "transport": "stdio",
        "command": "/bin/sh",
        "args": ["-c", "id"],
        "env": [{"key": "LD_PRELOAD", "value": "/tmp/evil.so", "enabled": true}],
        "cwd": "/tmp"
    });
    proto::apply_mcp(&mut req, None, &attack);
    let McpTransport::Stdio { command, args, env, cwd } = req.transport else {
        panic!("expected stdio");
    };
    assert_eq!(command, "npx");
    assert_eq!(args, vec!["-y", "safe-server"]);
    assert_eq!(env.len(), 1);
    assert_eq!(env[0].key, "SAFE");
    assert_eq!(cwd, "/safe");
}

#[cfg(unix)]
#[test]
fn workspace_store_rejects_external_symlinks_on_write_and_load() {
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim.txt");
    std::fs::write(&victim, "DO NOT TOUCH").unwrap();

    let ws_dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::memory();
    let (store, _) = WorkspaceStore::create(ws_dir.path(), "SymlinkTest").unwrap();

    // Plant a symlink inside globals.json pointing outside the workspace.
    let globals_path = ws_dir.path().join("globals.json");
    let _ = std::fs::remove_file(&globals_path);
    std::os::unix::fs::symlink(&victim, &globals_path).unwrap();

    // Saving globals must refuse to follow or clobber the outside target.
    assert!(store.save_globals(&[Variable::new("a", "1")], &secrets).is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "DO NOT TOUCH");

    // Plant an escaping directory symlink under collections/.
    let col_dir = ws_dir.path().join("collections/escape");
    std::fs::create_dir_all(col_dir.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(outside.path(), &col_dir).unwrap();
    let (_, loaded) = WorkspaceStore::open(ws_dir.path(), &secrets).unwrap();
    assert!(loaded.collections.is_empty());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "DO NOT TOUCH");
}

#[cfg(unix)]
#[test]
fn file_backend_enforces_0600_and_refuses_symlinks() {
    use std::os::unix::fs::PermissionsExt;

    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim.json");
    std::fs::write(&victim, "keep").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secrets.json");
    let backend = FileBackend { path: path.clone() };
    let map = IndexMap::from([("k".to_string(), "v".to_string())]);
    backend.save(&map).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    // Replace with symlink to outside file -> must refuse.
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    assert!(backend.save(&map).is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
}

#[test]
fn auth_scrub_secrets_and_storage_strip_non_string_sensitive_params() {
    let mut auth = Auth::new(AuthType::Bearer).with("token", json!(123456)).with("note", json!("ok"));
    auth.scrub_secrets();
    assert_eq!(auth.params.get("token"), Some(&json!("")));
    assert_eq!(auth.params.get("note"), Some(&json!("ok")));

    let dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::memory();
    let (store, _) = WorkspaceStore::create(dir.path(), "NonStr").unwrap();
    let mut c = Collection::new("API");
    c.auth = Auth::new(AuthType::ApiKey).with("key", "X-Key").with("value", json!(999999));
    store.save_collection(&c, &secrets).unwrap();
    let on_disk = std::fs::read_to_string(dir.path().join("collections/api/collection.json")).unwrap();
    assert!(!on_disk.contains("999999"), "{on_disk}");
}

#[test]
fn fork_merge_preserves_unpulled_parent_changes_and_moved_children_and_secrets() {
    let mut parent = Collection::new("API");
    let mut f_old = Folder::new("Old");
    f_old.items.push(Item::Request(RequestItem::new("Child", HttpRequest::new("GET", "https://h/c"))));
    let f_new = Folder::new("New");
    parent.items.push(Item::Folder(f_old));
    parent.items.push(Item::Folder(f_new));
    parent.items.push(Item::Request(RequestItem::new("Other", HttpRequest::new("GET", "https://h/o"))));
    parent.variables.push(Variable::secret("tok", "parent-secret"));
    parent.variables.push(Variable { current: Some("local-override".into()), ..Variable::new("host", "prod.example") });

    let (mut fork_c, mut link) = fork::create_fork(&parent, "Fork", 1);

    // Unpulled edit on parent: Other changes description.
    if let Item::Request(r) = &mut parent.items[2] {
        r.description = "edited on parent".into();
    }
    // Fork moves Child from Old -> New and deletes Old.
    let moved = match &mut fork_c.items[0] {
        Item::Folder(f) => f.items.remove(0),
        _ => panic!(),
    };
    if let Item::Folder(f) = &mut fork_c.items[1] {
        f.items.push(moved);
    }
    fork_c.items.remove(0);

    let plan = fork::plan_merge(&link, &parent, &fork_c);
    assert!(plan.entries.iter().any(|e| e.name == "Old" && e.kind == ChangeKind::Removed));
    assert!(plan.entries.iter().any(|e| e.name == "Child" && e.kind == ChangeKind::Modified));
    fork::apply_merge(&mut link, &plan, &fork_c, &mut parent);

    // Child survived removal of Old and now lives under New!
    assert!(parent.find_request_by_name("Child").is_some(), "moved child was lost");
    // Target secret and current value survived!
    assert_eq!(parent.variables[0].value, "parent-secret");
    assert_eq!(parent.variables[1].current.as_deref(), Some("local-override"));
    // Unpulled parent edit to Other is NOT falsely reported as a fork change on the next merge.
    let next_merge = fork::plan_merge(&link, &parent, &fork_c);
    assert!(
        !next_merge.entries.iter().any(|e| e.name == "Other"),
        "unpulled parent change leaked into next merge: {next_merge:?}"
    );
    // And pull still offers Other to the fork.
    let next_pull = fork::plan_pull(&link, &parent, &fork_c);
    assert!(next_pull.entries.iter().any(|e| e.name == "Other" && e.kind == ChangeKind::Modified));
}

#[test]
fn fork_line_diff_large_inputs_complete_quickly() {
    let a = (0..3000).map(|i| format!("line-{i}")).collect::<Vec<_>>().join("\n");
    let mut b_lines: Vec<String> = (0..3000).map(|i| format!("line-{i}")).collect();
    b_lines[1500] = "CHANGED".into();
    let b = b_lines.join("\n");
    let start = Instant::now();
    let diff = fork::line_diff(&a, &b);
    assert!(start.elapsed().as_secs() < 2, "line_diff took {:?}", start.elapsed());
    assert!(diff.iter().any(|(op, l)| *op == fork::DiffOp::Added && *l == "CHANGED"));
}

#[test]
fn jsonpath_adversarial_inputs_do_not_panic_overflow_or_hang() {
    let data = json!([0, 1, 2, 3, 4]);
    // Extreme positive/negative slice step and bounds.
    let _ = jsonpath::query(&data, "$[1::9223372036854775807]").unwrap();
    let _ = jsonpath::query(&data, "$[0:5:-9223372036854775808]").unwrap();
    // Negating i64::MIN in script expression must error cleanly, never overflow.
    assert!(jsonpath::query(&data, "$[(@.length - -9223372036854775808)]").is_err());

    // Deeply nested brackets, negations, and regex groups hit depth limit instead of stack overflow.
    let deep_parens = format!("$[?({}1{})]", "(".repeat(2000), ")".repeat(2000));
    assert!(jsonpath::query(&data, &deep_parens).is_err());
    let deep_not = format!("$[?({}@.a)]", "!".repeat(2000));
    assert!(jsonpath::query(&data, &deep_not).is_err());
    let deep_regex = format!("$[?(@ =~ /{}a{}/)]", "(".repeat(2000), ")".repeat(2000));
    assert!(jsonpath::query(&json!(["a"]), &deep_regex).is_err());

    // Zero-width nested quantifier must terminate without infinite loop.
    let samples = json!(["", "aaa", "b"]);
    let matches = jsonpath::query(&samples, "$[?(@ =~ /^(a*)+$/)]").unwrap();
    assert_eq!(matches.len(), 2);
}

#[test]
fn variables_billion_laughs_dag_is_bounded() {
    let mut scopes = VariableScopes::default();
    scopes.environment.insert("v0".into(), "A".repeat(128));
    for i in 1..=10 {
        let prev = format!("{{{{v{}}}}}", i - 1);
        scopes.environment.insert(format!("v{i}"), prev.repeat(10));
    }
    let start = Instant::now();
    let out = Resolver::new(&scopes).resolve("{{v10}}");
    assert!(start.elapsed().as_secs() < 2, "resolution took {:?}", start.elapsed());
    assert!(out.len() <= 16 * 1024 * 1024);
}

#[test]
fn postman_import_rejects_reserved_and_duplicate_ids_and_export_handles_secrets_and_urls() {
    let bad = json!({
        "info": {
            "_postman_id": "globals",
            "name": "Malicious",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": [
            { "id": "dup:id", "name": "A", "request": { "method": "GET", "url": "https://h/a" } },
            { "id": "dup:id", "name": "B", "request": { "method": "GET", "url": "https://h/b" } },
            { "id": "same-valid-id", "name": "C", "request": { "method": "GET", "url": "https://h/c" } },
            { "id": "same-valid-id", "name": "D", "request": { "method": "GET", "url": "https://h/d" } }
        ]
    });
    let (col, _) = postman::import_collection(&bad).unwrap();
    assert_ne!(col.id, "globals");
    let ids: Vec<String> = col.items.iter().map(|it| it.id().to_string()).collect();
    assert!(ids.iter().all(|id| !id.contains(':')));
    let uniq: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(uniq.len(), ids.len(), "duplicate IDs survived import: {ids:?}");

    // Export redacts numeric auth secrets, preserves userinfo without misparsing port, and handles valueless query flags.
    let mut c = Collection::new("ExportTest");
    c.auth = Auth::new(AuthType::Bearer).with("token", json!(424242));
    let mut req = HttpRequest::new("GET", "https://user:pass@example.com/p?flag&k=v");
    req.params = urlutil::parse_query("flag&k=v");
    c.items.push(Item::Request(RequestItem::new("R", req)));
    let (exported, report) =
        postman::export_collection(&c, ExportOptions { include_secrets: false, ..Default::default() });
    assert!(report.redacted >= 1);
    assert!(!exported.to_string().contains("424242"));
    let url = &exported["item"][0]["request"]["url"];
    assert!(url.get("port").is_none(), "userinfo colon misparsed as port: {url}");
    assert_eq!(url["host"], json!(["user:pass@example", "com"]), "{url}");
    let q0 = &url["query"][0];
    assert_eq!(q0["key"], json!("flag"));
    assert_eq!(q0["value"], json!(null));
}

#[cfg(unix)]
#[test]
fn history_store_refuses_symlinks_and_creates_0600() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.jsonl");
    let store = HistoryStore::new(&path, 10);
    let entry = HistoryEntry {
        id: "e1".into(),
        timestamp: 0,
        request: HttpRequest::new("GET", "https://h/1"),
        resolved_url: "https://h/1".into(),
        status: Some(200),
        elapsed_ms: 1.0,
        size: 1,
        error: None,
        source_request_id: None,
        name: None,
    };
    store.append(&entry).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim");
    std::fs::write(&victim, "untouched").unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    assert!(store.append(&entry).is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn client_certificate_matches_ports_ipv6_and_rejects_malformed() {
    let with_port = ClientCertificate { host: "api.example.com:8443".into(), ..Default::default() };
    assert!(with_port.matches("api.example.com", Some(8443)));
    assert!(!with_port.matches("api.example.com", None));
    assert!(!with_port.matches("api.example.com", Some(443)));

    let v6_port = ClientCertificate { host: "[::1]:8443".into(), ..Default::default() };
    assert!(v6_port.matches("::1", Some(8443)));
    assert!(v6_port.matches("[::1]", Some(8443)));
    assert!(!v6_port.matches("::1", None));

    let bad_port = ClientCertificate { host: "api.example.com:99999".into(), ..Default::default() };
    assert!(!bad_port.matches("api.example.com", Some(443)));

    let empty_wild = ClientCertificate { host: "*.".into(), ..Default::default() };
    assert!(!empty_wild.matches("example.com", Some(443)));
}

#[test]
fn environment_set_current_targets_enabled_variable() {
    let mut env = Environment::new("Dev");
    env.variables.push(Variable { enabled: false, ..Variable::new("tok", "disabled") });
    env.variables.push(Variable::new("tok", "enabled"));
    env.set_current("tok", "updated");
    assert_eq!(env.variables[0].current, None);
    assert_eq!(env.variables[1].current.as_deref(), Some("updated"));
}

#[test]
fn urlutil_response_prepared_and_protocols_edge_cases() {
    // Brace-aware `#` split and schemeless `://` path detection.
    assert_eq!(urlutil::split("https://h/{{a#b}}?q=1#frag"), ("https://h/{{a#b}}", Some("q=1"), "#frag"));
    assert_eq!(urlutil::path_var_names("example.com/a/:id/http://other"), vec!["id"]);

    // Truncated multi-byte UTF-8 at 64-byte sniff boundary still detects Text.
    let mut bytes = vec![b'a'; 63];
    bytes.extend_from_slice("é".as_bytes()); // 2-byte char straddling byte 64
    assert_eq!(BodyLanguage::detect("", &bytes), BodyLanguage::Text);

    // FormData file field skips empty resolved paths.
    let mut req = HttpRequest::new("POST", "https://h/upload");
    req.body.mode = BodyMode::FormData;
    req.body.form_data.push(FormField::file("f", "{{empty}}"));
    let mut scopes = VariableScopes::default();
    scopes.environment.insert("empty".into(), "   ".into());
    let auth = Auth::none();
    let defaults = RequestDefaults::default();
    let ctx = PrepareContext {
        resolver: Resolver::new(&scopes),
        auth: &auth,
        auth_owner_id: None,
        defaults: &defaults,
        base_dir: None,
    };
    let prep = prepare(&req, &ctx);
    match prep.body {
        PreparedBody::Multipart(parts) => assert!(parts.is_empty(), "empty file path should be skipped: {parts:?}"),
        other => panic!("expected multipart, got {other:?}"),
    }

    // Uppercase 0X hex prefix in SocketIoArg::decode_binary.
    assert_eq!(SocketIoArg::decode_binary("0XDEADBEEF"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
}

trait FindByName {
    fn find_request_by_name(&self, name: &str) -> Option<&RequestItem>;
}

impl FindByName for Collection {
    fn find_request_by_name(&self, name: &str) -> Option<&RequestItem> {
        fn walk<'a>(items: &'a [Item], name: &str) -> Option<&'a RequestItem> {
            for it in items {
                match it {
                    Item::Request(r) if r.name == name => return Some(r),
                    Item::Folder(f) => {
                        if let Some(found) = walk(&f.items, name) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        walk(&self.items, name)
    }
}
