# Using keel-engine

The format and the runtime live in one standalone crate, `keel-engine`, under `crates/`. Any CLI, editor plugin or CI job can read and resolve a workspace without the GUI.

```bash
cargo add keel-engine
```

Minimal example:

```rust
use keel_engine::{import_curl, workspace};

let dir = tempfile::tempdir().unwrap();
workspace::init_workspace(dir.path(), "Demo").unwrap();

let doc = import_curl::curl_to_request(
    "curl https://api.example.com/users",
    None,
).unwrap();
assert_eq!(doc.request.url, "https://api.example.com/users");
```

## Modules

| Module | What it carries |
| --- | --- |
| `model` | Typed documents for every file, with round-tripping `from_yaml` / `to_yaml` / `from_path` / `to_path` |
| `inherit` | Effective headers, auth, script ordering and variable layers for a request, resolved through the collection + folder chain |
| `variables` | `{{name}}` interpolation over ordered scopes, with `$timestamp`, `$uuid` and `$randomInt` builtins |
| `engine` | Request sending (HTTP/1.1 + HTTP/2, TLS, proxies, client certs), scripts over `keel` / `req` / `res`, the test engine |
| `runner` | Collection and folder runs, streaming events, data files, delays, bail-on-failure and cancellation |
| `auth` | Bearer, basic, API key, digest, OAuth2 |
| `workspace` | Open, init, tree, create/move/delete, flows |
| `import_curl` / `import_openapi` / `import_postman` / `import_source`, `export_curl` / `export_openapi` | Import and export |
| `codegen` | curl, fetch, axios, Python, HTTPie, Go, Java, Node |
| `history`, `cookies`, `ws`, `grpc`, `graphql`, `datafile`, `path_params` | Supporting runtime pieces |
| `gitutil` | Status, stage, commit, log, diffs, branches, sync |
| `secrets` | OS keychain storage |
| `ai` / `ai_guard` | Request generation with a topic guard |
| `settings` | The app config file |

Format tooling only needs `model`, `inherit` and `variables`.

## Secrets in headless builds

Secret *resolution* is caller-provided. Implement the `SecretSource` trait (or use the in-memory `MapSecrets`) and pass it to the interpolator or runner:

```rust
use keel_engine::variables::{Interpolator, MapSecrets, ScopeStack, Scope};

let mut stack = ScopeStack::default();
stack.push(Scope::from_pairs([("baseUrl", "http://localhost:8080")]));
let secrets = MapSecrets::from_pairs([("apiToken", "from-keychain")]);

let resolved = Interpolator::new(&stack, &secrets).resolve("Bearer {{apiToken}}");
```

## Testing

The whole workspace tests with:

```bash
cargo test --workspace --features keel/cli
```
