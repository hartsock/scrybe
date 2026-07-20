// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! UI-parity + app-control tools — `state`, `set_theme`, `view_mode`,
//! `set_vim`, `logs`, `quit`, `close_tab` — dispatched to the live app over
//! typed socket methods (workstream A2). These replace the legacy MCP
//! handlers' `/tmp` signal files and `pkill` fallback: every control now
//! drives the same code path as the human toolbar and reports what actually
//! happened. With no live app they return a business `no_live_app`
//! tool_error, never an engine fault.

use serde_json::{json, Value};

use super::editor::{dispatch, str_arg};
use crate::{DataSchema, Facet, ToolSpec};

const DATA_VERSION: u32 = 1;

/// All UI-parity tools, in one call for registration.
pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![
        state_spec(),
        set_theme_spec(),
        view_mode_spec(),
        set_vim_spec(),
        logs_spec(),
        quit_spec(),
        close_tab_spec(),
    ]
}

fn object_schema() -> DataSchema {
    DataSchema {
        version: DATA_VERSION,
        schema: || json!({ "type": "object" }),
    }
}

// ── state ────────────────────────────────────────────────────────────────────

fn state_spec() -> ToolSpec {
    ToolSpec {
        name: "state",
        description: "Report the running Scrybe app's current UI state: the \
            active tab's path/title/dirty flag, view mode, theme, Vim and wrap \
            toggles, and every open path. Served live from the frontend — the \
            human equivalents are the path bar, tab mode icon, theme dropdown, \
            and Vim toggle.",
        input_schema: || json!({ "type": "object", "properties": {} }),
        data_schema: object_schema(),
        mutates: false,
        facet: Facet::UiParity,
        handler: |ctx, _args| dispatch(ctx, "state", "state", json!({})),
    }
}

// ── set_theme ────────────────────────────────────────────────────────────────

fn set_theme_spec() -> ToolSpec {
    ToolSpec {
        name: "set_theme",
        description: "Set the editor + preview theme in the running Scrybe app \
            (same entry point as the toolbar dropdown). Returns the applied \
            theme.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "theme": { "type": "string", "enum": ["default", "dark", "solarized"] }
                },
                "required": ["theme"]
            })
        },
        data_schema: object_schema(),
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "set_theme",
                "set_theme",
                json!({ "theme": str_arg(args, "theme") }),
            )
        },
    }
}

// ── view_mode ────────────────────────────────────────────────────────────────

fn view_mode_spec() -> ToolSpec {
    ToolSpec {
        name: "view_mode",
        description: "Set the active tab's view mode in the running Scrybe app \
            — `both`, `edit`, `preview`, or `cycle` to advance both→edit→preview \
            (same entry point as the toolbar View button). Returns the CONCRETE \
            mode now active (a code/text tab pins to `edit`).",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["both", "edit", "preview", "cycle"] }
                },
                "required": ["mode"]
            })
        },
        data_schema: object_schema(),
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "view_mode",
                "view_mode",
                json!({ "mode": str_arg(args, "mode") }),
            )
        },
    }
}

// ── set_vim ──────────────────────────────────────────────────────────────────

fn set_vim_spec() -> ToolSpec {
    ToolSpec {
        name: "set_vim",
        description: "Enable or disable Vim keybindings in the running Scrybe \
            editor (same entry point as the toolbar Vim toggle). Returns the \
            applied setting.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } },
                "required": ["enabled"]
            })
        },
        data_schema: object_schema(),
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            dispatch(ctx, "set_vim", "set_vim", json!({ "enabled": enabled }))
        },
    }
}

// ── logs ─────────────────────────────────────────────────────────────────────

fn logs_spec() -> ToolSpec {
    ToolSpec {
        name: "logs",
        description: "Read recent console output from the running Scrybe app \
            (errors, warnings, info) — newest last, up to `tail` lines (default \
            50). Served from the app's in-memory ring; nothing is written to \
            disk.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "tail": { "type": "integer", "minimum": 1, "maximum": 500 }
                }
            })
        },
        data_schema: object_schema(),
        mutates: false,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            let tail = args.get("tail").and_then(Value::as_u64);
            dispatch(ctx, "logs", "logs", json!({ "tail": tail }))
        },
    }
}

