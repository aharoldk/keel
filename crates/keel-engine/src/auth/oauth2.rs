//! OAuth2 token acquisition + in-memory caching.
//!
//! No Tauri in this module: the browser-open side effect is injected via
//! `open_browser`. The authorization-code flow runs a local HTTP callback
//! listener on the port parsed from `callbackUrl`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use rand::Rng as _;
use sha2::{Digest as _, Sha256};

use crate::model::{Auth, AuthType, OAuth2GrantType};

/// A token held in the in-memory cache (never persisted to disk/git).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    /// Absolute wall-clock (monotonic) expiry; `None` = never expires.
    pub expires_at: Option<Instant>,
}

/// Cache keyed by credentials fingerprint. Public field so the integrator can
/// construct/inspect it directly from app state.
#[derive(Debug, Default)]
pub struct Oauth2Cache(pub Mutex<HashMap<String, CachedToken>>);

/// Raw alias kept for contract symmetry (see docs/CONTRACT_V2.md).
pub type OAuth2Cache = Mutex<HashMap<String, CachedToken>>;

/// Fully-resolved OAuth2 settings (all `{{vars}}` already interpolated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oauth2Config {
    /// "clientCredentials" | "password" | "authorizationCode"
    pub grant_type: String,
    pub access_token_url: String,
    pub refresh_token_url: Option<String>,
    pub authorization_url: Option<String>,
    pub callback_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub state: Option<String>,
    pub pkce: bool,
    /// "body" (default) | "basicAuthHeader"
    pub credentials_placement: String,
    /// "header" (default) | "query"
    pub token_placement: String,
    pub token_header_prefix: String,
    pub token_query_key: String,
}

impl Oauth2Config {
    /// Builds a config from a resolved [`Auth`] record. Returns `None` when
    /// the auth type is not OAuth2 or required fields are missing.
    pub fn from_auth(a: &Auth) -> Option<Self> {
        if a.auth_type != AuthType::Oauth2 {
            return None;
        }
        let grant_type = match a.grant_type? {
            OAuth2GrantType::ClientCredentials => "clientCredentials",
            OAuth2GrantType::Password => "password",
            OAuth2GrantType::AuthorizationCode => "authorizationCode",
        };
        let access_token_url = a.access_token_url.clone().filter(|s| !s.trim().is_empty())?;
        let authorization_url = a.authorization_url.clone().filter(|s| !s.trim().is_empty());
        let callback_url = a.callback_url.clone().filter(|s| !s.trim().is_empty());
        if grant_type == "authorizationCode" {
            authorization_url.as_ref()?;
            callback_url.as_ref()?;
        }
        Some(Self {
            grant_type: grant_type.to_string(),
            access_token_url,
            refresh_token_url: a.refresh_token_url.clone().filter(|s| !s.trim().is_empty()),
            authorization_url,
            callback_url,
            client_id: a.client_id.clone().unwrap_or_default(),
            client_secret: a.client_secret.clone().filter(|s| !s.trim().is_empty()),
            scope: a.scope.clone().filter(|s| !s.trim().is_empty()),
            username: a.username.clone().filter(|s| !s.trim().is_empty()),
            password: a.password.clone().filter(|s| !s.trim().is_empty()),
            state: a.state.clone().filter(|s| !s.trim().is_empty()),
            pkce: a.pkce,
            credentials_placement: a
                .credentials_placement
                .clone()
                .unwrap_or_else(|| "body".to_string()),
            token_placement: a
                .token_placement
                .clone()
                .unwrap_or_else(|| "header".to_string()),
            token_header_prefix: a
                .token_header_prefix
                .clone()
                .unwrap_or_else(|| "Bearer".to_string()),
            token_query_key: a
                .token_query_key
                .clone()
                .unwrap_or_else(|| "access_token".to_string()),
        })
    }

