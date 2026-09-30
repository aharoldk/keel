//! Keel Format documents: every file the format defines on disk.
//!
//! A workspace is a folder tracked by Git. Its layout is:
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
//! The canonical YAML uses `camelCase` keys and a `schemaVersion` field that
//! defaults to [`SCHEMA_VERSION`]. Every document round-trips through these
//! types; [`from_yaml`], [`to_yaml`], [`from_path`] and [`to_path`] are the
//! entry points.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// Version written to new documents when none is present.
pub const SCHEMA_VERSION: &str = "1";

fn default_true() -> bool {
    true
}

fn default_schema_version() -> String {
    SCHEMA_VERSION.to_string()
}

/// `enabled` serializes only when false to keep YAML diffs minimal.
fn skip_enabled(enabled: &bool) -> bool {
    *enabled
}

fn skip_none<T>(opt: &Option<T>) -> bool {
    opt.is_none()
}

fn is_false(b: &bool) -> bool {
    !*b
}

// ---------- shared building blocks ----------

/// Name/value row used for params, headers and form bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KV {
    /// Field name.
    pub name: String,
    /// Field value. For multipart rows with `kind: file` this is a path.
    #[serde(default)]
    pub value: String,
    /// Disabled rows are kept in the file but skipped when sending.
    #[serde(default = "default_true", skip_serializing_if = "skip_enabled")]
    pub enabled: bool,
    /// For multipart rows: `"file"` means `value` is a path on disk.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub kind: Option<String>,
}

impl Default for KV {
    fn default() -> Self {
        Self {
            name: String::new(),
            value: String::new(),
            enabled: true,
            kind: None,
        }
    }
}

impl KV {
    /// Creates an enabled row.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Self::default()
        }
    }
}

fn deserialize_opt_kv_list<'de, D>(de: D) -> std::result::Result<Option<Vec<KV>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    kv_list_from_map_or_array(de).map(Some)
}

/// Parses either an array of `{name, value, ...}` rows (canonical) or a map
/// `{name: value}` (concise YAML form). Maps lose ordering and all rows are
/// enabled.
pub fn kv_list_from_map_or_array<'de, D>(de: D) -> std::result::Result<Vec<KV>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Rows(Vec<KV>),
        Map(BTreeMap<String, String>),
    }
    match Raw::deserialize(de)? {
        Raw::Rows(rows) => Ok(rows),
        Raw::Map(map) => Ok(map
            .into_iter()
            .map(|(name, value)| KV {
                name,
                value,
                ..KV::default()
            })
            .collect()),
    }
}

/// Supported HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    /// `GET`
    GET,
    /// `POST`
    POST,
    /// `PUT`
    PUT,
    /// `PATCH`
    PATCH,
    /// `DELETE`
    DELETE,
    /// `HEAD`
    HEAD,
    /// `OPTIONS`
    OPTIONS,
    /// `TRACE`
    TRACE,
}

impl HttpMethod {
    /// Uppercase method name, as written in YAML.
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::PATCH => "PATCH",
            HttpMethod::DELETE => "DELETE",
            HttpMethod::HEAD => "HEAD",
            HttpMethod::OPTIONS => "OPTIONS",
            HttpMethod::TRACE => "TRACE",
        }
    }

    /// Methods that conventionally carry a body.
    pub fn allows_body(&self) -> bool {
        matches!(
            self,
            HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH | HttpMethod::DELETE
        )
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Body encoding of a request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BodyType {
    /// No body.
    #[default]
    None,
    /// `application/json`; see [`Body::content`].
    Json,
    /// `text/plain`.
    Text,
    /// `application/xml`.
    Xml,
    /// `application/x-www-form-urlencoded`; see [`Body::items`].
    FormUrlencoded,
    /// `multipart/form-data`; see [`Body::items`].
    Multipart,
    /// Raw bytes read from [`Body::path`].
    Binary,
    /// GraphQL; see [`Body::query`] and [`Body::variables`].
    Graphql,
}

/// Request body. Only the fields relevant to [`Body::body_type`] are set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    /// Encoding of the body.
    #[serde(rename = "type")]
    pub body_type: BodyType,
    /// Inline text for json/text/xml/graphql bodies.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub content: Option<String>,
    /// Rows for `form-urlencoded` and `multipart` bodies.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub items: Option<Vec<KV>>,
    /// File path for binary bodies.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub path: Option<String>,
    /// GraphQL query document.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub query: Option<String>,
    /// GraphQL variables, as a JSON string.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<String>,
}

