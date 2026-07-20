# AGENTS.md — Scrybe Agent Interaction Guide

This document is for AI agents (Claude, Codex, Ollama, etc.) working with Scrybe
as an MCP client, as a code contributor, or as a swarm participant.

## Connecting to a Running Scrybe Instance

```bash
# Add to Claude Code
claude mcp add scrybe -- scrybe-mcp-server stdio

# Verify tools are available
scrybe-mcp-server tools
```

## MCP Tools Reference

Every tool is served by the ONE shared `scrybe-tools` registry (the same
handlers the CLI uses), so this table is also the CLI surface. Tabs are
addressed by canonical `path`.

| Tool | Description | Key Args |
|---|---|---|
| `open` | Open (or refresh) a file as a tab in the running editor | `path` |
| `read` | Read an open tab's buffer (sees unsaved edits) | `path` |
| `section` | Extract a heading section by heading text (substring) | `path`, `heading` |
| `edit` | Replace an inclusive 1-indexed LINE RANGE with new content (live buffer) | `path`, `start_line`, `end_line`, `content` |
| `save` | Write an open tab's buffer to its file (explicit persist; buffers stay dirty until saved) | `path` |
| `find` | Regex/literal search over open tabs (or named paths, falling back to disk) | `pattern`, `paths?`, `literal?` |
| `render` | Render Markdown to HTML | `source`, `theme?` |
| `embed` | Embed Mermaid source into an existing PNG (iTXt), in place | `png_path`, `source` |
| `extract` | Extract Mermaid source from a PNG, verifying its sha256 (mismatch → `verification_failed`; `verification: "verified"` or `"no-digest"`) | `png_path` |
| `lint` | Word count, headings, code blocks, links | `source` |
| `list_tabs` | List the running app's open tabs | — |
| `mermaid_to_png` | Render a Mermaid diagram to PNG + embed its source | `source`, `output_path` |
| `logs` | Read recent console log entries from the GUI | `tail?` |
| `reload` | Re-read an open tab from disk into its live buffer | `path`, `force?` |
| `close_tab` | Close a tab by its canonical path | `path` |
| `quit` | Gracefully close the Scrybe GUI window | `force?` |
| `state` | Report the GUI's active path, view mode, theme, and Vim state | — |
| `set_theme` | Set editor + preview theme (human: theme dropdown) | `theme` |
| `view_mode` | Set active tab view mode (human: View button) | `mode` |
| `set_vim` | Toggle Vim keybindings (human: Vim toggle) | `enabled` |
| `export` | Export Markdown to Word (.docx) with Mermaid PNGs | `path`, `output?`, `no_diagrams?` |
| `export_figures` | Export every Mermaid diagram in a document to sibling `<stem>_fig_NN.png` files (each embeds its source) | `path` |

> **Parity rule:** every human control in scrybe-app has an MCP equivalent
> and vice versa. The `state`/`set_theme`/`view_mode`/`set_vim`/`export`/
> `export_figures` tools mirror the path bar, theme dropdown, View button,
> Vim toggle, the Export button, and the "Export Diagrams…" menu item
> respectively.

> `embed`, `extract`, `export`, `mermaid_to_png`, `export_figures`, `render`,
> and `lint` are **in-process** — they work headless, no running app needed.
> The stateful editor/UI tools return a `no_live_app` tool_error when no app
> is running.

## Opening Files in the GUI

When Scrybe is running, calling `open` with a file path also opens it in the
GUI (via the `scrybe://open` single-instance event). Use `scrybe` CLI for
the same effect:

```bash
scrybe path/to/file.md    # open file in GUI + load into MCP workspace
scrybe ./                 # open directory browser in GUI
scrybe                    # open welcome screen
```

## Agent Identification (Required)

All agents MUST identify themselves in any external-facing communication:
GitHub issues, PR descriptions, commit co-author lines, git notes.

**Format:** `Nickname (machine context, model)` — e.g., `Beaver (MacBook, Claude Sonnet 4.6)`

