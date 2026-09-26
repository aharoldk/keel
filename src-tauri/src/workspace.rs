//! Workspace = a directory containing `collection.yaml`. Everything on disk is
//! the source of truth; `.keel/` holds git-ignored local metadata.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::{
    yaml_of, yaml_to, CollectionDoc, EnvDoc, EnvSummaryDto, EnvValuesDoc, FlowDoc, FolderDoc,
    RequestDoc, TreeNodeDto, WorkspaceDoc, WorkspaceInfoDto, SCHEMA_VERSION,
};

pub const COLLECTION_FILE: &str = "collection.yaml";
pub const META_DIR: &str = ".keel";
pub const ENV_DIR: &str = "environments";
const IGNORED_DIRS: &[&str] = &[".git", META_DIR, "node_modules", "target", "flows"];

// ---------- path safety ----------

/// Validates a relative path (no `..`, no empty segments, `/`-separated) and
/// resolves it under the workspace root.
pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Ok(root.to_path_buf());
    }
    let rel = Path::new(relative);
    if rel.is_absolute() {
        return Err("path must be relative to the workspace".into());
    }
    for comp in rel.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => return Err(format!("illegal path segment in `{relative}`")),
        }
    }
    let joined = root.join(rel);
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("workspace root: {e}"))?;
    let canonical = canonical_under(&canonical_root, &joined, relative)?;
    if !canonical.starts_with(&canonical_root) {
        return Err(format!("path `{relative}` escapes the workspace"));
    }
    Ok(joined)
}

/// Resolves `joined` the same way as `canonical_root`, including the existing
/// prefix of a path that has not been created yet. Comparing a raw temp path
/// (`/var/...`) with its canonical form (`/private/var/...`) falsely reports
/// an escape on macOS.
fn canonical_under(canonical_root: &Path, joined: &Path, relative: &str) -> Result<PathBuf, String> {
    if joined.exists() {
        return joined
            .canonicalize()
            .map_err(|e| format!("path `{relative}`: {e}"));
    }
    let mut missing = Vec::new();
    let mut cursor = joined.to_path_buf();
    while !cursor.exists() {
        if let Some(name) = cursor.file_name() {
            missing.push(name.to_os_string());
        }
        if !cursor.pop() {
            break;
        }
    }
    let mut resolved = if cursor.exists() {
        cursor
            .canonicalize()
            .map_err(|e| format!("path `{relative}`: {e}"))?
    } else {
        canonical_root.to_path_buf()
    };
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

pub fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut out = String::new();
    let mut last_dash = false;
    for c in slug.chars() {
        if c == '-' {
            if !last_dash && !out.is_empty() {
                out.push('-');
            }
            last_dash = true;
        } else {
            out.push(c);
            last_dash = false;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "request".to_string()
    } else {
        trimmed
    }
}

fn unique_path(dir: &Path, file_stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{file_stem}.yaml"));
    let mut i = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{file_stem}-{i}.yaml"));
        i += 1;
    }
    candidate
}

// ---------- workspace lifecycle ----------

pub fn open_workspace(root: &Path) -> Result<WorkspaceInfoDto, String> {
    let canonical = root
        .canonicalize()
        .map_err(|e| format!("cannot open `{}`: {e}", root.display()))?;
    if !canonical.join(COLLECTION_FILE).is_file() {
        return Err(format!(
            "`{}` is not a Keel workspace (missing {COLLECTION_FILE})",
            canonical.display()
        ));
    }
    ensure_meta(&canonical)?;
    let collection: CollectionDoc = yaml_to(
        &std::fs::read_to_string(canonical.join(COLLECTION_FILE)).map_err(|e| e.to_string())?,
    )?;
    Ok(WorkspaceInfoDto {
        root: canonical.to_string_lossy().into_owned(),
        name: collection.name,
        has_git: canonical.join(".git").exists(),
        default_environment: collection.default_environment,
    })
}

pub fn init_workspace(root: &Path, name: &str) -> Result<WorkspaceInfoDto, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("create workspace: {e}"))?;
    let canonical = root
        .canonicalize()
        .map_err(|e| format!("resolve path: {e}"))?;

    if !canonical.join(COLLECTION_FILE).exists() {
        let collection = CollectionDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: name.to_string(),
            description: Some("A Keel API workspace".into()),
            variables: None,
            default_environment: Some("local".into()),
            auth: None,
            headers: None,
            scripts: None,
            order: None,
        };
        std::fs::write(canonical.join(COLLECTION_FILE), yaml_of(&collection)?)
            .map_err(|e| e.to_string())?;
    }

    let env_dir = canonical.join(ENV_DIR);
    std::fs::create_dir_all(&env_dir).map_err(|e| e.to_string())?;
    let local_env = env_dir.join("local.yaml");
    if !local_env.exists() {
        let env = EnvDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: "local".into(),
            description: Some("Local development".into()),
            variables: Some(
                [("baseUrl".to_string(), "http://localhost:8080".to_string())]
                    .into_iter()
                    .collect(),
            ),
            secrets: None,
        };
        std::fs::write(local_env, yaml_of(&env)?).map_err(|e| e.to_string())?;
    }

    ensure_meta(&canonical)?;

    Ok(WorkspaceInfoDto {
        root: canonical.to_string_lossy().into_owned(),
        name: name.to_string(),
        has_git: canonical.join(".git").exists(),
        default_environment: Some("local".into()),
    })
}