/// Authentication scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthType {
    /// No auth.
    None,
    /// `Authorization: Bearer <token>`.
    Bearer,
    /// HTTP basic credentials.
    Basic,
    /// API key header or query parameter.
    Apikey,
    /// Digest authentication; retried once on a 401 challenge.
    Digest,
    /// OAuth2; see [`Auth::grant_type`].
    Oauth2,
}

/// OAuth2 flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OAuth2GrantType {
    /// `client_credentials`
    ClientCredentials,
    /// `password`
    Password,
    /// `authorization_code` with PKCE support.
    AuthorizationCode,
}

/// Authentication configuration. Fields unused by [`Auth::auth_type`] stay
/// `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    /// Scheme to apply.
    #[serde(rename = "type")]
    pub auth_type: AuthType,
    /// Bearer token template.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token: Option<String>,
    /// Basic auth username.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub username: Option<String>,
    /// Basic auth password or OAuth2 password grant password.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub password: Option<String>,
    /// API key name.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub key: Option<String>,
    /// API key value.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub value: Option<String>,
    /// API key placement: `header` or `query`.
    #[serde(default, rename = "in", skip_serializing_if = "skip_none")]
    pub location: Option<String>,
    // ---- OAuth2 (Keel Format v1.1) ----
    /// OAuth2 grant.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub grant_type: Option<OAuth2GrantType>,
    /// Token endpoint.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub access_token_url: Option<String>,
    /// Refresh endpoint; defaults to the token endpoint.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub refresh_token_url: Option<String>,
    /// OAuth2 client id.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub client_id: Option<String>,
    /// OAuth2 client secret.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub client_secret: Option<String>,
    /// Requested scope.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scope: Option<String>,
    /// Authorization endpoint (authorization code grant).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub authorization_url: Option<String>,
    /// Redirect URI (authorization code grant).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub callback_url: Option<String>,
    /// Use PKCE for the authorization code grant.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pkce: bool,
    /// Static `state` parameter.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub state: Option<String>,
    /// Where client credentials go: `basic` or `body`.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub credentials_placement: Option<String>,
    /// Where the access token goes: `header` or `query`.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token_placement: Option<String>,
    /// Prefix for the header token placement.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token_header_prefix: Option<String>,
    /// Query key for the query token placement.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token_query_key: Option<String>,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            auth_type: AuthType::None,
            token: None,
            username: None,
            password: None,
            key: None,
            value: None,
            location: None,
            grant_type: None,
            access_token_url: None,
            refresh_token_url: None,
            authorization_url: None,
            callback_url: None,
            client_id: None,
            client_secret: None,
            scope: None,
            state: None,
            pkce: false,
            credentials_placement: None,
            token_placement: None,
            token_header_prefix: None,
            token_query_key: None,
        }
    }
}

/// One row of the Tests tab: an expression plus exactly one matcher key,
/// e.g. `expect: response.status` with `toBe: 200`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAssertion {
    /// Path expression evaluated against the response.
    pub expect: String,
    /// Exactly one matcher key at the same level, e.g. `toBe: 200`.
    #[serde(flatten)]
    pub matcher: BTreeMap<String, Value>,
}

/// Pre-request and post-response scripts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scripts {
    /// Runs before the request is sent.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub pre_request: Option<String>,
    /// Runs after the response arrives.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub post_response: Option<String>,
}