Co-author line in commits:
```
Co-Authored-By: Beaver (MacBook, Claude Sonnet 4.6) <noreply@anthropic.com>
```

## Editing Documents via MCP

Preferred workflow for modifying a document (tabs are addressed by `path`):

```
1. open(path)                                  → tab is live in the editor
2. read(path)                                  → inspect the buffer (+ is_dirty)
3. find(pattern, paths?)                       → locate target text
4. edit(path, start_line, end_line, content)   → change the buffer (stays dirty)
5. read(path) / render                         → verify the result
6. save(path)                                  → persist to disk — or don't
```

`edit` changes only the **in-memory buffer** and leaves the tab dirty —
deliberately. The dirty buffer is the review step: verify first, then call
`save(path)` to write the file, or skip `save` (and `reload` with `force`)
to discard. Autosave only writes a `<path>.scrybe-buffer` crash-recovery
sidecar, never the real file. The human's Cmd+S / 💾 and the agent's `save`
are the same explicit act. Headless (no running app), the editor tools
return a `no_live_app` tool_error — manage disk writes yourself.

## Swarm Participation (scrybe-swarm)

Scrybe supports a NATS-based swarm for multi-agent collaboration on the
same document. Swarm agents receive activity feed events when the active
document changes, tabs open/close, or content is edited.

Connection: configure NATS endpoint in `~/.config/scrybe/swarm.toml`.

## Reading GUI Logs

The GUI forwards `console.log/warn/error` to `/tmp/scrybe-debug.log`.
Use the `logs` MCP tool or read the file directly to observe GUI state
without opening DevTools.

```bash
# Via MCP
{"tool": "logs", "arguments": {"tail": 50}}

# Directly
tail -f /tmp/scrybe-debug.log
```

## Branch and PR Rules for Agent Contributions

1. Never commit directly to `main`
2. Create a feature branch: `feat/description`, `fix/description`,
   `chore/description`, or `docs/description`
3. All checks must pass before pushing (pre-push hook enforces this)
4. Open a PR via `gh pr create`
5. Include agent identity in the PR description
6. Apply a risk label (see below) so the autonomy rules can be applied

## Risk Classification

Every PR carries one of two labels.

A change is **`risk:low`** if ALL of these are true:

- Scoped to a single issue, bug fix, doc update, or version bump
- Has a regression test for every behavioral fix
- Touches no CI/CD workflows, push hooks, or build configuration
- Deletes nothing that isn't provably dead
- Local `just check` and `just test` pass

Everything else is **`risk:high`** — including but not limited to:

- Multi-issue / cross-cutting changes
- Edits to `.github/workflows/`, `.githooks/`, `Cargo.toml` workspace
  membership, `justfile`, `pyproject.toml` build metadata
- Code or asset deletions whose dead-ness isn't obvious
- Any change without a corresponding test
- Anything an agent feels uncertain about — when in doubt, label `risk:high`

The labels exist in the repo (`risk:low` green, `risk:high` red). If a PR
lands without a risk label, treat it as `risk:high` until labeled.

## Agent Autonomy Table

| Action | Autonomous? |
|---|---|
| Create branch + push | Yes |
| Open PR | Yes |
| Push fixup commits to own PR | Yes |
| Apply / change a risk label | Yes |
| Merge a `risk:low` PR after CI green | Yes |
| Merge a `risk:high` PR | Only after explicit human approval |
| Tag and push `v*` (cuts a release) | Only when the human asks for a release |
| Cut a `release/0.Y.x` branch | Only when a real backport is in flight (see `RELEASE.md`) |
| Close issues | Only via `Fixes #N` in a merged PR body |
| Force-push to any branch | Never |
| Bypass push hooks (`--no-verify`) | Never |

## CI / Hook Parity

The push hook is the local equivalent of the CI pipeline. When editing
either side, audit the other:

- Editing `.github/workflows/*.yml`? Update `.githooks/pre-push` to match.
- Editing `.githooks/pre-push`? Update the workflows to match.

Both files should carry a top-of-file comment naming their counterpart so
the relationship is auditable.