/// Creates `.keel/` with a workspace doc, a `.gitignore` (so the folder stays
/// out of VCS) and an empty history file.
pub fn ensure_meta(root: &Path) -> Result<(), String> {
    let meta = root.join(META_DIR);
    std::fs::create_dir_all(&meta).map_err(|e| e.to_string())?;
    let gitignore = meta.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(gitignore, "*\n").map_err(|e| e.to_string())?;
    }
    let ws_doc_path = meta.join("workspace.yaml");
    if !ws_doc_path.exists() {
        let doc = WorkspaceDoc {
            schema_version: SCHEMA_VERSION.into(),
            variables: None,
        };
        std::fs::write(ws_doc_path, yaml_of(&doc)?).map_err(|e| e.to_string())?;
    }
    let history = meta.join("history.jsonl");
    if !history.exists() {
        std::fs::write(history, "").map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn load_collection(root: &Path) -> Result<CollectionDoc, String> {
    let text = std::fs::read_to_string(root.join(COLLECTION_FILE))
        .map_err(|e| format!("read collection: {e}"))?;
    yaml_to(&text)
}

pub fn load_workspace_doc(root: &Path) -> WorkspaceDoc {
    std::fs::read_to_string(root.join(META_DIR).join("workspace.yaml"))
        .ok()
        .and_then(|t| yaml_to::<WorkspaceDoc>(&t).ok())
        .unwrap_or_default()
}

// ---------- tree ----------

pub fn load_tree(root: &Path) -> Result<Vec<TreeNodeDto>, String> {
    let order = load_collection(root).ok().and_then(|c| c.order);
    let mut nodes = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| format!("read workspace: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == COLLECTION_FILE || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) || name == ENV_DIR {
                continue;
            }
            nodes.push(folder_node(root, &path, &name));
        } else if name.ends_with(".yaml") {
            if let Some(node) = request_node(root, &path) {
                nodes.push(node);
            }
        }
    }
    sort_nodes(&mut nodes);
    if let Some(order) = order.as_deref() {
        apply_order(&mut nodes, order);
    }
    Ok(nodes)
}

fn sort_nodes(nodes: &mut [TreeNodeDto]) {
    nodes.sort_by(|a, b| {
        let ka = if a.kind == "folder" { 0 } else { 1 };
        let kb = if b.kind == "folder" { 0 } else { 1 };
        ka.cmp(&kb)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// A saved order wins; names it doesn't mention keep the fallback order after it.
fn apply_order(nodes: &mut [TreeNodeDto], order: &[String]) {
    if order.is_empty() {
        return;
    }
    nodes.sort_by_key(|n| {
        order
            .iter()
            .position(|name| name == &node_file_name(&n.path))
            .unwrap_or(usize::MAX)
    });
}

fn node_file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn folder_node(root: &Path, path: &Path, name: &str) -> TreeNodeDto {
    let mut children = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let child_path = entry.path();
            let child_name = entry.file_name().to_string_lossy().into_owned();
            if child_name.starts_with('.') {
                continue;
            }
            if child_path.is_dir() {
                if IGNORED_DIRS.contains(&child_name.as_str()) {
                    continue;
                }
                children.push(folder_node(root, &child_path, &child_name));
            } else if child_name.ends_with(".yaml") {
                if let Some(node) = request_node(root, &child_path) {
                    children.push(node);
                }
            }
        }
    }
    sort_nodes(&mut children);
    if let Some(order) = read_folder(path).and_then(|f| f.order) {
        apply_order(&mut children, &order);
    }
    TreeNodeDto {
        path: rel_path(root, path),
        name: name.to_string(),
        kind: "folder".into(),
        method: None,
        url: None,
        children: Some(children),
        meta: folder_meta_for(path),
    }
}

fn request_node(root: &Path, path: &Path) -> Option<TreeNodeDto> {
    let text = std::fs::read_to_string(path).ok()?;
    let doc: RequestDoc = yaml_to(&text).ok()?;
    Some(TreeNodeDto {
        path: rel_path(root, path),
        name: doc.name,
        kind: "request".into(),
        method: Some(doc.request.method),
        url: Some(doc.request.url),
        children: None,
        meta: None,
    })
}

/// Loads `folder.yaml` metadata for badge display in the tree.
pub fn folder_meta_for(dir: &Path) -> Option<crate::model::FolderMetaDto> {
    let folder: FolderDoc = read_folder(dir)?;
    let has_auth = folder
        .auth
        .as_ref()
        .map(|a| a.auth_type != crate::model::AuthType::None)
        .unwrap_or(false);
    let has_scripts = folder
        .scripts
        .as_ref()
        .map(|s| {
            s.pre_request.as_deref().unwrap_or("").trim().is_empty() == false
                || s.post_response.as_deref().unwrap_or("").trim().is_empty() == false
        })
        .unwrap_or(false);
    Some(crate::model::FolderMetaDto {
        has_auth,
        has_scripts,
        header_count: folder.headers.as_ref().map(|h| h.len()).unwrap_or(0),
        variable_count: folder.variables.as_ref().map(|v| v.len()).unwrap_or(0),
    })
}

/// Reads `folder.yaml` in a directory, if present.
pub fn read_folder(dir: &Path) -> Option<FolderDoc> {
    let text = std::fs::read_to_string(dir.join("folder.yaml")).ok()?;
    yaml_to(&text).ok()
}

pub fn save_folder(dir: &Path, doc: &FolderDoc) -> Result<(), String> {
    std::fs::write(dir.join("folder.yaml"), yaml_of(doc)?).map_err(|e| e.to_string())
}

// ---------- requests & folders ----------

pub fn read_request(root: &Path, relative: &str) -> Result<RequestDoc, String> {
    let path = safe_join(root, relative)?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("read `{relative}`: {e}"))?;
    yaml_to(&text)
}

