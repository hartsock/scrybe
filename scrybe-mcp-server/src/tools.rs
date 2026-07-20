// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Tool registry for the Scrybe MCP server.
//!
//! The LEGACY half of the MCP tool surface: embed, extract, logs, quit,
//! close_tab, plus the UI-control parity tools state, set_theme, view_mode,
//! set_vim, and export. The editor tools (open/read/section/edit/find/
//! render/lint/reload and friends) are served by the shared `scrybe-tools`
//! registry — see `server.rs`. Every tool here is slated for migration or
//! removal before 0.6.0 (workstream A2).

use serde_json::{json, Value};

/// All tool names exposed by scrybe-mcp-server.
pub const TOOL_NAMES: &[&str] = &[
    "embed",
    "extract",
    "logs",
    "quit",
    "close_tab",
    "state",
    "set_theme",
    "view_mode",
    "set_vim",
    "export",
];

/// Path shared between the Tauri app's `log_append` command and this tool.
const LOG_FILE: &str = "/tmp/scrybe-debug.log";

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
}

fn home_venv_bin(name: &str) -> Option<std::path::PathBuf> {
    let bin_dir = if cfg!(windows) { "Scripts" } else { "bin" };
    home_dir().map(|home| home.join("venv").join(bin_dir).join(name))
}