// ---------- request files ----------

/// The `request:` block of a request file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBlock {
    /// HTTP method.
    pub method: HttpMethod,
    /// URL template; `{{name}}` placeholders are interpolated at send time.
    pub url: String,
    /// Query parameters.
    #[serde(default, skip_serializing_if = "skip_none")]
    #[serde(deserialize_with = "deserialize_opt_kv_list")]
    pub params: Option<Vec<KV>>,
    /// Request headers.
    #[serde(default, skip_serializing_if = "skip_none")]
    #[serde(deserialize_with = "deserialize_opt_kv_list")]
    pub headers: Option<Vec<KV>>,
    /// Values for `:name` segments in [`RequestBlock::url`].
    #[serde(default, skip_serializing_if = "skip_none")]
    #[serde(deserialize_with = "deserialize_opt_kv_list")]
    pub path_params: Option<Vec<KV>>,
    /// Request body.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub body: Option<Body>,
}

impl Default for RequestBlock {
    fn default() -> Self {
        Self {
            method: HttpMethod::GET,
            url: String::new(),
            params: None,
            headers: None,
            path_params: None,
            body: None,
        }
    }
}

/// A request file (`**/*.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Display name. The file name is the stable identity.
    pub name: String,
    /// Always `request` for request files.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    /// `http` (default) | `graphql` | `websocket` | `grpc`. Informational for
    /// the editor; HTTP sends still use `request`.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub protocol: Option<String>,
    /// GraphQL editor state.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub graphql: Option<GraphqlSpec>,
    /// WebSocket editor state.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub websocket: Option<WebsocketSpec>,
    /// gRPC editor state.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub grpc: Option<GrpcSpec>,
    /// The HTTP request itself.
    pub request: RequestBlock,
    /// Request-level auth; overrides inherited auth.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub auth: Option<Auth>,
    /// Request-level variables; highest committed precedence.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    /// Request-level scripts.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scripts: Option<Scripts>,
    /// Declarative test assertions.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub tests: Option<Vec<TestAssertion>>,
}

fn default_kind() -> String {
    "request".to_string()
}

impl Default for RequestDoc {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            name: String::new(),
            kind: default_kind(),
            description: None,
            protocol: None,
            graphql: None,
            websocket: None,
            grpc: None,
            request: RequestBlock::default(),
            auth: None,
            variables: None,
            scripts: None,
            tests: None,
        }
    }
}

/// GraphQL editor state on a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlSpec {
    /// The query text. HTTP sends use [`BodyType::Graphql`] instead.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub operation: Option<String>,
    /// Endpoint override.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub endpoint: Option<String>,
}

/// WebSocket editor state on a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsocketSpec {
    /// Comma-separated subprotocols.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub protocols: Option<String>,
}

/// gRPC editor state on a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcSpec {
    /// Fully-qualified service name.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub service: Option<String>,
    /// Method name.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub method: Option<String>,
    /// Path to the `.proto` file.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub proto: Option<String>,
}

// ---------- workspace documents ----------

/// An environment file (`environments/*.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Environment name, shown in the UI.
    pub name: String,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    /// Committed default values.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    /// Declared secrets: name → default value. The default value is the
    /// committed-to-Git fallback used when no current value is set in the OS
    /// keychain (which always wins). Accepts the legacy list-of-names form.
    #[serde(
        default,
        skip_serializing_if = "skip_none",
        deserialize_with = "deserialize_secrets"
    )]
    pub secrets: Option<BTreeMap<String, String>>,
}

/// Accepts both the v1 list-of-names form (`secrets: [apiToken]`, default value
/// empty) and the map form (`secrets: { apiToken: dev-token }`).
fn deserialize_secrets<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<BTreeMap<String, String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Legacy {
        Names(Vec<String>),
        Map(BTreeMap<String, String>),
    }

    let Some(value) = Option::<Legacy>::deserialize(deserializer)? else {
        return Ok(None);
    };
    Ok(Some(match value {
        Legacy::Map(map) => map,
        Legacy::Names(names) => names
            .into_iter()
            .map(|n| (n.trim().to_string(), String::new()))
            .filter(|(n, _)| !n.is_empty())
            .collect(),
    }))
}

