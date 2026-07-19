# scrybe-tui

A Markdown viewer for the terminal — another lens onto the same `scrybe-core`
AST as the desktop app, CLI, and MCP server. One or more documents in scrollable
panes; two or more files open as a **split screen**; panes **reload live** when
their file changes on disk (edit in your editor, watch the view update).

```bash
scrybe-tui README.md                    # single pane
scrybe-tui a.md b.md                    # side-by-side split
scrybe-tui --vertical a.md b.md         # stacked split
```

The renderer + viewer are a **reusable library**: `scrybe_tui::view::MarkdownView`
is a ratatui `StatefulWidget` another project can embed in its own layout via
`frame.render_stateful_widget(view, area, &mut state)`.

## Keys

| Key | Action |
|---|---|
| `j` / `k` / `↓` / `↑` | scroll a line (focused pane) |
| `Ctrl-d` / `Ctrl-u` | half page |
| `Space` / `PageDown` / `PageUp` | page |
| `g` / `G` | top / bottom |
| `Tab` / `Shift-Tab` | switch focused pane (split) |
| `o` | toggle split orientation (horizontal / vertical) |
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
