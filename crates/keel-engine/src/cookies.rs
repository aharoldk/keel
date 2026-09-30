//! App-level cookie jar (`docs/CONTRACT_V2.md` → cookie_list/delete/clear).
//!
//! Deliberately dependency-free parsing of `Set-Cookie` values (RFC 6265
//! flavored): the send pipeline absorbs raw values from
//! [`crate::engine::http::ExchangeResult::set_cookie_raw`] and asks for a
//! matching `Cookie:` header before each request.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::model::CookieDto;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Stored {
    name: String,
    value: String,
    domain: String,
    path: String,
    expires_at: Option<DateTime<Utc>>,
    secure: bool,
    http_only: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CookieJar {
    entries: Vec<Stored>,
}

fn normalize_domain(raw: &str) -> String {
    raw.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn domain_matches(cookie_domain: &str, host: &str) -> bool {
    let cd = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    let h = host.to_ascii_lowercase();
    if h == cd {
        return true;
    }
    // Dot rule: `example.com` matches `sub.example.com` but never
    // `notexample.com`.
    h.ends_with(&format!(".{cd}"))
}

/// RFC 6265 §5.3: a `Domain` attribute is only honored when the request host
/// equals the domain or is a subdomain of it. Also rejected for IP hosts and
/// for single-label domains — without a public-suffix list, `com`/`org`/…
/// (which are single-label) are the common cookie-tossing targets.
fn domain_attr_allowed(request_host: &str, domain: &str) -> bool {
    if request_host.parse::<std::net::IpAddr>().is_ok() {
        return false;
    }
    if !domain.contains('.') {
        return false;
    }
    request_host == domain || request_host.ends_with(&format!(".{domain}"))
}

fn path_matches(cookie_path: &str, url_path: &str) -> bool {
    if cookie_path == "/" || url_path == cookie_path {
        return true;
    }
    url_path.starts_with(cookie_path)
        && (cookie_path.ends_with('/')
            || url_path.as_bytes().get(cookie_path.len()) == Some(&b'/'))
}

/// RFC 6265 default-path: everything up to (and including) the last `/`.
fn default_path(url_path: &str) -> String {
    match url_path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(idx) => url_path[..=idx].to_string(),
    }
}

