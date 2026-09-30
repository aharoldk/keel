//! # keel-engine
//!
//! The Keel format and runtime: everything an API client needs below the
//! GUI. The on-disk format (typed documents, `{{variable}}` interpolation,
//! the collection → folder → request inheritance chain) lives in [`model`],
//! [`inherit`] and [`variables`]. The runtime on top adds request sending,
//! auth flows, scripts, the collection runner, cookies, WebSocket/gRPC
//! sessions, import/export, code generation, Git, keychain secrets, AI
//! helpers and workspace management.
//!
//! A workspace is a folder of YAML files in a Git repository:
//!
//! ```text
//! collection.yaml          collection root: name, variables, default auth/headers/scripts
//! folder.yaml              folder metadata: inherited by every request below it
//! **/*.yaml                one request per file
//! environments/*.yaml      environments: variables and declared secrets
//! flows/**/*.yaml          saved request sequences
//! .keel/workspace.yaml     local workspace variables (never committed)
//! .keel/env-values.yaml    local "current value" overrides (never committed)
//! ```
//!
//! A GUI (Tauri, egui, Slint, …) or a CLI only needs this crate to drive the
//! same workspaces, and the Keel desktop app is a thin command layer on top
//! of it.
//!
//! ```
//! use keel_engine::{import_curl, workspace};
//!
//! let dir = tempfile::tempdir().unwrap();
//! workspace::init_workspace(dir.path(), "Demo").unwrap();
//!
//! let doc = import_curl::curl_to_request("curl https://api.example.com/users", None).unwrap();
//! assert_eq!(doc.request.url, "https://api.example.com/users");
//! ```
//!
//! # What it covers
//!
//! - **[`model`]**: typed documents for every file, with round-tripping
//!   `from_yaml` / `to_yaml` / `from_path` / `to_path` helpers.
//! - **[`inherit`]**: effective headers, auth, script ordering and variable
//!   layers for a request, resolved through the collection + folder chain.
//! - **[`variables`]**: `{{name}}` interpolation over ordered scopes, with
//!   `$timestamp`, `$uuid` and `$randomInt` builtins.
//! - **[`engine`]**: request sending (HTTP/1.1 + HTTP/2, TLS, proxies, client
//!   certs), request/response scripts over `keel` / `req` / `res`, and the
//!   test engine.
//! - **[`runner`]**: collection and folder runs, streaming events, data
//!   files, delays, bail-on-failure and cancellation.
//! - **[`auth`]**: bearer, basic, API key, digest and OAuth2 (client
//!   credentials, password, authorization code with PKCE).
//! - **[`workspace`]**: open, init, tree, create/move/delete and flows.
//! - **[`import_curl`] / [`import_openapi`] / [`import_postman`] /
//!   [`import_source`]** and **[`export_curl`] / [`export_openapi`]**.
//! - **[`codegen`]**: curl, fetch, axios, Python, HTTPie, Go, Java, Node.
//! - **[`history`]**, **[`cookies`]**, **[`ws`]**, **[`grpc`]**,
//!   **[`graphql`]**, **[`datafile`]**, **[`path_params`]**.
//! - **[`gitutil`]**: status, stage, commit, log, diffs, branches and sync.
//! - **[`secrets`]**: OS keychain storage. Secret *resolution* is
//!   caller-provided via [`variables::SecretSource`], so headless builds can
//!   plug in their own backend.
//! - **[`ai`]**: request generation against OpenAI/Anthropic/custom
//!   endpoints.
//! - **[`settings`]**: the app config file used by the AI helpers and the
//!   desktop shell.

mod error;

pub use error::{Error, Result};
pub use inherit::{build, folder_chain, from_docs, Inherited};
pub use model::*;
pub use variables::{Interpolator, MapSecrets, Resolved, Scope, ScopeStack, SecretSource};

pub mod ai;
pub mod auth;
pub mod codegen;
pub mod cookies;
pub mod datafile;
pub mod engine;
pub mod export_curl;
pub mod export_openapi;
pub mod gitutil;
pub mod graphql;
pub mod grpc;
pub mod history;
pub mod import_curl;
pub mod inherit;
pub mod import_openapi;
pub mod import_postman;
pub mod import_source;
pub mod model;
pub mod path_params;
pub mod runner;
pub mod secrets;
pub mod settings;
pub mod variables;
pub mod workspace;
pub mod ws;
