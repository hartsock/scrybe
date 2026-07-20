<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Changelog

All notable changes to Scrybe are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/); versions are the workspace
lock-step version (`[workspace.package] version`).

## [Unreleased]

### Fixed
- **`scrybe_mermaid::extract` verifies the embedded digest by default** —
  shipped docs (the PyPI-facing `extract` docstring) promised a `ValueError`
  on sha256 mismatch, but extraction never checked the digest. `extract` now
  recomputes the SHA-256 of the extracted source and distinguishes three
  outcomes: verified (`VerificationStatus::Verified`), tampered
  (`MermaidError::VerificationFailed { expected, actual, .. }`), and
  no-digest-present (`VerificationStatus::NoDigest` — never a false
  "verified"). `extract_unverified` returns the raw stored fields for
  forensics. Python gains `payload.verified` and `extract_unverified`;
  `scrybe mermaid extract` exits 2 on a tampered PNG (1 = no payload,
  0 = success) and takes `--unverified`; the MCP `extract` tool reports
  `verification: "verified" | "no-digest"` and errors on mismatch.

## [0.5.0] — 2026-07-19 — "Parity"

One tool registry now serves every surface, and the Mermaid pipeline is pure
Rust end-to-end. Buffers stay dirty until an explicit `save` — the agent and
the human share the same persist semantics.

### One shared tool registry (#121, #122)
- **`scrybe-tools`**: a single `ToolSpec` registry consumed by BOTH the MCP
  server and the CLI (#169). Business failures are in-band `tool_error` data;
  engine faults alone are MCP `isError`.
- **The MCP server serves the shared registry** (#177), retiring the legacy
  per-tool handlers from the dispatch path (dead shadow `Workspace` mapped in
  #181, marked in-code in #182).
- **Editor tools over the socket** (#176, #179): `open`/`read`/`find`/
  `section`/`edit` dispatch to the *live app* via `scrybe-rpc` on the
  path-addressed contract; headless returns a clean `no_live_app` tool_error.
- **`list_tabs`** — the live tab set over the socket, CLI + MCP (#46, #178).
- **`reload`** — re-read a tab from disk as a first-class socket method +
  tool, replacing the old `/tmp` poke (#180); refusing to clobber unsaved
  edits without `force` (`ERR_DIRTY_RELOAD_REFUSED`).
- **`save`** — explicit persist as a first-class tool, the agent-side twin of
  Cmd+S / 💾 (#183): reply-correlated `{path, bytes, was_dirty}` (no more
  optimistic `applied: true`), plus two dirty-truthfulness fixes (socket
  edits of the active tab stay dirty; mid-write keystrokes are never marked
  clean). Autosave writes only the `.scrybe-buffer` crash-recovery sidecar —
  the real file changes only on explicit save.

### Pure-Rust Mermaid (#37, #119, #132)
- **Adopted `mermaid-rs-renderer`** behind `scrybe-mermaid-render` (#171):
  SVG with embedded `<metadata>` provenance; the #132 fidelity gate passed on
  a real render drive.
- **`render_png`** — SVG → PNG via the crate's resvg path (#172), and a
  per-artifact **uuid in the iTXt payload** (#173) so every PNG carries
  provenance (`source`, `sha256`, `uuid`).
- **`mermaid_to_png` tool** + `scrybe mermaid png` CLI + skill (#174) — the
  full headless diagram pipeline, no browser, no JS.

### Fixed
- Flaky parallel-test failures in `scrybe-mcp-client` (shared fixed temp
  paths → unique per-spawn names); Windows CI job added and green.
- `scrybe-vcs` status test made toolchain-portable (`Path::new` comparison —
  compiles on the pinned 1.88 and current stable).

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
