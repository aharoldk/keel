//! Local bring-your-own-key AI. The API key lives in the OS keychain
//! (`keel` / `app:ai:key`) and is never written to settings or returned
//! to the UI. Requests go straight to the user's provider.

use serde::{Deserialize, Serialize};

use crate::secrets;
use crate::settings::AppSettings;

const KEY_ACCOUNT_ROOT: &str = "app";
const KEY_ACCOUNT_ENV: &str = "ai";
const KEY_NAME: &str = "key";

pub fn key_set(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("API key is empty".into());
    }
    secrets::set(KEY_ACCOUNT_ROOT, KEY_ACCOUNT_ENV, KEY_NAME, value)
}

pub fn key_clear() -> Result<(), String> {
    secrets::delete(KEY_ACCOUNT_ROOT, KEY_ACCOUNT_ENV, KEY_NAME)
}

pub fn key_configured() -> bool {
    secrets::exists(KEY_ACCOUNT_ROOT, KEY_ACCOUNT_ENV, KEY_NAME)
}

fn key_get() -> Result<String, String> {
    secrets::get(KEY_ACCOUNT_ROOT, KEY_ACCOUNT_ENV, KEY_NAME)
        .map_err(|_| "No AI API key is saved. Add one in Settings → AI.".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiKind {
    Script,
    Test,
    Docs,
    Request,
}

impl AiKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "script" => Ok(Self::Script),
            "test" => Ok(Self::Test),
            "docs" => Ok(Self::Docs),
            "request" => Ok(Self::Request),
            other => Err(format!("unknown AI task `{other}`")),
        }
    }
}

pub fn system_prompt(kind: AiKind) -> &'static str {
    match kind {
        AiKind::Script => {
            "You write one Keel pre-request or post-response script. \
Reply with only the script, no markdown fences and no explanation. \
One call per line. Available everywhere: set(\"name\", value), get(\"name\"), log(arg). \
Post-response only: json(\"dot.path\"), status(), header(\"Name\"), time(), size(). \
Values are strings, numbers, booleans, or nested calls."
        }
        AiKind::Test => {
            "You write Keel test assertions as a JSON array. \
Reply with only the JSON array, no markdown fences and no explanation. \
Each item is {\"expect\":\"response.status\"} plus exactly one matcher: \
toBe, toEqual, toContain, toMatch, toBeGreaterThan, toBeLessThan, toBeTruthy, or toBeNull. \
toBeTruthy and toBeNull take true; the others take the expected value. \
expect paths: response.status, response.time, response.size, response.body, response.headers.Name, json.dot.path."
        }
        AiKind::Docs => {
            "You write Markdown documentation for one HTTP request. \
Reply with only the markdown, no surrounding fences and no preamble."
        }
        AiKind::Request => {
            "You edit one Keel request file. Reply with only the full YAML document, \
no markdown fences and no explanation. Keep schemaVersion, kind, and name. \
Change only what the user asked. Use request.method, request.url, request.params, \
request.headers, request.body, auth, scripts, tests, and description. \
Params and headers are lists of {name, value, enabled}."
        }
    }
}

pub fn build_user_prompt(kind: AiKind, prompt: &str, context: &str) -> String {
    let prompt = prompt.trim();
    let context = context.trim();
    if context.is_empty() {
        prompt.to_string()
    } else {
        format!(
            "{prompt}\n\nCurrent {}:\n{context}",
            match kind {
                AiKind::Script => "script",
                AiKind::Test => "tests",
                AiKind::Docs => "documentation",
                AiKind::Request => "request YAML",
            }
        )
    }
}

fn endpoint(settings: &AppSettings) -> String {
    let custom = settings
        .ai_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let base = custom.unwrap_or(match settings.ai_provider.as_str() {
        "anthropic" => "https://api.anthropic.com",
        _ => "https://api.openai.com",
    });
    let base = base.trim_end_matches('/');
    if settings.ai_provider == "anthropic" && custom.is_none() {
        format!("{base}/v1/messages")
    } else if base.ends_with("/chat/completions") || base.ends_with("/messages") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

fn strip_fences(text: &str) -> String {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '+');
    let rest = rest.trim_start_matches('\n');
    rest.strip_suffix("```")
        .unwrap_or(rest)
        .trim()
        .to_string()
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    text: Option<String>,
}

fn extract_openai(body: &str) -> Result<String, String> {
    if let Ok(parsed) = serde_json::from_str::<OpenAiResponse>(body) {
        if let Some(text) = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .filter(|s| !s.trim().is_empty())
        {
            return Ok(strip_fences(&text));
        }
    }
    Err(provider_error(body))
}

