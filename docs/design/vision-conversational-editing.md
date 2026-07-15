<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Vision: Conversational Document Editing via MCP

**Status:** Accepted north star. Elaborates the CLAUDE.md principle *"the document
is the conversation"* into a concrete editing architecture.
**Date:** 2026-07-15
**Related design:** `docs/design/mcp-rebuild.md` (#122 — the tool engine this surface grows on),
`docs/design/cli-rpc.md` (the live CLI↔GUI socket that already carries edits to the running editor).
**Sequenced by:** `ROADMAP.md`. Feeds three epics (§5) targeting ~v0.7–v0.9, after the #122 rebuild.
**Design decision:** object identity is **decided** — derived IDs for coarse named
structure + a sidecar index for fine-grained anchors, embedded anchors opt-in (§3).

> Voice expresses intent. Documents preserve knowledge. MCP connects the two.

This document is in two parts. **Part I** is the vision, captured verbatim.
**Parts 2–6** map it onto what Scrybe already is, record the one design decision it
forces, and sequence the work.

---

## Part I — The Vision (verbatim)

### Executive Summary

Current AI interfaces force users to constantly transition between two worlds:

* Conversation (chat or voice)
* Artifacts (documents, diagrams, code, notes)

The friction comes from constantly copying information from one world into the other.
The goal of Scrybe is to eliminate that boundary.

Instead of producing chat responses that users manually copy into documents, the
conversation itself becomes an editing session over persistent artifacts. The AI is
no longer "writing text." The AI is proposing edits to shared documents.

### Core Principle

> Voice expresses intent.
> Documents preserve knowledge.
> MCP connects the two.

Conversation should be ephemeral. Documents should be durable. Every meaningful idea
eventually becomes a structured artifact.

### Mental Model

Today:

```
Human → Conversation → copy/paste → Document
```

Desired future:

```
Human → Speech/Text → Conversational AI → Intent → Scrybe MCP → Patch Operations → Persistent Documents
```

The conversation never "creates another document." The conversation *operates on*
existing documents.

### Example Conversation

> Figure 2 is way too tall.

The AI should *not* search an entire document. Instead, Scrybe resolves:

```
Figure 2  →  document_id, object_id, revision
```

The MCP request becomes something like:

```
update_object(document="design.md", object="figure-2", operation="make_wider")
```

The AI never has to guess. The editor provides grounding.

> Split Figure 3 into two figures.

The AI returns a patch:

```
Delete Figure 3
Create Figure 3A
Create Figure 3B
Update references
```

The editor previews the diff. The human accepts or edits. Conversation continues.

### The Artifact is the Shared Workspace

The document is not output. The document is shared memory. Conversation manipulates
that memory: Markdown, Mermaid diagrams, architecture drawings, TODO lists, design
docs, specifications, meeting notes, source code, UML, whiteboard objects.
Everything is an editable object.

### Object Addressability

Every meaningful element should have a stable identity. Instead of `Line 83`, use
`figure-2`, `table-performance`, `section-installation`, `diagram-build-pipeline`,
`paragraph-17`, `bullet-42`. This lets conversation naturally refer to objects
("Make Figure 2 wider", "Move this section earlier", "Rename that table", "Expand
bullet four"). No brittle line numbers. No fuzzy searching.

### Ideal MCP Surface

Expose document operations, not GUI automation:

```
workspace.list / open / search / status
document.read / write / create / delete / diff / history
document.outline / resolve_reference / references
document.apply_patch / insert / delete_range / replace / move
document.comment / reply / resolve_comment
mermaid.validate / render / layout / export
```

### Patch-Oriented Editing

The AI should rarely overwrite documents. Instead it produces patches (replace
object → revision +1). Patches are reviewable, undoable, mergeable, versionable,
auditable.

### Voice is Intent, not Dictation

Traditional voice systems produce text. This system produces edits. "Figure 2 is too
tall" does not become text — it becomes `target: figure-2, operation: reshape(horizontal)`.
Speech becomes an editing language.

### Grounding Responsibilities

The AI should not waste tokens searching. Scrybe performs grounding:
`"Figure 2" → figure-2 → diagram object → revision 14 → passed to AI`. The AI
receives the target, its current geometry, its current revision, and its current
contents. This dramatically improves precision.

### Human Workflow

Talk naturally ("Make this wider", "Split that section", "Move that above", "Explain
this better", "Rewrite this paragraph"). Scrybe resolves references. AI proposes
patches. Diff appears. Human edits if desired. Conversation continues. The artifact
continuously evolves.

### Long-Term Vision

The AI is not replacing the editor. The editor is not replacing the conversation.
Each specializes. Conversation excels at brainstorming, design, teaching, explaining,
negotiating ideas. Documents excel at permanence, structure, versioning,
collaboration, publishing. The bridge between them is a structured editing protocol.

### Design Philosophy

The best AI editor should feel less like talking to a chatbot and more like sitting
beside an expert collaborator with a shared notebook open between you. The notebook is
the source of truth. The conversation simply moves the pencil.

### A Future Extension

A natural extension is multimodal collaboration: voice references a figure, eye
tracking selects an object, pointer hovers a paragraph, the document highlights the
current context, and the AI receives all of those signals simultaneously.
Conversation becomes spatial. Documents become living workspaces rather than static
files.

### Guiding Principle

> Chat is temporary. Artifacts are forever. The future interface is not "AI chat."
> The future interface is collaborative editing over shared, structured artifacts.

---

## Part II — Scrybe reality mapping

This vision is not a pivot; it is the sharp form of the north star already in
`CLAUDE.md`. What it adds are two mechanisms that turn the slogan into architecture:

1. **Grounding is Scrybe's job, not the model's.** Reference resolution happens
   *before* the model is invoked. The model receives an object handle + revision +
   local context — never a whole document to search. This is the highest-leverage
   idea in the vision: it makes responses fast, deterministic, and cheap in tokens.
2. **Edits are patches, not overwrites.** Every change is a reviewable, revisioned,
   auditable patch — not a blind rewrite.

Both already have foundations in the codebase, which is what makes this an extension
rather than a rewrite:

| Vision mechanism | What Scrybe already has |
|---|---|
| Object handles | The `section` MCP tool already addresses by heading (`{id, heading}`); Mermaid diagrams already carry a **UUID + SHA256** in PNG iTXt / SVG `<metadata>` — object identity for the diagram case, done. |
| Patch / revision / audit | `ContentAddressable` (BLAKE3 + CBOR) in `scrybe-core` + `scrybe-vcs` (git2) **are** the "revision +1, versionable, auditable" substrate. Patches map onto content-addressed revisions and commits. |
| Live diff → accept/edit | `cli-rpc.md`'s request-with-reply path (`dispatch_with_reply`) already round-trips an operation into the running editor and back — the transport a diff-preview UI needs. |
| The richer `document.*` / `workspace.*` surface | **#122** (MCP rebuild, progressive disclosure) is precisely the vehicle to register these tools. |

So the work is mostly *layering an addressing + grounding + patch surface* onto pieces
that exist.

---

## Part III — The object-identity decision (decided)

Object identity in plain-text Markdown is the crux, and it collides with the
Scrybe/​workspace philosophy: *"formats should be plain text; don't build lock-in."*
Markdown has no node IDs, so `figure-2` / `paragraph-17` must come from somewhere.
Three strategies, with the tradeoff against plain-text sovereignty:

- **A — Derived IDs (deterministic).** Compute IDs from structure/content: heading
  slugs, figure captions, table captions, an nth-of-type path.
  *Pro:* zero file pollution; plain text stays sovereign. *Con:* IDs move when content
  moves — good for *named* things, weak for anonymous prose.
- **B — Embedded anchors.** Kramdown-style `{#figure-2}` attributes or
  `<!-- scrybe:id figure-2 -->` comments in the `.md`. *Pro:* genuinely stable.
  *Con:* pollutes the plain text — the lock-in smell the philosophy warns against.
- **C — Sidecar index.** A `.scrybe/` sidecar maps stable IDs → anchored positions,
  kept in sync by the editor. *Pro:* `.md` stays pristine **and** IDs are stable.
  *Con:* a real anchoring/re-sync problem (external edits, conflicts).

### Decision

**A for coarse named structure + C for fine-grained/ephemeral anchors; B is opt-in.**

- **A (derived) — the default for the things people actually name aloud:** headings
  (slug), captioned figures (image + mermaid/diagram blocks), tables, and fenced
  code/mermaid blocks. These get an ID with **no file pollution**. Mermaid already
  proves the pattern — identity in metadata, source stays plain.
- **C (sidecar `.scrybe/`) — for fine-grained or ephemeral anchors:** an arbitrary
  paragraph, a bullet, a text range a user wants to point at. IDs are stable across
  inserts/deletes because the sidecar re-anchors on save; the existing fs-watcher
  re-syncs when the file changes on disk. Unresolvable anchors degrade gracefully to
  the nearest enclosing heading rather than erroring.
- **B (embedded) — opt-in only,** when a user deliberately wants a hard, permanent,
  human-visible anchor (e.g. a spec clause referenced from elsewhere). Never written
  automatically.

### ID semantics to nail down in the epic

- Derived IDs are stable only as long as the *name* is stable. Renaming a heading
  changes its slug → the sidecar keeps a **redirect/alias** so old references
  (and conversation history) still resolve.
- Default ID coverage is coarse structure; fine-grained IDs are **assigned lazily**
  (on first reference), not eagerly for every paragraph — keeps the sidecar small.
- The sidecar is derived, disposable state (like a build cache), **never** the source
  of truth. Delete `.scrybe/` and the document is unharmed; coarse IDs regenerate.

---

## Part IV — Grounding: deterministic vs deictic

Reference resolution splits in two, with very different difficulty. Do not let v1
grounding quietly promise what only the multimodal layer can deliver.

- **Named references** — "Figure 2", "the installation section", "the performance
  table". **Deterministic, buildable now** on the addressing layer.
  `document.resolve_reference("Figure 2")` → `{document_id, object_id, revision,
  geometry, contents, surrounding_context}`. This is the win.
- **Deictic references** — "*this* paragraph", "move *this* above", "*that* section".
  These need a selection/cursor signal. **Partial win available now:** when the
  editor is driving, it already knows the cursor/selection, so the app can supply
  "this" as the current selection (cursor = "this"). **Full deixis** — pointer, eye
  tracking, spatial context — is the "Future Extension" and is a later epic gated on a
  multimodal input layer. In pure-voice/headless mode with no selection, "this" stays
  ambiguous and correctly falls back to asking rather than guessing.

---

## Part V — Sequencing (three epics, layered on #122)

In dependency order, slotted **after** the v0.4–v0.7 MCP rebuild (#122), ~v0.7–v0.9:

1. **Object addressability** *(foundation — everything else needs it).*
   Stable-ID scheme (A + C) over the `scrybe-core` AST; `document.outline()` exposes
   the ID tree; sidecar anchor store + re-sync via the fs-watcher; alias/redirect on
   rename.
2. **Reference resolution / grounding.**
   `document.resolve_reference()` + `document.references()`. Deterministic named-ref
   resolution first; deixis-via-selection for the app-driven case; full multimodal
   deixis deferred.
3. **Patch-oriented editing.**
   `document.apply_patch()` + `document.diff()` + the revision model + the
   accept/reject diff-preview UI in `scrybe-app`. Rides `ContentAddressable` +
   `scrybe-vcs`; structural ops (move, split, replace-object) modeled as patches.

Comments (`document.comment / reply / resolve_comment`) and the richer
`mermaid.layout/export` surface are natural follow-ons once (1)–(3) land.

---

## Part VI — Non-goals & open questions

**Non-goals (for this arc):**
- Full multimodal input (eye tracking, pointer/gaze fusion) — named after, not part
  of, v1 grounding.
- Real-time multi-user collaboration. "Shared workspace" here means *human + agent*
  over one document, not concurrent human editors.
- A universal object model for arbitrary binary artifacts. Markdown, Mermaid, code,
  tables, figures first.

**Open questions for the addressability epic:**
1. Sidecar format + anchoring algorithm (how positions survive external edits; what
   happens on merge conflict in the sidecar).
2. Patch representation for structural ops — is "split Figure 3 → 3A/3B + update
   references" one atomic patch or a sequenced transaction?
3. Reference history: when conversation says "Figure 2" three turns later and a figure
   was inserted above it, do we resolve to the *same object* (by ID) or the *same
   position* (by number)? (Leaning: by ID — identity beats ordinal.)
4. Do coarse IDs live purely derived, or do we cache them in the sidecar too for
   rename-alias continuity? (Leaning: derive + cache aliases only.)
