<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 Shawn Hartsock and contributors
-->

# Scrybe MCP-Server Rebuild — Native modulex-style Tool Engine

**Status:** Proposed. Targets v0.4.0 (next release) through v0.7.0.
**Date:** 2026-07-13
**Supersedes:** the hand-rolled `scrybe-mcp-server/src/server.rs` + `src/tools.rs`.
**Related design:** `docs/design/cli-rpc.md` (the CLI↔GUI socket protocol this rebuild unifies onto).
**Sequenced by:** `ROADMAP.md` (v0.4→v0.12) and `docs/TRIAGE.md`.

> "Python on the outside, Rust on the inside." The MCP server is a *view* onto
> the live editor — not a second, shadow editor. This rebuild makes that true by
> construction.

**Decision context:** modulex-mcp (`hartsock/modulex-mcp`) demonstrates the
target properties — one tool registry serving both CLI and MCP, a versioned
data contract, progressive disclosure, and a feature-gated plugin seam. We
**adopt those patterns natively** in Scrybe (no modulex dependency now), shaped
so `scrybe-tools` can later be extracted as a `modulex-plugin-scrybe` crate.

---

## 1. Problem statement — two diverged IPC paths

Scrybe has **two parallel IPC paths to the running editor that have drifted
apart**. They were built at different times, speak different protocols, carry
different identity models, and disagree about where document state lives.

### Path 1 — the CLI socket path (works)

```
scrybe-cli/src/main.rs
  └─ scrybe-cli/src/rpc_client.rs   send(method, params)  →  ~/.scrybe/sock
        └─ scrybe-app/src-tauri/src/cli_rpc.rs   dispatch()
              └─ emits scrybe://cli-{open,save,close,quit,read,find,section,edit}
                    └─ live CodeMirror frontend (owns tab state)
```

This path is correct. `scrybe-rpc/src/lib.rs` is the single source of truth for
the wire types (`Request`, `Response`, `RpcError`, `OpenParams`/`OpenResult`,
`ReadParams`/`ReadResult`, `FindParams`/`FindResult`, `SectionParams`,
`EditParams`, `EventEnvelope<T>`, `Reply`). Read-side commands use a
request-with-reply correlation (`PENDING_REPLIES` + the `cli_rpc_reply` Tauri
command in `cli_rpc.rs`). The app owns the truth; the CLI is a thin client.
`cli_rpc.rs` today handles exactly: **open, save, close, quit, read, find,
section, edit**.

### Path 2 — the MCP server path (diverged)

```
scrybe-mcp-server/src/server.rs   McpServer (stdio JSON-RPC)
  └─ scrybe-mcp-server/src/tools.rs   ToolRegistry
        ├─ owns its OWN scrybe_core::Workspace + id_map  ← shadow state
        ├─ tool_open() → std::process::Command::new(scrybe-app).spawn()  ← spawns a NEW app
        └─ UI-parity tools poll/write hardcoded /tmp files
```

`tools.rs` keeps a private `Workspace` and `HashMap<String, DocumentId>` that
have **no connection to the running app**. Its `tool_open` (`tools.rs:339`)
reads the file into *its own* workspace and *also* `spawn()`s a brand-new
`scrybe-app` process rather than dialing `~/.scrybe/sock`.

### Symptoms this root cause explains