    /// sha256 hex over (tokenUrl|clientId|scope|username|grantType).
    pub fn fingerprint(&self) -> String {
        let joined = format!(
            "{}|{}|{}|{}|{}",
            self.access_token_url,
            self.client_id,
            self.scope.clone().unwrap_or_default(),
            self.username.clone().unwrap_or_default(),
            self.grant_type,
        );
        let digest = Sha256::digest(joined.as_bytes());
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Returns a valid cached token or fetches a fresh one. Refresh-token flow is
/// used automatically when the cached token expired and carries a refresh
/// token.
pub async fn get_token(
    cfg: &Oauth2Config,
    cache: &Oauth2Cache,
    open_browser: &(dyn Fn(&str) -> Result<(), String> + Send + Sync),
) -> Result<CachedToken, String> {
    let key = cfg.fingerprint();
    let cached = cache_lock(cache).get(&key).cloned();

    if let Some(tok) = &cached {
        if is_valid(tok) {
            return Ok(tok.clone());
        }
    }

    // Expired token that carries a refresh token → refresh grant.
    if let Some(refresh_token) = cached.as_ref().and_then(|t| t.refresh_token.clone()) {
        let form = refresh_form(cfg, &refresh_token);
        let tok =
            request_token(cfg, &form, auth_header_for(cfg)).await?;
        let tok = merge_refresh(tok, Some(refresh_token));
        cache_lock(cache).insert(key.clone(), tok.clone());
        return Ok(tok);
    }

    let tok = match cfg.grant_type.as_str() {
        "clientCredentials" => {
            let mut form = vec![("grant_type".to_string(), "client_credentials".to_string())];
            push_client_credentials(cfg, &mut form);
            if let Some(scope) = &cfg.scope {
                form.push(("scope".to_string(), scope.clone()));
            }
            request_token(cfg, &form, auth_header_for(cfg)).await?
        }
        "password" => {
            let mut form = vec![("grant_type".to_string(), "password".to_string())];
            push_client_credentials(cfg, &mut form);
            form.push((
                "username".to_string(),
                cfg.username.clone().unwrap_or_default(),
            ));
            form.push((
                "password".to_string(),
                cfg.password.clone().unwrap_or_default(),
            ));
            if let Some(scope) = &cfg.scope {
                form.push(("scope".to_string(), scope.clone()));
            }
            request_token(cfg, &form, auth_header_for(cfg)).await?
        }
        "authorizationCode" => {
            let old_refresh = cached.and_then(|t| t.refresh_token);
            let tok = auth_code_flow(cfg, open_browser).await?;
            merge_refresh(tok, old_refresh)
        }
        other => return Err(format!("unsupported OAuth2 grant type: {other}")),
    };

    cache_lock(cache).insert(key, tok.clone());
    Ok(tok)
}

/// Adds the access token to the outgoing request, per `tokenPlacement`.
pub fn apply_token(
    cfg: &Oauth2Config,
    tok: &CachedToken,
    headers: &mut Vec<(String, String)>,
    query: &mut Vec<(String, String)>,
) {
    if cfg.token_placement == "query" {
        query.push((cfg.token_query_key.clone(), tok.access_token.clone()));
    } else if cfg.token_header_prefix.is_empty() {
        headers.push(("Authorization".to_string(), tok.access_token.clone()));
    } else {
        headers.push((
            "Authorization".to_string(),
            format!("{} {}", cfg.token_header_prefix, tok.access_token),
        ));
    }
}

// ---------- internals ----------

const EXPIRY_SKEW: Duration = Duration::from_secs(30);

fn cache_lock(cache: &Oauth2Cache) -> std::sync::MutexGuard<'_, HashMap<String, CachedToken>> {
    cache.0.lock().unwrap_or_else(|e| e.into_inner())
}

fn is_valid(tok: &CachedToken) -> bool {
    match tok.expires_at {
        None => true,
        Some(at) => Instant::now() + EXPIRY_SKEW < at,
    }
}

fn merge_refresh(mut tok: CachedToken, old: Option<String>) -> CachedToken {
    if tok.refresh_token.is_none() {
        tok.refresh_token = old;
    }
    tok
}

fn auth_header_for(cfg: &Oauth2Config) -> Option<(String, String)> {
    if cfg.credentials_placement != "basicAuthHeader" {
        return None;
    }
    let raw = format!("{}:{}", cfg.client_id, cfg.client_secret.clone().unwrap_or_default());
    Some((
        "Authorization".to_string(),
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        ),
    ))
}