/// The collection root file (`collection.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Collection name.
    pub name: String,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    /// Collection-wide variables.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    /// Environment used when none is selected.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub default_environment: Option<String>,
    /// Default auth for requests without their own (nearest folder wins).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub auth: Option<Auth>,
    /// Headers merged into every request (request/folder headers win).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub headers: Option<Vec<KV>>,
    /// Scripts inherited by every request (see [`crate::inherit`]).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scripts: Option<Scripts>,
    /// Child names (file or folder) in display order. Missing names sort last.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub order: Option<Vec<String>>,
}

/// Folder metadata file (`folder.yaml`). Auth, headers, scripts and variables
/// are inherited by every request below the folder (nearest folder wins).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Always `folder` when present.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub kind: Option<String>,
    /// Optional display name; the directory name is the identity.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub name: Option<String>,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    /// Folder-scoped variables.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    /// Folder auth; overrides outer folders and the collection.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub auth: Option<Auth>,
    /// Folder headers, merged into every request below.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub headers: Option<Vec<KV>>,
    /// Folder scripts (see [`crate::inherit`] for ordering).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scripts: Option<Scripts>,
    /// Child names (file or folder) in display order. Missing names sort last.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub order: Option<Vec<String>>,
}

/// Workspace-local metadata (`.keel/workspace.yaml`, never committed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Workspace-scoped variables, below environment precedence.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
}

impl Default for WorkspaceDoc {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            variables: None,
        }
    }
}

/// Local "current value" overrides for environment variables
/// (`.keel/env-values.yaml`, never committed). At send time a variable uses
/// its current value when set, otherwise the default value committed in the
/// environment file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvValuesDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// env file name → variable name → current value.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub values: Option<BTreeMap<String, BTreeMap<String, String>>>,
}

impl Default for EnvValuesDoc {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            values: None,
        }
    }
}

// ---------- flows ----------

/// A saved request sequence (`flows/**/*.yaml`). Steps reference request files
/// by workspace-relative path and run in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDoc {
    /// Format version; defaults to [`SCHEMA_VERSION`].
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Display name.
    pub name: String,
    /// Always `flow`.
    #[serde(default = "default_flow_kind")]
    pub kind: String,
    /// Steps, in run order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<FlowStep>,
}

fn default_flow_kind() -> String {
    "flow".to_string()
}

/// One step in a flow. Untagged so a bare path string stays valid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlowStep {
    /// A bare workspace-relative request path.
    Path(String),
    /// A step object, e.g. with `onFailure`.
    Detailed(FlowStepDetail),
}

/// A detailed flow step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStepDetail {
    /// Workspace-relative request path.
    pub path: String,
    /// `stop` (default) or `continue`.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub on_failure: Option<String>,
}

impl FlowStep {
    /// The request path this step points at.
    pub fn path(&self) -> &str {
        match self {
            FlowStep::Path(path) => path,
            FlowStep::Detailed(step) => &step.path,
        }
    }

    /// Missing or anything other than `continue` stops the flow.
    pub fn stops_on_failure(&self) -> bool {
        match self {
            FlowStep::Path(_) => true,
            FlowStep::Detailed(step) => step.on_failure.as_deref() != Some("continue"),
        }
    }
}

// ---------- local history ----------

/// One history record (`.keel/history.jsonl`). The URL is stored unresolved
/// (as a template) so secrets can never leak into history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    /// RFC 3339 timestamp.
    pub ts: String,
    /// HTTP method.
    pub method: String,
    /// URL template, exactly as written in the request file.
    pub url: String,
    /// Response status, when the request completed.
    pub status: Option<i64>,
    /// Whether the request completed without a transport error.
    pub ok: bool,
    /// Round-trip time in milliseconds.
    pub time_ms: f64,
    /// Environment name at send time.
    pub env: Option<String>,
    /// Workspace-relative request file path.
    pub request_path: Option<String>,
}

