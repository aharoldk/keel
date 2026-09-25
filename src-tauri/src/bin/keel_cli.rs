//! `keel` CLI (no Tauri runtime): list / run / import curl / export openapi.
//!
//! Exit codes: 0 ok, 1 failed or errored requests, 2 bad args or paths.
//! Never prints resolved URLs (template only) or secret values.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use keel_lib::engine::http::HttpOptions;
use keel_lib::engine::send::{self, SendInput};
use keel_lib::engine::variables::SecretSource;
use keel_lib::export_openapi::export_openapi;
use keel_lib::import_curl::curl_to_request;
use keel_lib::inherit;
use keel_lib::model::{EnvDoc, RunOptions, RunnerEvent, RunnerItemDto};
use keel_lib::runner::{self, RunEnv};
use keel_lib::{history, secrets, workspace};

const USAGE: &str = "\
keel — Keel collection CLI

USAGE:
    keel <COMMAND> [OPTIONS]

COMMANDS:
    list [--folder F] [--env E]                print the collection tree
    run <paths...> [--env E] [-r] [--bail]     run requests (files) or folders
         [--delay MS] [--data FILE] [--tests-only]
         [--output FILE] [--format json]
    import curl \"<text>\" --folder F            create a request from curl
    import openapi <file> [--folder F]         import an OpenAPI 3 document
    import postman <file> [--folder F]         import a Postman v2.1 collection
    export openapi [--folder F] [-o FILE]      write an OpenAPI 3.0 document
    -V, --version                              print version
    -h, --help                                 print help
";

/// Keyring-backed secret source for the CLI (values are never printed).
struct CliSecrets {
    workspace_root: String,
    env_name: String,
}

impl SecretSource for CliSecrets {
    fn get(&self, name: &str) -> Result<String, String> {
        secrets::get(&self.workspace_root, &self.env_name, name)
    }
}

pub fn run_cli(args: &[String], cwd: &Path) -> i32 {
    let first = args.first().map(String::as_str).unwrap_or("");
    match first {
        "--version" | "-V" => {
            println!("keel {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            0
        }
        "" => {
            eprint!("{USAGE}");
            2
        }
        "list" => cmd_list(&args[1..], cwd),
        "run" => cmd_run(&args[1..], cwd),
        "import" => cmd_import(&args[1..], cwd),
        "export" => cmd_export(&args[1..], cwd),
        other => {
            eprintln!("error: unknown command `{other}`\n{USAGE}");
            2
        }
    }
}

// ---------- arg helpers ----------

#[derive(Default)]
struct Flags {
    positional: Vec<String>,
    folder: Option<String>,
    env: Option<String>,
    output: Option<String>,
    format: Option<String>,
    delay_ms: Option<u64>,
    data_file: Option<String>,
    recursive: bool,
    bail: bool,
    tests_only: bool,
}

/// Tiny manual parser: `numeric`/`bools` list the value/flag options enabled
/// in the current subcommand; anything else starting with `-` is an error.
fn parse_flags(args: &[String], numeric: &[&str], bools: &[&str]) -> Result<Flags, String> {
    let mut f = Flags::default();
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        i += 1;
        match a {
            "--folder" | "--env" | "-e" | "--output" | "-o" | "--format" | "--data" => {
                let v = args
                    .get(i)
                    .ok_or_else(|| format!("`{a}` needs a value"))?
                    .clone();
                i += 1;
                match a {
                    "--folder" => f.folder = Some(v),
                    "--env" | "-e" => f.env = Some(v),
                    "--output" | "-o" => f.output = Some(v),
                    "--data" => {
                        if !numeric.contains(&"--data") {
                            return Err(format!("`{a}` is not valid here"));
                        }
                        f.data_file = Some(v);
                    }
                    _ => f.format = Some(v),
                }
            }
            "--delay" => {
                if !numeric.contains(&"--delay") {
                    return Err(format!("`{a}` is not valid here"));
                }
                let v = args
                    .get(i)
                    .ok_or_else(|| "`--delay` needs a value".to_string())?;
                i += 1;
                f.delay_ms = Some(
                    v.parse::<u64>()
                        .map_err(|_| "--delay must be a number of milliseconds".to_string())?,
                );
            }
            "-r" | "--bail" | "--tests-only" => {
                if !bools.contains(&a) {
                    return Err(format!("`{a}` is not valid here"));
                }
                match a {
                    "-r" => f.recursive = true,
                    "--bail" => f.bail = true,
                    _ => f.tests_only = true,
                }
            }
            _ if a.starts_with('-') && a.len() > 1 => {
                return Err(format!("unknown flag `{a}`"))
            }
            _ => f.positional.push(a.to_string()),
        }
    }
    Ok(f)
}

