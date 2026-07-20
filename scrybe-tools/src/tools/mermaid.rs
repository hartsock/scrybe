// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The Mermaid PNG tools — `mermaid_to_png`, `embed`, `extract`.
//!
//! `mermaid_to_png` renders a Mermaid diagram to PNG and embeds its source
//! (per-artifact UUID + SHA256) in the PNG's iTXt metadata, so the diagram is
//! losslessly round-trippable. Design §7 / #119. `embed` stamps source into an
//! EXISTING PNG in place; `extract` recovers it, verifying the embedded digest
//! by default (the B5 verified-extraction API) — a mismatch is a business
//! `verification_failed` tool_error, never silently-returned tampered source.
//! All three are in-process (no live app needed) and work headless.
//!
//! Rendering uses the adopted `mermaid-rs-renderer` (pure Rust, #132) — no `mmdc`.

use scrybe_mermaid::MermaidError;
use scrybe_mermaid_render::{render_png, source_sha256};
use serde_json::{json, Value};

use crate::{Ctx, DataSchema, Facet, ToolError, ToolOutcome, ToolSpec};

/// Version of these tools' `data` payloads.
const DATA_VERSION: u32 = 1;

/// All Mermaid PNG tools, in one call for registration.
pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![spec(), embed_spec(), extract_spec()]
}

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

// ── embed ────────────────────────────────────────────────────────────────────

/// The `embed` tool spec — stamp Mermaid source into an existing PNG in place.
fn embed_spec() -> ToolSpec {
    ToolSpec {
        name: "embed",
        description: "Embed Mermaid `source` into an EXISTING PNG file's iTXt \
             metadata, in place (a per-artifact UUID + the source's SHA-256 \
             travel with the image). Use this to retrofit a PNG you already \
             have; prefer `mermaid_to_png` to render + embed in one step. \
             In-process — no running app needed. Returns `{ png_path, uuid, \
             sha256, bytes }`; recover the source later with `extract`.",
        input_schema: embed_input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: embed_data_schema,
        },
        mutates: true,
        facet: Facet::Mermaid,
        handler: embed_handler,
    }
}

fn embed_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "png_path": { "type": "string", "description": "PNG file to embed into (rewritten in place)." },
            "source": { "type": "string", "description": "Mermaid diagram source." }
        },
        "required": ["png_path", "source"]
    })
}

fn embed_data_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "v": { "const": DATA_VERSION },
            "kind": { "const": "embed" },
            "png_path": { "type": "string" },
            "uuid": { "type": "string", "description": "Per-artifact id embedded in the PNG." },
            "sha256": { "type": "string", "description": "SHA-256 of the Mermaid source." },
            "bytes": { "type": "integer" }
        },
        "required": ["v", "kind", "png_path", "uuid", "sha256", "bytes"]
    })
}

fn embed_handler(_ctx: &Ctx, args: &Value) -> ToolOutcome {
    // Required args are gated by the dispatcher.
    let png_path = args
        .get("png_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let base = json!({ "v": DATA_VERSION, "kind": "embed", "png_path": png_path });

    let bytes = match std::fs::read(png_path) {
        Ok(b) => b,
        Err(e) => {
            return ToolOutcome::fail(
                base,
                ToolError::new("read_failed", format!("could not read {png_path}: {e}")),
            )
        }
    };

    // Mint the uuid ourselves (embed_with_uuid) so we can report it without
    // re-extracting — same pattern as `mermaid_to_png`.
    let uuid = new_uuid();
    let embedded = match scrybe_mermaid::embed_with_uuid(&bytes, source, &uuid) {
        Ok(out) => out,
        Err(e) => {
            return ToolOutcome::fail(
                base,
                ToolError::new("embed_failed", format!("could not embed source: {e}")),
            )
        }
    };

    if let Err(e) = std::fs::write(png_path, &embedded) {
        return ToolOutcome::fail(
            base,
            ToolError::new("write_failed", format!("could not write {png_path}: {e}")),
        );
    }

    ToolOutcome::ok(json!({
        "v": DATA_VERSION,
        "kind": "embed",
        "png_path": png_path,
        "uuid": uuid,
        "sha256": source_sha256(source),
        "bytes": embedded.len(),
    }))
}

// ── extract ──────────────────────────────────────────────────────────────────

/// The `extract` tool spec — verified extraction (B5 API).
fn extract_spec() -> ToolSpec {
    ToolSpec {
        name: "extract",
        description: "Extract the Mermaid source embedded in a PNG's iTXt \
             metadata, verifying the stored SHA-256 against the extracted \
             source by default. Returns `{ source, uuid, verification }` where \
             `verification` is \"verified\" (digest matched) or \"no-digest\" \
             (the payload stored none — older or foreign embedder, explicitly \
             NOT verified). A digest mismatch is the business tool_error \
             `verification_failed` (tampered source is never returned as good); \
             a PNG with no payload is `no_payload`. In-process — no running \
             app needed.",
        input_schema: extract_input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: extract_data_schema,
        },
        mutates: false,
        facet: Facet::Mermaid,
        handler: extract_handler,
    }
}

