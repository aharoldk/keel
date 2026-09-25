//! RFC 2617 HTTP Digest authentication (MD5 algorithm only).
//!
//! Pure functions: the send pipeline parses the `WWW-Authenticate` challenge,
//! builds the `Authorization` header and retries the request once on 401.

use md5::{Digest as _, Md5};
use rand::Rng as _;

/// A parsed `WWW-Authenticate: Digest` challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    /// Only `MD5` (case-insensitive) is supported; anything else makes
    /// [`parse_challenge`] return `None`.
    pub algorithm: Option<String>,
    /// Raw comma-separated quality-of-protection list, e.g. `"auth,auth-int"`.
    pub qop: Option<String>,
}

fn md5_hex(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let out = hasher.finalize();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

fn random_hex(bytes_len: usize) -> String {
    let mut buf = vec![0u8; bytes_len];
    rand::rng().fill(buf.as_mut_slice());
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parses a `WWW-Authenticate` Digest header value. Handles quoted and
/// unquoted values, case-insensitive keys and commas inside quoted strings.
/// Returns `None` for non-Digest schemes, missing realm/nonce, or an
/// unsupported algorithm.
pub fn parse_challenge(header_value: &str) -> Option<Challenge> {
    let rest = header_value.trim();
    let scheme_len = "digest".len();
    if rest.len() <= scheme_len || !rest[..scheme_len].eq_ignore_ascii_case("digest") {
        return None;
    }
    let rest = rest[scheme_len..].trim_start_matches([' ', '\t']);

    let mut params: Vec<(String, String)> = Vec::new();
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        // key
        let key_start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-') {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if key.is_empty() || i >= chars.len() || chars[i] != '=' {
            // Junk (e.g. trailing commas); skip to the next comma.
            while i < chars.len() && chars[i] != ',' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        i += 1; // consume '='
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let value = if i < chars.len() && chars[i] == '"' {
            i += 1;
            let mut v = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                v.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // closing quote
            }
            v
        } else {
            let v_start = i;
            while i < chars.len() && chars[i] != ',' {
                i += 1;
            }
            chars[v_start..i].iter().collect::<String>().trim().to_string()
        };
        params.push((key.to_ascii_lowercase(), value));
        while i < chars.len() && (chars[i] == ',' || chars[i].is_whitespace()) {
            i += 1;
        }
    }

    let get = |name: &str| {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let realm = get("realm")?;
    let nonce = get("nonce")?;
    if nonce.is_empty() {
        return None;
    }
    let algorithm = get("algorithm");
    if let Some(alg) = &algorithm {
        if !alg.eq_ignore_ascii_case("md5") {
            return None; // MD5 only; skip MD5-sess / SHA-256 etc.
        }
    }
    Some(Challenge {
        realm,
        nonce,
        opaque: get("opaque"),
        algorithm,
        qop: get("qop").filter(|q| !q.trim().is_empty()),
    })
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Builds the `Authorization` header value for the given challenge using
/// RFC 2617 MD5. A random `cnonce` is generated; `nc` is always
/// `"00000001"` (single retry per exchange).
pub fn build_header(
    ch: &Challenge,
    method: &str,
    uri: &str,
    user: &str,
    pass: &str,
) -> String {
    let cnonce = random_hex(8);
    build_header_with_cnonce(ch, method, uri, user, pass, &cnonce)
}

/// Deterministic variant of [`build_header`] (testable against RFC vectors).
pub fn build_header_with_cnonce(
    ch: &Challenge,
    method: &str,
    uri: &str,
    username: &str,
    password: &str,
    cnonce: &str,
) -> String {
    let ha1 = md5_hex(&format!("{}:{}:{}", username, ch.realm, password));
    let ha2 = md5_hex(&format!("{}:{}", method, uri));

    let qop_selected = ch
        .qop
        .as_deref()
        .and_then(|list| {
            list.split(',')
                .map(str::trim)
                .find(|q| q.eq_ignore_ascii_case("auth"))
        })
        .map(|_| "auth");

    let mut parts: Vec<String> = vec![
        format!("username={}", quote(username)),
        format!("realm={}", quote(&ch.realm)),
        format!("nonce={}", quote(&ch.nonce)),
        format!("uri={}", quote(uri)),
    ];
    let response = match qop_selected {
        Some(qop) => {
            let nc = "00000001";
            parts.push(format!("qop={qop}"));
            parts.push(format!("nc={nc}"));
            parts.push(format!("cnonce={}", quote(cnonce)));
            md5_hex(&format!("{ha1}:{}:{nc}:{cnonce}:{qop}:{ha2}", ch.nonce))
        }
        None => md5_hex(&format!("{ha1}:{}:{ha2}", ch.nonce)),
    };
    parts.push(format!("response={}", quote(&response)));
    if let Some(opaque) = &ch.opaque {
        parts.push(format!("opaque={}", quote(opaque)));
    }
    format!("Digest {}", parts.join(", "))
}

/// Should the caller retry with digest credentials? True only for a plain
/// 401 whose `WWW-Authenticate` advertises the Digest scheme.
pub fn needs_retry(status: u16, www_auth: Option<&str>) -> bool {
    status == 401
        && www_auth
            .map(|v| {
                let v = v.trim();
                v.get(..6).map(|h| h.eq_ignore_ascii_case("digest"))
            })
            .unwrap_or(None)
            == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 2617 §3.5 challenge (the "Mufasa" example).
    const RFC_CHALLENGE: &str = "Digest realm=\"testrealm@host.com\", qop=\"auth,auth-int\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";

    fn rfc_challenge() -> Challenge {
        parse_challenge(RFC_CHALLENGE).expect("parse")
    }

    #[test]
    fn parse_rfc2617_challenge() {
        let ch = rfc_challenge();
        assert_eq!(ch.realm, "testrealm@host.com");
        assert_eq!(ch.nonce, "dcd98b7102dd2f0e8b11d0f600bfb0c093");
        assert_eq!(ch.opaque.as_deref(), Some("5ccc069c403ebaf9f0171e9517f40e41"));
        assert_eq!(ch.qop.as_deref(), Some("auth,auth-int"));
        assert_eq!(ch.algorithm, None);
    }

    #[test]
    fn parse_quoted_comma_unquoted_and_case() {
        let ch = parse_challenge(
            "Digest Realm=test, NONCE=\"a,b,c\", Algorithm=MD5, qop=Auth, Opaque=xyz",
        )
        .expect("parse");
        assert_eq!(ch.realm, "test");
        assert_eq!(ch.nonce, "a,b,c");
        assert_eq!(ch.qop.as_deref(), Some("Auth"));
        assert_eq!(ch.opaque.as_deref(), Some("xyz"));
        assert_eq!(ch.algorithm.as_deref(), Some("MD5"));
    }

    #[test]
    fn parse_rejects_other_schemes_and_algorithms() {
        assert!(parse_challenge("Basic realm=\"x\"").is_none());
        assert!(parse_challenge("Digest realm=\"r\", nonce=\"n\", algorithm=MD5-sess").is_none());
        assert!(parse_challenge("Digest realm=\"r\", nonce=\"n\", algorithm=SHA-256").is_none());
        assert!(parse_challenge("Digest realm=\"r\"").is_none()); // no nonce
        assert!(parse_challenge("").is_none());
    }

    #[test]
    fn rfc2617_qop_auth_vector() {
        let ch = rfc_challenge();
        let header = build_header_with_cnonce(
            &ch, "GET", "/dir/index.html", "Mufasa", "Circle Of Life", "0a4f113b",
        );
        assert!(header.starts_with("Digest "), "{header}");
        assert!(
            header.contains("response=\"6629fae49393a05397450978507c4ef1\""),
            "{header}"
        );
        assert!(header.contains("qop=auth"), "{header}");
        assert!(header.contains("nc=00000001"), "{header}");
        assert!(header.contains("cnonce=\"0a4f113b\""), "{header}");
        assert!(
            header.contains("opaque=\"5ccc069c403ebaf9f0171e9517f40e41\""),
            "{header}"
        );
    }

    #[test]
    fn rfc2069_no_qop_vector() {
        let ch = parse_challenge(
            "Digest realm=\"testrealm@host.com\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"",
        )
        .expect("parse");
        let header = build_header_with_cnonce(
            &ch, "GET", "/dir/index.html", "Mufasa", "Circle Of Life", "ignored",
        );
        assert!(
            header.contains("response=\"670fd8c2df070c60b045671b8b24ff02\""),
            "{header}"
        );
        assert!(!header.contains("qop="), "{header}");
        assert!(!header.contains("cnonce="), "{header}");
    }

    #[test]
    fn random_cnonce_builds_valid_header() {
        let ch = rfc_challenge();
        let header = build_header(&ch, "GET", "/dir/index.html", "Mufasa", "Circle Of Life");
        // A fresh random cnonce round-trips through the same digest formula.
        let cnonce = header
            .split("cnonce=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("cnonce present");
        assert_eq!(cnonce.len(), 16);
        let expected = build_header_with_cnonce(
            &ch, "GET", "/dir/index.html", "Mufasa", "Circle Of Life", cnonce,
        );
        assert_eq!(header, expected);
    }

    #[test]
    fn needs_retry_matrix() {
        assert!(needs_retry(401, Some(RFC_CHALLENGE)));
        assert!(needs_retry(401, Some("digest realm=\"r\", nonce=\"n\"")));
        assert!(!needs_retry(401, Some("Basic realm=\"r\"")));
        assert!(!needs_retry(401, None));
        assert!(!needs_retry(403, Some(RFC_CHALLENGE)));
        assert!(!needs_retry(200, Some(RFC_CHALLENGE)));
    }
}
