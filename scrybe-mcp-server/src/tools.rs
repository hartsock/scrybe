// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Tool registry for the Scrybe MCP server.
//!
//! The LEGACY half of the MCP tool surface — now just embed, extract, and
//! export (the A2a migration inventory). Everything else is served by the
//! shared `scrybe-tools` registry — see `server.rs`. A2b moved the UI-parity
//! controls (state/set_theme/view_mode/set_vim/logs/quit/close_tab) onto
//! typed socket methods and deleted the /tmp signal files and the pkill
//! fallback they rode on.

use serde_json::{json, Value};

/// All tool names exposed by scrybe-mcp-server.
pub const TOOL_NAMES: &[&str] = &["embed", "extract", "export"];

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
/// socket. What remains here are embed/extract/export — the A2a inventory,
/// each to become an in-process shared tool (embed/extract ride the
/// verified-extraction API; export drives scrybe-docx).
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

    // ── UI-control parity tools (mirror scrybe-app human controls) ──────────

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
        // 3 legacy tools remain post-A2b (embed/extract/export — the A2a
        // migration inventory); everything else is served by the shared
        // scrybe-tools registry.
        assert_eq!(arr.len(), 3);
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
