//! Shared application state.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::Mutex;
use tokio::sync::RwLock;

use crate::auth::oauth2::Oauth2Cache;
use crate::cookies::CookieJar;
use crate::grpc::GrpcHub;
use crate::settings::AppSettings;
use crate::ws::WsHub;

pub struct AppState {
    /// Workspace root (absolute) when a workspace is open.
    pub workspace: Mutex<Option<PathBuf>>,
    pub settings: RwLock<AppSettings>,
    /// Session variables set by scripts (in-memory only, never persisted).
    pub transient: Mutex<BTreeMap<String, String>>,
    pub config_dir: PathBuf,
    /// OAuth2 token cache (in-memory only — never written to disk/git).
    pub oauth_cache: Oauth2Cache,
    /// App-wide cookie jar (session only).
    pub cookie_jar: StdMutex<CookieJar>,
    /// Cancellation flags for active runner executions.
    pub runs: Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
    /// Filesystem watcher for the open workspace (None until opened).
    pub watcher: StdMutex<Option<notify::RecommendedWatcher>>,
    /// Open WebSocket sessions (in-memory only).
    pub ws: Arc<WsHub>,
    /// Open gRPC server streams (in-memory only).
    pub grpc: Arc<GrpcHub>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Self {
        let settings = AppSettings::load(&config_dir);
        Self {
            workspace: Mutex::new(None),
            settings: RwLock::new(settings),
            transient: Mutex::new(BTreeMap::new()),
            config_dir,
            oauth_cache: Oauth2Cache(std::sync::Mutex::new(HashMap::new())),
            cookie_jar: StdMutex::new(CookieJar::new()),
            runs: Arc::new(StdMutex::new(HashMap::new())),
            watcher: StdMutex::new(None),
            ws: Arc::new(WsHub::new()),
            grpc: Arc::new(GrpcHub::new()),
        }
    }
}

/// Resolves secrets from the OS keychain for a specific workspace/env pair.
pub struct KeyringSecrets {
    pub workspace_root: String,
    pub env_name: String,
}

impl crate::engine::variables::SecretSource for KeyringSecrets {
    fn get(&self, name: &str) -> Result<String, String> {
        crate::secrets::get(&self.workspace_root, &self.env_name, name)
    }
}

// ---------- runner registry ----------

/// Maximum concurrent runner executions.
pub const MAX_CONCURRENT_RUNS: usize = 4;

pub type RunsMap = Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>;

/// Atomically checks the concurrency cap and registers `run_id` (single lock
/// scope, so concurrent registrations can't race past the cap). The returned
/// guard removes the entry on drop — even when the run task panics.
pub fn try_register_run(
    runs: &RunsMap,
    run_id: String,
    cancel: Arc<AtomicBool>,
) -> Result<RunGuard, String> {
    let mut map = runs.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() >= MAX_CONCURRENT_RUNS {
        return Err("Too many runner executions in flight".into());
    }
    map.insert(run_id.clone(), cancel);
    Ok(RunGuard {
        runs: runs.clone(),
        run_id,
    })
}

#[derive(Debug)]
pub struct RunGuard {
    runs: RunsMap,
    run_id: String,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn fifth_registration_fails() {
        let runs: RunsMap = Arc::new(StdMutex::new(HashMap::new()));
        let mut guards = Vec::new();
        for i in 0..MAX_CONCURRENT_RUNS {
            guards.push(try_register_run(&runs, format!("run-{i}"), flag()).expect("slot"));
        }
        let err = try_register_run(&runs, "run-5".into(), flag()).expect_err("cap reached");
        assert!(err.contains("Too many"), "{err}");
        assert_eq!(runs.lock().unwrap_or_else(|e| e.into_inner()).len(), MAX_CONCURRENT_RUNS);
    }

    #[test]
    fn dropping_guard_frees_slot() {
        let runs: RunsMap = Arc::new(StdMutex::new(HashMap::new()));
        let mut guards: Vec<RunGuard> = (0..MAX_CONCURRENT_RUNS)
            .map(|i| try_register_run(&runs, format!("run-{i}"), flag()).expect("slot"))
            .collect();
        assert!(try_register_run(&runs, "extra".into(), flag()).is_err());
        guards.pop().expect("one guard");
        let extra = try_register_run(&runs, "extra".into(), flag()).expect("slot freed");
        assert_eq!(runs.lock().unwrap_or_else(|e| e.into_inner()).len(), MAX_CONCURRENT_RUNS);
        drop(extra);
    }

    #[test]
    fn registry_survives_poisoned_lock() {
        let runs: RunsMap = Arc::new(StdMutex::new(HashMap::new()));
        let runs2 = runs.clone();
        let _ = std::panic::catch_unwind(move || {
            let _guard = runs2.lock().expect("lock");
            panic!("poison the mutex");
        });
        assert!(runs.lock().is_err(), "mutex is poisoned");
        // Registration still works instead of panicking forever.
        let guard = try_register_run(&runs, "run-1".into(), flag()).expect("slot");
        assert_eq!(runs.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
        drop(guard);
        assert!(runs.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    }
}

