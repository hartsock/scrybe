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
- `render_png(source) -> Result<Vec<u8>>` — Mermaid → PNG bytes, via the crate's
  own resvg/tiny-skia rasterizer (`png` feature). Verified end-to-end: produces a
  real `PNG image data … 8-bit/color RGBA` file.
- `source_sha256(source) -> String` — the digest shared with the PNG iTXt codec.

## Next

The PNG iTXt `mermaid_to_png` tool (#119), which renders via `render_png` and
embeds `source + uuid + sha256` in the PNG metadata (requires adding `uuid` to
the `scrybe-mermaid` iTXt payload).
