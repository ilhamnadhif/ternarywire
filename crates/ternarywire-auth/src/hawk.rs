//! Hawk HTTP authentication (https://github.com/mozilla/hawk).

use base64::Engine;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use ternarywire_core::{PreparedBody, PreparedRequest};

use crate::error::AuthError;
use crate::util::{B64, hmac_sha1, hmac_sha256, now_secs, param, parse_url, random_alnum, read_file};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Alg {
    Sha256,
    Sha1,
}

impl Alg {
    fn parse(s: &str) -> Result<Alg, AuthError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "sha256" => Ok(Alg::Sha256),
            "sha1" => Ok(Alg::Sha1),
            other => Err(AuthError::invalid(format!("unsupported Hawk algorithm `{other}`"))),
        }
    }

    fn hash(self, data: &[u8]) -> String {
        match self {
            Alg::Sha256 => B64.encode(Sha256::digest(data)),
            Alg::Sha1 => B64.encode(Sha1::digest(data)),
        }
    }

    fn mac(self, key: &[u8], data: &[u8]) -> String {
        match self {
            Alg::Sha256 => B64.encode(hmac_sha256(key, data)),
            Alg::Sha1 => B64.encode(hmac_sha1(key, data)),
        }
    }
}

fn payload_hash(alg: Alg, content_type: &str, payload: &[u8]) -> String {
    let ct = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let mut data = format!("hawk.1.payload\n{ct}\n").into_bytes();
    data.extend_from_slice(payload);
    data.push(b'\n');
    alg.hash(&data)
}

/// [`payload_hash`] of the request body, reading a body file.
fn body_hash(alg: Alg, content_type: &str, body: &PreparedBody) -> Result<String, AuthError> {
    fn file<D: Digest>(content_type: &str, path: &std::path::Path) -> Result<String, AuthError> {
        let ct = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        let mut d = D::new();
        d.update(format!("hawk.1.payload\n{ct}\n"));
        read_file(path, &mut |b| d.update(b))?;
        d.update(b"\n");
        Ok(B64.encode(d.finalize()))
    }
    match body {
        PreparedBody::File(path) => match alg {
            Alg::Sha256 => file::<Sha256>(content_type, path),
            Alg::Sha1 => file::<Sha1>(content_type, path),
        },
        PreparedBody::Multipart(_) => Err(AuthError::invalid(
            "Hawk: a form-data body has no payload hash before it is sent (its boundary is chosen then); \
             turn off \"Include payload hash\" or send the body another way",
        )),
        body => Ok(payload_hash(alg, content_type, body.as_bytes().unwrap_or_default())),
    }
}

pub(crate) struct HawkInput<'a> {
    pub ts: &'a str,
    pub nonce: &'a str,
    pub method: &'a str,
    pub resource: &'a str,
    pub host: &'a str,
    pub port: u16,
    pub hash: &'a str,
    pub ext: &'a str,
    pub app: &'a str,
    pub dlg: &'a str,
}

fn normalized(i: &HawkInput<'_>) -> String {
    let mut s = format!(
        "hawk.1.header\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
        i.ts,
        i.nonce,
        i.method.to_ascii_uppercase(),
        i.resource,
        i.host.to_ascii_lowercase(),
        i.port,
        i.hash,
        i.ext.replace('\\', "\\\\").replace('\n', "\\n")
    );
    if !i.app.is_empty() {
        s.push_str(&format!("{}\n{}\n", i.app, i.dlg));
    }
    s
}

