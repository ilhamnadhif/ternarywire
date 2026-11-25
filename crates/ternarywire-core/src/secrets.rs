//! Secret storage: vault entries, secret variable values, sensitive auth
//! parameters and OAuth tokens. None of these are written to workspace files.
//!
//! All secrets of a workspace are kept as one JSON map in a single backend
//! entry (one Keychain item per workspace), so opening a workspace costs at
//! most one Keychain prompt.

use std::path::PathBuf;

use indexmap::IndexMap;
use parking_lot::{Mutex, RwLock};

use crate::tokens::{OAuth2Token, TokenStore};
use crate::variables::VaultLookup;

pub trait SecretBackend: Send + Sync {
    fn load(&self) -> Result<IndexMap<String, String>, String>;
    fn save(&self, secrets: &IndexMap<String, String>) -> Result<(), String>;
    fn describe(&self) -> String;
}

/// Keeps secrets only for the lifetime of the process.
#[derive(Default)]
pub struct MemoryBackend;

impl SecretBackend for MemoryBackend {
    fn load(&self) -> Result<IndexMap<String, String>, String> {
        Ok(IndexMap::new())
    }

    fn save(&self, _: &IndexMap<String, String>) -> Result<(), String> {
        Ok(())
    }

    fn describe(&self) -> String {
        "memory".into()
    }
}

/// Plain JSON file, used when no OS keychain is available (CI, headless
/// Linux). The file lives in the workspace's git-ignored `.ternarywire/` folder.
pub struct FileBackend {
    pub path: PathBuf,
}

impl SecretBackend for FileBackend {
    fn load(&self) -> Result<IndexMap<String, String>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(IndexMap::new()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn save(&self, secrets: &IndexMap<String, String>) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_vec_pretty(secrets).map_err(|e| e.to_string())?;
        crate::storage::write_atomic_private(&self.path, &data).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn describe(&self) -> String {
        format!("file {}", self.path.display())
    }
}

/// In-memory view over a backend. Reads are served from memory; writes are
/// persisted immediately.
pub struct SecretStore {
    backend: Box<dyn SecretBackend>,
    map: RwLock<IndexMap<String, String>>,
    load_error: Option<String>,
    last_save_error: RwLock<Option<String>>,
    /// Held from snapshot to backend write, so that of two concurrent
    /// writers the older snapshot can never be saved last.
    persist_lock: Mutex<()>,
}

impl Default for SecretStore {
    fn default() -> Self {
        SecretStore::new(Box::new(MemoryBackend))
    }
}

pub const VAULT_KEY_PREFIX: &str = "vault:";
pub const OAUTH_KEY_PREFIX: &str = "oauth2:";

impl SecretStore {
    pub fn new(backend: Box<dyn SecretBackend>) -> Self {
        let (map, load_error) = match backend.load() {
            Ok(m) => (m, None),
            Err(e) => {
                tracing::warn!("could not load secrets from {}: {e}", backend.describe());
                (IndexMap::new(), Some(e))
            }
        };
        SecretStore {
            backend,
            map: RwLock::new(map),
            load_error,
            last_save_error: RwLock::new(None),
            persist_lock: Mutex::new(()),
        }
    }

    pub fn memory() -> Self {
        SecretStore::default()
    }

    pub fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }

    pub fn last_save_error(&self) -> Option<String> {
        self.last_save_error.read().clone()
    }

    pub fn backend_description(&self) -> String {
        self.backend.describe()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.read().get(key).cloned()
    }

    pub fn set(&self, key: &str, value: &str) {
        let changed = {
            let mut map = self.map.write();
            if map.get(key).map(String::as_str) == Some(value) {
                false
            } else {
                map.insert(key.to_string(), value.to_string());
                true
            }
        };
        if changed {
            self.persist();
        }
    }

    pub fn remove(&self, key: &str) {
        let removed = self.map.write().shift_remove(key).is_some();
        if removed {
            self.persist();
        }
    }