| Issue | Symptom | Mechanism in the current code |
|---|---|---|
| **#108** | `mcp open` returns a valid `DocumentId` but no tab appears; opening 4 files in parallel returns 4 successes, 0 tabs | `tool_open` (`tools.rs:339-360`) loads into the MCP-private `Workspace` and `spawn()`s a new process instead of emitting `scrybe://cli-open` on the socket. On a single-instance app the spawn is dropped; the returned id is meaningless to the live app. |
| **#46** | Agents can't see what tabs are open | There is no `list_tabs` on either path; MCP's "state" is a `/tmp/scrybe-state.json` snapshot, not the live tab set. |
| **#15 / #28** | Open documents don't reload when the file changes on disk; embedded-Mermaid PNGs not re-rendered | `tool_reload` (`tools.rs:627`) reloads the *MCP-private* buffer and writes `/tmp/scrybe-reload-tab.txt` as a best-effort poke; the live app's buffer is unaffected. Reload is not a first-class socket op. |
| **/tmp polling brittleness** | Races, non-cross-platform, stale files | `tools.rs` hardcodes `/tmp/scrybe-debug.log`, `/tmp/scrybe-close-tab.txt`, `/tmp/scrybe-reload-tab.txt`, `/tmp/scrybe-state.json`, `/tmp/scrybe-set-theme.txt`, `/tmp/scrybe-view-mode.txt`, `/tmp/scrybe-set-vim.txt`. The frontend must *poll* these. Two MCP servers, or Windows, or a wiped `/tmp` all break silently. |
| **JSON-RPC error nesting** | Unknown methods return an error *inside* `result` | `server.rs:96-101` builds `{"error": {...}}` as the `result` value, then `server.rs:104-110` wraps it as `{"jsonrpc","id","result": <that>}`. The error lands at `result.error`, never at the top-level `error` field. `test_unknown_method_with_id_returns_error_result` (`server.rs:210`) even *codifies* the bug. |
| **No `isError`** | Agents can't structurally distinguish success from failure | `server.rs:87-95` (`tools/call`) always returns `{"content":[{"type":"text","text": …}]}` with no `isError`. Tool failures are `{"error": …}` JSON *stuffed into a text block* (e.g. `tools.rs:342`), so an agent must parse prose to know a call failed. |

There is also a quieter divergence: **identity and semantics differ across the
two paths.** MCP `edit` is `{id, old, new}` (first-occurrence replace,
`tools.rs:426`); CLI `edit` is `{path, start_line, end_line, content}`
(`scrybe-rpc` `EditParams`). MCP `section` is `{id, level, index}`; CLI
`section` is `{path, heading}`. MCP addresses documents by `DocumentId(uuid)`;
the socket addresses them by canonical path. Any "parity" claim in `AGENTS.md`
and `scrybe-mcp-server/README.md` is currently aspirational.

**Conclusion:** the fix is not to patch `tools.rs`. It is to collapse the two
paths into one — a shared tool registry whose handlers dispatch through
`scrybe-rpc` to the live app, with a headless fallback — so CLI↔MCP parity
holds *by construction*.

---

## 2. Target architecture — one shared `ToolSpec` registry

### 2.1 New crate: `scrybe-tools`

Introduce a new workspace crate, **`scrybe-tools`**, that owns the single tool
registry consumed by *both* front ends. (A `scrybe-core::tools` module was
considered and rejected: the registry must depend on `scrybe-render`,
`scrybe-mermaid`, and `scrybe-rpc`, which would pull rendering and IPC into
`scrybe-core` and violate its "pure AST/Document" role. A dedicated crate also
gives us the clean extraction seam in §9.)

```
scrybe-tools/
├── src/
│   ├── lib.rs           ToolSpec, Facet, ToolOutcome, Registry
│   ├── ctx.rs           Ctx { transport: Box<dyn Transport>, ... }
│   ├── transport.rs     trait Transport  (LiveApp | Headless)
│   ├── data.rs          versioned typed `data` payload structs (+ DATA_VERSION)
│   ├── facets.rs        Facet enum + default-set membership
│   └── tools/           one module per tool group (core, editor, mermaid, ui, vcs*, swarm*)
└── Cargo.toml           features: docx, vcs, swarm  (gated groups)
```

Dependency edges (all *toward* leaves, no cycles):

```
scrybe-cli ───────┐
                  ├──► scrybe-tools ──► scrybe-core, scrybe-render,
scrybe-mcp-server ┘                     scrybe-mermaid, scrybe-rpc
```

`scrybe-mcp-server` shrinks to a **transport shim**: read stdio, parse MCP
JSON-RPC, look up the `ToolSpec`, call its handler, format the MCP envelope.
`scrybe-cli` shrinks similarly: `clap` subcommands become thin wrappers that
build the same args map and call the same handler. The old `ToolRegistry` in
`tools.rs` and its private `Workspace`/`id_map` are **deleted**.

