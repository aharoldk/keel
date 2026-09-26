//! Keel format domain models. Serialization is the wire format for both YAML
//! files (Keel Format v1, see docs/IPC_CONTRACT.md) and Tauri IPC payloads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Name/value row used for params, headers and form bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KV {
    pub name: String,
    #[serde(default)]
    pub value: String,
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

fn deserialize_opt_kv_list<'de, D>(de: D) -> Result<Option<Vec<KV>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    kv_list_from_map_or_array(de).map(Some)
}

/// Parses either an array of `{name, value, ...}` rows (canonical) or a map
/// `{name: value}` (concise YAML form). Maps lose ordering and all rows are
/// enabled.
pub fn kv_list_from_map_or_array<'de, D>(de: D) -> Result<Vec<KV>, D::Error>
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
    TRACE,
}

impl HttpMethod {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BodyType {
    #[default]
    None,
    Json,
    Text,
    Xml,
    FormUrlencoded,
    Multipart,
    Binary,
    Graphql,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    #[serde(rename = "type")]
    pub body_type: BodyType,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub items: Option<Vec<KV>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthType {
    None,
    Bearer,
    Basic,
    Apikey,
    Digest,
    Oauth2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OAuth2GrantType {
    ClientCredentials,
    Password,
    AuthorizationCode,
}

fn is_false(b: &bool) -> bool {
    !*b
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    #[serde(rename = "type")]
    pub auth_type: AuthType,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub value: Option<String>,
    #[serde(default, rename = "in", skip_serializing_if = "skip_none")]
    pub location: Option<String>,
    // ---- OAuth2 (Keel Format v1.1) ----
    #[serde(default, skip_serializing_if = "skip_none")]
    pub grant_type: Option<OAuth2GrantType>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub access_token_url: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub refresh_token_url: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub authorization_url: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub callback_url: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pkce: bool,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub credentials_placement: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token_placement: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token_header_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub token_query_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAssertion {
    pub expect: String,
    /// Exactly one matcher key at the same level, e.g. `toBe: 200`.
    #[serde(flatten)]
    pub matcher: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scripts {
    #[serde(default, skip_serializing_if = "skip_none")]
    pub pre_request: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub post_response: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBlock {
    pub method: HttpMethod,
    pub url: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    #[serde(deserialize_with = "deserialize_opt_kv_list")]
    pub params: Option<Vec<KV>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    #[serde(deserialize_with = "deserialize_opt_kv_list")]
    pub headers: Option<Vec<KV>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    #[serde(deserialize_with = "deserialize_opt_kv_list")]
    pub path_params: Option<Vec<KV>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub body: Option<Body>,
}

/// A request file (`*.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    /// `http` (default) | `graphql` | `websocket` | `grpc`. Informational for
    /// the editor; HTTP sends still use `request`.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub graphql: Option<GraphqlSpec>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub websocket: Option<WebsocketSpec>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub grpc: Option<GrpcSpec>,
    pub request: RequestBlock,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scripts: Option<Scripts>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlSpec {
    #[serde(default, skip_serializing_if = "skip_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsocketSpec {
    #[serde(default, skip_serializing_if = "skip_none")]
    pub protocols: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcSpec {
    #[serde(default, skip_serializing_if = "skip_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub proto: Option<String>,
}

/// An environment file (`environments/*.yaml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    /// Declared secrets: name → default value. The default value is the
    /// committed-to-Git fallback used when no current value is set in the OS
    /// keychain (which always wins). Accepts the legacy list-of-names form.
    #[serde(default, skip_serializing_if = "skip_none", deserialize_with = "deserialize_secrets")]
    pub secrets: Option<BTreeMap<String, String>>,
}

/// Accepts both the v1 list-of-names form (`secrets: [apiToken]`, default value
/// empty) and the map form (`secrets: { apiToken: dev-token }`).
fn deserialize_secrets<'de, D>(deserializer: D) -> Result<Option<BTreeMap<String, String>>, D::Error>
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
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub default_environment: Option<String>,
    /// Default auth for requests without their own (nearest folder wins).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub auth: Option<Auth>,
    /// Headers merged into every request (request/folder headers win).
    #[serde(default, skip_serializing_if = "skip_none")]
    pub headers: Option<Vec<KV>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scripts: Option<Scripts>,
    /// Child names (file or folder) in display order. Missing names sort last.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub order: Option<Vec<String>>,
}

/// Folder metadata file (`folder.yaml`, Keel Format v1.1). Auth, headers,
/// scripts and variables are inherited by every request below the folder
/// (nearest folder wins).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub headers: Option<Vec<KV>>,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub scripts: Option<Scripts>,
    /// Child names (file or folder) in display order. Missing names sort last.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub order: Option<Vec<String>>,
}

/// Workspace-local metadata (`.keel/workspace.yaml`, never committed).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "skip_none")]
    pub variables: Option<BTreeMap<String, String>>,
}

/// Local "current value" overrides for environment variables
/// (`.keel/env-values.yaml`, never committed). At send time a variable uses
/// its current value when set, otherwise the default value committed in the
/// environment file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvValuesDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// env file name → variable name → current value.
    #[serde(default, skip_serializing_if = "skip_none")]
    pub values: Option<BTreeMap<String, BTreeMap<String, String>>>,
}

/// A saved request sequence (`flows/*.yaml`). Steps reference request files by
/// workspace-relative path and run in order. A step is a path string, or an
/// object with `onFailure` so older files still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub name: String,
    #[serde(default = "default_flow_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<FlowStep>,
}

/// One step in a flow. Untagged so a bare path string stays valid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlowStep {
    Path(String),
    Detailed(FlowStepDetail),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStepDetail {
    pub path: String,
    /// `stop` (default) or `continue`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
}

impl FlowStep {
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

fn default_flow_kind() -> String {
    "flow".to_string()
}

/// One history record (`.keel/history.jsonl`). The URL is stored unresolved
/// (as a template) so secrets can never leak into history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub ts: String,
    pub method: String,
    pub url: String,
    pub status: Option<i64>,
    pub ok: bool,
    pub time_ms: f64,
    pub env: Option<String>,
    pub request_path: Option<String>,
}

/// A pinned history row. Keyed by timestamp plus request path so the
/// append-only log does not have to be rewritten. Bodies are not stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPin {
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_path: Option<String>,
}

