// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `mermaid_to_png` — render a Mermaid diagram to PNG and embed its source
//! (per-artifact UUID + SHA256) in the PNG's iTXt metadata, so the diagram is
//! losslessly round-trippable. Design §7 / #119. Mutates (writes a file);
//! `Mermaid` facet.
//!
//! Rendering uses the adopted `mermaid-rs-renderer` (pure Rust, #132) — no `mmdc`.

use scrybe_mermaid_render::{render_png, source_sha256};
use serde_json::{json, Value};

use crate::{Ctx, DataSchema, Facet, ToolError, ToolOutcome, ToolSpec};

/// Version of this tool's `data` payload.
const DATA_VERSION: u32 = 1;

/// The `mermaid_to_png` tool spec.
pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "mermaid_to_png",
        description: "Render a Mermaid diagram to PNG AND embed the source in the \
             PNG's iTXt metadata (a per-artifact UUID + the source's SHA-256). \
             ALWAYS use this instead of calling `mmdc` directly — raw `mmdc` skips \
             the embedding and breaks lossless round-trips and document publishing. \
             Rendering is pure Rust (no browser). Input: `source` (Mermaid text) + \
             `output_path`. Returns `{ png_path, uuid, sha256, bytes }`; recover the \
             source later with `extract`.",
        input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: data_schema,
        },
        mutates: true,
        facet: Facet::Mermaid,
        handler,
    }
}

fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "source": { "type": "string", "description": "Mermaid diagram source." },
            "output_path": { "type": "string", "description": "Where to write the PNG." }
        },
        "required": ["source", "output_path"]
    })
}

fn data_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "v": { "const": DATA_VERSION },
            "kind": { "const": "mermaid_to_png" },
            "png_path": { "type": "string" },
            "uuid": { "type": "string", "description": "Per-artifact id embedded in the PNG." },
            "sha256": { "type": "string", "description": "SHA-256 of the Mermaid source." },
            "bytes": { "type": "integer" }
        },
        "required": ["v", "kind", "png_path", "uuid", "sha256", "bytes"]
    })
}

fn handler(_ctx: &Ctx, args: &Value) -> ToolOutcome {
    // Required args are gated by the dispatcher.
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let output_path = args
        .get("output_path")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let base = json!({ "v": DATA_VERSION, "kind": "mermaid_to_png", "png_path": output_path });

    // 1. Render Mermaid -> PNG bytes (an invalid diagram is a business failure).
    let png = match render_png(source) {
        Ok(bytes) => bytes,
        Err(e) => {
            return ToolOutcome::fail(
                base,
                ToolError::new("render_failed", format!("could not render diagram: {e}")),
            )
        }
    };

    // 2. Mint a uuid and embed source + uuid + sha256 into the PNG's iTXt.
    let uuid = new_uuid();
    let embedded = match scrybe_mermaid::embed_with_uuid(&png, source, &uuid) {
        Ok(bytes) => bytes,
        Err(e) => {
            return ToolOutcome::fail(
                base,
                ToolError::new("embed_failed", format!("could not embed source: {e}")),
            )
        }
    };

    // 3. Write the PNG.
    if let Err(e) = std::fs::write(output_path, &embedded) {
        return ToolOutcome::fail(
            base,
            ToolError::new(
                "write_failed",
                format!("could not write {output_path}: {e}"),
            ),
        );
    }

    ToolOutcome::ok(json!({
        "v": DATA_VERSION,
        "kind": "mermaid_to_png",
        "png_path": output_path,
        "uuid": uuid,
        "sha256": source_sha256(source),
        "bytes": embedded.len(),
    }))
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A unique PNG path in the platform temp dir (never a hardcoded `/tmp`).
    fn temp_png() -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "scrybe-tools-mermaid-test-{}-{}.png",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        p
    }

    #[test]
    fn renders_and_embeds_source_round_trips() {
        let out = temp_png();
        let source = "graph TD; A[Start]-->B[End]";
        let outcome = Registry::default()
            .call(
                "mermaid_to_png",
                &Ctx::headless(),
                &json!({ "source": source, "output_path": out.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(outcome.is_ok(), "tool_error: {:?}", outcome.tool_error);

        let data = &outcome.data;
        assert_eq!(data["kind"], "mermaid_to_png");
        assert!(data["bytes"].as_u64().unwrap() > 100);
        assert_eq!(data["sha256"], source_sha256(source));
        let uuid = data["uuid"].as_str().unwrap();
        assert!(uuid::Uuid::parse_str(uuid).is_ok(), "uuid should parse");

        // The written file is a real PNG whose embedded source round-trips.
        let bytes = std::fs::read(&out).expect("png written");
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "real PNG signature"
        );
        let payload = scrybe_mermaid::extract(&bytes).expect("extract embedded");
        assert_eq!(payload.source, source, "source round-trips out of the PNG");
        assert_eq!(payload.uuid, uuid, "the returned uuid is the embedded one");

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn missing_output_path_is_engine_fault() {
        let err = Registry::default()
            .call(
                "mermaid_to_png",
                &Ctx::headless(),
                &json!({ "source": "graph TD; A-->B" }),
            )
            .unwrap_err();
        assert!(
            matches!(err, crate::EngineFault::BadArgs(ref m) if m.contains("output_path")),
            "expected BadArgs for missing output_path, got {err:?}"
        );
    }

    #[test]
    fn spec_is_mutating_mermaid_facet() {
        let s = spec();
        assert_eq!(s.name, "mermaid_to_png");
        assert!(s.mutates);
        assert_eq!(s.facet, Facet::Mermaid);
    }
}
