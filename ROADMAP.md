<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Scrybe ROADMAP — v0.4.0 → v1.0.0

**Current:** `v0.6.3` — tagged 2026-07-21 (release assets published 2026-07-29);
5 commits unreleased on main: the `scrybe view` TUI train (#229–#231), the
socket-quit fix (#233), and the UAT release-gate tier (#234).
**Milestone drift resolved 2026-07-29:** the re-compaction this file promised
after #132 has been executed — v0.4.0 and v0.10.0 closed, v0.8.0 retitled
"Bindings (py)", v0.9.0 retitled "Authoring", all descriptions reconciled to the
post-adoption reality (see "Re-compaction — 2026-07-29" below).
**Target:** `v1.0.0`, delivered across milestones **v0.5.0 → v0.12.0** (the
renderer epic closed early by adoption — #37/#85 closed 2026-07-21 — so 1.0
arrives sooner than the milestone count implies).
**Created:** 2026-07-13 · **Last reconciled:** 2026-07-29 (post-adoption backlog
scrub + re-compaction) · **Maintained per:** [`.claude/skills/repository-roadmap/SKILL.md`](.claude/skills/repository-roadmap/SKILL.md)

> **GitHub issues are the state; this document is the map.** Every work item
> carries a tracking issue number. When this document and GitHub disagree,
> **GitHub wins** — reconcile before trusting the prose.

## Re-compaction — 2026-07-29

A full-workflow backlog scrub audited all 34 open issues, every milestone, and
the release statements against the adopted-renderer reality (`mermaid-rs-renderer`
v0.3.1 via the `scrybe-mermaid-render` wrapper). Outcome — GitHub already
reflects all of it:

- **Renderer epic complete:** #37 (umbrella) and #85 (publish + pin) closed
  2026-07-21. A 5-issue spot-check of the #52–#76 "provided by the dependency"
  mass-close confirmed every sampled capability exists (one nuance: #64's
  `<metadata>` provenance is provided by the *wrapper*, by design).
- **Genuinely remaining renderer tail:** #84 (third-party license notice —
  **rides the next release**; corrected 2026-07-29: resvg/usvg relicensed to
  `Apache-2.0 OR MIT`, so the actual MPL-2.0 surface is the **desktop bundles
  only** — `cssparser`/`selectors`/`dtoa-short`/`option-ext` via the tauri/dirs
  chains; every wheel and crates.io package is MPL-free), #83 (golden-snapshot
  upgrade gate — the unshipped half of #85's pin+gate), #77/#78 (SSIM fidelity,
  moved v0.9 → v0.11), #79 (PyO3 surface, **absorbs #80–#82**, moved v0.10 → v0.8).
