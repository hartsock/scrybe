// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! UI-parity + app-control tools — `state`, `set_theme`, `view_mode`,
//! `set_vim`, `set_language`, `set_explorer`, `logs`, `quit`, `close_tab` — dispatched to the live app over
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
        set_language_spec(),
        set_explorer_spec(),
        logs_spec(),
        quit_spec(),
        close_tab_spec(),
    ]
}

/// `quit` and `close_tab` share the socket's fire-and-forget ack shape.
fn ack_schema(kind: &'static str) -> Value {
    crate::schema::envelope(
        kind,
        DATA_VERSION,
        json!({
            "applied": { "type": "boolean", "description": "False when the command was a no-op (e.g. the path was not an open tab)." }
        }),
        &["applied"],
    )
}

// ── state ────────────────────────────────────────────────────────────────────

fn state_spec() -> ToolSpec {
    ToolSpec {
        name: "state",
        description: "Report the running Scrybe app's current UI state: the \
            active tab's path/title/dirty flag, view mode, document kind and \
            language, theme, \
            Vim and wrap toggles, whether the Explorer sidebar is showing, and \
            every open path. `vim` and `wrap` are the \
            values in effect for the active tab (both false on a `wysiwyg` tab, \
            whose editor supports neither). Served live from the frontend — the \
            human equivalents are the path bar, tab mode icon, theme dropdown, \
            and Vim toggle.",
        input_schema: || json!({ "type": "object", "properties": {} }),
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "state",
                    DATA_VERSION,
                    json!({
                        "active_path": { "type": ["string", "null"], "description": "Canonical path of the active tab (null on the welcome screen)." },
                        "active_title": { "type": ["string", "null"] },
                        "is_dirty": { "type": "boolean" },
                        "view_mode": { "type": "string", "enum": ["both", "edit", "preview", "wysiwyg"] },
                        "doc_kind": { "type": "string", "enum": ["markdown", "code", "text"], "description": "How the active tab's document is treated: Markdown (preview + WYSIWYG available), a recognized source-code language, or plain text (both edit-only)." },
                        "language": { "type": ["string", "null"], "description": "The active tab's language: `Markdown`, a source language such as `Rust` or `C++`, or null for plain text." },
                        "theme": { "type": "string" },
                        "vim": { "type": "boolean" },
                        "wrap": { "type": "boolean" },
                        "explorer": { "type": "boolean", "description": "The Explorer sidebar is showing." },
                        "open_paths": { "type": "array", "items": { "type": "string" } }
                    }),
                    &[
                        "active_path",
                        "active_title",
                        "is_dirty",
                        "view_mode",
                        "doc_kind",
                        "language",
                        "theme",
                        "vim",
                        "wrap",
                        "explorer",
                        "open_paths",
                    ],
                )
            },
        },
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
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "set_theme",
                    DATA_VERSION,
                    json!({
                        "theme": {
                            "type": "string",
                            "enum": ["default", "dark", "solarized"],
                            "description": "The theme now applied."
                        }
                    }),
                    &["theme"],
                )
            },
        },
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
            — `both` (source editor + preview), `edit`, `preview`, `wysiwyg` \
            (one rich-text editing surface for Markdown; no Vim or wrap toggle), \
            or `cycle` to advance both→edit→preview→wysiwyg (same entry point as \
            the toolbar View button). Returns the CONCRETE mode now active (a \
            code/text tab pins to `edit`).",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["both", "edit", "preview", "wysiwyg", "cycle"] }
                },
                "required": ["mode"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "view_mode",
                    DATA_VERSION,
                    json!({
                        "mode": {
                            "type": "string",
                            "enum": ["both", "edit", "preview", "wysiwyg"],
                            "description": "The CONCRETE mode now active (never `cycle`)."
                        }
                    }),
                    &["mode"],
                )
            },
        },
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
            setting actually in effect: on a `wysiwyg` tab, which has no Vim \
            mode, nothing changes and `enabled` is false.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } },
                "required": ["enabled"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "set_vim",
                    DATA_VERSION,
                    json!({
                        "enabled": { "type": "boolean", "description": "The setting now in effect for the active tab." }
                    }),
                    &["enabled"],
                )
            },
        },
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

// ── set_language ─────────────────────────────────────────────────────────────

fn set_language_spec() -> ToolSpec {
    ToolSpec {
        name: "set_language",
        description: "Set the language of the ACTIVE tab in the running Scrybe \
            app (same entry point as the language chip in the path bar): \
            highlighting, indentation, and whether the tab is Markdown (with \
            preview and WYSIWYG) or edit-only code/text. Takes a language name \
            or alias (`Rust`, `python`, `cpp`, `go`, `rb`, `markdown`, `text`) \
            or `auto` to detect again from the file name and `#!` line. \
            Returns the language, document kind, and whether it is detected. \
            An unknown name is an `app_error` and changes nothing.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "language": { "type": "string", "description": "A language name or alias, or `auto`." }
                },
                "required": ["language"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "set_language",
                    DATA_VERSION,
                    json!({
                        "language": { "type": ["string", "null"], "description": "Canonical language name now in effect; null for plain text." },
                        "doc_kind": { "type": "string", "enum": ["markdown", "code", "text"] },
                        "auto": { "type": "boolean", "description": "True when the language is detected rather than set by hand." }
                    }),
                    &["language", "doc_kind", "auto"],
                )
            },
        },
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            dispatch(
                ctx,
                "set_language",
                "set_language",
                json!({ "language": str_arg(args, "language") }),
            )
        },
    }
}

