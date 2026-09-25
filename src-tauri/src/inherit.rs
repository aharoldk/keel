//! Collection + folder inheritance (Keel Format v1.1).
//!
//! [`build`] loads `collection.yaml` at the workspace root plus `folder.yaml`
//! from every ancestor directory of a request and resolves the effective
//! headers / auth / scripts / variable layers. Pre-request scripts run
//! collection → folders (outer → inner) → request; post-response scripts run
//! in reverse (sandwich), see docs/CONTRACT_V2.md.

use std::collections::BTreeMap;
use std::path::Path;

use crate::model::{yaml_to, Auth, CollectionDoc, FolderDoc, KV};

/// Everything a request inherits from its ancestors.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inherited {
    /// Collection headers + folder-chain headers merged (inner wins).
    pub headers: Vec<KV>,
    /// Nearest folder/collection auth (applied by the caller only when the
    /// request has no auth of its own).
    pub auth: Option<Auth>,
    /// Collection pre-script → folder pre-scripts outer→inner. The request's
    /// own script is appended by the caller.
    pub pre_scripts: Vec<String>,
    /// Folder post-scripts inner→outer → collection post-script. The
    /// request's own script comes first (prepended by the caller).
    pub post_scripts: Vec<String>,
    /// Variable layers, lowest precedence first: collection, then folders
    /// outer→inner. Each layer keeps its own map (empty if the file had none).
    pub scope_layers: Vec<BTreeMap<String, String>>,
    /// `"collection"` or the `/`-separated relative path of the folder that
    /// provides the effective auth.
    pub source: Option<String>,
}

impl Inherited {
    /// Flattened variables with the documented precedence (later layers win).
    pub fn merged_variables(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for layer in &self.scope_layers {
            for (k, v) in layer {
                map.insert(k.clone(), v.clone());
            }
        }
        map
    }
}

/// Loads the inheritance chain for the request (or folder) at
/// `rel_request_path` under workspace `root`. Missing or invalid files are
/// skipped; this never errors.
pub fn build(root: &Path, rel_request_path: &str) -> Inherited {
    let mut out = Inherited::default();

    // Directory chain (outer → inner) that contains the request, relative to
    // the root. A path whose last segment is not a `.yaml` file is treated as
    // a folder and contributes its own folder.yaml.
    let rel = rel_request_path.replace('\\', "/");
    let mut comps: Vec<&str> = rel.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    if comps.last().is_some_and(|last| last.ends_with(".yaml")) {
        comps.pop(); // strip the request file itself
    }

    let mut collection_post: Vec<String> = Vec::new();
    let mut folder_posts: Vec<String> = Vec::new(); // outer → inner so far

    // Collection layer (only ever at the root).
    if let Some(coll) = load::<CollectionDoc>(&root.join("collection.yaml")) {
        merge_headers(&mut out.headers, coll.headers.as_deref().unwrap_or(&[]));
        push_script(
            &mut out.pre_scripts,
            coll.scripts.as_ref().and_then(|s| s.pre_request.as_deref()),
        );
        push_script(
            &mut collection_post,
            coll.scripts.as_ref().and_then(|s| s.post_response.as_deref()),
        );
        out.scope_layers
            .push(coll.variables.clone().unwrap_or_default());
        if coll.auth.is_some() {
            out.auth = coll.auth.clone();
            out.source = Some("collection".into());
        }
    }

    // Folder layers, outer → inner (never the root itself: only
    // collection.yaml is read at the root).
    for i in 1..=comps.len() {
        let dir_rel = comps[..i].join("/");
        let Some(folder) = load::<FolderDoc>(&root.join(&dir_rel).join("folder.yaml")) else {
            continue;
        };
        merge_headers(&mut out.headers, folder.headers.as_deref().unwrap_or(&[]));
        push_script(
            &mut out.pre_scripts,
            folder.scripts.as_ref().and_then(|s| s.pre_request.as_deref()),
        );
        push_script(
            &mut folder_posts,
            folder.scripts.as_ref().and_then(|s| s.post_response.as_deref()),
        );
        out.scope_layers
            .push(folder.variables.clone().unwrap_or_default());
        // Nearest folder wins over anything loaded so far.
        if folder.auth.is_some() {
            out.auth = folder.auth.clone();
            out.source = Some(dir_rel);
        }
    }

    // Sandwich order: folders inner → outer, then the collection.
    folder_posts.reverse();
    folder_posts.append(&mut collection_post);
    out.post_scripts = folder_posts;
    out
}

fn push_script(list: &mut Vec<String>, text: Option<&str>) {
    if let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) {
        list.push(t.to_string());
    }
}

/// Merges `add` into `base` by header name (case-insensitive); a later layer
/// replaces the value/enabled/kind of an earlier one in place, otherwise it
/// appends.
fn merge_headers(base: &mut Vec<KV>, add: &[KV]) {
    for kv in add {
        if let Some(existing) = base
            .iter_mut()
            .find(|h| h.name.eq_ignore_ascii_case(&kv.name))
        {
            *existing = kv.clone();
        } else {
            base.push(kv.clone());
        }
    }
}

