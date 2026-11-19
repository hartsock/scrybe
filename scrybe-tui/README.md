# scrybe-tui

A Markdown viewer and CSV/TSV editor for the terminal — another lens onto the same `scrybe-core`
AST as the desktop app, CLI, and MCP server. One or more documents in scrollable
panes; two or more files open as a **split screen**; panes **reload live** when
their file changes on disk (edit in your editor, watch the view update).

```bash
scrybe-tui README.md                    # single pane
scrybe-tui data.csv                     # .csv / .tsv open as a scrollable grid
scrybe-tui a.md b.md                    # side-by-side split
scrybe-tui --vertical a.md b.md         # stacked split
```

The renderer + viewer are a **reusable library**: `scrybe_tui::view::MarkdownView`
is a ratatui `StatefulWidget` another project can embed in its own layout via
`frame.render_stateful_widget(view, area, &mut state)`.

## Keys

| Key | Action |
|---|---|
| `j` / `k` / `↓` / `↑` | scroll a line (Markdown); move the cursor a row (grid) |
| `h` / `l` / `←` / `→` | move the cursor a column (grid); no-op on Markdown |
| `Ctrl-d` / `Ctrl-u` | half page (the grid cursor moves with it) |
| `Space` / `PageDown` / `PageUp` | page |
| `g` / `G` | top / bottom (grid: first / last row) |
| `Tab` / `Shift-Tab` | switch focused pane (split) |
| `Ctrl-o` | toggle split orientation (horizontal / vertical), any pane |
| `o` | toggle split orientation on a Markdown pane; **insert a row below** on a grid |
| `q` / `Esc` / `Ctrl-c` | quit (asks first when a grid has unsaved edits) |

Grid panes (CSV/TSV), normal mode:

| Key | Action |
|---|---|
| `Enter` / `i` | edit the cell under the cursor |
| `o` / `O` | insert an empty row below / above and move onto it |
| `dd` | delete the cursor row (two `d` presses, nothing between) |
| `u` / `Ctrl-r` | undo / redo |
| `Ctrl-s` | save (writes a temp file next to it, then renames over the file) |

Edit mode: type to insert, `Backspace` / `Delete` / `←` / `→` / `Home` / `End` edit, `Alt-Enter`
inserts a newline, `Enter` or `Esc` commits, `Ctrl-c` cancels. Every other key is text or ignored.

Each edit changes only that cell's or row's bytes; quoting, line endings, a BOM and ragged rows
elsewhere are kept. A pane with unsaved edits shows ` [+]` in its title. Quitting with unsaved
edits asks: `s` save all and quit, `d` discard and quit, `c` cancel. If the file changes on disk
under unsaved edits the pane is never replaced: `r` reloads (discarding your edits), `k` keeps
yours (a later `Ctrl-s` then overwrites the disk version).

## Scope

Text Markdown: headings, emphasis, lists, block quotes, fenced code, links, and
images (as `🖼 alt` placeholders). Renders `scrybe_core::ast::Node` → styled
`ratatui` text (`src/render.rs`), viewed in a scrollable pane (`src/app.rs`).

CSV / TSV (chosen by file extension, case-insensitive): a read-only grid with
the first row pinned as a header, a row-number gutter, a cursor, and cell editing with undo
and save (`scrybe_ratatui::GridView` plus the `scrybe_core::table` codec). `scrybe view` stays
read-only.

**Not yet** (tracked follow-ups): Mermaid / graphics (a later milestone via a
terminal graphics protocol over the existing `scrybe-mermaid` PNG), and the **live MCP surface** — subscribing to
`~/.scrybe/sock` so `mcp open` drives the TUI exactly like the desktop app.

License: Apache-2.0
