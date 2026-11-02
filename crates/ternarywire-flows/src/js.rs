//! Running JavaScript for Script, Condition, Template and Loop blocks through
//! the shared [`ScriptEngine`](ternarywire_core::script::ScriptEngine).
//!
//! The flow context travels in the data scope under [`CONTEXT_KEY`] (values
//! keep their JSON type there) and the return value comes back as a local
//! variable, so only the engine's stable job/outcome contract is used.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ternarywire_core::script::{ScriptError, ScriptJob, ScriptOutcome};
use ternarywire_runner::Services;
use tokio_util::sync::CancellationToken;

/// Data-scope key holding `{ blocks, labels, index, inputs, vars, loop, input, flow }`.
pub const CONTEXT_KEY: &str = "__ternarywire_flow";
/// Local variable carrying the JSON-encoded return value.
pub const RESULT_KEY: &str = "__ternarywire_result";
/// Local variable carrying the JSON-encoded `vars` object after the script.
pub const VARS_KEY: &str = "__ternarywire_vars";
/// Prefix of internal keys that never become flow variables.
pub const INTERNAL_PREFIX: &str = "__ternarywire_";

/// `blocks`: the records of the blocks that ran by id, with their labels as
/// extra keys; any other key finds its block the way templates do
/// ([`Flow::find_block`](crate::model::Flow::find_block): id, label, label
/// ignoring ASCII case, then slug) through `__mf.index` (`[id, label, slug]`
/// of every block, in flow order). One line, like the rest of the prelude.
const BLOCKS: &str = concat!(
    r#"const __mb = Object.assign({}, __mf.blocks || {}); "#,
    r#"for (const [k, v] of Object.entries(__mf.labels || {})) { if (!(k in __mb) && __mb[v] !== undefined) __mb[k] = __mb[v]; } "#,
    r#"const __mk = (k) => { const key = k.trim(), ix = __mf.index || [], lo = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase()); "#,
    r#"const hit = ix.find((b) => b[0] === key) || ix.find((b) => b[1] === key) || ix.find((b) => lo(b[1]) === lo(key)); "#,
    r#"if (hit) return hit[0]; let s = "", n = 0, dash = false; "#,
    r#"for (const c of key) { if (/[\p{Alphabetic}\p{N}]/u.test(c)) { const l = c.toLowerCase(); s += l; dash = false; "#,
    r#"for (const x of l) { const p = x.codePointAt(0); n += p < 0x80 ? 1 : p < 0x800 ? 2 : p < 0x10000 ? 3 : 4; } } "#,
    r#"else if (!dash && s !== "") { s += "-"; n += 1; dash = true; } if (n >= 60) break; } "#,
    r#"s = s.replace(/-+$/, "") || "untitled"; const slug = ix.find((b) => b[1] !== "" && b[2] === s); return slug && slug[0]; }; "#,
    r#"const blocks = new Proxy(__mb, { get: (t, k) => { if (typeof k !== "string" || k in t) return t[k]; const id = __mk(k); return id === undefined ? undefined : t[id]; }, "#,
    r#"has: (t, k) => { if (typeof k !== "string" || k in t) return k in t; const id = __mk(k); return id !== undefined && id in t; } }); "#,
);

/// True when `code` uses `await` as a word outside of comments and string
/// literals. When the scan gets lost (an unterminated string or comment,
/// say after a regular expression with a quote in it), any `await` counts:
/// missing a real one would be a syntax error.
pub(crate) fn uses_await(code: &str) -> bool {
    let b = code.as_bytes();
    let lost = || code.contains("await");
    let ident = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$') || c >= 0x80;
    // Brace depth where each open `${` of a template literal started.
    let mut subs: Vec<usize> = Vec::new();
    let (mut depth, mut i, mut template) = (0usize, 0usize, false);
    while i < b.len() {
        if template {
            match b[i] {
                b'\\' => i += 2,
                b'`' => {
                    template = false;
                    i += 1;
                }
                b'$' if b.get(i + 1) == Some(&b'{') => {
                    subs.push(depth);
                    depth += 1;
                    template = false;
                    i += 2;
                }
                _ => i += 1,
            }
            continue;
        }
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => i = code[i..].find('\n').map_or(b.len(), |n| i + n),
            b'/' if b.get(i + 1) == Some(&b'*') => match code[i + 2..].find("*/") {
                Some(n) => i += n + 4,
                None => return lost(),
            },
            q @ (b'"' | b'\'') => {
                i += 1;
                loop {
                    match b.get(i) {
                        None | Some(b'\n') => return lost(),
                        Some(b'\\') => i += 2,
                        Some(c) => {
                            i += 1;
                            if *c == q {
                                break;
                            }
                        }
                    }
                }
            }
            b'`' => {
                template = true;
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if subs.last() == Some(&depth) {
                    subs.pop();
                    template = true;
                }
                i += 1;
            }
            c if ident(c) => {
                let start = i;
                while i < b.len() && ident(b[i]) {
                    i += 1;
                }
                if &b[start..i] == b"await" {
                    return true;
                }
            }
            _ => i += 1,
        }
    }
    if template { lost() } else { false }
}

