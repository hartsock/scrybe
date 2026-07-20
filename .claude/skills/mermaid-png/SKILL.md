---
name: mermaid-png
description: Render a Mermaid diagram to a PNG that carries its own source — the Mermaid text, a UUID, and a SHA-256 are embedded losslessly in the PNG's iTXt metadata, so the diagram is editable and verifiable forever without hunting for the original .md. Pure Rust, no mmdc/browser.
when_to_use: When you need a Mermaid diagram as a raster PNG for a document, README, wiki, or slide — anywhere a live ```mermaid fenced block won't render (Word/PDF export, Confluence, image hosts). Also whenever you would otherwise reach for `mmdc`: use this instead so the source travels inside the image. To recover or verify a diagram's source from a PNG someone shared, use the extract/verify steps below.
version: 1.0.0
license: Apache-2.0
caveats:
  exec: { only: ["scrybe"] }
  fs_read: all
  net: { only: [] }
  max_calls: unlimited
---

# Mermaid → PNG with the source baked in

Scrybe's signature move for diagrams: **the artifact carries its own proof of
what it is.** `scrybe mermaid png` renders a Mermaid diagram to a PNG and embeds,
in the PNG's `iTXt` metadata, the exact Mermaid **source**, a per-artifact
**UUID**, and the source's **SHA-256**. The rendering is pure Rust (the adopted
`mermaid-rs-renderer` — no `mmdc`, no headless browser).

## Never use raw `mmdc`

Raw `mmdc` produces a PNG with **no embedded source** — the diagram becomes a
dead image you can't edit or verify later, and publishing loses the round-trip.
Always render through `scrybe mermaid png` (CLI) or the `mermaid_to_png` MCP tool
so the source lives inside the file.

## The loop: render → (later) extract → verify

```bash
# 1. Render. `input` is a file of Mermaid source (e.g. a .mmd file).
scrybe mermaid png diagram.mmd --out diagram.png
#   Wrote diagram.png
#     uuid   b84e7f72-a050-4f7a-926e-e95f298d7da8
#     sha256 be689150b4c5c5fd...

# 2. Recover the source from any such PNG — no .md needed. Extraction
#    verifies the stored sha256 by default: exit 2 + no output if tampered
#    (use --unverified for forensics on a tampered file).
scrybe mermaid extract diagram.png        # prints the Mermaid source

# 3. Explicit integrity check: does the stored sha256 match the embedded source?
scrybe mermaid verify diagram.png         # "OK — sha256 … matches" (exit 0) or "TAMPERED"/"MISSING" (exit 1)
```

## From an agent (MCP)

The same capability is the `mermaid_to_png` tool. Read its typed `data`, never
the prose:

```jsonc
// mermaid_to_png { "source": "graph TD; A-->B", "output_path": "/abs/diagram.png" }
// → data: { "v": 1, "kind": "mermaid_to_png",
//           "png_path": "/abs/diagram.png",
//           "uuid": "…", "sha256": "…", "bytes": 6166 }
```

A malformed diagram comes back as a **business failure** (`tool_error`, e.g.
`render_failed`), not an engine fault — the tool ran and told you the diagram is
invalid.

## PNG vs a fenced ```mermaid block

- **Fenced ```mermaid block** — for Markdown that renders live (the Scrybe editor,
  GitHub, docs sites). Keep the source in the document; no PNG needed.
- **`scrybe mermaid png`** — when the destination can't render Mermaid live
  (Word/PDF export, Confluence, an image host) *and* you still want the source to
  travel with the picture. The embedded source means the PNG is not a dead end:
  anyone can `extract` it back to editable Mermaid.

## Naming for figure sets

When emitting many figures for one document, name them so they sort and map back
to headings — e.g. `YYYY-MM-DD_Doc_Fig-NN_Title.png`. (A future
`markdown_extract_and_render` tool automates this from `## Fig NN: Title`
headings; until then, name by hand.)
