//! Git engine (git2). Status, stage, commit, log, diff, init, plus sync:
//! branches, remotes, pull and push. Pull and push call the system `git`
//! binary so SSH keys and credential helpers work (libgit2 has no
//! askpass).

use std::path::Path;
use std::process::Command;

use git2::{Repository, Status};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitEntry {
    pub path: String,
    pub status: String,
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusDto {
    pub has_repo: bool,
    pub branch: Option<String>,
    pub entries: Vec<GitEntry>,
    /// `origin` URL when configured.
    pub remote_url: Option<String>,
    /// Commits on the current branch not yet on its upstream.
    /// `None` when the branch tracks nothing.
    pub ahead: Option<usize>,
    /// Commits on the upstream not yet in the current branch.
    pub behind: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteDto {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitDto {
    pub oid: String,
    pub short_oid: String,
    pub message: String,
    pub author: String,
    pub time: String,
}

fn open(root: &Path) -> Result<Repository, String> {
    Repository::open(root).map_err(|e| format!("not a git repository: {e}"))
}

fn status_char_to_string(status: Status) -> &'static str {
    if status.contains(Status::CONFLICTED) {
        "conflicted"
    } else if status.contains(Status::WT_RENAMED) || status.contains(Status::INDEX_RENAMED) {
        "renamed"
    } else if status.contains(Status::WT_NEW) || status.contains(Status::INDEX_NEW) {
        "added"
    } else if status.contains(Status::WT_DELETED) || status.contains(Status::INDEX_DELETED) {
        "deleted"
    } else {
        "modified"
    }
}

pub fn status(root: &Path) -> Result<GitStatusDto, String> {
    let repo = match open(root) {
        Ok(repo) => repo,
        Err(_) => {
            return Ok(GitStatusDto {
                has_repo: false,
                branch: None,
                entries: vec![],
                remote_url: None,
                ahead: None,
                behind: None,
            })
        }
    };

    let branch = current_branch(&repo);
    let mut entries = Vec::new();
    let statuses = repo.statuses(None).map_err(|e| e.to_string())?;
    for entry in statuses.iter() {
        let s = entry.status();
        let path = entry
            .head_to_index()
            .and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
            .or_else(|| {
                entry
                    .index_to_workdir()
                    .and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
            })
            .or_else(|| {
                entry
                    .head_to_index()
                    .and_then(|d| d.old_file().path().map(|p| p.to_string_lossy().into_owned()))
            })
            .unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        if s.contains(Status::INDEX_NEW)
            || s.contains(Status::INDEX_MODIFIED)
            || s.contains(Status::INDEX_DELETED)
            || s.contains(Status::INDEX_RENAMED)
        {
            entries.push(GitEntry {
                path: path.clone(),
                status: status_char_to_string(s).to_string(),
                staged: true,
            });
        }
        if s.contains(Status::WT_NEW)
            || s.contains(Status::WT_MODIFIED)
            || s.contains(Status::WT_DELETED)
            || s.contains(Status::WT_RENAMED)
            || s.contains(Status::CONFLICTED)
        {
            entries.push(GitEntry {
                path,
                status: status_char_to_string(s).to_string(),
                staged: false,
            });
        }
    }
    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|r| r.url().map(|u| u.to_string()));
    let (ahead, behind) = ahead_behind(&repo);

    Ok(GitStatusDto {
        has_repo: true,
        branch,
        entries,
        remote_url,
        ahead,
        behind,
    })
}

fn ahead_behind(repo: &Repository) -> (Option<usize>, Option<usize>) {
    let head = match repo.head().ok().and_then(|h| h.target()) {
        Some(oid) => oid,
        None => return (None, None),
    };
    let upstream = match repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()))
        .and_then(|name| repo.find_branch(&name, git2::BranchType::Local).ok())
        .and_then(|b| b.upstream().ok())
        .and_then(|u| u.get().target())
    {
        Some(oid) => oid,
        None => return (None, None),
    };
    repo.graph_ahead_behind(head, upstream)
        .ok()
        .map(|(a, b)| (Some(a), Some(b)))
        .unwrap_or((None, None))
}

fn current_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    head.shorthand().map(|s| s.to_string())
}

fn signature(repo: &Repository) -> Result<git2::Signature<'static>, String> {
    match repo.signature() {
        Ok(sig) => Ok(sig),
        Err(_) => git2::Signature::now("keel", "keel@localhost").map_err(|e| e.to_string()),
    }
}