    /// Applies many changes with a single backend write.
    pub fn update(&self, f: impl FnOnce(&mut IndexMap<String, String>)) {
        let changed = {
            let mut map = self.map.write();
            let before = map.clone();
            f(&mut map);
            *map != before
        };
        if changed {
            self.persist();
        }
    }

    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.map.read().keys().filter(|k| k.starts_with(prefix)).cloned().collect()
    }

    fn persist(&self) {
        if let Some(e) = &self.load_error {
            // The backend holds secrets that could not be read; saving the
            // in-memory map would replace all of them. Changes stay in memory.
            tracing::error!("not saving secrets to {}: they could not be loaded ({e})", self.backend.describe());
            return;
        }
        let _guard = self.persist_lock.lock();
        let snapshot = self.map.read().clone();
        match self.backend.save(&snapshot) {
            Ok(()) => {
                *self.last_save_error.write() = None;
            }
            Err(e) => {
                tracing::error!("could not save secrets to {}: {e}", self.backend.describe());
                *self.last_save_error.write() = Some(e);
            }
        }
    }

    // Vault helpers: names are visible to the user, values are not.

    pub fn vault_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .keys_with_prefix(VAULT_KEY_PREFIX)
            .into_iter()
            .map(|k| k[VAULT_KEY_PREFIX.len()..].to_string())
            .collect();
        names.sort();
        names
    }

    pub fn vault_get(&self, name: &str) -> Option<String> {
        self.get(&format!("{VAULT_KEY_PREFIX}{name}"))
    }

    pub fn vault_set(&self, name: &str, value: &str) {
        self.set(&format!("{VAULT_KEY_PREFIX}{name}"), value)
    }

    pub fn vault_remove(&self, name: &str) {
        self.remove(&format!("{VAULT_KEY_PREFIX}{name}"))
    }
}

impl VaultLookup for SecretStore {
    fn get(&self, key: &str) -> Option<String> {
        self.vault_get(key)
    }
}

/// Vault names of the app settings' own secrets (proxy password, client
/// certificate passphrases): `{{vault:settings.<name>}}` in `settings.json`.
pub const SETTINGS_VAULT_PREFIX: &str = "settings.";

/// A vault that looks up `settings.` names in the store of the app
/// settings' secrets and every other name in `inner` (the workspace vault).
pub struct SettingsVault {
    pub settings: std::sync::Arc<SecretStore>,
    pub inner: Option<std::sync::Arc<dyn VaultLookup>>,
}

impl VaultLookup for SettingsVault {
    fn get(&self, key: &str) -> Option<String> {
        match key.strip_prefix(SETTINGS_VAULT_PREFIX) {
            Some(name) => self.settings.vault_get(name),
            None => self.inner.as_ref()?.get(key),
        }
    }
}

impl TokenStore for SecretStore {
    fn get(&self, owner_id: &str) -> Option<OAuth2Token> {
        SecretStore::get(self, &format!("{OAUTH_KEY_PREFIX}{owner_id}")).and_then(|s| serde_json::from_str(&s).ok())
    }

    fn set(&self, owner_id: &str, token: Option<OAuth2Token>) {
        let key = format!("{OAUTH_KEY_PREFIX}{owner_id}");
        match token {
            Some(t) => SecretStore::set(self, &key, &serde_json::to_string(&t).unwrap_or_default()),
            None => self.remove(&key),
        }
    }
}

/// Maximum UTF-16 code units per OS keychain credential entry.
/// Windows Credential Manager enforces `CRED_MAX_CREDENTIAL_BLOB_SIZE`
/// (2,560 bytes = 1,280 UTF-16 code units), so payloads larger than this
/// threshold are transparently split across numbered entries (`{account}#chunk:{i}`).
#[cfg(any(feature = "keychain", test))]
const KEYCHAIN_CHUNK_UNITS: usize = 1_000;
#[cfg(any(feature = "keychain", test))]
const CHUNK_MANIFEST_KEY: &str = "__tw_chunks";
#[cfg(any(feature = "keychain", test))]
const MAX_KEYCHAIN_CHUNKS: usize = 10_000;

