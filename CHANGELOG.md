<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Changelog

All notable changes to Scrybe are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/); versions are the workspace
lock-step version (`[workspace.package] version`).

## [0.4.0] — 2026-07-14 — "Keystone"

The MCP server now genuinely **drives the live editor**, and the release
machinery moves to a single lock-step version. (v0.3.x work — path bar, theme
sync, Vim toggle, view cycle, Word export, MCP UI-parity — merged to `main`
after v0.2.0 but was never tagged; it ships here, folded into 0.4.0.)

### MCP drives the live app
- **Fixed the broken MCP** (#108): `open` now dispatches through `scrybe-rpc` to
  the running app (with a headless fallback) instead of spawning a phantom
  second process, so a tab actually appears. One shared socket dialer
  (`scrybe_rpc::client`) for the CLI and the MCP server.
- **read / find / section / edit reflect the LIVE buffer** (#122): routed through
  `scrybe-rpc`. `edit` is now a line-range op `{id, start_line, end_line,
  content}` (matching the app and `scrybe` CLI); `section` is heading-based
  `{id, heading}`. Headless fallback preserved.
- **Edits persist** (#140): the fs-watcher no longer reverts an in-flight MCP
  edit (no-op reload when disk already matches the buffer); the preview updates
  after an edit.
- **`open` waits for the tab** (#141): moved to request-with-reply, so an
  immediate follow-up `read`/`edit` no longer races into "not open".
- Correct JSON-RPC: unknown methods return a top-level `error`; `tools/call`
  sets `isError`.

### Editor
- **Word-wrap toggle** in the toolbar (#136), with MCP-parity plumbing
  (`poll_set_wrap`, `wrap` in the published state).

### Build & tooling
- **Vite 6 → 8** (Rolldown); build uses the native Oxc minifier (#131).
- **Headless UI verification**: a reusable `headless-ui-verify` skill +
  `scrybe-app.sh` harness, and `docs/BUILD_AND_TEST.md` documenting the whole
  build/test methodology (#139).
- **Release hygiene** (#128): `[workspace.package] version` — every crate now
  inherits one version and bumps in lock-step.

### Planning
- `ROADMAP.md` (v0.4→v1.0) + `docs/TRIAGE.md`; the `scrybe-mermaid-render` epic
  (#37) is re-scoped to **adopt** a pure-Rust renderer, gated on a fidelity
  spike (#132). Mermaid source-in-image provenance (PNG iTXt + SVG metadata) is
  the headline result and is kept in-house (#133).
- Strategic explores resolved **build-ours** (#114 Ferrite, #115 markdown-tui-explorer).

[0.4.0]: https://github.com/hartsock/scrybe/releases/tag/v0.4.0
