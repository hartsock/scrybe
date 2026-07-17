// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! An embeddable ratatui widget for viewing rendered Markdown.
//!
//! [`MarkdownView`] is a `StatefulWidget` another project can drop into its own
//! layout — one half of a split, a popup, a sidebar — and [`MarkdownViewState`]
//! holds the scroll position, adapting to the viewport at render time. Feed it a
//! `Text` produced by [`crate::render`].
//!
//! ```no_run
//! # use ratatui::{Frame, layout::Rect, widgets::{Block, Borders}};
//! # use scrybe_tui::{render, view::{MarkdownView, MarkdownViewState}};
//! # fn draw(f: &mut Frame, area: Rect, state: &mut MarkdownViewState) {
//! let text = render::render_source("# Hello\n\nworld");
//! let view = MarkdownView::new(&text).block(Block::default().borders(Borders::ALL));
//! f.render_stateful_widget(view, area, state);
//! # }
//! ```

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget, Wrap};

/// Scroll state for a [`MarkdownView`] — one per viewer pane.
///
/// The viewport height is learned at render time, so scroll helpers (page,
/// half-page, bottom) are correct against whatever area the widget was last
/// drawn into. Cheap to clone.
#[derive(Debug, Default, Clone)]
pub struct MarkdownViewState {
    scroll: u16,
    viewport: u16,
    line_count: usize,
}

impl MarkdownViewState {
    /// State for a document with `line_count` rendered lines.
    pub fn new(line_count: usize) -> Self {
        Self {
            scroll: 0,
            viewport: 1,
            line_count,
        }
    }

    /// Update the rendered line count after the content changes (live re-render),
    /// keeping the scroll offset in range.
    pub fn set_line_count(&mut self, line_count: usize) {
        self.line_count = line_count;
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Current top-line offset.
    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Largest valid scroll offset, given the last-rendered viewport height.
    pub fn max_scroll(&self) -> u16 {
        (self.line_count as u16).saturating_sub(self.viewport)
    }

    /// Scroll by a signed line delta, clamped into range.
    pub fn scroll_by(&mut self, delta: i32) {
        let next = (self.scroll as i32 + delta).clamp(0, self.max_scroll() as i32);
        self.scroll = next as u16;
    }

    /// Jump to the top.
    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    /// Jump to the bottom.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.max_scroll();
    }

    /// Half-viewport scroll (Ctrl-d / Ctrl-u). `down = false` scrolls up.
    pub fn half_page(&mut self, down: bool) {
        let half = (self.viewport / 2).max(1) as i32;
        self.scroll_by(if down { half } else { -half });
    }

    /// Near-full-viewport scroll (PageDown / PageUp / Space). `down = false` up.
    pub fn page(&mut self, down: bool) {
        let page = self.viewport.saturating_sub(1).max(1) as i32;
        self.scroll_by(if down { page } else { -page });
    }

    /// Scroll progress, 0–100%. When the document fits the viewport there is
    /// nothing to scroll; this returns 0 (see [`Self::fits`] to distinguish
    /// "fits entirely" from "at the top of a longer document").
    pub fn percent(&self) -> u16 {
        let max = self.max_scroll();
        if max == 0 {
            0
        } else {
            (self.scroll as u32 * 100 / max as u32).min(100) as u16
        }
    }

    /// Whether the whole document fits the viewport (nothing to scroll).
    pub fn fits(&self) -> bool {
        self.max_scroll() == 0
    }

    /// Record the viewport height at render time and re-clamp the scroll.
    fn sync_viewport(&mut self, height: u16) {
        self.viewport = height.max(1);
        self.scroll = self.scroll.min(self.max_scroll());
    }
}

/// A scrollable, word-wrapped view of rendered Markdown `Text`, optionally
/// framed by a `Block`. Render it with `render_stateful_widget`.
pub struct MarkdownView<'a> {
    text: &'a Text<'a>,
    block: Option<Block<'a>>,
}

impl<'a> MarkdownView<'a> {
    /// A view over already-rendered Markdown text (see [`crate::render`]).
    pub fn new(text: &'a Text<'a>) -> Self {
        Self { text, block: None }
    }

    /// Frame the view in a bordered block (title, borders, …).
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl StatefulWidget for MarkdownView<'_> {
    type State = MarkdownViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let inner = match self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.render(area, buf);
                inner
            }
            None => area,
        };
        state.sync_viewport(inner.height);
        Paragraph::new(self.text.clone())
            .wrap(Wrap { trim: false })
            .scroll((state.scroll, 0))
            .render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(lines: usize, viewport: u16) -> MarkdownViewState {
        let mut s = MarkdownViewState::new(lines);
        s.sync_viewport(viewport);
        s
    }

    #[test]
    fn scroll_clamps_within_bounds() {
        let mut s = state(20, 5); // max_scroll = 15
        s.scroll_by(100);
        assert_eq!(s.scroll(), 15);
        s.scroll_by(-100);
        assert_eq!(s.scroll(), 0);
    }

    #[test]
    fn top_bottom_and_pages() {
        let mut s = state(100, 10); // max_scroll = 90
        s.scroll_to_bottom();
        assert_eq!(s.scroll(), 90);
        s.scroll_to_top();
        assert_eq!(s.scroll(), 0);
        s.half_page(true);
        assert_eq!(s.scroll(), 5);
        s.page(true);
        assert_eq!(s.scroll(), 5 + 9);
        s.page(false);
        assert_eq!(s.scroll(), 5);
    }

    #[test]
    fn percent_reports_progress() {
        let mut s = state(20, 5);
        assert_eq!(s.percent(), 0);
        s.scroll_to_bottom();
        assert_eq!(s.percent(), 100);
    }

    #[test]
    fn fits_when_content_shorter_than_viewport() {
        let s = state(3, 10); // 3 lines, 10-row viewport
        assert!(s.fits());
        assert_eq!(s.percent(), 0);
        assert_eq!(s.max_scroll(), 0);
        // A longer document does not fit.
        assert!(!state(50, 10).fits());
    }

    #[test]
    fn set_line_count_reclamps_scroll() {
        let mut s = state(100, 10);
        s.scroll_to_bottom(); // 90
        s.set_line_count(20); // max now 10
        assert_eq!(s.scroll(), 10);
    }

    #[test]
    fn widget_renders_text_into_a_buffer() {
        let text = crate::render::render_source("# Hi\n\nbody text\n");
        let mut st = MarkdownViewState::new(text.lines.len());
        let area = Rect::new(0, 0, 30, 6);
        let mut buf = Buffer::empty(area);
        MarkdownView::new(&text).render(area, &mut buf, &mut st);

        let mut screen = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                screen.push_str(buf[(x, y)].symbol());
            }
            screen.push('\n');
        }
        assert!(screen.contains("Hi"), "heading missing:\n{screen}");
        assert!(screen.contains("body text"), "body missing:\n{screen}");
        // Rendering learned the viewport height (area, no block).
        assert_eq!(st.max_scroll(), (text.lines.len() as u16).saturating_sub(6));
    }
}
