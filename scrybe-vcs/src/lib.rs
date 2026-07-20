// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe VCS — git2 wrapper with a configurable multi-remote role policy.
//!
//! Remote roles (origin / mirror / backup) are assigned by an explicit
//! [`RemoteRolePolicy`] — conventional remote names by default, user-supplied
//! name/URL-glob rules when configured. No hosts, brands, or ports are baked
//! in. Auth: ssh-agent, or the `SCRYBE_GITEA_TOKEN` env var for HTTPS tokens.

pub mod auth;
pub mod remote;
pub mod repo;
pub mod types;

pub use remote::{RemoteEntry, RemoteMatcher, RemoteRole, RemoteRolePolicy, RemoteRule};
pub use repo::ScrybeRepo;
pub use types::{CommitSummary, FileStatus, GitAuthor, StatusEntry};