pub fn save_request(root: &Path, relative: &str, doc: &RequestDoc) -> Result<(), String> {
    let path = safe_join(root, relative)?;
    if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
        return Err("request files must be .yaml".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, yaml_of(doc)?).map_err(|e| format!("write `{relative}`: {e}"))
}

pub fn create_request(root: &Path, folder: &str, name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("request name is empty".into());
    }
    let dir = safe_join(root, folder)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = unique_path(&dir, &slugify(name));
    let doc = RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: name.trim().to_string(),
        kind: "request".into(),
        description: None,
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: crate::model::RequestBlock {
            method: crate::model::HttpMethod::GET,
            url: "{{baseUrl}}/".into(),
            ..crate::model::RequestBlock::default()
        },
        auth: None,
        variables: None,
        scripts: None,
        tests: None,
    };
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())?;
    Ok(rel_path(root, &target))
}

pub fn create_folder(root: &Path, parent: &str, name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("folder name is empty".into());
    }
    let dir = safe_join(root, parent)?;
    let target = unique_dir(&dir, &slugify(name));
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    Ok(rel_path(root, &target))
}

fn unique_dir(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(stem);
    let mut i = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{i}"));
        i += 1;
    }
    candidate
}

fn rel_path(root: &Path, target: &Path) -> String {
    target
        .strip_prefix(root)
        .unwrap_or(target)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Public helpers used by the importers.
pub fn unique_file(dir: &Path, stem: &str) -> PathBuf {
    unique_path(dir, stem)
}

pub fn rel_path_public(root: &Path, target: &Path) -> String {
    rel_path(root, target)
}

pub fn rename_request(root: &Path, relative: &str, new_name: &str) -> Result<String, String> {
    let old = safe_join(root, relative)?;
    let mut doc = read_request(root, relative)?;
    doc.name = new_name.trim().to_string();
    let stem = slugify(&doc.name);
    let new_target = unique_path(old.parent().unwrap_or(root), &stem);
    if new_target != old {
        std::fs::rename(&old, &new_target).map_err(|e| format!("rename: {e}"))?;
    }
    std::fs::write(&new_target, yaml_of(&doc)?).map_err(|e| e.to_string())?;
    Ok(rel_path(root, &new_target))
}

pub fn duplicate_request(root: &Path, relative: &str) -> Result<String, String> {
    let mut doc = read_request(root, relative)?;
    let new_name = format!("{} copy", doc.name);
    doc.name = new_name;
    let old = safe_join(root, relative)?;
    let dir = old.parent().unwrap_or(root).to_path_buf();
    let target = unique_path(&dir, &slugify(&doc.name));
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())?;
    Ok(rel_path(root, &target))
}

fn is_protected(relative: &str) -> bool {
    relative == COLLECTION_FILE
        || relative == META_DIR
        || relative == ENV_DIR
        || relative.starts_with(&format!("{META_DIR}/"))
        || relative.starts_with(".git")
}

