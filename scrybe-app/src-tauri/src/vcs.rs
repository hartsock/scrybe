// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! VCS IPC (P4.8): cached repository state and the `vcs_*` commands.

// ─── P4.8 — VCS state ────────────────────────────────────────────────────────

use scrybe_vcs::{GitAuthor, ScrybeRepo};
use std::sync::Mutex as VcsMutex;

struct VcsState {
    repo: Option<ScrybeRepo>,
}

static VCS: VcsMutex<VcsState> = VcsMutex::new(VcsState { repo: None });

// ─── P4.8 — VCS IPC commands ─────────────────────────────────────────────────

/// Open a git repository at `path` and store it for subsequent VCS commands.
///
/// Returns a JSON object with `path`, `head` (SHA or null), and `branch` (name
/// or null when HEAD is detached).  Subsequent calls replace the cached repo.
#[tauri::command]
pub fn vcs_open(path: String) -> Result<serde_json::Value, String> {
    let repo = ScrybeRepo::open(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    let head = repo.head_sha().unwrap_or(None);
    let branch = repo.current_branch().unwrap_or(None);
    VCS.lock().unwrap().repo = Some(repo);
    Ok(serde_json::json!({ "path": path, "head": head, "branch": branch }))
}

/// Returns the git status of the open repository as a JSON array.
///
/// Each element has `path` (relative) and `status` (debug string of
/// [`scrybe_vcs::FileStatus`]).
#[tauri::command]
pub fn vcs_status() -> Result<Vec<serde_json::Value>, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let entries = repo.status().map_err(|e| e.to_string())?;
    Ok(entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "path": e.path.to_string_lossy(),
                "status": format!("{:?}", e.status),
            })
        })
        .collect())
}

/// Stage all working-tree changes (equivalent to `git add -A`).
#[tauri::command]
pub fn vcs_stage_all() -> Result<(), String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    repo.stage_all().map_err(|e| e.to_string())
}

/// Create a commit from the current index. Returns the new commit SHA.
#[tauri::command]
pub fn vcs_commit(
    message: String,
    author_name: String,
    author_email: String,
) -> Result<String, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let author = GitAuthor {
        name: author_name,
        email: author_email,
    };
    repo.commit(&message, &author).map_err(|e| e.to_string())
}

/// Fetch from the named remote (no checkout or merge).
#[tauri::command]
pub fn vcs_fetch(remote: String) -> Result<(), String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    repo.fetch(&remote).map_err(|e| e.to_string())
}

/// Return the most recent `max` commits starting from HEAD.
///
/// Each element has `sha` (8 chars), `message` (first line), and `author`.
#[tauri::command]
pub fn vcs_log(max: usize) -> Result<Vec<serde_json::Value>, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let commits = repo.log(max).map_err(|e| e.to_string())?;
    Ok(commits
        .iter()
        .map(|c| {
            serde_json::json!({
                "sha": &c.sha[..8.min(c.sha.len())],
                "message": c.message.lines().next().unwrap_or(""),
                "author": &c.author_name,
            })
        })
        .collect())
}

/// Return the configured remotes with their names, URLs, and roles assigned
/// by the default [`scrybe_vcs::RemoteRolePolicy`] (conventional remote
/// names: `origin` / `mirror` / `backup`; anything else is `Other`).
#[tauri::command]
pub fn vcs_remotes() -> Result<Vec<serde_json::Value>, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let remotes = repo.remotes().map_err(|e| e.to_string())?;
    Ok(remotes
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name,
                "url": r.url,
                "role": format!("{:?}", r.role),
            })
        })
        .collect())
}
