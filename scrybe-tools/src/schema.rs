// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The shared JSON-Schema envelope for every tool's `data` payload (A4).
//!
//! Every tool's `data` object carries the same envelope keys added by
//! dispatch — `v` (payload version), `kind` (the tool name), and, on a
//! business failure, `tool_error {code, message}`. [`envelope`] wraps a
//! tool's payload properties in that envelope exactly once, so the 22 tool
//! schemas don't hand-copy it (and can't drift).
//!
//! Semantics: the payload's `required` keys describe the SUCCESS shape —
//! the object a caller gets when the tool did its job. A business-failure
//! outcome carries only the envelope (`v`, `kind`, sometimes extra
//! diagnostic keys) plus `tool_error`. At the MCP boundary a failed
//! invocation is surfaced as an `isError: true` result whose
//! `structuredContent` is the error object — so the schema served as MCP
//! `outputSchema` is honest for every successful `structuredContent`.

use serde_json::{json, Map, Value};

/// Wrap a tool's payload schema in the shared data envelope.
///
/// * `kind` — the tool name, pinned via `const` (the payload discriminator).
/// * `version` — the tool's `DATA_VERSION`, pinned via `const`.
/// * `properties` — the payload's own properties (a JSON object of
///   JSON-Schema property definitions).
/// * `required` — the payload keys required on a SUCCESS outcome; `v` and
///   `kind` are always added. `tool_error` is never required — it appears
///   only on business failures.
pub fn envelope(kind: &str, version: u32, properties: Value, required: &[&str]) -> Value {
    let mut props = Map::new();
    props.insert(
        "v".into(),
        json!({ "const": version, "description": "Payload schema version." }),
    );
    props.insert(
        "kind".into(),
        json!({ "const": kind, "description": "Payload discriminator — the tool name." }),
    );
    props.insert(
        "tool_error".into(),
        json!({
            "type": "object",
            "description": "Business failure — the tool ran and said \"no\". Absent on \
                success. (At the MCP boundary a business failure surfaces as an \
                isError:true result instead of riding inside a success payload.)",
            "properties": {
                "code": { "type": "string", "description": "Stable machine code, e.g. no_live_app." },
                "message": { "type": "string" }
            },
            "required": ["code", "message"]
        }),
    );
    if let Value::Object(payload) = properties {
        for (key, prop) in payload {
            props.insert(key, prop);
        }
    }
    let mut req: Vec<Value> = vec![json!("v"), json!("kind")];
    req.extend(required.iter().map(|k| json!(k)));
    json!({ "type": "object", "properties": props, "required": req })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_adds_v_kind_and_optional_tool_error() {
        let s = envelope(
            "demo",
            3,
            json!({ "answer": { "type": "integer" } }),
            &["answer"],
        );
        assert_eq!(s["type"], "object");
        assert_eq!(s["properties"]["v"]["const"], 3);
        assert_eq!(s["properties"]["kind"]["const"], "demo");
        assert_eq!(s["properties"]["answer"]["type"], "integer");
        // tool_error is described but never required.
        assert_eq!(s["properties"]["tool_error"]["type"], "object");
        let required: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, ["v", "kind", "answer"]);
    }

    #[test]
    fn envelope_with_no_payload_requires_only_v_and_kind() {
        let s = envelope("bare", 1, json!({}), &[]);
        let required: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(required, ["v", "kind"]);
    }
}