#[cfg(any(feature = "keychain", test))]
fn split_utf16_chunks(s: &str, max_units: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut units = 0;
    for (idx, ch) in s.char_indices() {
        let w = ch.len_utf16();
        if units + w > max_units && idx > start {
            chunks.push(&s[start..idx]);
            start = idx;
            units = 0;
        }
        units += w;
    }
    if start < s.len() || chunks.is_empty() {
        chunks.push(&s[start..]);
    }
    chunks
}

#[cfg(any(feature = "keychain", test))]
fn chunk_manifest_count(json: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let o = v.as_object()?;
    if o.len() != 1 {
        return None;
    }
    let n = o.get(CHUNK_MANIFEST_KEY)?.as_u64()?;
    usize::try_from(n).ok().filter(|&n| (1..=MAX_KEYCHAIN_CHUNKS).contains(&n))
}

#[cfg(any(feature = "keychain", test))]
fn load_chunked<F>(mut get: F) -> Result<IndexMap<String, String>, String>
where
    F: FnMut(Option<usize>) -> Result<Option<String>, String>,
{
    let Some(primary) = get(None)? else {
        return Ok(IndexMap::new());
    };
    if let Some(count) = chunk_manifest_count(&primary) {
        let mut full = String::new();
        for i in 0..count {
            let part = get(Some(i))?.ok_or_else(|| format!("missing keychain chunk {i} of {count}"))?;
            full.push_str(&part);
        }
        return serde_json::from_str(&full).map_err(|e| e.to_string());
    }
    serde_json::from_str(&primary).map_err(|e| e.to_string())
}

#[cfg(any(feature = "keychain", test))]
fn save_chunked<G, S, D>(
    secrets: &IndexMap<String, String>,
    mut get_primary: G,
    mut set: S,
    mut del: D,
) -> Result<(), String>
where
    G: FnMut() -> Result<Option<String>, String>,
    S: FnMut(Option<usize>, &str) -> Result<(), String>,
    D: FnMut(Option<usize>) -> Result<(), String>,
{
    let prev_chunks = get_primary().ok().flatten().and_then(|s| chunk_manifest_count(&s)).unwrap_or(0);
    if secrets.is_empty() {
        del(None)?;
        for i in 0..prev_chunks {
            let _ = del(Some(i));
        }
        return Ok(());
    }
    let json = serde_json::to_string(secrets).map_err(|e| e.to_string())?;
    let chunks = split_utf16_chunks(&json, KEYCHAIN_CHUNK_UNITS);
    if chunks.len() <= 1 {
        set(None, &json)?;
        for i in 0..prev_chunks {
            let _ = del(Some(i));
        }
    } else {
        for (i, chunk) in chunks.iter().enumerate() {
            set(Some(i), chunk)?;
        }
        let manifest = format!("{{\"{CHUNK_MANIFEST_KEY}\":{}}}", chunks.len());
        set(None, &manifest)?;
        for i in chunks.len()..prev_chunks {
            let _ = del(Some(i));
        }
    }
    Ok(())
}

/// macOS Keychain / Windows Credential Manager / persistent OS keychain backend.
#[cfg(feature = "keychain")]
pub struct KeychainBackend {
    pub service: String,
    pub account: String,
}

#[cfg(feature = "keychain")]
impl KeychainBackend {
    /// One entry per workspace, keyed by the workspace id.
    pub fn for_workspace(workspace_id: &str) -> Self {
        KeychainBackend { service: "dev.ternarywire.workspace".into(), account: workspace_id.into() }
    }

    /// The app settings' own secrets, one entry for every workspace (see
    /// [`SETTINGS_VAULT_PREFIX`]).
    pub fn for_settings() -> Self {
        KeychainBackend::for_workspace("app-settings")
    }

    fn entry_for(&self, chunk: Option<usize>) -> Result<keyring::Entry, String> {
        let account = match chunk {
            None => self.account.clone(),
            Some(i) => format!("{}#chunk:{i}", self.account),
        };
        keyring::Entry::new(&self.service, &account).map_err(|e| e.to_string())
    }

