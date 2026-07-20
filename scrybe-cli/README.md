<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-cli

Headless CLI binary for Scrybe: render, lint, mermaid encode/decode, and GUI
launcher. Distributed as a `maturin` binary wheel (`scrybe-cli`). Python on
the outside, Rust on the inside.

## What it does

Provides a `scrybe` command-line tool with four primary subcommands plus a
bare invocation shortcut. The binary is self-contained — no Python runtime
required at execution time when installed from the wheel.

## Role in the architecture

`scrybe-cli` is the human-facing entry point for headless use and scripting.
It delegates to `scrybe-core`, `scrybe-render`, and `scrybe-mermaid`. It is
also the launcher for the Tauri GUI app: `scrybe file.md` locates the
`Scrybe.app` bundle (macOS) or `scrybe-app` binary and opens the file in it.

## Subcommands

| Command | Description |
|---------|-------------|
| `scrybe render [FILE]` | Render Markdown to HTML (stdin → stdout by default); `--theme`, `--full-html`, `--output` |
| `scrybe lint FILE` | Word count, headings, code blocks, broken links; `--json` for machine output; exits 1 if broken links found |
| `scrybe mermaid embed PNG SOURCE` | Embed Mermaid source into PNG iTXt chunk |
| `scrybe mermaid extract PNG` | Print embedded Mermaid source, verifying its SHA-256 by default; exit 2 if tampered, `--unverified` to skip the check (forensics) |
| `scrybe mermaid verify PNG` | Verify SHA-256 integrity of embedded source; exits 1 if tampered or digest missing |
| `scrybe open [PATH]` | Launch the Scrybe GUI, optionally at a file or directory |
| `scrybe version` | Print version and active feature flags |
| `scrybe [PATH]` | Bare invocation with a path injects `open` automatically |

On macOS, `scrybe open` prefers the `.app` bundle via `open -n -a` to satisfy
WebKit's bundle entitlement requirements.

## Key library helpers (for integration)

| Symbol | Description |
|--------|-------------|
| `lint_document(doc) -> LintReport` | Programmatic lint used by both CLI and MCP `lint` tool |
| `wrap_full_html(output, title)` | Wraps a `RenderOutput` in a complete `<!DOCTYPE html>` with CDN tags |
| `version_string()` / `active_features()` | Used by `scrybe version` |

## Build and install

```sh
# Rust build (produces scrybe binary)
cargo build -p scrybe-cli --release

# Python wheel (maturin)
maturin build -m scrybe-cli/Cargo.toml --release
pip install target/wheels/scrybe_cli-*.whl

# Run tests
cargo test -p scrybe-cli
```
