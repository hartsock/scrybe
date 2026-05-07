// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Remote configuration for Gitea-first, GitHub-mirror topology.

use serde::{Deserialize, Serialize};

/// The role a remote plays in the Gitea-first topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteRole {
    /// Gitea — source of truth.
    Origin,
    /// GitHub — read-only mirror.
    Mirror,
    /// Any other remote.
    Other,
}

impl RemoteRole {
    /// Infers the role from the remote URL.
    ///
    /// Rules (checked in order):
    /// - URL contains "gitea" or port 30222 → `Origin`
    /// - URL contains "github.com" → `Mirror`
    /// - Otherwise → `Other`
    pub fn from_url(url: &str) -> Self {
        if url.contains("gitea") || url.contains("30222") {
            Self::Origin
        } else if url.contains("github.com") {
            Self::Mirror
        } else {
            Self::Other
        }
    }
}

/// Named remote entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub name: String,
    pub url: String,
    /// Role inferred from the URL.
    pub role: RemoteRole,
}