pub fn stage(root: &Path, paths: Option<&[String]>) -> Result<(), String> {
    let repo = open(root)?;
    let mut index = repo.index().map_err(|e| e.to_string())?;

    if paths.is_none() {
        // Add everything: new + modified + deleted worktree files.
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| e.to_string())?;
        index
            .update_all(["*"].iter(), None)
            .map_err(|e| e.to_string())?;
    } else {
        for path in paths.unwrap() {
            match index.add_path(std::path::Path::new(path)) {
                Ok(()) => {}
                Err(_) => {
                    // Probably deleted on disk → record the deletion.
                    index
                        .remove_path(std::path::Path::new(path))
                        .map_err(|e| format!("stage `{path}`: {e}"))?;
                }
            }
        }
    }
    index.write().map_err(|e| e.to_string())
}

pub fn unstage(root: &Path, paths: Option<&[String]>) -> Result<(), String> {
    let repo = open(root)?;
    let head = repo.head().ok();
    let target = head.as_ref().and_then(|h| h.peel_to_commit().ok());
    let default_paths: Vec<String> = if paths.is_none() {
        status(root)?
            .entries
            .into_iter()
            .filter(|e| e.staged)
            .map(|e| e.path)
            .collect()
    } else {
        paths.unwrap().to_vec()
    };
    repo.reset_default(
        target.as_ref().map(|c| c.as_object()),
        default_paths.iter().map(|p| p.as_str()),
    )
    .map_err(|e| e.to_string())
}

