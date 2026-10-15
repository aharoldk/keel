// Validates the example workspace against the real format parsers.
use keel_lib::model::{yaml_to, CollectionDoc, EnvDoc, RequestDoc};
use std::path::Path;

#[test]
fn demo_workspace_is_valid() {
    let root = Path::new("../examples/demo-workspace");

    let collection: CollectionDoc = yaml_to(
        &std::fs::read_to_string(root.join("collection.yaml")).expect("collection.yaml"),
    )
    .expect("collection.yaml parses");
    assert_eq!(collection.name, "Demo API");

    for rel in ["environments/local.yaml", "environments/staging.yaml"] {
        let env: EnvDoc = yaml_to(&std::fs::read_to_string(root.join(rel)).expect(rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(env.variables.is_some(), "{rel} declares variables");
    }

    for rel in [
        "auth/bearer-token.yaml",
        "auth/basic-auth.yaml",
        "auth/api-key-header.yaml",
        "auth/api-key-query.yaml",
        "auth/digest-auth.yaml",
        "auth/no-auth-overrides-collection-bearer.yaml",
        "folder-auth-digest/echo-inherits-folder-auth.yaml",
        "folder-auth-digest/echo-own-bearer-wins.yaml",
        "rest/list-users.yaml",
        "rest/get-user-by-id.yaml",
        "rest/create-user.yaml",
        "rest/update-user.yaml",
        "rest/delete-user.yaml",
        "rest/user-not-found-404.yaml",
        "rest/force-server-error.yaml",
        "rest/slow-response-2s.yaml",
        "rest/echo-query-in-raw-url.yaml",
        "rest/echo-header-flags.yaml",
        "rest/echo-multipart-body.yaml",
        "rest/echo-urlencoded-body.yaml",
        "graphql/query-user-by-id-variables.yaml",
        "graphql/mutation-create-user.yaml",
        "grpc/introspect-schema.yaml",
        "graphql/query-raw-json-envelope.yaml",
        "grpc/getuser.yaml",
        "scripts/pre-request-test-scripts.yaml",
        "tls-self-signed/list-users-https.yaml",
        "tls-self-signed/graphql-https.yaml",
        "import-edge-cases/saved-response-examples.yaml",
    ] {
        let doc: RequestDoc = yaml_to(&std::fs::read_to_string(root.join(rel)).expect(rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert_eq!(doc.kind, "request", "{rel}");
        assert!(!doc.request.url.is_empty(), "{rel}");
    }
}
