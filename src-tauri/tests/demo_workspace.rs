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
        "auth/login.yaml",
        "auth/refresh-token.yaml",
        "users/list-users.yaml",
        "users/get-user.yaml",
        "users/create-user.yaml",
        "users/delete-user.yaml",
    ] {
        let doc: RequestDoc = yaml_to(&std::fs::read_to_string(root.join(rel)).expect(rel))
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert_eq!(doc.kind, "request", "{rel}");
        assert!(!doc.request.url.is_empty(), "{rel}");
    }
}
