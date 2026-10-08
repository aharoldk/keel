//! Tauri command handlers. Thin wrappers over the engines; every command
//! validates paths against the open workspace.

use base64::Engine as _;
use std::collections::BTreeMap;
use tauri::{Emitter, State};

use crate::engine::send::{send as engine_send, SendInput};
use crate::gitutil;
use crate::history;
use crate::model::*;
use crate::secrets;
use crate::settings::AppSettings;
use crate::auth::oauth2::Oauth2Cache;
use crate::state::{AppState, KeyringSecrets};
use crate::workspace;

type Ctx<'a> = State<'a, AppState>;

async fn with_workspace<F, T>(ctx: &Ctx<'_>, f: F) -> Result<T, String>
where
    F: FnOnce(&std::path::Path) -> Result<T, String>,
{
    let ws = ctx.workspace.lock().await;
    let Some(root) = ws.as_ref() else {
        return Err("No workspace is open".into());
    };
    f(root)
}

// ---------- workspace & collection ----------

#[tauri::command]
pub async fn workspace_open(
    app: tauri::AppHandle,
    path: String,
    ctx: Ctx<'_>,
) -> Result<WorkspaceInfoDto, String> {
    let info = workspace::open_workspace(std::path::Path::new(&path))?;
    *ctx.workspace.lock().await = Some(std::path::PathBuf::from(&info.root));
    *ctx.prev.lock().await = None;
    start_watcher(&app, &ctx, std::path::Path::new(&info.root));
    Ok(info)
}

/// Watches the workspace tree and emits a debounced `workspace://changed`
/// event so the UI refreshes after external edits (git checkout, other
/// editors, …).
fn start_watcher(app: &tauri::AppHandle, ctx: &AppState, root: &std::path::Path) {
    use std::sync::Mutex as StdMutex;
    use std::time::{Duration, Instant};

    let root_watch = root.to_path_buf();
    let root = root.to_path_buf();
    let app2 = app.clone();
    let last: std::sync::Arc<StdMutex<Instant>> =
        std::sync::Arc::new(StdMutex::new(Instant::now() - Duration::from_secs(60)));
    use notify::Watcher as _;
    let mut watcher = match notify::recommended_watcher(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if !event.paths.iter().any(|p| p.starts_with(&root_watch)) {
                    return;
                }
                let mut last = match last.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if last.elapsed() < Duration::from_millis(300) {
                    return;
                }
                *last = Instant::now();
                let _ = app2.emit("workspace://changed", ());
            }
        },
    ) {
        Ok(w) => w,
        Err(_) => return,
    };
    let _ = watcher.watch(root.as_path(), notify::RecursiveMode::Recursive);
    if let Ok(mut slot) = ctx.watcher.lock() {
        *slot = Some(watcher);
    }
}

fn stop_watcher(ctx: &AppState) {
    if let Ok(mut slot) = ctx.watcher.lock() {
        *slot = None;
    }
}

#[tauri::command]
pub async fn workspace_init(path: String, name: String, ctx: Ctx<'_>) -> Result<WorkspaceInfoDto, String> {
    let info = workspace::init_workspace(std::path::Path::new(&path), &name)?;
    *ctx.workspace.lock().await = Some(std::path::PathBuf::from(&info.root));
    Ok(info)
}

#[tauri::command]
pub async fn workspace_close(ctx: Ctx<'_>) -> Result<(), String> {
    stop_watcher(&ctx);
    *ctx.workspace.lock().await = None;
    ctx.transient.lock().await.clear();
    *ctx.prev.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn workspace_info(ctx: Ctx<'_>) -> Result<Option<WorkspaceInfoDto>, String> {
    with_workspace(&ctx, |root| workspace::open_workspace(root)).await.map(Some).or(Ok(None))
}

/// Reads a workspace's display info without switching the open workspace.
/// Missing or invalid paths come back as `None` so a recent-projects list can
/// skip folders that were moved or deleted.
#[tauri::command]
pub async fn workspace_peek(path: String) -> Result<Option<WorkspaceInfoDto>, String> {
    Ok(workspace::open_workspace(std::path::Path::new(&path)).ok())
}

#[tauri::command]
pub async fn workspace_load_tree(ctx: Ctx<'_>) -> Result<Vec<TreeNodeDto>, String> {
    with_workspace(&ctx, |root| workspace::load_tree(root)).await
}

#[tauri::command]
pub async fn folder_create(parent: String, name: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::create_folder(root, &parent, &name)).await
}