fn existing_file(path: std::path::PathBuf) -> Option<String> {
    if path.is_file() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn which_scrybe_docx() -> Result<String, String> {
    if let Ok(path) = std::env::var("SCRYBE_DOCX_BIN") {
        if let Some(path) = existing_file(std::path::PathBuf::from(path)) {
            return Ok(path);
        }
    }

    let name = executable_name("scrybe-docx");
    if let Ok(exe) = std::env::current_exe() {
        if let Some(path) = existing_file(exe.with_file_name(&name)) {
            return Ok(path);
        }
    }
    if let Ok(path) = which::which(&name) {
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Some(path) = home_venv_bin(&name).and_then(existing_file) {
        return Ok(path);
    }

    Err(
        "scrybe-docx not found. Reinstall the Scrybe Python toolkit with docx export support or set SCRYBE_DOCX_BIN to the exporter executable."
            .to_string(),
    )
}

/// Legacy hand-rolled registry — the shadowed id-based editor tools and their
/// shadow `Workspace`/id-map were DELETED (#181); the shared `scrybe-tools`
/// registry (`server.rs::call_shared`) serves those tools path-based over the
/// socket. What remains here are the still-live legacy control tools
/// (embed/extract/quit/close_tab/logs/state/set_theme/view_mode/set_vim/
/// export), each slated to migrate to a typed socket method, an in-process
/// service, or removal from the 0.6.0 surface (workstream A2).
pub struct ToolRegistry;

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Creates a new tool registry.
    pub fn new() -> Self {
        Self
    }

    /// Returns the MCP `tools/list` response body.
    pub fn list_tools_json(&self) -> Value {
        json!({"tools": [
            {
                "name": "embed",
                "description": "Embed Mermaid source into a PNG file (iTXt).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "png_path": {"type": "string"},
                        "source": {"type": "string"}
                    },
                    "required": ["png_path", "source"]
                }
            },
            {
                "name": "extract",
                "description": "Extract Mermaid source from a PNG file, verifying the \
                     embedded sha256 against the extracted source. Returns \
                     `verification`: \"verified\" (digest matched) or \"no-digest\" \
                     (payload stored none); a digest mismatch is an error.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"png_path": {"type": "string"}},
                    "required": ["png_path"]
                }
            },
            {
                "name": "quit",
                "description": "Gracefully close the running Scrybe app window.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "close_tab",
                "description": "Close a tab in the running Scrybe app by file path. Closes the active tab if no path given.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute path of the tab to close (omit to close active tab)"}
                    }
                }
            },
            {
                "name": "logs",
                "description": "Read recent console log entries from the running Scrybe app (errors, warnings, info). Returns up to `tail` lines (default 50).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tail": {"type": "integer", "minimum": 1, "maximum": 1000, "description": "Max lines to return from the end of the log (default 50)"}
                    }
                }
            },
            {
                "name": "state",
                "description": "Report the running Scrybe app's current UI state: the active tab's path/title/dirty flag, view mode, theme, and whether Vim mode is on. Human equivalent: the path bar, tab mode icon, theme dropdown, and Vim toggle.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "set_theme",
                "description": "Set the editor + preview theme in the running Scrybe app. Human equivalent: the toolbar theme dropdown.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "theme": {"type": "string", "enum": ["default", "dark", "solarized"]}
                    },
                    "required": ["theme"]
                }
            },
            {
                "name": "view_mode",
                "description": "Set the active tab's view mode in the running Scrybe app. Human equivalent: the toolbar View button / per-tab mode icon.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "mode": {"type": "string", "enum": ["both", "edit", "preview", "cycle"], "description": "Concrete mode, or 'cycle' to advance both→edit→preview"}
                    },
                    "required": ["mode"]
                }
            },
            {
                "name": "set_vim",
                "description": "Enable or disable Vim keybindings in the running Scrybe editor. Human equivalent: the toolbar Vim toggle.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "enabled": {"type": "boolean"}
                    },
                    "required": ["enabled"]
                }
            },
            {
                "name": "export",
                "description": "Export a Markdown file to a Word (.docx) document with Mermaid diagrams rendered to PNGs (source embedded in PNG metadata). Human equivalent: the toolbar Export button.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "input": {"type": "string", "description": "Path to the Markdown file to export"},
                        "output": {"type": "string", "description": "Output .docx path (default: input with .docx extension)"},
                        "no_diagrams": {"type": "boolean", "description": "Skip Mermaid rendering; keep fenced blocks as monospace text"}
                    },
                    "required": ["input"]
                }
            }
        ]})
    }

    /// Dispatch a tool call by name.
    pub fn call_tool(&mut self, name: &str, args: &Value) -> Value {
        match name {
            "embed" => self.tool_embed(args),
            "extract" => self.tool_extract(args),
            "logs" => self.tool_logs(args),
            "quit" => self.tool_quit(),
            "close_tab" => self.tool_close_tab(args),
            "state" => self.tool_state(),
            "set_theme" => self.tool_set_theme(args),
            "view_mode" => self.tool_view_mode(args),
            "set_vim" => self.tool_set_vim(args),
            "export" => self.tool_export(args),
            other => json!({"error": format!("unknown tool: {other}")}),
        }
    }

    // -----------------------------------------------------------------------
    // Individual tool implementations
    // -----------------------------------------------------------------------

    fn tool_embed(&self, args: &Value) -> Value {
        let png_path = match args["png_path"].as_str() {
            Some(p) => p,
            None => return json!({"error": "png_path required"}),
        };
        let source = match args["source"].as_str() {
            Some(s) => s,
            None => return json!({"error": "source required"}),
        };
        let bytes = match std::fs::read(png_path) {
            Ok(b) => b,
            Err(e) => return json!({"error": e.to_string()}),
        };
        match scrybe_mermaid::embed(&bytes, source) {
            Ok(out) => {
                if let Err(e) = std::fs::write(png_path, &out) {
                    return json!({"error": e.to_string()});
                }
                json!({"ok": true, "path": png_path})
            }
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    fn tool_extract(&self, args: &Value) -> Value {
        let png_path = match args["png_path"].as_str() {
            Some(p) => p,
            None => return json!({"error": "png_path required"}),
        };
        let bytes = match std::fs::read(png_path) {
            Ok(b) => b,
            Err(e) => return json!({"error": e.to_string()}),
        };
        // Verifies the stored sha256 by default; a mismatch surfaces as an
        // error (the Display carries both digests) rather than silently
        // returning tampered source.
        match scrybe_mermaid::extract(&bytes) {
            Ok(payload) => json!({
                "source": payload.source,
                "sha256": payload.sha256().unwrap_or_default(),
                "uuid": payload.uuid,
                "verification": if payload.is_verified() { "verified" } else { "no-digest" },
            }),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    fn tool_close_tab(&self, args: &Value) -> Value {
        let path = args["path"].as_str().unwrap_or("").to_string();
        // With a concrete path and a live app, close the tab over the socket
        // (retires the /tmp/scrybe-close-tab.txt poll for the common case).
        if !path.is_empty() && scrybe_rpc::client::is_live() {
            match scrybe_rpc::client::send("close", json!({ "path": path })) {
                Ok(resp) if resp.error.is_none() => {
                    let applied = resp
                        .result
                        .as_ref()
                        .and_then(|r| r.get("applied"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    return json!({"ok": true, "path": path, "applied": applied, "via": "rpc"});
                }
                Ok(resp) => {
                    let msg = resp.error.map(|e| e.message).unwrap_or_default();
                    return json!({"error": msg, "path": path});
                }
                Err(_) => { /* fall through to the file signal */ }
            }
        }
        // No path (active tab) or no live socket — keep the file-signal path the
        // frontend polls. (Active-tab close over the socket lands with #46/#123.)
        let signal_path = "/tmp/scrybe-close-tab.txt";
        match std::fs::write(signal_path, &path) {
            Ok(_) => json!({"ok": true, "path": path, "via": "signal"}),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    fn tool_quit(&self) -> Value {
        // Prefer a graceful quit over the live socket; the app can then run its
        // dirty-buffer checks. Fall back to a signal when no app is reachable.
        if scrybe_rpc::client::is_live() {
            match scrybe_rpc::client::send("quit", json!({ "force": false })) {
                Ok(resp) if resp.error.is_none() => return json!({"ok": true, "via": "rpc"}),
                Ok(resp) => {
                    let msg = resp.error.map(|e| e.message).unwrap_or_default();
                    return json!({"ok": false, "error": msg, "via": "rpc"});
                }
                Err(_) => { /* fall through to pkill */ }
            }
        }
        let result = std::process::Command::new("pkill")
            .args(["-TERM", "-f", "scrybe-app"])
            .output();
        match result {
            Ok(out) if out.status.success() || out.status.code() == Some(1) => {
                // pkill exits 1 when no process matched (already closed) — treat as ok
                json!({"ok": true, "via": "signal"})
            }
            Ok(out) => {
                json!({"ok": false, "error": String::from_utf8_lossy(&out.stderr).trim().to_string()})
            }
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    // ── UI-control parity tools (mirror scrybe-app human controls) ──────────

    /// Report the running app's current UI state (active path, view mode,
    /// theme, vim). The app mirrors this to `/tmp/scrybe-state.json` via its
    /// `publish_state` command whenever the human changes something.
    fn tool_state(&self) -> Value {
        const STATE_FILE: &str = "/tmp/scrybe-state.json";
        match std::fs::read_to_string(STATE_FILE) {
            Ok(s) => match serde_json::from_str::<Value>(&s) {
                Ok(v) => v,
                Err(e) => json!({"error": format!("invalid state file: {e}")}),
            },
            Err(_) => json!({
                "note": "no state available — is scrybe-app running?",
                "path": STATE_FILE
            }),
        }
    }

    /// Signal the running app to change the editor + preview theme. The
    /// frontend polls `/tmp/scrybe-set-theme.txt` and applies it.
    fn tool_set_theme(&self, args: &Value) -> Value {
        let theme = match args["theme"].as_str() {
            Some(t @ ("default" | "dark" | "solarized")) => t,
            Some(other) => return json!({"error": format!("invalid theme: {other}")}),
            None => return json!({"error": "theme required"}),
        };
        match std::fs::write("/tmp/scrybe-set-theme.txt", theme) {
            Ok(_) => json!({"ok": true, "theme": theme}),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    /// Signal the running app to change the active tab's view mode.
    fn tool_view_mode(&self, args: &Value) -> Value {
        let mode = match args["mode"].as_str() {
            Some(m @ ("both" | "edit" | "preview" | "cycle")) => m,
            Some(other) => return json!({"error": format!("invalid mode: {other}")}),
            None => return json!({"error": "mode required"}),
        };
        match std::fs::write("/tmp/scrybe-view-mode.txt", mode) {
            Ok(_) => json!({"ok": true, "mode": mode}),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    /// Signal the running app to enable/disable Vim keybindings.
    fn tool_set_vim(&self, args: &Value) -> Value {
        let enabled = match args["enabled"].as_bool() {
            Some(b) => b,
            None => return json!({"error": "enabled (boolean) required"}),
        };
        let signal = if enabled { "on" } else { "off" };
        match std::fs::write("/tmp/scrybe-set-vim.txt", signal) {
            Ok(_) => json!({"ok": true, "vim": enabled}),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    /// Export a Markdown file to Word (.docx) by shelling to `scrybe-docx`.
    /// Renders Mermaid blocks to PNGs with the source embedded in metadata.
    fn tool_export(&self, args: &Value) -> Value {
        let input = match args["input"].as_str() {
            Some(p) => p.to_string(),
            None => return json!({"error": "input required"}),
        };
        let output = match args["output"].as_str() {
            Some(o) => o.to_string(),
            None => {
                let p = std::path::Path::new(&input);
                p.with_extension("docx").to_string_lossy().into_owned()
            }
        };
        let no_diagrams = args["no_diagrams"].as_bool().unwrap_or(false);

        let bin = match which_scrybe_docx() {
            Ok(bin) => bin,
            Err(e) => return json!({"error": e}),
        };

        let mut cmd = std::process::Command::new(bin);
        cmd.arg(&input).arg("-o").arg(&output);
        if no_diagrams {
            cmd.arg("--no-diagrams");
        }
        match cmd.output() {
            Ok(out) if out.status.success() => json!({"ok": true, "output": output}),
            Ok(out) => json!({
                "error": String::from_utf8_lossy(&out.stderr).trim().to_string()
            }),
            Err(e) => json!({
                "error": format!("failed to run scrybe-docx ({e})")
            }),
        }
    }

    fn tool_logs(&self, args: &Value) -> Value {
        let tail = args["tail"].as_u64().unwrap_or(50) as usize;
        match std::fs::read_to_string(LOG_FILE) {
            Ok(content) => {
                let all: Vec<&str> = content.lines().collect();
                let entries: Vec<&str> = all.iter().rev().take(tail).rev().cloned().collect();
                json!({"entries": entries, "total": all.len(), "path": LOG_FILE})
            }
            Err(e) => json!({"entries": [], "total": 0, "path": LOG_FILE, "note": e.to_string()}),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_list_tools_count_matches_registry() {
        let reg = ToolRegistry::new();
        let tools = reg.list_tools_json();
        let arr = tools["tools"].as_array().unwrap();
        assert_eq!(arr.len(), TOOL_NAMES.len());
        // 10 legacy control tools remain post-#181 (the 8 shadowed editor
        // tools were deleted; the shared scrybe-tools registry serves them).
        assert_eq!(arr.len(), 10);
    }

    #[test]
    fn test_set_theme_rejects_invalid() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("set_theme", &json!({"theme": "neon"}));
        assert!(result["error"].as_str().unwrap().contains("invalid theme"));
    }

    #[test]
    fn test_view_mode_requires_mode() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("view_mode", &json!({}));
        assert!(result["error"].is_string());
    }

    #[test]
    fn test_set_vim_requires_bool() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("set_vim", &json!({"enabled": "yes"}));
        assert!(result["error"].is_string());
    }

    #[test]
    fn test_export_requires_input() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("export", &json!({}));
        assert!(result["error"].as_str().unwrap().contains("input required"));
    }

    #[test]
    fn test_docx_binary_name_is_platform_specific() {
        let name = executable_name("scrybe-docx");
        if cfg!(windows) {
            assert_eq!(name, "scrybe-docx.exe");
        } else {
            assert_eq!(name, "scrybe-docx");
        }
    }

    #[test]
    fn test_existing_file_returns_candidate_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(executable_name("scrybe-docx"));
        std::fs::write(&path, "#!/bin/sh\n").expect("seed exporter");

        assert_eq!(
            existing_file(path.clone()),
            Some(path.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn test_unknown_tool_returns_error() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("nonexistent", &json!({}));
        assert!(result["error"].as_str().unwrap().contains("unknown tool"));
    }
}