### 2.2 The `ToolSpec`

```rust
/// One tool, shared verbatim by the CLI and the MCP server.
pub struct ToolSpec {
    /// Wire name, e.g. "open", "mermaid_to_png". Also the CLI subcommand stem.
    pub name: &'static str,

    /// Human/agent-facing description. This is ALSO the embedded agent prompt:
    /// it carries behavioral guidance ("ALWAYS use this instead of raw mmdc"),
    /// not just a label. Rendered into MCP tools/list AND `scrybe <cmd> --help`.
    pub description: &'static str,

    /// JSON Schema for arguments (MCP `inputSchema`; also drives CLI arg parse).
    pub input_schema: fn() -> serde_json::Value,

    /// Versioned, typed schema for the tool's stable `data` payload.
    /// Agents read `data`; they NEVER parse `description` prose or `text`.
    pub data_schema: DataSchema,   // { version: u32, schema: fn()->Value }

    /// Does this tool change editor/disk/app state? Gates read-only agents,
    /// dry-run mode, and the autonomy rules in AGENTS.md.
    pub mutates: bool,

    /// Tool group for progressive disclosure + feature gating.
    pub facet: Facet,             // Core | Editor | Mermaid | Vcs | UiParity | Swarm

    /// The one implementation, shared by both front ends.
    pub handler: fn(&Ctx, &serde_json::Value) -> ToolOutcome,
}

pub struct ToolOutcome {
    /// Typed, versioned payload. Serialized under `data` in every surface.
    pub data: serde_json::Value,
    /// Business-level failure (tool ran, said "no"): e.g. "heading not found".
    /// This is DATA, not an engine fault. isError stays false.
    pub tool_error: Option<ToolError>,
}
```

Engine faults (bad JSON args, transport down, panic) are returned as `Err`
from the dispatcher and become MCP `isError: true` / a non-zero CLI exit.
Business failures live in `ToolOutcome.tool_error` and travel inside `data`.
This is the modulex "per-tool errors are data; engine faults are `isError`"
rule (§5).

### 2.3 Dispatch through `scrybe-rpc` (the unification)

Every stateful handler talks to the **live app** through a `Transport`:

```rust
pub trait Transport {
    /// Round-trip a scrybe-rpc Request over ~/.scrybe/sock. Reuses the exact
    /// client logic in scrybe-cli/src/rpc_client.rs (promoted into scrybe-rpc).
    fn call(&self, method: &str, params: Value) -> Result<Value, TransportError>;
    fn is_live(&self) -> bool;
}
```

- **`LiveApp`** — wraps `scrybe_rpc::send` (the `rpc_client.rs` dialer, moved
  into `scrybe-rpc` so both crates share it). `open` emits `scrybe://cli-open`
  exactly like the CLI does today — **fixing #108 for free**.
- **`Headless`** — no socket. Runs the pure, GUI-free subset in-process against
  a transient `scrybe_core::Document` (render, lint, mermaid embed/extract,
  mermaid_to_png). GUI-only tools return a clean "no app running" `tool_error`.

Because the handler is identical and the transport is the only variable, the
CLI and the MCP server cannot diverge: they are the same function with a
different envelope.

---

## 3. Retiring `/tmp` polling — new `scrybe-rpc` methods

The UI-parity and tab tools must become first-class socket methods in
`scrybe-app/src-tauri/src/cli_rpc.rs`, replacing the `/tmp/*.txt` pokes in
`tools.rs`.

### Already exist in `cli_rpc.rs` (`dispatch`, line 217)

`open`, `save`, `close`, `quit` (fire-and-forget events);
`read`, `find`, `section`, `edit` (request-with-reply).

### New methods to add (retire the `/tmp` files)

Add these to `scrybe-rpc/src/lib.rs` (params/results) and to the `dispatch`
match in `cli_rpc.rs`, with matching `scrybe://cli-*` events and, where a value
comes back, the existing `dispatch_with_reply` correlation.

