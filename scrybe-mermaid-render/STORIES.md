# scrybe-mermaid-render — Story tracking

This document indexes the **drake-flight-sized sub-stories** of the
[scrybe-mermaid-render umbrella issue #37](https://github.com/hartsock/scrybe/issues/37).

Each story corresponds to one GitHub issue and one PR. The full
specification (what to implement, files touched, tests required, acceptance
criteria, out-of-scope) lives on the issue itself — this file is just the
index so an agent can navigate from the repo.

## Standard acceptance contract — applies to every story

Every story PR must meet all of:

- [ ] `cargo build -p scrybe-mermaid-render` succeeds.
- [ ] `cargo test -p scrybe-mermaid-render` — all tests pass (incl. trace
      tests that this PR is supposed to flip from SKIP/IGNORE to PASS).
- [ ] `cargo clippy -p scrybe-mermaid-render -- -D warnings` clean.
- [ ] `cargo fmt -- --check` clean.
- [ ] **Coverage for new code ≥ 80%** (`cargo llvm-cov`).
- [ ] PR body has "What", "Test plan", "Out of scope" sections plus
      `Fixes #N` and `Refs #37`.
- [ ] Branch name = `mermaid-render/<step-id>-<slug>` (see table below).
- [ ] No `#[ignore]` left on a test this PR was supposed to make pass.

## Tracking table

| # | Phase | Story | Branch |
|---|---|---|---|
| #52 | 1 | parser(sequence): participant + actor declarations | `mermaid-render/1.1-participant-actor-declarations` |
| #53 | 1 | parser(sequence): basic message arrows (->, ->>, -->, -->>) | `mermaid-render/1.2-basic-message-arrows` |
| #54 | 1 | parser(sequence): cancel arrows + Note over/left/right | `mermaid-render/1.3-cancel-arrows-note-over-left-right` |
| #55 | 1 | parser(sequence): loop / alt-else / opt / par blocks | `mermaid-render/1.4-loop-alt-else-opt-par-blocks` |
| #56 | 1 | parser(sequence): activate / deactivate stack | `mermaid-render/1.5-activate-deactivate-stack` |
| #57 | 1 | parser(flowchart): direction + minimal node + edge | `mermaid-render/1.6-direction-minimal-node-edge` |
| #58 | 1 | parser(flowchart): all node shapes | `mermaid-render/1.7-all-node-shapes` |
| #59 | 1 | parser(flowchart): edge variants | `mermaid-render/1.8-edge-variants` |
| #60 | 1 | parser(flowchart): subgraph blocks | `mermaid-render/1.9-subgraph-blocks` |
| #61 | 2 | layout(sequence): participant x + statement-walk y | `mermaid-render/2.1-participant-x-statement-walk-y` |
| #62 | 2 | layout(sequence): activation stacks per participant | `mermaid-render/2.2-activation-stacks-per-participant` |
| #63 | 2 | layout(sequence): notes + group-block layout | `mermaid-render/2.3-notes-group-block-layout` |
| #64 | 2 | svg: <metadata> root with source + sha256 (Tier-0 enabling step) | `mermaid-render/2.4-metadata-root-with-source-sha256-tier-0-enabling-s` |
| #65 | 2 | svg(sequence): lifeline + participant header primitives | `mermaid-render/2.5-lifeline-participant-header-primitives` |
| #66 | 2 | svg(sequence): arrow + activation primitives | `mermaid-render/2.6-arrow-activation-primitives` |
| #67 | 2 | svg(sequence): assemble builder — sequence trace tests PASS Tier 0+1 | `mermaid-render/2.7-assemble-builder-sequence-trace-tests-pass-tier-0-` |
| #68 | 3 | layout(sugiyama): cycle removal (DFS back-edge reversal) | `mermaid-render/3.1-cycle-removal-dfs-back-edge-reversal` |
| #69 | 3 | layout(sugiyama): layer assignment (Longest Path) | `mermaid-render/3.2-layer-assignment-longest-path` |
| #70 | 3 | layout(sugiyama): dummy node insertion for cross-layer edges | `mermaid-render/3.3-dummy-node-insertion-for-cross-layer-edges` |
| #71 | 3 | layout(sugiyama): barycenter crossing minimization (single pass) | `mermaid-render/3.4-barycenter-crossing-minimization-single-pass` |
| #72 | 3 | layout(sugiyama): iterate crossing-min to convergence (≤3 passes) | `mermaid-render/3.5-iterate-crossing-min-to-convergence-3-passes` |
| #73 | 3 | layout(sugiyama): coordinate assignment (simplified Brandes-Köpf) | `mermaid-render/3.6-coordinate-assignment-simplified-brandes-köpf` |
| #74 | 4 | svg(flowchart): shape primitives | `mermaid-render/4.1-shape-primitives` |
| #75 | 4 | svg(flowchart): assemble builder — nodes + edges via Sugiyama coords | `mermaid-render/4.2-assemble-builder-nodes-edges-via-sugiyama-coords` |
| #76 | 4 | png: resvg/tiny-skia rasterization behind `png` feature | `mermaid-render/4.3-resvg-tiny-skia-rasterization-behind-png-feature` |
| #77 | 4 | grader: SSIM via image-compare | `mermaid-render/4.4-ssim-via-image-compare` |
| #78 | 4 | trace tests: enforce SSIM ≥ 0.92 for flowchart fixtures | `mermaid-render/4.5-enforce-ssim-0-92-for-flowchart-fixtures` |
| #79 | 5 | python: render_to_svg PyO3 wrapper | `mermaid-render/5.1-render-to-svg-pyo3-wrapper` |
| #80 | 5 | python: render_to_png PyO3 wrapper (gated) | `mermaid-render/5.2-render-to-png-pyo3-wrapper-gated` |
| #81 | 5 | python: package surface (__init__.py + py.typed + pyproject) | `mermaid-render/5.3-package-surface-init-py-py-typed-pyproject` |
| #82 | 5 | python: pytest suite with _RUST_AVAILABLE guard | `mermaid-render/5.4-pytest-suite-with-rust-available-guard` |
| #83 | 6 | conformance: gen_oracle.sh --with-upstream + upstream fixtures Tier 1 | `mermaid-render/6.1-gen-oracle-sh-with-upstream-upstream-fixtures-tier` |
| #84 | 6 | licensing: add LICENSE-MPL-2.0 for resvg/usvg distribution | `mermaid-render/6.2-add-license-mpl-2-0-for-resvg-usvg-distribution` |
| #85 | 6 | release: flip publish = true + bump version + tick gating checklist | `mermaid-render/6.3-flip-publish-true-bump-version-tick-gating-checkli` |

## Grouped by phase

### Phase 1 — Parsers

- **1.1** — [#52](https://github.com/hartsock/scrybe/issues/52) — parser(sequence): participant + actor declarations
- **1.2** — [#53](https://github.com/hartsock/scrybe/issues/53) — parser(sequence): basic message arrows (->, ->>, -->, -->>)
- **1.3** — [#54](https://github.com/hartsock/scrybe/issues/54) — parser(sequence): cancel arrows + Note over/left/right
- **1.4** — [#55](https://github.com/hartsock/scrybe/issues/55) — parser(sequence): loop / alt-else / opt / par blocks
- **1.5** — [#56](https://github.com/hartsock/scrybe/issues/56) — parser(sequence): activate / deactivate stack
- **1.6** — [#57](https://github.com/hartsock/scrybe/issues/57) — parser(flowchart): direction + minimal node + edge
- **1.7** — [#58](https://github.com/hartsock/scrybe/issues/58) — parser(flowchart): all node shapes
- **1.8** — [#59](https://github.com/hartsock/scrybe/issues/59) — parser(flowchart): edge variants
- **1.9** — [#60](https://github.com/hartsock/scrybe/issues/60) — parser(flowchart): subgraph blocks

### Phase 2 — Sequence layout + SVG

- **2.1** — [#61](https://github.com/hartsock/scrybe/issues/61) — layout(sequence): participant x + statement-walk y
- **2.2** — [#62](https://github.com/hartsock/scrybe/issues/62) — layout(sequence): activation stacks per participant
- **2.3** — [#63](https://github.com/hartsock/scrybe/issues/63) — layout(sequence): notes + group-block layout
- **2.4** — [#64](https://github.com/hartsock/scrybe/issues/64) — svg: <metadata> root with source + sha256 (Tier-0 enabling step)
- **2.5** — [#65](https://github.com/hartsock/scrybe/issues/65) — svg(sequence): lifeline + participant header primitives
- **2.6** — [#66](https://github.com/hartsock/scrybe/issues/66) — svg(sequence): arrow + activation primitives
- **2.7** — [#67](https://github.com/hartsock/scrybe/issues/67) — svg(sequence): assemble builder — sequence trace tests PASS Tier 0+1

### Phase 3 — Sugiyama layout

- **3.1** — [#68](https://github.com/hartsock/scrybe/issues/68) — layout(sugiyama): cycle removal (DFS back-edge reversal)
- **3.2** — [#69](https://github.com/hartsock/scrybe/issues/69) — layout(sugiyama): layer assignment (Longest Path)
- **3.3** — [#70](https://github.com/hartsock/scrybe/issues/70) — layout(sugiyama): dummy node insertion for cross-layer edges
- **3.4** — [#71](https://github.com/hartsock/scrybe/issues/71) — layout(sugiyama): barycenter crossing minimization (single pass)
- **3.5** — [#72](https://github.com/hartsock/scrybe/issues/72) — layout(sugiyama): iterate crossing-min to convergence (≤3 passes)
- **3.6** — [#73](https://github.com/hartsock/scrybe/issues/73) — layout(sugiyama): coordinate assignment (simplified Brandes-Köpf)

### Phase 4 — Flowchart SVG + PNG + SSIM

- **4.1** — [#74](https://github.com/hartsock/scrybe/issues/74) — svg(flowchart): shape primitives
- **4.2** — [#75](https://github.com/hartsock/scrybe/issues/75) — svg(flowchart): assemble builder — nodes + edges via Sugiyama coords
- **4.3** — [#76](https://github.com/hartsock/scrybe/issues/76) — png: resvg/tiny-skia rasterization behind `png` feature
- **4.4** — [#77](https://github.com/hartsock/scrybe/issues/77) — grader: SSIM via image-compare
- **4.5** — [#78](https://github.com/hartsock/scrybe/issues/78) — trace tests: enforce SSIM ≥ 0.92 for flowchart fixtures

### Phase 5 — PyO3 bindings

- **5.1** — [#79](https://github.com/hartsock/scrybe/issues/79) — python: render_to_svg PyO3 wrapper
- **5.2** — [#80](https://github.com/hartsock/scrybe/issues/80) — python: render_to_png PyO3 wrapper (gated)
- **5.3** — [#81](https://github.com/hartsock/scrybe/issues/81) — python: package surface (__init__.py + py.typed + pyproject)
- **5.4** — [#82](https://github.com/hartsock/scrybe/issues/82) — python: pytest suite with _RUST_AVAILABLE guard

### Phase 6 — Conformance / licensing / release

- **6.1** — [#83](https://github.com/hartsock/scrybe/issues/83) — conformance: gen_oracle.sh --with-upstream + upstream fixtures Tier 1
- **6.2** — [#84](https://github.com/hartsock/scrybe/issues/84) — licensing: add LICENSE-MPL-2.0 for resvg/usvg distribution
- **6.3** — [#85](https://github.com/hartsock/scrybe/issues/85) — release: flip publish = true + bump version + tick gating checklist

## Notes

- Phase dependencies follow the layered build of the renderer:
  Phase 1 (parsers) → Phase 2 (sequence end-to-end) and Phase 3 (Sugiyama
  layout) in parallel → Phase 4 (flowchart end-to-end) → Phase 5 (Python
  bindings) → Phase 6 (release readiness).
- Within a phase, stories can usually run in parallel; the rare ordering
  constraint is called out in the issue body ("Refs" or "Out of scope").
- The full breakdown design (issue bodies, mocks per story, rationale)
  lives at `~/.claude/plans/scrybe-37-breakdown.md` (private planning
  artifact).
