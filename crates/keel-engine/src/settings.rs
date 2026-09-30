//! App settings persisted in the OS config directory (`settings.json`).
//! Mirrors `AppSettings` in the UI (see docs/IPC_CONTRACT.md).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_timeout")]
    pub request_timeout_sec: u64,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default)]
    pub save_on_send: bool,
    #[serde(default = "default_font_size")]
    pub editor_font_size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_workspaces: Vec<String>,
    // ---- v2 (docs/CONTRACT_V2.md → AppSettings) ----
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub insecure_tls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_cert_path: Option<String>,
    #[serde(default = "default_true")]
    pub send_cookies: bool,
    #[serde(default = "default_true")]
    pub store_cookies: bool,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: u64,
    /// Debounced save of dirty tabs. Off by default.
    #[serde(default)]
    pub auto_save: bool,
    /// Milliseconds to wait after the last edit before auto-saving.
    #[serde(default = "default_auto_save_interval")]
    pub auto_save_interval: u64,
    /// User overrides of the default keyboard shortcuts (`action` → combo).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub shortcuts: std::collections::BTreeMap<String, String>,
    /// `off` | `openai` | `anthropic` | `custom`. The API key is not stored here.
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    /// OpenAI-compatible base URL. Empty uses the provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_base_url: Option<String>,
}

fn default_theme() -> String {
    "dark".to_string()
}
fn default_timeout() -> u64 {
    30
}
fn default_true() -> bool {
    true
}
fn default_font_size() -> u32 {
    13
}
fn default_max_redirects() -> u64 {
    10
}
fn default_auto_save_interval() -> u64 {
    1000
}
fn default_ai_provider() -> String {
    "off".to_string()
}
fn default_ai_model() -> String {
    "gpt-4o".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            request_timeout_sec: default_timeout(),
            follow_redirects: default_true(),
            save_on_send: false,
            editor_font_size: default_font_size(),
            last_workspace: None,
            recent_workspaces: Vec::new(),
            proxy_url: None,
            insecure_tls: false,
            ca_cert_path: None,
            send_cookies: default_true(),
            store_cookies: default_true(),
            max_redirects: default_max_redirects(),
            auto_save: false,
            auto_save_interval: default_auto_save_interval(),
            shortcuts: std::collections::BTreeMap::new(),
            ai_provider: default_ai_provider(),
            ai_model: default_ai_model(),
            ai_base_url: None,
        }
    }
}

impl AppSettings {
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("settings.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(config_dir).map_err(|e| format!("config dir: {e}"))?;
        let path = config_dir.join("settings.json");
        let text =
            serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(path, text).map_err(|e| format!("write settings: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().expect("dir");
        let mut s = AppSettings::default();
        s.theme = "light".into();
        s.request_timeout_sec = 11;
        s.proxy_url = Some("http://127.0.0.1:8080".into());
        s.insecure_tls = true;
        s.ca_cert_path = Some("/tmp/ca.pem".into());
        s.send_cookies = false;
        s.store_cookies = true;
        s.max_redirects = 3;
        s.auto_save = true;
        s.auto_save_interval = 750;
        s.shortcuts.insert("save".into(), "ctrl+shift+s".into());
        s.recent_workspaces = vec!["/tmp/a".into(), "/tmp/b".into()];
        s.save(dir.path()).expect("save");
        let loaded = AppSettings::load(dir.path());
        assert_eq!(loaded.theme, "light");
        assert_eq!(loaded.request_timeout_sec, 11);
        assert_eq!(loaded.recent_workspaces, vec!["/tmp/a", "/tmp/b"]);
        assert!(loaded.follow_redirects);
        assert_eq!(loaded.proxy_url.as_deref(), Some("http://127.0.0.1:8080"));
        assert!(loaded.insecure_tls);
        assert_eq!(loaded.ca_cert_path.as_deref(), Some("/tmp/ca.pem"));
        assert!(!loaded.send_cookies);
        assert!(loaded.store_cookies);
        assert_eq!(loaded.max_redirects, 3);
        assert!(loaded.auto_save);
        assert_eq!(loaded.auto_save_interval, 750);
        assert_eq!(loaded.shortcuts.get("save").map(String::as_str), Some("ctrl+shift+s"));
    }

    #[test]
    fn legacy_file_gets_v2_defaults() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"theme":"dark","requestTimeoutSec":30,"followRedirects":true,"saveOnSend":false,"editorFontSize":13}"#,
        )
        .expect("write v1 settings");
        let loaded = AppSettings::load(dir.path());
        assert_eq!(loaded.proxy_url, None);
        assert!(!loaded.insecure_tls);
        assert_eq!(loaded.ca_cert_path, None);
        assert!(loaded.send_cookies);
        assert!(loaded.store_cookies);
        assert_eq!(loaded.max_redirects, 10);
        assert!(!loaded.auto_save);
        assert_eq!(loaded.auto_save_interval, 1000);
        assert!(loaded.shortcuts.is_empty());
        assert_eq!(loaded.ai_provider, "off");
        assert_eq!(loaded.ai_model, "gpt-4o");
        assert_eq!(loaded.ai_base_url, None);
    }

    #[test]
    fn empty_json_defaults() {
        let loaded: AppSettings = serde_json::from_str("{}").expect("defaults apply");
        assert_eq!(loaded.max_redirects, 10);
        assert!(loaded.send_cookies);
        assert!(loaded.store_cookies);
        assert!(!loaded.insecure_tls);
        assert!(!loaded.auto_save);
        assert_eq!(loaded.auto_save_interval, 1000);
        assert!(loaded.shortcuts.is_empty());
        assert_eq!(loaded.ai_provider, "off");
        assert_eq!(loaded.ai_model, "gpt-4o");
    }
}
