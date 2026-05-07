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

`scrybe-mcp-server` is the agent-facing surface of Scrybe. It holds an in-memory
`Workspace` (from `scrybe-core`) and coordinates with `scrybe-render` and
`scrybe-mermaid` to serve tool calls. The Tauri app launches it as a sidecar
process. Agents connect via:

```sh
claude mcp add scrybe -- scrybe-mcp-server stdio
```

## Exposed tools (12)

| Tool | Description |
|------|-------------|
| `open` | Open a Markdown file or directory; returns document ID |
| `read` | Return raw Markdown source of an open document |
| `section` | Extract a heading section by H-level and 0-based index |
| `edit` | Replace first occurrence of `old_text` with `new_text` |
| `find` | Search for a string; returns matching lines with line numbers |
| `render` | Render an open document to HTML (theme: default/dark/solarized) |
| `embed` | Embed Mermaid source into a PNG as an iTXt metadata chunk |
| `extract` | Extract Mermaid source from a PNG |
| `lint` | Word count, heading count, code blocks, broken links |
| `logs` | Tail recent console log entries from the running app |
| `close_tab` | Close a tab in the running app by file path |
| `quit` | Gracefully terminate the running Scrybe app window |

## Key public types

| Symbol | Description |
|--------|-------------|
| `McpServer` | Top-level server: owns the stdio transport loop |
| `ToolRegistry` | Dispatches tool calls; holds the `Workspace` and id map |
| `TOOL_NAMES` | `&[&str]` slice of all 12 tool name strings |

## Build and test

```sh
cargo build -p scrybe-mcp-server
cargo test -p scrybe-mcp-server

# Install the binary
cargo install --path .
```

The binary (`scrybe-mcp-server`) speaks MCP over stdio. Add it to any MCP
client's server list with transport `stdio`.