#[tauri::command]
pub async fn request_create(folder: String, name: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::create_request(root, &folder, &name)).await
}

#[tauri::command]
pub async fn request_read(path: String, ctx: Ctx<'_>) -> Result<RequestDoc, String> {
    with_workspace(&ctx, |root| workspace::read_request(root, &path)).await
}

#[tauri::command]
pub async fn request_save(path: String, doc: RequestDoc, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| workspace::save_request(root, &path, &doc)).await
}

#[tauri::command]
pub async fn request_rename(path: String, new_name: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::rename_request(root, &path, &new_name)).await
}

#[tauri::command]
pub async fn request_duplicate(path: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::duplicate_request(root, &path)).await
}

#[tauri::command]
pub async fn node_delete(path: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| workspace::delete_node(root, &path)).await
}

#[tauri::command]
pub async fn node_move(path: String, dest: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::move_node(root, &path, &dest)).await
}

#[tauri::command]
pub async fn node_reorder(
    path: String,
    target: String,
    before: bool,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::reorder_node(root, &path, &target, before)).await
}

// ---------- environments & secrets ----------

#[tauri::command]
pub async fn env_list(ctx: Ctx<'_>) -> Result<Vec<EnvSummaryDto>, String> {
    with_workspace(&ctx, |root| Ok(workspace::env_list(root))).await
}

#[tauri::command]
pub async fn env_read(file_name: String, ctx: Ctx<'_>) -> Result<EnvDoc, String> {
    with_workspace(&ctx, |root| workspace::env_read(root, &file_name)).await
}

#[tauri::command]
pub async fn env_save(
    file_name: Option<String>,
    doc: EnvDoc,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::env_save(root, file_name, &doc)).await
}

#[tauri::command]
pub async fn env_delete(file_name: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| workspace::env_delete(root, &file_name)).await
}

#[tauri::command]
pub async fn env_values_read(
    file_name: String,
    ctx: Ctx<'_>,
) -> Result<BTreeMap<String, String>, String> {
    with_workspace(&ctx, |root| Ok(workspace::env_values_read(root, &file_name))).await
}

#[tauri::command]
pub async fn env_value_set(
    file_name: String,
    name: String,
    value: String,
    ctx: Ctx<'_>,
) -> Result<(), String> {
    with_workspace(&ctx, |root| workspace::env_value_set(root, &file_name, &name, &value)).await
}

#[tauri::command]
pub async fn env_value_delete(
    file_name: String,
    name: String,
    ctx: Ctx<'_>,
) -> Result<(), String> {
    with_workspace(&ctx, |root| workspace::env_value_delete(root, &file_name, &name)).await
}

#[tauri::command]
pub async fn secret_set(env_name: String, name: String, value: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| {
        secrets::set(&root.to_string_lossy(), &env_name, &name, &value)
    })
    .await
}

#[tauri::command]
pub async fn secret_delete(env_name: String, name: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| {
        secrets::delete(&root.to_string_lossy(), &env_name, &name)
    })
    .await
}

#[tauri::command]
pub async fn secret_list(env_name: String, ctx: Ctx<'_>) -> Result<Vec<String>, String> {
    with_workspace(&ctx, |root| {
        secrets::list(&root.to_string_lossy(), &env_name)
    })
    .await
}

// ---------- http ----------