fn extract_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "png_path": { "type": "string", "description": "PNG file to extract from." }
        },
        "required": ["png_path"]
    })
}

fn extract_data_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "v": { "const": DATA_VERSION },
            "kind": { "const": "extract" },
            "png_path": { "type": "string" },
            "source": { "type": "string" },
            "uuid": { "type": "string" },
            "sha256": { "type": "string", "description": "Verified digest; empty when the payload stored none." },
            "verification": { "enum": ["verified", "no-digest"] }
        },
        "required": ["v", "kind", "png_path", "source", "uuid", "sha256", "verification"]
    })
}

fn extract_handler(_ctx: &Ctx, args: &Value) -> ToolOutcome {
    let png_path = args
        .get("png_path")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let base = json!({ "v": DATA_VERSION, "kind": "extract", "png_path": png_path });

    let bytes = match std::fs::read(png_path) {
        Ok(b) => b,
        Err(e) => {
            return ToolOutcome::fail(
                base,
                ToolError::new("read_failed", format!("could not read {png_path}: {e}")),
            )
        }
    };

    // Verified extraction (B5): a stored-digest mismatch is an Err — the tool
    // ran and said "no" (business failure), the engine did its job.
    match scrybe_mermaid::extract(&bytes) {
        Ok(payload) => ToolOutcome::ok(json!({
            "v": DATA_VERSION,
            "kind": "extract",
            "png_path": png_path,
            "source": payload.source,
            "uuid": payload.uuid,
            "sha256": payload.sha256().unwrap_or_default(),
            "verification": if payload.is_verified() { "verified" } else { "no-digest" },
        })),
        Err(MermaidError::VerificationFailed {
            algorithm,
            expected,
            actual,
        }) => {
            let mut data = base;
            if let Some(obj) = data.as_object_mut() {
                obj.insert("expected".into(), json!(expected));
                obj.insert("actual".into(), json!(actual));
            }
            ToolOutcome::fail(
                data,
                ToolError::new(
                    "verification_failed",
                    format!(
                        "{algorithm} verification failed: stored digest {expected} \
                         does not match computed digest {actual}"
                    ),
                ),
            )
        }
        // Missing chunk / invalid PNG / malformed payload: the file simply
        // carries no (readable) Scrybe payload — likewise a business failure.
        Err(e) => ToolOutcome::fail(
            base,
            ToolError::new("no_payload", format!("no embedded Mermaid payload: {e}")),
        ),
    }
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

    // ── embed / extract (A2a: in-process shared tools) ──────────────────────

    #[test]
    fn embed_and_extract_are_registered_with_honest_flags() {
        let reg = Registry::default();
        let embed = reg.get("embed").expect("embed registered");
        assert!(embed.mutates, "embed rewrites the PNG");
        assert_eq!(embed.facet, Facet::Mermaid);
        let extract = reg.get("extract").expect("extract registered");
        assert!(!extract.mutates, "extract only reads");
        assert_eq!(extract.facet, Facet::Mermaid);
    }

    #[test]
    fn embed_requires_png_path_and_source() {
        let reg = Registry::default();
        let err = reg
            .call(
                "embed",
                &Ctx::headless(),
                &json!({ "source": "graph TD; A" }),
            )
            .unwrap_err();
        assert!(
            matches!(err, crate::EngineFault::BadArgs(ref m) if m.contains("png_path")),
            "expected BadArgs for missing png_path, got {err:?}"
        );
        let err = reg
            .call("embed", &Ctx::headless(), &json!({ "png_path": "/x.png" }))
            .unwrap_err();
        assert!(
            matches!(err, crate::EngineFault::BadArgs(ref m) if m.contains("source")),
            "expected BadArgs for missing source, got {err:?}"
        );
    }

    #[test]
    fn extract_requires_png_path() {
        let err = Registry::default()
            .call("extract", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert!(
            matches!(err, crate::EngineFault::BadArgs(ref m) if m.contains("png_path")),
            "expected BadArgs for missing png_path, got {err:?}"
        );
    }

    #[test]
    fn embed_extract_round_trip_verified() {
        let out = temp_png();
        // Start from a plain rendered PNG with NO embedded payload.
        let png = render_png("graph TD; A-->B").expect("render");
        std::fs::write(&out, &png).expect("seed png");

        let reg = Registry::default();
        let source = "graph TD; X-->Y";
        let embedded = reg
            .call(
                "embed",
                &Ctx::headless(),
                &json!({ "png_path": out.to_string_lossy(), "source": source }),
            )
            .expect("dispatch embed");
        assert!(embedded.is_ok(), "tool_error: {:?}", embedded.tool_error);
        assert_eq!(embedded.data["kind"], "embed");
        assert_eq!(embedded.data["sha256"], source_sha256(source));
        let uuid = embedded.data["uuid"].as_str().unwrap().to_string();

        let extracted = reg
            .call(
                "extract",
                &Ctx::headless(),
                &json!({ "png_path": out.to_string_lossy() }),
            )
            .expect("dispatch extract");
        assert!(extracted.is_ok(), "tool_error: {:?}", extracted.tool_error);
        assert_eq!(extracted.data["kind"], "extract");
        assert_eq!(extracted.data["source"], source);
        assert_eq!(extracted.data["uuid"], uuid);
        assert_eq!(extracted.data["verification"], "verified");
        assert_eq!(extracted.data["sha256"], source_sha256(source));

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn extract_tampered_png_is_business_verification_failed() {
        let out = temp_png();
        let png = render_png("graph TD; A-->B").expect("render");
        // Embed source containing a marker, then flip the marker bytes in the
        // file with a SAME-LENGTH replacement: the stored digest no longer
        // matches the (modified) source. Chunk CRCs are not validated on read,
        // so the tamper is only catchable by the digest check.
        let embedded = scrybe_mermaid::embed(&png, "graph TD; AAAA-->B").expect("embed");
        let tampered: Vec<u8> = {
            let needle = b"graph TD; AAAA";
            let replacement = b"graph TD; ZZZZ";
            let pos = embedded
                .windows(needle.len())
                .position(|w| w == needle)
                .expect("marker present in payload");
            let mut t = embedded.clone();
            t[pos..pos + replacement.len()].copy_from_slice(replacement);
            t
        };
        std::fs::write(&out, &tampered).expect("write tampered png");

        let outcome = Registry::default()
            .call(
                "extract",
                &Ctx::headless(),
                &json!({ "png_path": out.to_string_lossy() }),
            )
            .expect("dispatch — tampering is a business failure, not an engine fault");
        assert!(!outcome.is_ok());
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "verification_failed");
        // The message carries both digests for forensics.
        let expected = outcome.data["expected"].as_str().unwrap();
        let actual = outcome.data["actual"].as_str().unwrap();
        assert_ne!(expected, actual);
        assert!(err.message.contains(expected) && err.message.contains(actual));

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn extract_png_without_payload_is_business_no_payload() {
        let out = temp_png();
        let png = render_png("graph TD; A-->B").expect("render");
        std::fs::write(&out, &png).expect("write plain png");

        let outcome = Registry::default()
            .call(
                "extract",
                &Ctx::headless(),
                &json!({ "png_path": out.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "no_payload");

        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn extract_missing_file_is_business_read_failed() {
        let outcome = Registry::default()
            .call(
                "extract",
                &Ctx::headless(),
                &json!({ "png_path": "/nonexistent/scrybe-a2a-test.png" }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "read_failed");
    }
}