fn load<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    yaml_to::<T>(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{yaml_of, Scripts, SCHEMA_VERSION};
    use tempfile::TempDir;

    fn kv(name: &str, value: &str) -> KV {
        KV {
            name: name.into(),
            value: value.into(),
            ..KV::default()
        }
    }

    fn write_yaml<T: serde::Serialize>(path: &Path, doc: &T) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, yaml_of(doc).unwrap()).unwrap();
    }

    fn coll(
        root: &Path,
        headers: Vec<KV>,
        auth: Option<Auth>,
        pre: &str,
        post: &str,
        vars: Vec<(&str, &str)>,
    ) {
        write_yaml(
            &root.join("collection.yaml"),
            &CollectionDoc {
                schema_version: SCHEMA_VERSION.into(),
                name: "C".into(),
                description: None,
                variables: Some(vars.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                default_environment: None,
                auth,
                headers: Some(headers),
                scripts: Some(Scripts {
                    pre_request: Some(pre.into()),
                    post_response: Some(post.into()),
                }),
                order: None,
            },
        );
    }

    fn folder(
        root: &Path,
        rel: &str,
        headers: Vec<KV>,
        auth: Option<Auth>,
        pre: &str,
        post: &str,
        vars: Vec<(&str, &str)>,
    ) {
        write_yaml(
            &root.join(rel).join("folder.yaml"),
            &FolderDoc {
                schema_version: SCHEMA_VERSION.into(),
                kind: Some("folder".into()),
                name: None,
                description: None,
                variables: Some(vars.into_iter().map(|(k, v)| (k.into(), v.into())).collect()),
                auth,
                headers: Some(headers),
                scripts: Some(Scripts {
                    pre_request: Some(pre.into()),
                    post_response: Some(post.into()),
                }),
                order: None,
            },
        );
    }

    fn bearer(token: &str) -> Option<Auth> {
        Some(Auth {
            auth_type: crate::model::AuthType::Bearer,
            token: Some(token.into()),
            ..Default::default()
        })
    }

    fn basic() -> Option<Auth> {
        Some(Auth {
            auth_type: crate::model::AuthType::Basic,
            username: Some("u".into()),
            password: Some("p".into()),
            ..Default::default()
        })
    }

    #[test]
    fn two_level_nesting_orders_scripts_sandwich() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        coll(
            root,
            vec![kv("X-A", "coll")],
            bearer("coll-tok"),
            "collPre()",
            "collPost()",
            vec![("v", "coll"), ("onlyColl", "1")],
        );
        folder(root, "a", vec![kv("X-A", "a")], None, "aPre()", "aPost()", vec![("v", "a")]);
        folder(root, "a/b", vec![], basic(), "bPre()", "bPost()", vec![("v", "b")]);

        let inh = build(root, "a/b/req.yaml");

        assert_eq!(
            inh.pre_scripts,
            vec!["collPre()", "aPre()", "bPre()"],
            "pre: collection → outer → inner"
        );
        assert_eq!(
            inh.post_scripts,
            vec!["bPost()", "aPost()", "collPost()"],
            "post sandwich: inner → outer → collection"
        );
        assert_eq!(inh.headers, vec![kv("X-A", "a")], "inner header wins in place");
        assert_eq!(
            inh.auth.as_ref().map(|a| a.auth_type),
            Some(crate::model::AuthType::Basic),
            "nearest folder with auth wins"
        );
        assert_eq!(inh.source.as_deref(), Some("a/b"));
        assert_eq!(
            inh.scope_layers.iter().map(|l| l.len()).collect::<Vec<_>>(),
            vec![2, 1, 1]
        );
        assert_eq!(
            inh.merged_variables().get("v").map(String::as_str),
            Some("b")
        );
        assert_eq!(
            inh.merged_variables().get("onlyColl").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn collection_auth_source_and_no_folders() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        coll(root, vec![], bearer("t"), "", "  ", vec![]);
        let inh = build(root, "req.yaml");
        assert_eq!(inh.source.as_deref(), Some("collection"));
        assert!(inh.pre_scripts.is_empty(), "empty/whitespace scripts skipped");
        assert!(inh.post_scripts.is_empty());
        assert_eq!(inh.scope_layers, vec![BTreeMap::new()]);
    }

    #[test]
    fn missing_files_are_skipped() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        // Nothing at all.
        let inh = build(root, "a/b/req.yaml");
        assert_eq!(inh, Inherited::default());
        // Invalid YAML in the middle folder — skipped, outer folder still loads.
        folder(root, "a", vec![kv("X-Outer", "1")], None, "p1", "q1", vec![]);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/folder.yaml"), "{{{ invalid").unwrap();
        let inh = build(root, "a/b/req.yaml");
        assert_eq!(inh.pre_scripts, vec!["p1"]);
        assert_eq!(inh.headers, vec![kv("X-Outer", "1")]);
        assert_eq!(inh.scope_layers.len(), 1);
    }

    #[test]
    fn root_folder_yaml_is_ignored() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write_yaml(
            &root.join("folder.yaml"),
            &FolderDoc {
                schema_version: SCHEMA_VERSION.into(),
                kind: None,
                name: None,
                description: None,
                variables: None,
                auth: basic(),
                headers: Some(vec![kv("X-Root", "1")]),
                scripts: None,
                order: None,
            },
        );
        let inh = build(root, "req.yaml");
        assert!(inh.headers.is_empty());
        assert!(inh.auth.is_none());
    }

    #[test]
    fn header_order_outer_first_inner_replaces_in_place() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        coll(root, vec![kv("X-A", "1"), kv("X-B", "2")], None, "", "", vec![]);
        folder(root, "a", vec![kv("x-a", "3"), kv("X-C", "4")], None, "", "", vec![]);
        let inh = build(root, "a/req.yaml");
        let names: Vec<&str> = inh.headers.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["x-a", "X-B", "X-C"], "case-insensitive replace keeps order");
        assert_eq!(inh.headers[0].value, "3");
    }
}
