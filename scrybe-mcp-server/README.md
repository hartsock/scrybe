<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-mcp-server

Inbound MCP server: exposes Scrybe's editing capabilities as MCP tools that
external agents (Claude Code, Codex, custom agents) can call. Python on the
outside, Rust on the inside.

## What it does

Implements the MCP JSON-RPC 2.0 protocol over stdio. An AI agent connects to
this process and gains structured, safe access to open Markdown documents —
reading, editing, searching, rendering, and managing the running app — without
direct filesystem access.

## Role in the architecture

`scrybe-mcp-server` is the agent-facing surface of Scrybe — a thin stdio shim
over the shared `scrybe-tools` registry, the ONE tool registry (handlers,
schemas, dispatch) it shares with the CLI, so the two surfaces match by
construction. Stateful tools drive the *running app* over `~/.scrybe/sock`;
in-process tools (`render`, `lint`, `embed`, `extract`, `export`,
`mermaid_to_png`, `export_figures`) work headless. The Google Docs tools
(`gdoc_auth`, `gdoc_list`, `gdoc_export`, `gdoc_import`) are in-process too,
but need the user's Google sign-in — a bring-your-own OAuth client and a
cached token (`docs/google-docs.md`); `gdoc_auth` `login` opens a browser on
the host. Agents connect via:

```sh
claude mcp add scrybe -- scrybe-mcp-server stdio
```

## Exposed tools

The authoritative list is `scrybe-mcp-server tools` (or MCP `tools/list`):
`open`, `read`, `section`, `edit`, `save`, `find`, `render`, `embed`,
`extract`, `lint`, `list_tabs`, `mermaid_to_png`, `export_figures`, `export`,
`logs`, `reload`, `close_tab`, `quit`, `state`, `set_theme`, `view_mode`,
`set_vim`, `gdoc_auth`, `gdoc_list`, `gdoc_export`, `gdoc_import`. See
`AGENTS.md` (repo root) for the full per-tool reference. Business failures
from the `gdoc_*` tools are `isError: true` results whose
`structuredContent.code` is one of `not_configured`, `not_logged_in`,
`auth_failed`, `doc_not_found`, `api_error`, `http_error`, `render_failed`,
`invalid_argument`, `io_error`.

## Key public types

| Symbol | Description |
|--------|-------------|
| `McpServer` | Top-level server: owns the stdio transport loop and formats the MCP envelope around `scrybe_tools::Registry` outcomes |

## Build and test

```sh
cargo build -p scrybe-mcp-server
cargo test -p scrybe-mcp-server

# Install the binary
cargo install --path .
```

The binary (`scrybe-mcp-server`) speaks MCP over stdio. Add it to any MCP
client's server list with transport `stdio`.