fn quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn apply(req: &mut PreparedRequest) -> Result<(), AuthError> {
    let auth = req.auth.clone();
    let id = param(&auth, "authId");
    let key = auth.get("authKey");
    if id.is_empty() {
        return Err(AuthError::Missing { scheme: "Hawk", field: "authId" });
    }
    if key.is_empty() {
        return Err(AuthError::Missing { scheme: "Hawk", field: "authKey" });
    }
    let alg = Alg::parse(auth.get("algorithm"))?;
    let url = parse_url(req)?;
    let ts = match param(&auth, "timestamp") {
        "" => now_secs().to_string(),
        t => t.to_string(),
    };
    let nonce = match param(&auth, "nonce") {
        "" => random_alnum(6),
        n => n.to_string(),
    };
    let resource = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_string(),
    };
    let hash = if auth.get_bool("includePayloadHash", false) {
        body_hash(alg, req.header("content-type").unwrap_or(""), &req.body)?
    } else {
        String::new()
    };
    let ext = auth.get("extraData");
    let app = auth.get("app");
    let dlg = auth.get("delegation");
    let input = HawkInput {
        ts: &ts,
        nonce: &nonce,
        method: &req.method,
        resource: &resource,
        host: url.host_str().unwrap_or_default(),
        port: url.port_or_known_default().unwrap_or(80),
        hash: &hash,
        ext,
        app,
        dlg,
    };
    let mac = alg.mac(key.as_bytes(), normalized(&input).as_bytes());
    let mut header = format!("Hawk id=\"{}\", ts=\"{ts}\", nonce=\"{nonce}\"", quote(id));
    if !hash.is_empty() {
        header.push_str(&format!(", hash=\"{hash}\""));
    }
    if !ext.is_empty() {
        header.push_str(&format!(", ext=\"{}\"", quote(ext)));
    }
    header.push_str(&format!(", mac=\"{mac}\""));
    if !app.is_empty() {
        header.push_str(&format!(", app=\"{}\"", quote(app)));
        if !dlg.is_empty() {
            header.push_str(&format!(", dlg=\"{}\"", quote(dlg)));
        }
    }
    req.set_header("Authorization", header);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ternarywire_core::{Auth, AuthType, MultipartPart, MultipartValue};

    fn creds() -> Auth {
        Auth::new(AuthType::Hawk)
            .with("authId", "dh37fgj492je")
            .with("authKey", "werxhqb98rpaxn39848xrunpaw3489ruxnpa98w4rxn")
            .with("algorithm", "sha256")
            .with("user", "Steve")
            .with("timestamp", "1353832234")
            .with("nonce", "j4h3g2")
            .with("extraData", "some-app-ext-data")
    }

    #[test]
    fn readme_get_example() {
        // Hawk README "Usage Example".
        let mut req = PreparedRequest {
            url: "http://example.com:8000/resource/1?b=1&a=2".into(),
            auth: creds(),
            ..Default::default()
        };
        apply(&mut req).unwrap();
        assert_eq!(
            req.header("authorization").unwrap(),
            r#"Hawk id="dh37fgj492je", ts="1353832234", nonce="j4h3g2", ext="some-app-ext-data", mac="6R4rV5iE+NPoym+WwjeHzjAGXUtLNIxmo1vpMofpLAE=""#
        );
    }

    #[test]
    fn readme_payload_example() {
        // Hawk README "Payload Validation".
        assert_eq!(
            payload_hash(Alg::Sha256, "text/plain", b"Thank you for flying Hawk"),
            "Yi9LfIIFRtBEPt74PVmbTF/xVAwPn7ub15ePICfgnuY="
        );
        let mut req = PreparedRequest {
            method: "POST".into(),
            url: "http://example.com:8000/resource/1?b=1&a=2".into(),
            headers: vec![("Content-Type".into(), "text/plain".into())],
            body: PreparedBody::Bytes("Thank you for flying Hawk".into()),
            auth: creds().with("includePayloadHash", true),
            ..Default::default()
        };
        apply(&mut req).unwrap();
        let h = req.header("authorization").unwrap();
        assert!(h.contains(r#"hash="Yi9LfIIFRtBEPt74PVmbTF/xVAwPn7ub15ePICfgnuY=""#), "{h}");
        assert!(h.contains(r#"mac="aSe1DERmZuRl3pI36/9BdZmnErTw3sNzOOAUlfeKjVw=""#), "{h}");
    }

    #[test]
    fn payload_hash_of_body_files_and_form_data() {
        // The README payload example, from a file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.txt");
        std::fs::write(&path, "Thank you for flying Hawk").unwrap();
        let mut req = PreparedRequest {
            method: "POST".into(),
            url: "http://example.com:8000/resource/1?b=1&a=2".into(),
            headers: vec![("Content-Type".into(), "text/plain".into())],
            body: PreparedBody::File(path),
            auth: creds().with("includePayloadHash", true),
            ..Default::default()
        };
        apply(&mut req).unwrap();
        let h = req.header("authorization").unwrap();
        assert!(h.contains(r#"hash="Yi9LfIIFRtBEPt74PVmbTF/xVAwPn7ub15ePICfgnuY=""#), "{h}");
        assert!(h.contains(r#"mac="aSe1DERmZuRl3pI36/9BdZmnErTw3sNzOOAUlfeKjVw=""#), "{h}");
        let sha1 = body_hash(Alg::Sha1, "", &PreparedBody::File(dir.path().join("body.txt"))).unwrap();
        assert_eq!(sha1, payload_hash(Alg::Sha1, "", b"Thank you for flying Hawk"));
        // Missing files and form-data bodies are errors, not a hash of nothing.
        req.body = PreparedBody::File(dir.path().join("missing.bin"));
        assert!(apply(&mut req).unwrap_err().to_string().contains("missing.bin"));
        req.body = PreparedBody::Multipart(vec![MultipartPart {
            name: "a".into(),
            value: MultipartValue::Text("b".into()),
            content_type: None,
        }]);
        assert!(apply(&mut req).unwrap_err().to_string().contains("form-data"));
    }

    #[test]
    fn missing_fields() {
        let mut req =
            PreparedRequest { url: "http://x/".into(), auth: Auth::new(AuthType::Hawk), ..Default::default() };
        assert!(matches!(apply(&mut req), Err(AuthError::Missing { field: "authId", .. })));
    }
}
