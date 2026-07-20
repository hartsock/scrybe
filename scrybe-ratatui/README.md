# scrybe-ratatui

Render a [`scrybe-core`](https://crates.io/crates/scrybe-core) Markdown
document as styled [`ratatui`](https://ratatui.rs) text — an embeddable
Markdown view for any ratatui app, extracted from Scrybe's terminal viewer.

Dependencies: `scrybe-core` + `ratatui`. No crossterm, no file IO, no event
loop, no terminal backend — you own the loop; the widget drops into your
layout.

## Use

```rust
use ratatui::{Frame, layout::Rect, widgets::{Block, Borders}};
use scrybe_ratatui::{render_source, MarkdownView, MarkdownViewState};

fn draw(f: &mut Frame, area: Rect, state: &mut MarkdownViewState) {
    let text = render_source("# Hello\n\nSome *styled* Markdown.");
    let view = MarkdownView::new(&text).block(Block::default().borders(Borders::ALL));
    f.render_stateful_widget(view, area, state);
}
```

`render(&Ast)` renders an already-parsed `scrybe_core::ast::Ast`;
`render_source(&str)` parses and renders in one call. `MarkdownViewState`
holds the scroll offset and adapts to the viewport at render time.

## Compatibility

`MarkdownView` implements ratatui's `StatefulWidget`, which ties each release
of this crate to a ratatui major line (currently **0.29**); a ratatui bump is
a semver event here.

Part of the [Scrybe](https://github.com/hartsock/scrybe) workspace; versions
ship in lock-step with the other `scrybe-*` crates.

SPDX-License-Identifier: Apache-2.0
