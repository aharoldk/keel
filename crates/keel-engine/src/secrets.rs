//! Secrets live in the OS keychain, never in Git. Keyring entry:
//! service `keel`, user `<workspace>:<envName>:<varName>`.
//! Values are never returned to the UI — only names.

use keyring::Entry;

pub const SERVICE: &str = "keel";

fn account(workspace_root: &str, env_name: &str, name: &str) -> String {
    format!("{workspace_root}:{env_name}:{name}")
}

fn entry(workspace_root: &str, env_name: &str, name: &str) -> Result<Entry, String> {
    let user = account(workspace_root, env_name, name);
    Entry::new(SERVICE, &user).map_err(|e| format!("keychain unavailable: {e}"))
}

pub fn set(workspace_root: &str, env_name: &str, name: &str, value: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("secret name is empty".into());
    }
    entry(workspace_root, env_name, name)?
        .set_secret(value.as_bytes())
        .map_err(|e| format!("could not store secret: {e}"))
}

pub fn delete(workspace_root: &str, env_name: &str, name: &str) -> Result<(), String> {
    let e = entry(workspace_root, env_name, name)?;
    match e.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(format!("could not delete secret: {other}")),
    }
}

pub fn get(workspace_root: &str, env_name: &str, name: &str) -> Result<String, String> {
    let e = entry(workspace_root, env_name, name)?;
    match e.get_password() {
        Ok(v) => Ok(v),
        Err(keyring::Error::NoEntry) => Err(format!("secret `{name}` is not set in the keychain")),
        Err(other) => Err(format!("keychain error: {other}")),
    }
}

/// Declared secret names for the given environment that exist in the keychain.
/// Names come from the env file's `secrets:` map (the keyring has no
/// portable enumeration API), existence is checked per name.
pub fn list(workspace_root: &str, env_name: &str) -> Result<Vec<String>, String> {
    let env_dir = std::path::Path::new(workspace_root).join("environments");
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&env_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(doc) = crate::model::yaml_to::<crate::model::EnvDoc>(&text) {
                    if doc.name == env_name {
                        for (name, _) in doc.secrets.iter().flatten() {
                            let name = name.trim().to_string();
                            if !name.is_empty() && !names.contains(&name) {
                                names.push(name);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(names
        .into_iter()
        .filter(|n| exists(workspace_root, env_name, n))
        .collect())
}

pub fn exists(workspace_root: &str, env_name: &str, name: &str) -> bool {
    entry(workspace_root, env_name, name)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_some()
}
