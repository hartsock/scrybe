// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Compile-time identity for the exact Scrybe build.
//!
//! Cargo supplies the package version; `build.rs` adds the checked-out Git
//! commit and marks builds made from a modified worktree as `dirty`, so
//! `scrybe version` / the app's About panel can tell two builds sharing the
//! same `CARGO_PKG_VERSION` apart.

/// SemVer package version from the workspace manifest.
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Twelve-character Git commit captured when `scrybe-core` was built.
pub const GIT_COMMIT: &str = env!("SCRYBE_BUILD_GIT_COMMIT");

/// Git commit plus a `-dirty` suffix when tracked or untracked changes existed.
pub const SOURCE_ID: &str = env!("SCRYBE_BUILD_SOURCE_ID");

/// User-visible build identity, for example `0.6.3 (a1b2c3d4e5f6-dirty)`.
pub const VERSION_WITH_COMMIT: &str = env!("SCRYBE_BUILD_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_version_contains_package_and_source_identity() {
        assert!(VERSION_WITH_COMMIT.starts_with(PACKAGE_VERSION));
        assert!(VERSION_WITH_COMMIT.contains(GIT_COMMIT));
        assert!(SOURCE_ID.starts_with(GIT_COMMIT));
    }
}