| New method | Kind | Params | Result | Replaces |
|---|---|---|---|---|
| `list_tabs` | reply | `{}` | `ListTabsResult { tabs: Vec<TabInfo> }` where `TabInfo { path, title, is_dirty, view_mode, active }` | **#46** (nothing today) |
| `state` | reply | `{}` | `StateResult { active: Option<TabInfo>, theme, vim_enabled }` | `/tmp/scrybe-state.json` (`tools.rs:684`) |
| `set_theme` | fire | `{ theme }` (`default\|dark\|solarized`) | `AckResult { applied }` | `/tmp/scrybe-set-theme.txt` (`tools.rs:700`) |
| `view_mode` | fire | `{ mode }` (`both\|edit\|preview\|cycle`) | `ViewModeResult { mode }` (resolved after cycle) | `/tmp/scrybe-view-mode.txt` (`tools.rs:713`) |
| `set_vim` | fire | `{ enabled }` | `AckResult { applied }` | `/tmp/scrybe-set-vim.txt` (`tools.rs:726`) |
| `close_tab` | fire | `{ path?: String }` (omit = active) | `AckResult { applied }` | `/tmp/scrybe-close-tab.txt` (`tools.rs:599`) |
| `reload` | reply | `{ path, force?: bool }` | `ReloadResult { path, bytes, was_dirty }` | `/tmp/scrybe-reload-tab.txt` (`tools.rs:627`) — **#15/#28** |
| `logs` | reply | `{ tail?: u32 }` | `LogsResult { entries, total }` | `/tmp/scrybe-debug.log` tail (`tools.rs:775`) |

Notes:

- `reload` becomes a **live** operation: the frontend re-reads disk into the
  *actual* CodeMirror buffer and re-runs the embedded-Mermaid render pass,
  directly closing **#15** ("don't reload on disk change") and **#28** ("PNG
  with embedded Mermaid not rendered"). The `force` flag maps to the existing
  dirty-buffer guard (`tools.rs:660`).
- `close_tab` keys off canonical path exactly like `close` — the frontend
  already has the handler; we just stop signaling it through a temp file.
- `logs` reads the log the frontend already writes; it can stay a
  reply-over-socket instead of a shared file, or (transition) read the same
  file through the app so there is one owner.
- Reload/list_tabs reuse `EventEnvelope<T>` + `Reply` and the
  `dispatch_with_reply` path already proven for `read`/`find`/`section`/`edit`.

After this, **`tools.rs` writes zero `/tmp` files** and the frontend polls
nothing. One protocol, one socket, one source of truth.

---

## 4. The three modulex pillars, applied to Scrybe

### (A) Data contract — every tool emits a stable, versioned `data`

Agents read a typed `data` object; they never parse `description` prose or the
MCP `text` block. Every surface (MCP `tools/call`, `scrybe <cmd> --json`,
`scrybe <cmd> --format data`) returns the same `data`. A top-level
`DATA_VERSION` (per tool) lets agents pin behavior.

**`open` →**

```jsonc
{
  "v": 1,
  "kind": "open",
  "tab": {
    "path": "/abs/notes.md",     // canonical path == stable handle (matches OpenResult.tab_id)
    "title": "notes.md",
    "reloaded": false,           // true == already-open tab refreshed (matches OpenResult.reloaded)
    "is_dirty": false
  },
  "live": true                   // dispatched to a running app (false == headless)
}
```

Note: the handle is the **canonical path**, not `DocumentId(uuid)`. This
retires the MCP-private id map (`tools.rs:98`) and makes ids meaningful to the
live app — the semantic half of the #108 fix.

**`list_tabs` →**

```jsonc
{
  "v": 1,
  "kind": "list_tabs",
  "tabs": [
    { "path": "/abs/a.md", "title": "a.md", "is_dirty": false, "view_mode": "both",    "active": true  },
    { "path": "/abs/b.md", "title": "b.md", "is_dirty": true,  "view_mode": "preview", "active": false }
  ],
  "count": 2
}
```

