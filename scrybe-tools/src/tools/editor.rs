// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The stateful editor tools — `open`, `read`, `find`, `section`, `edit` —
//! dispatched through the transport to the *live app* over `~/.scrybe/sock`
//! (#122 Phase 2). These are **path-based** (the socket/CLI contract), unifying
//! the MCP surface off the legacy `DocumentId`/shadow-`Workspace` path onto the
//! one source of truth. With no live app they return a business `tool_error`
//! (`no_live_app`), never an engine fault.

use serde_json::{json, Value};

use crate::{Ctx, DataSchema, Facet, ToolError, ToolOutcome, ToolSpec, TransportError};

const DATA_VERSION: u32 = 1;

/// All editor tools, in one call for registration.
pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![
        open_spec(),
        read_spec(),
        find_spec(),
        section_spec(),
        edit_spec(),
        reload_spec(),
    ]
}

/// Round-trip a socket `method` and wrap its result under `data` (kind = the
/// tool name). A live-app failure becomes a business `tool_error`.
fn dispatch(ctx: &Ctx, method: &str, kind: &'static str, params: Value) -> ToolOutcome {
    let base = || json!({ "v": DATA_VERSION, "kind": kind });
    match ctx.transport.call(method, params) {
        Ok(value) => {
            let mut data = base();
            if let (Some(obj), Some(res)) = (data.as_object_mut(), value.as_object()) {
                for (k, v) in res {
                    obj.insert(k.clone(), v.clone());
                }
            }
            ToolOutcome::ok(data)
        }
        Err(TransportError::NoApp) => ToolOutcome::fail(
            base(),
            ToolError::new(
                "no_live_app",
                format!("no Scrybe app is running for `{method}`"),
            ),
        ),
        Err(TransportError::Io(msg)) => ToolOutcome::fail(base(), ToolError::new("app_error", msg)),
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or_default()
}

// ── open ────────────────────────────────────────────────────────────────────

fn open_spec() -> ToolSpec {
    ToolSpec {
        name: "open",
        description: "Open (or refresh) a file as a tab in the running Scrybe \
            editor. The tab is addressed by its canonical `path` thereafter. \
            Requires a live app. Returns `{ tab_id, reloaded }`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "File to open." } },
                "required": ["path"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || json!({ "type": "object" }),
        },
        mutates: true,
        facet: Facet::Core,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "open",
                "open",
                json!({ "path": str_arg(args, "path") }),
            )
        },
    }
}

// ── read ────────────────────────────────────────────────────────────────────

fn read_spec() -> ToolSpec {
    ToolSpec {
        name: "read",
        description: "Read the in-memory buffer of an open tab (sees unsaved \
            edits, unlike reading the file from disk). Address the tab by `path`. \
            Returns `{ path, content, is_dirty }`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || json!({ "type": "object" }),
        },
        mutates: false,
        facet: Facet::Core,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "read",
                "read",
                json!({ "path": str_arg(args, "path") }),
            )
        },
    }
}

// ── find ────────────────────────────────────────────────────────────────────

fn find_spec() -> ToolSpec {
    ToolSpec {
        name: "find",
        description: "Search open tabs (or named `paths`, falling back to disk) \
            for a regex `pattern` (or a `literal` string). Returns `{ hits: [{ \
            path, line, column, text }] }`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "paths": { "type": "array", "items": { "type": "string" } },
                    "literal": { "type": "boolean" },
                    "case_sensitive": { "type": "boolean" }
                },
                "required": ["pattern"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || json!({ "type": "object" }),
        },
        mutates: false,
        facet: Facet::Editor,
        handler: |ctx, args| {
            let params = json!({
                "pattern": str_arg(args, "pattern"),
                "paths": args.get("paths").cloned().unwrap_or_else(|| json!([])),
                "literal": args.get("literal").and_then(Value::as_bool).unwrap_or(false),
                "case_sensitive": args.get("case_sensitive").and_then(Value::as_bool).unwrap_or(false),
            });
            dispatch(ctx, "find", "find", params)
        },
    }
}

// ── section ───────────────────────────────────────────────────────────────────

