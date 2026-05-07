<!--
SPDX-License-Identifier: AGPL-3.0-or-later
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-mcp-client

Outbound MCP client: lets Scrybe connect to external agent MCP servers and
invoke their tools on behalf of the user. Python on the outside, Rust on the
inside.

## What it does

Manages a set of named agent server connections. Each registered server is
an MCP endpoint (stdio transport, SSE planned). `scrybe-app` uses this crate
to populate the agent panel and to forward panel/bake-off prompts to multiple
agents simultaneously.

## Role in the architecture

`scrybe-mcp-client` is the outbound counterpart to `scrybe-mcp-server`. While
the server makes Scrybe controllable by agents, this crate lets Scrybe reach
out to agents. It is consumed by `scrybe-panels` (bake-off orchestrator) and
by the Tauri backend's agent panel commands.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `McpClient` | Single connection to one MCP server; sends tool calls and parses responses |
| `ServerInfo` | Metadata returned by `initialize` handshake |
| `ToolDef` | Tool name + JSON schema as reported by `tools/list` |
| `AgentRegistry` | Named map of `AgentEntry` records; `register`, `get`, `list`, `load_presets` |
| `AgentEntry` | Name + transport + enabled flag |
| `Transport` | Enum: `Stdio { command, args }` (SSE stub planned) |
| `HarnessPreset` | Canned config for a known agent: `claude-code`, `codex`, `anthropic-api`, `openai-api`, `ollama` |
| `builtin_presets()` | Returns all five built-in presets (all disabled by default) |
| `get_preset(id)` | Look up a preset by its short id string |
| `load_agent_config(path)` | Parse `~/.config/scrybe/agents.toml` into `Vec<AgentConfigEntry>` |

Agents are disabled by default; operators enable them via
`~/.config/scrybe/agents.toml` or the in-app agent panel.

## Build and test

```sh
cargo build -p scrybe-mcp-client
cargo test -p scrybe-mcp-client
```

No network access is required for tests; external server processes are not
spawned unless tests explicitly request them.
