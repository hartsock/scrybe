// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `list_tabs` — the live set of tabs open in the running editor, over the
//! socket (#46). `Editor` facet, non-mutating. It needs a live app: with none
//! running it reports a *business* `tool_error` (`no_live_app`), not an engine
//! fault — the tool ran and told the agent "there's no editor to ask".

use serde_json::{json, Value};

use crate::{Ctx, DataSchema, Facet, ToolError, ToolOutcome, ToolSpec, TransportError};

/// Version of this tool's `data` payload.
const DATA_VERSION: u32 = 1;

/// The `list_tabs` tool spec.
pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "list_tabs",
        description: "List the tabs currently open in the running Scrybe editor — \
            each with its path, title, dirty flag, view mode, and whether it is the \
            active tab. Requires a live app; with none running it reports \
            `no_live_app`. Read the `data.tabs` array.",
        input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: data_schema,
        },
        mutates: false,
        facet: Facet::Editor,
        handler,
    }
}

fn input_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn data_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "v": { "const": DATA_VERSION },
            "kind": { "const": "list_tabs" },
            "tabs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "title": { "type": "string" },
                        "is_dirty": { "type": "boolean" },
                        "view_mode": { "type": "string" },
                        "active": { "type": "boolean" }
                    }
                }
            },
            "count": { "type": "integer" }
        },
        "required": ["v", "kind", "tabs", "count"]
    })
}

fn handler(ctx: &Ctx, _args: &Value) -> ToolOutcome {
    let empty = || json!({ "v": DATA_VERSION, "kind": "list_tabs", "tabs": [], "count": 0 });
    match ctx.transport.call("list_tabs", json!({})) {
        // Validate the reply against the typed contract, then re-emit it.
        Ok(value) => match serde_json::from_value::<scrybe_rpc::ListTabsResult>(value) {
            Ok(res) => ToolOutcome::ok(json!({
                "v": DATA_VERSION,
                "kind": "list_tabs",
                "count": res.tabs.len(),
                "tabs": res.tabs,
            })),
            Err(e) => ToolOutcome::fail(
                empty(),
                ToolError::new("bad_reply", format!("malformed list_tabs reply: {e}")),
            ),
        },
        Err(TransportError::NoApp) => ToolOutcome::fail(
            empty(),
            ToolError::new("no_live_app", "no Scrybe app is running to list tabs"),
        ),
        Err(TransportError::Io(msg)) => {
            ToolOutcome::fail(empty(), ToolError::new("transport_error", msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry, Transport};

    /// A transport that returns a canned reply — lets us drive the tool's logic
    /// without a running app.
    struct MockTabs(Value);
    impl Transport for MockTabs {
        fn call(&self, _method: &str, _params: Value) -> Result<Value, TransportError> {
            Ok(self.0.clone())
        }
        fn is_live(&self) -> bool {
            true
        }
    }

    fn call_with(t: Box<dyn Transport>) -> ToolOutcome {
        Registry::default()
            .call("list_tabs", &Ctx::with_transport(t), &json!({}))
            .expect("dispatch")
    }

    #[test]
    fn returns_tabs_from_the_live_app() {
        let reply = json!({ "tabs": [
            { "path": "/a.md", "title": "a.md", "is_dirty": false, "view_mode": "both",    "active": true  },
            { "path": "/b.md", "title": "b.md", "is_dirty": true,  "view_mode": "preview", "active": false }
        ]});
        let out = call_with(Box::new(MockTabs(reply)));
        assert!(out.is_ok());
        assert_eq!(out.data["kind"], "list_tabs");
        assert_eq!(out.data["count"], 2);
        assert_eq!(out.data["tabs"][0]["active"], true);
        assert_eq!(out.data["tabs"][1]["is_dirty"], true);
    }

    #[test]
    fn malformed_reply_is_a_business_failure() {
        let out = call_with(Box::new(MockTabs(json!({ "wrong": "shape" }))));
        assert!(!out.is_ok());
        assert_eq!(out.tool_error.as_ref().unwrap().code, "bad_reply");
    }

    #[test]
    fn no_live_app_is_a_business_failure_not_an_engine_fault() {
        // Headless transport → NoApp → tool_error, isError stays false.
        let out = Registry::default()
            .call("list_tabs", &Ctx::headless(), &json!({}))
            .expect("dispatch");
        assert!(!out.is_ok());
        assert_eq!(out.tool_error.as_ref().unwrap().code, "no_live_app");
        assert_eq!(out.data["count"], 0);
    }
}
