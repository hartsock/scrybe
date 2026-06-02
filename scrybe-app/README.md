<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-app

Tauri 2 desktop application: Rust backend wired to a TypeScript/CodeMirror 6
frontend via Tauri IPC. Python on the outside, Rust on the inside.

## What it does

Provides the full graphical Scrybe editor: a tabbed Markdown editor with live
HTML preview, syntax highlighting, VCS panel, agent panel, MCP sidecar, plugin
runner, swarm sidebar, and an embedded terminal. The app bundles the
`scrybe-mcp-server` as a sidecar so agents can connect without a separate
install step.

## Role in the architecture

`scrybe-app` is the integration layer — it does not own domain logic but wires
every other crate together through Tauri `invoke(...)` IPC calls. The Rust
backend (`src-tauri/`) owns state; the TypeScript frontend (`src/`) owns
presentation.

## Directory structure

```
scrybe-app/
├── src/                   TypeScript frontend
│   ├── main.ts            App bootstrap, window/tab lifecycle, MCP tab signal polling
│   ├── editor.ts          CodeMirror 6 editor setup and state management
│   ├── preview.ts         Live HTML preview pane
│   ├── tabs.ts            Tab bar: open, close, switch, dirty indicator
│   ├── sidebar.ts         File tree / folder browser
│   ├── mcp_panel.ts       Agent MCP connection panel
│   ├── vcs_panel.ts       Git status, stage, commit, fetch, log UI
│   ├── plugins.ts         Python plugin runner panel
│   ├── terminal.ts        Embedded shell terminal (P4.11)
│   ├── toolbar.ts         Top toolbar and theme switcher
│   ├── toast.ts           Non-blocking notification toasts
│   ├── state.ts           Shared frontend state (open docs, active tab, etc.)
│   └── styles/            Per-component CSS
│       ├── tabs.css
│       ├── sidebar.css
│       ├── preview.css
│       ├── mcp_panel.css
│       ├── vcs_panel.css
│       ├── terminal.css
│       └── toast.css
├── public/                Static assets (logo, fonts)
│   └── scrybe-logo.png
├── src-tauri/             Rust Tauri backend
│   └── src/
│       ├── lib.rs         All Tauri IPC commands (render, VCS, MCP sidecar, plugins, terminal)
│       └── main.rs        Binary entry point
├── index.html             Single-page app shell
├── vite.config.ts         Vite bundler config
├── tsconfig.json          TypeScript config
└── package.json           npm dependencies (CodeMirror 6, Vite)
```

## Tauri backend IPC commands (src-tauri/src/lib.rs)

Key `invoke(...)` targets exposed to the frontend:

| Command | Description |
|---------|-------------|
| `render_markdown` | Markdown → HTML via `scrybe-render` |
| `read_file` / `list_directory` | Filesystem access |
| `get_builtin_agents` / `set_agent_enabled` | Agent panel state |
| `list_plugins` / `run_plugin` | Python plugin execution |
| `mcp_server_start` / `mcp_server_status` / `mcp_connection_info` | In-app MCP sidecar (P4.7) |
| `vcs_open` / `vcs_status` / `vcs_stage_all` / `vcs_commit` / `vcs_fetch` / `vcs_log` / `vcs_remotes` | Git operations via `scrybe-vcs` (P4.8) |
| `terminal_start` / `terminal_write` / `terminal_run` | Embedded shell (P4.11) |
| `get_version` | Version string |

## Build and run

```sh
# Prerequisites: Rust toolchain, Node.js >= 20, Tauri CLI
cargo install tauri-cli --version "^2"
cd scrybe-app && npm install

# Development (hot-reload)
cargo tauri dev

# Production build (creates .app / .exe / .deb)
cargo tauri build

# Run tests (Rust backend only)
cargo test -p scrybe-app
```

On macOS the production build produces `scrybe-app/target/release/bundle/macos/Scrybe.app`.
Install to `~/Applications/Scrybe.app` for the CLI launcher to find it automatically.

From the repository root, `just install-app` builds and installs the desktop app
and the Python runtime tools it shells out to, including the Word (`.docx`)
exporter.
