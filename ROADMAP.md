<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Scrybe ROADMAP — v0.4.0 → v1.0.0

**Current:** `v0.2.0` (last pushed tag) · `0.2.1-dev` (working tree) — see
[Version reconciliation](docs/TRIAGE.md#version-reconciliation-read-this-first).
**Target:** `v1.0.0`, delivered across milestones **v0.4.0 → v0.12.0**.
**Created:** 2026-07-13 · **Maintained per:** [`.claude/skills/repository-roadmap/SKILL.md`](.claude/skills/repository-roadmap/SKILL.md)

> **GitHub issues are the state; this document is the map.** Every work item
> carries a tracking issue number. When this document and GitHub disagree,
> **GitHub wins** — reconcile before trusting the prose.

## Ground-truth protocol

```bash
# live state of any milestone's issues (never trust the doc's checkboxes)
gh issue list --repo hartsock/scrybe --state all \
  --search "119 108 32 116 114 115 122 128" \
  --json number,title,state,closedAt

# a single issue
gh issue view <N> --repo hartsock/scrybe --json number,title,state,closedAt
```

A **milestone is done** when every issue in its tables is closed (or carries a
comment re-scoping it out). Report progress as `closed / total` per milestone;
flag any issue whose GitHub state contradicts this document for a roadmap-update
PR. Edit this file only for **structural** change (items added/removed/re-phased,
exit criteria changed) — issue *state* lives in GitHub, not here.

## Source plans

This roadmap *sequences* existing plans; it does not replace them.

- [`docs/TRIAGE.md`](docs/TRIAGE.md) — full 58-issue triage, epics, dispositions.
- [`docs/design/mcp-rebuild.md`](docs/design/mcp-rebuild.md) — the native-modulex MCP rebuild (epic **#122**).
- [`docs/design/cli-rpc.md`](docs/design/cli-rpc.md) — the CLI↔GUI socket protocol the rebuild unifies onto.
- [`docs/adr/0001-python-outside-rust-inside.md`](docs/adr/0001-python-outside-rust-inside.md) — the distribution philosophy.
- **#37** epic — `PLAN.md` + `COMPATIBILITY.md` live on the `feat/scrybe-mermaid-render` branch. The `scrybe-mermaid-render` crate does **not** exist on `main` yet; this roadmap builds it (v0.6–v0.11). Today only `scrybe-mermaid` (the iTXt PNG codec) ships. Note: `scrybe-swarm` / `scrybe-panels` are in `experimental/`, not shipped workspace members.

## Epics at a glance

| Epic | Milestones | Tracking |
|---|---|---|
| MCP rebuild / CLI↔MCP parity (native-modulex) | v0.4–v0.7 | **#122** (epic), #108 #46 #121 #28 #15 #123 #124 #125 #126 #127 |
| Mermaid-PNG round-trip + agent skills | v0.4–v0.5 | #119 #28 #121 #126 |
| scrybe-mermaid-render (pure Rust) | v0.6–v0.11 | **#37** (umbrella), #52–#85 |
| Human editor UX | v0.4–v0.7 | #32 #15 #109 #45 #111 #120 #44 |
| scrybe-py library | v0.7–v0.8 | #6 #7 #8 |
| Packaging / distribution / CI guardrails | v0.4, v0.11 | #116 #1 #2 #128 |
| New feature plugins (v0) | v0.9, v0.12 | #31 #33 #34 |
| Strategic explores (decision spikes) | v0.4 | #114 #115 |

---

## v0.4.0 — "Keystone" (the next release)

**Theme:** Make the MCP actually work and ship the priority Mermaid-PNG skill,
behind privacy guardrails, with the strategic spikes resolved up front so later
epics can pivot before heavy investment.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Mermaid→PNG skill (iTXt source+uuid+sha256) | **#119** | — | **PRIORITY.** `/mermaid-png` skill largely exists; deliver the `mermaid_to_png` foundation + `mermaid-png` SKILL.md. |
| MCP rebuild epic opened | #122 | — | Umbrella; create `scrybe-tools`, `ToolSpec`/`Facet`/`Transport` (Headless first); port pure tools. |
| Fix `open` → dispatch via `scrybe-rpc` (root cause) | #108 | — | `open` emits `scrybe://cli-open` to the live app; delete the MCP-private `Workspace`. Fix JSON-RPC top-level `error` + `tools/call` `isError`. Add `--tools`/`--probe`. |
| Path-bar copy affordance | #32 | — | *verify-may-be-done* — verify full-path vs relative-to-content-root copy; close remainder. |
| Secret-scan CI guardrail | #116 | — | gitleaks (free binary) + internal-specifics linter on GitHub-hosted runners. |
| Release hygiene: lock-step version + v0.3.0 | #128 | — | `[workspace.package]` version; tag or fold v0.3.0 before the next release. |
| Spike: wrap Ferrite? | #114 | — | *explore-spike* — written wrap-vs-build recommendation. |
| Spike: wrap markdown-tui-explorer? | #115 | — | *explore-spike* — may re-scope v0.6+. |

**Exit:**
- `mcp open <file>` makes a tab **actually appear** in the running app (headless fallback when no app); an agent can distinguish success from failure via `isError`. (#108)
- #119 skill renders Mermaid→PNG with embedded source+uuid+sha256, round-trips via `extract`.
- Secret-scan CI is green on every PR. (#116)
- #114/#115 spikes resolved with a recommendation that scopes the editor/renderer epics.
- A written decision on the v0.3.0 tag + `[workspace.package]` landed. (#128)

---

## v0.5.0 — "Parity"

**Theme:** Build out true CLI↔MCP parity on the unified `scrybe-rpc` foundation
(data contract + the mermaid tools), and land the first batch of independent
editor quality-of-life increments.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Atomic `mermaid_to_png` MCP tool | #121 | #108 | Description doubles as an embedded agent prompt ("never raw mmdc"). |
| `markdown_extract_and_render` tool | #126 | #108, #121 | `## Fig NN: Title` → `YYYY-MM-DD_Doc_Fig-NN_Title.png`, all embedded. |
| `list_tabs` over the live socket | #46 | #108 | Agent sees the real tab set. |
| Inline render of embedded-source PNGs | #28 | — | `![alt](x.png)` with iTXt `mermaid-source` renders like a fenced block (rides live `reload`). |
| Versioned typed data contract | #123 | #108 | Every tool emits a stable `data` payload; `--format data`. Agents never parse prose. |
| `mcp-editing` agent skill | #127 | — | The safe `open→read→find→edit→render/lint` loop + reload discipline. |
| fs-watcher reload | #15 | — | Clean = silent reload; dirty = prompt. |
| Tab context menu (bulk close) | #109 | — | Close to the Right / Left / Others. |
| Vim keybinding depth | #45 | — | *verify-may-be-done* — verify search/regex-replace; close remainder. |

**Exit:**
- Agents call `mermaid_to_png` / `markdown_extract_and_render` from the MCP surface (no bare `mmdc`). (#121, #126)
- `list_tabs` returns the live tab set with paths + dirty flags. (#46)
- Every tool returns a versioned `data` payload on both CLI and MCP; golden `--tools` snapshot test passes. (#123)
- Each MCP tool built this milestone ships **with** its CLI subcommand (parity-by-construction; the CI *gate* lands v0.7/#125).
- `mcp-editing` and `repository-roadmap` skills are installable. (#127)

---

## v0.6.0 — "Grammar"

**Theme:** Open the pure-Rust `scrybe-mermaid-render` epic (Phase 1 parsers) and
finish MCP progressive disclosure, plus two editor increments.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Progressive disclosure (≤12 tools + trio + facets) | #124 | #123 | `tool_search`/`tool_describe`/`tool_invoke`; CI budget test. Advances #45/#32 discoverability. |
| Renderer umbrella | #37 | — | Phase 1→6 gating checklist; stays open through v0.11. |
| P1 sequence parser | #52→#53→#54→#55→#56 | chain | participants → arrows → cancel+Note → loop/alt/opt/par → activate/deactivate. |
| P1 flowchart parser | #57→#58→#59→#60 | chain | direction+node/edge → shapes → edge variants → subgraphs. |
| Split-pane scroll-sync toggle | #111 | — | Fuzzy match acceptable; split view only. |
| Print / print-to-PDF | #120 | — | Cmd/Ctrl+P + print CSS. Optional `export_pdf` MCP tool rides the parity surface. |

**Exit:**
- `tools/list` is pinned to ≤12; the long tail is reachable only via the discovery trio; the CI budget test blocks growth. (#124)
- Both parser chains produce ASTs for their Tier-0/1 fixtures. (#52–#60)
- Scroll-sync toggle and Cmd/Ctrl+P print work. (#111, #120)

---

## v0.7.0 — "Geometry"

**Theme:** Renderer Phase 2 (sequence layout + SVG with metadata round-trip),
the CLI↔MCP parity CI gate, non-Markdown viewing, and the scrybe-py foundation.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| CLI↔MCP parity gate (CI) | #125 | #123, #124 | Test: CLI subcommand set == MCP `tools/list` set; fill remaining CLI gaps. |
| P2 sequence layout | #61→#62→#63 | #56 | participant-x/statement-walk-y → activation stacks → notes/group-block. |
| P2 shared SVG metadata primitive | #64 | #61 | `<metadata>` root + sha256 (used by both builders). |
| P2 sequence SVG | #65→#66→#67 | #64, #63 | lifeline/header → arrow/activation → assemble (flips 10 seq trace tests). |
| Non-Markdown / git-diff viewing | #44 | — | All text types + git diffs, preview off; agents can open non-md. |
| scrybe-py Phase 1 | #6 | — | Usable library (Document, render, AST, content_id); thin PyO3. |

**Exit:**
- CI fails on any CLI↔MCP parity drift; every MCP tool has a CLI subcommand. (#125)
- The 10 sequence trace tests PASS at Tier 0+1. (#67)
- `import scrybe` gives Document/render/AST/content_id. (#6)

---

## v0.8.0 — "Sugiyama"

**Theme:** Renderer Phase 3 (full flowchart layout) + the scrybe-py plugin
protocol and reference plugins.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| P3 Sugiyama pipeline | #68→#69→#70→#71→#72→#73 | #60 | cycle removal → layer assignment → dummy nodes → barycenter → converge → coordinates. |
| scrybe-py Phase 2 plugin protocol | #7 | #6 | stdin/stdout tier + class-based tier. |
| scrybe-py Phase 3 reference plugins | #8 | #7 | word-count, docx (align with `scrybe-plugin-docx`). |

**Exit:**
- Flowchart layout produces stable coordinates for fixtures; gates Phase 4. (#73)
- Plugin protocol + two reference plugins run end-to-end. (#7, #8)

---

## v0.9.0 — "Raster"

**Theme:** Renderer Phase 4 (flowchart SVG, PNG rasterization, SSIM gate) + the
highest-value new plugin.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| P4 flowchart SVG | #74→#75 | #64, #73 | shape primitives → assemble (flips 10 flowchart trace tests). |
| P4 PNG rasterization | #76 | #75 | resvg/tiny-skia behind the `png` feature. |
| P4 SSIM grader + gate | #77→#78 | #76 | image-compare grader → enforce SSIM ≥ 0.92. |
| scrybe-quill inline AI authoring | #31 | — | Cmd+K, BYO OpenAI-compatible/Ollama, offline, no telemetry. |

**Exit:**
- Flowchart fixtures render to PNG and pass SSIM ≥ 0.92. (#78)
- Quill drafts/edits text against a local endpoint with no telemetry. (#31)

---

## v0.10.0 — "Bindings"

**Theme:** The renderer's PyO3 Python surface (Phase 5).

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| `render_to_svg` PyO3 | #79 | #67 | |
| `render_to_png` PyO3 (gated) | #80 | #76 | |
| Package surface | #81 | #79 | `__init__` / `py.typed` / `pyproject`. |
| pytest suite | #82 | #81 | `_RUST_AVAILABLE` guard. |

**Exit:** `pip install scrybe-mermaid-render` renders SVG (and PNG when the feature is on) from Python, tests green. (#82)

---

## v0.11.0 — "Conformance"

**Theme:** Renderer Phase 6 (upstream conformance, licensing, release flip) +
near-1.0 native install channels.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Upstream conformance fixtures | #83 | #78 | `gen_oracle.sh --with-upstream` + Tier-1 fixtures. |
| MPL-2.0 licensing | #84 | #76 | LICENSE-MPL-2.0 for resvg/usvg distribution. |
| Renderer release (closes umbrella) | #85 | #82, #83, #84 | publish=true + bump + tick #37 checklist. |
| Homebrew formula | #1 | — | macOS install. |
| Chocolatey package | #2 | — | Windows install. |

**Exit:**
- `scrybe-mermaid-render` published; **#37 umbrella closes** with its last child. (#85)
- `brew install` and `choco install` work against release artifacts. (#1, #2)

---

## v0.12.0 — "Frontier"

**Theme:** Speculative expansion toward 1.0 — the plugins furthest from the core
editing mission, riding the now-mature plugin protocol.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| scrybe-plugin-cad | #33 | (#7) | Parametric 3D from code-fenced blocks (v0). |
| scrybe-plugin-printer-control | #34 | (#7) | Drive 3D printers (v0). |

**Exit:** both plugins load via the plugin protocol and demo their v0 capability; 1.0 hardening has room. (#33, #34)

---

## Release criteria (every milestone)

Per `CLAUDE.md`'s zero-warning policy and `AGENTS.md`'s autonomy rules:

- `cargo clippy -- -D warnings` and `cargo fmt --check` clean; `just check` / `just test` green.
- Every behavioral fix carries a regression test (red before, green after).
- Each milestone's issues are closed via `Fixes #N` in merged PRs, or re-scoped with an issue comment.
- A version-bump PR (lock-step via `[workspace.package]`, once #128 lands) precedes the tag.
- On ship: move this file to `docs/roadmaps/ROADMAP-<version>.md` and start the next at the root (per the skill).

## Deliberately out (re-entry conditions)

- **Depend on modulex as an external crate.** We adopt its patterns natively
  (#122). Re-enters only if `scrybe-tools` is extracted as `modulex-plugin-scrybe`
  after modulex stabilizes (design §9).
- **Bidirectional swarm / NATS features (`scrybe-swarm`) and `scrybe-vcs` tool
  groups.** Facet stubs are reserved in v0.7 but the tool groups themselves are
  post-1.0 unless a concrete need lands an issue.
- **Windows named-pipe transport.** `cli_rpc.rs` is unix-only; `Transport` is the
  clean seam. Re-enters when a Windows user files the need (tracked as a follow-up
  to #108).
- **Deprecating Scrybe in favor of Ferrite / markdown-tui-explorer.** Gated by the
  #114/#115 spikes; re-enters only if a spike recommends wrapping over building.