// ── quit ─────────────────────────────────────────────────────────────────────

fn quit_spec() -> ToolSpec {
    ToolSpec {
        name: "quit",
        description: "Gracefully close the running Scrybe app via the socket. \
            The app runs its dirty-buffer checks and may refuse (unsaved \
            edits); pass `force: true` to discard them. Never signals or kills \
            processes.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "force": { "type": "boolean", "description": "Quit even with unsaved edits." }
                }
            })
        },
        data_schema: object_schema(),
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
            dispatch(ctx, "quit", "quit", json!({ "force": force }))
        },
    }
}

// ── close_tab ────────────────────────────────────────────────────────────────

fn close_tab_spec() -> ToolSpec {
    ToolSpec {
        name: "close_tab",
        description: "Close an open tab in the running Scrybe app by its \
            canonical `path`. (Closing the active tab without naming it was a \
            legacy /tmp-signal behavior and is retired — name the path.)",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            })
        },
        data_schema: object_schema(),
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "close",
                "close_tab",
                json!({ "path": str_arg(args, "path") }),
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry, Transport, TransportError};

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
    fn all_ui_tools_registered_and_no_app_is_business_failure() {
        let reg = Registry::default();
        for name in [
            "state",
            "set_theme",
            "view_mode",
            "set_vim",
            "logs",
            "quit",
            "close_tab",
        ] {
            assert!(reg.get(name).is_some(), "missing tool: {name}");
        }
        let out = reg.call("state", &Ctx::headless(), &json!({})).unwrap();
        assert!(!out.is_ok());
        assert_eq!(out.tool_error.unwrap().code, "no_live_app");
    }

    #[test]
    fn set_theme_forwards_theme_and_wraps_reply() {
        let spy = std::sync::Arc::new(Spy {
            reply: json!({ "theme": "dark" }),
            seen: std::sync::Mutex::new(None),
        });
        struct Fwd(std::sync::Arc<Spy>);
        impl Transport for Fwd {
            fn call(&self, m: &str, p: Value) -> Result<Value, TransportError> {
                self.0.call(m, p)
            }
            fn is_live(&self) -> bool {
                true
            }
        }
        let reg = Registry::default();
        let out = reg
            .call(
                "set_theme",
                &Ctx::with_transport(Box::new(Fwd(spy.clone()))),
                &json!({ "theme": "dark" }),
            )
            .unwrap();
        assert!(out.is_ok());
        assert_eq!(out.data["kind"], "set_theme");
        assert_eq!(out.data["theme"], "dark");
        let (method, params) = spy.seen.lock().unwrap().clone().unwrap();
        assert_eq!(method, "set_theme");
        assert_eq!(params["theme"], "dark");
    }

    #[test]
    fn close_tab_dispatches_the_socket_close_method() {
        let spy = std::sync::Arc::new(Spy {
            reply: json!({ "applied": true }),
            seen: std::sync::Mutex::new(None),
        });
        struct Fwd(std::sync::Arc<Spy>);
        impl Transport for Fwd {
            fn call(&self, m: &str, p: Value) -> Result<Value, TransportError> {
                self.0.call(m, p)
            }
            fn is_live(&self) -> bool {
                true
            }
        }
        let reg = Registry::default();
        let out = reg
            .call(
                "close_tab",
                &Ctx::with_transport(Box::new(Fwd(spy.clone()))),
                &json!({ "path": "/a.md" }),
            )
            .unwrap();
        assert!(out.is_ok());
        let (method, params) = spy.seen.lock().unwrap().clone().unwrap();
        assert_eq!(method, "close", "close_tab rides the socket `close` method");
        assert_eq!(params["path"], "/a.md");
    }

    #[test]
    fn mutability_flags_are_honest() {
        let reg = Registry::default();
        for (name, mutates) in [
            ("state", false),
            ("logs", false),
            ("set_theme", true),
            ("view_mode", true),
            ("set_vim", true),
            ("quit", true),
            ("close_tab", true),
        ] {
            assert_eq!(reg.get(name).unwrap().mutates, mutates, "tool {name}");
        }
    }
}