#[tauri::command]
pub async fn send_request(
    app: tauri::AppHandle,
    path: String,
    env_name: Option<String>,
    doc: Option<RequestDoc>,
    ctx: Ctx<'_>,
) -> Result<SendResult, String> {
    let root = with_workspace(&ctx, |root| Ok(root.to_path_buf())).await?;

    let doc = match doc {
        Some(d) => d,
        None => workspace::read_request(&root, &path)?,
    };

    let (env_doc, env_label) = match &env_name {
        Some(file_name) => {
            let d = workspace::env_read(&root, file_name)?;
            let label = d.name.clone();
            (Some(d), Some(label))
        }
        None => (None, None),
    };

    let collection = workspace::load_collection(&root).ok();
    let workspace_doc = workspace::load_workspace_doc(&root);
    let chain = crate::inherit::build(&root, &path);
    let merged = crate::runner::merge_send_doc(&doc, &chain);

    let settings = ctx.settings.read().await.clone();
    let http = crate::engine::http::HttpOptions {
        timeout_secs: settings.request_timeout_sec.max(1),
        follow_redirects: settings.follow_redirects,
        proxy_url: settings.proxy_url.clone().filter(|s| !s.trim().is_empty()),
        insecure_tls: settings.insecure_tls,
        ca_cert_path: settings.ca_cert_path.clone().filter(|s| !s.trim().is_empty()),
        max_redirects: settings.max_redirects.max(0).min(50),
    };

    let env_display_name = env_label.clone().unwrap_or_default();
    let secret_source = KeyringSecrets {
        workspace_root: root.to_string_lossy().into_owned(),
        env_name: env_display_name,
    };
    let open_browser = move |url: &str| -> Result<(), String> {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
    };

    let mut transient = ctx.transient.lock().await.clone();
    let prev = ctx.prev.lock().await.clone();
    let env_values = match &env_name {
        Some(file_name) => workspace::env_values_read(&root, file_name),
        None => BTreeMap::new(),
    };
    let input = SendInput {
        doc: &merged,
        env_label,
        env: env_doc.as_ref(),
        collection: collection.as_ref(),
        workspace_doc: Some(&workspace_doc),
        transient: &mut transient,
        prev: prev.as_ref(),
        http,
        secret_source: &secret_source,
        request_path: Some(path.clone()),
        collection_vars: collection
            .as_ref()
            .and_then(|c| c.variables.clone())
            .unwrap_or_default(),
        folder_vars: chain.merged_variables(),
        env_values,
        env_file: env_name.clone(),
        workspace_root: Some(root.clone()),
        oauth_cache: &ctx.oauth_cache,
        open_browser: &open_browser,
        cookie_jar: Some(&ctx.cookie_jar),
        send_cookies: settings.send_cookies,
        store_cookies: settings.store_cookies,
        iteration_vars: Default::default(),
    };

    let output = engine_send(input).await;
    *ctx.transient.lock().await = transient;
    if let Some(captured) = output.captured {
        *ctx.prev.lock().await = Some(captured);
    }

    if let Some(record) = &output.history {
        history::append(&root, record)?;
    }
    Ok(output.result)
}

/// Suggestion paths for the `#{…}` previous-response tags: `status`, `body`,
/// JSON leaves under `body.*`, and `header.Name`. Values are never returned.
#[tauri::command]
pub async fn prev_refs(ctx: Ctx<'_>) -> Result<Vec<String>, String> {
    let prev = ctx.prev.lock().await.clone();
    Ok(prev
        .map(|p| crate::engine::variables::prev_ref_paths(&p))
        .unwrap_or_default())
}

// ---------- collection & folder metadata (v1.1) ----------

#[tauri::command]
pub async fn collection_read(ctx: Ctx<'_>) -> Result<CollectionDoc, String> {
    with_workspace(&ctx, |root| workspace::load_collection(root)).await
}