fn parse_expiry(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(raw.trim())
        .map(|d| d.with_timezone(&Utc))
        .or_else(|_| DateTime::parse_from_rfc3339(raw.trim()).map(|d| d.with_timezone(&Utc)))
        .ok()
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store cookies from raw `Set-Cookie` header values of a response.
    /// Same (name, domain, path) entries replace older ones; expired values
    /// (Max-Age ≤ 0 or past Expires) delete the stored entry.
    pub fn absorb(&mut self, url: &str, raw_set_cookies: &[String]) {
        let Ok(parsed) = url::Url::parse(url) else {
            return;
        };
        let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
        if host.is_empty() {
            return;
        }
        let url_path = if parsed.path().is_empty() {
            "/"
        } else {
            parsed.path()
        };
        for raw in raw_set_cookies {
            let mut parts = raw.split(';');
            let Some(pair) = parts.next() else { continue };
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let mut domain = host.clone();
            let mut path = default_path(url_path);
            let mut expires_at: Option<DateTime<Utc>> = None;
            let mut max_age: Option<i64> = None;
            let mut secure = false;
            let mut http_only = false;
            let mut reject = false;
            for attr in parts {
                let (key, val) = match attr.split_once('=') {
                    Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim()),
                    None => (attr.trim().to_ascii_lowercase(), ""),
                };
                match key.as_str() {
                    "domain" => {
                        if !val.is_empty() {
                            let d = normalize_domain(val);
                            // RFC 6265 §5.3: ignore the cookie when the Domain
                            // attribute is not the request host or a parent
                            // suffix of it (cookie-tossing guard).
                            if !domain_attr_allowed(&host, &d) {
                                reject = true;
                                break;
                            }
                            domain = d;
                        }
                    }
                    "path" => {
                        if !val.is_empty() {
                            path = val.to_string();
                        }
                    }
                    "max-age" => max_age = val.parse::<i64>().ok(),
                    "expires" => expires_at = parse_expiry(val),
                    "secure" => secure = true,
                    "httponly" => http_only = true,
                    _ => {}
                }
            }
            if reject {
                continue;
            }
            // Max-Age wins over Expires.
            let expiry = match max_age {
                Some(secs) => Some(Utc::now() + chrono::Duration::seconds(secs)),
                None => expires_at,
            };
            // Replace any previous entry for the same triple.
            self.entries.retain(|e| {
                !(e.name == name && e.domain == domain && e.path == path)
            });
            let expired = expiry.map(|at| at <= Utc::now()).unwrap_or(false);
            if expired {
                continue;
            }
            self.entries.push(Stored {
                name: name.to_string(),
                value: value.trim().to_string(),
                domain,
                path,
                expires_at: expiry,
                secure,
                http_only,
            });
        }
    }

    /// `Cookie: a=1; b=2` value for the URL, or None when nothing matches.
    pub fn header_for(&self, url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        let https = parsed.scheme() == "https";
        let url_path = if parsed.path().is_empty() {
            "/"
        } else {
            parsed.path()
        };
        let now = Utc::now();
        let pairs: Vec<String> = self
            .entries
            .iter()
            .filter(|e| {
                !e.name.is_empty()
                    && domain_matches(&e.domain, &host)
                    && path_matches(&e.path, url_path)
                    && (!e.secure || https)
                    && e.expires_at.map(|at| at > now).unwrap_or(true)
            })
            .map(|e| format!("{}={}", e.name, e.value))
            .collect();
        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
    }

    /// All jar entries, for the `cookie_list` command (UI shows the rest).
    pub fn list(&self) -> Vec<CookieDto> {
        self.entries
            .iter()
            .map(|e| CookieDto {
                name: e.name.clone(),
                value: e.value.clone(),
                domain: e.domain.clone(),
                path: e.path.clone(),
                expires: e.expires_at.map(|at| at.to_rfc3339()),
                secure: e.secure,
                http_only: e.http_only,
            })
            .collect()
    }

    pub fn delete(&mut self, domain: &str, name: &str) {
        let domain = normalize_domain(domain);
        self.entries.retain(|e| {
            !(e.domain == domain && (name.is_empty() || e.name == name))
        });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jar_with(url: &str, raws: &[&str]) -> CookieJar {
        let mut jar = CookieJar::new();
        jar.absorb(url, &raws.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        jar
    }

    #[test]
    fn absorb_parse_matrix() {
        let jar = jar_with(
            "http://api.example.com/v1/ping",
            &[
                "sid=42; Path=/; HttpOnly",
                "theme=dark",
                "s2=secure-only; Secure",
                "j=; Domain=example.com",
                "novalue",
                "over=written; Path=/v1",
                "over=final; Path=/v1",
                "uni=caf\u{e9}",
            ],
        );
        let list = jar.list();
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        // Attribute-only garbage (`novalue`, no `=`) is skipped; empty value
        // (`j=`) is a legal RFC 6265 cookie and kept; duplicate `over`
        // collapses to the last write.
        assert_eq!(names, vec!["sid", "theme", "s2", "j", "over", "uni"]);
        let sid = list.iter().find(|c| c.name == "sid").expect("sid");
        assert!(sid.http_only && !sid.secure);
        assert_eq!(sid.path, "/");
        assert_eq!(sid.domain, "api.example.com");
        let over = list.iter().find(|c| c.name == "over").expect("over");
        assert_eq!(over.value, "final");
        assert_eq!(over.expires, None, "session cookie has no expiry");
        assert_eq!(
            list.iter().find(|c| c.name == "uni").expect("uni").value,
            "caf\u{e9}"
        );
    }

    #[test]
    fn default_path_is_directory() {
        let jar = jar_with("http://e.com/a/b/c.txt", &["x=1"]);
        let cookie = &jar.list()[0];
        assert_eq!(cookie.path, "/a/b/");
        assert_eq!(jar.header_for("http://e.com/a/b/other").as_deref(), Some("x=1"));
        assert_eq!(jar.header_for("http://e.com/a/c"), None);
        assert_eq!(jar.header_for("http://e.com/").as_deref(), None);
    }

    #[test]
    fn max_age_and_expires() {
        let jar = jar_with(
            "http://e.com/",
            &[
                "future_maxage=1; Max-Age=3600",
                "past_2822=1; Expires=Wed, 21 Oct 2015 07:28:00 GMT",
                "future_2822=1; Expires=Tue, 13 Aug 2030 12:00:00 GMT",
                "future_iso=1; Expires=2030-08-13T12:00:00+00:00",
                "wins=1; Max-Age=-5; Expires=Tue, 13 Aug 2030 12:00:00 GMT",
                "deleted_now=1; Max-Age=0",
            ],
        );
        let names: Vec<String> = jar.list().into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["future_maxage", "future_2822", "future_iso"]);
        assert_eq!(
            jar.header_for("http://e.com/").unwrap(),
            "future_maxage=1; future_2822=1; future_iso=1"
        );
    }

    #[test]
    fn expired_max_age_deletes_previous_entry() {
        let mut jar = jar_with("http://e.com/", &["a=1"]);
        assert_eq!(jar.header_for("http://e.com/").as_deref(), Some("a=1"));
        jar.absorb("http://e.com/", &["a=; Max-Age=0".to_string()]);
        assert_eq!(jar.header_for("http://e.com/"), None);
        assert!(jar.list().is_empty());
    }

    #[test]
    fn domain_matching_dot_rule() {
        let jar = jar_with("http://api.example.com/", &["s=1; Domain=example.com"]);
        assert!(jar.header_for("http://api.example.com/").is_some());
        assert!(jar.header_for("http://sub.api.example.com/x").is_some(), "suffix");
        assert_eq!(jar.header_for("http://notexample.com/"), None, "no partial suffix");
        assert_eq!(jar.header_for("http://example.org/"), None);
    }

    #[test]
    fn domain_attr_must_be_origin_or_parent() {
        // Same-domain and parent-domain attributes are honored.
        let jar = jar_with(
            "http://sub.example.com/",
            &["a=1; Domain=sub.example.com", "b=2; Domain=example.com"],
        );
        assert_eq!(
            jar.header_for("http://sub.example.com/").as_deref(),
            Some("a=1; b=2")
        );
        // The parent-domain cookie is sent to the parent too.
        assert_eq!(jar.header_for("http://example.com/").as_deref(), Some("b=2"));
    }

    #[test]
    fn domain_attr_cookie_tossing_rejected() {
        let jar = jar_with(
            "http://evil.example.com/",
            &[
                "tld=1; Domain=com",
                "other=1; Domain=other.com",
                "suffix=1; Domain=ample.com",
                "sibling=1; Domain=good.example.com",
            ],
        );
        assert!(jar.list().is_empty(), "all must be rejected: {:?}", jar.list());
        assert_eq!(jar.header_for("http://com/"), None);
        assert_eq!(jar.header_for("http://other.com/"), None);
        assert_eq!(jar.header_for("http://ample.com/"), None);
    }

    #[test]
    fn domain_attr_rejected_for_ip_hosts() {
        let jar = jar_with("http://127.0.0.1:8080/", &["a=1; Domain=127.0.0.1"]);
        assert!(jar.list().is_empty());
        // Host-only cookies still work on IPs.
        let jar = jar_with("http://127.0.0.1:8080/", &["b=2"]);
        assert_eq!(jar.header_for("http://127.0.0.1:9000/").as_deref(), Some("b=2"));
    }

    #[test]
    fn no_domain_is_host_scoped() {
        // Without a Domain attribute the cookie belongs to the origin host:
        // it is never sent to sibling or parent hosts.
        let jar = jar_with("http://sub.example.com/", &["h=1"]);
        assert_eq!(jar.header_for("http://sub.example.com/").as_deref(), Some("h=1"));
        assert_eq!(jar.header_for("http://sibling.example.com/"), None);
        assert_eq!(jar.header_for("http://example.com/"), None);
    }

    #[test]
    fn secure_only_on_https() {
        let jar = jar_with("https://e.com/", &["s=1; Secure"]);
        assert!(jar.header_for("https://e.com/").is_some());
        assert_eq!(jar.header_for("http://e.com/"), None);
    }

    #[test]
    fn path_prefix_boundary() {
        let jar = jar_with("http://e.com/x/y", &["p=1; Path=/x"]);
        assert!(jar.header_for("http://e.com/x").is_some());
        assert!(jar.header_for("http://e.com/x/z").is_some());
        assert_eq!(jar.header_for("http://e.com/xyz"), None, "must match on segment boundary");
    }

    #[test]
    fn delete_and_clear() {
        let mut jar = jar_with(
            "http://e.com/",
            &["a=1", "b=2", "c=3; Domain=other.com"],
        );
        jar.delete("e.com", "a");
        assert_eq!(jar.header_for("http://e.com/").unwrap(), "b=2");
        jar.delete("other.com", "");
        assert!(jar.header_for("http://e.com/").is_some());
        assert_eq!(jar.list().len(), 1);
        jar.clear();
        assert!(jar.list().is_empty());
    }

    #[test]
    fn invalid_url_is_ignored() {
        let mut jar = CookieJar::new();
        jar.absorb("not a url", &["a=1".to_string()]);
        assert!(jar.list().is_empty());
        assert_eq!(jar.header_for("still-not-a-url"), None);
    }
}
