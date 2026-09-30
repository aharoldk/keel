//! End-to-end tests for the crate: write a workspace from typed documents,
//! read it back, resolve inheritance and interpolate variables.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use keel_engine::inherit;
use keel_engine::model::{
    from_path, to_path, Auth, AuthType, CollectionDoc, EnvDoc, EnvValuesDoc, FlowDoc, FolderDoc,
    RequestDoc, Scripts, WorkspaceDoc, KV, SCHEMA_VERSION,
};
use keel_engine::variables::{Interpolator, MapSecrets, Scope, ScopeStack};
use tempfile::TempDir;

/// A checked-in copy of the Keel demo workspace, kept as a format
/// conformance fixture. Edit `examples/demo-workspace` in the app repo and
/// re-copy it here when the format changes.
fn demo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/demo-workspace")
}

#[test]
fn parses_the_demo_workspace_fixture() {
    let root = demo_root();

    let collection: CollectionDoc = from_path(root.join("collection.yaml")).expect("collection");
    assert_eq!(collection.name, "Demo API");
    assert_eq!(collection.default_environment.as_deref(), Some("local"));
    assert_eq!(
        collection
            .variables
            .unwrap()
            .get("userId")
            .map(String::as_str),
        Some("1")
    );

    let request: RequestDoc = from_path(root.join("users/get-user.yaml")).expect("request");
    assert_eq!(request.request.method.as_str(), "GET");
    assert_eq!(request.request.url, "{{baseUrl}}/users/{{userId}}");
    assert_eq!(request.tests.unwrap().len(), 2);

    let chain = inherit::build(&root, "users/get-user.yaml");
    assert_eq!(
        chain.auth.as_ref().map(|a| a.auth_type),
        Some(AuthType::Bearer)
    );
    assert_eq!(chain.source.as_deref(), Some("users"));
    assert_eq!(
        chain.headers,
        vec![KV::new("X-Client", "keel-demo")],
        "folder headers inherited"
    );
    assert_eq!(
        chain.merged_variables().get("userId").map(String::as_str),
        Some("1"),
        "collection variables inherited"
    );

    let env: EnvDoc = from_path(root.join("environments/local.yaml")).expect("env");
    assert_eq!(
        env.secrets.unwrap().get("apiToken").map(String::as_str),
        Some("local-dev-token")
    );

    let flow: FlowDoc = from_path(root.join("flows/user-lifecycle.yaml")).expect("flow");
    assert_eq!(flow.kind, "flow");
    assert_eq!(flow.steps.len(), 5);
    assert_eq!(flow.steps[0].path(), "auth/login.yaml");
    assert!(flow.steps[0].stops_on_failure());
    assert_eq!(flow.steps[4].path(), "users/delete-user.yaml");
    assert!(!flow.steps[4].stops_on_failure(), "onFailure: continue");

    let workspace: WorkspaceDoc = from_path(root.join(".keel/workspace.yaml")).expect("workspace");
    assert_eq!(workspace.schema_version, SCHEMA_VERSION);
}