- **Milestones:** v0.4.0 closed (#32 → v0.5.0); v0.10.0 closed (merged into
  v0.8.0); v0.8.0 retitled **"Bindings (py)"**; v0.9.0 retitled **"Authoring"**
  (#147 joins #31); descriptions of v0.5–v0.7 and v0.11 rewritten — no milestone
  still describes the dead from-scratch "Renderer Phase N" plan.
- **Issue bodies re-scoped by comment** (2026-07-29): #1 #2 #6 #7 #8 #28 #32
  #77 #78 #83 #84 #124 #146 #166 #188.
- **New issues the adoption owed us:** #235 (preview renders Mermaid via CDN
  `mermaid@11`/KaTeX — offline-broken, diverges from native export; unify or
  vendor), #236 (dependabot/upstream watch for the 0.3.1 pin), #237 (Mermaid
  render smoke in the UAT release gate).

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
- **Windows live-editor IPC:** #222 adds the per-user named-pipe transport and
  native round-trip coverage while preserving the 0.6 JSON-RPC contract.
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
| **SVG → `<metadata>`** | the re-scoped renderer wrapper injects source + SHA256 into an SVG `<metadata>` element (namespace `https://scrybe.ai/ns/mermaid`) **after** the adopted engine renders — so provenance is Scrybe's, not the dependency's. | #37 (closed 2026-07-21; wrapper #171/#172 shipped) |
| **Agent surface** | `mermaid_to_png` (#121), `markdown_extract_and_render` (#126, `## Fig NN:` → named PNGs), inline render of embedded-source PNGs (#28), `mermaid-png` skill (#119). | v0.4–v0.5 |

This is the **ContentAddressable** philosophy applied to diagrams: the artifact
carries its own proof of what it is. **Adopting a third-party renderer does not
touch it** — we post-process the renderer's output to add the metadata. The
provenance layer was delivered across v0.4 (#119), v0.5 (#28 / #121 / #126), and
the SVG wrapper (#37, pulled forward to v0.5 and closed 2026-07-21), and is a
hard requirement of every renderer option.

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
  pin-and-gate / release tail. *(Since then: #85 closed 2026-07-21; #80–#82
  folded into #79 on 2026-07-29 — the surviving tail is #77/#78/#83/#84 + #79.)*

**Shipped:** the `scrybe-mermaid-render` wrapper — `render_svg` + Scrybe
`<metadata>` provenance (#171) and `render_png` via resvg (#172) — and **#119 is
closed**: `scrybe mermaid png` renders Mermaid → PNG with the source + UUID +
SHA-256 embedded in iTXt, driven end-to-end (`png` → `extract` → `verify`).
Because the renderer became a *validated dependency* (not a 34-issue build),
**#37 was pulled forward from v0.6 → v0.5 and closed with its last child #85 on
2026-07-21.** The v0.6–v0.11 renderer long pole is gone; the surviving tail is
#77/#78/#83/#84 (v0.11) and #79 (v0.8, absorbs #80–#82).

[#132]: https://github.com/hartsock/scrybe/issues/132

## Source plans

This roadmap *sequences* existing plans; it does not replace them.

- [`docs/TRIAGE.md`](docs/TRIAGE.md) — full 58-issue triage, epics, dispositions.
- [`docs/design/mcp-rebuild.md`](docs/design/mcp-rebuild.md) — the native-modulex MCP rebuild (epic **#122**).
- [`docs/design/cli-rpc.md`](docs/design/cli-rpc.md) — the CLI↔GUI socket protocol the rebuild unifies onto.
- [`docs/design/vision-conversational-editing.md`](docs/design/vision-conversational-editing.md) — the conversational-editing north star; feeds **#147** (addressability) → **#148** (grounding) → **#149** (patches), built on **#122**.
- [`docs/adr/0001-python-outside-rust-inside.md`](docs/adr/0001-python-outside-rust-inside.md) — the distribution philosophy.
- **#132** — the crate-adoption spike + re-scope for the renderer epic (**#37**). Supersedes the from-scratch `PLAN.md` (archived at the `archive/scrybe-mermaid-render-plan` tag; the branch is deleted). Both `scrybe-mermaid` (iTXt codec) and `scrybe-mermaid-render` (adopted-renderer wrapper) are shipped workspace members; `scrybe-swarm` / `scrybe-panels` remain in `experimental/`, not compiled.

## Epics at a glance

| Epic | Milestones | Tracking |
|---|---|---|
| MCP rebuild / CLI↔MCP parity (native-modulex) | v0.4–v0.7 | **#122** (epic), #108 #46 #121 #28 #15 #123 #124 #125 #126 #127 |
| **Conversational editing** (object IDs → grounding → patches) | v0.9+ | **#147** (v0.9) → #148 → #149 + [vision](docs/design/vision-conversational-editing.md); builds on **#122** |
| **Mermaid provenance** (source in PNG/SVG metadata) ★ | v0.4–v0.6 | #119 #28 #121 #126 + #37 wrapper |
| Mermaid renderer — **ADOPTED** `mermaid-rs-renderer` v0.3.1 (#132 ✓) | v0.5 (pulled fwd) | **#37 + #85 CLOSED 2026-07-21 — epic complete.** Tail: #77/#78 SSIM + #83 snapshots + #84 MPL (v0.11), #79 PyO3 (v0.8, absorbs #80–#82) |
| **Adoption follow-through** (preview unification, upstream watch, UAT smoke) | v0.7, v0.11 | #235 #237 (v0.7) · #236 (v0.11) |
| Human editor UX | v0.4–v0.7 | #32 #15 #109 #45 #111 #120 #44 |
| scrybe-py library | v0.7–v0.8 | #6 #7 #8 #79 |
| Packaging / distribution / CI guardrails | v0.4, v0.11 | #116 #1 #2 #128 #236 |
| New feature plugins (v0) | v0.9, v0.12 | #31 #33 #34 |
| Strategic explores (resolved) | v0.4 | #114 #115 → both **build-ours** |
| **scrybe-tui viewer** (terminal lens on the AST) | v0.6 | **#154** (delivered viewer #155–#158; harness #159); #162–#164 shipped (on main, unreleased) · #165 closed-superseded · #166 #167 backlog |
| **Install / upgrade** — `scrybe upgrade` + npm shim | v0.5, v0.11 | **#146** (Part A shipped #151) |

### Conversational editing arc (post-#122)

The MCP rebuild (**#122**) is the platform; the next arc turns Scrybe into a
*conversational editor* — the document is the shared state and conversation
generates structured edits, rather than a chatbot bolted onto an editor. See
[`docs/design/vision-conversational-editing.md`](docs/design/vision-conversational-editing.md).
Three epics, in dependency order, starting v0.9 (#147 is milestoned
"v0.9.0 — Authoring"; #148/#149 stay unmilestoned until #147 lands):

1. **#147 — object addressability.** Stable IDs over the AST (derived for named
   structure + a `.scrybe/` sidecar for fine anchors; embedded anchors opt-in).
   Foundation — everything else needs it.
2. **#148 — reference resolution / grounding.** Resolve "Figure 2" to an object
   handle *before* the model runs; named references deterministic now, full deixis
   later (multimodal).
3. **#149 — patch-oriented editing.** Edits arrive as reviewable, revisioned patches
   (on `ContentAddressable` + `scrybe-vcs`), not blind overwrites.

Detailed per-milestone placement lands as these are scheduled; #148/#149 slot in
as #147 delivers.

---

## v0.4.0 — "Keystone" (SHIPPED 2026-07-14 · milestone closed 2026-07-29, #32 → v0.5)

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
> **2026-07-29:** #32 (content-root-relative copy remainder) moved here from the
> now-closed v0.4.0 milestone.

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

## v0.7.0 — "Geometry → Parity & unification"

**Theme:** The CLI↔MCP parity gate, preview/export renderer unification, and the
scrybe-py foundation remainder. (The former "close #61–#67" row was executed
2026-07-18 — provided by the adopted crate; #44 non-Markdown viewing is closed.)

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| CLI↔MCP parity gate (CI) | #125 | — | Test: CLI subcommand set == MCP `tools/list` set; fill remaining CLI gaps. (#123 closed; #124 rides v0.6.) |
| Preview render unification / offline-proofing | #235 | — | Preview = CDN `mermaid@11` + KaTeX today; unify on `scrybe-mermaid-render` or vendor the JS. Must render offline. |
| UAT renderer smoke | #237 | — | `mermaid png` / `render` / MCP `mermaid_to_png` provenance smoke in the release gate. |
| scrybe-py Phase 1 remainder | #6 | — | `Document.ast()` + pytest wired into CI; the rest of Phase 1 shipped 0.6.x. |

**Exit:**
- CI fails on any CLI↔MCP parity drift; every MCP tool has a CLI subcommand. (#125)
- The app preview renders diagrams + math with networking disabled; the UAT gate covers the render paths. (#235, #237)
- `import scrybe` exposes `Document.ast()`; pytest runs in CI. (#6)

---

## v0.8.0 — "Bindings (py)" (retitled from "Sugiyama")

**Theme:** The scrybe-py plugin protocol, reference plugins, and the Mermaid PyO3
surface. (The former "Renderer Phase 3" layout issues #68–#73 were closed
2026-07-18 — Sugiyama layout is internal to the adopted crate.)

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| scrybe-py Phase 2 plugin protocol (Tier 2) | #7 | #6 | Tier 1 stdin/stdout shipped on main; the class-based `scrybe.plugin` tier remains. |
| scrybe-py Phase 3 reference plugins | #8 | #7 | word-count (class-based); the docx plugin already shipped — README section + mmdc→native decision remain. |
| Mermaid PyO3 surface | #79 | — | Absorbs #80–#82 (2026-07-29): `render_mermaid_svg`/`render_mermaid_png` inside scrybe-py, provenance included. |

**Exit:**
- Plugin protocol Tier 2 + the class-based word-count plugin run end-to-end. (#7, #8)
- `scrybe.render_mermaid_svg`/`render_mermaid_png` work from Python with provenance, tests green. (#79)

---

## v0.9.0 — "Authoring" (retitled from "Raster")

**Theme:** Inline AI authoring + the conversational-editing foundation. (The
former "Renderer Phase 4" rows are gone: #74/#75 closed 2026-07-18; #76 closed —
`render_png` shipped in the wrapper; #77/#78 SSIM moved to v0.11 Conformance.)

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| scrybe-quill inline AI authoring | #31 | — | Cmd+K, BYO OpenAI-compatible/Ollama, offline, no telemetry. |
| Object addressability | #147 | — | Stable IDs over the AST; foundation of the conversational arc (#148/#149 follow as it lands). |

**Exit:**
- Quill drafts/edits text against a local endpoint with no telemetry. (#31)
- Stable object IDs resolvable over the MCP surface; #148 unblocked. (#147)

---

## v0.10.0 — CLOSED 2026-07-29 (merged into v0.8.0)

The PyO3 quartet #79–#82 folded into a single re-scoped **#79**, which lives in
**"v0.8.0 — Bindings (py)"** — the Python surface ships inside `scrybe-py`, not
as a standalone `scrybe-mermaid-render` wheel (the release matrix publishes no
such wheel). The "Renderer Phase 5" this milestone was named for was superseded
by the adoption (#132).

---

## v0.11.0 — "Conformance & distribution"

**Theme:** Pin-and-gate the **adopted** renderer + near-1.0 native install
channels. (#85 — publish + pin — and the #37 umbrella closed 2026-07-21; what
remains is the drift-detection half of the gate and two publish secrets.)

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| Golden-snapshot upgrade gate | #83 | — | Snapshot the adopted crate's SVG per diagram type; a version bump ⇒ reviewable diff. The unshipped half of #85's "pin + gate". |
| SSIM fidelity grader | #77 | — | Report-only SSIM vs reference renders (moved from v0.9). |
| SSIM threshold enforcement | #78 | #77 | Calibrate the threshold to real scores; consider merging into #77. |
| MPL-2.0 distribution notice | #84 | — | **Pulled forward — rides the next release.** Corrected 2026-07-29: resvg/usvg are now `Apache-2.0 OR MIT`; the MPL-2.0 code in shipped artifacts is `cssparser`/`selectors`/`dtoa-short`/`option-ext`, statically linked into the **desktop bundles** (wheels/crates are MPL-free). |
| Dependabot / upstream watch | #236 | — | The trigger for #83's gate; no dependabot.yml exists at all today. |
| Homebrew tap push | #1 | — | Provision `TAP_GITHUB_TOKEN` + one Apple Silicon smoke; the cask channel is already built. |
| Chocolatey push | #2 | — | Provision `CHOCO_API_KEY`; the nupkg is already staged each release. |

**Exit:**
- A `mermaid-rs-renderer` bump produces a reviewable (and, below threshold, failing) snapshot/SSIM diff. (#83, #77/#78, #236)
- MPL-2.0 text + third-party notice ship in the desktop app bundles (the only artifacts carrying MPL code) — done at the next release, not held to v0.11. (#84)
- `brew install --cask` resolves the current release; `choco install scrybe` is live on the community feed. (#1, #2)

---

## v0.12.0 — "Frontier"

**Theme:** Speculative expansion toward 1.0 — the plugins furthest from the core
editing mission, riding the now-mature plugin protocol.

| Item | Issue | Blocked by | Notes |
|---|---|---|---|
| scrybe-plugin-cad | #33 | (#7) | Parametric 3D from code-fenced blocks (v0). |
| scrybe-plugin-printer-control | #34 | (#7) | Drive 3D printers (v0). |

**Exit:** both plugins load via the plugin protocol and demo their v0 capability; 1.0 hardening has room. (#33, #34)

> **Adoption dividend — realized.** #52–#76 closed 2026-07-17/18 (provided by
> the dependency; spot-verified 2026-07-29), #37/#85 closed 2026-07-21, and the
> promised re-compaction executed 2026-07-29: v0.4.0/v0.10.0 closed, v0.8.0 →
> "Bindings (py)", v0.9.0 → "Authoring", the SSIM/conformance tail consolidated
> in v0.11. 1.0 now gates on parity (#125), the Python surface (#6–#8, #79),
> authoring (#31, #147–#149), and conformance/distribution (v0.11) — not on
> renderer construction.

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
- **Deprecating Scrybe in favor of Ferrite / markdown-tui-explorer.** Both
  #114/#115 spikes resolved **build-ours** — neither exposes an MCP/IPC surface
  to host Scrybe's live-buffer co-editing thesis. Re-enters only on a new spike.
