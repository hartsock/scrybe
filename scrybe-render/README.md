<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-render

Markdown-to-HTML rendering pipeline for Scrybe. Python on the outside, Rust on
the inside.

## What it does

Takes a `Document` from `scrybe-core` and produces styled HTML. The pipeline
applies syntax highlighting to fenced code blocks, extracts and re-injects math
expressions (KaTeX-ready), wraps Mermaid code blocks (Mermaid.js-ready), and
prepends theme CSS.

## Role in the architecture

`scrybe-render` is a pure transformation layer with no I/O side-effects. It is
consumed by the Tauri backend (`scrybe-app`), the MCP server (`scrybe-mcp-server`),
the CLI (`scrybe-cli`), and the Python bindings (`scrybe-py`). It depends only
on `scrybe-core`.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `render_html(doc, theme) -> RenderOutput` | Primary entry point: full pipeline |
| `RenderOutput` | `html` (with `<style>`) and `body_html` (body fragment only) |
| `Theme` | Enum: `Default`, `Dark`, `Solarized` — each carries its own CSS |
| `extract_math(source) -> (String, Vec<MathPlaceholder>)` | Strips math before cmark sees it |
| `inject_mermaid_wrappers(html) -> String` | Wraps `<pre class="language-mermaid">` for Mermaid.js |

Syntax highlighting uses `syntect` loaded with default built-in themes and
syntax definitions (no external `.tmTheme` files required at runtime).

## Build and test

```sh
cargo build -p scrybe-render
cargo test -p scrybe-render
```

`syntect` and `pulldown-cmark` are pure Rust. KaTeX and Mermaid rendering are
client-side (JavaScript); this crate only produces the HTML hooks they attach to.
