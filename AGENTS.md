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

| Tool | Description | Key Args |
|---|---|---|
| `open` | Open a file (returns doc ID); also launches GUI | `path` |
| `read` | Read Markdown source of an open document | `id` |
| `section` | Extract a heading section by H-level and index | `id`, `level`, `index` |
| `edit` | Replace first occurrence of text in a document | `id`, `old`, `new` |
| `find` | Search for a string with line context | `id`, `query` |
| `render` | Render document to HTML | `id`, `theme?` |
| `embed` | Embed Mermaid source into a PNG (iTXt) | `png_path`, `source` |
| `extract` | Extract Mermaid source from a PNG | `png_path` |
| `lint` | Word count, headings, code blocks, links | `id` |
| `logs` | Read recent console log entries from the GUI | `tail?` |
| `close_tab` | Close a tab by path (omit path = close active tab) | `path?` |
| `quit` | Gracefully close the Scrybe GUI window | — |

### Document IDs

The `open` tool returns a `DocumentId(uuid)` string. Pass the full string
(including `DocumentId(...)`) to subsequent tools. IDs are scoped to the
current MCP server process session.

```json
// open returns:
{"id": "DocumentId(4ec5463d-9f3f-487b-83bc-e0e6ab586388)", "path": "/path/to/file.md"}

// use full string in subsequent calls:
{"id": "DocumentId(4ec5463d-9f3f-487b-83bc-e0e6ab586388)"}
```

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

Preferred workflow for modifying a document:

```
1. open(path)           → get doc ID
2. read(id)             → inspect current content
3. find(id, query)      → locate target text
4. edit(id, old, new)   → make change (first occurrence only)
5. render(id)           → verify rendered output
```

The file is **not written to disk** via MCP tools — `edit` updates the
in-memory workspace. The running GUI writes to disk via autosave (1 s
debounce). If running headlessly, call `render` or `lint` but manage disk
writes yourself.

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
2. Create a feature branch: `feat/description` or `fix/description`
3. All checks must pass before pushing (pre-push hook enforces this)
4. Open a PR via `gh pr create`
5. Include agent identity in the PR description