fn discover(start: &Path) -> Option<PathBuf> {
    let mut cursor = start.to_path_buf();
    loop {
        if cursor.join(workspace::COLLECTION_FILE).is_file() {
            return Some(cursor.canonicalize().unwrap_or(cursor));
        }
        match cursor.parent() {
            Some(p) => cursor = p.to_path_buf(),
            None => return None,
        }
    }
}

fn root_or_fail(cwd: &Path, fallback_paths: &[String]) -> Result<PathBuf, String> {
    if let Some(r) = discover(cwd) {
        return Ok(r);
    }
    for p in fallback_paths {
        let candidate = cwd.join(p);
        let dir = if candidate.is_dir() {
            candidate
        } else {
            candidate.parent().map(PathBuf::from).unwrap_or_default()
        };
        if let Some(r) = discover(&dir) {
            return Ok(r);
        }
    }
    Err("no Keel workspace found (walk up for collection.yaml)".into())
}

/// Turns a CLI path argument (relative to cwd or absolute) into a
/// workspace-relative `/` path.
fn to_rel(root: &Path, cwd: &Path, raw: &str) -> Result<String, String> {
    let joined = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        cwd.join(raw)
    };
    let canonical = joined
        .canonicalize()
        .map_err(|_| format!("`{raw}` does not exist"))?;
    let rel = canonical
        .strip_prefix(root)
        .map_err(|_| format!("`{raw}` is outside the workspace `{}`", root.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    if rel.is_empty() {
        return Ok(String::new());
    }
    workspace::safe_join(root, &rel).map(|_| rel).map_err(|e| e)
}

/// Maps `--env` (file name, stem, or display name) to an env file name.
fn resolve_env_file(root: &Path, wanted: &str) -> Option<String> {
    let trimmed = wanted.trim_end_matches(".yaml");
    let direct = format!("{trimmed}.yaml");
    if root.join(workspace::ENV_DIR).join(&direct).is_file() {
        return Some(direct);
    }
    workspace::env_list(root)
        .into_iter()
        .find(|e| e.name == wanted || e.file_name == wanted)
        .map(|e| e.file_name)
}

// ---------- list ----------

fn cmd_list(args: &[String], cwd: &Path) -> i32 {
    let flags = match parse_flags(args, &[], &[]) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let root = match root_or_fail(cwd, &flags.positional) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    if let Some(env) = &flags.env {
        println!("env: {env}");
    }
    let start_rel = flags.folder.clone().unwrap_or_default();
    let start = match workspace::safe_join(&root, &start_rel) {
        Ok(p) if p.is_dir() => p,
        Ok(_) => return fail(format!("`{start_rel}` is not a folder")),
        Err(e) => return fail(e),
    };
    print_tree(&root, &start, 0);
    0
}

fn print_tree(root: &Path, dir: &Path, depth: usize) {
    let indent = "  ".repeat(depth);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    for name in names {
        if name.starts_with('.')
            || matches!(
                name.as_str(),
                ".git" | ".keel"
                    | "node_modules"
                    | "target"
                    | "environments"
                    | "collection.yaml"
                    | "folder.yaml"
            )
        {
            continue;
        }
        let path = dir.join(&name);
        if path.is_dir() {
            println!("{indent}{name}/");
            print_tree(root, &path, depth + 1);
        } else if name.ends_with(".yaml") {
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or(name.clone());
            match workspace::read_request(root, &rel) {
                Ok(doc) => println!(
                    "{indent}  {:<7} {rel}  {}",
                    doc.request.method.as_str(),
                    doc.name
                ),
                Err(_) => println!("{indent}  ???     {rel}"),
            }
        }
    }
}

// ---------- run ----------

#[derive(Default)]
struct Tally {
    passed: usize,
    failed: usize,
    errored: usize,
    skipped: usize,
    total: usize,
}

impl Tally {
    fn count(&mut self, status: &str) {
        self.total += 1;
        match status {
            "passed" => self.passed += 1,
            "failed" => self.failed += 1,
            "error" => self.errored += 1,
            _ => self.skipped += 1,
        }
    }

    fn summary_line(&self) -> String {
        format!(
            "Total {} · Passed {} · Failed {} · Errored {} · Skipped {}",
            self.total, self.passed, self.failed, self.errored, self.skipped
        )
    }

    fn exit_code(&self) -> i32 {
        if self.failed + self.errored > 0 {
            1
        } else {
            0
        }
    }
}

/// One printable result line; `url` is always the unresolved template.
fn item_line(
    method: &str,
    url: &str,
    status: &str,
    code: Option<i64>,
    time_ms: f64,
    tp: usize,
    tt: usize,
    error: Option<&str>,
) -> String {
    match status {
        "error" => format!("ERROR  {method}  {url}  {}", error.unwrap_or("unknown error")),
        "skipped" => format!("SKIP   {method}  {url}"),
        "running" => String::new(),
        _ => {
            let label = if status == "passed" { "PASS" } else { "FAIL" };
            let mut line = format!(
                "{label}  {method}  {url}  {}  {:.0}ms",
                code.map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
                time_ms
            );
            if tt > 0 {
                line.push_str(&format!("  {tp}/{tt} tests"));
            }
            line
        }
    }
}

fn cmd_run(args: &[String], cwd: &Path) -> i32 {
    let flags = match parse_flags(args, &["--delay", "--data"], &["-r", "--bail", "--tests-only"]) {
        Ok(f) if !f.positional.is_empty() => f,
        Ok(_) => return fail("run needs at least one path"),
        Err(e) => return fail(e),
    };
    if let Some(fmt) = &flags.format {
        if fmt != "json" {
            return fail(format!("unknown format `{fmt}` (json only)"));
        }
    }
    let root = match root_or_fail(cwd, &flags.positional) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let _ = workspace::ensure_meta(&root);

    // Resolve every path up front so bad args exit 2 before any request runs.
    let mut targets: Vec<(String, bool)> = Vec::new(); // (rel, is_dir)
    for raw in &flags.positional {
        let rel = match to_rel(&root, cwd, raw) {
            Ok(r) => r,
            Err(e) => return fail(e),
        };
        let is_dir = if rel.is_empty() { true } else { root.join(&rel).is_dir() };
        if !is_dir && workspace::read_request(&root, &rel).is_err() {
            return fail(format!("`{raw}` is not a request or folder"));
        }
        targets.push((rel, is_dir));
    }

    let env_file = match &flags.env {
        Some(w) => match resolve_env_file(&root, w) {
            Some(f) => Some(f),
            None => return fail(format!("environment `{w}` not found")),
        },
        None => None,
    };
    let env_doc: Option<EnvDoc> = env_file
        .as_ref()
        .and_then(|f| workspace::env_read(&root, f).ok());
    let env_label = env_doc.as_ref().map(|d| d.name.clone());
    let env_values: BTreeMap<String, String> = env_file
        .as_ref()
        .map(|f| workspace::env_values_read(&root, f))
        .unwrap_or_default();
    let collection = workspace::load_collection(&root).ok();
    let workspace_doc = workspace::load_workspace_doc(&root);

    let data_rows = match keel_lib::datafile::iterations(flags.data_file.as_deref()) {
        Ok(rows) => rows,
        Err(e) => return fail(e),
    };
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
    let opts = RunOptions {
        delay_ms: flags.delay_ms.unwrap_or(0),
        stop_on_failure: flags.bail,
        recursive: true, // -r accepted for compatibility; folders recurse by default
        data_file: flags.data_file.clone(),
    };

    let mut tally = Tally::default();
    let mut json_items: Vec<serde_json::Value> = Vec::new();
    let mut transient: BTreeMap<String, String> = BTreeMap::new();
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return fail(format!("runtime: {e}")),
    };

    for (rel, is_dir) in &targets {
        if *is_dir {
            let lookup: BTreeMap<String, (String, String)> =
                runner::flatten(&root, rel, opts.recursive)
                    .into_iter()
                    .map(|(p, d, _)| {
                        (p, (d.request.method.as_str().to_string(), d.request.url.clone()))
                    })
                    .collect();
            let root_for_secrets = root.clone();
            let run_env = RunEnv {
                root: root.clone(),
                env_file_name: env_file.clone(),
                http: HttpOptions::default(),
                secret_source_factory: Box::new(move |name: &str| {
                    Box::new(CliSecrets {
                        workspace_root: root_for_secrets.to_string_lossy().into_owned(),
                        env_name: name.to_string(),
                    }) as Box<dyn SecretSource>
                }),
                oauth_cache: std::sync::Arc::new(keel_lib::auth::oauth2::Oauth2Cache(
                    std::sync::Mutex::new(std::collections::HashMap::new()),
                )),
                open_browser: std::sync::Arc::new(|url: &str| {
                    eprintln!("note: open this URL to complete OAuth2: {url}");
                    Ok(())
                }),
                cookie_jar: std::sync::Arc::new(std::sync::Mutex::new(
                    keel_lib::cookies::CookieJar::new(),
                )),
                send_cookies: false,
                store_cookies: false,
            };
            {
                let tal_cell = std::sync::Mutex::new(std::mem::take(&mut tally));
                let json_cell = std::sync::Mutex::new(std::mem::take(&mut json_items));
                let emit = |ev: RunnerEvent| {
                    let Some(item) = &ev.item else { return };
                    if item.status == "running" {
                        return;
                    }
                    let (method, url) = lookup
                        .get(&item.path)
                        .cloned()
                        .unwrap_or_else(|| (item.method.clone(), item.path.clone()));
                    record_item(
                        &mut tal_cell.lock().unwrap(),
                        &mut json_cell.lock().unwrap(),
                        &method,
                        &url,
                        item,
                        flags.tests_only,
                    );
                };
                rt.block_on(runner::run_folder(
                    &run_env,
                    rel,
                    &opts,
                    &emit,
                    &AtomicBool::new(false),
                    uuid::Uuid::new_v4().to_string(),
                ));
                tally = tal_cell.into_inner().unwrap();
                json_items = json_cell.into_inner().unwrap();
            }
        } else {
            let doc = match workspace::read_request(&root, rel) {
                Ok(d) => d,
                Err(e) => return fail(e),
            };
            let chain = inherit::build(&root, rel);
            let merged = runner::merge_send_doc(&doc, &chain);
            let secret_source = CliSecrets {
                workspace_root: root.to_string_lossy().into_owned(),
                env_name: env_label.clone().unwrap_or_default(),
            };
            let cli_oauth_cache = std::sync::Arc::new(keel_lib::auth::oauth2::Oauth2Cache(
                std::sync::Mutex::new(std::collections::HashMap::new()),
            ));
            let cli_cookie_jar = std::sync::Arc::new(std::sync::Mutex::new(
                keel_lib::cookies::CookieJar::new(),
            ));
            let open_browser = |url: &str| -> Result<(), String> {
                eprintln!("note: open this URL to complete OAuth2: {url}");
                Ok(())
            };
            for (pass, row) in data_rows.iter().enumerate() {
            let iteration_vars = keel_lib::datafile::without_secrets(row, &secret_names);
            let output = rt.block_on(send::send(SendInput {
                doc: &merged,
                env_label: env_label.clone(),
                env: env_doc.as_ref(),
                collection: collection.as_ref(),
                workspace_doc: Some(&workspace_doc),
                transient: &mut transient,
                http: HttpOptions::default(),
                secret_source: &secret_source,
                request_path: Some(rel.clone()),
                collection_vars: collection
                    .as_ref()
                    .and_then(|c| c.variables.clone())
                    .unwrap_or_default(),
                folder_vars: chain.merged_variables(),
                env_values: env_values.clone(),
                env_file: env_file.clone(),
                workspace_root: Some(root.clone()),
                oauth_cache: &cli_oauth_cache,
                open_browser: &open_browser,
                cookie_jar: Some(&cli_cookie_jar),
                send_cookies: false,
                store_cookies: false,
                iteration_vars,
            }));
            if let Some(record) = &output.history {
                let _ = history::append(&root, record);
            }
            let res = &output.result;
            let tests_total = res.test_results.len();
            let tests_passed = res.test_results.iter().filter(|t| t.passed).count();
            let status = if res.error.is_some() {
                "error"
            } else if flags.tests_only && tests_total == 0 {
                "skipped"
            } else if tests_passed == tests_total && res.status.map(|s| s < 400).unwrap_or(false) {
                "passed"
            } else {
                "failed"
            };
            let item = RunnerItemDto {
                path: rel.clone(),
                name: doc.name.clone(),
                method: doc.request.method.as_str().to_string(),
                status: status.to_string(),
                status_code: res.status,
                time_ms: res.time_ms,
                size_bytes: res.size_bytes,
                tests_total,
                tests_passed,
                iteration: pass as u32,
                error: res.error.clone(),
            };
            record_item(
                &mut tally,
                &mut json_items,
                doc.request.method.as_str(),
                &doc.request.url,
                &item,
                flags.tests_only,
            );
            if flags.bail && (status == "failed" || status == "error") {
                break;
            }
            }
        }
    }

    println!("{}", tally.summary_line());
    if let Some(out) = &flags.output {
        let report = serde_json::json!({
            "items": json_items,
            "summary": {
                "total": tally.total, "passed": tally.passed, "failed": tally.failed,
                "errored": tally.errored, "skipped": tally.skipped,
            },
        });
        let text = serde_json::to_string_pretty(&report).expect("serialize");
        if let Err(e) = write_file(Path::new(out), cwd, &text) {
            return fail(e);
        }
    }
    tally.exit_code()
}