fn dir_child_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            if name.starts_with('.') || name == "folder.yaml" || name == COLLECTION_FILE {
                return false;
            }
            let path = dir.join(name);
            path.is_dir() || name.ends_with(".yaml")
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn order_of(dir: &Path, root: &Path) -> Vec<String> {
    let saved = if dir == root {
        load_collection(root).ok().and_then(|c| c.order)
    } else {
        read_folder(dir).and_then(|f| f.order)
    };
    let mut names = dir_child_names(dir);
    if let Some(saved) = saved {
        names.sort_by_key(|n| saved.iter().position(|o| o == n).unwrap_or(usize::MAX));
    }
    names
}

fn write_order(dir: &Path, root: &Path, order: Vec<String>) -> Result<(), String> {
    if dir == root {
        let mut doc = load_collection(root)?;
        doc.order = Some(order);
        std::fs::write(root.join(COLLECTION_FILE), yaml_of(&doc)?).map_err(|e| e.to_string())
    } else {
        let mut doc = read_folder(dir).unwrap_or(FolderDoc {
            schema_version: SCHEMA_VERSION.into(),
            kind: Some("folder".into()),
            name: None,
            description: None,
            variables: None,
            auth: None,
            headers: None,
            scripts: None,
            order: None,
        });
        doc.order = Some(order);
        save_folder(dir, &doc)
    }
}

/// Places `relative` before or after `target` (same folder, or moved there first).
pub fn reorder_node(
    root: &Path,
    relative: &str,
    target: &str,
    before: bool,
) -> Result<String, String> {
    let moved = move_node(root, relative, &parent_rel(target))?;
    let path = safe_join(root, &moved)?;
    let dir = path.parent().unwrap_or(root);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("`{moved}` has no name"))?;
    let anchor = Path::new(target)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut order: Vec<String> = order_of(dir, root)
        .into_iter()
        .filter(|n| n != &name)
        .collect();
    let at = order.iter().position(|n| n == &anchor).unwrap_or(order.len());
    let at = if before { at } else { (at + 1).min(order.len()) };
    order.insert(at, name);
    write_order(dir, root, order)?;
    Ok(moved)
}

fn parent_rel(relative: &str) -> String {
    match relative.rfind('/') {
        Some(i) => relative[..i].to_string(),
        None => String::new(),
    }
}

/// Moves a request file or folder into another folder (empty dest = workspace root).
/// Keeps the display name; the file stem is preserved and uniquified on collision.
pub fn move_node(root: &Path, relative: &str, dest_folder: &str) -> Result<String, String> {
    if relative.trim().is_empty() || is_protected(relative) {
        return Err(format!("`{relative}` cannot be moved"));
    }
    if !dest_folder.is_empty() && is_protected(dest_folder) {
        return Err(format!("`{dest_folder}` is not a valid destination"));
    }
    let src = safe_join(root, relative)?;
    if !src.exists() {
        return Err(format!("`{relative}` does not exist"));
    }
    let dest_dir = safe_join(root, dest_folder)?;
    if !dest_folder.is_empty() && !dest_dir.is_dir() {
        return Err(format!("`{dest_folder}` is not a folder"));
    }
    let src_rel = rel_path(root, &src);
    let dest_rel = rel_path(root, &dest_dir);
    if dest_rel == src_rel || dest_rel.starts_with(&format!("{src_rel}/")) {
        return Err("cannot move a folder into itself".into());
    }
    let current_parent = src
        .parent()
        .map(|p| rel_path(root, p))
        .unwrap_or_default();
    let current_parent = if current_parent == "." { String::new() } else { current_parent };
    if current_parent == dest_rel || (dest_folder.is_empty() && current_parent.is_empty()) {
        return Ok(src_rel);
    }
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let file_name = src
        .file_name()
        .ok_or_else(|| format!("`{relative}` has no name"))?
        .to_string_lossy()
        .into_owned();
    let mut target = dest_dir.join(&file_name);
    if target.exists() {
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| slugify(&file_name));
        target = if src.is_dir() {
            unique_dir(&dest_dir, &stem)
        } else {
            unique_path(&dest_dir, &stem)
        };
    }
    std::fs::rename(&src, &target).map_err(|e| format!("move: {e}"))?;
    Ok(rel_path(root, &target))
}

/// Deletes a file or folder. Protected paths are refused.
pub fn delete_node(root: &Path, relative: &str) -> Result<(), String> {
    if is_protected(relative) {
        return Err(format!("`{relative}` is protected and cannot be deleted"));
    }
    let path = safe_join(root, relative)?;
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("delete folder: {e}"))
    } else if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| format!("delete file: {e}"))
    } else {
        Err(format!("`{relative}` does not exist"))
    }
}

// ---------- flows ----------

pub const FLOW_DIR: &str = "flows";

