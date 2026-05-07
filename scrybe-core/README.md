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
| `ContentId` | BLAKE3 content identifier (lowercase hex); stable across serialization formats |
| `ContentAddressable` | Trait: `fn content_id(&self) -> ContentId` |
| `Ast` / `Node` | Markdown AST types produced by the parser |
| `DocumentChange` / `DocumentHistory` / `TextRange` | Fine-grained change tracking |
| `Plugin` | Trait for Python and native extension plugins |
| `Workspace` | Collection of open documents keyed by `DocumentId` |
| `ScrybeError` | Unified error type for the workspace |

`ContentId::of(bytes)` computes BLAKE3 and encodes as hex. `ContentId::verify`
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
