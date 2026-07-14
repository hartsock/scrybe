<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-mermaid-render

Pure-Rust Mermaid diagram renderer (sequence + flowchart). SVG is the primary
output; PNG is a secondary conversion via `resvg` behind the optional `png`
feature. Every SVG produced by this crate embeds its Mermaid source (plus a
SHA-256 digest) in a `<metadata>` element, so the diagram is self-describing
and round-trippable without a separate sidecar file. Optional Python bindings
are available behind the `python` feature (PyO3).

Part of [scrybe](https://github.com/hartsock/scrybe), the MCP-native
cross-platform Markdown editor where the document is the conversation.

## Build and test

```sh
cargo build -p scrybe-mermaid-render
cargo test -p scrybe-mermaid-render
```

## License

Apache-2.0
