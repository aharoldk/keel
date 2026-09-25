//! End-to-end flow for the `keel-cli` binary: temp workspaces against
//! wiremock, asserting exit codes and the exact output line shapes.

use std::path::Path;
use std::process::Command;

use keel_lib::model::{
    yaml_of, CollectionDoc, EnvDoc, HttpMethod, RequestBlock, RequestDoc, TestAssertion,
    SCHEMA_VERSION,
};
use serde_json::Value;

fn keel() -> Command {
    let path = std::env::var("CARGO_BIN_EXE_keel-cli")
        .or_else(|_| std::env::var("CARGO_BIN_EXE_keel_cli"))
        .expect("keel-cli binary is built for integration tests");
    Command::new(path)
}

fn req(name: &str, url: &str, status_wanted: i64) -> RequestDoc {
    RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: name.into(),
        kind: "request".into(),
        description: None,
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: RequestBlock {
            method: HttpMethod::GET,
            url: url.into(),
            ..RequestBlock::default()
        },
        auth: None,
        variables: None,
        scripts: None,
        tests: Some(vec![TestAssertion {
            expect: "response.status".into(),
            matcher: [("toBe".to_string(), Value::from(status_wanted))]
                .into_iter()
                .collect(),
        }]),
    }
}

fn workspace(root: &Path, base_url: &str) {
    std::fs::write(
        root.join("collection.yaml"),
        yaml_of(&CollectionDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: "CLI Test".into(),
            description: None,
            variables: None,
            default_environment: Some("test".into()),
            auth: None,
            headers: None,
            scripts: None,
            order: None,
        })
        .unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("environments")).unwrap();
    std::fs::write(
        root.join("environments/test.yaml"),
        yaml_of(&EnvDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: "test".into(),
            description: None,
            variables: Some(
                [("baseUrl".to_string(), base_url.to_string())]
                    .into_iter()
                    .collect(),
            ),
            secrets: None,
        })
        .unwrap(),
    )
    .unwrap();
}

fn run(cwd: &Path, args: &[&str]) -> std::process::Output {
    keel()
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("keel-cli executes")
}

#[tokio::test]
async fn run_folder_passes() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    workspace(&root, &server.uri());
    std::fs::create_dir_all(root.join("smoke")).unwrap();
    std::fs::write(
        root.join("smoke/one.yaml"),
        yaml_of(&req("One", "{{baseUrl}}/one".into(), 200)).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("smoke/two.yaml"),
        yaml_of(&req("Two", "{{baseUrl}}/two".into(), 200)).unwrap(),
    )
    .unwrap();

    let out = run(&root, &["run", "smoke", "--env", "test"]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.code() == Some(0), "exit: {out:?}\n{stdout}");
    assert!(stdout.contains("PASS  GET  {{baseUrl}}/one"), "{stdout}");
    assert!(stdout.contains("PASS  GET  {{baseUrl}}/two"), "{stdout}");
    assert!(
        stdout.contains("Total 2 · Passed 2 · Failed 0 · Errored 0 · Skipped 0"),
        "{stdout}"
    );
    assert!(stdout.contains("1/1 tests"), "{stdout}");
}

#[tokio::test]
async fn failing_test_exits_1() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    workspace(&root, &server.uri());
    std::fs::write(
        root.join("bad.yaml"),
        yaml_of(&req("Bad", "{{baseUrl}}/bad".into(), 200)).unwrap(),
    )
    .unwrap();

    let out = run(&root, &["run", "bad.yaml", "--env", "test"]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(1), "{out:?}\n{stdout}");
    assert!(stdout.contains("FAIL  GET  {{baseUrl}}/bad  500"), "{stdout}");
}

#[test]
fn unknown_path_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    workspace(&root, "http://127.0.0.1:1");
    let out = run(&root, &["run", "nope.yaml"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(stderr.contains("nope.yaml"), "{stderr}");
}

#[test]
fn version_list_and_help() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    workspace(&root, "http://127.0.0.1:1");
    std::fs::create_dir_all(root.join("users")).unwrap();
    std::fs::write(
        root.join("users/list.yaml"),
        yaml_of(&req("List Users", "{{baseUrl}}/users".into(), 200)).unwrap(),
    )
    .unwrap();

    let out = run(&root, &["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")),
        "version prints"
    );

    let out = run(&root, &["list"]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("users/"), "{stdout}");
    assert!(stdout.contains("GET"), "{stdout}");
    assert!(stdout.contains("List Users"), "{stdout}");

    let out = run(&root, &["--help"]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn import_curl_creates_request() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    workspace(&root, "http://127.0.0.1:1");
    let out = run(
        &root,
        &[
            "import",
            "curl",
            "curl -X POST https://api.example.com/items -H 'Content-Type: application/json' -d '{\"a\":1}'",
            "--folder",
            "imported",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{out:?}\n{stdout}");
    let rel = stdout.trim();
    assert!(rel.starts_with("imported/"), "{rel}");
    assert!(root.join(rel).is_file(), "file written");
    let text = std::fs::read_to_string(root.join(rel)).unwrap();
    assert!(text.contains("POST"), "{text}");
}