fn extract_anthropic(body: &str) -> Result<String, String> {
    if let Ok(parsed) = serde_json::from_str::<AnthropicResponse>(body) {
        let text = parsed
            .content
            .into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("");
        if !text.trim().is_empty() {
            return Ok(strip_fences(&text));
        }
    }
    Err(provider_error(body))
}

fn provider_error(body: &str) -> String {
    #[derive(Deserialize)]
    struct ErrBody {
        error: Option<ErrInner>,
        message: Option<String>,
    }
    #[derive(Deserialize)]
    struct ErrInner {
        message: Option<String>,
    }
    if let Ok(parsed) = serde_json::from_str::<ErrBody>(body) {
        if let Some(message) = parsed.error.and_then(|e| e.message).or(parsed.message) {
            if !message.trim().is_empty() {
                return format!("AI provider: {message}");
            }
        }
    }
    let snippet: String = body.chars().take(180).collect();
    format!("AI provider returned an unexpected response: {snippet}")
}

pub async fn complete(
    settings: &AppSettings,
    kind: AiKind,
    prompt: &str,
    context: &str,
) -> Result<String, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Describe what to generate".into());
    }
    if settings.ai_provider == "off" {
        return Err("AI is turned off. Enable it in Settings → AI.".into());
    }
    let model = settings.ai_model.trim();
    if model.is_empty() {
        return Err("Choose a model in Settings → AI.".into());
    }
    let key = key_get()?;
    let url = endpoint(settings);
    let user = build_user_prompt(kind, prompt, context);
    let anthropic = settings.ai_provider == "anthropic"
        && settings
            .ai_base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("AI client: {e}"))?;

    let request = if anthropic {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 2048,
            "system": system_prompt(kind),
            "messages": [{ "role": "user", "content": user }],
        });
        client
            .post(&url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
    } else {
        let body = serde_json::json!({
            "model": model,
            "temperature": 0.2,
            "messages": [
                { "role": "system", "content": system_prompt(kind) },
                { "role": "user", "content": user },
            ],
        });
        client.post(&url).bearer_auth(key).json(&body)
    };

    let response = request.send().await.map_err(|e| format!("AI request failed: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("AI response: {e}"))?;
    if !status.is_success() {
        return Err(provider_error(&text));
    }
    let text = if anthropic {
        extract_anthropic(&text)?
    } else {
        extract_openai(&text)?
    };
    if kind == AiKind::Request {
        validate_request_yaml(&text)?;
    }
    Ok(text)
}

pub fn validate_request_yaml(text: &str) -> Result<crate::model::RequestDoc, String> {
    crate::model::yaml_to(text).map_err(|e| format!("AI returned invalid request YAML: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub configured: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_defaults() {
        let mut s = AppSettings::default();
        s.ai_provider = "openai".into();
        assert_eq!(endpoint(&s), "https://api.openai.com/v1/chat/completions");
        s.ai_provider = "anthropic".into();
        assert_eq!(endpoint(&s), "https://api.anthropic.com/v1/messages");
        s.ai_base_url = Some("http://127.0.0.1:11434/v1".into());
        assert_eq!(endpoint(&s), "http://127.0.0.1:11434/v1/chat/completions");
        s.ai_base_url = Some("https://example.test/v1/chat/completions/".into());
        assert_eq!(endpoint(&s), "https://example.test/v1/chat/completions");
    }

    #[test]
    fn strips_markdown_fences() {
        assert_eq!(strip_fences("```js\nset(\"a\", 1)\n```"), "set(\"a\", 1)");
        assert_eq!(strip_fences("plain"), "plain");
    }

    #[test]
    fn extracts_provider_text() {
        let openai = r#"{"choices":[{"message":{"content":"```\nset(\"a\", 1)\n```"}}]}"#;
        assert_eq!(extract_openai(openai).unwrap(), "set(\"a\", 1)");
        let anthropic = "{\"content\":[{\"type\":\"text\",\"text\":\"# Title\"}]}";
        assert_eq!(extract_anthropic(anthropic).unwrap(), "# Title");
        let err = r#"{"error":{"message":"invalid api key"}}"#;
        assert!(extract_openai(err).unwrap_err().contains("invalid api key"));
    }

    #[test]
    fn prompt_includes_context() {
        let text = build_user_prompt(AiKind::Docs, "describe it", "GET /users");
        assert!(text.contains("describe it"));
        assert!(text.contains("GET /users"));
        assert!(system_prompt(AiKind::Test).contains("JSON array"));
        assert!(system_prompt(AiKind::Request).contains("YAML"));
    }

    #[test]
    fn rejects_invalid_request_yaml() {
        assert!(validate_request_yaml("name: only").is_err());
        let ok = validate_request_yaml(
            "schemaVersion: \"1\"\nkind: request\nname: Get\nrequest:\n  method: GET\n  url: /users\n",
        );
        assert!(ok.is_ok(), "{ok:?}");
    }
}
