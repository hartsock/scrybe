// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Tool registry for the Scrybe MCP server.
//!
//! Exposes 9 tools to MCP clients: open, read, section, edit, find,
//! render, embed, extract, lint.

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
];

/// Path shared between the Tauri app's `log_append` command and this tool.
const LOG_FILE: &str = "/tmp/scrybe-debug.log";

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
                "description": "Return a heading section by H-level and 0-based index.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "level": {"type": "integer", "minimum": 1, "maximum": 6},
                        "index": {"type": "integer", "minimum": 0}
                    },
                    "required": ["id", "level", "index"]
                }
            },
            {
                "name": "edit",
                "description": "Replace first occurrence of old_text with new_text in a document.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "old": {"type": "string"},
                        "new": {"type": "string"}
                    },
                    "required": ["id", "old", "new"]
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
        // Launch the GUI with the file path (best-effort; don't fail the MCP call if GUI unavailable).
        if let Ok(binary) = which_scrybe_app() {
            let _ = std::process::Command::new(&binary).arg(&path).spawn();
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => return json!({"error": e.to_string()}),
        };
        let doc = Document::from_file(path.clone(), source);
        let doc_id = self.workspace.open(doc);
        let id_str = format!("{doc_id:?}");
        self.id_map.insert(id_str.clone(), doc_id);
        json!({"id": id_str, "path": path.display().to_string()})
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

    fn tool_read(&self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        match self.id_map.get(id_str) {
            Some(doc_id) => match self.workspace.get(doc_id) {
                Some(doc) => json!({"source": doc.source}),
                None => json!({"error": "document not found"}),
            },
            None => json!({"error": format!("unknown id: {id_str}")}),
        }
    }

    fn tool_section(&self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let level = match args["level"].as_u64() {
            Some(l) if l >= 1 && l <= 6 => l as u8,
            _ => return json!({"error": "level must be 1-6"}),
        };
        let index = match args["index"].as_u64() {
            Some(i) => i as usize,
            None => return json!({"error": "index required"}),
        };

        let doc_id = match self.id_map.get(id_str) {
            Some(id) => id,
            None => return json!({"error": format!("unknown id: {id_str}")}),
        };
        let doc = match self.workspace.get(doc_id) {
            Some(d) => d,
            None => return json!({"error": "document not found"}),
        };

        let ast = doc.ast();
        let mut count = 0usize;
        for node in &ast.nodes {
            if let Node::Heading { level: l, children } = node {
                if *l == level {
                    if count == index {
                        let title = children_text(children);
                        return json!({"level": level, "index": index, "title": title});
                    }
                    count += 1;
                }
            }
        }
        json!({"error": format!("no H{level} at index {index}")})
    }

    fn tool_edit(&mut self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let old = match args["old"].as_str() {
            Some(s) if !s.is_empty() => s,
            _ => return json!({"error": "old required and must be non-empty"}),
        };
        let new_text = match args["new"].as_str() {
            Some(s) => s,
            None => return json!({"error": "new required"}),
        };

        let doc_id = match self.id_map.get(id_str) {
            Some(id) => *id,
            None => return json!({"error": format!("unknown id: {id_str}")}),
        };
        let doc = match self.workspace.get_mut(&doc_id) {
            Some(d) => d,
            None => return json!({"error": "document not found"}),
        };

        if doc.source.contains(old) {
            doc.source = doc.source.replacen(old, new_text, 1);
            json!({"replaced": true, "old": old, "new": new_text})
        } else {
            json!({"replaced": false, "old": old, "new": new_text, "note": "old_text not found"})
        }
    }

    fn tool_find(&self, args: &Value) -> Value {
        let id_str = match args["id"].as_str() {
            Some(s) => s,
            None => return json!({"error": "id required"}),
        };
        let query = match args["query"].as_str() {
            Some(q) if !q.is_empty() => q,
            _ => return json!({"error": "query required and must be non-empty"}),
        };

        let doc_id = match self.id_map.get(id_str) {
            Some(id) => id,
            None => return json!({"error": format!("unknown id: {id_str}")}),
        };
        let doc = match self.workspace.get(doc_id) {
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

        json!({"query": query, "matches": matches})
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
        let payload = args["path"].as_str().unwrap_or("").to_string();
        // Signal the running app via a temporary file that the frontend polls,
        // reusing the same pkill+signal pattern used by quit but for a tab event.
        // Write the path to a well-known file; main.ts polls it via scrybe://close.
        let signal_path = "/tmp/scrybe-close-tab.txt";
        match std::fs::write(signal_path, &payload) {
            Ok(_) => json!({"ok": true, "path": payload}),
            Err(e) => json!({"error": e.to_string()}),
        }
    }

    fn tool_quit(&self) -> Value {
        let result = std::process::Command::new("pkill")
            .args(["-TERM", "-f", "scrybe-app"])
            .output();
        match result {
            Ok(out) if out.status.success() || out.status.code() == Some(1) => {
                // pkill exits 1 when no process matched (already closed) — treat as ok
                json!({"ok": true})
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

fn children_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(s) => out.push_str(s),
            Node::InlineCode { content } => out.push_str(content),
            Node::Emphasis { children }
            | Node::Strong { children }
            | Node::Link { children, .. } => out.push_str(&children_text(children)),
            _ => {}
        }
    }
    out
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
    fn test_list_tools_returns_nine() {
        let reg = ToolRegistry::new();
        let tools = reg.list_tools_json();
        let arr = tools["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 13);
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
}