#[test]
fn generated_workspace_round_trips() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let collection = CollectionDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: "Demo API".into(),
        description: Some("demo".into()),
        variables: Some(BTreeMap::from([("userId".into(), "7".into())])),
        default_environment: Some("local".into()),
        auth: None,
        headers: None,
        scripts: None,
        order: None,
    };
    to_path(root.join("collection.yaml"), &collection).unwrap();

    let folder = FolderDoc {
        schema_version: SCHEMA_VERSION.into(),
        kind: Some("folder".into()),
        name: Some("Users".into()),
        description: None,
        variables: Some(BTreeMap::from([("tenant".into(), "acme".into())])),
        auth: Some(Auth {
            auth_type: AuthType::Bearer,
            token: Some("{{accessToken}}".into()),
            ..Auth::default()
        }),
        headers: Some(vec![KV::new("X-Client", "keel-demo")]),
        scripts: Some(Scripts {
            pre_request: Some("log('before')".into()),
            post_response: Some("log('after')".into()),
        }),
        order: None,
    };
    to_path(root.join("users/folder.yaml"), &folder).unwrap();

    let request = RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: "Get User".into(),
        kind: "request".into(),
        description: None,
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: keel_engine::model::RequestBlock {
            method: keel_engine::model::HttpMethod::GET,
            url: "{{baseUrl}}/{{tenant}}/users/{{userId}}".into(),
            ..Default::default()
        },
        auth: None,
        variables: Some(BTreeMap::from([("userId".into(), "7".into())])),
        scripts: None,
        tests: None,
    };
    to_path(root.join("users/get-user.yaml"), &request).unwrap();

    let env = EnvDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: "local".into(),
        description: None,
        variables: Some(BTreeMap::from([(
            "baseUrl".into(),
            "http://localhost:8080".into(),
        )])),
        secrets: Some(BTreeMap::from([(
            "accessToken".into(),
            "committed-default".into(),
        )])),
    };
    to_path(root.join("environments/local.yaml"), &env).unwrap();

    let re_read: RequestDoc = from_path(root.join("users/get-user.yaml")).unwrap();
    assert_eq!(re_read, request, "document round-trips byte-stably");
    let collection_read: CollectionDoc = from_path(root.join("collection.yaml")).unwrap();
    assert_eq!(collection_read, collection);

    let chain = inherit::build(root, "users/get-user.yaml");
    assert_eq!(
        chain.auth.as_ref().map(|a| a.auth_type),
        Some(AuthType::Bearer),
        "folder auth wins"
    );
    assert_eq!(chain.headers, vec![KV::new("X-Client", "keel-demo")]);
    assert_eq!(chain.pre_scripts, vec!["log('before')"]);
    assert_eq!(chain.post_scripts, vec!["log('after')"]);

    // Scope stack, lowest → highest: workspace, environment (with a secret),
    // collection+folders (inherited), request.
    let mut scopes = ScopeStack::default();
    scopes.push(Scope::from_pairs([("workspaceName", "demo")]));

    let mut env_scope = Scope::new();
    env_scope.insert("baseUrl", "http://localhost:8080");
    env_scope.insert("accessToken", "committed-default");
    env_scope.declare_secret("accessToken");
    scopes.push(env_scope);

    scopes.push(Scope::from_map(&chain.merged_variables()));
    scopes.push(Scope::from_map(
        &request.variables.clone().unwrap_or_default(),
    ));

    let secrets = MapSecrets::from_pairs([("accessToken", "keychain-token")]);
    let interpolator = Interpolator::new(&scopes, &secrets);
    let url = interpolator.resolve(&request.request.url);
    assert_eq!(url.value, "http://localhost:8080/acme/users/7");
    assert!(url.missing.is_empty());

    let token = interpolator.resolve(&chain.auth.unwrap().token.unwrap_or_default());
    assert_eq!(token.value, "keychain-token");
    assert_eq!(token.secrets, vec!["accessToken"]);

    // Local-only documents.
    let workspace = WorkspaceDoc {
        schema_version: SCHEMA_VERSION.into(),
        variables: Some(BTreeMap::from([("machine".into(), "dev".into())])),
    };
    to_path(root.join(".keel/workspace.yaml"), &workspace).unwrap();
    let env_values = EnvValuesDoc {
        schema_version: SCHEMA_VERSION.into(),
        values: Some(BTreeMap::from([(
            "local.yaml".into(),
            BTreeMap::from([("baseUrl".into(), "http://127.0.0.1:9090".into())]),
        )])),
    };
    to_path(root.join(".keel/env-values.yaml"), &env_values).unwrap();

    let workspace_read: WorkspaceDoc = from_path(root.join(".keel/workspace.yaml")).unwrap();
    assert_eq!(workspace_read, workspace);
    let env_values_read: EnvValuesDoc = from_path(root.join(".keel/env-values.yaml")).unwrap();
    assert_eq!(env_values_read, env_values);
}
