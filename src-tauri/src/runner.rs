//! Sequential folder runner (no Tauri). See docs/CONTRACT_V2.md ("Scripting"
//! + runner).
//!
//! The JS runtime is not built yet, so the runner feeds the inherited
//! collection/folder scripts into `engine::send` through a merged copy of the
//! [`RequestDoc`] (see [`merge_send_doc`], also intended for the GUI
//! `send_request` path). `keel.setNextRequest`/`skipRequest` are simulated in
//! the meantime through reserved transient keys the v0 DSL can already set
//! (`set("__keel.nextRequest", "Name")`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::engine::http::HttpOptions;
use crate::engine::send::{self, SendInput};
use crate::engine::variables::SecretSource;
use crate::inherit::{self, Inherited};
use crate::model::{
    EnvDoc, KV, RequestDoc, RunOptions, RunnerEvent, RunnerItemDto, RunnerSummaryDto, Scripts,
};
use crate::{history, workspace};

/// Transient key a script can set to jump to the named request (`null` stops).
pub const KEY_NEXT_REQUEST: &str = "__keel.nextRequest";
/// Transient key a script can set to mark the current item skipped.
pub const KEY_SKIP_REQUEST: &str = "__keel.skipRequest";

const MAX_JUMPS: usize = 1000;
const IGNORED_DIRS: &[&str] = &[".git", ".keel", "environments", "node_modules", "target"];

/// Everything the runner needs that isn't a Tauri state object.
pub struct RunEnv {
    pub root: PathBuf,
    /// Environment file name (e.g. `local.yaml`); `None` = no environment.
    pub env_file_name: Option<String>,
    pub http: HttpOptions,
    /// Builds the secret source for an environment *display* name (tests pass
    /// a fake; the CLI wires a real keyring adapter).
    pub secret_source_factory: Box<dyn Fn(&str) -> Box<dyn SecretSource> + Send + Sync>,
    // ---- v2 (JS runtime, oauth2, cookies) ----
    pub oauth_cache: std::sync::Arc<crate::auth::oauth2::Oauth2Cache>,
    pub open_browser: std::sync::Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>,
    pub cookie_jar: std::sync::Arc<std::sync::Mutex<crate::cookies::CookieJar>>,
    pub send_cookies: bool,
    pub store_cookies: bool,
}