#[tauri::command]
pub async fn collection_save(doc: CollectionDoc, ctx: Ctx<'_>) -> Result<(), String> {
    if doc.name.trim().is_empty() {
        return Err("collection name is required".into());
    }
    with_workspace(&ctx, |root| {
        let path = root.join(workspace::COLLECTION_FILE);
        std::fs::write(path, yaml_of(&doc)?).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn folder_read(path: String, ctx: Ctx<'_>) -> Result<Option<FolderDoc>, String> {
    with_workspace(&ctx, |root| {
        let dir = workspace::safe_join(root, &path)?;
        Ok(workspace::read_folder(&dir))
    })
    .await
}

#[tauri::command]
pub async fn folder_save(path: String, doc: FolderDoc, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| {
        let dir = workspace::safe_join(root, &path)?;
        if !dir.is_dir() {
            return Err(format!("`{path}` is not a folder"));
        }
        workspace::save_folder(&dir, &doc)
    })
    .await
}

#[tauri::command]
pub async fn folder_delete_meta(path: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| {
        let dir = workspace::safe_join(root, &path)?;
        let meta = dir.join("folder.yaml");
        if meta.exists() {
            std::fs::remove_file(&meta).map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
}

// ---------- runner (v1.1) ----------

#[tauri::command]
pub async fn run_folder(
    app: tauri::AppHandle,
    path: String,
    env_name: Option<String>,
    options: RunOptions,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    // Check the cap and register atomically (single lock scope, before any
    // await) so two concurrent invocations can't both slip past the limit.
    let run_id = uuid::Uuid::new_v4().to_string();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let guard = crate::state::try_register_run(&ctx.runs, run_id.clone(), cancel.clone())?;
    let root = with_workspace(&ctx, |root| Ok(root.to_path_buf())).await?;
    let env_file_name = env_name.clone();
    let settings = ctx.settings.read().await.clone();
    let http = crate::engine::http::HttpOptions {
        timeout_secs: settings.request_timeout_sec.max(1),
        follow_redirects: settings.follow_redirects,
        proxy_url: settings.proxy_url.clone().filter(|s| !s.trim().is_empty()),
        insecure_tls: settings.insecure_tls,
        ca_cert_path: settings.ca_cert_path.clone().filter(|s| !s.trim().is_empty()),
        max_redirects: settings.max_redirects.max(0).min(50),
    };

    let ws_root = root.to_string_lossy().into_owned();
    let run_env = crate::runner::RunEnv {
        root,
        env_file_name,
        http,
        secret_source_factory: Box::new(move |env_label: &str| {
            Box::new(KeyringSecrets {
                workspace_root: ws_root.clone(),
                env_name: env_label.to_string(),
            }) as Box<dyn crate::engine::variables::SecretSource>
        }),
        oauth_cache: std::sync::Arc::new(Oauth2Cache(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        ))),
        open_browser: {
            use tauri_plugin_opener::OpenerExt;
            let app3 = app.clone();
            std::sync::Arc::new(move |url: &str| -> Result<(), String> {
                app3.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
            })
        },
        cookie_jar: std::sync::Arc::new(std::sync::Mutex::new(
            crate::cookies::CookieJar::new(),
        )),
        send_cookies: settings.send_cookies,
        store_cookies: settings.store_cookies,
    };

    let app2 = app.clone();
    let rid = run_id.clone();
    let path_for_run = path.clone();
    tauri::async_runtime::spawn(async move {
        // Dropping the guard removes the runs entry — even if the task panics.
        let _guard = guard;
        let emit = move |event: RunnerEvent| {
            let _ = app2.emit("runner://update", event);
        };
        let _summary =
            crate::runner::run_folder(&run_env, &path_for_run, &options, &emit, &cancel, rid)
                .await;
    });
    Ok(run_id.clone())
}

#[tauri::command]
pub async fn run_cancel(run_id: String, ctx: Ctx<'_>) -> Result<(), String> {
    let runs = ctx.runs.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(flag) = runs.get(&run_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

// ---------- cookies (v1.1) ----------

#[tauri::command]
pub async fn cookie_list(ctx: Ctx<'_>) -> Result<Vec<crate::model::CookieDto>, String> {
    Ok(ctx.cookie_jar.lock().unwrap_or_else(|e| e.into_inner()).list())
}

#[tauri::command]
pub async fn cookie_delete(domain: String, name: String, ctx: Ctx<'_>) -> Result<(), String> {
    ctx.cookie_jar
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .delete(&domain, &name);
    Ok(())
}

#[tauri::command]
pub async fn cookie_clear(ctx: Ctx<'_>) -> Result<(), String> {
    ctx.cookie_jar.lock().unwrap_or_else(|e| e.into_inner()).clear();
    Ok(())
}

// ---------- import / export / codegen (v1.1) ----------

#[tauri::command]
pub async fn import_postman(
    source_path: String,
    folder: String,
    ctx: Ctx<'_>,
) -> Result<crate::model::OpenApiResultDto, String> {
    with_workspace(&ctx, |root| {
        crate::import_postman::import_postman(std::path::Path::new(&source_path), root, &folder)
    })
    .await
}

#[tauri::command]
pub async fn export_openapi(folder: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| crate::export_openapi::export_openapi(root, &folder)).await
}

#[tauri::command]
pub async fn generate_code(
    path: String,
    doc: Option<RequestDoc>,
    target: String,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    with_workspace(&ctx, |root| {
        let d = match doc {
            Some(d) => d,
            None => workspace::read_request(root, &path)?,
        };
        crate::codegen::generate_code(&d, &target)
    })
    .await
}

// ---------- history ----------

#[tauri::command]
pub async fn history_list(limit: Option<usize>, ctx: Ctx<'_>) -> Result<Vec<HistoryRecord>, String> {
    with_workspace(&ctx, |root| {
        Ok(history::list(root, limit.unwrap_or(100)))
    })
    .await
}

#[tauri::command]
pub async fn history_clear(ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| history::clear(root)).await
}

#[tauri::command]
pub async fn history_pins(ctx: Ctx<'_>) -> Result<Vec<HistoryPin>, String> {
    with_workspace(&ctx, |root| Ok(history::pins(root))).await
}

#[tauri::command]
pub async fn history_pin(pin: HistoryPin, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| history::pin(root, pin)).await
}

#[tauri::command]
pub async fn history_unpin(
    ts: String,
    request_path: Option<String>,
    ctx: Ctx<'_>,
) -> Result<(), String> {
    with_workspace(&ctx, |root| history::unpin(root, &ts, request_path.as_deref())).await
}

// ---------- git ----------

#[tauri::command]
pub async fn git_status(ctx: Ctx<'_>) -> Result<gitutil::GitStatusDto, String> {
    with_workspace(&ctx, gitutil::status).await
}

#[tauri::command]
pub async fn git_stage(paths: Option<Vec<String>>, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::stage(root, paths.as_deref())).await
}

#[tauri::command]
pub async fn git_unstage(paths: Option<Vec<String>>, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::unstage(root, paths.as_deref())).await
}