**`lint` →** (superset of today's `tools.rs:591` output, plus the richer fields
`scrybe-cli`'s `lint_document` already computes)

```jsonc
{
  "v": 1,
  "kind": "lint",
  "content_id": "af1349b9…",     // scrybe-core ContentAddressable BLAKE3 content digest (lowercase hex)
  "word_count": 812,
  "heading_count": 14,
  "max_heading_depth": 3,
  "code_block_count": 6,
  "code_block_langs": ["rust", "bash"],
  "has_math": true,
  "has_mermaid": true,
  "broken_links": [ { "text": "spec", "url": "./missing.md" } ],
  "clean": false                 // == broken_links.is_empty()
}
```

### (B) Progressive disclosure — small default set + discovery trio + facets

`tools/list` returns **≤ 12 tools**, CI-pinned. The long tail is reachable via
`tool_search → tool_describe → tool_invoke`. `Facet` gates which groups a search
can surface.

**Default set (12), by facet:**

| # | Tool | Facet | mutates |
|---|---|---|---|
| 1 | `open` | Core | yes |
| 2 | `read` | Core | no |
| 3 | `edit` | Editor | yes |
| 4 | `find` | Editor | no |
| 5 | `render` | Core | no |
| 6 | `lint` | Core | no |
| 7 | `state` | UiParity | no |
| 8 | `list_tabs` | Editor | no |
| 9 | `mermaid_to_png` | Mermaid | yes |
| 10 | `tool_search` | Core | no |
| 11 | `tool_describe` | Core | no |
| 12 | `tool_invoke` | Core | yes* |

*(`tool_invoke`'s effective `mutates` is that of the tool it dispatches.)*

**Long tail (facet-gated, reached via the trio):** `section`, `embed`,
`extract`, `export`, `markdown_extract_and_render`, `set_theme`, `view_mode`,
`set_vim`, `close_tab`, `reload`, `logs`, `quit`, plus future `vcs.*` and
`swarm.*` groups.

**The trio:**

- `tool_search { query, facet? } → { hits: [{name, facet, summary}] }` —
  keyword/facet search over the registry.
- `tool_describe { name } → { name, description, input_schema, data_schema, mutates, facet }` —
  the full spec on demand.
- `tool_invoke { name, arguments } → <that tool's data>` — call any registered
  tool, gated by `mutates`/facet policy.

**CI budget test** (blocks merge, in `scrybe-tools/tests/budget.rs`):

```rust
#[test]
fn default_tools_list_is_pinned() {
    let names = Registry::default().default_list_names();
    assert!(names.len() <= 12, "default tools/list exceeds budget: {}", names.len());
    assert_eq!(names, EXPECTED_DEFAULT_12);   // golden set — changing it is a deliberate review event
}
```

This replaces the current brittle `assert_eq!(arr.len(), 18)` in `tools.rs:877`
and `assert!(tools.len() >= 11)` in `server.rs:151`.

### (C) Feature-gated seam

Each facet beyond `Core`/`Editor` is a Cargo feature on `scrybe-tools`:

```toml
[features]
default = ["mermaid", "ui-parity", "docx"]
mermaid   = []
ui-parity = []
docx      = []            # gates `export` (shells to scrybe-docx)
vcs       = []            # future: scrybe-vcs tool group
swarm     = []            # future: scrybe-swarm tool group
```

`register()` conditionally adds groups (`#[cfg(feature = "vcs")]`). A disabled
feature means the tool is absent from the registry entirely — no dead entries,
no runtime "not compiled" errors. This is the modulex "feature-gated plugin
crate" pattern in one crate, staged for the split in §9.

---

## 5. Correct MCP protocol

Rewrite `server.rs` `handle()` so:

1. **Unknown methods go to the top-level `error` field.** Replace `server.rs:96`
   so `other =>` returns a real JSON-RPC error object *sibling to* `id`, never
   nested under `result`:

   ```jsonc
   { "jsonrpc": "2.0", "id": 7, "error": { "code": -32601, "message": "method not found: foo" } }
   ```

   Delete the bug-codifying assertion in `server.rs:210` and replace it with a
   test that asserts `resp["error"]["code"] == -32601` and `resp.get("result").is_none()`.

2. **`tools/call` sets `isError`.** On an engine fault (unknown tool, arg-parse
   failure, transport error) return:

   ```jsonc
   { "content": [{ "type": "text", "text": "<message>" }], "isError": true }
   ```

   On success return `isError: false` and put the typed payload in a
   `structuredContent`/`data` field *and* mirror a compact form in `text`:

   ```jsonc
   { "content": [{ "type": "text", "text": "opened /abs/notes.md" }],
     "isError": false,
     "data": { "v": 1, "kind": "open", "tab": { … } } }
   ```

3. **Per-tool business failures are data, not `isError`.** A "heading not found"
   or "tab not open" is `isError: false` with `data.tool_error` populated —
   the call *succeeded* in telling the agent "no". Only engine faults flip
   `isError`. This is the crisp success/failure signal agents lack today.

4. **New CLI sanity flags** (parity: also exposed as `scrybe --tools` /
   `scrybe --probe` and `scrybe-mcp-server --tools` / `--probe`):

   - `--tools` — dump the full `ToolSpec` registry as JSON (name, facet,
     `mutates`, schemas) and exit 0. CI diffs this against a golden file.
   - `--probe` — health check: is `~/.scrybe/sock` live? what transport would
     be used (LiveApp vs Headless)? Exit 0 if a coherent transport exists,
     non-zero otherwise. Gives agents and CI a one-shot readiness signal.

---

## 6. CLI ⇔ MCP parity

Every MCP tool maps to a `scrybe` subcommand and back. Table reflects the
**target** state; the **Gap** column is the work.

| MCP tool | CLI subcommand | Status / gap |
|---|---|---|
| `open` | `scrybe open <path>` / `scrybe <path>` | Exists (`main.rs:481`). Re-route through shared handler. |
| `read` | `scrybe read <path>` | Exists (`main.rs:628`). |
| `edit` | `scrybe edit <path> --start-line --end-line --content` | Exists (`main.rs:704`). Unify semantics (line-range wins; drop MCP `old/new`). |
| `find` | `scrybe find <pattern> [paths…]` | Exists (`main.rs:645`). |
| `section` | `scrybe section <path> --heading` | Exists (`main.rs:687`). Unify on `{path, heading}`; drop MCP `{level,index}`. |
| `render` | `scrybe render <file>` | Exists (`main.rs:331`). Add `--id/--path` to render a live buffer. |
| `lint` | `scrybe lint <file>` | Exists (`main.rs:376`). Emit versioned `data`. |
| `embed` | `scrybe embed` / `scrybe mermaid embed` | Exists (`main.rs:581`, `439`). |
| `extract` | `scrybe extract` / `scrybe mermaid extract` | Exists (`main.rs:589`, `447`). |
| `mermaid_to_png` | `scrybe mermaid png …` | **GAP — new** (#119/#121). Add subcommand. |
| `markdown_extract_and_render` | `scrybe mermaid md-render <file> --out <dir>` | **GAP — new**. Detects `## Fig NN: Title` headings, names PNGs `YYYY-MM-DD_Doc_Fig-NN_Title.png`. |
| `export` | `scrybe export <input> [-o]` | **GAP** — MCP-only today (`tools.rs:740`). Add subcommand. |
| `state` | `scrybe state` | **GAP** — add subcommand (reads new `state` rpc). |
| `list_tabs` | `scrybe tabs` | **GAP** — add subcommand (#46). |
| `set_theme` | `scrybe set-theme <theme>` | **GAP** — add subcommand. |
| `view_mode` | `scrybe view <mode>` | **GAP** — add subcommand. |
| `set_vim` | `scrybe set-vim <on\|off>` | **GAP** — add subcommand. |
| `close_tab` | `scrybe close-tab [path]` | **GAP** — distinct from `close` (active-tab default). |
| `reload` | `scrybe reload <path> [--force]` | **GAP** — add subcommand (#15). |
| `logs` | `scrybe logs [--tail]` | **GAP** — add subcommand. |
| `quit` | `scrybe quit [--force]` | Exists (`main.rs:546`). |
| `tool_search` / `tool_describe` / `tool_invoke` | `scrybe tools search\|describe\|invoke` | **GAP — new** (discovery trio). |

Parity is enforced by a test in `scrybe-tools`: every `ToolSpec.name` must have
a registered CLI subcommand and vice versa (fail CI on drift). The existing
`SUBCOMMANDS` slice (`main.rs:321`) is generated from the registry, not
hand-maintained.

---

## 7. Help prompts as embedded agent guidance + agent skills

### Descriptions are prompts

`ToolSpec.description` is written to *steer behavior*, not merely label. It is
rendered verbatim into MCP `tools/list` **and** `scrybe <cmd> --help`. Example
for the #121 tool:

> `mermaid_to_png` — "Render a Mermaid diagram to PNG **and embed the source in
> PNG iTXt metadata (UUID + SHA256)**. ALWAYS use this instead of calling `mmdc`
> directly — raw `mmdc` skips embedding and breaks lossless round-trips and
> document publishing. Input: mermaid `source` + `output_path`. Returns
> `{ png_path, uuid, sha256 }`."

The "ALWAYS use this instead of raw mmdc" clause lives in the *tool surface*, so
an agent that never loaded a skill still gets the guidance — the exact miss
described in #121.

### Skills to ship (`SKILL.md`)

Ship three skills, source-controlled at `scrybe-app/skills/` and installed to
`.claude/skills/<name>/SKILL.md` (and bundled as Tauri resources so the desktop
app can register them):

| Skill | Purpose | Backed by tools |
|---|---|---|
| `mermaid-png` | The render→embed→verify round-trip; the `## Fig NN: Title` → `YYYY-MM-DD_Doc_Fig-NN_Title.png` naming convention; when to use PNG vs fenced block; Confluence publishing note (#119) | `mermaid_to_png`, `markdown_extract_and_render`, `extract` |
| `mcp-editing` | The safe editing loop: `open → read → find → edit → render/lint`; reload-after-external-edit discipline; dirty-buffer rules | `open`, `read`, `find`, `edit`, `reload`, `render`, `lint` |
| `repository-roadmap` | How the v0.4→v1.0 milestones, facets, and the data contract fit together; how to add a tool without breaking the CI budget. **Already present at `.claude/skills/repository-roadmap/`** — reference/extend, don't duplicate. | `tool_search`, `tool_describe`, `tool_invoke` |

Skills reference tools by their stable `name` and `data` schema — never by prose
output — so they stay valid as descriptions evolve.

---

## 8. Migration / rollout — one PR per phase

Each phase is a single reviewable PR. Phases are ordered so the tree is always
green and each step delivers standalone value. Milestone mapping is authoritative
in `ROADMAP.md`.

### Phase 1 — Foundation + protocol fixes + #119 (→ **v0.4.0**, the next release)

- Create `scrybe-tools` crate; define `ToolSpec`, `Facet`, `ToolOutcome`,
  `Registry`, `Transport` (with `Headless` only for now).
- Port the *pure* tools (render, lint, embed, extract) into the registry.
- **Add `mermaid_to_png`** and its CLI subcommand + the `mermaid-png` skill,
  delivering the #119 priority.
- Fix `server.rs` protocol: top-level `error` for unknown methods; `isError` on
  `tools/call`; delete the bug-codifying test.
- Add `--tools` / `--probe`.
- **Closes: #119** (+ the JSON-RPC nesting/`isError` defects).

### Phase 2 — Dispatch unification (→ **v0.4.x / v0.5.0**)

- Promote `rpc_client.rs` dialer into `scrybe-rpc`; add the `LiveApp` transport.
- Re-route `open`/`read`/`edit`/`find`/`section` handlers through `scrybe-rpc`;
  delete the MCP-private `Workspace`/`id_map` (`tools.rs:96-99`).
- Add socket methods `list_tabs`, `state`, `close_tab`, `reload` and retire the
  corresponding `/tmp` files.
- **Closes: #108** (open now emits `scrybe://cli-open`), **#46** (`list_tabs`),
  **#15 / #28** (`reload` re-renders live buffer).

### Phase 3 — Data contract (→ **v0.5.0**)

- Add versioned `data` payloads (`data.rs`, per-tool `DATA_VERSION`) to every
  tool; wire `structuredContent`/`data` into MCP and `--format data` into CLI.
- Golden `--tools` snapshot test.
- Ship `mermaid_to_png` + `markdown_extract_and_render` as MCP tools (#121).

### Phase 4 — Progressive disclosure (→ **v0.6.0**)

- Add `tool_search`/`tool_describe`/`tool_invoke`; implement facet gating;
  shrink `tools/list` to the pinned 12; land the CI budget test.
- Retire remaining `/tmp` UI files via socket `set_theme`/`view_mode`/`set_vim`.
- **Closes: #45** (vim/themes now first-class parity ops), advances **#32**
  (file-system location tools become discoverable via facets).

### Phase 5 — Parity backfill + skills + seam (→ **v0.7.0**)

- Fill every CLI gap from §6 (`state`, `tabs`, `set-theme`, `view`, `set-vim`,
  `close-tab`, `reload`, `logs`, `export`, `tools …`); land the CLI↔MCP parity
  CI gate (subcommand set == `tools/list` set).
- Ship `mcp-editing` and `repository-roadmap` skills.
- Finalize feature-gated facets (`vcs`, `swarm` stubs) and the extraction seam.

v0.8→v0.12 (out of scope here) carry the long-tail facet groups (`scrybe-vcs`,
`scrybe-swarm`, printer/CAD plugins from #33/#34) onto the same registry toward
1.0.

---

## 9. Risks, tradeoffs, and the "extract to modulex plugin later" exit

**Risks / tradeoffs**

- *Live-app coupling.* Routing MCP through the socket means MCP tools that
  mutate the GUI require a running app. Mitigation: the `Headless` transport
  keeps render/lint/mermaid usable with no app, and `--probe` tells agents
  which mode is active. This is strictly better than today's silent spawn.
- *Semantic breaking change.* Unifying `edit`/`section` on the socket
  semantics (line-range / heading-substring) and dropping `DocumentId(uuid)`
  changes the MCP contract. Mitigation: bump the tool `data` versions, land it
  in v0.4/v0.5 (pre-1.0), and document in `AGENTS.md` + `scrybe-mcp-server/README.md`
  (both currently describe the pre-rebuild surface).
- *Socket round-trips add latency.* Negligible for an interactive editor;
  `dispatch_with_reply` already uses a 5 s timeout (`cli_rpc.rs:49`) that is
  generous for the slowest op.
- *Windows.* `cli_rpc.rs` is unix-only today (`cli_rpc.rs:159`). The rebuild
  does not regress this; named-pipe transport stays a tracked follow-up, and
  `Transport` is the clean place to add it.
- *One-crate facets vs real plugin crates.* We consolidate facets into
  `scrybe-tools` features rather than separate crates now, trading strict
  isolation for a smaller diff. Acceptable pre-1.0; the seam below preserves the
  option.

**The exit — extract to `modulex-plugin-scrybe`**

`scrybe-tools` is shaped to be lifted out as a modulex plugin with no rewrite:

- `ToolSpec` fields (`name`, `description`, `input_schema`, versioned
  `data_schema`, `mutates`, `facet`, `handler`) are a superset-compatible map to
  modulex's `ToolSpec`; `Facet` maps to modulex facets; the discovery trio and
  the ≤12 CI-pinned budget already mirror modulex's contract.
- `Transport` is the declared-authority exec leash boundary — the only place
  that touches the outside world — so a modulex host can inject its own.
- No credentials at rest (the socket path is the only ambient dependency).

When the extraction happens, `scrybe-tools` implements modulex's `Plugin`
trait, `scrybe-cli`/`scrybe-mcp-server` keep consuming the same registry, and
Scrybe gains modulex's routine engine for free — **without taking a modulex
dependency now.**
