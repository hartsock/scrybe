# scrybe-tui

A single-pane Markdown viewer for the terminal — another lens onto the same
`scrybe-core` AST as the desktop app, CLI, and MCP server. No tabs, one document.

```bash
scrybe-tui README.md
```

## Keys

| Key | Action |
|---|---|
| `j` / `k` / `↓` / `↑` | scroll a line |
| `Ctrl-d` / `Ctrl-u` | half page |
| `Space` / `PageDown` / `PageUp` | page |
| `g` / `G` | top / bottom |
| `q` / `Esc` / `Ctrl-c` | quit |

## Scope

Text Markdown: headings, emphasis, lists, block quotes, fenced code, links, and
images (as `🖼 alt` placeholders). Renders `scrybe_core::ast::Node` → styled
`ratatui` text (`src/render.rs`), viewed in a scrollable pane (`src/app.rs`).

**Not yet** (tracked follow-ups): Mermaid / graphics (a later milestone via a
terminal graphics protocol over the existing `scrybe-mermaid` PNG), syntect code
highlighting, wrap-aware scroll, and the **live MCP surface** — subscribing to
`~/.scrybe/sock` so `mcp open` drives the TUI exactly like the desktop app.

License: Apache-2.0
