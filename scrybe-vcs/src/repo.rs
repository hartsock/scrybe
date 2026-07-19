// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Core [`ScrybeRepo`] type — thin wrapper around `git2::Repository`.

use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use git2::{Repository, Signature, StatusOptions};
use scrybe_core::error::{Result, ScrybeError};

use crate::{
    auth,
    remote::{RemoteEntry, RemoteRole},
    types::{CommitSummary, FileStatus, GitAuthor, StatusEntry},
};

/// A git repository managed by Scrybe.
pub struct ScrybeRepo {
    inner: Repository,
}

impl ScrybeRepo {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Opens an existing repository at *path* (walks up to discover `.git`).
    pub fn open(path: &Path) -> Result<Self> {
        let inner = Repository::discover(path).map_err(|e| ScrybeError::msg(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Initialises a new bare-free repository at *path*.
    pub fn init(path: &Path) -> Result<Self> {
        let inner = Repository::init(path).map_err(|e| ScrybeError::msg(e.to_string()))?;
        Ok(Self { inner })
    }

    // ── Read operations ───────────────────────────────────────────────────────

    /// Returns the HEAD commit SHA, or `None` for an unborn HEAD.
    pub fn head_sha(&self) -> Result<Option<String>> {
        match self.inner.head() {
            Ok(head) => {
                let oid = head
                    .peel_to_commit()
                    .map_err(|e| ScrybeError::msg(e.to_string()))?
                    .id();
                Ok(Some(oid.to_string()))
            }
            Err(_) => Ok(None),
        }
    }

    /// Returns the current branch name, or `None` when HEAD is detached.
    pub fn current_branch(&self) -> Result<Option<String>> {
        let head = match self.inner.head() {
            Ok(h) => h,
            Err(_) => return Ok(None),
        };
        if head.is_branch() {
            Ok(head.shorthand().map(|s| s.to_owned()))
        } else {
            Ok(None)
        }
    }

    /// Returns all changed / untracked files in the working tree.
    pub fn status(&self) -> Result<Vec<StatusEntry>> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);

        let statuses = self
            .inner
            .statuses(Some(&mut opts))
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        let mut entries = Vec::new();
        for entry in statuses.iter() {
            let path = match entry.path() {
                Some(p) => std::path::PathBuf::from(p),
                None => continue,
            };
            let flags = entry.status();

            let file_status = if flags
                .intersects(git2::Status::INDEX_RENAMED | git2::Status::WT_RENAMED)
            {
                let from = entry
                    .head_to_index()
                    .or_else(|| entry.index_to_workdir())
                    .and_then(|d| d.old_file().path())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default();
                FileStatus::Renamed { from }
            } else if flags.intersects(git2::Status::INDEX_MODIFIED | git2::Status::WT_MODIFIED) {
                FileStatus::Modified
            } else if flags.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW) {
                if flags.contains(git2::Status::WT_NEW) {
                    FileStatus::Untracked
                } else {
                    FileStatus::Added
                }
            } else if flags.intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED) {
                FileStatus::Deleted
            } else if flags.contains(git2::Status::CONFLICTED) {
                FileStatus::Conflicted
            } else {
                continue; // IGNORED or CURRENT — skip
            };

            entries.push(StatusEntry {
                path,
                status: file_status,
            });
        }

