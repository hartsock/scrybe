// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! In-app MCP server (P4.7): sidecar state and the `mcp_*` commands.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

// ─── P4.7 — In-app MCP server ────────────────────────────────────────────────

/// Tracks whether the in-app MCP sidecar is running.
static MCP_RUNNING: AtomicBool = AtomicBool::new(false);

// ─── P4.7 — MCP server commands ─────────────────────────────────────────────

/// Start the in-app MCP server as a sidecar process.
///
/// Spawns `scrybe-mcp-server stdio` and keeps it alive.  The server
/// communicates with external agents (Claude Code, Codex, etc.) over
/// stdio via the MCP JSON-RPC protocol.  Calling this when the server is
/// already running is a no-op (returns `"already running"`).
#[tauri::command]
pub fn mcp_server_start() -> Result<String, String> {
    if MCP_RUNNING.load(Ordering::Relaxed) {
        return Ok("already running".to_string());
    }
    let binary = which_scrybe_mcp_server()?;
    Command::new(&binary)
        .arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start scrybe-mcp-server: {e}"))?;
    MCP_RUNNING.store(true, Ordering::Relaxed);
    Ok(format!("started: {binary}"))
}

/// Returns whether the MCP server is running and its active transport.
#[tauri::command]
pub fn mcp_server_status() -> serde_json::Value {
    serde_json::json!({
        "running": MCP_RUNNING.load(Ordering::Relaxed),
        "transport": "stdio",
        "info": "Use `claude mcp add scrybe -- scrybe-mcp-server stdio` to connect."
    })
}

/// Returns the MCP connection instructions for the active session.
#[tauri::command]
pub fn mcp_connection_info() -> serde_json::Value {
    serde_json::json!({
        "stdio": {
            "command": "scrybe-mcp-server",
            "args": ["stdio"],
            "description": "Connect any MCP client via stdio transport"
        },
        "claude_code": "claude mcp add scrybe -- scrybe-mcp-server stdio",
        "codex": "codex mcp add scrybe -- scrybe-mcp-server stdio",
        "tools": [
            "open", "read", "section", "edit", "find",
            "render", "embed", "extract", "lint"
        ]
    })
}

/// Locate the `scrybe-mcp-server` binary on PATH or next to the current executable.
fn which_scrybe_mcp_server() -> Result<String, String> {
    if let Ok(path) = which::which("scrybe-mcp-server") {
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Ok(exe) = std::env::current_exe() {
        let name = if cfg!(windows) {
            "scrybe-mcp-server.exe"
        } else {
            "scrybe-mcp-server"
        };
        let sibling = exe.with_file_name(name);
        if sibling.exists() {
            return Ok(sibling.to_string_lossy().into_owned());
        }
    }
    Err(
        "scrybe-mcp-server not found on PATH. Install with: pip install scrybe-mcp-server"
            .to_string(),
    )
}