fn section_spec() -> ToolSpec {
    ToolSpec {
        name: "section",
        description: "Extract a section of an open tab by `heading` \
            (case-insensitive substring). Address the tab by `path`. Returns \
            `{ heading, level, content }`.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" }, "heading": { "type": "string" } },
                "required": ["path", "heading"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || json!({ "type": "object" }),
        },
        mutates: false,
        facet: Facet::Editor,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "section",
                "section",
                json!({ "path": str_arg(args, "path"), "heading": str_arg(args, "heading") }),
            )
        },
    }
}

// ── edit ──────────────────────────────────────────────────────────────────────

fn edit_spec() -> ToolSpec {
    ToolSpec {
        name: "edit",
        description: "Replace lines `start_line..=end_line` (1-indexed) of an open \
            tab's buffer with `content`. Address the tab by `path`. The unified \
            line-range edit (the old MCP `{old, new}` replace is dropped).",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "content": { "type": "string" }
                },
                "required": ["path", "start_line", "end_line", "content"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || json!({ "type": "object" }),
        },
        mutates: true,
        facet: Facet::Editor,
        handler: |ctx, args| {
            let params = json!({
                "path": str_arg(args, "path"),
                "start_line": args.get("start_line").cloned().unwrap_or(json!(0)),
                "end_line": args.get("end_line").cloned().unwrap_or(json!(0)),
                "content": str_arg(args, "content"),
            });
            dispatch(ctx, "edit", "edit", params)
        },
    }
}

// ── reload ────────────────────────────────────────────────────────────────────

fn reload_spec() -> ToolSpec {
    ToolSpec {
        name: "reload",
        description: "Re-read an open tab from disk into its live buffer (picks up \
            external edits). Address the tab by `path`; pass `force: true` to \
            discard unsaved edits. Returns `{ path, bytes, was_dirty }`. Requires a \
            live app; a first-class socket op, replacing the old /tmp poke.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "force": { "type": "boolean", "description": "Discard unsaved edits." }
                },
                "required": ["path"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || json!({ "type": "object" }),
        },
        mutates: true,
        facet: Facet::Editor,
        handler: |ctx, args| {
            let params = json!({
                "path": str_arg(args, "path"),
                "force": args.get("force").and_then(Value::as_bool).unwrap_or(false),
            });
            dispatch(ctx, "reload", "reload", params)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry, Transport};

    /// Records the method+params it was called with and returns a canned reply.
    struct Spy {
        reply: Value,
        seen: std::sync::Mutex<Option<(String, Value)>>,
    }
    impl Transport for Spy {
        fn call(&self, method: &str, params: Value) -> Result<Value, TransportError> {
            *self.seen.lock().unwrap() = Some((method.to_string(), params));
            Ok(self.reply.clone())
        }
        fn is_live(&self) -> bool {
            true
        }
    }

    #[test]
    fn read_forwards_path_and_wraps_result() {
        let spy = std::sync::Arc::new(Spy {
            reply: json!({ "path": "/a.md", "content": "hi", "is_dirty": false }),
            seen: std::sync::Mutex::new(None),
        });
        let reg = Registry::default();
        // A thin Transport that delegates to the shared spy.
        struct Fwd(std::sync::Arc<Spy>);
        impl Transport for Fwd {
            fn call(&self, m: &str, p: Value) -> Result<Value, TransportError> {
                self.0.call(m, p)
            }
            fn is_live(&self) -> bool {
                true
            }
        }
        let out = reg
            .call(
                "read",
                &Ctx::with_transport(Box::new(Fwd(spy.clone()))),
                &json!({ "path": "/a.md" }),
            )
            .unwrap();
        assert!(out.is_ok());
        assert_eq!(out.data["kind"], "read");
        assert_eq!(out.data["content"], "hi");
        let (method, params) = spy.seen.lock().unwrap().clone().unwrap();
        assert_eq!(method, "read");
        assert_eq!(params["path"], "/a.md");
    }

    #[test]
    fn all_editor_tools_are_registered_and_no_app_is_a_business_failure() {
        let reg = Registry::default();
        for name in ["open", "read", "find", "section", "edit", "reload"] {
            assert!(reg.get(name).is_some(), "missing tool: {name}");
        }
        // No live app → business tool_error, never an engine fault.
        let out = reg
            .call("read", &Ctx::headless(), &json!({ "path": "/nope.md" }))
            .unwrap();
        assert!(!out.is_ok());
        assert_eq!(out.tool_error.unwrap().code, "no_live_app");
    }
}
