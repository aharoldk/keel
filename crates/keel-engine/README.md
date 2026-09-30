# keel-engine

[![CI](https://github.com/aharoldk/keel/actions/workflows/ci.yml/badge.svg)](https://github.com/aharoldk/keel/actions/workflows/ci.yml)

The Keel format and runtime as a standalone Rust library: read, write and resolve a local-first, Git-native API workspace, then send requests, run collections, import and export — without the Keel app.

The on-disk format itself (documents, YAML round-tripping, folder inheritance, `{{variable}}` interpolation) lives in the `model`, `inherit` and `variables` modules. The runtime on top adds request sending, auth flows, scripts, the collection runner, cookies, WebSocket/gRPC sessions, import/export, code generation, Git, keychain secrets, AI and workspace management.

The Keel desktop app and `keel-cli` in [aharoldk/keel](https://github.com/aharoldk/keel) are thin layers on top of this crate, so any GUI or CLI can drive the same workspaces and stay format-compatible.

A workspace is a folder of YAML files in a Git repository:

```text
collection.yaml          collection root: name, variables, default auth/headers/scripts
folder.yaml              folder metadata: inherited by every request below it
**/*.yaml                one request per file
environments/*.yaml      environments: variables and declared secrets
flows/**/*.yaml          saved request sequences
.keel/workspace.yaml     local workspace variables (never committed)
.keel/env-values.yaml    local "current value" overrides (never committed)
```

## Install

```bash
cargo add keel-engine
```

## Example

```rust
use keel_engine::{import_curl, workspace};

let dir = tempfile::tempdir()?;
workspace::init_workspace(dir.path(), "Demo")?;

let doc = import_curl::curl_to_request("curl https://api.example.com/users", None)?;
assert_eq!(doc.request.url, "https://api.example.com/users");
# Ok::<(), String>(())
```

Sending a request needs a scope stack and a secret source; the full flow (open a workspace, resolve inheritance, send, run tests, write history) is exercised in the [Keel app's end-to-end test](https://github.com/aharoldk/keel/blob/main/src-tauri/tests/app_flow.rs).

## What it covers

- **`engine`** — request sending over HTTP/1.1 and HTTP/2 with redirects, timeouts, proxies, a custom CA, client certificates and client-side TLS; request/response scripts over `keel` / `req` / `res`; the `test()` / `expect()` engine.
- **`runner`** — collection and folder runs with streaming events, pause between requests, stop-on-first-failure, `keel.setNextRequest()` and cancellation.
- **`auth`** — bearer, basic, API key, digest (retries on a 401 challenge) and OAuth2: client credentials, password, authorization code with PKCE. Tokens stay in memory.
- **`workspace`** — open, init, tree, create/rename/move/delete, flows and environments.
- **`import_curl`**, **`import_openapi`**, **`import_postman`**, **`import_source`**; **`export_curl`**, **`export_openapi`**.
- **`codegen`** — curl, fetch, axios, Python, HTTPie, Go, Java, Node.
- **`history`**, **`cookies`**, **`ws`** (WebSocket sessions), **`grpc`** (proto parsing, calls, server streams), **`graphql`**, **`datafile`** (CSV/JSON rows), **`path_params`**.
- **`gitutil`** — status, stage, unstage, commit, log, diffs, branches, pull and push through the system `git` binary.
- **`secrets`** — OS keychain storage (`keel:<workspace>:<env>:<var>`). Resolution is caller-provided through `variables::SecretSource`, so headless tools can plug in their own backend.
- **`ai`** — request generation against OpenAI, Anthropic or an OpenAI-compatible endpoint.
- **`settings`** — the app config file used by the AI helpers and the desktop shell.

Format tooling (linters, editor plugins, CI diffs) only needs `model`, `inherit` and `variables`; anything that sends requests or mutates a workspace uses the rest of the crate.

## License

[MIT](LICENSE).