pub fn flow_list(root: &Path) -> Result<Vec<crate::model::FlowSummaryDto>, String> {
    let dir = root.join(FLOW_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut items: Vec<crate::model::FlowSummaryDto> = std::fs::read_dir(&dir)
        .map_err(|e| format!("read flows: {e}"))?
        .flatten()
        .filter_map(|e| {
            let file_name = e.file_name().to_string_lossy().into_owned();
            if !file_name.ends_with(".yaml") {
                return None;
            }
            let name = flow_read(root, &file_name)
                .map(|doc| doc.name)
                .unwrap_or_else(|_| file_name.trim_end_matches(".yaml").to_string());
            Some(crate::model::FlowSummaryDto { file_name, name })
        })
        .collect();
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}

pub fn flow_read(root: &Path, file_name: &str) -> Result<FlowDoc, String> {
    let path = flow_path(root, file_name)?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read `{file_name}`: {e}"))?;
    yaml_to(&text)
}

pub fn flow_save(root: &Path, file_name: Option<&str>, doc: &FlowDoc) -> Result<String, String> {
    if doc.name.trim().is_empty() {
        return Err("flow name is empty".into());
    }
    let dir = root.join(FLOW_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = match file_name {
        Some(name) => flow_file_name(name)?,
        None => {
            let target = unique_path(&dir, &slugify(&doc.name));
            target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }
    };
    let path = dir.join(&name);
    std::fs::write(&path, yaml_of(doc)?).map_err(|e| format!("write `{name}`: {e}"))?;
    Ok(name)
}

pub fn flow_delete(root: &Path, file_name: &str) -> Result<(), String> {
    let path = flow_path(root, file_name)?;
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

/// Copies a flow YAML into `flows/`, renaming on collision. The file must
/// parse as a flow.
pub fn flow_import(root: &Path, source: &Path) -> Result<String, String> {
    if !source.is_file() {
        return Err("no flow file selected".into());
    }
    let text = std::fs::read_to_string(source).map_err(|e| format!("read flow: {e}"))?;
    let doc: FlowDoc = yaml_to(&text).map_err(|e| format!("not a flow file: {e}"))?;
    if doc.kind != "flow" {
        return Err("file is not a Keel flow".into());
    }
    flow_save(root, None, &doc)
}

/// Serializes one request file as YAML.
pub fn export_request_yaml(root: &Path, relative: &str) -> Result<String, String> {
    let doc = read_request(root, relative)?;
    if doc.kind != "request" {
        return Err("not a request file".into());
    }
    yaml_of(&doc)
}

/// Zips a folder (or the whole collection when `relative` is empty).
/// Includes request YAML, `folder.yaml`, and `collection.yaml` for a root
/// export. Skips `.git`, `.keel`, environments, and flows.
pub fn export_collection_zip(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let start = safe_join(root, relative)?;
    if !relative.is_empty() && !start.is_dir() {
        return Err(format!("folder `{relative}` not found"));
    }
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    if relative.is_empty() {
        let collection = root.join(COLLECTION_FILE);
        if collection.is_file() {
            let bytes = std::fs::read(&collection).map_err(|e| format!("read collection: {e}"))?;
            files.push((COLLECTION_FILE.to_string(), bytes));
        }
    }
    collect_export_files(root, &start, &mut files)?;
    if files.is_empty() {
        return Err("nothing to export".into());
    }
    zip_files(&files)
}

fn collect_export_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("read `{}`: {e}", dir.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with('.')
            || IGNORED_DIRS.contains(&name.as_str())
            || name == ENV_DIR
            || name == COLLECTION_FILE
        {
            continue;
        }
        if path.is_dir() {
            collect_export_files(root, &path, out)?;
        } else if name.ends_with(".yaml") {
            let rel = rel_path(root, &path);
            let bytes = std::fs::read(&path).map_err(|e| format!("read `{rel}`: {e}"))?;
            out.push((rel, bytes));
        }
    }
    Ok(())
}

fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        zip.start_file(name.replace('\\', "/"), opts)
            .map_err(|e| format!("zip `{name}`: {e}"))?;
        zip.write_all(bytes).map_err(|e| format!("zip `{name}`: {e}"))?;
    }
    let cursor = zip.finish().map_err(|e| format!("zip: {e}"))?;
    Ok(cursor.into_inner())
}

fn flow_file_name(file_name: &str) -> Result<String, String> {
    if file_name.contains('/') || file_name.contains('\\') || !file_name.ends_with(".yaml") {
        return Err("invalid flow file name".into());
    }
    Ok(file_name.to_string())
}

fn flow_path(root: &Path, file_name: &str) -> Result<PathBuf, String> {
    Ok(root.join(FLOW_DIR).join(flow_file_name(file_name)?))
}

// ---------- environments ----------

