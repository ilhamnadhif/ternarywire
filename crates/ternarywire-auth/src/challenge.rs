//! Parsing of `WWW-Authenticate` headers (RFC 7235), including several
//! challenges in one header and token68 values such as NTLM messages. The
//! parser lives in `ternarywire-http`, whose proxy tunnels read
//! `Proxy-Authenticate` with it.

pub use ternarywire_http::challenge::{Challenge, parse_challenges};