/// A pinned history row. Keyed by timestamp plus request path so the
/// append-only log does not have to be rewritten. Bodies are not stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPin {
    /// Timestamp of the pinned record.
    pub ts: String,
    /// Workspace-relative request file path.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub request_path: Option<String>,
}

// ---------- helpers ----------

/// Parses a YAML document.
pub fn from_yaml<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_yaml_ng::from_str(text).map_err(Error::Yaml)
}

/// Serializes a document to YAML.
pub fn to_yaml<T: Serialize>(value: &T) -> Result<String> {
    serde_yaml_ng::to_string(value).map_err(Error::Serialize)
}

/// Reads and parses a YAML document from disk.
pub fn from_path<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| Error::read(path, e))?;
    from_yaml(&text)
}

/// Serializes a document and writes it to disk, creating parent directories.
pub fn to_path<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<()> {
    let path = path.as_ref();
    let text = to_yaml(value)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| Error::write(path, e))?;
    }
    std::fs::write(path, text).map_err(|e| Error::write(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_secrets_accept_map_form() {
        let yaml = r#"
name: local
secrets:
  apiToken: default-token
  emptyDefault: ""
"#;
        let doc: EnvDoc = from_yaml(yaml).expect("parse map form");
        let secrets = doc.secrets.expect("secrets");
        assert_eq!(
            secrets.get("apiToken").map(String::as_str),
            Some("default-token")
        );
        assert_eq!(secrets.get("emptyDefault").map(String::as_str), Some(""));
    }

    #[test]
    fn env_secrets_accept_legacy_list_form() {
        let yaml = r#"
name: local
secrets:
  - apiToken
  - refreshToken
"#;
        let doc: EnvDoc = from_yaml(yaml).expect("parse legacy list form");
        let secrets = doc.secrets.expect("secrets");
        assert_eq!(secrets.get("apiToken").map(String::as_str), Some(""));
        assert_eq!(secrets.get("refreshToken").map(String::as_str), Some(""));
    }

    #[test]
    fn env_secrets_absent_is_none() {
        let yaml = "name: local\n";
        let doc: EnvDoc = from_yaml(yaml).expect("parse without secrets");
        assert!(doc.secrets.is_none());
    }

    #[test]
    fn kv_lists_accept_rows_and_maps() {
        let rows = r#"
name: Rows
request:
  method: GET
  url: "https://x.dev"
  headers:
    - name: Accept
      value: application/json
"#;
        let doc: RequestDoc = from_yaml(rows).expect("rows");
        let headers = doc.request.headers.expect("headers");
        assert_eq!(headers, vec![KV::new("Accept", "application/json")]);

        let map = r#"
name: Map
request:
  method: GET
  url: "https://x.dev"
  headers:
    Accept: application/json
"#;
        let doc: RequestDoc = from_yaml(map).expect("map");
        let headers = doc.request.headers.expect("headers");
        assert_eq!(headers, vec![KV::new("Accept", "application/json")]);
    }

    #[test]
    fn kv_enabled_serializes_only_when_false() {
        let enabled = to_yaml(&KV::new("a", "1")).expect("serialize");
        assert!(!enabled.contains("enabled"), "enabled omitted: {enabled}");

        let disabled = KV {
            enabled: false,
            ..KV::new("a", "1")
        };
        let text = to_yaml(&disabled).expect("serialize");
        assert!(text.contains("enabled: false"), "disabled kept: {text}");
    }

    #[test]
    fn request_defaults_apply() {
        let yaml = r#"
name: Get user
request:
  method: GET
  url: "https://x.dev/users/1"
"#;
        let doc: RequestDoc = from_yaml(yaml).expect("parse");
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
        assert_eq!(doc.kind, "request");
        assert_eq!(doc.request.method, HttpMethod::GET);
        assert!(doc.request.body.is_none());
    }

    #[test]
    fn body_type_is_kebab_case() {
        let yaml = r#"
name: Post
request:
  method: POST
  url: "https://x.dev"
  body:
    type: form-urlencoded
    items:
      - name: a
        value: "1"
"#;
        let doc: RequestDoc = from_yaml(yaml).expect("parse");
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::FormUrlencoded);
        assert_eq!(body.items, Some(vec![KV::new("a", "1")]));
    }

    #[test]
    fn flow_steps_accept_both_forms() {
        let yaml = r#"
name: Lifecycle
kind: flow
steps:
  - auth/login.yaml
  - path: users/delete-user.yaml
    onFailure: continue
"#;
        let doc: FlowDoc = from_yaml(yaml).expect("parse");
        assert_eq!(doc.steps.len(), 2);
        assert_eq!(doc.steps[0].path(), "auth/login.yaml");
        assert!(doc.steps[0].stops_on_failure());
        assert_eq!(doc.steps[1].path(), "users/delete-user.yaml");
        assert!(!doc.steps[1].stops_on_failure());
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn documents_round_trip_through_yaml() {
        let request = r#"
schemaVersion: "1"
name: Create User
request:
  method: POST
  url: "{{baseUrl}}/users"
  headers:
    - name: Accept
      value: application/json
  body:
    type: json
    content: '{"name": "Ada"}'
auth:
  type: bearer
  token: "{{accessToken}}"
variables:
  userId: "1"
scripts:
  postResponse: set("userId", json("id"))
tests:
  - expect: response.status
    toBe: 201
"#;
        let doc: RequestDoc = from_yaml(request).expect("parse");
        let text = to_yaml(&doc).expect("serialize");
        let again: RequestDoc = from_yaml(&text).expect("re-parse");
        assert_eq!(doc, again);
    }

    #[test]
    fn collection_and_folder_round_trip() {
        let collection = CollectionDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: "Demo API".into(),
            description: Some("demo".into()),
            variables: Some(BTreeMap::from([("userId".into(), "1".into())])),
            default_environment: Some("local".into()),
            auth: Some(Auth {
                auth_type: AuthType::Bearer,
                token: Some("{{token}}".into()),
                ..Auth::default()
            }),
            headers: Some(vec![KV::new("X-Client", "keel-demo")]),
            scripts: Some(Scripts {
                pre_request: Some("log('go')".into()),
                post_response: None,
            }),
            order: Some(vec!["users".into()]),
        };
        let again: CollectionDoc =
            from_yaml(&to_yaml(&collection).expect("serialize")).expect("re-parse");
        assert_eq!(collection, again);

        let folder = FolderDoc {
            schema_version: SCHEMA_VERSION.into(),
            kind: Some("folder".into()),
            name: Some("Users".into()),
            description: None,
            variables: None,
            auth: None,
            headers: None,
            scripts: None,
            order: None,
        };
        let again: FolderDoc = from_yaml(&to_yaml(&folder).expect("serialize")).expect("re-parse");
        assert_eq!(folder, again);
    }

    #[test]
    fn workspace_and_env_values_default_to_current_schema() {
        assert_eq!(WorkspaceDoc::default().schema_version, SCHEMA_VERSION);
        assert_eq!(EnvValuesDoc::default().schema_version, SCHEMA_VERSION);
        let doc: WorkspaceDoc = from_yaml("variables:\n  a: b\n").expect("parse");
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn history_record_parses_camel_case() {
        let line = r#"{"ts":"2026-01-01T00:00:00Z","method":"GET","url":"{{baseUrl}}/users/1","status":200,"ok":true,"timeMs":12.5,"env":"local","requestPath":"users/get-user.yaml"}"#;
        let record: HistoryRecord = serde_json::from_str(line).expect("parse");
        assert!(record.ok);
        assert_eq!(record.time_ms, 12.5);
    }
}
