// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe VCS — Gitea-first git2 wrapper.
//!
//! Multi-remote strategy: Gitea as origin (source of truth),
//! GitHub as mirror. SSH auth via ssh-agent or `SCRYBE_GITEA_TOKEN`.

pub mod auth;
pub mod remote;
pub mod repo;
pub mod types;

pub use remote::{RemoteEntry, RemoteRole};
pub use repo::ScrybeRepo;
pub use types::{CommitSummary, FileStatus, GitAuthor, StatusEntry};