#[tauri::command]
pub async fn git_discard(ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::discard(root)).await
}

#[tauri::command]
pub async fn git_commit(message: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| gitutil::commit(root, &message)).await
}

#[tauri::command]
pub async fn git_log(limit: Option<usize>, ctx: Ctx<'_>) -> Result<Vec<gitutil::GitCommitDto>, String> {
    with_workspace(&ctx, |root| gitutil::log(root, limit.unwrap_or(20))).await
}

#[tauri::command]
pub async fn git_init(ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::init(root)).await
}

#[tauri::command]
pub async fn git_diff_file(path: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| gitutil::diff_file(root, &path)).await
}

#[tauri::command]
pub async fn git_diff_commit(oid: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| gitutil::diff_commit(root, &oid)).await
}

#[tauri::command]
pub async fn git_staged_diff(ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, gitutil::staged_diff).await
}

#[tauri::command]
pub async fn git_branches(ctx: Ctx<'_>) -> Result<Vec<String>, String> {
    with_workspace(&ctx, gitutil::branches).await
}

#[tauri::command]
pub async fn git_checkout(name: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::checkout(root, &name)).await
}

#[tauri::command]
pub async fn git_create_branch(name: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::create_branch(root, &name)).await
}

#[tauri::command]
pub async fn git_remotes(ctx: Ctx<'_>) -> Result<Vec<gitutil::GitRemoteDto>, String> {
    with_workspace(&ctx, gitutil::remotes).await
}

#[tauri::command]
pub async fn git_set_remote(url: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::set_remote(root, &url)).await
}

#[tauri::command]
pub async fn git_pull(ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, gitutil::pull).await
}

#[tauri::command]
pub async fn git_push(ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, gitutil::push).await
}

#[tauri::command]
pub async fn git_resolve(path: String, side: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| gitutil::resolve(root, &path, &side)).await
}

// ---------- import / export ----------

#[tauri::command]
pub async fn import_curl(text: String, folder: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| {
        let doc = crate::import_curl::curl_to_request(&text, None)?;
        let dir = workspace::safe_join(root, &folder)?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let target = workspace::unique_file(&dir, &crate::workspace::slugify(&doc.name));
        std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())?;
        Ok(workspace::rel_path_public(root, &target))
    })
    .await
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("no file selected".into());
    }
    let file = std::path::Path::new(&path);
    if !file.is_file() {
        return Err(format!("`{path}` is not a file"));
    }
    let len = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if len > 20 * 1024 * 1024 {
        return Err("file is larger than 20 MB".into());
    }
    std::fs::read_to_string(file).map_err(|e| format!("read `{path}`: {e}"))
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    let url = url.trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Enter an http(s) URL".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(&url).send().await.map_err(|e| format!("fetch failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("fetch failed: HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("response is larger than 20 MB".into());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "response is not text".into())
}

#[tauri::command]
pub async fn import_zip(
    path: Option<String>,
    data_base64: Option<String>,
    folder: String,
    ctx: Ctx<'_>,
) -> Result<crate::model::OpenApiResultDto, String> {
    let bytes = if let Some(path) = path.filter(|p| !p.trim().is_empty()) {
        std::fs::read(&path).map_err(|e| format!("read `{path}`: {e}"))?
    } else if let Some(data) = data_base64.filter(|d| !d.trim().is_empty()) {
        base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .map_err(|e| format!("decode zip: {e}"))?
    } else {
        return Err("no zip selected".into());
    };
    if bytes.len() > 40 * 1024 * 1024 {
        return Err("zip is larger than 40 MB".into());
    }
    with_workspace(&ctx, |root| crate::import_source::import_zip(&bytes, root, &folder)).await
}