fn record_item(
    tally: &mut Tally,
    json: &mut Vec<serde_json::Value>,
    method: &str,
    url: &str,
    item: &RunnerItemDto,
    tests_only: bool,
) {
    let mut status = item.status.as_str();
    if tests_only && item.tests_total == 0 && matches!(status, "passed" | "failed") {
        status = "skipped";
    }
    let line = item_line(
        method,
        url,
        status,
        item.status_code,
        item.time_ms,
        item.tests_passed,
        item.tests_total,
        item.error.as_deref(),
    );
    if !line.is_empty() {
        println!("{line}");
    }
    tally.count(status);
    json.push(serde_json::json!({
        "path": item.path,
        "name": item.name,
        "method": method,
        "url": url,
        "status": status,
        "statusCode": item.status_code,
        "timeMs": (item.time_ms * 100.0).round() / 100.0,
        "sizeBytes": item.size_bytes,
        "testsPassed": item.tests_passed,
        "testsTotal": item.tests_total,
        "error": item.error,
    }));
}

fn write_file(path: &Path, cwd: &Path, text: &str) -> Result<(), String> {
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f =
        std::fs::File::create(&target).map_err(|e| format!("open `{}`: {e}", target.display()))?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- import / export ----------

fn cmd_import(args: &[String], cwd: &Path) -> i32 {
    match args.first().map(String::as_str) {
        Some("curl") => {}
        Some("openapi") | Some("postman") => {
            return cmd_import_spec(args.first().unwrap(), &args[1..], cwd);
        }
        Some(other) => {
            return fail(format!(
                "unknown import source `{other}` (curl | openapi | postman)"
            ));
        }
        None => return fail("keel import needs: curl | openapi | postman"),
    }
    let flags = match parse_flags(&args[1..], &[], &[]) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let text = match flags.positional.first() {
        Some(t) => t.clone(),
        None => return fail("import curl needs the curl command text"),
    };
    let root = match root_or_fail(cwd, &[]) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let doc = match curl_to_request(&text, None) {
        Ok(d) => d,
        Err(e) => return fail(format!("curl parse error: {e}")),
    };
    let folder = flags.folder.clone().unwrap_or_default();
    let created = match workspace::create_request(&root, &folder, &doc.name) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    if let Err(e) = workspace::save_request(&root, &created, &doc) {
        return fail(e);
    }
    println!("{created}");
    0
}

/// `keel import openapi|postman <file> [--folder F]`. Same engine as the GUI.
fn cmd_import_spec(kind: &str, args: &[String], cwd: &Path) -> i32 {
    let flags = match parse_flags(args, &[], &[]) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let raw = match flags.positional.first() {
        Some(p) => p.clone(),
        None => return fail(format!("import {kind} needs a source file")),
    };
    let source = if Path::new(&raw).is_absolute() {
        PathBuf::from(&raw)
    } else {
        cwd.join(&raw)
    };
    if !source.is_file() {
        return fail(format!("`{raw}` is not a file"));
    }
    let root = match root_or_fail(cwd, &[raw.clone()]) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let folder = flags.folder.clone().unwrap_or_default();
    let result = match kind {
        "openapi" => keel_lib::import_openapi::import_openapi(&source, &root, &folder)
            .map(|r| keel_lib::model::OpenApiResultDto {
                files: r.files,
                skipped: r.skipped,
                warnings: r.warnings,
            }),
        _ => keel_lib::import_postman::import_postman(&source, &root, &folder),
    };
    let result = match result {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    for file in &result.files {
        println!("{file}");
    }
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }
    if result.skipped > 0 {
        eprintln!("{} request(s) skipped", result.skipped);
    }
    0
}

fn cmd_export(args: &[String], cwd: &Path) -> i32 {
    if args.first().map(String::as_str) != Some("openapi") {
        return fail("keel export supports only: openapi [--folder F] [-o FILE]");
    }
    let flags = match parse_flags(&args[1..], &[], &[]) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let root = match root_or_fail(cwd, &flags.positional) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    // NOTE: `keel_lib::export_openapi` is the R2-owned module referenced by
    // path per the R3 spec; `keel_lib::import_postman` likewise exists for a
    // future `import postman` subcommand.
    let json = match export_openapi(&root, flags.folder.as_deref().unwrap_or("")) {
        Ok(j) => j,
        Err(e) => return fail(e),
    };
    match &flags.output {
        Some(out) => {
            if let Err(e) = write_file(Path::new(out), cwd, &json) {
                return fail(e);
            }
            println!("{out}");
        }
        None => println!("{json}"),
    }
    0
}

fn fail(msg: impl std::fmt::Display) -> i32 {
    eprintln!("error: {msg}");
    2
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    if !args.is_empty() {
        args.remove(0);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    std::process::exit(run_cli(&args, &cwd));
}