fn refresh_form(cfg: &Oauth2Config, refresh_token: &str) -> Vec<(String, String)> {
    let mut form = vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        ("refresh_token".to_string(), refresh_token.to_string()),
    ];
    if cfg.credentials_placement != "basicAuthHeader" && !cfg.client_id.is_empty() {
        form.push(("client_id".to_string(), cfg.client_id.clone()));
        if let Some(secret) = &cfg.client_secret {
            form.push(("client_secret".to_string(), secret.clone()));
        }
    }
    form
}

fn token_endpoint(cfg: &Oauth2Config) -> String {
    cfg.refresh_token_url
        .clone()
        .unwrap_or_else(|| cfg.access_token_url.clone())
}

/// Adds `client_id`/`client_secret` body pairs unless they travel in a Basic
/// auth header instead.
fn push_client_credentials(cfg: &Oauth2Config, form: &mut Vec<(String, String)>) {
    if cfg.credentials_placement == "basicAuthHeader" {
        return;
    }
    form.push(("client_id".to_string(), cfg.client_id.clone()));
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret".to_string(), secret.clone()));
    }
}

/// Chooses the endpoint: refreshes go to refreshTokenUrl (fallback
/// accessTokenUrl); other grants always use accessTokenUrl.
fn token_endpoint_for(cfg: &Oauth2Config, form: &[(String, String)]) -> String {
    let is_refresh = form
        .iter()
        .any(|(k, v)| k == "grant_type" && v == "refresh_token");
    if is_refresh {
        token_endpoint(cfg)
    } else {
        cfg.access_token_url.clone()
    }
}

async fn request_token(
    cfg: &Oauth2Config,
    form: &[(String, String)],
    extra_auth: Option<(String, String)>,
) -> Result<CachedToken, String> {
    post_form(&token_endpoint_for(cfg, form), form, extra_auth).await
}

async fn post_form(
    url: &str,
    form: &[(String, String)],
    extra_auth: Option<(String, String)>,
) -> Result<CachedToken, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("http client error: {e}"))?;
    let mut req = client
        .post(url)
        .header("accept", "application/json")
        .form(form);
    if let Some((name, value)) = extra_auth {
        req = req.header(name, value);
    }
    let response = req
        .send()
        .await
        .map_err(|e| format!("OAuth2 token request failed: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("OAuth2 token read failed: {e}"))?;
    if !status.is_success() {
        let snippet: String = text.chars().take(300).collect();
        return Err(format!(
            "OAuth2 token request failed: HTTP {status} {snippet}"
        ));
    }
    parse_token_json(&text)
}

fn parse_token_json(text: &str) -> Result<CachedToken, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("invalid OAuth2 token JSON: {e}"))?;
    let access_token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            let err = json
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("no access_token in response");
            format!("OAuth2 error: {err}")
        })?
        .to_string();
    let refresh_token = json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let token_type = json
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();
    let expires_at = parse_expires_in(json.get("expires_in"))
        .map(|secs| Instant::now() + Duration::from_secs(secs));
    Ok(CachedToken {
        access_token,
        refresh_token,
        token_type,
        expires_at,
    })
}

fn parse_expires_in(value: Option<&serde_json::Value>) -> Option<u64> {
    match value? {
        serde_json::Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        serde_json::Value::String(s) => {
            s.trim().parse::<u64>().ok().or_else(|| s.trim().parse::<f64>().ok().map(|f| f.max(0.0) as u64))
        }
        _ => None,
    }
}