/// Merges the inheritance chain into a copy of `doc` so a single
/// `engine::send` call behaves as if collection/folder scripts, headers,
/// auth and variables were defined on the request itself.
///
/// - pre scripts: `inherit.pre_scripts` (collection → folders outer→inner),
///   then the request's own pre script, joined with newlines.
/// - post scripts (sandwich): the request's own post script first, then
///   `inherit.post_scripts` (folders inner→outer → collection).
/// - headers: inherited first, request rows win on name (case-insensitive).
/// - auth: only when the request has none of its own.
/// - variables: inherited layers (collection → folders) overridden by the
///   request's map, passed through `RequestDoc.variables` (the workspace and
///   environment layers are already applied by `engine::send` itself, which
///   also keeps env secret handling intact).
pub fn merge_send_doc(doc: &RequestDoc, inherit: &Inherited) -> RequestDoc {
    let mut merged = doc.clone();

    let req_pre = doc
        .scripts
        .as_ref()
        .and_then(|s| s.pre_request.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let req_post = doc
        .scripts
        .as_ref()
        .and_then(|s| s.post_response.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut pre_parts: Vec<&str> = inherit.pre_scripts.iter().map(String::as_str).collect();
    pre_parts.extend(req_pre);
    let mut post_parts: Vec<&str> = Vec::new();
    post_parts.extend(req_post);
    post_parts.extend(inherit.post_scripts.iter().map(String::as_str));

    merged.scripts = if pre_parts.is_empty() && post_parts.is_empty() {
        None
    } else {
        Some(Scripts {
            pre_request: (!pre_parts.is_empty()).then(|| pre_parts.join("\n")),
            post_response: (!post_parts.is_empty()).then(|| post_parts.join("\n")),
        })
    };

    let mut headers: Vec<KV> = inherit.headers.clone();
    for kv in doc.request.headers.clone().unwrap_or_default() {
        if let Some(existing) = headers
            .iter_mut()
            .find(|h| h.name.eq_ignore_ascii_case(&kv.name))
        {
            *existing = kv;
        } else {
            headers.push(kv);
        }
    }
    merged.request.headers = (!headers.is_empty()).then_some(headers);

    if merged.auth.is_none() {
        merged.auth = inherit.auth.clone();
    }

    let mut vars = inherit.merged_variables();
    for (k, v) in doc.variables.clone().unwrap_or_default() {
        vars.insert(k, v);
    }
    merged.variables = (!vars.is_empty()).then_some(vars);

    merged
}

/// Pure `keel.setNextRequest` resolution: the index of the first item *after*
/// `current` whose name matches `next` (`None` when there is no jump).
pub fn apply_next_request(list: &[String], current: usize, next: Option<String>) -> Option<usize> {
    let target = next?.trim().to_string();
    if target.is_empty() || target.eq_ignore_ascii_case("null") {
        return None;
    }
    list.iter()
        .skip(current + 1)
        .position(|n| *n == target)
        .map(|p| current + 1 + p)
}

/// `true` when a `setNextRequest(null)`/empty value means "stop the run".
pub fn halted_by_next_request(next: Option<&str>) -> bool {
    next.map(|n| n.trim().is_empty() || n.eq_ignore_ascii_case("null"))
        .unwrap_or(false)
}

/// Deterministic request list under `folder_rel` ("" = collection root):
/// folders first (alpha), then requests (alpha), recursively when asked.
/// Returns `(rel path, doc, inheritance chain)` per request.
pub fn flatten(
    root: &Path,
    folder_rel: &str,
    recursive: bool,
) -> Vec<(String, RequestDoc, Inherited)> {
    let start = match workspace::safe_join(root, folder_rel) {
        Ok(p) if p.is_dir() => p,
        _ => return Vec::new(),
    };
    let mut rel_paths = Vec::new();
    walk(&start, root, recursive, &mut rel_paths);
    rel_paths
        .iter()
        .filter_map(|rel| {
            let doc = workspace::read_request(root, rel).ok()?;
            Some((rel.clone(), doc, inherit::build(root, rel)))
        })
        .collect()
}

fn walk(dir: &Path, root: &Path, recursive: bool, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    let order = if dir == root {
        workspace::load_collection(root).ok().and_then(|c| c.order)
    } else {
        workspace::read_folder(dir).and_then(|f| f.order)
    };
    if let Some(order) = order {
        names.sort_by_key(|n| order.iter().position(|o| o == n).unwrap_or(usize::MAX));
    }
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for name in names {
        if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        let path = dir.join(&name);
        if path.is_dir() {
            dirs.push(name);
        } else if name.ends_with(".yaml")
            && name != "folder.yaml"
            && name != workspace::COLLECTION_FILE
        {
            files.push(name);
        }
    }
    if recursive {
        for name in &dirs {
            walk(&dir.join(name), root, true, out);
        }
    }
    for name in &files {
        let abs = dir.join(name);
        let rel = abs
            .strip_prefix(root)
            .unwrap_or(abs.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if workspace::read_request(root, &rel).is_ok() {
            out.push(rel);
        }
    }
}

/// Runs every request under `folder_rel` sequentially, emitting
/// [`RunnerEvent`]s (the `runner://update` payload shape) and returning the
/// summary. Cancellation is via the [`AtomicBool`] passed in; the commands
/// layer owns the run-id → flag map (the runner stays pure).
pub async fn run_folder(
    env: &RunEnv,
    folder_rel: &str,
    opts: &RunOptions,
    emit: &(dyn Fn(RunnerEvent) + Send + Sync),
    cancel: &AtomicBool,
    run_id: String,
) -> RunnerSummaryDto {
    let started = Instant::now();
    let items = flatten(&env.root, folder_rel, opts.recursive);
    let names: Vec<String> = items.iter().map(|(_, d, _)| d.name.clone()).collect();
    let n = items.len();
    let iterations = match crate::datafile::iterations(opts.data_file.as_deref()) {
        Ok(rows) => rows,
        Err(e) => {
            let summary = RunnerSummaryDto {
                total: 0,
                passed: 0,
                failed: 0,
                errored: 1,
                skipped: 0,
                duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            };
            emit(RunnerEvent {
                run_id: run_id.clone(),
                kind: "done".into(),
                item: Some(RunnerItemDto {
                    path: folder_rel.to_string(),
                    name: "data file".into(),
                    method: String::new(),
                    status: "error".into(),
                    status_code: None,
                    time_ms: 0.0,
                    size_bytes: 0,
                    tests_total: 0,
                    tests_passed: 0,
                    iteration: 0,
                    error: Some(e),
                }),
                summary: Some(summary.clone()),
            });
            return summary;
        }
    };

    let collection = workspace::load_collection(&env.root).ok();
    let workspace_doc = workspace::load_workspace_doc(&env.root);
    let env_doc: Option<EnvDoc> = env
        .env_file_name
        .as_ref()
        .and_then(|f| workspace::env_read(&env.root, f).ok());
    let env_label = env_doc.as_ref().map(|d| d.name.clone());
    let secret_names: std::collections::BTreeSet<String> = env_doc
        .as_ref()
        .and_then(|d| d.secrets.as_ref())
        .map(|secrets| {
            secrets
                .keys()
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let env_values: BTreeMap<String, String> = env
        .env_file_name
        .as_ref()
        .map(|f| workspace::env_values_read(&env.root, f))
        .unwrap_or_default();
    let secret_source = (env.secret_source_factory)(env_label.as_deref().unwrap_or(""));

    // Session variables shared across the whole run (scripts may set them).
    let mut transient: BTreeMap<String, String> = BTreeMap::new();
    let passes = iterations.len();
    let total = n.saturating_mul(passes);
    let mut statuses: Vec<Option<(RunnerItemDto, &'static str)>> = vec![None; total];
    let mut jumps = 0usize;
    let mut stopped = false;

    for (pass, row) in iterations.iter().enumerate() {
        if stopped || cancel.load(Ordering::Relaxed) {
            break;
        }
        let iteration = pass as u32;
        let iteration_vars = crate::datafile::without_secrets(row, &secret_names);
        let mut index = 0usize;
        while index < n {
        if cancel.load(Ordering::Relaxed) {
            stopped = true;
            break;
        }
        let slot = pass * n + index;
        let (rel, doc, chain) = &items[index];
        emit(item_event(
            &run_id,
            status_item(rel, &doc.name, doc.request.method.as_str(), "running", iteration, None),
        ));

        let merged = merge_send_doc(doc, chain);
        let input = SendInput {
            doc: &merged,
            env_label: env_label.clone(),
            env: env_doc.as_ref(),
            collection: collection.as_ref(),
            workspace_doc: Some(&workspace_doc),
            transient: &mut transient,
            http: env.http.clone(),
            secret_source: &*secret_source,
            request_path: Some(rel.clone()),
            collection_vars: collection
                .as_ref()
                .and_then(|c| c.variables.clone())
                .unwrap_or_default(),
            folder_vars: chain.merged_variables(),
            env_values: env_values.clone(),
            iteration_vars: iteration_vars.clone(),
            oauth_cache: &env.oauth_cache,
            open_browser: &*env.open_browser,
            cookie_jar: Some(&env.cookie_jar),
            send_cookies: env.send_cookies,
            store_cookies: env.store_cookies,
        };
        let output = send::send(input).await;
        if let Some(record) = &output.history {
            let _ = history::append(&env.root, record);
        }
        let res = &output.result;

        // Runner directives (v0 DSL bridge until the JS runtime lands).
        let next = transient.remove(KEY_NEXT_REQUEST);
        let skip_flag = transient.remove(KEY_SKIP_REQUEST).is_some();

        let tests_total = res.test_results.len();
        let tests_passed = res.test_results.iter().filter(|t| t.passed).count();
        let status: &'static str = if res.error.is_some() {
            "error"
        } else if skip_flag {
            "skipped"
        } else if tests_passed == tests_total && res.status.map(|s| s < 400).unwrap_or(false) {
            "passed"
        } else {
            "failed"
        };
        statuses[slot] = Some((
            RunnerItemDto {
                path: rel.clone(),
                name: doc.name.clone(),
                method: doc.request.method.as_str().to_string(),
                status: status.to_string(),
                status_code: res.status,
                time_ms: res.time_ms,
                size_bytes: res.size_bytes,
                tests_total,
                tests_passed,
                iteration,
                error: res.error.clone(),
            },
            status,
        ));
        emit(item_event(
            &run_id,
            statuses[slot].as_ref().expect("just set").0.clone(),
        ));

        if opts.stop_on_failure && (status == "failed" || status == "error") {
            stopped = true;
            break; // the trailing pass marks the rest skipped
        }

        if let Some(target_name) = next {
            if halted_by_next_request(Some(&target_name)) {
                break; // keel.setNextRequest(null) → stop; rest skipped
            }
            jumps += 1;
            if jumps > MAX_JUMPS {
                // A script looped the runner; report and stop.
                if index + 1 < n && statuses[pass * n + index + 1].is_none() {
                    let (rel_j, doc_j, _) = &items[index + 1];
                    let mut item = status_item(
                        rel_j,
                        &doc_j.name,
                        doc_j.request.method.as_str(),
                        "error",
                        iteration,
                        None,
                    );
                    item.error = Some("too many nextRequest jumps (possible script loop)".into());
                    statuses[pass * n + index + 1] = Some((item, "error"));
                    emit(item_event(
                        &run_id,
                        statuses[pass * n + index + 1].as_ref().expect("g").0.clone(),
                    ));
                }
                stopped = true;
                break;
            }
            match apply_next_request(&names, index, Some(target_name)) {
                Some(t) => {
                    for j in (index + 1)..t {
                        let sj = pass * n + j;
                        if statuses[sj].is_none() {
                            let (rel_j, doc_j, _) = &items[j];
                            statuses[sj] = Some((
                                status_item(
                                    rel_j,
                                    &doc_j.name,
                                    doc_j.request.method.as_str(),
                                    "skipped",
                                    iteration,
                                    None,
                                ),
                                "skipped",
                            ));
                            emit(item_event(
                                &run_id,
                                statuses[sj].as_ref().expect("s").0.clone(),
                            ));
                        }
                    }
                    index = t;
                }
                None => index += 1,
            }
        } else {
            index += 1;
        }
        let more = index < n || pass + 1 < passes;
        if more && opts.delay_ms > 0 && !cancel.load(Ordering::Relaxed) && !stopped {
            tokio::time::sleep(Duration::from_millis(opts.delay_ms)).await;
        }
        }
    }

    // Anything never visited (cancel, stop-on-failure, stop/halt, tail) = skipped.
    for (j, slot) in statuses.iter_mut().enumerate() {
        if slot.is_none() {
            let pass = j / n.max(1);
            let item_index = j % n.max(1);
            let iteration = pass as u32;
            let (rel, doc, _) = &items[item_index];
            *slot = Some((
                status_item(
                    rel,
                    &doc.name,
                    doc.request.method.as_str(),
                    "skipped",
                    iteration,
                    None,
                ),
                "skipped",
            ));
            emit(item_event(&run_id, slot.as_ref().expect("sk").0.clone()));
        }
    }

    let mut summary = RunnerSummaryDto {
        total: n,
        passed: 0,
        failed: 0,
        errored: 0,
        skipped: 0,
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
    };
    for slot in statuses.iter().flatten() {
        match slot.1 {
            "passed" => summary.passed += 1,
            "failed" => summary.failed += 1,
            "error" => summary.errored += 1,
            _ => summary.skipped += 1,
        }
    }
    emit(RunnerEvent {
        run_id,
        kind: "done".into(),
        item: None,
        summary: Some(summary.clone()),
    });
    summary
}

fn status_item(
    path: &str,
    name: &str,
    method: &str,
    status: &str,
    iteration: u32,
    error: Option<String>,
) -> RunnerItemDto {
    RunnerItemDto {
        path: path.to_string(),
        name: name.to_string(),
        method: method.to_string(),
        status: status.to_string(),
        status_code: None,
        time_ms: 0.0,
        size_bytes: 0,
        tests_total: 0,
        tests_passed: 0,
        iteration,
        error,
    }
}

fn item_event(run_id: &str, item: RunnerItemDto) -> RunnerEvent {
    RunnerEvent {
        run_id: run_id.to_string(),
        kind: "item".into(),
        item: Some(item),
        summary: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::variables::MapSecrets;
    use crate::model::{
        yaml_of, Auth, AuthType, CollectionDoc, EnvDoc, HttpMethod, RequestBlock, TestAssertion,
        SCHEMA_VERSION,
    };
    use serde_json::Value;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn req_doc(name: &str, url: String, tests: Vec<(&str, i64)>) -> RequestDoc {
        RequestDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: name.into(),
            kind: "request".into(),
            description: None,
            protocol: None,
            graphql: None,
            websocket: None,
            grpc: None,
            request: RequestBlock {
                method: HttpMethod::GET,
                url,
                ..RequestBlock::default()
            },
            auth: None,
            variables: None,
            scripts: None,
            tests: (!tests.is_empty()).then(|| {
                tests
                    .into_iter()
                    .map(|(expr, want)| TestAssertion {
                        expect: expr.into(),
                        matcher: [("toBe".to_string(), Value::from(want))]
                            .into_iter()
                            .collect(),
                    })
                    .collect()
            }),
        }
    }

    fn setup(root: &Path, base_url: &str) {
        std::fs::write(
            root.join("collection.yaml"),
            yaml_of(&CollectionDoc {
                schema_version: SCHEMA_VERSION.into(),
                name: "T".into(),
                description: None,
                variables: None,
                default_environment: None,
                auth: None,
                headers: None,
                scripts: None,
                order: None,
            })
            .unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("environments")).unwrap();
        std::fs::write(
            root.join("environments/test.yaml"),
            yaml_of(&EnvDoc {
                schema_version: SCHEMA_VERSION.into(),
                name: "test".into(),
                description: None,
                variables: Some(
                    [("baseUrl".to_string(), base_url.to_string())]
                        .into_iter()
                        .collect(),
                ),
                secrets: None,
            })
            .unwrap(),
        )
        .unwrap();
        crate::workspace::ensure_meta(root).unwrap();
    }

    fn run_env(root: &Path) -> RunEnv {
        RunEnv {
            root: root.to_path_buf(),
            env_file_name: Some("test.yaml".into()),
            http: HttpOptions::default(),
            secret_source_factory: Box::new(|_name| {
                Box::new(MapSecrets(BTreeMap::new())) as Box<dyn SecretSource>
            }),
            oauth_cache: std::sync::Arc::new(crate::auth::oauth2::Oauth2Cache(
                std::sync::Mutex::new(std::collections::HashMap::new()),
            )),
            open_browser: std::sync::Arc::new(|_| Ok(())),
            cookie_jar: std::sync::Arc::new(std::sync::Mutex::new(crate::cookies::CookieJar::new())),
            send_cookies: false,
            store_cookies: false,
        }
    }

    fn collector() -> (
        Box<dyn Fn(RunnerEvent) + Send + Sync>,
        Arc<Mutex<Vec<RunnerEvent>>>,
    ) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let s2 = sink.clone();
        (
            Box::new(move |ev: RunnerEvent| s2.lock().unwrap().push(ev)),
            sink,
        )
    }

    fn final_items(events: &[RunnerEvent]) -> Vec<(String, String)> {
        events
            .iter()
            .filter(|e| e.kind == "item")
            .filter_map(|e| e.item.as_ref())
            .filter(|i| i.status != "running")
            .map(|i| (i.path.clone(), i.status.clone()))
            .collect()
    }

    #[tokio::test]
    async fn run_passes_in_tree_order() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, &server.uri());
        std::fs::write(
            root.join("zeta.yaml"),
            yaml_of(&req_doc(
                "Zeta",
                "{{baseUrl}}/z".into(),
                vec![("response.status", 200)],
            ))
            .unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::write(
            root.join("alpha/b.yaml"),
            yaml_of(&req_doc("B", "{{baseUrl}}/b".into(), vec![])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("alpha/a.yaml"),
            yaml_of(&req_doc(
                "A",
                "{{baseUrl}}/a".into(),
                vec![("response.status", 200)],
            ))
            .unwrap(),
        )
        .unwrap();

        let (emit, sink) = collector();
        let cancel = AtomicBool::new(false);
        let opts = RunOptions {
            delay_ms: 0,
            stop_on_failure: false,
            recursive: true,
            data_file: None,
        };
        let summary = run_folder(&run_env(root), "", &opts, &*emit, &cancel, uuid::Uuid::new_v4().to_string()).await;

        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 3, "{summary:?}");
        assert_eq!(summary.failed + summary.errored + summary.skipped, 0);
        let events = sink.lock().unwrap();
        assert_eq!(final_items(&events).len(), 3);
        assert_eq!(
            final_items(&events)
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha/a.yaml", "alpha/b.yaml", "zeta.yaml"],
            "folders first, then requests, all alpha"
        );
        assert_eq!(events.last().unwrap().kind, "done");
        assert_eq!(events.last().unwrap().summary.as_ref().unwrap().total, 3);
    }

    #[tokio::test]
    async fn failure_classification_and_stop_on_failure() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/good"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/bad"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, &server.uri());
        std::fs::write(
            root.join("1-bad.yaml"),
            yaml_of(&req_doc(
                "Bad",
                "{{baseUrl}}/bad".into(),
                vec![("response.status", 200)],
            ))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("2-good.yaml"),
            yaml_of(&req_doc(
                "Good",
                "{{baseUrl}}/good".into(),
                vec![("response.status", 200)],
            ))
            .unwrap(),
        )
        .unwrap();

        let (emit, sink) = collector();
        let summary = run_folder(
            &run_env(root),
            "",
            &RunOptions {
                delay_ms: 0,
                stop_on_failure: true,
                recursive: true,
                data_file: None,
            },
            &*emit,
            &AtomicBool::new(false),
            uuid::Uuid::new_v4().to_string(),
        )
        .await;

        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.passed, 0);
        assert_eq!(
            final_items(&sink.lock().unwrap()),
            vec![
                ("1-bad.yaml".to_string(), "failed".to_string()),
                ("2-good.yaml".to_string(), "skipped".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn error_when_url_is_dead() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, "http://127.0.0.1:1");
        std::fs::write(
            root.join("dead.yaml"),
            yaml_of(&req_doc("Dead", "{{baseUrl}}/x".into(), vec![])).unwrap(),
        )
        .unwrap();
        let (emit, _sink) = collector();
        let summary = run_folder(
            &run_env(root),
            "",
            &RunOptions::default(),
            &*emit,
            &AtomicBool::new(false),
            uuid::Uuid::new_v4().to_string(),
        )
        .await;
        assert_eq!(summary.errored, 1);
    }

    #[tokio::test]
    async fn cancel_skips_everything() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, "http://127.0.0.1:1");
        std::fs::write(
            root.join("a.yaml"),
            yaml_of(&req_doc("A", "{{baseUrl}}".into(), vec![])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("b.yaml"),
            yaml_of(&req_doc("B", "{{baseUrl}}".into(), vec![])).unwrap(),
        )
        .unwrap();
        let (emit, sink) = collector();
        let cancel = AtomicBool::new(true);
        let summary = run_folder(&run_env(root), "", &RunOptions::default(), &*emit, &cancel, uuid::Uuid::new_v4().to_string()).await;
        assert_eq!(summary.skipped, 2);
        assert!(final_items(&sink.lock().unwrap())
            .iter()
            .all(|(_, s)| s == "skipped"));
    }

    #[tokio::test]
    async fn next_request_jump_skips_middle() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, &server.uri());
        let mut first = req_doc("A", "{{baseUrl}}/a".into(), vec![]);
        first.scripts = Some(Scripts {
            pre_request: Some(format!("set(\"{KEY_NEXT_REQUEST}\", \"C\")")),
            post_response: None,
        });
        std::fs::write(root.join("1-a.yaml"), yaml_of(&first).unwrap()).unwrap();
        std::fs::write(
            root.join("2-b.yaml"),
            yaml_of(&req_doc("B", "{{baseUrl}}/b".into(), vec![])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("3-c.yaml"),
            yaml_of(&req_doc("C", "{{baseUrl}}/c".into(), vec![])).unwrap(),
        )
        .unwrap();

        let (emit, sink) = collector();
        let summary = run_folder(
            &run_env(root),
            "",
            &RunOptions::default(),
            &*emit,
            &AtomicBool::new(false),
            uuid::Uuid::new_v4().to_string(),
        )
        .await;
        assert_eq!(
            final_items(&sink.lock().unwrap()),
            vec![
                ("1-a.yaml".to_string(), "passed".to_string()),
                ("2-b.yaml".to_string(), "skipped".to_string()),
                ("3-c.yaml".to_string(), "passed".to_string()),
            ]
        );
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.skipped, 1);
    }

    #[tokio::test]
    async fn skip_request_marks_current() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, &server.uri());
        let mut doc = req_doc("S", "{{baseUrl}}/s".into(), vec![]);
        doc.scripts = Some(Scripts {
            pre_request: Some(format!("set(\"{KEY_SKIP_REQUEST}\", \"1\")")),
            post_response: None,
        });
        std::fs::write(root.join("s.yaml"), yaml_of(&doc).unwrap()).unwrap();
        let (emit, _sink) = collector();
        let summary = run_folder(
            &run_env(root),
            "",
            &RunOptions::default(),
            &*emit,
            &AtomicBool::new(false),
            uuid::Uuid::new_v4().to_string(),
        )
        .await;
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.total, 1);
    }

    #[test]
    fn apply_next_request_pure() {
        let list = ["A".to_string(), "B".to_string(), "C".to_string()];
        assert_eq!(apply_next_request(&list, 0, Some("C".into())), Some(2));
        assert_eq!(apply_next_request(&list, 0, Some("B".into())), Some(1));
        assert_eq!(
            apply_next_request(&list, 1, Some("A".into())),
            None,
            "forward only"
        );
        assert_eq!(apply_next_request(&list, 0, Some("zzz".into())), None);
        assert_eq!(apply_next_request(&list, 0, None), None);
        assert_eq!(apply_next_request(&list, 0, Some("".into())), None);
        assert_eq!(apply_next_request(&list, 0, Some("null".into())), None);
        assert!(halted_by_next_request(Some("null")));
        assert!(halted_by_next_request(Some("  ")));
        assert!(!halted_by_next_request(Some("C")));
    }

    #[test]
    fn merge_send_doc_sandwiches_and_merges() {
        let mut doc = req_doc("R", "{{baseUrl}}/x".into(), vec![]);
        doc.scripts = Some(Scripts {
            pre_request: Some("reqPre()".into()),
            post_response: Some("reqPost()".into()),
        });
        doc.request.headers = Some(vec![
            KV {
                name: "X-A".into(),
                value: "req".into(),
                ..KV::default()
            },
            KV {
                name: "X-R".into(),
                value: "1".into(),
                ..KV::default()
            },
        ]);
        doc.variables = Some([("v".to_string(), "req".to_string())].into_iter().collect());

        let inh = Inherited {
            headers: vec![
                KV {
                    name: "x-a".into(),
                    value: "inh".into(),
                    ..KV::default()
                },
                KV {
                    name: "X-C".into(),
                    value: "c".into(),
                    ..KV::default()
                },
            ],
            auth: Some(Auth {
                auth_type: AuthType::Bearer,
                token: Some("t".into()),
                ..Default::default()
            }),
            pre_scripts: vec!["collPre()".into(), "foldPre()".into()],
            post_scripts: vec!["foldPost()".into(), "collPost()".into()],
            scope_layers: vec![[(
                "v".to_string(),
                "coll".to_string(),
            )]
            .into_iter()
            .collect()],
            source: Some("collection".into()),
        };

        let merged = merge_send_doc(&doc, &inh);
        assert_eq!(
            merged.scripts.as_ref().unwrap().pre_request.as_deref(),
            Some("collPre()\nfoldPre()\nreqPre()")
        );
        assert_eq!(
            merged.scripts.as_ref().unwrap().post_response.as_deref(),
            Some("reqPost()\nfoldPost()\ncollPost()")
        );
        let headers = merged.request.headers.as_ref().unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0].name, "X-A");
        assert_eq!(headers[0].value, "req", "request row wins, case-insensitive");
        assert_eq!(
            merged.auth.as_ref().map(|a| a.auth_type),
            Some(AuthType::Bearer),
            "inherited auth applied when request has none"
        );
        let vars = merged.variables.as_ref().unwrap();
        assert_eq!(vars.get("v").map(String::as_str), Some("req"));
        assert_eq!(vars.get("c"), None);

        // Request's own auth is never overridden.
        let mut doc2 = doc.clone();
        doc2.auth = Some(Auth {
            auth_type: AuthType::Basic,
            ..Default::default()
        });
        let merged2 = merge_send_doc(&doc2, &inh);
        assert_eq!(merged2.auth.unwrap().auth_type, AuthType::Basic);
    }

    #[test]
    fn flatten_skips_meta_and_orders() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        setup(root, "http://x");
        std::fs::create_dir_all(root.join("b").join("deep")).unwrap();
        std::fs::write(
            root.join("b/deep/r.yaml"),
            yaml_of(&req_doc("R", "http://x".into(), vec![])).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("b/outer.yaml"),
            yaml_of(&req_doc("O", "http://x".into(), vec![])).unwrap(),
        )
        .unwrap();
        std::fs::write(root.join("b/folder.yaml"), "kind: folder\n").unwrap();
        std::fs::create_dir_all(root.join(".keel")).unwrap();
        std::fs::write(
            root.join(".keel/hidden.yaml"),
            yaml_of(&req_doc("H", "http://x".into(), vec![])).unwrap(),
        )
        .unwrap();
        let rec: Vec<String> = flatten(root, "b", true)
            .into_iter()
            .map(|(p, _, _)| p)
            .collect();
        assert_eq!(rec, vec!["b/deep/r.yaml", "b/outer.yaml"]);
        let shallow: Vec<String> = flatten(root, "b", false)
            .into_iter()
            .map(|(p, _, _)| p)
            .collect();
        assert_eq!(shallow, vec!["b/outer.yaml"]);
        assert!(flatten(root, "does-not-exist", true).is_empty());
    }
}