// ── set_explorer ─────────────────────────────────────────────────────────────

fn set_explorer_spec() -> ToolSpec {
    ToolSpec {
        name: "set_explorer",
        description: "Show or hide the Explorer sidebar (file browser, agents \
            and version-control panels) in the running Scrybe app — same entry \
            point as the toolbar ☰ button and View ▸ Show Explorer. Returns \
            whether it is now showing.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "visible": { "type": "boolean" } },
                "required": ["visible"]
            })
        },
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "set_explorer",
                    DATA_VERSION,
                    json!({
                        "visible": { "type": "boolean", "description": "The Explorer is now showing." }
                    }),
                    &["visible"],
                )
            },
        },
        mutates: true,
        facet: Facet::UiParity,
        handler: |ctx, args| {
            let visible = args.get("visible").and_then(Value::as_bool).unwrap_or(true);
            dispatch(
                ctx,
                "set_explorer",
                "set_explorer",
                json!({ "visible": visible }),
            )
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
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || {
                crate::schema::envelope(
                    "logs",
                    DATA_VERSION,
                    json!({
                        "lines": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Recent console lines, newest last."
                        }
                    }),
                    &["lines"],
                )
            },
        },
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
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || ack_schema("quit"),
        },
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
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: || ack_schema("close_tab"),
        },
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
            "set_language",
            "set_explorer",
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
    fn view_mode_offers_wysiwyg_and_forwards_it() {
        let reg = Registry::default();
        let spec = reg.get("view_mode").unwrap();
        let input = (spec.input_schema)();
        let modes = input["properties"]["mode"]["enum"].as_array().unwrap();
        assert!(modes.contains(&json!("wysiwyg")), "input enum: {modes:?}");
        let output = (spec.data_schema.schema)();
        assert!(
            output.to_string().contains("wysiwyg"),
            "the reply schema must admit the wysiwyg mode"
        );

        let spy = std::sync::Arc::new(Spy {
            reply: json!({ "mode": "wysiwyg" }),
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
        let out = reg
            .call(
                "view_mode",
                &Ctx::with_transport(Box::new(Fwd(spy.clone()))),
                &json!({ "mode": "wysiwyg" }),
            )
            .unwrap();
        assert!(out.is_ok());
        assert_eq!(out.data["mode"], "wysiwyg");
        let (method, params) = spy.seen.lock().unwrap().clone().unwrap();
        assert_eq!(method, "view_mode");
        assert_eq!(params["mode"], "wysiwyg");
    }

    #[test]
    fn set_language_forwards_the_name_and_wraps_reply() {
        let spy = std::sync::Arc::new(Spy {
            reply: json!({ "language": "Python", "doc_kind": "code", "auto": false }),
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
                "set_language",
                &Ctx::with_transport(Box::new(Fwd(spy.clone()))),
                &json!({ "language": "py" }),
            )
            .unwrap();
        assert!(out.is_ok());
        assert_eq!(out.data["kind"], "set_language");
        assert_eq!(out.data["language"], "Python");
        assert_eq!(out.data["doc_kind"], "code");
        assert_eq!(out.data["auto"], false);
        let (method, params) = spy.seen.lock().unwrap().clone().unwrap();
        assert_eq!(method, "set_language");
        assert_eq!(params["language"], "py");
    }

    #[test]
    fn set_explorer_forwards_visibility_and_wraps_reply() {
        let spy = std::sync::Arc::new(Spy {
            reply: json!({ "visible": false }),
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
        let out = Registry::default()
            .call(
                "set_explorer",
                &Ctx::with_transport(Box::new(Fwd(spy.clone()))),
                &json!({ "visible": false }),
            )
            .unwrap();
        assert!(out.is_ok());
        assert_eq!(out.data["kind"], "set_explorer");
        assert_eq!(out.data["visible"], false);
        let (method, params) = spy.seen.lock().unwrap().clone().unwrap();
        assert_eq!(method, "set_explorer");
        assert_eq!(params["visible"], false);
    }

    #[test]
    fn set_language_unknown_name_is_a_business_failure() {
        struct Refuse;
        impl Transport for Refuse {
            fn call(&self, _m: &str, _p: Value) -> Result<Value, TransportError> {
                Err(TransportError::Remote(scrybe_rpc::RpcError {
                    code: -32602,
                    message: "unknown language: klingon".into(),
                    data: None,
                }))
            }
            fn is_live(&self) -> bool {
                true
            }
        }
        let out = Registry::default()
            .call(
                "set_language",
                &Ctx::with_transport(Box::new(Refuse)),
                &json!({ "language": "klingon" }),
            )
            .unwrap();
        assert!(!out.is_ok());
        let err = out.tool_error.unwrap();
        assert_eq!(err.code, "app_error");
        assert!(err.message.contains("klingon"));
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
            ("set_language", true),
            ("set_explorer", true),
            ("quit", true),
            ("close_tab", true),
        ] {
            assert_eq!(reg.get(name).unwrap().mutates, mutates, "tool {name}");
        }
    }
}