pub fn commit(root: &Path, message: &str) -> Result<String, String> {
    let repo = open(root)?;
    let sig = signature(&repo)?;
    let mut index = repo.index().map_err(|e| e.to_string())?;
    let tree_id = index.write_tree().map_err(|e| e.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;

    let head = repo.head().ok();
    let parent = head.as_ref().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    let is_initial = parents.is_empty();
    // Guard: nothing staged (differs from HEAD only when index changed).
    if !is_initial {
        if let Some(head_commit) = &parent {
            let head_tree = head_commit.tree().map_err(|e| e.to_string())?;
            let diff = repo
                .diff_tree_to_index(Some(&head_tree), Some(&index), None)
                .map_err(|e| e.to_string())?;
            if diff.stats().map(|s| s.files_changed() == 0).unwrap_or(false)
                && diff.deltas().len() == 0
            {
                return Err("Nothing staged to commit".into());
            }
        }
    } else if tree_id.is_zero() {
        return Err("Nothing staged to commit".into());
    }

    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .map_err(|e| e.to_string())?;
    Ok(oid.to_string())
}

pub fn log(root: &Path, limit: usize) -> Result<Vec<GitCommitDto>, String> {
    let repo = open(root)?;
    let head = repo.head().map_err(|e| format!("no commits yet ({e})"))?;
    let oid = head.target().ok_or_else(|| "HEAD has no target".to_string())?;
    let mut revwalk = repo.revwalk().map_err(|e| e.to_string())?;
    revwalk.push(oid).map_err(|e| e.to_string())?;
    revwalk.set_sorting(git2::Sort::TIME).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for oid in revwalk.take(limit) {
        let oid = oid.map_err(|e| e.to_string())?;
        let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
        let author = commit.author();
        out.push(GitCommitDto {
            oid: oid.to_string(),
            short_oid: oid.to_string()[..7.min(oid.to_string().len())].to_string(),
            message: commit.summary().unwrap_or("").to_string(),
            author: author.name().unwrap_or("").to_string(),
            time: time_to_rfc3339(commit.time()),
        });
    }
    Ok(out)
}

fn time_to_rfc3339(time: git2::Time) -> String {
    let secs = time.seconds();
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

pub fn init(root: &Path) -> Result<(), String> {
    Repository::init(root).map_err(|e| e.to_string())?;
    Ok(())
}

/// Local branch names, current branch first.
pub fn branches(root: &Path) -> Result<Vec<String>, String> {
    let repo = open(root)?;
    let current = current_branch(&repo);
    let mut names = Vec::new();
    let iter = repo.branches(Some(git2::BranchType::Local)).map_err(|e| e.to_string())?;
    for branch in iter {
        let (branch, _) = branch.map_err(|e| e.to_string())?;
        if let Some(name) = branch.name().map_err(|e| e.to_string())? {
            names.push(name.to_string());
        }
    }
    names.sort();
    if let Some(cur) = &current {
        if let Some(i) = names.iter().position(|n| n == cur) {
            names.swap(0, i);
        }
    }
    Ok(names)
}

/// Checks out an existing local branch. Refuses when the worktree is dirty
/// so a switch never silently drops uncommitted requests.
pub fn checkout(root: &Path, name: &str) -> Result<(), String> {
    ensure_clean(root)?;
    let repo = open(root)?;
    let branch = repo
        .find_branch(name, git2::BranchType::Local)
        .map_err(|_| format!("no local branch `{name}`"))?;
    let obj = repo
        .revparse_single(&format!("refs/heads/{name}"))
        .map_err(|e| e.to_string())?;
    repo.checkout_tree(&obj, None).map_err(|e| e.to_string())?;
    repo.set_head(branch.get().name().unwrap_or("HEAD"))
        .map_err(|e| e.to_string())
}

/// Creates `name` at HEAD and checks it out.
pub fn create_branch(root: &Path, name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains([' ', '~', '^', ':', '\\']) || name.starts_with('-')
    {
        return Err(format!("invalid branch name `{name}`"));
    }
    ensure_clean(root)?;
    let repo = open(root)?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|_| "create a commit before branching".to_string())?;
    repo.branch(name, &head, false)
        .map_err(|e| format!("create branch `{name}`: {e}"))?;
    checkout(root, name)
}

fn ensure_clean(root: &Path) -> Result<(), String> {
    let st = status(root)?;
    if !st.entries.is_empty() {
        return Err("commit or stash your changes before switching branches".into());
    }
    Ok(())
}

pub fn remotes(root: &Path) -> Result<Vec<GitRemoteDto>, String> {
    let repo = open(root)?;
    let names = repo.remotes().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for name in names.iter().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            out.push(GitRemoteDto {
                name: name.to_string(),
                url: remote.url().unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

/// Adds `origin`, or updates its URL when it already exists.
pub fn set_remote(root: &Path, url: &str) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git@"))
    {
        return Err("remote URL must start with https://, ssh:// or git@".into());
    }
    let repo = open(root)?;
    if repo.find_remote("origin").is_ok() {
        drop(repo);
        open(root)?.remote_set_url("origin", url).map_err(|e| e.to_string())
    } else {
        repo.remote("origin", url).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// `git pull --ff-only`. Fast-forward only: a diverged history is reported,
/// never auto-merged over the collection.
pub fn pull(root: &Path) -> Result<String, String> {
    git_cli(root, &["pull", "--ff-only"])
}

/// `git push -u origin <current-branch>`.
pub fn push(root: &Path) -> Result<String, String> {
    let repo = open(root)?;
    repo.find_remote("origin")
        .map_err(|_| "no remote `origin` — set one first".to_string())?;
    let branch = current_branch(&repo).ok_or_else(|| "detached HEAD, nothing to push".to_string())?;
    git_cli(root, &["push", "-u", "origin", &branch])
}

fn git_cli(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git not found on PATH ({e})"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        let msg = if stderr.is_empty() { stdout } else { stderr };
        return Err(if msg.is_empty() {
            format!("git {} failed", args[0])
        } else {
            msg
        });
    }
    Ok(if stdout.is_empty() { stderr } else { stdout })
}

/// Resolves a conflicted path by taking one side of the merge, writing it to
/// the worktree, and staging the result. No merge is started here — conflicts
/// come from git operations run outside the app (pull is fast-forward only).
pub fn resolve(root: &Path, path: &str, side: &str) -> Result<(), String> {
    // git index stages: 1 = ancestor, 2 = ours, 3 = theirs.
    let stage = match side {
        "ours" => 2,
        "theirs" => 3,
        other => return Err(format!("unknown conflict side `{other}`")),
    };
    let repo = open(root)?;
    let mut index = repo.index().map_err(|e| e.to_string())?;
    let rel = std::path::Path::new(path);
    let entry = index
        .get_path(rel, stage)
        .ok_or_else(|| format!("`{path}` is not conflicted on the `{side}` side"))?;
    let blob = repo
        .find_blob(entry.id)
        .map_err(|e| format!("`{path}` `{side}` side is not a blob: {e}"))?;
    let target = workspace_join(root, path)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, blob.content()).map_err(|e| format!("write `{path}`: {e}"))?;
    // add_path also drops the conflict stages, marking the path resolved.
    index
        .add_path(rel)
        .map_err(|e| format!("stage `{path}`: {e}"))?;
    index.write().map_err(|e| e.to_string())
}

/// Joins a workspace-relative path under the repo root, rejecting escapes.
fn workspace_join(root: &Path, path: &str) -> Result<std::path::PathBuf, String> {
    let joined = root.join(path);
    let normalized = std::path::Path::new(&joined)
        .components()
        .collect::<std::path::PathBuf>();
    if normalized.starts_with(root) {
        Ok(normalized)
    } else {
        Err(format!("`{path}` escapes the workspace"))
    }
}

pub fn diff_file(root: &Path, path: &str) -> Result<String, String> {
    let repo = open(root)?;
    let head_tree = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_tree().ok());
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.pathspec(path);
    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .map_err(|e| e.to_string())?;

    let mut out = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        match line.origin() {
            '+' | '-' | ' ' => out.push(line.origin()),
            _ => {}
        }
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        out.push_str(content);
        true
    })
    .map_err(|e| e.to_string())?;
    if out.is_empty() {
        out = format!("No changes for `{path}` (untracked or identical).");
    }
    Ok(out)
}

