<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-cli

Headless CLI binary for Scrybe: render, lint, mermaid encode/decode, Google
Docs export/import, GUI launcher, and the buffer-aware commands that drive a
running app. Distributed as a `maturin` binary wheel (`scrybe-cli`). Python
on the outside, Rust on the inside.

## What it does

Provides a `scrybe` command-line tool with a set of subcommands plus a
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
| `scrybe view FILE` | View a Markdown file in the terminal (single-pane TUI): j/k scroll, g/G top/bottom, q quits; requires a TTY (exits 2 in a pipe) |
| `scrybe lint FILE` | Word count, headings, code blocks, broken links; `--json` for machine output; exits 1 if broken links found |
| `scrybe mermaid embed PNG SOURCE` | Embed Mermaid source into PNG iTXt chunk |
| `scrybe mermaid extract PNG` | Print embedded Mermaid source, verifying its SHA-256 by default; exit 2 if tampered, `--unverified` to skip the check (forensics) |
| `scrybe mermaid verify PNG` | Verify SHA-256 integrity of embedded source; exits 1 if tampered or digest missing |
| `scrybe gdoc login [--json]` | One-time Google sign-in: opens a browser for the consent screen and caches the token (`drive.file` scope only). Needs a bring-your-own OAuth client — see `docs/google-docs.md` |
| `scrybe gdoc logout [--json]` | Revoke and forget the cached token |
| `scrybe gdoc status [--json]` | Sign-in state without contacting Google, always naming the config directory; exits 0 when signed in, 1 otherwise |
| `scrybe gdoc list [--json]` | The Google Docs Scrybe created, newest first (`<id>  <name>  <url>` per line) |
| `scrybe gdoc export FILE [--title TITLE] [--json]` | Export a Markdown file to a Google Doc: creates it on the first export and writes `<stem>.gdoc.json` beside the file, updates it in place afterwards; Mermaid fences travel as PNGs carrying their source; a bare `.mmd` exports as a one-figure doc |
| `scrybe gdoc import DOC -o/--out FILE [--json]` | Import a Google Doc (`DOC` is an id or a docs/drive URL) as Markdown: Mermaid PNGs become fences again, other images land as `<stem>_img_NN.<ext>` beside `FILE` |
| `scrybe open [PATH]` | Launch the Scrybe GUI, optionally at a file or directory |
| `scrybe tabs` | List the tabs open in the running app (path, dirty, active) |
| `scrybe save FILE` | Save an open tab's buffer to disk (silent no-op if not open) |
| `scrybe close FILE` | Close a tab (silent no-op if not open) |
| `scrybe quit [--force]` | Quit the running app; `--force` skips the dirty-buffer prompt |
| `scrybe read FILE` | Print the in-memory contents of an open buffer (sees unsaved edits) |
| `scrybe find PATTERN [FILE…] [--literal]` | Search across open tabs for a regex (or a literal string) |
| `scrybe section FILE --heading TEXT` | Extract a section by heading (case-insensitive substring) |
| `scrybe edit FILE --start-line N --end-line M --content TEXT` | Replace lines N..=M of an open buffer (the tab stays dirty until saved) |
| `scrybe version` | Print version and active feature flags |
| `scrybe [PATH]` | Bare invocation with a path injects `open` automatically |

`gdoc` runs inline like `render` and `lint` — no running app — but needs
network access and a Google sign-in. `tabs`, `save`, `close`, `quit`,
`read`, `find`, `section` and `edit` talk to the running app over local IPC
(`~/.scrybe/sock` on Unix, a per-user named pipe on Windows).

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
