<!--
SPDX-License-Identifier: AGPL-3.0-or-later
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-vcs

Gitea-first, multi-remote git2 wrapper for the Scrybe editor. Python on the
outside, Rust on the inside.

## What it does

Provides a thin, safe abstraction over `libgit2` (via the `git2` Rust crate).
The primary remote strategy is Gitea as origin (source of truth) with GitHub as
a read-only mirror. SSH auth is resolved via `ssh-agent` or the
`SCRYBE_GITEA_TOKEN` environment variable.

## Role in the architecture

`scrybe-vcs` is consumed by the Tauri backend (`scrybe-app`) to power the
in-app VCS panel: working-tree status, staging, commit, fetch, log, and remote
listing. The backend exposes these as Tauri IPC commands (`vcs_open`,
`vcs_status`, `vcs_stage_all`, `vcs_commit`, `vcs_fetch`, `vcs_log`,
`vcs_remotes`). No other crate depends on `scrybe-vcs`.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `ScrybeRepo` | Core type: wraps `git2::Repository`; all VCS operations go through this |
| `ScrybeRepo::open(path)` | Discover and open a repository (walks up to find `.git`) |
| `ScrybeRepo::init(path)` | Initialise a new repository |
| `ScrybeRepo::head_sha()` | Returns `Option<String>` for the HEAD commit SHA |
| `ScrybeRepo::current_branch()` | Returns `Option<String>` for the branch name |
| `RemoteEntry` / `RemoteRole` | Named remote + role enum (`Origin`, `Mirror`, `Other`) |
| `CommitSummary` / `FileStatus` / `GitAuthor` / `StatusEntry` | Value types for log and status results |

## Build and test

```sh
cargo build -p scrybe-vcs
cargo test -p scrybe-vcs
```

`git2` is compiled with `vendored-openssl` so no system OpenSSL is required.
Tests that touch the filesystem create temporary repositories via `tempfile`.
