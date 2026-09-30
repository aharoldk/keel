//! Keel format domain models + IPC DTOs.
//!
//! The on-disk format itself (documents, YAML helpers, inheritance,
//! variables) lives in `doc`, [`crate::inherit`] and [`crate::variables`],
//! and is re-exported here so the rest of the app keeps a single import
//! path. This module only adds the types that are specific to the Tauri IPC
//! wire (see docs/IPC_CONTRACT.md).

mod doc;
pub use doc::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parses YAML into a Keel document, reporting the error as a string for the
/// command layer. Thin wrapper over [`from_yaml`].
pub fn yaml_to<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, String> {
    doc::from_yaml(text).map_err(|e| e.to_string())
}

/// Serializes a Keel document to YAML. Thin wrapper over
/// [`to_yaml`].
pub fn yaml_of<T: Serialize>(value: &T) -> Result<String, String> {
    doc::to_yaml(value).map_err(|e| e.to_string())
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

// ---------- IPC DTOs ----------

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
    /// Path relative to `flows/`, using `/`.
    pub file_name: String,
    pub name: String,
}

/// One node in the flows tree. Folders hold children; flows are leaves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowTreeNodeDto {
    /// Path relative to `flows/`, using `/`.
    pub path: String,
    pub name: String,
    /// `folder` or `flow`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FlowTreeNodeDto>>,
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
