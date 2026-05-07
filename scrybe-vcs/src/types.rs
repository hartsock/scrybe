// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Supporting types for VCS operations.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A file and its current status in the working tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    pub path: PathBuf,
    pub status: FileStatus,
}

/// The status of a file relative to the index / HEAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed { from: PathBuf },
    Untracked,
    Conflicted,
}

/// Author / committer identity for creating commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
}

/// A condensed view of a single git commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSummary {
    pub sha: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