    /// Checks whether a *persistent* OS keychain is available and accessible
    /// (returns `false` on volatile backends like Linux `keyutils` `UntilReboot`
    /// and on headless CI machines without keychain access).
    pub fn available() -> bool {
        static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            if !matches!(
                keyring::default::default_credential_builder().persistence(),
                keyring::credential::CredentialPersistence::UntilDelete
            ) {
                return false;
            }
            let Ok(entry) = keyring::Entry::new("dev.ternarywire.probe", "probe") else {
                return false;
            };
            matches!(entry.get_password(), Ok(_) | Err(keyring::Error::NoEntry))
        })
    }
}

#[cfg(feature = "keychain")]
impl SecretBackend for KeychainBackend {
    fn load(&self) -> Result<IndexMap<String, String>, String> {
        load_chunked(|chunk| match self.entry_for(chunk)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        })
    }

    fn save(&self, secrets: &IndexMap<String, String>) -> Result<(), String> {
        save_chunked(
            secrets,
            || match self.entry_for(None)?.get_password() {
                Ok(s) => Ok(Some(s)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            |chunk, data| self.entry_for(chunk)?.set_password(data).map_err(|e| e.to_string()),
            |chunk| match self.entry_for(chunk)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        )
    }

    fn describe(&self) -> String {
        format!("keychain {}/{}", self.service, self.account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_backend_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        {
            let s = SecretStore::new(Box::new(FileBackend { path: path.clone() }));
            s.vault_set("api", "k1");
            TokenStore::set(&s, "col1", Some(OAuth2Token { access_token: "t".into(), ..Default::default() }));
        }
        let s = SecretStore::new(Box::new(FileBackend { path }));
        assert_eq!(s.vault_get("api").as_deref(), Some("k1"));
        assert_eq!(s.vault_names(), vec!["api"]);
        assert_eq!(TokenStore::get(&s, "col1").unwrap().access_token, "t");
        assert_eq!(VaultLookup::get(&s, "api").as_deref(), Some("k1"));
    }

    #[test]
    fn unreadable_backend_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        // Hand-edited or written by a newer version: cannot be parsed.
        let original = "{\"vault:api\": \"k1\", \"vault:db\": 42}";
        std::fs::write(&path, original).unwrap();
        let s = SecretStore::new(Box::new(FileBackend { path: path.clone() }));
        assert!(s.load_error().is_some());
        // Saving a new token must not replace the secrets it could not read.
        TokenStore::set(&s, "col1", Some(OAuth2Token { access_token: "t".into(), ..Default::default() }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert_eq!(TokenStore::get(&s, "col1").unwrap().access_token, "t");
    }

    #[test]
    fn concurrent_writes_never_save_an_older_snapshot_last() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        type Saves = Arc<parking_lot::Mutex<Vec<IndexMap<String, String>>>>;
        /// The first save is slow (it waits for the second writer, at most 500 ms).
        struct SlowFirstSave {
            saves: Saves,
            calls: Arc<AtomicUsize>,
            release: parking_lot::Mutex<std::sync::mpsc::Receiver<()>>,
        }
        impl SecretBackend for SlowFirstSave {
            fn load(&self) -> Result<IndexMap<String, String>, String> {
                Ok(IndexMap::new())
            }
            fn save(&self, secrets: &IndexMap<String, String>) -> Result<(), String> {
                if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    let _ = self.release.lock().recv_timeout(Duration::from_millis(500));
                }
                self.saves.lock().push(secrets.clone());
                Ok(())
            }
            fn describe(&self) -> String {
                "slow".into()
            }
        }
        let saves = Saves::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        let backend =
            SlowFirstSave { saves: saves.clone(), calls: calls.clone(), release: parking_lot::Mutex::new(rx) };
        let store = Arc::new(SecretStore::new(Box::new(backend)));
        let first = {
            let store = store.clone();
            std::thread::spawn(move || store.set("a", "1"))
        };
        while calls.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        store.set("b", "2");
        let _ = tx.send(());
        first.join().unwrap();
        let last = saves.lock().last().cloned().unwrap();
        assert_eq!(last.get("b").map(String::as_str), Some("2"), "the newer secret was overwritten on disk");
        assert_eq!(last.get("a").map(String::as_str), Some("1"));
    }

    #[test]
    fn chunked_keychain_helpers_roundtrip_and_shrink() {
        use std::cell::RefCell;
        use std::collections::HashMap;

        let store: RefCell<HashMap<Option<usize>, String>> = RefCell::new(HashMap::new());
        let save = |map: &IndexMap<String, String>| {
            save_chunked(
                map,
                || Ok(store.borrow().get(&None).cloned()),
                |idx, val| {
                    store.borrow_mut().insert(idx, val.to_string());
                    Ok(())
                },
                |idx| {
                    store.borrow_mut().remove(&idx);
                    Ok(())
                },
            )
            .unwrap();
        };
        let load = || load_chunked(|idx| Ok(store.borrow().get(&idx).cloned())).unwrap();

        // Large payload (> 2 chunks, including multi-byte emoji) splits and round-trips.
        let big = IndexMap::from([
            ("big1".to_string(), format!("{}🎉", "a".repeat(KEYCHAIN_CHUNK_UNITS))),
            ("big2".to_string(), "b".repeat(KEYCHAIN_CHUNK_UNITS + 200)),
        ]);
        save(&big);
        assert!(store.borrow().len() >= 4, "expected manifest + >=3 chunks: {:?}", store.borrow().len());
        assert!(store.borrow().values().all(|v| v.encode_utf16().count() <= KEYCHAIN_CHUNK_UNITS));
        assert_eq!(load(), big);

        // Shrinking to a small payload removes all stale chunk entries.
        let small = IndexMap::from([("k".to_string(), "v".to_string())]);
        save(&small);
        assert_eq!(store.borrow().len(), 1);
        assert!(store.borrow().contains_key(&None));
        assert_eq!(load(), small);

        // Saving an empty map cleans up everything.
        save(&IndexMap::new());
        assert!(store.borrow().is_empty());
        assert!(load().is_empty());
    }

    #[test]
    fn save_error_is_recorded_and_noop_update_skips_save() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        struct CountingFail {
            saves: Arc<AtomicUsize>,
            fail: Arc<AtomicBool>,
        }
        impl SecretBackend for CountingFail {
            fn load(&self) -> Result<IndexMap<String, String>, String> {
                Ok(IndexMap::new())
            }
            fn save(&self, _secrets: &IndexMap<String, String>) -> Result<(), String> {
                self.saves.fetch_add(1, Ordering::SeqCst);
                if self.fail.load(Ordering::SeqCst) { Err("disk full".into()) } else { Ok(()) }
            }
            fn describe(&self) -> String {
                "counting".into()
            }
        }
        let saves = Arc::new(AtomicUsize::new(0));
        let fail = Arc::new(AtomicBool::new(false));
        let store = SecretStore::new(Box::new(CountingFail { saves: saves.clone(), fail: fail.clone() }));
        store.update(|_| {});
        assert_eq!(saves.load(Ordering::SeqCst), 0, "no-op update must not call save");
        fail.store(true, Ordering::SeqCst);
        store.set("k", "v");
        assert_eq!(saves.load(Ordering::SeqCst), 1);
        assert_eq!(store.last_save_error().as_deref(), Some("disk full"));
        fail.store(false, Ordering::SeqCst);
        store.set("k", "v2");
        assert_eq!(store.last_save_error(), None);
    }
}

#[cfg(test)]
mod settings_vault_tests {
    use super::*;

    #[test]
    fn settings_names_come_from_the_settings_store() {
        let settings = std::sync::Arc::new(SecretStore::memory());
        settings.vault_set("proxy-password", "s3cret");
        let workspace = std::sync::Arc::new(SecretStore::memory());
        workspace.vault_set("token", "t0k");
        workspace.vault_set("settings.proxy-password", "not this one");
        let inner: std::sync::Arc<dyn VaultLookup> = workspace;
        let vault = SettingsVault { settings: settings.clone(), inner: Some(inner) };
        assert_eq!(vault.get("settings.proxy-password").as_deref(), Some("s3cret"));
        assert_eq!(vault.get("token").as_deref(), Some("t0k"));
        assert_eq!(vault.get("settings.missing"), None);
        let alone = SettingsVault { settings, inner: None };
        assert_eq!(alone.get("token"), None);
    }
}
