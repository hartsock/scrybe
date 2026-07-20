<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-vcs

Multi-remote git2 wrapper for the Scrybe editor. Python on the outside, Rust
on the inside.

## What it does

Provides a thin, safe abstraction over `libgit2` (via the `git2` Rust crate).
Remote roles (origin / mirror / backup) are assigned by an explicit,
configurable `RemoteRolePolicy` — conventional remote names by default,
user-supplied name/URL-glob rules when configured. No hosts, brands, or ports
are baked in. SSH auth is resolved via `ssh-agent` or the
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
| `RemoteEntry` / `RemoteRole` | Named remote + role enum (`Origin`, `Mirror`, `Backup`, `Other(label)`) |
| `RemoteRolePolicy` / `RemoteRule` / `RemoteMatcher` | Ordered name/URL-glob rules mapping remotes to roles; `default()` = conventional names only |
| `CommitSummary` / `FileStatus` / `GitAuthor` / `StatusEntry` | Value types for log and status results |

## Build and test

```sh
cargo build -p scrybe-vcs
cargo test -p scrybe-vcs
```

`git2` is compiled with `vendored-openssl` so no system OpenSSL is required.
Tests that touch the filesystem create temporary repositories via `tempfile`.
