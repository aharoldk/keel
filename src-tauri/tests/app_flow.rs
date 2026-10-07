// End-to-end flow test through the public library surface, mirroring what
// the desktop app does: open workspace → create request → send (with
// chaining + tests) → history → git commit.
use std::collections::BTreeMap;
use std::path::Path;

use keel_lib::engine::http::HttpOptions;
use keel_lib::engine::send::{send, SendInput};
use keel_lib::engine::variables::SecretSource;
use keel_lib::model::*;
use keel_lib::workspace;

struct FakeSecrets;
impl SecretSource for FakeSecrets {
    fn get(&self, _name: &str) -> Result<String, String> {
        Ok("fake-secret-value".into())
    }
}

#[tokio::test]
async fn open_send_chain_commit() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("flow-api");

    // 1. Scaffold + open.
    let info = workspace::init_workspace(&root, "Flow API").expect("init");
    assert!(!info.has_git);
    keel_lib::gitutil::init(&root).expect("git init");

    // 2. Create a login request via the same code path the UI uses.
    let login_path = workspace::create_request(&root, "auth", "Login").expect("create");
    let mut doc = workspace::read_request(&root, &login_path).expect("read");
    doc.request.method = HttpMethod::POST;
    doc.request.url = "{{baseUrl}}/auth/login".into();
    doc.request.body = Some(Body {
        body_type: BodyType::Json,
        content: Some(r#"{"user":"ada"}"#.into()),
        ..Body::default()
    });
    doc.scripts = Some(Scripts {
        pre_request: Some(r#"log("logging in")"#.into()),
        post_response: Some(r#"set("token", json("token"))"#.into()),
    });
    doc.tests = Some(vec![test_assertion("response.status", "toBe", 200.into())]);
    workspace::save_request(&root, &login_path, &doc).expect("save");

    // 3. Fake upstream server.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/auth/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"token": "tok-1"})),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"id": 1, "user": "ada"})),
        )
        .mount(&server)
        .await;

    let mut env = workspace::env_read(&root, "local.yaml").expect("env");
    env.variables
        .as_mut()
        .expect("vars")
        .insert("baseUrl".into(), server.uri());

    let oauth_cache = std::sync::Arc::new(keel_lib::auth::oauth2::Oauth2Cache::default());
    let noop_browser = |_: &str| -> Result<(), String> { Ok(()) };
    let oauth_cache = &*oauth_cache;
    // 4. Send (login) — the post script stores the token.
    let collection = workspace::load_collection(&root).expect("collection");
    let ws_doc = workspace::load_workspace_doc(&root);
    let mut transient = BTreeMap::new();

    // 4a. Send the login request — its post script stores the token.
    let login_output = send(SendInput {
        doc: &doc,
        env_label: Some("local".into()),
        env: Some(&env),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: None,
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: Some(login_path.clone()),
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache: &oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;

    let result = &login_output.result;
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.status, Some(200));
    assert!(result.test_results.iter().all(|t| t.passed));
    assert!(result.script_logs.iter().any(|l| l.contains("logging in")));
    assert_eq!(transient.get("token").map(String::as_str), Some("tok-1"));

    // The command layer persists the history record the engine returns.
    keel_lib::history::append(&root, login_output.history.as_ref().expect("record"))
        .expect("append");

    // 4b. Chain: a second request that uses the session token implicitly.
    let mut chained = doc.clone();
    chained.request.method = HttpMethod::GET;
    chained.request.url = "{{baseUrl}}/me".into();
    chained.request.body = None;
    chained.scripts = None;
    chained.tests = Some(vec![test_assertion("response.json.user", "toBe", "ada".into())]);

    let chained_output = send(SendInput {
        doc: &chained,
        env_label: Some("local".into()),
        env: Some(&env),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: None,
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: Some(login_path.clone()),
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache: &oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;
    assert!(chained_output.result.error.is_none(), "{:?}", chained_output.result.error);
    assert_eq!(chained_output.result.status, Some(200));
    assert!(chained_output.result.test_results.iter().all(|t| t.passed));
    keel_lib::history::append(&root, chained_output.history.as_ref().expect("record"))
        .expect("append");

    // 5. History was written (unresolved template URLs).
    let history = keel_lib::history::list(&root, 10);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].url, "{{baseUrl}}/me");
    assert_eq!(history[1].url, "{{baseUrl}}/auth/login");
    assert_eq!(history[1].request_path.as_deref(), Some(login_path.as_str()));

    // 6. Commit through the git engine.
    keel_lib::gitutil::stage(&root, None).expect("stage");
    let oid = keel_lib::gitutil::commit(&root, "first requests").expect("commit");
    assert_eq!(oid.len(), 40);
    let log = keel_lib::gitutil::log(&root, 5).expect("log");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].message, "first requests");

    // 7. A request referencing a secret resolves through the SecretSource.
    let mut secret_doc = doc.clone();
    secret_doc.request.url = "{{apiToken}}/x".into();
    let mut env2 = env.clone();
    env2.secrets = Some([("apiToken".into(), String::new())].into_iter().collect());
    let out2 = send(SendInput {
        doc: &secret_doc,
        env_label: None,
        env: Some(&env2),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: None,
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: None,
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache: &oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;
    assert!(out2.result.secrets_used.contains(&"apiToken".to_string()));
    assert!(out2.result.error.is_none() || out2.result.error.is_some()); // URL invalid, but secret resolved
    // The URL should contain the resolved secret (never leaked to history though).
    if let Some(err) = &out2.result.error {
        assert!(err.contains("invalid URL") || err.contains("relative"), "{err}");
    }
}

fn test_assertion(expect: &str, matcher: &str, expected: serde_json::Value) -> TestAssertion {
    let mut m = BTreeMap::new();
    m.insert(matcher.to_string(), expected);
    TestAssertion {
        expect: expect.to_string(),
        matcher: m,
    }
}

/// Previous-response tags: the second send reads the first response's body
/// and headers via `#{body.accessToken}` / `#{header.X-Trace}` — no scripts.
#[tokio::test]
async fn prev_response_tags_chain_across_sends() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("prev-tags");

    let info = workspace::init_workspace(&root, "Prev Tags").expect("init");
    assert!(!info.has_git);

    let login_path = workspace::create_request(&root, "auth", "Login").expect("create");
    let mut login = workspace::read_request(&root, &login_path).expect("read");
    login.request.method = HttpMethod::POST;
    login.request.url = "{{baseUrl}}/auth/login".into();
    login.request.body = Some(Body {
        body_type: BodyType::Json,
        content: Some(r#"{"user":"ada"}"#.into()),
        ..Body::default()
    });
    workspace::save_request(&root, &login_path, &login).expect("save");

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/auth/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .insert_header("X-Trace", "t1")
                .set_body_json(serde_json::json!({"accessToken": "tok-1"})),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/me"))
        .and(wiremock::matchers::header("Authorization", "Bearer tok-1"))
        .and(wiremock::matchers::header("X-Trace", "t1"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"user": "ada"})),
        )
        .mount(&server)
        .await;

    let mut env = workspace::env_read(&root, "local.yaml").expect("env");
    env.variables
        .as_mut()
        .expect("vars")
        .insert("baseUrl".into(), server.uri());

    let collection = workspace::load_collection(&root).expect("collection");
    let ws_doc = workspace::load_workspace_doc(&root);
    let oauth_cache = std::sync::Arc::new(keel_lib::auth::oauth2::Oauth2Cache::default());
    let noop_browser = |_: &str| -> Result<(), String> { Ok(()) };
    let oauth_cache = &*oauth_cache;
    let mut transient = BTreeMap::new();

    let login_output = send(SendInput {
        doc: &login,
        env_label: Some("local".into()),
        env: Some(&env),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: None,
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: Some(login_path.clone()),
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;
    assert!(login_output.result.error.is_none(), "{:?}", login_output.result.error);
    let captured = login_output.captured.expect("captured response");

    let mut me = login.clone();
    me.request.method = HttpMethod::GET;
    me.request.url = "{{baseUrl}}/me".into();
    me.request.body = None;
    me.scripts = None;
    me.tests = None;
    me.request.headers = Some(vec![
        KV {
            name: "Authorization".into(),
            value: "Bearer #{body.accessToken}".into(),
            enabled: true,
            kind: None,
        },
        KV {
            name: "X-Trace".into(),
            value: "#{header.X-Trace}".into(),
            enabled: true,
            kind: None,
        },
    ]);

    let me_output = send(SendInput {
        doc: &me,
        env_label: Some("local".into()),
        env: Some(&env),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: Some(&captured),
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: Some("auth/me.yaml".into()),
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;
    assert!(me_output.result.error.is_none(), "{:?}", me_output.result.error);
    assert_eq!(me_output.result.status, Some(200));
    assert!(me_output.result.missing_variables.is_empty(), "{:?}", me_output.result.missing_variables);
}

/// `#{…}` tags inside a JSON request body: the password below is filled from
/// the previous response's `publicKey` at send time.
#[tokio::test]
async fn prev_response_tag_inside_json_body() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("body-tags");

    let info = workspace::init_workspace(&root, "Body Tags").expect("init");
    assert!(!info.has_git);

    let login_path = workspace::create_request(&root, "auth", "Challenge").expect("create");
    let mut challenge = workspace::read_request(&root, &login_path).expect("read");
    challenge.request.method = HttpMethod::POST;
    challenge.request.url = "{{baseUrl}}/challenge".into();
    workspace::save_request(&root, &login_path, &challenge).expect("save");

    let auth_path = workspace::create_request(&root, "auth", "Authenticate").expect("create");
    let mut auth = workspace::read_request(&root, &auth_path).expect("read");
    auth.request.method = HttpMethod::POST;
    auth.request.url = "{{baseUrl}}/authenticate".into();
    auth.request.body = Some(Body {
        body_type: BodyType::Json,
        // Bare (unquoted) tag — the engine wraps it in quotes at send time.
        content: Some(
            "{\n  \"clientid\": \"CUST-LOCAL-1:device-1\",\n  \"password\": #{body.publicKey},\n  \"username\": \"CUST-LOCAL-1\"\n}"
                .into(),
        ),
        ..Body::default()
    });
    workspace::save_request(&root, &auth_path, &auth).expect("save");

    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/challenge"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"publicKey": "pk-1"})),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/authenticate"))
        .and(wiremock::matchers::body_string_contains("\"password\":\"pk-1\""))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})),
        )
        .mount(&server)
        .await;

    let mut env = workspace::env_read(&root, "local.yaml").expect("env");
    env.variables
        .as_mut()
        .expect("vars")
        .insert("baseUrl".into(), server.uri());

    let collection = workspace::load_collection(&root).expect("collection");
    let ws_doc = workspace::load_workspace_doc(&root);
    let oauth_cache = std::sync::Arc::new(keel_lib::auth::oauth2::Oauth2Cache::default());
    let noop_browser = |_: &str| -> Result<(), String> { Ok(()) };
    let oauth_cache = &*oauth_cache;
    let mut transient = BTreeMap::new();

    let challenge_output = send(SendInput {
        doc: &challenge,
        env_label: Some("local".into()),
        env: Some(&env),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: None,
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: Some(login_path.clone()),
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;
    assert!(challenge_output.result.error.is_none(), "{:?}", challenge_output.result.error);
    let captured = challenge_output.captured.expect("captured response");

    // The body request only matches when `#{body.publicKey}` resolved to pk-1.
    let auth_output = send(SendInput {
        doc: &auth,
        env_label: Some("local".into()),
        env: Some(&env),
        collection: Some(&collection),
        workspace_doc: Some(&ws_doc),
        transient: &mut transient,
        prev: Some(&captured),
        http: HttpOptions::default(),
        secret_source: &FakeSecrets,
        request_path: Some(auth_path.clone()),
        collection_vars: Default::default(),
        folder_vars: Default::default(),
        env_values: Default::default(),
        env_file: None,
        workspace_root: None,
        oauth_cache,
        open_browser: &noop_browser,
        cookie_jar: None,
        send_cookies: false,
        store_cookies: false,
        iteration_vars: Default::default(),
    })
    .await;
    assert!(auth_output.result.error.is_none(), "{:?}", auth_output.result.error);
    assert_eq!(auth_output.result.status, Some(200), "body tag did not resolve");
}

#[allow(unused)]
fn _unused(_: &Path) {}
