//! Protocol clients for TernaryWire: the HTTP engine (redirects, cookies, proxy,
//! certificates, streaming of large bodies to disk), WebSocket sessions, the
//! client-certificate loading shared by every TLS client ([`certs`]) and the
//! proxy selection and tunnels shared by every other client ([`proxy`]),
//! plus the HTTP authentication primitives they share with `ternarywire-auth`
//! ([`challenge`], [`digest`]).

pub mod certs;
pub mod challenge;
pub mod cookies;
pub mod digest;
pub mod engine;
pub mod error;
pub mod proxy;
pub mod tls;
pub mod ws;

pub use certs::{ClientIdentity, SecretResolver};
pub use cookies::{CookieInfo, CookieJar};
pub use engine::{EngineConfig, HttpEngine, Progress, SendOptions, SendOutcome, SentRequest};
pub use error::{HttpError, HttpErrorKind};
pub use tokio_util::sync::CancellationToken;

/// Runs `f`, which may block for a while (downloading or calling a PAC
/// script, reading the system proxy settings), without stalling the other
/// tasks of the tokio worker it is called on: on a multi-threaded runtime
/// through `block_in_place`. Elsewhere (no runtime, a current-thread one)
/// `f` simply runs.
pub(crate) fn blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => tokio::task::block_in_place(f),
        _ => f(),
    }
}