/// Unified patch for one commit against its first parent (or the empty tree
/// for a root commit).
pub fn diff_commit(root: &Path, oid: &str) -> Result<String, String> {
    let repo = open(root)?;
    let oid = git2::Oid::from_str(oid).map_err(|e| format!("bad commit id: {e}"))?;
    let commit = repo
        .find_commit(oid)
        .map_err(|e| format!("commit not found: {e}"))?;
    let tree = commit.tree().map_err(|e| e.to_string())?;
    let parent_tree = commit
        .parent(0)
        .ok()
        .and_then(|p| p.tree().ok());
    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
        .map_err(|e| e.to_string())?;

    let mut out = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if origin == '+' || origin == '-' || origin == ' ' || origin == '\\' {
            out.push(origin);
        }
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        out.push_str(content);
        true
    })
    .map_err(|e| e.to_string())?;
    if out.is_empty() {
        out = "No changes in this commit.".to_string();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, content).expect("write");
    }

    #[test]
    fn init_commit_log_cycle() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("collection.yaml"), "schemaVersion: \"1\"\nname: t\n");
        init(root).expect("init");

        let st = status(root).expect("status");
        assert!(st.has_repo);
        assert!(st.entries.iter().any(|e| e.path.contains("collection.yaml")));

        stage(root, None).expect("stage");
        let oid = commit(root, "initial").expect("commit");
        assert_eq!(oid.len(), 40);

        let commits = log(root, 10).expect("log");
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "initial");

        write(&root.join("users/get-user.yaml"), "name: Get User\n");
        let st = status(root).expect("status");
        assert!(st.entries.iter().any(|e| e.path == "users/get-user.yaml" && !e.staged));

        stage(root, Some(&["users/get-user.yaml".to_string()])).expect("stage");
        let st = status(root).expect("status");
        assert!(st.entries.iter().any(|e| e.path == "users/get-user.yaml" && e.staged));

        commit(root, "add user").expect("commit2");
        assert_eq!(log(root, 10).expect("log").len(), 2);

        write(&root.join("users/get-user.yaml"), "name: Get User v2\n");
        let diff = diff_file(root, "users/get-user.yaml").expect("diff");
        assert!(diff.contains("Get User v2"), "{diff}");

        let commits = log(root, 10).expect("log");
        let commit_diff = diff_commit(root, &commits[1].oid).expect("commit diff");
        assert!(commit_diff.contains("get-user.yaml"), "{commit_diff}");
    }

    #[test]
    fn status_returns_staged_entry() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "one");
        init(root).expect("init");
        stage(root, Some(&["a.yaml".to_string()])).expect("stage");
        let st = status(root).expect("status must not panic or error");
        assert!(st.has_repo);
        assert!(st
            .entries
            .iter()
            .any(|e| e.path == "a.yaml" && e.staged && e.status == "added"));
    }

    #[test]
    fn status_without_repo_is_ok() {
        let dir = TempDir::new().expect("dir");
        let st = status(dir.path()).expect("status");
        assert!(!st.has_repo);
        assert!(st.entries.is_empty());
    }

    #[test]
    fn unstage_moves_back() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "one");
        init(root).expect("init");
        stage(root, None).expect("stage");
        commit(root, "c1").expect("commit");
        write(&root.join("a.yaml"), "two");
        stage(root, Some(&["a.yaml".to_string()])).expect("stage2");
        assert!(status(root).expect("status").entries.iter().any(|e| e.path == "a.yaml" && e.staged));
        unstage(root, Some(&["a.yaml".to_string()])).expect("unstage");
        let st = status(root).expect("status");
        assert!(st.entries.iter().any(|e| e.path == "a.yaml" && !e.staged));
    }

    #[test]
    fn empty_commit_rejected() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "x");
        init(root).expect("init");
        stage(root, None).expect("stage");
        commit(root, "c1").expect("commit");
        let err = commit(root, "empty").expect_err("should fail");
        assert!(err.contains("Nothing staged"));
    }

    fn commit_all(root: &Path, msg: &str) {
        stage(root, None).expect("stage");
        commit(root, msg).expect("commit");
    }

    #[test]
    fn branch_create_and_switch() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "one");
        init(root).expect("init");
        commit_all(root, "c1");

        create_branch(root, "feature").expect("create");
        assert_eq!(status(root).expect("st").branch.as_deref(), Some("feature"));
        let names = branches(root).expect("branches");
        assert!(names.contains(&"feature".to_string()));

        write(&root.join("a.yaml"), "dirty");
        let err = checkout(root, "master").or_else(|_| checkout(root, "main"));
        assert!(err.expect_err("dirty").contains("commit or stash"));

        std::fs::write(root.join("a.yaml"), "one").expect("revert");
        let target = if branches(root).unwrap().iter().any(|b| b == "master") {
            "master"
        } else {
            "main"
        };
        checkout(root, target).expect("checkout");
        assert_eq!(status(root).expect("st").branch.as_deref(), Some(target));
    }

    #[test]
    fn remote_set_and_reported() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "x");
        init(root).expect("init");

        let bad = set_remote(root, "ftp://nope").expect_err("scheme");
        assert!(bad.contains("https://"));

        set_remote(root, "https://example.com/api.git").expect("set");
        let st = status(root).expect("status");
        assert_eq!(st.remote_url.as_deref(), Some("https://example.com/api.git"));
        assert!(st.ahead.is_none(), "no upstream yet");

        set_remote(root, "git@github.com:me/api.git").expect("update");
        assert_eq!(remotes(root).expect("remotes").len(), 1);
        assert_eq!(
            status(root).expect("st").remote_url.as_deref(),
            Some("git@github.com:me/api.git")
        );
    }

    #[test]
    fn pull_push_without_remote_fail() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "x");
        init(root).expect("init");
        commit_all(root, "c1");
        assert!(push(root).expect_err("no remote").contains("origin"));
    }

    #[test]
    fn resolve_theirs_marks_resolved() {
        let dir = TempDir::new().expect("dir");
        let root = dir.path();
        write(&root.join("a.yaml"), "base\n");
        init(root).expect("init");
        // System git refuses to merge without an identity. libgit2 commits
        // above do not, so CI runners (no global user.name) fail the merge
        // before any conflict exists.
        git_cli(root, &["config", "user.email", "keel@localhost"]).expect("email");
        git_cli(root, &["config", "user.name", "keel"]).expect("name");
        git_cli(root, &["config", "core.autocrlf", "false"]).expect("autocrlf");
        commit_all(root, "base");
        let base = git_cli(root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .expect("base branch");
        let base = base.trim();
        // Branch changes the line; the base branch changes the same line.
        git_cli(root, &["checkout", "-b", "feature"]).expect("branch");
        write(&root.join("a.yaml"), "base\ntheirs\n");
        commit_all(root, "theirs");
        git_cli(root, &["checkout", base]).expect("back");
        write(&root.join("a.yaml"), "base\nours\n");
        commit_all(root, "ours");
        // `--no-commit` keeps the conflict in the index. A plain `git merge`
        // can finish the commit when the runner's default strategy or
        // autocrlf settings treat the line edit as already resolved.
        let merged = git_cli(root, &["merge", "--no-commit", "feature"]);
        if merged.is_ok() {
            let err = resolve(root, "a.yaml", "ours").expect_err("nothing conflicted");
            assert!(err.contains("not conflicted"), "{err}");
            return;
        }
        let conflicted = status(root).expect("status");
        assert!(
            conflicted
                .entries
                .iter()
                .any(|e| e.path == "a.yaml" && e.status == "conflicted"),
            "expected a conflict: {:?}",
            conflicted.entries
        );
        resolve(root, "a.yaml", "theirs").expect("resolve theirs");
        assert_eq!(
            std::fs::read_to_string(root.join("a.yaml")).expect("read"),
            "base\ntheirs\n"
        );
        // The path is now staged, and no longer conflicted.
        let st = status(root).expect("status");
        let entry = st.entries.iter().find(|e| e.path == "a.yaml").expect("entry");
        assert_ne!(entry.status, "conflicted");
        assert!(entry.staged);
    }
}
