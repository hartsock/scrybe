<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-mermaid-render

A thin wrapper over [`mermaid-rs-renderer`](https://crates.io/crates/mermaid-rs-renderer)
(pure-Rust Mermaid → SVG) that adds **Scrybe provenance**: the original Mermaid
source and its SHA-256 are injected into the SVG so the diagram is
self-describing and losslessly round-trippable.

Adopting the crate rather than building a renderer was decided in the **#132**
fidelity spike (adversarially surveyed, then **run hands-on** — flowchart and
sequence both render to valid SVG on real hardware). This wrapper keeps Scrybe's
differentiating value — *the source lives inside the artifact* — in-house, by
post-processing the crate's output. See `docs/design/mcp-rebuild.md` and
`ROADMAP.md` → "The renderer epic (#37): adopted, not built".

## API (this increment)

- `render_svg(source) -> Result<String>` — Mermaid → SVG (delegates to the crate).
- `render_svg_with_source(source) -> Result<String>` — SVG + an injected
  `<metadata>` element (namespace `https://scrybe.ai/ns/mermaid`) carrying the
  escaped source and its SHA-256.

## Next

PNG-via-resvg (`render_png`), then the PNG iTXt `mermaid_to_png` tool (#119)
which embeds `source + uuid + sha256` in the PNG metadata.
