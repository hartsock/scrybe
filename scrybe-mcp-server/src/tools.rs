// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Tool registry for the Scrybe MCP server.
//!
//! Exposes the Scrybe tool surface to MCP clients: open, read, section,
//! edit, find, render, embed, extract, lint, logs, quit, close_tab,
//! reload, plus the UI-control parity tools state, set_theme, view_mode,
//! set_vim, and export (each mirrors a human control in scrybe-app).

use scrybe_core::{Document, Node, Workspace};
use scrybe_render::{render_html, Theme};
use serde_json::{json, Value};
use std::collections::HashMap;

/// All tool names exposed by scrybe-mcp-server.
pub const TOOL_NAMES: &[&str] = &[
    "open",
    "read",
    "section",
    "edit",
    "find",
    "render",
    "embed",
    "extract",
    "lint",
    "logs",
    "quit",
    "close_tab",
    "reload",
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

/// Registry of all scrybe MCP tools plus the open-document workspace.
pub struct ToolRegistry {
    workspace: Workspace,
    /// Maps the string representation of a DocumentId to the actual id.
    id_map: HashMap<String, scrybe_core::workspace::DocumentId>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Creates a new, empty tool registry.
    pub fn new() -> Self {
        Self {
            workspace: Workspace::new(),
            id_map: HashMap::new(),
        }
    }

    /// Returns the MCP `tools/list` response body.
    pub fn list_tools_json(&self) -> Value {
        json!({"tools": [
            {
                "name": "open",
                "description": "Open a Markdown file (returns document ID) or a directory (launches the scrybe-app folder viewer).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute or relative file or directory path"}
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "read",
                "description": "Read the Markdown source of an open document.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"]
                }
            },
            {
                "name": "section",
                "description": "Return a heading section by heading text (case-insensitive substring). Returns {heading, level, content}. Reflects the LIVE editor buffer when the app is running.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "heading": {"type": "string", "description": "Heading text to find (case-insensitive substring)"}
                    },
                    "required": ["id", "heading"]
                }
            },
            {
                "name": "edit",
                "description": "Replace an inclusive 1-indexed LINE RANGE with new content (same as the scrybe CLI and the app). Use start_line == end_line to replace one line, or start_line == end_line + 1 to insert without replacing. Applies to the LIVE editor buffer when the app is running.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "start_line": {"type": "integer", "minimum": 1, "description": "1-indexed first line to replace"},
                        "end_line": {"type": "integer", "minimum": 0, "description": "1-indexed last line to replace (inclusive)"},
                        "content": {"type": "string", "description": "Replacement text for the range"}
                    },
                    "required": ["id", "start_line", "end_line", "content"]
                }
            },
            {
                "name": "find",
                "description": "Search for a string; returns lines that contain it with context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "query": {"type": "string"}
                    },
                    "required": ["id", "query"]
                }
            },
            {
                "name": "render",
                "description": "Render an open document to HTML.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "theme": {"type": "string", "enum": ["default", "dark", "solarized"]}
                    },
                    "required": ["id"]
                }
            },
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
                "description": "Extract Mermaid source from a PNG file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"png_path": {"type": "string"}},
                    "required": ["png_path"]
                }
            },
            {
                "name": "lint",
                "description": "Lint a document: word count, headings, code blocks, broken links.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"]
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
                "name": "reload",
                "description": "Re-read an open document from disk, replacing the in-memory buffer. Use after any external edit (vim, Claude Code Edit, git checkout) to keep Scrybe in sync and prevent autosave from clobbering external changes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "description": "DocumentId(...) of the open document to reload"},
                        "force": {"type": "boolean", "description": "If false (default) and buffer differs from disk, returns an error. Set true to discard in-memory changes."}
                    },
                    "required": ["id"]
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
            "open" => self.tool_open(args),
            "read" => self.tool_read(args),
            "section" => self.tool_section(args),
            "edit" => self.tool_edit(args),
            "find" => self.tool_find(args),
            "render" => self.tool_render(args),
            "embed" => self.tool_embed(args),
            "extract" => self.tool_extract(args),
            "lint" => self.tool_lint(args),
            "logs" => self.tool_logs(args),
            "quit" => self.tool_quit(),
            "close_tab" => self.tool_close_tab(args),
            "reload" => self.tool_reload(args),
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

    fn tool_open(&mut self, args: &Value) -> Value {
        let path = match args["path"].as_str() {
            Some(p) => std::path::PathBuf::from(p),
            None => return json!({"error": "path required"}),
        };
        if path.is_dir() {
            return self.launch_app(&path);
        }
        // Read the file into the MCP workspace so read/find/section/edit have a
        // buffer to work against regardless of GUI state.
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => return json!({"error": e.to_string()}),
        };
        let doc = Document::from_file(path.clone(), source);
        let doc_id = self.workspace.open(doc);
        let id_str = format!("{doc_id:?}");
        self.id_map.insert(id_str.clone(), doc_id);
        let path_str = path.display().to_string();

        // Surface the tab in the live editor. When an app is already running we
        // MUST dial it over scrybe-rpc (emitting `scrybe://cli-open`, exactly as
        // the CLI does) — spawning a second process is swallowed by the
        // single-instance guard, which is the root cause of #108 ("open returns
        // success but no tab appears"). Only when nothing is running do we
        // launch a fresh app with the path.
        if scrybe_rpc::client::is_live() {
            match scrybe_rpc::client::send("open", json!({ "path": path_str })) {
                Ok(resp) if resp.error.is_none() => {
                    let tab_id = resp
                        .result
                        .as_ref()
                        .and_then(|r| r.get("tab_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&path_str)
                        .to_string();
                    json!({"id": id_str, "path": path_str, "live": true, "tab_id": tab_id})
                }
                Ok(resp) => {
                    let msg = resp.error.map(|e| e.message).unwrap_or_default();
                    json!({"error": format!("live app rejected open: {msg}"), "path": path_str})
                }
                Err(e) => {
                    json!({"error": format!("failed to reach live app: {e}"), "path": path_str})
                }
            }
        } else if let Ok(binary) = which_scrybe_app() {
            let _ = std::process::Command::new(&binary).arg(&path).spawn();
            json!({"id": id_str, "path": path_str, "live": false, "launched": true})
        } else {
            json!({"id": id_str, "path": path_str, "live": false,
                "note": "no running app and scrybe-app not found; buffer loaded headlessly"})
        }
    }

    fn launch_app(&self, path: &std::path::Path) -> Value {
        let app_binary = match which_scrybe_app() {
            Ok(b) => b,
            Err(e) => return json!({"error": e}),
        };
        match std::process::Command::new(&app_binary).arg(path).spawn() {
            Ok(_) => json!({"opened": path.display().to_string(), "app": app_binary}),
            Err(e) => json!({"error": format!("failed to launch scrybe-app: {e}")}),
        }
    }

    /// Resolve an open-document id to its canonical file path (the handle the
    /// live app and scrybe-rpc address documents by). Errors as a JSON value.
    fn resolve_path(&self, id_str: &str) -> Result<String, Value> {
        let doc_id = self
            .id_map
            .get(id_str)
            .ok_or_else(|| json!({"error": format!("unknown id: {id_str}")}))?;
        let doc = self
            .workspace
            .get(doc_id)
            .ok_or_else(|| json!({"error": "document not found"}))?;
        doc.path
            .as_ref()
            .map(|p| p.display().to_string())
            .ok_or_else(|| json!({"error": "document has no file path"}))
    }

    /// Read the document. When the app is live, this returns the LIVE buffer
    /// (including the human's unsaved edits) via scrybe-rpc; otherwise it falls
    /// back to the in-memory workspace copy loaded at `open`.
    fn tool_read(&self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let path = match self.resolve_path(id_str) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if scrybe_rpc::client::is_live() {
            if let Ok(resp) = scrybe_rpc::client::send("read", json!({ "path": path })) {
                if resp.error.is_none() {
                    let r = resp.result.unwrap_or_default();
                    return json!({
                        "source": r.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                        "is_dirty": r.get("is_dirty").and_then(|v| v.as_bool()).unwrap_or(false),
                        "live": true,
                    });
                }
            }
            // rpc failed — fall through to the workspace copy below.
        }
        match self.id_map.get(id_str).and_then(|d| self.workspace.get(d)) {
            Some(doc) => json!({"source": doc.source, "live": false}),
            None => json!({"error": "document not found"}),
        }
    }

    /// Return a heading section by heading text (case-insensitive substring).
    /// Live app returns the current buffer's section via scrybe-rpc; headless
    /// falls back to slicing the in-memory copy.
    fn tool_section(&self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let heading = match args["heading"].as_str() {
            Some(h) if !h.is_empty() => h,
            _ => return json!({"error": "heading required and must be non-empty"}),
        };
        let path = match self.resolve_path(id_str) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if scrybe_rpc::client::is_live() {
            if let Ok(resp) =
                scrybe_rpc::client::send("section", json!({ "path": path, "heading": heading }))
            {
                match resp.error {
                    None => {
                        let mut r = resp.result.unwrap_or_default();
                        if let Some(o) = r.as_object_mut() {
                            o.insert("live".into(), json!(true));
                        }
                        return r;
                    }
                    Some(e) => return json!({"error": e.message}),
                }
            }
        }
        // Headless: slice the workspace copy by heading.
        let doc = match self.id_map.get(id_str).and_then(|d| self.workspace.get(d)) {
            Some(d) => d,
            None => return json!({"error": "document not found"}),
        };
        match section_by_heading(&doc.source, heading) {
            Some((h, level, content)) => {
                json!({"heading": h, "level": level, "content": content, "live": false})
            }
            None => json!({"error": format!("no heading matching '{heading}'")}),
        }
    }

    /// Replace the inclusive 1-indexed line range with `content` (matching the
    /// live app and `scrybe` CLI edit semantics; `start == end + 1` inserts).
    /// Live app edits the running buffer via scrybe-rpc; headless applies to
    /// the in-memory copy.
    fn tool_edit(&mut self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let start_line = match args["start_line"].as_u64() {
            Some(n) if n >= 1 => n,
            _ => return json!({"error": "start_line required (1-indexed, >= 1)"}),
        };
        let end_line = match args["end_line"].as_u64() {
            Some(n) => n,
            None => return json!({"error": "end_line required"}),
        };
        let content = match args["content"].as_str() {
            Some(c) => c,
            None => return json!({"error": "content required"}),
        };
        let path = match self.resolve_path(id_str) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if scrybe_rpc::client::is_live() {
            if let Ok(resp) = scrybe_rpc::client::send(
                "edit",
                json!({ "path": path, "start_line": start_line, "end_line": end_line, "content": content }),
            ) {
                match resp.error {
                    None => {
                        let mut r = resp.result.unwrap_or_default();
                        if let Some(o) = r.as_object_mut() {
                            o.insert("live".into(), json!(true));
                        }
                        return r;
                    }
                    Some(e) => return json!({"error": e.message}),
                }
            }
        }
        // Headless: splice the workspace copy.
        let doc_id = match self.id_map.get(id_str) {
            Some(id) => *id,
            None => return json!({"error": format!("unknown id: {id_str}")}),
        };
        let doc = match self.workspace.get_mut(&doc_id) {
            Some(d) => d,
            None => return json!({"error": "document not found"}),
        };
        match splice_lines(&doc.source, start_line as usize, end_line as usize, content) {
            Ok(new_source) => {
                let size = new_source.len();
                doc.source = new_source;
                json!({"applied": true, "size_after": size, "live": false})
            }
            Err(e) => json!({"error": e}),
        }
    }

    /// Search the document. Live app searches the running buffer via scrybe-rpc;
    /// headless searches the in-memory copy.
    fn tool_find(&self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let query = match args["query"].as_str() {
            Some(q) if !q.is_empty() => q,
            _ => return json!({"error": "query required and must be non-empty"}),
        };
        let path = match self.resolve_path(id_str) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if scrybe_rpc::client::is_live() {
            if let Ok(resp) = scrybe_rpc::client::send(
                "find",
                json!({ "pattern": query, "paths": [path], "literal": true }),
            ) {
                if resp.error.is_none() {
                    let hits = resp
                        .result
                        .as_ref()
                        .and_then(|r| r.get("hits"))
                        .and_then(|h| h.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let matches: Vec<Value> = hits
                        .iter()
                        .map(|h| {
                            json!({
                                "line": h.get("line").cloned().unwrap_or(json!(0)),
                                "text": h.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                            })
                        })
                        .collect();
                    return json!({"query": query, "matches": matches, "live": true});
                }
            }
        }
        // Headless: search the workspace copy.
        let doc = match self.id_map.get(id_str).and_then(|d| self.workspace.get(d)) {
            Some(d) => d,
            None => return json!({"error": "document not found"}),
        };
        let matches: Vec<Value> = doc
            .source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(query))
            .map(|(i, line)| json!({"line": i + 1, "text": line}))
            .collect();
        json!({"query": query, "matches": matches, "live": false})
    }

    fn tool_render(&self, args: &Value) -> Value {
        let theme = match args["theme"].as_str() {
            Some("dark") => Theme::Dark,
            Some("solarized") => Theme::Solarized,
            _ => Theme::Default,
        };

        // Prefer an open document by id.
        if let Some(id_str) = args["id"].as_str() {
            let doc_id = match self.id_map.get(id_str) {
                Some(id) => id,
                None => return json!({"error": format!("unknown id: {id_str}")}),
            };
            let doc = match self.workspace.get(doc_id) {
                Some(d) => d,
                None => return json!({"error": "document not found"}),
            };
            let out = render_html(doc, theme);
            return json!({"html": out.html});
        }

        // Fall back to inline source (convenience for quick renders).
        if let Some(source) = args["source"].as_str() {
            let doc = Document::new(source.to_string());
            let out = render_html(&doc, theme);
            return json!({"html": out.html});
        }

        json!({"error": "provide id or source"})
    }

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
        match scrybe_mermaid::extract(&bytes) {
            Ok(payload) => json!({"source": payload.source, "sha256": payload.sha256}),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    fn tool_lint(&self, args: &Value) -> Value {
        let tmp_doc: Document;
        let doc_ref: &Document;

        if let Some(id_str) = args["id"].as_str() {
            let doc_id = match self.id_map.get(id_str) {
                Some(id) => id,
                None => return json!({"error": format!("unknown id: {id_str}")}),
            };
            doc_ref = match self.workspace.get(doc_id) {
                Some(d) => d,
                None => return json!({"error": "document not found"}),
            };
        } else if let Some(source) = args["source"].as_str() {
            tmp_doc = Document::new(source.to_string());
            doc_ref = &tmp_doc;
        } else {
            return json!({"error": "provide id or source"});
        }

        let words = doc_ref.source.split_whitespace().count();
        let ast = doc_ref.ast();

        let mut heading_count = 0usize;
        let mut code_block_count = 0usize;
        let mut link_count = 0usize;

        count_nodes(
            &ast.nodes,
            &mut heading_count,
            &mut code_block_count,
            &mut link_count,
        );

        json!({
            "word_count": words,
            "heading_count": heading_count,
            "code_block_count": code_block_count,
            "link_count": link_count,
        })
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

    fn tool_reload(&mut self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let force = args["force"].as_bool().unwrap_or(false);

        let doc_id = match self.id_map.get(id_str) {
            Some(id) => *id,
            None => return json!({"error": format!("unknown id: {id_str}")}),
        };

        // Get the file path from the document.
        let path = match self.workspace.get(&doc_id) {
            Some(doc) => match doc.path.clone() {
                Some(p) => p,
                None => return json!({"error": "document has no file path"}),
            },
            None => return json!({"error": "document not found"}),
        };

        let new_source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => return json!({"error": format!("read failed: {e}")}),
        };

        // Dirty check: in-memory content differs from what's on disk.
        let is_dirty = self
            .workspace
            .get(&doc_id)
            .map(|d| d.source != new_source)
            .unwrap_or(false);

        if is_dirty && !force {
            return json!({"error": "buffer dirty; pass force=true to discard"});
        }

        // Replace in-memory content.
        if let Some(doc) = self.workspace.get_mut(&doc_id) {
            doc.source = new_source.clone();
        }

        // Signal the GUI to refresh the tab.
        let path_str = path.display().to_string();
        if let Err(e) = std::fs::write("/tmp/scrybe-reload-tab.txt", &path_str) {
            return json!({"ok": true, "path": path_str,
                "warning": format!("GUI signal failed: {e}")});
        }

        json!({"ok": true, "path": path_str, "bytes": new_source.len()})
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

/// Locate the `scrybe-app` binary: sibling of current exe first, then PATH.
fn which_scrybe_app() -> Result<String, String> {
    if let Ok(exe) = std::env::current_exe() {
        let name = if cfg!(windows) {
            "scrybe-app.exe"
        } else {
            "scrybe-app"
        };
        let sibling = exe.with_file_name(name);
        if sibling.exists() {
            return Ok(sibling.to_string_lossy().into_owned());
        }
    }
    let output = std::process::Command::new("which")
        .arg("scrybe-app")
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !p.is_empty() {
            return Ok(p);
        }
    }
    Err("scrybe-app not found. Build with: cd scrybe-app && cargo tauri build".to_string())
}

/// Markdown ATX heading level (1-6) if `line` is a heading, else `None`.
fn heading_level(line: &str) -> Option<u8> {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

/// Find a Markdown section by heading (case-insensitive substring match) and
/// return (full heading text, level, body up to the next same-or-higher
/// heading). Headless fallback for the `section` tool.
fn section_by_heading(source: &str, query: &str) -> Option<(String, u8, String)> {
    let q = query.to_lowercase();
    let lines: Vec<&str> = source.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if let Some(level) = heading_level(line) {
            let title = line.trim_start().trim_start_matches('#').trim().to_string();
            if title.to_lowercase().contains(&q) {
                let mut body: Vec<&str> = Vec::new();
                for next in &lines[i + 1..] {
                    if let Some(nl) = heading_level(next) {
                        if nl <= level {
                            break;
                        }
                    }
                    body.push(next);
                }
                return Some((title, level, body.join("\n")));
            }
        }
    }
    None
}

/// Replace the inclusive 1-indexed line range `[start, end]` of `source` with
/// `content`. `start == end + 1` inserts before `start` without removing. The
/// original trailing newline (if any) is preserved. Headless fallback for `edit`.
fn splice_lines(source: &str, start: usize, end: usize, content: &str) -> Result<String, String> {
    if start == 0 {
        return Err("start_line must be >= 1".to_string());
    }
    let had_trailing_nl = source.ends_with('\n');
    let lines: Vec<&str> = source.lines().collect();
    let n = lines.len();
    let s = start - 1; // 0-indexed start
    if s > n {
        return Err(format!(
            "start_line {start} is beyond the end of the document ({n} lines)"
        ));
    }
    // Inclusive `end` -> exclusive index; for an insert (end < start) it equals s.
    let e = if end >= start { end.min(n) } else { s };
    let mut out: Vec<&str> = Vec::with_capacity(n);
    out.extend_from_slice(&lines[..s]);
    out.extend(content.lines());
    if e < n {
        out.extend_from_slice(&lines[e..]);
    }
    let mut result = out.join("\n");
    if had_trailing_nl {
        result.push('\n');
    }
    Ok(result)
}

fn count_nodes(nodes: &[Node], headings: &mut usize, code_blocks: &mut usize, links: &mut usize) {
    for node in nodes {
        match node {
            Node::Heading { children, .. } => {
                *headings += 1;
                count_nodes(children, headings, code_blocks, links);
            }
            Node::FencedCode { .. } => {
                *code_blocks += 1;
            }
            Node::Link { children, .. } => {
                *links += 1;
                count_nodes(children, headings, code_blocks, links);
            }
            Node::Paragraph { children }
            | Node::BlockQuote { children }
            | Node::List {
                items: children, ..
            }
            | Node::ListItem { children }
            | Node::Emphasis { children }
            | Node::Strong { children } => {
                count_nodes(children, headings, code_blocks, links);
            }
            _ => {}
        }
    }
}

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
        assert_eq!(arr.len(), 18);
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

    #[test]
    fn test_render_inline_source() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("render", &json!({"source": "# Hi\n\nWorld."}));
        let html = result["html"].as_str().unwrap_or("");
        assert!(html.contains("h1") || html.contains("Hi"));
    }

    #[test]
    fn test_lint_inline_source() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("lint", &json!({"source": "# Hello\n\nThis is a test.\n"}));
        assert!(result["word_count"].as_u64().unwrap() > 0);
        assert_eq!(result["heading_count"].as_u64().unwrap(), 1);
    }

    #[test]
    fn test_open_missing_file_returns_error() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("open", &json!({"path": "/nonexistent/file.md"}));
        assert!(result["error"].is_string());
    }

    #[test]
    fn test_read_unknown_id_returns_error() {
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("read", &json!({"id": "bogus-id-xyz"}));
        assert!(result["error"].is_string());
    }

    #[test]
    fn test_splice_lines_replaces_inclusive_range() {
        // Replace lines 2..=3 of a 4-line doc.
        let src = "a\nb\nc\nd\n";
        let out = splice_lines(src, 2, 3, "X\nY").unwrap();
        assert_eq!(out, "a\nX\nY\nd\n");
    }

    #[test]
    fn test_splice_lines_single_line() {
        let out = splice_lines("a\nb\nc\n", 2, 2, "B").unwrap();
        assert_eq!(out, "a\nB\nc\n");
    }

    #[test]
    fn test_splice_lines_insert_when_end_before_start() {
        // start == end + 1 inserts before `start` without removing.
        let out = splice_lines("a\nb\nc\n", 2, 1, "NEW").unwrap();
        assert_eq!(out, "a\nNEW\nb\nc\n");
    }

    #[test]
    fn test_splice_lines_rejects_out_of_range_start() {
        assert!(splice_lines("a\nb\n", 5, 5, "x").is_err());
        assert!(splice_lines("a\n", 0, 0, "x").is_err());
    }

    #[test]
    fn test_section_by_heading_slices_body() {
        let src = "# Title\n\nintro\n\n## Alpha\n\nbody one\nbody two\n\n## Beta\n\nother\n";
        let (h, level, content) = section_by_heading(src, "alpha").unwrap();
        assert_eq!(h, "Alpha");
        assert_eq!(level, 2);
        assert!(content.contains("body one"));
        assert!(content.contains("body two"));
        // stops at the next same-level heading
        assert!(!content.contains("Beta"));
        assert!(!content.contains("other"));
    }

    #[test]
    fn test_section_by_heading_missing_returns_none() {
        assert!(section_by_heading("# Only\n\ntext\n", "nope").is_none());
    }

    #[test]
    fn test_edit_requires_line_range_args() {
        // The edit contract is now line-range, not old/new.
        let mut reg = ToolRegistry::new();
        let result = reg.call_tool("edit", &json!({"id": "x", "old": "a", "new": "b"}));
        assert!(
            result["error"].as_str().unwrap().contains("start_line"),
            "expected start_line requirement, got: {result}"
        );
    }
}
