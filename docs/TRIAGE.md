<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Scrybe Issue Triage — 2026-07-13

Full triage of the **58 open issues** in `hartsock/scrybe`, clustered into
epics and dispositioned toward the release plan in [`ROADMAP.md`](../ROADMAP.md).

> **Ground truth is GitHub, not this file.** Every row cites an issue number;
> when this document and GitHub disagree, GitHub wins. Reconcile with:
> `gh issue list --repo hartsock/scrybe --state all --json number,title,state`.
> Sequencing and exit criteria live in `ROADMAP.md`; the MCP epic's design lives
> in [`docs/design/mcp-rebuild.md`](design/mcp-rebuild.md).

## Version reconciliation (read this first)

The version story and git reality disagree — surfaced here rather than papered
over:

- **Pushed release tags:** `v0.1.0`, `v0.1.1`, `v0.2.0`. The last *tagged*
  release is **v0.2.0**.
- **Crate versions today:** `0.2.1-dev` (per-crate; there is no
  `[workspace.package]` block, so versions are hand-maintained per crate).
- Commit `161ae4e` *"chore(release): bump workspace to 0.3.0 (#100)"* exists,
  **but there is no `v0.3.0` tag** and later commits still read `0.2.x`. The
  0.3.0 bump was never tagged or released.