/// Wraps user code (a function body; `return` sets the result). Everything
/// before the user code stays on the first line so error line numbers match
/// what the user typed. Only code that awaits gets the async wrapper, which
/// also resolves a returned promise; an `await` in a comment or a string
/// must not change the result.
pub fn wrap(body: &str) -> String {
    let is_async = uses_await(body);
    let (open, close) = if is_async { ("await (async function () {", "})()") } else { ("(function () {", "})()") };
    format!(
        "const __mf = pm.iterationData.get(\"{CONTEXT_KEY}\") || {{}}; {BLOCKS}\
         const inputs = __mf.inputs || {{}}, vars = __mf.vars || {{}}, flow = __mf.flow || {{}}; \
         const loop = __mf.loop === undefined ? null : __mf.loop, input = __mf.input === undefined ? null : __mf.input; \
         const __mr = {open}{body}\n{close}; \
         pm.variables.set(\"{RESULT_KEY}\", JSON.stringify(__mr === undefined ? null : __mr)); \
         pm.variables.set(\"{VARS_KEY}\", JSON.stringify(vars));"
    )
}

/// Wraps an expression so its value is the result.
pub fn wrap_expression(expr: &str) -> String {
    wrap(&format!("return ({}\n);", expr.trim().trim_end_matches(';')))
}

/// Runs a job on a blocking thread, honoring cancellation (same as the
/// request pipeline does).
pub async fn run_job(services: &Services, mut job: ScriptJob, cancel: &CancellationToken) -> ScriptOutcome {
    let flag = Arc::new(AtomicBool::new(false));
    job.cancel = flag.clone();
    job.timeout = services.script_timeout;
    let watcher = {
        let cancel = cancel.clone();
        let flag = flag.clone();
        tokio::spawn(async move {
            cancel.cancelled().await;
            flag.store(true, Ordering::SeqCst);
        })
    };
    let engine = services.scripts.clone();
    let request = job.request.clone();
    let variables = job.variables.clone();
    let outcome = tokio::task::spawn_blocking(move || engine.run(job)).await;
    watcher.abort();
    match outcome {
        Ok(o) => o,
        Err(e) => ScriptOutcome {
            request,
            variables,
            errors: vec![ScriptError { location: "script engine".into(), message: format!("engine crashed: {e}") }],
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn await_outside_comments_and_strings() {
        for code in [
            "await pm.sendRequest(url);",
            "const r = await fetch(); // done",
            "return `${await x()}`;",
            "return `a ${ {b: await y()}.b } c`;",
            "if (a) {\n  await b;\n}",
            "x = '\\'' + await y;",
            // Lost in a regular expression: counted, to be safe.
            "const re = /'/; await z;",
        ] {
            assert!(uses_await(code), "{code}");
        }
        for code in [
            "// await later\nreturn 1;",
            "/* await */ return 2;",
            "return 'please await';",
            "return \"await\";",
            "return `await ${x} later`;",
            "const awaitTime = 3; return awaitTime;",
            "return obj.awaited;",
            "",
        ] {
            assert!(!uses_await(code), "{code}");
        }
        assert!(wrap("await 1;").contains("await (async function"));
        assert!(!wrap("// await\nreturn 1;").contains("(async function"));
    }
}