#[tauri::command]
pub async fn import_source(
    text: String,
    folder: String,
    ctx: Ctx<'_>,
) -> Result<crate::model::OpenApiResultDto, String> {
    with_workspace(&ctx, |root| crate::import_source::import_text(&text, root, &folder)).await
}

/// Clones a git repository as a sibling of the open workspace (or into the
/// chosen parent) and returns the directory that holds `collection.yaml`.
/// The UI then opens that directory as the workspace.
#[tauri::command]
pub async fn import_git(url: String, parent: Option<String>) -> Result<String, String> {
    let url = url.trim().to_string();
    if !crate::import_source::is_git_repository_url(&url) {
        return Err("Enter a git repository URL, like https://github.com/org/repo".into());
    }
    let parent_dir = match parent.filter(|p| !p.trim().is_empty()) {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    if !parent_dir.is_dir() {
        return Err(format!("`{}` is not a directory", parent_dir.display()));
    }
    let name = crate::import_source::repo_name_from_url(&url);
    let mut dest = parent_dir.join(&name);
    let mut i = 2;
    while dest.exists() {
        dest = parent_dir.join(format!("{name}-{i}"));
        i += 1;
    }
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let cloned = git2::Repository::clone(&url, &dest).map_err(|e| {
        let _ = std::fs::remove_dir_all(&dest);
        format!("clone failed: {e}")
    })?;
    drop(cloned);
    find_collection_root(&dest)
        .ok_or_else(|| {
            let _ = std::fs::remove_dir_all(&dest);
            "Cloned repository has no collection.yaml — it isn't a Keel workspace.".into()
        })
        .map(|p| p.to_string_lossy().into_owned())
}

fn find_collection_root(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if dir.join("collection.yaml").is_file() {
        return Some(dir.to_path_buf());
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if let Some(found) = find_collection_root(&path) {
                return Some(found);
            }
        }
    }
    None
}

#[tauri::command]
pub async fn import_openapi(
    source_path: String,
    folder: String,
    ctx: Ctx<'_>,
) -> Result<OpenApiResultDto, String> {
    with_workspace(&ctx, |root| {
        let result = crate::import_openapi::import_openapi(
            std::path::Path::new(&source_path),
            root,
            &folder,
        )?;
        Ok(OpenApiResultDto {
            files: result.files,
            skipped: result.skipped,
            warnings: result.warnings,
        })
    })
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiResultDto {
    pub files: Vec<String>,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn export_curl(path: String, doc: Option<RequestDoc>, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| {
        let doc = match doc {
            Some(d) => d,
            None => workspace::read_request(root, &path)?,
        };
        Ok(crate::export_curl::request_to_curl(&doc))
    })
    .await
}

#[tauri::command]
pub async fn save_response(path: String, data_base64: String, ctx: Ctx<'_>) -> Result<(), String> {
    // The path comes from the OS save dialog; still guard against empties.
    if path.trim().is_empty() {
        return Err("no destination selected".into());
    }
    let _ = ctx;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("decode response: {e}"))?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("write file: {e}"))
}

// ---------- app ----------

#[tauri::command]
pub async fn settings_get(ctx: Ctx<'_>) -> Result<AppSettings, String> {
    Ok(ctx.settings.read().await.clone())
}

#[tauri::command]
pub async fn settings_set(settings: AppSettings, ctx: Ctx<'_>) -> Result<(), String> {
    settings.save(&ctx.config_dir)?;
    *ctx.settings.write().await = settings;
    Ok(())
}

#[tauri::command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

// ---------- graphql / websocket / grpc ----------

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlIntrospectArgs {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KV>,
}

