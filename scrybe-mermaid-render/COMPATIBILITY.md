<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Compatibility

## Mermaid.js version target

This crate targets **Mermaid.js v11.x** syntax.

That is the version loaded by `scrybe-render`'s HTML wrapper
(`scrybe-cli/src/wrap.rs`) and the version shipped by the `mmdc` oracle
used in trace tests. Compatibility is defined as: for any `.mmd` source file
in the supported subset, `render_to_svg(source)` produces an SVG that is
**semantically equivalent** to `mmdc`'s SVG output — same participants/nodes,
same edges, same visible labels, SSIM ≥ 0.92 when both are rasterized by
`resvg`.

## Oracle version pinning

`tests/oracle/.oracle-meta.json` is written by `scripts/gen_oracle.sh`
and records the exact `mmdc` and Mermaid.js versions used to generate the
oracle SVG files. This file is **gitignored** alongside the oracle SVGs;
regenerate both together.

```json
{
  "generated_at": "ISO-8601 timestamp",
  "mmdc_version": "10.x.y",
  "mermaid_version": "11.x.y",
  "generator": "scripts/gen_oracle.sh",
  "oracle_dir": "tests/oracle/",
  "fixtures": {
    "sequence": ["01_minimal", "02_request_response", "..."],
    "flowchart": ["01_minimal", "02_linear", "..."]
  }
}
```

Whenever the oracle is regenerated with a different `mmdc` version, update
the `mmdc_version` field and re-run the trace tests to confirm the threshold
still holds.

---

## Diagram type support matrix

| Diagram type | Status | Target syntax | Phase |
|---|---|---|---|
| Sequence | Planned | Mermaid 11.x | 1–2 |
| Flowchart / graph | Planned | Mermaid 11.x | 3–5 |
| Class | Future | — | — |
| State | Future | — | — |
| Entity Relationship | Future | — | — |
| Gantt | Future | — | — |
| Pie chart | Future | — | — |
| Git graph | Future | — | — |
| Mindmap | Future | — | — |
| Timeline | Future | — | — |
| User Journey | Future | — | — |
| C4 | Future | — | — |
| Sankey | Future | — | — |
| XY Chart | Future | — | — |
| Block diagram | Future | — | — |
| Architecture | Future | — | — |
| Kanban | Future | — | — |

Status key:
- **Planned** — in the drake-swarm phase roadmap
- **Future** — tracked, not yet scheduled
- **Partial** — some features implemented (used as phases complete)
- **Complete** — passes all trace tests at SSIM ≥ 0.92

---

## Feature-level compatibility

### Sequence diagrams (`sequenceDiagram`)

| Feature | Status | Notes |
|---|---|---|
| `participant A` declaration | Planned | |
| `actor A` declaration | Planned | |
| `participant A as Display` | Planned | |
| `->>` solid async arrow | Planned | |
| `-->>` dotted async arrow | Planned | |
| `->` solid line | Planned | |
| `-->` dotted line | Planned | |
| `-x` solid cross | Planned | |
| `--x` dotted cross | Planned | |
| `Note over A,B: text` | Planned | |
| `Note left of A: text` | Planned | |
| `Note right of A: text` | Planned | |
| `activate A` / `deactivate A` | Planned | |
| `loop label` … `end` | Planned | |
| `alt label` … `else` … `end` | Planned | |
| `opt label` … `end` | Planned | |
| `par label` … `and` … `end` | Planned | |
| Auto-numbering (`autonumber`) | Future | |
| `critical` / `break` blocks | Future | |
| Nested `alt` inside `loop` | Future | |
| `link` / external URLs | Future | |
| `box` grouping | Future | |

### Flowcharts / directed graphs (`graph`, `flowchart`)

| Feature | Status | Notes |
|---|---|---|
| `graph TD` direction | Planned | Top-down |
| `graph LR` direction | Planned | Left-right |
| `graph BT` direction | Planned | Bottom-top |
| `graph RL` direction | Planned | Right-left |
| `flowchart` keyword alias | Planned | |
| `A[rect label]` node | Planned | |
| `A(rounded label)` node | Planned | |
| `A{diamond label}` node | Planned | |
| `A([stadium label])` node | Planned | |
| `A((circle label))` node | Planned | |
| `A{{hexagon label}}` node | Planned | |
| `A["quoted label"]` | Planned | |
| `A --> B` arrow edge | Planned | |
| `A --- B` line edge | Planned | |
| `A -.-> B` dotted arrow | Planned | |
| `A ==> B` thick arrow | Planned | |
| `A --text--> B` labelled edge | Planned | |
| `subgraph id [label]` | Planned | Flat — no nested layout |
| `%%` comments | Planned | Stripped at parse time |
| `style A fill:#f9f` | Future | |
| `classDef` / `class` | Future | |
| `click A href` | Future | |
| ELK layout engine | Future | |
| Nested subgraphs | Future | |
| `direction` inside subgraph | Future | |

---

## SVG metadata

Every SVG produced by this crate embeds the Mermaid source in a
`<metadata>` element:

```xml
<metadata>
  <scrybe:mermaid xmlns:scrybe="https://scrybe.ai/ns/mermaid">
    <scrybe:source><![CDATA[...original source...]]></scrybe:source>
    <scrybe:sha256>hex-encoded SHA-256 of source bytes</scrybe:sha256>
  </scrybe:mermaid>
</metadata>
```

This is not part of the Mermaid.js / mmdc specification — it is an extension
unique to `scrybe-mermaid-render`. The SHA-256 matches the convention used
by `scrybe-mermaid` for PNG iTXt embedding.

---

## Rust API stability

`scrybe-mermaid-render` is **experimental** (`publish = false`). No API
stability guarantees are made while the version is `< 1.0.0`. The public
surface is:

```rust
pub fn render_to_svg(source: &str) -> Result<String>;

#[cfg(feature = "png")]
pub fn render_to_png(source: &str) -> Result<Vec<u8>>;

pub enum MermaidRenderError { ... }
```

Promotion to `publish = true` and API stability will follow Phase 6
completion and passage of all trace tests for sequence + flowchart.

---

## Python API compatibility

| Item | Version |
|---|---|
| Requires Python | ≥ 3.9 |
| PyO3 | 0.28 (abi3-py39 stable ABI) |
| `render_to_svg(source: str) -> str` | Phase 6 |
| `render_to_png(source: str) -> bytes` | Phase 6 (`png` feature) |

The Python package name is `scrybe-mermaid-render`. It follows the same
install pattern as `scrybe-mermaid`:

```bash
pip install scrybe-mermaid-render          # pre-built wheel
maturin develop --features python,extension-module  # from source
```