- **What actually merged since v0.2.0** (the de-facto, untagged "0.3.x" line):
  path bar + theme sync + view cycle + Vim toggle + Word export + MCP UI-parity
  tools (#105), sidecar autosave + close/open UX (#103, #102), Word export
  packaging (#107), agent risk/autonomy docs (#101).

**Roadmap treatment:** we honor the chosen `v0.4 → v0.12` numbering (v0.4.0 is
the next feature release). A **release-hygiene tracking issue** is filed in v0.4
to add a `[workspace.package]` lock-step version and reconcile the missing
`v0.3.0` tag. See `ROADMAP.md` → v0.4.0.

## Epics

| Epic | Issues | One-liner |
|---|---|---|
| **MCP rebuild / CLI↔MCP parity** (native-modulex) | #108, #46, #121 | Unify the two diverged IPC paths behind one `ToolSpec` registry over `scrybe-rpc`; adopt modulex's data-contract + progressive-disclosure + feature-gated seam. **Keystone.** |
| **Mermaid-PNG round-trip + agent skills** | #119, #28 | Lossless Mermaid↔PNG (iTXt source+uuid+sha256), LLM-callable skill, inline render of embedded-source PNGs. #119 is the **v0.4 priority**. |
| **Mermaid renderer** (#37) — **ADOPT, don't build** | #37 + #52–#85 (34) + #132 | **Re-scoped 2026-07-13:** adopt the pure-Rust crate `mermaid-rs-renderer` (MIT) instead of building `mmdc` from scratch. `scrybe-mermaid-render` becomes a thin wrapper (`render → inject `<metadata>` → resvg→PNG`). #52–#75 + PR #99 **close on the #132 spike-Pass**; #76–#85 kept, re-scoped to wrapper bits. See #37 decision + #132. |
| **Human editor UX** | #15, #111, #109, #120, #44, #45, #32 | fs-watch reload, scroll-sync toggle, tab bulk-close menu, print/PDF, non-Markdown viewing, vim/themes, file-location affordances. |
| **scrybe-py library** | #6, #7, #8 | `pip install scrybe`: usable library → plugin protocol → reference plugins. Strictly sequential. |
| **Packaging, distribution & CI guardrails** | #1, #2, #116 | Homebrew, Chocolatey, and a gitleaks + internal-specifics secret-scan CI privacy guardrail. |
| **New feature plugins (v0)** | #31, #33, #34 | scrybe-quill inline AI authoring, CAD-from-code-fence, 3D-printer control. Deferred toward 1.0. |
| **Strategic explores (decision spikes)** | #114, #115 | Time-boxed: wrap Ferrite / markdown-tui-explorer instead of duplicating? Resolve **early** — they gate downstream build scope. |

## Keystone: why the MCP is "not functioning"

`scrybe-cli → scrybe-rpc → scrybe-app/src-tauri/src/cli_rpc.rs` talks to the
**live** editor and works. `scrybe-mcp-server` is a **separate** hand-rolled
server that keeps its *own* in-memory `Workspace`, `spawn`s a **new** app
process for `open` (root cause of #108), polls hardcoded `/tmp/*.txt` files for
UI parity, and has a malformed JSON-RPC error path with no `isError`. The two
paths have diverged in protocol, identity model (path vs `DocumentId(uuid)`),
and semantics. The fix is not to patch `tools.rs` — it is to collapse both
paths onto one shared registry dispatching through `scrybe-rpc`. Full analysis:
[`docs/design/mcp-rebuild.md`](design/mcp-rebuild.md).

## Per-issue disposition

`keep` = build as-is · `verify-may-be-done` = partly shipped, verify + close
remainder · `explore-spike` = time-boxed decision, not build work.

### MCP rebuild / CLI↔MCP parity
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 108 | keep | v0.4 | — | **Root cause.** `open` spawns a new app instead of dispatching through `scrybe-rpc`. Fix = shared registry over the socket (headless fallback). Unblocks #46/#121. |
| 121 | keep | v0.5 | 108 | Atomic `mermaid_to_png` (+ `markdown_extract_and_render`) MCP tool whose description is an embedded agent prompt. |
| 46 | keep | v0.5 | 108 | `list_tabs` over the live socket (not the detached workspace). |

### Mermaid-PNG round-trip + agent skills
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 119 | keep | **v0.4** | — | **PRIORITY.** LLM-callable Mermaid→PNG skill (iTXt source+uuid+sha256). The `/mermaid-png` skill already exists; gap is the MCP-native surface + `mermaid_to_png` tool. |
| 28 | keep | v0.5 | — | Render inline `![alt](diagram.png)` PNGs carrying iTXt `mermaid-source`. Rides `reload`'s live re-render pass. |

### Mermaid renderer — ADOPTED `mermaid-rs-renderer` (#37, #132 ✓ PASSED)

> **DONE (2026-07-17):** the **#132** spike was *run* and **passed** —
> `mermaid-rs-renderer` v0.3.1 renders the MVP flowchart + sequence corpus to
> valid SVG (pure Rust, no `mmdc`). Disposition applied: **#52–#76 CLOSED** as
> provided-by-dependency (+ PR #99); **#77–#85 re-scoped** (kept open). The
> `scrybe-mermaid-render` wrapper shipped (#171/#172) and **#119 is closed**
> (`scrybe mermaid png`). **#37 pulled forward to v0.5.** The table below is the
> *original* build plan — historical; trust GitHub for live state.
>
> **Prior re-scope (2026-07-13):** milestones below reflect the original build
> plan; the renderer's Scrybe value-add — source in SVG `<metadata>` / PNG iTXt —
> is kept in-house. See `ROADMAP.md` → "The renderer epic (#37): adopted, not built".
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 37 | keep | v0.6→v0.11 | — | Umbrella; closes with its last child #85. |
| 52 | keep | v0.6 | — | P1 seq parser: participants/actors. |
| 53 | keep | v0.6 | 52 | P1 seq parser: message arrows. |
| 54 | keep | v0.6 | 53 | P1 seq parser: cancel arrows + Note. |
| 55 | keep | v0.6 | 54 | P1 seq parser: loop/alt/opt/par. |
| 56 | keep | v0.6 | 55 | P1 seq parser: activate/deactivate (completes seq parser). |
| 57 | keep | v0.6 | — | P1 flowchart parser: direction + node/edge. |
| 58 | keep | v0.6 | 57 | P1 flowchart parser: node shapes. |
| 59 | keep | v0.6 | 58 | P1 flowchart parser: edge variants. |
| 60 | keep | v0.6 | 59 | P1 flowchart parser: subgraphs (completes flowchart parser). |
| 61 | keep | v0.7 | 56 | P2 seq layout: participant-x + statement-walk-y. |
| 62 | keep | v0.7 | 61 | P2 seq layout: activation stacks. |
| 63 | keep | v0.7 | 62 | P2 seq layout: notes + group-block. |
| 64 | keep | v0.7 | 61 | P2 svg: `<metadata>` root + sha256 (shared primitive). |
| 65 | keep | v0.7 | 64 | P2 seq svg: lifeline/header. |
| 66 | keep | v0.7 | 65 | P2 seq svg: arrow/activation. |
| 67 | keep | v0.7 | 63, 66 | P2 seq svg: assemble builder (flips 10 seq trace tests). |
| 68 | keep | v0.8 | 60 | P3 Sugiyama: cycle removal. |
| 69 | keep | v0.8 | 68 | P3 Sugiyama: layer assignment. |
| 70 | keep | v0.8 | 69 | P3 Sugiyama: dummy-node insertion. |
| 71 | keep | v0.8 | 70 | P3 Sugiyama: barycenter crossing-min. |
| 72 | keep | v0.8 | 71 | P3 Sugiyama: iterate to convergence. |
| 73 | keep | v0.8 | 72 | P3 Sugiyama: coordinate assignment (completes flowchart layout). |
| 74 | keep | v0.9 | 64 | P4 flowchart svg: shape primitives. |
| 75 | keep | v0.9 | 73, 74 | P4 flowchart svg: assemble builder (flips 10 flowchart trace tests). |
| 76 | keep | v0.9 | 75 | P4 png: resvg/tiny-skia behind `png` feature. |
| 77 | keep | v0.9 | 76 | P4 grader: SSIM via image-compare. |
| 78 | keep | v0.9 | 77 | P4 trace tests: enforce SSIM ≥ 0.92. |
| 79 | keep | v0.10 | 67 | P5 python: `render_to_svg` PyO3. |
| 80 | keep | v0.10 | 76 | P5 python: `render_to_png` PyO3 (gated). |
| 81 | keep | v0.10 | 79 | P5 python: package surface. |
| 82 | keep | v0.10 | 81 | P5 python: pytest suite. |
| 83 | keep | v0.11 | 78 | P6 conformance: upstream oracle + Tier-1 fixtures. |
| 84 | keep | v0.11 | 76 | P6 licensing: LICENSE-MPL-2.0 for resvg/usvg. |
| 85 | keep | v0.11 | 82, 83, 84 | P6 release: publish=true + bump + tick #37 (closes umbrella). |

### Human editor UX
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 32 | verify-may-be-done | v0.4 | — | Path bar shipped; verify full-path vs relative-to-content-root copy, close remainder. |
| 15 | keep | v0.5 | — | fs-watcher reload (clean=silent, dirty=prompt). Synergistic with #108 but independent. |
| 109 | keep | v0.5 | — | Tab context menu: Close Right / Left / Others. Pure frontend. |
| 45 | verify-may-be-done | v0.5 | — | `set_vim`/themes/highlighting shipped; verify vim depth (search/regex-replace), close remainder. |
| 111 | keep | v0.6 | — | Split-pane scroll-sync toggle (fuzzy ok). |
| 120 | keep | v0.6 | — | Cmd/Ctrl+P system print / print-to-PDF + print CSS. Optional `export_pdf` MCP tool rides the parity surface. |
| 44 | keep | v0.7 | — | View non-Markdown/all-text + git diffs, preview off. Reuses existing git-diff features. |

### scrybe-py library
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 6 | keep | v0.7 | — | Phase 1 usable library (Document, render, AST, content_id). Thin PyO3. |
| 7 | keep | v0.8 | 6 | Phase 2 plugin protocol (stdin/stdout + class-based). |
| 8 | keep | v0.8 | 7 | Phase 3 reference plugins (word-count, docx — align with existing scrybe-plugin-docx). |

### Packaging, distribution & CI guardrails
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 116 | keep | v0.4 | — | gitleaks (free binary, not the paid action) + internal-specifics scan on GitHub-hosted runners. Matters most for a public repo — land early. |
| 1 | keep | v0.11 | — | Homebrew formula (macOS). Near-1.0 distribution polish. |
| 2 | keep | v0.11 | — | Chocolatey package (Windows). Near-1.0 distribution polish. |

### New feature plugins (v0)
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 31 | keep | v0.9 | — | scrybe-quill inline AI authoring (BYO OpenAI-compatible/Ollama, offline, no telemetry). Leads the plugins epic. |
| 33 | keep | v0.12 | — | scrybe-plugin-cad. Benefits from the #7 plugin protocol. |
| 34 | keep | v0.12 | — | scrybe-plugin-printer-control. Furthest from core mission. |

### Strategic explores (decision spikes)
| # | Disp. | Milestone | Blocked by | Note |
|---|---|---|---|---|
| 114 | explore-spike | v0.4 | — | Wrap **Ferrite** with scrybe MCP instead of duplicating? Resolve before heavy build. |
| 115 | explore-spike | v0.4 | — | Wrap/deprecate toward **markdown-tui-explorer**? May reshape the renderer/editor scope. |

## Net-new tracking issues filed for this roadmap

The MCP-rebuild design and the mermaid mid-turn context surface work with no
existing issue. Per the roadmap protocol ("no item without a number"), these were
filed as tracking issues on 2026-07-13:

| # | Title | Milestone |
|---|---|---|
| **#122** | epic: MCP rebuild — shared `scrybe-tools` ToolSpec registry (native modulex) — umbrella for #108/#46/#121/#28/#15 | v0.4→v0.7 |
| **#123** | mcp: versioned typed data contract on every tool | v0.5 |
| **#124** | mcp: progressive disclosure — ≤12 default tools + discovery trio + facets + CI budget | v0.6 |
| **#125** | mcp: CLI↔MCP parity gate (CI: subcommands == `tools/list`) | v0.7 |
| **#126** | feat(mcp): `markdown_extract_and_render` — `## Fig NN:` → per-figure PNGs | v0.5 |
| **#127** | agent skill: `mcp-editing` SKILL.md (safe edit loop over MCP) | v0.5 |
| **#128** | release hygiene: `[workspace.package]` lock-step version + reconcile v0.3.0 tag | v0.4 |
| **#132** | spike: adopt pure-Rust Mermaid→SVG crate for #37 (fidelity bake-off + re-scope) | v0.6 |
