<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-tools

One `ToolSpec` registry shared by the Scrybe **CLI** and the **MCP server**, so
CLI↔MCP parity holds by construction. This is the foundation crate for the MCP
rebuild — see [`docs/design/mcp-rebuild.md`](../docs/design/mcp-rebuild.md) and
epic [#122](https://github.com/hartsock/scrybe/issues/122).

## Model

- **`ToolSpec`** — one tool (name, description-as-agent-prompt, input schema,
  versioned `data` schema, `mutates`, `Facet`, handler), shared verbatim by both
  front ends.
- **`Registry`** — the set of tools. `Registry::default()` registers every
  built-in; `call(name, ctx, args)` dispatches.
- **`ToolOutcome`** — a successful call. A *business* failure (e.g. "heading not
  found") is data (`tool_error`), not an engine fault.
- **`EngineFault`** — the tool could not run at all (unknown tool, bad args,
  transport down). Surfaces as MCP `isError: true` / non-zero CLI exit.
- **`Transport`** — how a stateful tool reaches the live app over
  `~/.scrybe/sock`. `Headless` (this phase) runs the pure, GUI-free subset;
  `LiveApp` arrives in Phase 2.

## Status

Phase 1 (foundation): core types, the registry, the headless transport, and the
pure `render` tool. Follow-up phases add the remaining tools, `scrybe-rpc`
dispatch, the versioned data contract, progressive disclosure, and the CLI/MCP
rewiring.
