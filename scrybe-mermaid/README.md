<!--
SPDX-License-Identifier: Apache-2.0
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

## Python quick start

```bash
pip install scrybe-mermaid
```

```python
from pathlib import Path
from scrybe_mermaid import embed, extract

source = """
graph TD
    A[Christmas] -->|Get money| B(Go shopping)
    B --> C{Let me think}
    C -->|One| D[Laptop]
    C -->|Two| E[iPhone]
"""

# diagram.png: any PNG — render one with mmdc, Kroki, or the Mermaid live editor
png_in = Path("diagram.png").read_bytes()
png_out = embed(png_in, source)
Path("diagram-with-source.png").write_bytes(png_out)

payload = extract(png_out)  # verifies the embedded sha256 — raises ValueError if tampered
if payload.source != source:  # optional
    raise ValueError("Round-trip mismatch")
print(f"Round-tripped {len(payload.source)} chars; sha256={payload.sha256[:12]}…")
```

The resulting PNG renders normally in any image viewer *and* carries its own Mermaid source for round-tripping. See the [API reference](#key-public-types-and-entry-points) below.

## Codec format

- **Chunk key:** `scrybe-mermaid`
- **Value:** JSON `{ "source": "<mermaid source>", "sha256": "<hex>", "uuid": "<v4>" }`

The `sha256` field is a SHA-256 digest of the source bytes, and `extract`
**enforces it by default**: the digest is recomputed from the extracted
source and compared against the stored value. Three outcomes are
distinguished:

| Outcome | Python | Rust |
|---------|--------|------|
| Digest present and matching | payload with `verified == True` | `Ok(VerifiedPayload)` with `VerificationStatus::Verified` |
| Digest present but mismatched (tampered) | raises `ValueError` | `Err(MermaidError::VerificationFailed { expected, actual, .. })` |
| No digest stored (older/foreign payload) | payload with `verified == False`, `sha256 == ""` | `Ok(VerifiedPayload)` with `VerificationStatus::NoDigest` |

A payload with no digest is never reported as verified. For forensics on
tampered or foreign payloads, `extract_unverified` returns the raw stored
fields without any check.

## Role in the architecture

`scrybe-mermaid` is a self-contained utility crate with no dependency on
`scrybe-core`. It is used by `scrybe-mcp-server` (the `embed`/`extract` tools),
`scrybe-cli` (`scrybe mermaid embed/extract/verify`), and the Tauri backend.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `embed(png_bytes, source) -> Result<Vec<u8>>` | Inserts iTXt chunk; returns modified PNG bytes |
| `extract(png_bytes) -> Result<VerifiedPayload>` | Reads the iTXt chunk and verifies the stored sha256 against the source; mismatch → `MermaidError::VerificationFailed` |
| `extract_unverified(png_bytes) -> Result<MermaidPayload>` | Raw stored fields, no digest check (forensics) |
| `VerifiedPayload` | `source: String` + `uuid: String` + `verification: VerificationStatus` |
| `VerificationStatus` | `Verified { algorithm, digest }` or `NoDigest` (older/foreign payloads) |
| `MermaidPayload` | Raw stored `source` + `sha256` + `uuid` (unchecked) |
| `MermaidError` | Error type covering missing chunk, malformed JSON, PNG decode failure, digest mismatch (`VerificationFailed { expected, actual, .. }`) |

## Build and test

```sh
cargo build -p scrybe-mermaid
cargo test -p scrybe-mermaid
```

The codec parses PNG chunks from first principles — no external binaries
required.