/// Full authorization-code (+PKCE) exchange with a local loopback callback.
async fn auth_code_flow(
    cfg: &Oauth2Config,
    open_browser: &(dyn Fn(&str) -> Result<(), String> + Send + Sync),
) -> Result<CachedToken, String> {
    let auth_url = cfg
        .authorization_url
        .as_ref()
        .ok_or("authorizationCode grant requires an authorizationUrl")?;
    let callback = cfg
        .callback_url
        .as_ref()
        .ok_or("authorizationCode grant requires a callbackUrl")?;

    // PKCE verifier + challenge.
    let verifier = if cfg.pkce {
        let mut bytes = [0u8; 64];
        rand::rng().fill(&mut bytes);
        Some(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(bytes),
        )
    } else {
        None
    };
    let challenge = verifier.as_ref().map(|v| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes()))
    });
    let state = cfg
        .state
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Bind the loopback listener on the callback URL's port (port 0 = OS picks).
    let mut cb =
        reqwest::Url::parse(callback).map_err(|e| format!("invalid callbackUrl: {e}"))?;
    let port = cb
        .port()
        .ok_or("callbackUrl must include a port, e.g. http://localhost:53682/cb")?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("cannot bind OAuth2 callback port {port}: {e}"))?;
    if port == 0 {
        let bound = listener
            .local_addr()
            .map_err(|e| format!("callback bind error: {e}"))?
            .port();
        cb.set_port(Some(bound))
            .map_err(|_| "cannot rebuild callbackUrl".to_string())?;
    }
    let redirect_uri = cb.to_string();

    // Authorization URL with response params.
    let mut auth =
        reqwest::Url::parse(auth_url).map_err(|e| format!("invalid authorizationUrl: {e}"))?;
    {
        let mut q = auth.query_pairs_mut();
        q.append_pair("client_id", &cfg.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code");
        if let Some(scope) = &cfg.scope {
            q.append_pair("scope", scope);
        }
        q.append_pair("state", &state);
        if let Some(ch) = &challenge {
            q.append_pair("code_challenge", ch)
                .append_pair("code_challenge_method", "S256");
        }
    }
    let auth_url = auth.to_string();

    open_browser(&auth_url).map_err(|e| format!("cannot open browser: {e}"))?;

    let code = wait_for_callback(listener, &state).await?;

    // Exchange the code.
    let mut form = vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code),
        ("redirect_uri".to_string(), redirect_uri),
    ];
    push_client_credentials(cfg, &mut form);
    if let Some(v) = &verifier {
        form.push(("code_verifier".to_string(), v.clone()));
    }
    post_form(&cfg.access_token_url, &form, auth_header_for(cfg)).await
}

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(120);

async fn wait_for_callback(
    listener: tokio::net::TcpListener,
    state: &str,
) -> Result<String, String> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for the OAuth2 authorization callback".into());
        }
        let accepted = match tokio::time::timeout(remaining, listener.accept()).await {
            Ok(r) => r.map_err(|e| format!("OAuth2 callback listen error: {e}"))?,
            Err(_) => {
                return Err(
                    "timed out waiting for the OAuth2 authorization callback".into(),
                )
            }
        };
        let (mut stream, _) = accepted;
        // Read the request head (up to the blank line, capped at 64 KiB).
        let mut buf: Vec<u8> = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(10), async {
            let mut chunk = [0u8; 1024];
            loop {
                use tokio::io::AsyncReadExt;
                let n = stream.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 65536 {
                    break;
                }
            }
            Ok::<(), std::io::Error>(())
        })
        .await;
        if read.is_err() || buf.is_empty() {
            continue;
        }
        let head = String::from_utf8_lossy(&buf).into_owned();
        let target = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");
        let (_, query) = match target.split_once('?') {
            Some(v) => v,
            None => {
                let _ = respond_ok(stream).await;
                continue;
            }
        };
        let pairs: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        if let Some(err) = pairs.get("error") {
            let _ = respond_ok(stream).await;
            return Err(format!("OAuth2 authorization denied: {err}"));
        }
        let code = pairs.get("code");
        let st = pairs.get("state");
        match (code, st) {
            (Some(code), Some(st)) if st == state => {
                let _ = respond_ok(stream).await;
                return Ok(code.clone());
            }
            _ => {
                // Unrelated request (favicon, wrong state): ignore and wait.
                let _ = respond_ok(stream).await;
                continue;
            }
        }
    }
}

