// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `lint` — structured analysis of a Markdown document. Pure/headless: it works
//! under the `Headless` transport. Design §4A default set (`Core` facet,
//! non-mutating). Wraps [`crate::lint::lint_document`] and adds the source's
//! content id, so the same analysis backs both `scrybe lint` and the MCP tool.

use scrybe_core::{ContentAddressable, Document};
use serde_json::{json, Value};

use crate::lint::lint_document;
use crate::{Ctx, DataSchema, Facet, ToolOutcome, ToolSpec};

/// Version of this tool's `data` payload.
const DATA_VERSION: u32 = 1;

/// The `lint` tool spec.
pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "lint",
        description: "Analyze a Markdown document and return structured statistics \
             (word/heading/code counts, code-block languages, math & Mermaid \
             presence, broken links) plus the source's content id. Input: `source` \
             (Markdown string). Read the `data` payload — `clean` is true when \
             there are no broken links.",
        input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: data_schema,
        },
        mutates: false,
        facet: Facet::Core,
        handler,
    }
}

fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "source": {
                "type": "string",
                "description": "Markdown source to lint."
            }
        },
        "required": ["source"]
    })
}

fn data_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "v": { "const": DATA_VERSION },
            "kind": { "const": "lint" },
            "content_id": { "type": "string", "description": "BLAKE3 content digest of the source (64 lowercase hex chars)." },
            "word_count": { "type": "integer" },
            "heading_count": { "type": "integer" },
            "max_heading_depth": { "type": "integer" },
            "code_block_count": { "type": "integer" },
            "code_block_langs": { "type": "array", "items": { "type": "string" } },
            "has_math": { "type": "boolean" },
            "has_mermaid": { "type": "boolean" },
            "broken_links": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "url": { "type": "string" }
                    }
                }
            },
            "clean": { "type": "boolean" }
        },
        "required": [
            "v", "kind", "content_id", "word_count", "heading_count",
            "max_heading_depth", "code_block_count", "code_block_langs",
            "has_math", "has_mermaid", "broken_links", "clean"
        ]
    })
}

fn handler(_ctx: &Ctx, args: &Value) -> ToolOutcome {
    // `source` is guaranteed present by the dispatcher's required-args gate.
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let doc = Document::new(source);
    let report = lint_document(&doc);
    let broken: Vec<Value> = report
        .broken_links
        .iter()
        .map(|b| json!({ "text": b.text, "url": b.url }))
        .collect();
    ToolOutcome::ok(json!({
        "v": DATA_VERSION,
        "kind": "lint",
        // JSON key stays `content_id` — it is a versioned wire schema
        // (DATA_VERSION); only the Rust-side vocabulary was renamed.
        "content_id": doc.content_digest().to_string(),
        "word_count": report.word_count,
        "heading_count": report.heading_count,
        "max_heading_depth": report.max_heading_depth,
        "code_block_count": report.code_block_count,
        "code_block_langs": report.code_block_langs,
        "has_math": report.has_math,
        "has_mermaid": report.has_mermaid,
        "broken_links": broken,
        "clean": report.is_clean(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry};

    fn lint(src: &str) -> Value {
        Registry::default()
            .call("lint", &Ctx::headless(), &json!({ "source": src }))
            .expect("lint dispatches")
            .data
    }

    #[test]
    fn reports_counts_flags_and_content_id() {
        let d = lint("# H1\n\n## H2\n\ntext $x$\n\n```rust\nfn a(){}\n```\n");
        assert_eq!(d["v"], DATA_VERSION);
        assert_eq!(d["kind"], "lint");
        assert_eq!(d["heading_count"], 2);
        assert_eq!(d["max_heading_depth"], 2);
        assert_eq!(d["code_block_count"], 1);
        assert_eq!(d["has_math"], true);
        assert_eq!(d["clean"], true);
        // BLAKE3 hex content digest is present and non-trivial.
        assert!(d["content_id"].as_str().unwrap().len() >= 32);
    }

    #[test]
    fn detects_broken_links_and_reports_unclean() {
        let d = lint("[ok](https://x) and [bad]() and [frag](#)");
        assert_eq!(d["broken_links"].as_array().unwrap().len(), 2);
        assert_eq!(d["clean"], false);
    }

    #[test]
    fn mermaid_flagged_but_excluded_from_langs() {
        let d = lint("```mermaid\ngraph TD; A-->B\n```\n");
        assert_eq!(d["has_mermaid"], true);
        let langs = d["code_block_langs"].as_array().unwrap();
        assert!(!langs.iter().any(|l| l == "mermaid"));
    }
}