        Ok(entries)
    }

    /// Returns the last `max` commits starting from HEAD.
    pub fn log(&self, max: usize) -> Result<Vec<CommitSummary>> {
        let mut revwalk = self
            .inner
            .revwalk()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        revwalk
            .push_head()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        let mut summaries = Vec::with_capacity(max);
        for oid in revwalk.take(max) {
            let oid = oid.map_err(|e| ScrybeError::msg(e.to_string()))?;
            let commit = self
                .inner
                .find_commit(oid)
                .map_err(|e| ScrybeError::msg(e.to_string()))?;

            let timestamp: DateTime<Utc> = Utc
                .timestamp_opt(commit.time().seconds(), 0)
                .single()
                .unwrap_or_else(Utc::now);

            summaries.push(CommitSummary {
                sha: oid.to_string(),
                message: commit.message().unwrap_or("").to_owned(),
                author_name: commit.author().name().unwrap_or("").to_owned(),
                author_email: commit.author().email().unwrap_or("").to_owned(),
                timestamp,
            });
        }

        Ok(summaries)
    }

    /// Lists all configured remotes with URLs and inferred roles.
    pub fn remotes(&self) -> Result<Vec<RemoteEntry>> {
        let remote_names = self
            .inner
            .remotes()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        let mut entries = Vec::new();
        for name in remote_names.iter().flatten() {
            let remote = self
                .inner
                .find_remote(name)
                .map_err(|e| ScrybeError::msg(e.to_string()))?;
            let url = remote.url().unwrap_or("").to_owned();
            let role = RemoteRole::from_url(&url);
            entries.push(RemoteEntry {
                name: name.to_owned(),
                url,
                role,
            });
        }

        Ok(entries)
    }

    // ── Write operations ──────────────────────────────────────────────────────

    /// Stages a single file (equivalent to `git add <path>`).
    pub fn stage(&self, path: &Path) -> Result<()> {
        let mut index = self
            .inner
            .index()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        index
            .add_path(path)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        index.write().map_err(|e| ScrybeError::msg(e.to_string()))?;
        Ok(())
    }

    /// Stages all changes (equivalent to `git add -A`).
    pub fn stage_all(&self) -> Result<()> {
        let mut index = self
            .inner
            .index()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        index.write().map_err(|e| ScrybeError::msg(e.to_string()))?;
        Ok(())
    }

    /// Creates a commit from the current index. Returns the new commit SHA.
    pub fn commit(&self, message: &str, author: &GitAuthor) -> Result<String> {
        let sig = Signature::now(&author.name, &author.email)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        let mut index = self
            .inner
            .index()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        let tree_oid = index
            .write_tree()
            .map_err(|e| ScrybeError::msg(e.to_string()))?;
        let tree = self
            .inner
            .find_tree(tree_oid)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        // Collect parent commits (empty for the very first commit).
        let parent_commit;
        let parents: Vec<&git2::Commit<'_>> = match self.inner.head() {
            Ok(head) => {
                parent_commit = head
                    .peel_to_commit()
                    .map_err(|e| ScrybeError::msg(e.to_string()))?;
                vec![&parent_commit]
            }
            Err(_) => vec![],
        };

        let oid = self
            .inner
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        Ok(oid.to_string())
    }

    /// Fetches from a remote (no checkout or merge).
    pub fn fetch(&self, remote_name: &str) -> Result<()> {
        let mut remote = self
            .inner
            .find_remote(remote_name)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        let callbacks = auth::make_callbacks();
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        remote
            .fetch(&[] as &[&str], Some(&mut fetch_opts), None)
            .map_err(|e| ScrybeError::msg(e.to_string()))?;

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::remote::RemoteRole;
    use tempfile::TempDir;

    fn setup_repo() -> (TempDir, ScrybeRepo) {
        let dir = TempDir::new().unwrap();
        let repo = ScrybeRepo::init(dir.path()).unwrap();
        (dir, repo)
    }

    /// Helper: write a file and return its repo-relative path.
    fn write_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let abs = dir.path().join(name);
        fs::write(&abs, content).unwrap();
        std::path::PathBuf::from(name)
    }

    fn test_author() -> GitAuthor {
        GitAuthor {
            name: "Test User".to_owned(),
            email: "test@example.com".to_owned(),
        }
    }

    // ── init / open ───────────────────────────────────────────────────────────

    #[test]
    fn test_init_and_open() {
        let (dir, repo) = setup_repo();
        // HEAD is unborn on a fresh repo.
        assert!(repo.head_sha().unwrap().is_none());

        // open() should succeed given the same path.
        let reopened = ScrybeRepo::open(dir.path()).unwrap();
        assert!(reopened.head_sha().unwrap().is_none());
    }

    // ── status ────────────────────────────────────────────────────────────────

    #[test]
    fn test_status_untracked() {
        let (dir, repo) = setup_repo();
        write_file(&dir, "hello.txt", "hello");

        let entries = repo.status().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, std::path::PathBuf::from("hello.txt"));
        assert_eq!(entries[0].status, FileStatus::Untracked);
    }

    // ── stage + commit ────────────────────────────────────────────────────────

    #[test]
    fn test_stage_and_commit() {
        let (dir, repo) = setup_repo();
        write_file(&dir, "file.txt", "initial content");

        repo.stage_all().unwrap();
        let sha1 = repo.commit("initial commit", &test_author()).unwrap();

        // HEAD should now point at the new commit.
        assert_eq!(repo.head_sha().unwrap().unwrap(), sha1);

        // Modify the file and commit again.
        write_file(&dir, "file.txt", "updated content");
        repo.stage_all().unwrap();
        let sha2 = repo.commit("second commit", &test_author()).unwrap();

        assert_ne!(sha1, sha2);
        assert_eq!(repo.head_sha().unwrap().unwrap(), sha2);
    }

    #[test]
    fn test_stage_single_file() {
        let (dir, repo) = setup_repo();
        write_file(&dir, "a.txt", "aaa");
        write_file(&dir, "b.txt", "bbb");

        // Stage only a.txt.
        let rel = std::path::Path::new("a.txt");
        repo.stage(rel).unwrap();
        let sha = repo.commit("only a", &test_author()).unwrap();
        assert!(!sha.is_empty());

        // b.txt should still be untracked.
        let entries = repo.status().unwrap();
        assert!(entries.iter().any(|e| e.path == std::path::Path::new("b.txt")));
    }

    // ── log ───────────────────────────────────────────────────────────────────

    #[test]
    fn test_log() {
        let (dir, repo) = setup_repo();
        write_file(&dir, "f.txt", "v1");
        repo.stage_all().unwrap();
        repo.commit("commit one", &test_author()).unwrap();

        write_file(&dir, "f.txt", "v2");
        repo.stage_all().unwrap();
        repo.commit("commit two", &test_author()).unwrap();

        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 2);
        // Most recent first.
        assert!(log[0].message.contains("commit two"));
        assert!(log[1].message.contains("commit one"));
    }

    #[test]
    fn test_log_max_limit() {
        let (dir, repo) = setup_repo();
        for i in 0..5u8 {
            write_file(&dir, "x.txt", &i.to_string());
            repo.stage_all().unwrap();
            repo.commit(&format!("commit {i}"), &test_author()).unwrap();
        }
        let log = repo.log(3).unwrap();
        assert_eq!(log.len(), 3);
    }

    // ── current_branch ────────────────────────────────────────────────────────

    #[test]
    fn test_current_branch() {
        let (dir, repo) = setup_repo();

        // On a fresh repo with no commits there is no branch yet.
        assert!(repo.current_branch().unwrap().is_none());

        write_file(&dir, "x.txt", "x");
        repo.stage_all().unwrap();
        repo.commit("first", &test_author()).unwrap();

        let branch = repo.current_branch().unwrap();
        // git2 defaults the initial branch to "master" unless overridden.
        assert!(branch.is_some());
    }

    // ── remote role inference ─────────────────────────────────────────────────

    #[test]
    fn test_remote_role_inference() {
        assert_eq!(
            RemoteRole::from_url("ssh://git@gitea.example.lan:30222/user/repo.git"),
            RemoteRole::Origin
        );
        assert_eq!(
            RemoteRole::from_url("http://gitea.local/user/repo.git"),
            RemoteRole::Origin
        );
        assert_eq!(
            RemoteRole::from_url("git@github.com:user/repo.git"),
            RemoteRole::Mirror
        );
        assert_eq!(
            RemoteRole::from_url("https://github.com/user/repo.git"),
            RemoteRole::Mirror
        );
        assert_eq!(
            RemoteRole::from_url("https://gitlab.com/user/repo.git"),
            RemoteRole::Other
        );
    }

    // ── remotes list ─────────────────────────────────────────────────────────

    #[test]
    fn test_remotes_empty() {
        let (_dir, repo) = setup_repo();
        let remotes = repo.remotes().unwrap();
        assert!(remotes.is_empty());
    }
}
