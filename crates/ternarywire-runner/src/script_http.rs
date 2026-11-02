//! `pm.sendRequest` support: scripts run on blocking threads and call back
//! into the async HTTP engine through the runtime handle.

use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use ternarywire_core::auth::Auth;
use ternarywire_core::model::{Body, BodyMode, FormFieldKind, HttpRequest};
use ternarywire_core::prepared::{PrepareContext, prepare};
use ternarywire_core::script::{ScriptHttp, ScriptRequest, ScriptResponse};
use ternarywire_core::variables::{Resolver, VariableScopes};
use ternarywire_http::SendOptions;

use crate::pipeline::{Services, script_response};

pub struct PipelineHttp {
    services: Services,
    base_dir: Option<PathBuf>,
    cancel: CancellationToken,
    /// The end of the script job: a request still running then is dropped.
    deadline: Option<Instant>,
}

impl PipelineHttp {
    /// For one script job, created right before it runs. Scripts cannot send
    /// files until [`PipelineHttp::with_base_dir`] gives them a working
    /// directory.
    pub fn new(services: Services) -> Self {
        let deadline = Instant::now().checked_add(services.script_timeout);
        PipelineHttp { services, base_dir: None, cancel: CancellationToken::new(), deadline }
    }

    /// The working directory: file bodies of `pm.sendRequest` must be
    /// relative paths inside it.
    pub fn with_base_dir(mut self, dir: Option<&Path>) -> Self {
        self.base_dir = dir.map(Path::to_path_buf);
        self
    }

    /// Stops requests in flight when the run is cancelled.
    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }
}

impl ScriptHttp for PipelineHttp {
    fn send(&self, request: ScriptRequest, scopes: &VariableScopes) -> Result<ScriptResponse, String> {
        let mut http = HttpRequest {
            method: if request.method.is_empty() { "GET".into() } else { request.method },
            headers: request.headers,
            body: request.body,
            ..Default::default()
        };
        http.set_url(request.url);
        let mut resolver = Resolver::new(scopes);
        if let Some(v) = &self.services.vault {
            resolver = resolver.with_vault(v.as_ref());
        }
        check_script_files(&Body::default(), &http.body, &resolver, self.base_dir.as_deref())
            .map_err(|e| format!("pm.sendRequest: {e}"))?;
        let auth = Auth::none();
        let ctx = PrepareContext {
            resolver,
            auth: &auth,
            auth_owner_id: None,
            defaults: &self.services.defaults,
            base_dir: self.base_dir.as_deref(),
        };
        let prepared = prepare(&http, &ctx);
        let engine = self.services.http.clone();
        let opts = SendOptions::with_cancel(self.cancel.clone());
        let deadline = self.deadline;
        let fut = async move {
            let send = engine.send(&prepared, &opts);
            match deadline {
                Some(d) => match tokio::time::timeout_at(d.into(), send).await {
                    Ok(r) => r.map_err(|e| e.message),
                    Err(_) => Err("pm.sendRequest did not finish before the script timed out".to_string()),
                },
                None => send.await.map_err(|e| e.message),
            }
        };
        // Called from a blocking (non-runtime) thread, so block_on is allowed.
        let out = self.services.runtime.block_on(fut)?;
        Ok(script_response(&out.response))
    }
}

/// File paths a body sends, as [`prepare`] picks them (before variables).
fn body_files(b: &Body) -> Vec<&str> {
    match b.mode {
        BodyMode::Binary if !b.binary_path.trim().is_empty() => vec![b.binary_path.as_str()],
        BodyMode::FormData => b
            .form_data
            .iter()
            .filter(|f| f.enabled && !f.key.is_empty() && f.kind == FormFieldKind::File)
            .flat_map(|f| f.files.iter().map(String::as_str))
            .collect(),
        _ => Vec::new(),
    }
}

/// Checks the files a script put in `body`: like Postman's working
/// directory, they must be relative paths that stay inside `base_dir`
/// (symlinks resolved), so a script cannot upload arbitrary local files.
/// Paths that `saved` (the request before the script ran) already had are
/// the user's own and keep working as before.
pub(crate) fn check_script_files(
    saved: &Body,
    body: &Body,
    resolver: &Resolver<'_>,
    base_dir: Option<&Path>,
) -> Result<(), String> {
    let known = |p: &str| saved.binary_path == p || saved.form_data.iter().any(|f| f.files.iter().any(|s| s == p));
    for raw in body_files(body).into_iter().filter(|p| !known(p)) {
        let resolved = resolver.resolve(raw);
        let path = Path::new(&resolved);
        let refused = |why: &str| format!("file \"{resolved}\" set by a script was not sent: {why}");
        let Some(base) = base_dir else {
            return Err(refused("scripts can only send files from the working directory, and this run has none"));
        };
        if !path.is_relative() || path.components().any(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) {
            return Err(refused("scripts can only send files by a path relative to the working directory"));
        }
        let root = base.canonicalize().map_err(|e| refused(&format!("working directory {}: {e}", base.display())))?;
        let real = base.join(path).canonicalize().map_err(|e| refused(&e.to_string()))?;
        if !real.starts_with(&root) {
            return Err(refused(&format!("it is outside the working directory {}", base.display())));
        }
    }
    Ok(())
}