async fn respond_ok(mut stream: tokio::net::TcpStream) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    const BODY: &str = "<html><body><h2>You may close this window.</h2></body></html>";
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{BODY}",
        BODY.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OAuth2GrantType;

    fn base_cfg(url: String) -> Oauth2Config {
        Oauth2Config {
            grant_type: "clientCredentials".into(),
            access_token_url: url,
            refresh_token_url: None,
            authorization_url: None,
            callback_url: None,
            client_id: "cid".into(),
            client_secret: Some("s3cret".into()),
            scope: Some("read write".into()),
            username: None,
            password: None,
            state: None,
            pkce: false,
            credentials_placement: "body".into(),
            token_placement: "header".into(),
            token_header_prefix: "Bearer".into(),
            token_query_key: "access_token".into(),
        }
    }

    fn auth_with(f: impl FnOnce(&mut Auth)) -> Auth {
        let mut a = Auth {
            auth_type: AuthType::Oauth2,
            ..Auth::default()
        };
        f(&mut a);
        a
    }

    fn form_of(body: &[u8]) -> HashMap<String, String> {
        url::form_urlencoded::parse(body)
            .into_owned()
            .collect()
    }

    fn refuse_browser(_url: &str) -> Result<(), String> {
        Err("must not open a browser".to_string())
    }

    #[test]
    fn from_auth_gating_and_defaults() {
        assert!(Oauth2Config::from_auth(&Auth::default()).is_none());
        let none = Oauth2Config::from_auth(&auth_with(|a| {
            a.grant_type = Some(OAuth2GrantType::ClientCredentials);
        }));
        assert!(none.is_none(), "missing accessTokenUrl");
        let cfg = Oauth2Config::from_auth(&auth_with(|a| {
            a.grant_type = Some(OAuth2GrantType::ClientCredentials);
            a.access_token_url = Some("http://x/token".into());
            a.client_id = Some("cid".into());
        }))
        .expect("ok");
        assert_eq!(cfg.grant_type, "clientCredentials");
        assert_eq!(cfg.credentials_placement, "body");
        assert_eq!(cfg.token_header_prefix, "Bearer");
        assert_eq!(cfg.token_query_key, "access_token");
        // authorizationCode needs authorizationUrl + callbackUrl.
        assert!(Oauth2Config::from_auth(&auth_with(|a| {
            a.grant_type = Some(OAuth2GrantType::AuthorizationCode);
            a.access_token_url = Some("http://x/token".into());
        }))
        .is_none());
    }

    #[test]
    fn fingerprint_stable_and_scoped() {
        let a = base_cfg("http://x/token".into());
        let mut b = a.clone();
        assert_eq!(a.fingerprint(), b.fingerprint());
        b.scope = Some("other".into());
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.fingerprint().len(), 64);
    }

    #[tokio::test]
    async fn client_credentials_happy_path_and_cache() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(
                        r#"{"access_token":"tok1","refresh_token":"r1","expires_in":3600,"token_type":"Bearer"}"#,
                    ),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut cfg = base_cfg(format!("{}/token", server.uri()));
        cfg.grant_type = "password".into();
        cfg.username = Some("u1".into());
        cfg.password = Some("p1".into());
        let cache = Oauth2Cache::default();

        // First call fetches; password grant body + scope present.
        let tok1 = get_token(&cfg, &cache, &refuse_browser).await.expect("token");
        assert_eq!(tok1.access_token, "tok1");
        assert_eq!(tok1.refresh_token.as_deref(), Some("r1"));
        let received = server.received_requests().await.unwrap();
        let form = form_of(&received[0].body);
        assert_eq!(form.get("grant_type").map(String::as_str), Some("password"));
        assert_eq!(form.get("client_id").map(String::as_str), Some("cid"));
        assert_eq!(form.get("client_secret").map(String::as_str), Some("s3cret"));
        assert_eq!(form.get("username").map(String::as_str), Some("u1"));
        assert_eq!(form.get("scope").map(String::as_str), Some("read write"));
        assert_eq!(
            received[0].headers.get("accept").and_then(|v| v.to_str().ok()),
            Some("application/json")
        );

        // Second call is served from the cache (mock expects exactly 1 hit).
        let tok2 = get_token(&cfg, &cache, &refuse_browser).await.expect("cached");
        assert_eq!(tok2, tok1);
    }

    #[tokio::test]
    async fn refresh_on_expiry() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/refresh"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(r#"{"access_token":"tok2","expires_in":3600}"#),
            )
            .mount(&server)
            .await;
        let mut cfg = base_cfg(format!("{}/token", server.uri()));
        cfg.refresh_token_url = Some(format!("{}/refresh", server.uri()));
        let cache = Oauth2Cache(Mutex::new(HashMap::new()));
        cache.0.lock().unwrap().insert(
            cfg.fingerprint(),
            CachedToken {
                access_token: "old".into(),
                refresh_token: Some("r1".into()),
                token_type: "Bearer".into(),
                expires_at: Some(Instant::now() - Duration::from_secs(60)),
            },
        );
        let tok = get_token(&cfg, &cache, &|_| Ok(()))
            .await
            .expect("refreshed");
        assert_eq!(tok.access_token, "tok2");
        // Refresh response omitted refresh_token → old one is kept.
        assert_eq!(tok.refresh_token.as_deref(), Some("r1"));
        let form = form_of(&server.received_requests().await.unwrap()[0].body);
        assert_eq!(
            form.get("grant_type").map(String::as_str),
            Some("refresh_token")
        );
        assert_eq!(form.get("refresh_token").map(String::as_str), Some("r1"));
    }

    #[tokio::test]
    async fn basic_auth_header_placement() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(r#"{"access_token":"tok3","expires_in":60}"#),
            )
            .mount(&server)
            .await;
        let mut cfg = base_cfg(format!("{}/token", server.uri()));
        cfg.credentials_placement = "basicAuthHeader".into();
        let cache = Oauth2Cache::default();
        let tok = get_token(&cfg, &cache, &|_| Ok(()))
            .await
            .expect("token");
        assert_eq!(tok.access_token, "tok3");
        let received = server.received_requests().await.unwrap();
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("cid:s3cret")
        );
        assert_eq!(
            received[0].headers.get("authorization").and_then(|v| v.to_str().ok()),
            Some(expected.as_str())
        );
        let form = form_of(&received[0].body);
        assert!(form.get("client_secret").is_none(), "secret must not be in body");
    }

    #[tokio::test]
    async fn error_propagation() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(400)
                    .insert_header("content-type", "application/json")
                    .set_body_string(r#"{"error":"invalid_client","error_description":"nope"}"#),
            )
            .mount(&server)
            .await;
        let cfg = base_cfg(format!("{}/token", server.uri()));
        let cache = Oauth2Cache::default();
        let err = get_token(&cfg, &cache, &|_| Ok(()))
            .await
            .expect_err("must fail");
        assert!(err.contains("400"), "{err}");
        assert!(err.contains("invalid_client"), "{err}");
        assert!(cache.0.lock().unwrap().is_empty(), "nothing cached on error");
    }

    #[tokio::test]
    async fn auth_code_flow_with_pkce() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(
                        r#"{"access_token":"tok-code","expires_in":3600,"refresh_token":"r-code"}"#,
                    ),
            )
            .mount(&server)
            .await;
        let mut cfg = base_cfg(format!("{}/token", server.uri()));
        cfg.grant_type = "authorizationCode".into();
        cfg.authorization_url = Some(format!("{}/authorize", server.uri()));
        cfg.callback_url = Some("http://127.0.0.1:0/cb".into());
        cfg.pkce = true;

        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let seen2 = seen.clone();
        let opener = move |url: &str| -> Result<(), String> {
            seen2.lock().unwrap().push_str(url);
            let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
            let q: HashMap<String, String> =
                parsed.query_pairs().into_owned().collect();
            let state = q.get("state").cloned().ok_or("no state")?;
            let redirect = q.get("redirect_uri").cloned().ok_or("no redirect")?;
            // Deliver the code like a browser would (bad state first).
            tokio::spawn(async move {
                let client = reqwest::Client::new();
                let _ = client
                    .get(format!("{redirect}?code=ignored&state=bogus"))
                    .send()
                    .await;
                let _ = client
                    .get(format!("{redirect}?code=good-code&state={state}"))
                    .send()
                    .await;
            });
            Ok(())
        };
        let cache = Oauth2Cache::default();
        let tok = get_token(&cfg, &cache, &opener).await.expect("auth-code");
        assert_eq!(tok.access_token, "tok-code");

        let auth = seen.lock().unwrap().clone();
        assert!(auth.contains("response_type=code"), "{auth}");
        assert!(auth.contains("code_challenge_method=S256"), "{auth}");
        let parsed = reqwest::Url::parse(&auth).expect("url");
        let q: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
        let verifier_hint = q.get("code_challenge").expect("challenge");
        assert_eq!(verifier_hint.len(), 43, "S256 challenge is 43 chars unpadded");

        let received = server.received_requests().await.unwrap();
        let token_req = received
            .iter()
            .find(|r| r.url.path() == "/token")
            .expect("token exchange");
        let form = form_of(&token_req.body);
        assert_eq!(
            form.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
        assert_eq!(form.get("code").map(String::as_str), Some("good-code"));
        assert_eq!(
            form.get("redirect_uri").map(String::as_str),
            Some(q.get("redirect_uri").expect("redirect_uri in auth url").as_str())
        );
        let verifier = form.get("code_verifier").expect("code_verifier");
        assert!(verifier.len() >= 43, "verifier length: {}", verifier.len());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(expected, *verifier_hint, "challenge = S256(verifier)");

        // Second call uses the cache, no second browser round-trip.
        let again = get_token(&cfg, &cache, &|_| Err("should not open".into()))
            .await
            .expect("cached");
        assert_eq!(again, tok);
    }

    #[tokio::test]
    async fn auth_code_denied_by_user() {
        let server = wiremock::MockServer::start().await;
        let mut cfg = base_cfg(format!("{}/token", server.uri()));
        cfg.grant_type = "authorizationCode".into();
        cfg.authorization_url = Some(format!("{}/authorize", server.uri()));
        cfg.callback_url = Some("http://127.0.0.1:0/cb".into());
        let cache = Oauth2Cache::default();
        let opener = move |url: &str| -> Result<(), String> {
            let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
            let q: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
            let state = q.get("state").cloned().ok_or("no state")?;
            let redirect = q.get("redirect_uri").cloned().ok_or("no redirect")?;
            tokio::spawn(async move {
                let client = reqwest::Client::new();
                let _ = client
                    .get(format!(
                        "{redirect}?error=access_denied&state={state}&error_description=nope"
                    ))
                    .send()
                    .await;
            });
            Ok(())
        };
        let err = get_token(&cfg, &cache, &opener)
            .await
            .expect_err("denial must propagate");
        assert!(err.contains("access_denied"), "{err}");
        assert!(cache.0.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_token_placements() {
        let mut cfg = base_cfg("http://x/token".into());
        let tok = CachedToken {
            access_token: "abc".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            expires_at: None,
        };
        let mut headers = vec![("accept".to_string(), "*/*".to_string())];
        let mut query = vec![("page".to_string(), "1".to_string())];
        apply_token(&cfg, &tok, &mut headers, &mut query);
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer abc"));
        assert_eq!(query.len(), 1);

        cfg.token_placement = "query".into();
        let mut headers = vec![];
        let mut query = vec![];
        apply_token(&cfg, &tok, &mut headers, &mut query);
        assert!(headers.is_empty());
        assert_eq!(query, vec![("access_token".to_string(), "abc".to_string())]);

        cfg.token_placement = "header".into();
        cfg.token_header_prefix = "Token".into();
        let mut headers = vec![];
        apply_token(&cfg, &tok, &mut headers, &mut query);
        assert_eq!(headers[0].1, "Token abc");
    }

    #[test]
    fn expiry_skew_invalidates_soon_expired_tokens() {
        let tok = CachedToken {
            access_token: "x".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            expires_at: Some(Instant::now() + Duration::from_secs(10)),
        };
        assert!(!is_valid(&tok), "inside 30 s skew → refresh");
        let tok = CachedToken {
            access_token: "x".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            expires_at: Some(Instant::now() + Duration::from_secs(120)),
        };
        assert!(is_valid(&tok));
        let tok = CachedToken {
            access_token: "x".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            expires_at: None,
        };
        assert!(is_valid(&tok), "no expiry → valid");
    }

    #[test]
    fn parse_expires_in_matrix() {
        assert_eq!(parse_expires_in(Some(&serde_json::json!(60))), Some(60));
        assert_eq!(parse_expires_in(Some(&serde_json::json!("60"))), Some(60));
        assert_eq!(parse_expires_in(Some(&serde_json::json!(59.9))), Some(59));
        assert_eq!(parse_expires_in(Some(&serde_json::json!("x"))), None);
        assert_eq!(parse_expires_in(None), None);
    }
}
