<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Scrybe ROADMAP — v0.4.0 → v1.0.0

**Current:** `v0.4.0` — "Keystone", shipped 2026-07-14 (lock-step
`[workspace.package]` version, #128/#144). **Active milestone: `v0.5.0` — "Parity".**
**Target:** `v1.0.0`, delivered across milestones **v0.4.0 → v0.12.0** (the
renderer epic now **adopts** an upstream crate — see below — so the back half
compresses; 1.0 arrives sooner than the milestone count implies).
**Created:** 2026-07-13 · **Last reconciled:** 2026-07-17 · **Maintained per:** [`.claude/skills/repository-roadmap/SKILL.md`](.claude/skills/repository-roadmap/SKILL.md)

> **GitHub issues are the state; this document is the map.** Every work item
> carries a tracking issue number. When this document and GitHub disagree,
> **GitHub wins** — reconcile before trusting the prose.

## Reconciliation — 2026-07-17

This roadmap had drifted from GitHub (the header still read `v0.2.0`). Verified
against merged code + PRs and reconciled:

- **`v0.4.0` "Keystone" shipped** (CHANGELOG, #128/#144). The `v0.4.0` section
  below is now historical; live issue state is on GitHub.
- **Closed as verified-done** (evidence in the closing PR): #108 (open→tab
  appears; #134/#142/#143 + regression test), #45 (vim keys/themes; #105), #15
  (fs-watcher reload; #11/#143), #136 (word-wrap; shipped 0.4.0/#138), and the
  resolved spikes #114/#115 (both build-ours).
- **`#120` (print)** shipped early (#153) though milestoned `v0.6.0`.
- **New threads given a home** (were untracked): the **scrybe-tui viewer #154**
  (re-scoped to the *delivered* viewer; remaining checklist split into follow-ups
  #162–#167) and the **install/upgrade epic #146** (Part A shipped, #151).
- **Windows CI**: the external nightly Windows check (#135) is now mirrored by a
  `test-rust-windows` job in `ci.yml`, so Windows breakage is caught at PR time.
- **Renderer adopted (moved up from v0.6):** the #132 spike was *run* and
  **passed** → `mermaid-rs-renderer` v0.3.1 adopted; the `scrybe-mermaid-render`
  wrapper shipped (#171 SVG+provenance, #172 render_png) and **#119 is closed**
  (`scrybe mermaid png` + the `mermaid-png` skill). #52–#76 closed as
  provided-by-dependency; #77–#85 re-scoped; **#37 pulled forward to v0.5**. See
  "The renderer epic" below.
- **#122 MCP rebuild in progress:** `scrybe-tools` registry now carries
  `render` / `lint` / `mermaid_to_png` (#169/#170/#174); Phase 2 (dispatch
  unification via `scrybe-rpc`) is next.
- **Still genuinely open in `v0.4.0`**: #32 (content-root-relative path copy —
  only full-path shipped).

## Ground-truth protocol

```bash
# live state of any milestone's issues (never trust the doc's checkboxes)
gh issue list --repo hartsock/scrybe --state all \
  --search "119 108 37 132 121 126 28 122" \
  --json number,title,state,closedAt

# a single issue
gh issue view <N> --repo hartsock/scrybe --json number,title,state,closedAt
```

A **milestone is done** when every issue in its tables is closed (or carries a
comment re-scoping it out). Report progress as `closed / total` per milestone;
flag any issue whose GitHub state contradicts this document for a roadmap-update
PR. Edit this file only for **structural** change (items added/removed/re-phased,
exit criteria changed) — issue *state* lives in GitHub, not here.

---

## ★ Headline result: Mermaid provenance (the source lives inside the image)

Scrybe's signature, differentiating result — the one capability no editor or
renderer we surveyed provides, and the one we keep **100% in-house** even as we
adopt an external renderer (next section): **every diagram Scrybe emits carries
its own Mermaid source, losslessly, inside the image file.**

| Surface | How | Tracking |
|---|---|---|
| **PNG → iTXt** | `scrybe-mermaid` embeds Mermaid source + UUID + SHA256 in a PNG `iTXt` chunk; `extract` recovers it. A rendered PNG is fully round-trippable — edit the diagram later without hunting for the `.md`. | shipped codec + #119 #121 #126 #28 |
| **SVG → `<metadata>`** | the re-scoped renderer wrapper injects source + SHA256 into an SVG `<metadata>` element (namespace `https://scrybe.ai/ns/mermaid`) **after** the adopted engine renders — so provenance is Scrybe's, not the dependency's. | #37 (v0.6) |
| **Agent surface** | `mermaid_to_png` (#121), `markdown_extract_and_render` (#126, `## Fig NN:` → named PNGs), inline render of embedded-source PNGs (#28), `mermaid-png` skill (#119). | v0.4–v0.5 |

This is the **ContentAddressable** philosophy applied to diagrams: the artifact
carries its own proof of what it is. **Adopting a third-party renderer does not
touch it** — we post-process the renderer's output to add the metadata. The
provenance layer is delivered across v0.4 (#119), v0.5 (#28 / #121 / #126), and
the v0.6 SVG wrapper (#37), and is a hard requirement of every renderer option.

---

## The renderer epic (#37): adopted, not built

**Decision (2026-07-13, adversarially verified — [#37 comment](https://github.com/hartsock/scrybe/issues/37) · gate [#132]):**
the pure-Rust Mermaid→SVG problem #37 was scoped to build **has been solved
upstream.** We **adopt** [`mermaid-rs-renderer`](https://crates.io/crates/mermaid-rs-renderer)
(MIT, `render(&str) -> Result<String>` → SVG, PNG via resvg; fallback
[`merman`](https://crates.io/crates/merman), MIT/Apache) and collapse
`scrybe-mermaid-render` from a 34-issue from-scratch renderer into a **thin
wrapper**:

```
source → mermaid-rs-renderer::render → inject Scrybe <metadata> (sha256+source) → resvg → PNG
```

**[#132] spike — RUN and PASSED (2026-07-17).** A hands-on bake-off confirmed
`mermaid-rs-renderer` **v0.3.1** renders the MVP flowchart + sequence corpus to
valid SVG (pure Rust, no `mmdc`). Adopted; `merman` stays a documented fallback
only (unresolved GitHub "license: other"). Disposition **APPLIED**:

- **Closed — provided by the dependency:** #52–#76 (lexer, parsers, Sugiyama /
  layout, SVG emit, PNG-via-resvg) and draft **PR #99**.
- **Kept, re-scoped to wrapper bits:** #77–#85 — conformance-track the *dependency*
  (pin + golden snapshots + optional SSIM), PyO3 over the wrapper, and the
  pin-and-gate / release tail.

**Shipped:** the `scrybe-mermaid-render` wrapper — `render_svg` + Scrybe
`<metadata>` provenance (#171) and `render_png` via resvg (#172) — and **#119 is
closed**: `scrybe mermaid png` renders Mermaid → PNG with the source + UUID +
SHA-256 embedded in iTXt, driven end-to-end (`png` → `extract` → `verify`).
Because the renderer is now a *validated dependency* (not a 34-issue build),
**#37 is pulled forward from v0.6 → v0.5**; it closes with its last child (#85,
publish the wrapper). The v0.6–v0.11 renderer long pole is gone.

[#132]: https://github.com/hartsock/scrybe/issues/132

## Source plans

This roadmap *sequences* existing plans; it does not replace them.

- [`docs/TRIAGE.md`](docs/TRIAGE.md) — full 58-issue triage, epics, dispositions.
- [`docs/design/mcp-rebuild.md`](docs/design/mcp-rebuild.md) — the native-modulex MCP rebuild (epic **#122**).
- [`docs/design/cli-rpc.md`](docs/design/cli-rpc.md) — the CLI↔GUI socket protocol the rebuild unifies onto.
- [`docs/design/vision-conversational-editing.md`](docs/design/vision-conversational-editing.md) — the conversational-editing north star; feeds **#147** (addressability) → **#148** (grounding) → **#149** (patches), built on **#122**.
- [`docs/adr/0001-python-outside-rust-inside.md`](docs/adr/0001-python-outside-rust-inside.md) — the distribution philosophy.
- **#132** — the crate-adoption spike + re-scope for the renderer epic (**#37**). Supersedes the from-scratch `PLAN.md` on the `feat/scrybe-mermaid-render` branch. Today only `scrybe-mermaid` (the iTXt PNG codec) ships; `scrybe-swarm` / `scrybe-panels` are in `experimental/`, not shipped members.

## Epics at a glance

| Epic | Milestones | Tracking |
|---|---|---|
| MCP rebuild / CLI↔MCP parity (native-modulex) | v0.4–v0.7 | **#122** (epic), #108 #46 #121 #28 #15 #123 #124 #125 #126 #127 |
| **Conversational editing** (object IDs → grounding → patches) | v0.8–v0.10 | **#147** #148 #149 + [vision](docs/design/vision-conversational-editing.md); builds on **#122** |
| **Mermaid provenance** (source in PNG/SVG metadata) ★ | v0.4–v0.6 | #119 #28 #121 #126 + #37 wrapper |
| Mermaid renderer — **ADOPTED** `mermaid-rs-renderer` v0.3.1 (#132 ✓) | v0.5 (pulled fwd) | **#37**; wrapper #171/#172 shipped, #119 closed; #52–#76 closed, #77–#85 re-scoped |
| Human editor UX | v0.4–v0.7 | #32 #15 #109 #45 #111 #120 #44 |
| scrybe-py library | v0.7–v0.8 | #6 #7 #8 |
| Packaging / distribution / CI guardrails | v0.4, v0.11 | #116 #1 #2 #128 |
| New feature plugins (v0) | v0.9, v0.12 | #31 #33 #34 |
| Strategic explores (resolved) | v0.4 | #114 #115 → both **build-ours** |
| **scrybe-tui viewer** (terminal lens on the AST) | v0.6 | **#154** (delivered viewer #155–#158; harness #159); follow-ups #162 #163 #164 (v0.6) · #165 #166 #167 (backlog) |
| **Install / upgrade** — `scrybe upgrade` + npm shim | v0.5, v0.11 | **#146** (Part A shipped #151) |

### Conversational editing arc (post-#122)

The MCP rebuild (**#122**) is the platform; the next arc turns Scrybe into a
*conversational editor* — the document is the shared state and conversation
generates structured edits, rather than a chatbot bolted onto an editor. See
[`docs/design/vision-conversational-editing.md`](docs/design/vision-conversational-editing.md).
Three epics, in dependency order, ~v0.8–v0.10:

1. **#147 — object addressability.** Stable IDs over the AST (derived for named
   structure + a `.scrybe/` sidecar for fine anchors; embedded anchors opt-in).
   Foundation — everything else needs it.
2. **#148 — reference resolution / grounding.** Resolve "Figure 2" to an object
   handle *before* the model runs; named references deterministic now, full deixis
   later (multimodal).
3. **#149 — patch-oriented editing.** Edits arrive as reviewable, revisioned patches
   (on `ContentAddressable` + `scrybe-vcs`), not blind overwrites.

Detailed per-milestone placement lands as these are scheduled; they slot after the
v0.4–v0.7 rebuild.

---

## v0.4.0 — "Keystone" (SHIPPED 2026-07-14)

**Theme:** Make the MCP actually work and ship the priority Mermaid-PNG
provenance skill (★), behind privacy guardrails, with the strategic spikes
resolved up front so later epics can pivot before heavy investment.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Mermaid→PNG skill (iTXt source+uuid+sha256) ★ | **#119** | — | **PRIORITY.** `/mermaid-png` skill largely exists; deliver the `mermaid_to_png` foundation + `mermaid-png` SKILL.md. |
| MCP rebuild epic opened | #122 | — | Umbrella; create `scrybe-tools`, `ToolSpec`/`Facet`/`Transport` (Headless first); port pure tools. |
| Fix `open` → dispatch via `scrybe-rpc` (root cause) | #108 | — | `open` emits `scrybe://cli-open` to the live app; delete the MCP-private `Workspace`. Fix JSON-RPC top-level `error` + `tools/call` `isError`. Add `--tools`/`--probe`. |
| Path-bar copy affordance | #32 | — | *verify-may-be-done* — verify full-path vs relative-to-content-root copy; close remainder. |
| Secret-scan CI guardrail | #116 | — | gitleaks (free binary) + internal-specifics linter on GitHub-hosted runners. |
| Release hygiene: lock-step version + v0.3.0 | #128 | — | `[workspace.package]` version; tag or fold v0.3.0 before the next release. |
| Spike: wrap Ferrite? | #114 | — | **Resolved: build-ours.** No MCP/IPC surface to host our thesis. Idea bank only. |
| Spike: wrap markdown-tui-explorer? | #115 | — | **Resolved: build-ours** (editor); its renderer angle → adopt `mermaid-rs-renderer` (see #37/#132). |

**Exit:**
- `mcp open <file>` makes a tab **actually appear** in the running app (headless fallback when no app); an agent can distinguish success from failure via `isError`. (#108)
- #119 skill renders Mermaid→PNG with embedded source+uuid+sha256, round-trips via `extract`. ★
- Secret-scan CI is green on every PR. (#116)
- #114/#115 spikes resolved (both build-ours; renderer → adopt). (#114, #115)
- A written decision on the v0.3.0 tag + `[workspace.package]` landed. (#128)

---

## v0.5.0 — "Parity"

**Theme:** Build out true CLI↔MCP parity on the unified `scrybe-rpc` foundation
(data contract + the Mermaid provenance tools ★), and land the first batch of
independent editor quality-of-life increments.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Atomic `mermaid_to_png` MCP tool ★ | #121 | #108 | Description doubles as an embedded agent prompt ("never raw mmdc"). Embeds source+uuid+sha256. |
| `markdown_extract_and_render` tool ★ | #126 | #108, #121 | `## Fig NN: Title` → `YYYY-MM-DD_Doc_Fig-NN_Title.png`, all embedded. |
| `list_tabs` over the live socket | #46 | #108 | Agent sees the real tab set. |
| Inline render of embedded-source PNGs ★ | #28 | — | `![alt](x.png)` with iTXt `mermaid-source` renders like a fenced block (rides live `reload`). |
| Versioned typed data contract | #123 | #108 | Every tool emits a stable `data` payload; `--format data`. Agents never parse prose. |
| `mcp-editing` agent skill | #127 | — | The safe `open→read→find→edit→render/lint` loop + reload discipline. |
| fs-watcher reload | #15 | — | Clean = silent reload; dirty = prompt. |
| Tab context menu (bulk close) | #109 | — | Close to the Right / Left / Others. |
| Vim keybinding depth | #45 | — | *verify-may-be-done* — verify search/regex-replace; close remainder. |

**Exit:**
- Agents call `mermaid_to_png` / `markdown_extract_and_render` from the MCP surface (no bare `mmdc`); every PNG carries embedded source. ★ (#121, #126)
- `list_tabs` returns the live tab set with paths + dirty flags. (#46)
- Every tool returns a versioned `data` payload on both CLI and MCP; golden `--tools` snapshot test passes. (#123)
- Each MCP tool built this milestone ships **with** its CLI subcommand (parity-by-construction; the CI *gate* lands v0.7/#125).
- `mcp-editing` and `repository-roadmap` skills are installable. (#127)

> **Reconciliation:** the GitHub `v0.5.0` milestone also carries items absent
> from the table above — `#137` (tab-reorder drag-and-drop, still open) plus
> `#136` (word-wrap) and the bugs `#140`/`#141`, which were **pulled forward and
> shipped in 0.4.0** (#138/#143). Trust the milestone, not this table:
> `gh issue list --repo hartsock/scrybe --milestone "v0.5.0 — Parity" --state all`.

---

## v0.6.0 — "Grammar → Adopt"

> **Renderer rows below are historical (done early).** The #132 spike passed and
> the adoption was pulled forward to v0.5: the `scrybe-mermaid-render` wrapper
> shipped (#171/#172), #119 closed, #52–#76 closed, #77–#85 re-scoped, #37 → v0.5.
> See "The renderer epic (#37): adopted, not built" above. The MCP progressive-
> disclosure + editor items remain v0.6.

**Theme:** Deliver the Mermaid renderer by **adopting** a pure-Rust crate (not
building it), inject Scrybe's SVG provenance ★, finish MCP progressive
disclosure, and two editor increments.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Fidelity spike + adopt decision | **#132** | — | Bake-off `mermaid-rs-renderer` vs `merman` on the MVP corpus (scrybe-panels); pin a version. **Gate for the whole renderer epic.** |
| `scrybe-mermaid-render` thin wrapper (MVP) + SVG `<metadata>` ★ | #37 | #132 | `source → render() → inject <metadata> sha256+source → resvg→PNG`. Delivers flowchart + sequence. |
| Close build issues (provided by dependency) | #52–#60 | #132 | On spike-Pass: parsers are the crate's job — close. (Also #61–#75 as they come up in v0.7–v0.9.) **Closes PR #99.** |
| Progressive disclosure (≤12 tools + trio + facets) | #124 | #123 | `tool_search`/`tool_describe`/`tool_invoke`; CI budget test. |
| Split-pane scroll-sync toggle | #111 | — | Fuzzy match acceptable; split view only. |
| Print / print-to-PDF | #120 | — | Cmd/Ctrl+P + print CSS. Optional `export_pdf` MCP tool rides the parity surface. |

**Exit:**
- #132 concluded; the `scrybe-mermaid-render` wrapper renders MVP flowchart+sequence via the adopted crate, with Scrybe's `<metadata>` provenance injected. ★ (#37, #132)
- On spike-Pass, #52–#60 and **PR #99** are closed as provided-by-dependency.
- `tools/list` pinned ≤12 via the discovery trio; CI budget test blocks growth. (#124)
- Scroll-sync toggle and Cmd/Ctrl+P print work. (#111, #120)

---

## v0.7.0 — "Geometry → Wrapper completion"

**Theme:** Complete the renderer wrapper (PNG + the remaining parity closes), the
CLI↔MCP parity gate, non-Markdown viewing, and the scrybe-py foundation.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| CLI↔MCP parity gate (CI) | #125 | #123, #124 | Test: CLI subcommand set == MCP `tools/list` set; fill remaining CLI gaps. |
| Close build issues (provided by dependency) | #61–#67 | #132 | Sequence layout + SVG assembly are the crate's job — close on spike-Pass. |
| Non-Markdown / git-diff viewing | #44 | — | All text types + git diffs, preview off; agents can open non-md. |
| scrybe-py Phase 1 | #6 | — | Usable library (Document, render, AST, content_digest); thin PyO3. |

**Exit:**
- CI fails on any CLI↔MCP parity drift; every MCP tool has a CLI subcommand. (#125)
- #61–#67 closed as provided-by-dependency (spike-Pass). (#132)
- `import scrybe` gives Document/render/AST/content_digest. (#6)

---

## v0.8.0 — "Bindings & plugins (py)"

**Theme:** The scrybe-py plugin protocol and reference plugins. (Former renderer
Phase 3 layout issues #68–#73 close here as provided-by-dependency, spike-Pass.)

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Close build issues (provided by dependency) | #68–#73 | #132 | Sugiyama layout is internal to the adopted crate — close on spike-Pass. |
| scrybe-py Phase 2 plugin protocol | #7 | #6 | stdin/stdout tier + class-based tier. |
| scrybe-py Phase 3 reference plugins | #8 | #7 | word-count, docx (align with `scrybe-plugin-docx`). |

**Exit:**
- #68–#73 closed as provided-by-dependency. (#132)
- Plugin protocol + two reference plugins run end-to-end. (#7, #8)

---

## v0.9.0 — "Raster & authoring"

**Theme:** The renderer wrapper's **PNG** path (kept, re-scoped) + the
highest-value new plugin.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Close build issues (provided by dependency) | #74, #75 | #132 | Flowchart SVG assembly is the crate's job — close on spike-Pass. |
| Renderer PNG via resvg (kept, re-scoped) | #76–#78 | #37 | Rasterize the **metadata-bearing** SVG via Scrybe's resvg/tiny-skia; SSIM sanity vs `mmdc`. |
| scrybe-quill inline AI authoring | #31 | — | Cmd+K, BYO OpenAI-compatible/Ollama, offline, no telemetry. |

**Exit:**
- Wrapper rasterizes provenance-bearing SVG → PNG; SSIM sanity holds. ★ (#76–#78)
- #74/#75 closed as provided-by-dependency. (#132)
- Quill drafts/edits text against a local endpoint with no telemetry. (#31)

---

## v0.10.0 — "Python surface"

**Theme:** The renderer wrapper's **PyO3** surface (kept, re-scoped) — wrap the
adopted crate for Python.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| `render_to_svg` PyO3 (wraps the crate) | #79 | #37 | Python calls through the wrapper (adopted crate + provenance). |
| `render_to_png` PyO3 (gated) | #80 | #76 | |
| Package surface | #81 | #79 | `__init__` / `py.typed` / `pyproject`. |
| pytest suite | #82 | #81 | `_RUST_AVAILABLE` guard. |

**Exit:** `pip install scrybe-mermaid-render` renders SVG (and PNG when the feature is on) from Python via the adopted crate + provenance, tests green. ★ (#82)

---

## v0.11.0 — "Conformance & distribution"

**Theme:** Renderer **conformance = pin & gate the dependency** (kept,
re-scoped) + near-1.0 native install channels.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Pin + golden-SVG snapshots vs upstream | #83 | #78 | **Re-scoped:** snapshot the adopted crate's output; track `mmdc`-parity per type. |
| MPL-2.0 licensing (resvg/usvg + deps) | #84 | #76 | LICENSE-MPL-2.0 for the resvg/usvg distribution surface. |
| Dependency upgrade gate + release (closes #37) | #85 | #82, #83, #84 | **Re-scoped:** pin `mermaid-rs-renderer` version + upgrade gate; publish the wrapper; tick #37 checklist. |
| Homebrew formula | #1 | — | macOS install. |
| Chocolatey package | #2 | — | Windows install. |

**Exit:**
- `scrybe-mermaid-render` wrapper published; dependency pinned + upgrade-gated; **#37 umbrella closes**. (#85)
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

> **Adoption dividend:** because #52–#75 close as the dependency provides them
> (rather than being built one milestone at a time), the renderer stops gating
> v0.8–v0.11. Once #132 passes, expect the tail (scrybe-py, quill, packaging,
> plugins) to pull forward and **1.0 to arrive ahead of the v0.12 milestone
> count.** That re-compaction is a follow-up roadmap PR executed *after* #132,
> so the doc and GitHub milestones stay in lock-step until then.

---

## Release criteria (every milestone)

Per `CLAUDE.md`'s zero-warning policy and `AGENTS.md`'s autonomy rules:

- `cargo clippy -- -D warnings` and `cargo fmt --check` clean; `just check` / `just test` green.
- Every behavioral fix carries a regression test (red before, green after).
- Each milestone's issues are closed via `Fixes #N` in merged PRs, or re-scoped with an issue comment.
- A version-bump PR (lock-step via `[workspace.package]`, once #128 lands) precedes the tag.
- On ship: move this file to `docs/roadmaps/ROADMAP-<version>.md` and start the next at the root (per the skill).

## Deliberately out (re-entry conditions)

- **Build the Mermaid renderer from scratch.** Superseded by adopting
  `mermaid-rs-renderer` (#37 / #132). Re-enters only if the #132 fidelity spike
  shows both `mermaid-rs-renderer` and `merman` are unusable — then #52–#85
  revert to build-ours.
- **Depend on modulex as an external crate.** We adopt its patterns natively
  (#122). Re-enters only if `scrybe-tools` is extracted as `modulex-plugin-scrybe`
  after modulex stabilizes (design §9).
- **Bidirectional swarm / NATS features (`scrybe-swarm`) and `scrybe-vcs` tool
  groups.** Facet stubs are reserved in v0.7 but the tool groups themselves are
  post-1.0 unless a concrete need lands an issue.
- **Windows named-pipe transport.** `cli_rpc.rs` is unix-only; `Transport` is the
  clean seam. Re-enters when a Windows user files the need (tracked as a follow-up
  to #108).
- **Deprecating Scrybe in favor of Ferrite / markdown-tui-explorer.** Both
  #114/#115 spikes resolved **build-ours** — neither exposes an MCP/IPC surface
  to host Scrybe's live-buffer co-editing thesis. Re-enters only on a new spike.
