<!--
SPDX-License-Identifier: AGPL-3.0-or-later
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-mermaid

Standalone PNG iTXt codec: embeds and extracts Mermaid diagram source as
invisible metadata inside a PNG file. Python on the outside, Rust on the
inside.

## What it does

Uses the PNG iTXt (international text) metadata chunk mechanism to store
Mermaid diagram source alongside the rendered image. The PNG is fully valid
and renders normally in any image viewer. The source text travels with the
image and can be round-tripped without loss.

## Codec format

- **Chunk key:** `scrybe-mermaid`
- **Value:** JSON `{ "source": "<mermaid source>", "sha256": "<hex>" }`

The `sha256` field is a SHA-256 digest of the source bytes for lightweight
integrity verification.

## Role in the architecture

`scrybe-mermaid` is a self-contained utility crate with no dependency on
`scrybe-core`. It is used by `scrybe-mcp-server` (the `embed`/`extract` tools),
`scrybe-cli` (`scrybe mermaid embed/extract/verify`), and the Tauri backend.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `embed(png_bytes, source) -> Result<Vec<u8>>` | Inserts iTXt chunk; returns modified PNG bytes |
| `extract(png_bytes) -> Result<MermaidPayload>` | Reads and parses the iTXt chunk |
| `MermaidPayload` | `source: String` + `sha256: String` |
| `MermaidError` | Error type covering missing chunk, malformed JSON, PNG decode failure |

## Build and test

```sh
cargo build -p scrybe-mermaid
cargo test -p scrybe-mermaid
```

Depends on the `png` crate for chunk-level PNG manipulation and `base64` for
encoding the iTXt payload. No external binaries required.
