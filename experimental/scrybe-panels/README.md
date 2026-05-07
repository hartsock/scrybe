<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-panels

Bake-off orchestrator and SQLite calibration log for multi-agent panel scoring.
Python on the outside, Rust on the inside.

## What it does

Implements the "bake-off" pattern: a single prompt is broadcast to all enabled
agents registered in `scrybe-mcp-client`'s `AgentRegistry`. The responses are
presented side-by-side in the Scrybe panel UI. Human thumbs-up/down feedback is
recorded in a local SQLite database, producing a calibration dataset that can
be used to improve agent selection and prompt routing over time.

## Role in the architecture

`scrybe-panels` sits above `scrybe-mcp-client` (it uses `AgentRegistry` to
reach agents) and is consumed by the Tauri backend's panel commands. It
implements the dimensional agent model described in the project's design
philosophy: multiple agents with different dispositions, evaluated side-by-side
against real user feedback.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `PanelOrchestrator` | Broadcasts a prompt to N agents via `AgentRegistry`; collects responses |
| `CalibrationLog` | SQLite-backed store for panel results and thumbs-up/down votes |

## Build and test

```sh
cargo build -p scrybe-panels
cargo test -p scrybe-panels
```

SQLite is bundled via `rusqlite` with the `bundled` feature — no external
database installation required. The calibration database is created on first use
at `~/.local/share/scrybe/calibration.db` (or the OS-appropriate data dir).