pub fn env_list(root: &Path) -> Vec<EnvSummaryDto> {
    let dir = root.join(ENV_DIR);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".yaml") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(doc) = yaml_to::<EnvDoc>(&text) {
                out.push(EnvSummaryDto {
                    file_name,
                    name: doc.name,
                    description: doc.description,
                    variable_count: doc.variables.as_ref().map(|m| m.len()).unwrap_or(0),
                    secret_count: doc.secrets.as_ref().map(|v| v.len()).unwrap_or(0),
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn env_read(root: &Path, file_name: &str) -> Result<EnvDoc, String> {
    let file_name = sanitize_file_name(file_name)?;
    let text =
        std::fs::read_to_string(root.join(ENV_DIR).join(file_name)).map_err(|e| e.to_string())?;
    yaml_to(&text)
}

pub fn env_save(root: &Path, file_name: Option<String>, doc: &EnvDoc) -> Result<String, String> {
    let target = match file_name {
        Some(existing) => root.join(ENV_DIR).join(sanitize_file_name(&existing)?),
        None => {
            let dir = root.join(ENV_DIR);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            unique_path(&dir, &slugify(&doc.name))
        }
    };
    std::fs::create_dir_all(root.join(ENV_DIR)).map_err(|e| e.to_string())?;
    std::fs::write(&target, yaml_of(doc)?).map_err(|e| e.to_string())?;
    Ok(target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
}

pub fn env_delete(root: &Path, file_name: &str) -> Result<(), String> {
    let file_name = sanitize_file_name(file_name)?;
    std::fs::remove_file(root.join(ENV_DIR).join(file_name)).map_err(|e| e.to_string())
}

// ---------- environment current values ----------

const ENV_VALUES_FILE: &str = "env-values.yaml";

fn env_values_path(root: &Path) -> PathBuf {
    root.join(META_DIR).join(ENV_VALUES_FILE)
}

fn read_env_values_doc(root: &Path) -> EnvValuesDoc {
    std::fs::read_to_string(env_values_path(root))
        .ok()
        .and_then(|t| yaml_to::<EnvValuesDoc>(&t).ok())
        .unwrap_or_default()
}

fn write_env_values_doc(root: &Path, doc: &EnvValuesDoc) -> Result<(), String> {
    let meta = root.join(META_DIR);
    std::fs::create_dir_all(&meta).map_err(|e| e.to_string())?;
    std::fs::write(env_values_path(root), yaml_of(doc)?).map_err(|e| e.to_string())
}

/// Current (local, never committed) variable values for an environment file.
pub fn env_values_read(root: &Path, file_name: &str) -> BTreeMap<String, String> {
    read_env_values_doc(root)
        .values
        .and_then(|v| v.get(file_name).cloned())
        .unwrap_or_default()
}

/// Sets (replaces) the current value of a variable for an environment.
pub fn env_value_set(root: &Path, file_name: &str, name: &str, value: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("variable name is empty".into());
    }
    let mut doc = read_env_values_doc(root);
    doc.values
        .get_or_insert_with(BTreeMap::new)
        .entry(file_name.to_string())
        .or_default()
        .insert(name.trim().to_string(), value.to_string());
    write_env_values_doc(root, &doc)
}

/// Clears the current value — the committed default value takes over again.
pub fn env_value_delete(root: &Path, file_name: &str, name: &str) -> Result<(), String> {
    let mut doc = read_env_values_doc(root);
    if let Some(values) = doc.values.as_mut() {
        if let Some(per_env) = values.get_mut(file_name) {
            per_env.remove(name);
        }
    }
    write_env_values_doc(root, &doc)
}

fn sanitize_file_name(name: &str) -> Result<String, String> {
    let cleaned = name.replace('\\', "/");
    if cleaned.contains('/') || cleaned.contains("..") || cleaned.trim().is_empty() {
        return Err(format!("illegal file name `{name}`"));
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HttpMethod;
    use tempfile::TempDir;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Get Users"), "get-users");
        assert_eq!(slugify("  Create -- user!  "), "create-user");
        assert_eq!(slugify("___"), "request");
    }

    #[test]
    fn tree_paths_are_workspace_relative() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path().join("ws");
        init_workspace(&root, "WS").expect("init");
        let top = create_request(&root, "", "Ping").expect("create top");
        let _ = create_folder(&root, "", "users").expect("folder");
        let nested = create_request(&root, "users", "Get User").expect("create nested");

        let tree = load_tree(&root).expect("tree");
        let top_node = tree
            .iter()
            .find(|n| n.kind == "request")
            .expect("top request node");
        assert_eq!(top_node.path, top);
        assert!(!std::path::Path::new(&top_node.path).is_absolute());

        let folder = tree.iter().find(|n| n.kind == "folder").expect("folder node");
        assert_eq!(folder.path, "users");
        let child = &folder.children.as_ref().expect("children")[0];
        assert_eq!(child.path, nested);
        assert_eq!(child.path, "users/get-user.yaml");

        // tree paths must round-trip through the path-validated commands
        read_request(&root, &child.path).expect("read via tree path");
        read_request(&root, &top_node.path).expect("read via tree path");
    }

    #[test]
    fn init_open_tree_cycle() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path().join("my-api");
        let info = init_workspace(&root, "My API").expect("init");
        assert_eq!(info.name, "My API");
        assert!(!info.has_git);

        let reopened = open_workspace(&root).expect("open");
        assert_eq!(
            reopened.root,
            root.canonicalize().unwrap().to_string_lossy()
        );

        let _ = create_request(&root, "", "Get Users").expect("create");
        let _ = create_folder(&root, "", "users").expect("folder");
        let _ = create_request(&root, "users", "Get User").expect("create");
        let _ = create_request(&root, "users", "Get User 2").expect("create2");

        let tree = load_tree(&root).expect("tree");
        assert!(tree.iter().any(|n| n.kind == "folder" && n.name == "users"));
        let users = tree.iter().find(|n| n.name == "users").expect("users node");
        assert_eq!(users.children.as_ref().expect("children").len(), 2);

        // environments dir is hidden from the tree
        assert!(!tree.iter().any(|n| n.name == ENV_DIR));

        let doc = read_request(&root, "users/get-user.yaml").expect("read");
        assert_eq!(doc.request.method, HttpMethod::GET);
        assert_eq!(doc.name, "Get User");

        let new_path = rename_request(&root, "users/get-user.yaml", "Fetch User").expect("rename");
        assert!(new_path.ends_with("fetch-user.yaml"));
        assert!(!root.join("users/get-user.yaml").exists());

        let dup = duplicate_request(&root, &new_path).expect("dup");
        assert!(dup.ends_with("fetch-user-copy.yaml"));

        delete_node(&root, &dup).expect("delete");
        delete_node(&root, COLLECTION_FILE).expect_err("collection protected");
        delete_node(&root, "../outside").expect_err("traversal blocked");
    }

    #[test]
    fn move_node_into_folder_and_root() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path().join("ws");
        init_workspace(&root, "WS").expect("init");
        let top = create_request(&root, "", "Ping").expect("create top");
        let users = create_folder(&root, "", "users").expect("folder");
        let nested = create_folder(&root, "users", "admin").expect("nested");

        let moved = move_node(&root, &top, &users).expect("move into folder");
        assert_eq!(moved, "users/ping.yaml");
        assert!(!root.join("ping.yaml").exists());
        assert!(root.join("users/ping.yaml").is_file());

        let back = move_node(&root, &moved, "").expect("move to root");
        assert_eq!(back, "ping.yaml");
        assert!(root.join("ping.yaml").is_file());

        let folder_moved = move_node(&root, &users, "").expect("noop at root");
        assert_eq!(folder_moved, "users");
        assert!(move_node(&root, &users, &nested).is_err());
        assert!(move_node(&root, COLLECTION_FILE, &users).is_err());
        assert!(move_node(&root, "ping.yaml", "../outside").is_err());

        let reordered = reorder_node(&root, "ping.yaml", &users, false).expect("reorder");
        assert_eq!(reordered, "ping.yaml");
        let tree = load_tree(&root).expect("tree");
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names.first().map(|s| *s), Some("users"));
        assert_eq!(names.last().map(|s| *s), Some("Ping"));

        let again = move_node(&root, "ping.yaml", &users).expect("move again");
        let collision = create_request(&root, "", "Ping").expect("second ping");
        let uniquified = move_node(&root, &collision, &users).expect("collision");
        assert_ne!(uniquified, again);
        assert!(root.join(&uniquified).is_file());
    }

    #[test]
    fn env_crud() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        init_workspace(root, "t").expect("init");
        let envs = env_list(root);
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].name, "local");

        let doc = env_read(root, "local.yaml").expect("read");
        assert_eq!(
            doc.variables
                .as_ref()
                .and_then(|m| m.get("baseUrl"))
                .map(String::as_str),
            Some("http://localhost:8080")
        );

        let file = env_save(
            root,
            None,
            &EnvDoc {
                schema_version: SCHEMA_VERSION.into(),
                name: "staging".into(),
                description: None,
                variables: Some(
                    [("baseUrl".into(), "https://staging".into())]
                        .into_iter()
                        .collect(),
                ),
                secrets: Some([("apiToken".into(), String::new())].into_iter().collect()),
            },
        )
        .expect("save");
        assert!(file.starts_with("staging"));
        assert_eq!(env_list(root).len(), 2);
        env_delete(root, &file).expect("delete");
        assert_eq!(env_list(root).len(), 1);
    }

    #[test]
    fn env_values_roundtrip() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        init_workspace(root, "t").expect("init");

        assert!(env_values_read(root, "local.yaml").is_empty());

        env_value_set(root, "local.yaml", "baseUrl", "http://localhost:9999").expect("set");
        env_value_set(root, "local.yaml", "region", "eu").expect("set second");
        env_value_set(root, "staging.yaml", "baseUrl", "https://other").expect("set other env");

        let values = env_values_read(root, "local.yaml");
        assert_eq!(values.get("baseUrl").map(String::as_str), Some("http://localhost:9999"));
        assert_eq!(values.get("region").map(String::as_str), Some("eu"));
        // Per-environment isolation.
        assert_eq!(
            env_values_read(root, "staging.yaml").get("baseUrl").map(String::as_str),
            Some("https://other")
        );

        // The file must stay inside the git-ignored `.keel/` directory.
        assert!(root.join(META_DIR).join("env-values.yaml").exists());

        // Clearing restores the default; other variables are untouched.
        env_value_delete(root, "local.yaml", "baseUrl").expect("delete");
        let values = env_values_read(root, "local.yaml");
        assert!(!values.contains_key("baseUrl"));
        assert_eq!(values.get("region").map(String::as_str), Some("eu"));

        assert!(env_value_set(root, "local.yaml", "  ", "x").is_err());
    }

    #[test]
    fn safe_join_rules() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("a/b")).expect("mkdir");
        assert_eq!(
            safe_join(root, "a/b/c.yaml").expect("ok"),
            root.join("a/b/c.yaml")
        );
        assert!(safe_join(root, "../x").is_err());
        assert!(safe_join(root, "/etc/passwd").is_err());
        assert!(safe_join(root, "a/../../x").is_err());
    }

    #[test]
    fn map_headers_deserialize() {
        let yaml = r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: http://x
  headers:
    Accept: application/json
    X-Trace: "1"
"#;
        let doc: RequestDoc = yaml_to(yaml).expect("parse");
        let headers = doc.request.headers.expect("headers");
        assert_eq!(headers.len(), 2);
        assert!(headers.iter().all(|kv| kv.enabled));
        assert!(headers.iter().any(|kv| kv.name == "Accept"));
    }

    #[test]
    fn roundtrip_preserves_arrays() {
        let yaml = r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: http://x
  params:
    - name: page
      value: "1"
      enabled: true
  headers:
    - name: Accept
      value: application/json
      enabled: false
"#;
        let doc: RequestDoc = yaml_to(yaml).expect("parse");
        assert!(!doc.request.headers.as_ref().unwrap()[0].enabled);
        let out = yaml_of(&doc).expect("ser");
        assert!(out.contains("enabled: false"));
    }

    #[test]
    fn flow_roundtrip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let name = flow_save(
            root,
            None,
            &FlowDoc {
                schema_version: SCHEMA_VERSION.into(),
                name: "Login then list".into(),
                kind: "flow".into(),
                steps: vec![
                    crate::model::FlowStep::Path("login.yaml".into()),
                    crate::model::FlowStep::Path("list.yaml".into()),
                ],
            },
        )
        .expect("save");
        assert!(name.ends_with(".yaml"));
        let doc = flow_read(root, &name).expect("read");
        assert_eq!(doc.steps.len(), 2);
        let listed = flow_list(root).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].file_name, name);
        assert_eq!(listed[0].name, "Login then list");
        flow_delete(root, &name).expect("delete");
        assert!(flow_list(root).expect("list").is_empty());
    }

    #[test]
    fn flow_import_copies_into_flows() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let src = dir.path().join("outside.yaml");
        std::fs::write(
            &src,
            "schemaVersion: \"1\"\nname: Imported\nkind: flow\nsteps:\n  - login.yaml\n",
        )
        .unwrap();
        let name = flow_import(root, &src).expect("import");
        assert!(name.ends_with(".yaml"));
        let doc = flow_read(root, &name).expect("read");
        assert_eq!(doc.name, "Imported");
        assert_eq!(doc.steps.len(), 1);
    }

    #[test]
    fn export_collection_zip_roundtrip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        init_workspace(root, "Demo").expect("init");
        create_folder(root, "", "users").expect("folder");
        create_request(root, "users", "List").expect("request");
        let bytes = export_collection_zip(root, "").expect("zip");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("archive");
        let mut names = Vec::new();
        for i in 0..archive.len() {
            names.push(archive.by_index(i).unwrap().name().to_string());
        }
        assert!(names.iter().any(|n| n == "collection.yaml"));
        assert!(names.iter().any(|n| n.ends_with(".yaml") && n.contains("users")));
        let folder_bytes = export_collection_zip(root, "users").expect("folder zip");
        let mut folder = zip::ZipArchive::new(std::io::Cursor::new(folder_bytes)).expect("folder");
        for i in 0..folder.len() {
            let name = folder.by_index(i).unwrap().name().to_string();
            assert!(name.starts_with("users/"), "{name}");
        }
    }
}
