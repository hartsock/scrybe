// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `render` — Markdown → HTML via Scrybe's pipeline. Pure/headless: it needs no
//! running app, so it works under the `Headless` transport. Design §4A default
//! set (`Core` facet, non-mutating).

use scrybe_core::Document;
use scrybe_render::{render_html, Theme};
use serde_json::{json, Value};

use crate::{Ctx, DataSchema, EngineFault, Facet, ToolOutcome, ToolSpec};

/// Version of this tool's `data` payload.
const DATA_VERSION: u32 = 1;

/// The `render` tool spec.
pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "render",
        description: "Render Markdown to HTML using Scrybe's pipeline (syntect \
             highlighting, KaTeX-ready math, Mermaid wrappers, theme CSS). \
             Input: `source` (Markdown string) and optional `theme` \
             (default|dark|solarized). Returns `{ html, body_html, theme, bytes }` \
             — read the `data` payload, never this prose.",
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
                "description": "Markdown source to render."
            },
            "theme": {
                "type": "string",
                "enum": ["default", "dark", "solarized"],
                "description": "Theme CSS to prepend. Defaults to `default`."
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
            "kind": { "const": "render" },
            "html": { "type": "string", "description": "Full fragment incl. <style>." },
            "body_html": { "type": "string", "description": "Body only, no CSS." },
            "theme": { "type": "string" },
            "bytes": { "type": "integer" }
        },
        "required": ["v", "kind", "html", "body_html", "theme", "bytes"]
    })
}

fn handler(_ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
    // `source` is guaranteed present by the dispatcher's required-args gate.
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let theme = parse_theme(args.get("theme").and_then(Value::as_str));
    let doc = Document::new(source);
    let out = render_html(&doc, theme);
    Ok(ToolOutcome::ok(json!({
        "v": DATA_VERSION,
        "kind": "render",
        "html": out.html,
        "body_html": out.body_html,
        "theme": theme_name(theme),
        "bytes": out.html.len(),
    })))
}

/// Map the `theme` argument to a [`Theme`]; unknown/absent falls back to default.
fn parse_theme(name: Option<&str>) -> Theme {
    match name {
        Some("dark") => Theme::Dark,
        Some("solarized") => Theme::Solarized,
        _ => Theme::Default,
    }
}

fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Default => "default",
        Theme::Dark => "dark",
        Theme::Solarized => "solarized",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry};

    fn render(args: Value) -> ToolOutcome {
        Registry::default()
            .call("render", &Ctx::headless(), &args)
            .expect("render dispatches")
    }

    #[test]
    fn renders_h1_with_versioned_data() {
        let out = render(json!({ "source": "# Hello world" }));
        assert!(out.is_ok());
        assert_eq!(out.data["v"], DATA_VERSION);
        assert_eq!(out.data["kind"], "render");
        assert!(out.data["html"].as_str().unwrap().contains("<h1"));
        assert!(out.data["body_html"]
            .as_str()
            .unwrap()
            .contains("Hello world"));
        assert_eq!(out.data["theme"], "default");
        assert!(out.data["bytes"].as_u64().unwrap() > 0);
    }

    #[test]
    fn body_html_excludes_style_wrapper() {
        let out = render(json!({ "source": "text" }));
        assert!(out.data["html"].as_str().unwrap().contains("<style>"));
        assert!(!out.data["body_html"].as_str().unwrap().contains("<style>"));
    }

    #[test]
    fn theme_is_selected_and_reported() {
        assert_eq!(
            render(json!({ "source": "x", "theme": "dark" })).data["theme"],
            "dark"
        );
        assert_eq!(
            render(json!({ "source": "x", "theme": "solarized" })).data["theme"],
            "solarized"
        );
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        assert_eq!(
            render(json!({ "source": "x", "theme": "neon" })).data["theme"],
            "default"
        );
    }

    #[test]
    fn spec_is_pure_core_nonmutating() {
        let s = spec();
        assert_eq!(s.name, "render");
        assert!(!s.mutates);
        assert_eq!(s.facet, Facet::Core);
        assert_eq!(s.data_schema.version, DATA_VERSION);
    }
}