#[tauri::command]
pub async fn graphql_introspect(
    args: GraphqlIntrospectArgs,
    ctx: Ctx<'_>,
) -> Result<crate::graphql::GqlSchema, String> {
    let settings = ctx.settings.read().await.clone();
    let headers = args
        .headers
        .into_iter()
        .filter(|h| h.enabled && !h.name.trim().is_empty())
        .map(|h| (h.name, h.value))
        .collect::<Vec<_>>();
    crate::graphql::introspect(&args.url, &headers, settings.request_timeout_sec).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlBuildArgs {
    pub operation: String,
    pub root_field: String,
    #[serde(default)]
    pub selection: Vec<crate::graphql::Selection>,
}

#[tauri::command]
pub async fn graphql_build_query(args: GraphqlBuildArgs) -> Result<String, String> {
    crate::graphql::build_query(&args.operation, &args.root_field, &args.selection)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectArgs {
    pub session_id: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KV>,
    #[serde(default)]
    pub protocols: Vec<String>,
}

#[tauri::command]
pub async fn ws_connect(app: tauri::AppHandle, args: WsConnectArgs, ctx: Ctx<'_>) -> Result<(), String> {
    let headers = args
        .headers
        .into_iter()
        .filter(|h| h.enabled && !h.name.trim().is_empty())
        .map(|h| (h.name, h.value))
        .collect();
    let spec = crate::ws::WsConnect {
        url: args.url,
        headers,
        protocols: args.protocols,
    };
    let hub = ctx.ws.clone();
    let session_id = args.session_id;
    let app2 = app.clone();
    crate::ws::connect(hub, session_id, spec, move |event| {
        let _ = app2.emit("ws://message", event);
    })
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsSendArgs {
    pub session_id: String,
    pub data: String,
    #[serde(default)]
    pub binary: bool,
}

#[tauri::command]
pub async fn ws_send(args: WsSendArgs, ctx: Ctx<'_>) -> Result<(), String> {
    if args.binary {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(args.data.as_bytes())
            .map_err(|e| format!("binary frame is not base64: {e}"))?;
        ctx.ws.send_binary(&args.session_id, bytes)
    } else {
        ctx.ws.send_text(&args.session_id, args.data)
    }
}

#[tauri::command]
pub async fn ws_close(session_id: String, ctx: Ctx<'_>) -> Result<(), String> {
    ctx.ws.close(&session_id);
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcParseArgs {
    pub proto: String,
}

#[tauri::command]
pub async fn grpc_parse_proto(args: GrpcParseArgs) -> Result<crate::grpc::ProtoFile, String> {
    crate::grpc::parse_proto(&args.proto)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcCallArgs {
    pub url: String,
    pub service: String,
    pub method: String,
    pub body: serde_json::Value,
    #[serde(default)]
    pub fields: Vec<crate::grpc::ProtoField>,
    #[serde(default)]
    pub messages: Vec<crate::grpc::ProtoMessage>,
    #[serde(default)]
    pub headers: Vec<KV>,
}

#[tauri::command]
pub async fn grpc_call(args: GrpcCallArgs, ctx: Ctx<'_>) -> Result<crate::grpc::GrpcResult, String> {
    let settings = ctx.settings.read().await.clone();
    let headers = args
        .headers
        .into_iter()
        .filter(|h| h.enabled && !h.name.trim().is_empty())
        .map(|h| (h.name, h.value))
        .collect();
    crate::grpc::call_unary(crate::grpc::GrpcCall {
        url: args.url,
        service: args.service,
        method: args.method,
        body: args.body,
        fields: args.fields,
        messages: args.messages,
        headers,
        timeout_secs: settings.request_timeout_sec,
        insecure_tls: settings.insecure_tls,
    })
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcOpenArgs {
    pub session_id: String,
    pub url: String,
    pub service: String,
    pub method: String,
    pub body: serde_json::Value,
    #[serde(default)]
    pub fields: Vec<crate::grpc::ProtoField>,
    #[serde(default)]
    pub messages: Vec<crate::grpc::ProtoMessage>,
    #[serde(default)]
    pub headers: Vec<KV>,
}

#[tauri::command]
pub async fn grpc_open(app: tauri::AppHandle, args: GrpcOpenArgs, ctx: Ctx<'_>) -> Result<(), String> {
    let settings = ctx.settings.read().await.clone();
    let headers = args
        .headers
        .into_iter()
        .filter(|h| h.enabled && !h.name.trim().is_empty())
        .map(|h| (h.name, h.value))
        .collect();
    let call = crate::grpc::GrpcCall {
        url: args.url,
        service: args.service,
        method: args.method,
        body: args.body,
        fields: args.fields,
        messages: args.messages,
        headers,
        timeout_secs: settings.request_timeout_sec,
        insecure_tls: settings.insecure_tls,
    };
    let hub = ctx.grpc.clone();
    let session_id = args.session_id;
    tauri::async_runtime::spawn(async move {
        let app2 = app;
        let _ = crate::grpc::call_server_stream(hub, session_id, call, move |event| {
            let _ = app2.emit("grpc://message", event);
        })
        .await;
    });
    Ok(())
}

#[tauri::command]
pub async fn grpc_close(session_id: String, ctx: Ctx<'_>) -> Result<(), String> {
    ctx.grpc.close(&session_id);
    Ok(())
}

// ---------- ai ----------

#[tauri::command]
pub async fn ai_status() -> Result<crate::ai::AiStatus, String> {
    Ok(crate::ai::AiStatus {
        configured: crate::ai::key_configured(),
    })
}

#[tauri::command]
pub async fn ai_key_set(key: String) -> Result<(), String> {
    crate::ai::key_set(&key)
}

#[tauri::command]
pub async fn ai_key_clear() -> Result<(), String> {
    crate::ai::key_clear()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGenerateArgs {
    pub kind: String,
    pub prompt: String,
    #[serde(default)]
    pub context: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTestArgs {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
    /// Unsaved key from the settings form. Empty uses the keychain.
    #[serde(default)]
    pub key: Option<String>,
}

#[tauri::command]
pub async fn ai_test(args: AiTestArgs) -> Result<(), String> {
    let settings = crate::settings::AppSettings {
        ai_provider: args.provider,
        ai_model: args.model,
        ai_base_url: args.base_url,
        ..crate::settings::AppSettings::default()
    };
    crate::ai::test_connection(&settings, args.key.as_deref()).await
}

#[tauri::command]
pub async fn ai_generate(args: AiGenerateArgs, ctx: Ctx<'_>) -> Result<String, String> {
    let settings = ctx.settings.read().await.clone();
    let kind = crate::ai::AiKind::parse(&args.kind)?;
    crate::ai::complete(&settings, kind, &args.prompt, &args.context).await
}

#[tauri::command]
pub async fn flow_list(ctx: Ctx<'_>) -> Result<Vec<FlowSummaryDto>, String> {
    with_workspace(&ctx, |root| workspace::flow_list(root)).await
}

#[tauri::command]
pub async fn flow_tree(ctx: Ctx<'_>) -> Result<Vec<FlowTreeNodeDto>, String> {
    with_workspace(&ctx, |root| workspace::flow_tree(root)).await
}

#[tauri::command]
pub async fn flow_read(file_name: String, ctx: Ctx<'_>) -> Result<FlowDoc, String> {
    with_workspace(&ctx, |root| workspace::flow_read(root, &file_name)).await
}

#[tauri::command]
pub async fn flow_save(
    file_name: Option<String>,
    folder: Option<String>,
    doc: FlowDoc,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    with_workspace(&ctx, |root| {
        workspace::flow_save(root, file_name.as_deref(), folder.as_deref(), &doc)
    })
    .await
}

#[tauri::command]
pub async fn flow_mkdir(parent: String, name: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::flow_mkdir(root, &parent, &name)).await
}

#[tauri::command]
pub async fn flow_move(path: String, dest: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::flow_move(root, &path, &dest)).await
}

#[tauri::command]
pub async fn flow_reorder(
    path: String,
    target: String,
    before: bool,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::flow_reorder(root, &path, &target, before)).await
}

#[tauri::command]
pub async fn flow_duplicate(path: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::flow_duplicate(root, &path)).await
}

#[tauri::command]
pub async fn flow_delete(file_name: String, ctx: Ctx<'_>) -> Result<(), String> {
    with_workspace(&ctx, |root| workspace::flow_delete(root, &file_name)).await
}

#[tauri::command]
pub async fn flow_import(path: String, folder: Option<String>, ctx: Ctx<'_>) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("no flow file selected".into());
    }
    with_workspace(&ctx, |root| {
        workspace::flow_import(root, std::path::Path::new(&path), folder.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn export_environment(
    file_name: String,
    format: String,
    ctx: Ctx<'_>,
) -> Result<String, String> {
    with_workspace(&ctx, |root| {
        workspace::export_environment(root, &file_name, &format)
    })
    .await
}

#[tauri::command]
pub async fn export_request(path: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| workspace::export_request_yaml(root, &path)).await
}

#[tauri::command]
pub async fn export_collection(folder: String, ctx: Ctx<'_>) -> Result<String, String> {
    with_workspace(&ctx, |root| {
        let bytes = workspace::export_collection_zip(root, &folder)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    })
    .await
}

#[tauri::command]
pub fn request_to_yaml(doc: RequestDoc) -> Result<String, String> {
    crate::model::yaml_of(&doc)
}

#[tauri::command]
pub fn flow_to_yaml(doc: FlowDoc) -> Result<String, String> {
    crate::model::yaml_of(&doc)
}

#[tauri::command]
pub fn request_from_yaml(yaml: String) -> Result<RequestDoc, String> {
    crate::ai::validate_request_yaml(&yaml)
}
