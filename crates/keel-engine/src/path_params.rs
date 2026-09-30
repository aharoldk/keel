//! `:name` URL path params (Keel Format v1.1, see docs/CONTRACT_V2.md).
//!
//! Params are recognized only inside the *path* portion of a URL template
//! (after the host, before `?query`/`#fragment`). A path segment is a param
//! when it starts with `:` and the rest is `[A-Za-z0-9_]+`; anything else
//! (e.g. OData-style `(key)` or `(':id')` segments) is left untouched.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// Encode set for a single path segment: everything non-alphanumeric except
/// RFC 3986 unreserved/sub-delims (and `:`/`@`). Notably `/`, `?`, `#` and
/// `%` are encoded so a value can never break out of its segment.
pub const PATH_SEGMENT_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b':')
    .remove(b'@');

/// Returns the byte range of the path portion of `url` (after the host,
/// before `?`/`#`). Handles template hosts like `{{baseUrl}}/users/:id`.
fn path_bounds(url: &str) -> (usize, usize) {
    let after_host = if let Some(idx) = url.find("://") {
        let rest = idx + 3;
        match url[rest..].find('/') {
            Some(j) => rest + j,
            None => url.len(),
        }
    } else {
        match url.find('/') {
            Some(j) => j,
            None => url.len(),
        }
    };
    let end = url[after_host..]
        .find(['?', '#'])
        .map(|j| after_host + j)
        .unwrap_or(url.len());
    (after_host, end)
}

fn is_param_segment(seg: &str) -> Option<&str> {
    let name = seg.strip_prefix(':')?;
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(name)
    } else {
        None
    }
}

/// Extracts `:name` params from the path of a URL template, deduplicated in
/// order of first appearance.
pub fn parse(url_template: &str) -> Vec<String> {
    let (start, end) = path_bounds(url_template);
    let mut out: Vec<String> = Vec::new();
    for seg in url_template[start..end].split('/') {
        if let Some(name) = is_param_segment(seg) {
            let name = name.to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Substitutes each `/:name` path segment with the percent-encoded value from
/// `rows`. Errors when a param used by the URL has no row; rows whose names
/// do not appear in the URL are ignored.
pub fn apply(url: &str, rows: &[(String, String)]) -> Result<String, String> {
    let (start, end) = path_bounds(url);
    let path = &url[start..end];
    let used = parse(url);
    if used.is_empty() {
        return Ok(url.to_string());
    }
    for name in &used {
        if !rows.iter().any(|(n, _)| n == name) {
            return Err(format!("missing value for path param `{name}`"));
        }
    }
    let new_path: Vec<String> = path
        .split('/')
        .map(|seg| match is_param_segment(seg) {
            Some(name) => {
                let value = rows
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                utf8_percent_encode(value, PATH_SEGMENT_SET).to_string()
            }
            None => seg.to_string(),
        })
        .collect();
    let mut out = String::with_capacity(url.len());
    out.push_str(&url[..start]);
    out.push_str(&new_path.join("/"));
    out.push_str(&url[end..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_and_dedup() {
        assert_eq!(parse("http://x/users/:id"), vec!["id".to_string()]);
        assert_eq!(
            parse("http://x/users/:id/posts/:post_id"),
            vec!["id".to_string(), "post_id".to_string()]
        );
        assert_eq!(
            parse("http://x/:a/:b/:a"),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn parse_only_within_path() {
        // Port in the authority is not a param.
        assert_eq!(parse("http://host:8080/users/:id"), vec!["id".to_string()]);
        // Query and fragment are excluded.
        assert_eq!(
            parse("http://x/a/:id?b=:value#frag=:f"),
            vec!["id".to_string()]
        );
        // Template hosts work.
        assert_eq!(parse("{{baseUrl}}/users/:id"), vec!["id".to_string()]);
        assert_eq!(parse("/plain/:id"), vec!["id".to_string()]);
        assert!(parse("http://x/users").is_empty());
    }

    #[test]
    fn parse_rejects_non_param_segments() {
        // OData-ish parenthesised keys stay untouched.
        assert!(parse("http://x/Products(key)").is_empty());
        assert!(parse("http://x/Products(':id')").is_empty());
        // Colon not at segment start, or empty/invalid name.
        assert!(parse("http://x/foo:bar").is_empty());
        assert!(parse("http://x/:").is_empty());
        assert!(parse("http://x/:id-name").is_empty());
    }

    #[test]
    fn apply_substitutes_and_encodes() {
        let rows = vec![
            ("id".to_string(), "7".to_string()),
            ("name".to_string(), "a b/c?d".to_string()),
        ];
        assert_eq!(
            apply("http://x/users/:id", &rows).expect("ok"),
            "http://x/users/7"
        );
        assert_eq!(
            apply("http://x/u/:id/n/:name", &rows).expect("ok"),
            "http://x/u/7/n/a%20b%2Fc%3Fd"
        );
        // Query preserved, unused rows ignored.
        assert_eq!(
            apply("{{baseUrl}}/users/:id?page=1", &rows).expect("ok"),
            "{{baseUrl}}/users/7?page=1"
        );
    }

    #[test]
    fn apply_errors_on_missing_value() {
        let rows = vec![("other".to_string(), "1".to_string())];
        let err = apply("http://x/users/:id", &rows).expect_err("missing");
        assert!(err.contains("id"), "{err}");
        // No params at all → rows irrelevant, always ok.
        assert_eq!(
            apply("http://x/users", &rows).expect("ok"),
            "http://x/users"
        );
    }

    #[test]
    fn apply_leaves_odata_segments_untouched() {
        let rows = vec![("key".to_string(), "1".to_string())];
        assert_eq!(
            apply("http://x/Products(key)/Details", &rows).expect("ok"),
            "http://x/Products(key)/Details"
        );
    }
}
