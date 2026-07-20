<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-core

Foundation crate for the Scrybe workspace. Every other crate in the workspace
depends on this one. Python on the outside, Rust on the inside.

## What it does

Defines the canonical data model for Scrybe: documents, their content-addressed
identities, the Markdown AST, change tracking, the plugin interface, and the
in-memory workspace that holds all open documents during a session.

## Role in the architecture

`scrybe-core` is the single source of truth for shared types. No other crate
owns these definitions. The render, MCP, VCS, and Python binding crates all
depend on `scrybe-core` and never on each other for core types.

## Key public types and entry points

| Symbol | Description |
|--------|-------------|
| `Document` | Central editing unit: raw Markdown source + parsed AST + optional title |
| `ContentDigest` | Bare BLAKE3 digest of raw content bytes (32 bytes, 64 lowercase hex chars); stable across serialization formats. Not an IPFS/IPLD CID — no multibase/multicodec/multihash framing. `ContentId` remains as a deprecated alias |
| `ContentAddressable` | Trait: `fn content_digest(&self) -> ContentDigest` (deprecated `content_id` wrapper retained) |
| `Ast` / `Node` | Markdown AST types produced by the parser |
| `DocumentChange` / `DocumentHistory` / `TextRange` | Fine-grained change tracking |
| `Plugin` | Trait for Python and native extension plugins |
| `Workspace` | Collection of open documents keyed by `DocumentId` |
| `ScrybeError` | Unified error type for the workspace |

`ContentDigest::of(bytes)` computes BLAKE3 over exactly the given bytes
(for a `Document`, the raw Markdown source — never path, title, or other
metadata) and encodes it as lowercase hex. `ContentDigest::from_hex`
parses/validates an existing digest string. `ContentDigest::verify`
confirms integrity without re-hashing the full payload externally.

## Build and test

```sh
# From workspace root
cargo build -p scrybe-core
cargo test -p scrybe-core

# Or from this directory
cargo build
cargo test
```

No external system dependencies. All crypto is pure Rust via `blake3` and
`ciborium` (deterministic CBOR serialization).