pub fn yaml_to<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, String> {
    serde_yaml_ng::from_str(text).map_err(|e| format!("invalid YAML: {e}"))
}

pub fn yaml_of<T: Serialize>(value: &T) -> Result<String, String> {
    serde_yaml_ng::to_string(value).map_err(|e| format!("serialize error: {e}"))
}

/// Response snapshot passed to the test engine and post-response scripts.
#[derive(Debug, Clone, Default)]
pub struct ResponseCtx {
    pub status: Option<i64>,
    pub time_ms: f64,
    pub size: u64,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<(String, String)>,
    pub body: Option<String>,
    pub json: Option<serde_json::Value>,
}

/// IPC DTOs (wire shape for the UI; see docs/IPC_CONTRACT.md).

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderDto {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResultDto {
    pub expect: String,
    pub matcher: Option<String>,
    pub expected: Value,
    pub actual: Value,
    pub passed: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub request_id: String,
    pub status: Option<i64>,
    pub status_text: String,
    pub ok: bool,
    pub time_ms: f64,
    pub size_bytes: u64,
    pub headers: Vec<HeaderDto>,
    pub cookies: Vec<HeaderDto>,
    pub content_type: Option<String>,
    pub body_text: Option<String>,
    pub body_base64: Option<String>,
    pub truncated: bool,
    pub error: Option<String>,
    pub variables_used: Vec<String>,
    pub missing_variables: Vec<String>,
    pub secrets_used: Vec<String>,
    pub test_results: Vec<TestResultDto>,
    pub script_logs: Vec<String>,
    pub script_error: Option<String>,
    pub timeline: Vec<TimelineEventDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_used: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEventDto {
    pub ts: String,
    pub phase: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeDto {
    pub path: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<HttpMethod>,
    /// Request URL template. Absent on folders. Used by collection search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNodeDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<FolderMetaDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderMetaDto {
    pub has_auth: bool,
    pub has_scripts: bool,
    pub header_count: usize,
    pub variable_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfoDto {
    pub root: String,
    pub name: String,
    pub has_git: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowSummaryDto {
    pub file_name: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvSummaryDto {
    pub file_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub variable_count: usize,
    pub secret_count: usize,
}

// ---------- v1.1 IPC DTOs ----------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub stop_on_failure: bool,
    #[serde(default)]
    pub recursive: bool,
    /// CSV (header row) or JSON array of objects. One iteration per row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerItemDto {
    pub path: String,
    pub name: String,
    pub method: String,
    /// running | passed | failed | error | skipped
    pub status: String,
    pub status_code: Option<i64>,
    pub time_ms: f64,
    pub size_bytes: u64,
    pub tests_total: usize,
    pub tests_passed: usize,
    /// Data-file row, starting at 0. Always 0 when there is no data file.
    #[serde(default)]
    pub iteration: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerSummaryDto {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errored: usize,
    pub skipped: usize,
    pub duration_ms: f64,
}

/// Event payload emitted on `runner://update`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerEvent {
    pub run_id: String,
    /// item | done
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<RunnerItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RunnerSummaryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieDto {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    pub secure: bool,
    pub http_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiResultDto {
    pub files: Vec<String>,
    pub skipped: usize,
    pub warnings: Vec<String>,
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
        let doc: EnvDoc = yaml_to(yaml).expect("parse map form");
        let secrets = doc.secrets.expect("secrets");
        assert_eq!(secrets.get("apiToken").map(String::as_str), Some("default-token"));
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
        let doc: EnvDoc = yaml_to(yaml).expect("parse legacy list form");
        let secrets = doc.secrets.expect("secrets");
        assert_eq!(secrets.get("apiToken").map(String::as_str), Some(""));
        assert_eq!(secrets.get("refreshToken").map(String::as_str), Some(""));
    }

    #[test]
    fn env_secrets_absent_is_none() {
        let yaml = "name: local\n";
        let doc: EnvDoc = yaml_to(yaml).expect("parse without secrets");
        assert!(doc.secrets.is_none());
    }
}
