// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! An embeddable ratatui widget for viewing rendered Markdown.
//!
//! [`MarkdownView`] is a `StatefulWidget` another project can drop into its own
//! layout — one half of a split, a popup, a sidebar — and [`MarkdownViewState`]
//! holds the scroll position, adapting to the viewport at render time. Feed it a
//! `Text` produced by [`crate::render`].
//!
//! Scrolling is **wrap-aware** (#163): the widget word-wraps to the viewport
//! width, so the scroll offset counts *visual* (wrapped) rows, not logical
//! `Text` lines. Each render recomputes the wrapped height for the current
//! width ([`wrapped_height`]) and re-clamps the offset, so long lines page to
//! their true end and a resize never strands the view past the end.
//!
//! ```no_run
//! # use ratatui::{Frame, layout::Rect, widgets::{Block, Borders}};
//! # use scrybe_ratatui::{render, view::{MarkdownView, MarkdownViewState}};
//! # fn draw(f: &mut Frame, area: Rect, state: &mut MarkdownViewState) {
//! let text = render::render_source("# Hello\n\nworld");
//! let view = MarkdownView::new(&text).block(Block::default().borders(Borders::ALL));
//! f.render_stateful_widget(view, area, state);
//! # }
//! ```

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget, Wrap};
use unicode_width::UnicodeWidthStr;

/// Number of visual (wrapped) rows `text` occupies when word-wrapped to
/// `width` columns. Returns 0 when `width == 0` (nothing can render).
///
/// This mirrors the exact wrap mode [`MarkdownView`] renders with —
/// `Paragraph::wrap(Wrap { trim: false })`, i.e. ratatui 0.29's `WordWrapper`
/// line composer — grapheme for grapheme, so a scroll offset clamped against
/// this count lines up with what `Paragraph` actually draws. ratatui's own
/// `Paragraph::line_count` computes the same number but sits behind the
/// unstable `unstable-rendered-line-info` feature, which a published crate
/// must not enable; tests pin this function to it as an oracle instead
/// (dev-dependencies only — nothing unstable reaches consumers).
pub fn wrapped_height(text: &Text, width: u16) -> usize {
    if width == 0 {
        return 0;
    }
    text.lines
        .iter()
        .map(|line| wrapped_segments(line, width))
        .sum()
}

const NBSP: &str = "\u{00a0}";
const ZWSP: &str = "\u{200b}";

/// ratatui's `StyledGrapheme::is_whitespace`: a zero-width space wraps like
/// whitespace, a no-break space refuses to, everything else defers to
/// `char::is_whitespace`.
fn is_wrap_whitespace(symbol: &str) -> bool {
    symbol == ZWSP || symbol.chars().all(char::is_whitespace) && symbol != NBSP
}

/// Visual rows one logical [`Line`] occupies at `width` columns — a
/// counting-only port of ratatui 0.29's `WordWrapper::process_input` with
/// `trim: false`. Span boundaries are invisible to wrapping (graphemes flow
/// as one stream, exactly as `Paragraph` composes them); only display widths
/// and ratatui's whitespace predicate drive the state machine. Every logical
/// line yields at least one row (blank lines still take a row).
///
/// Accumulators are `u32` so pathological inputs can't overflow the `u16`
/// arithmetic ratatui uses internally; within `u16` range the branches are
/// a literal transcription.
fn wrapped_segments(line: &Line, width: u16) -> usize {
    let max = u32::from(width);
    let mut rows = 0usize;

    // WordWrapper's buffers, reduced to what counting needs: widths plus
    // emptiness. `pending_ws` keeps per-grapheme widths because the
    // end-of-row step pops trailing whitespace from the front while it fits.
    let mut row_empty = true; // pending_line.is_empty()
    let mut row_width: u32 = 0;
    let mut word_empty = true; // pending_word.is_empty()
    let mut word_width: u32 = 0;
    let mut ws_width: u32 = 0;
    let mut pending_ws: VecDeque<u32> = VecDeque::new();
    let mut after_word = false; // non_whitespace_previous

    for grapheme in line.styled_graphemes(Style::default()) {
        let is_ws = is_wrap_whitespace(grapheme.symbol);
        let w = grapheme.symbol.width() as u32;

        // Symbols wider than the wrap width are ignored entirely.
        if w > max {
            continue;
        }

        let word_found = after_word && is_ws;
        // The word being accumulated (plus its leading whitespace, which
        // trim:false keeps) no longer fits an empty row.
        let untrimmed_overflow = row_empty && word_width + ws_width + w > max;

        // Commit the finished word (and the whitespace before it) to the row.
        if word_found || untrimmed_overflow {
            if !pending_ws.is_empty() {
                row_empty = false;
            }
            row_width += ws_width;
            pending_ws.clear();
            if !word_empty {
                row_empty = false;
            }
            row_width += word_width;
            word_empty = true;
            ws_width = 0;
            word_width = 0;
        }

        let row_full = row_width >= max;
        // The pending word has grown to the wrap width and must break.
        let pending_word_overflow = w > 0 && row_width + ws_width + word_width >= max;

        if row_full || pending_word_overflow {
            let mut remaining = max.saturating_sub(row_width);
            rows += 1; // emit the row
            row_empty = true;
            row_width = 0;

            // Whitespace that would have fit at the end of the emitted row
            // is dropped rather than carried to the next row.
            while let Some(&front) = pending_ws.front() {
                if front > remaining {
                    break;
                }
                ws_width -= front;
                remaining -= front;
                pending_ws.pop_front();
            }
            // The whitespace that triggered the break does not carry over.
            if is_ws && pending_ws.is_empty() {
                continue;
            }
        }

        if is_ws {
            ws_width += w;
            pending_ws.push_back(w);
        } else {
            word_width += w;
            word_empty = false;
        }
        after_word = !is_ws;
    }

    // End-of-line flush, exactly as WordWrapper does it: a line holding
    // nothing but pending whitespace first emits an empty row; trim:false
    // then keeps the whitespace itself as a row of content.
    if row_empty && word_empty && !pending_ws.is_empty() {
        rows += 1;
    }
    if !pending_ws.is_empty() || !word_empty {
        row_empty = false;
    }
    if !row_empty {
        rows += 1;
    }
    rows.max(1)
}

/// Scroll state for a [`MarkdownView`] — one per viewer pane.
///
/// **The scroll offset is measured in visual (wrapped) rows at the
/// last-rendered viewport width**, not logical `Text` lines: the widget
/// word-wraps, so one long logical line can occupy many rows. Both the
/// viewport height and the wrapped row count are learned at render time, so
/// scroll helpers (page, half-page, bottom) are correct against whatever area
/// the widget was last drawn into, and every render re-clamps the offset —
/// a resize can never strand the view past the end of the document.
///
/// Before the first render the row count is whatever estimate
/// [`Self::new`] / [`Self::set_line_count`] were given (typically the
/// unwrapped `text.lines.len()`); the first render corrects it. Cheap to
/// clone.
#[derive(Debug, Default, Clone)]
pub struct MarkdownViewState {
    scroll: u16,
    viewport: u16,
    line_count: usize,
}

impl MarkdownViewState {
    /// State for a document with `line_count` rendered lines.
    ///
    /// The count is an initial estimate (the unwrapped line count is fine);
    /// the first render replaces it with the wrapped row count for the
    /// actual viewport width.
    pub fn new(line_count: usize) -> Self {
        Self {
            scroll: 0,
            viewport: 1,
            line_count,
        }
    }

    /// Update the rendered line count after the content changes (live
    /// re-render), keeping the scroll offset in range. As with [`Self::new`]
    /// this is an estimate until the next render recomputes the wrapped
    /// height for the current width.
    pub fn set_line_count(&mut self, line_count: usize) {
        self.line_count = line_count;
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Current top-row offset, in visual (wrapped) rows at the last-rendered
    /// width.
    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Largest valid scroll offset, given the last-rendered viewport height
    /// and wrapped row count. At this offset the last visual row sits exactly
    /// on the bottom row of the viewport.
    pub fn max_scroll(&self) -> u16 {
        (self.line_count.min(u16::MAX as usize) as u16).saturating_sub(self.viewport)
    }

    /// Scroll by a signed row delta, clamped into range.
    pub fn scroll_by(&mut self, delta: i32) {
        let next = (self.scroll as i32 + delta).clamp(0, self.max_scroll() as i32);
        self.scroll = next as u16;
    }

    /// Jump to the top.
    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    /// Jump to the bottom: the last visual row lands on the bottom row of
    /// the viewport.
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
    /// Wrap-aware: a single logical line that wraps past the viewport does
    /// not fit.
    pub fn fits(&self) -> bool {
        self.max_scroll() == 0
    }

    /// Record the viewport height and the wrapped row count for the width
    /// being rendered, and re-clamp the scroll. Called by the widget every
    /// render, before the offset is handed to `Paragraph`.
    fn sync_layout(&mut self, height: u16, wrapped_rows: usize) {
        self.viewport = height.max(1);
        self.line_count = wrapped_rows;
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
        // Learn the viewport and the wrapped height for *this* width, and
        // re-clamp, before Paragraph consumes the offset. Paragraph's
        // vertical scroll skips wrapped rows, so the visual-row offset maps
        // 1:1 onto what it draws.
        state.sync_layout(inner.height, wrapped_height(self.text, inner.width));
        Paragraph::new(self.text.clone())
            .wrap(Wrap { trim: false })
            .scroll((state.scroll, 0))
            .render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Stylize;
    use ratatui::text::Span;
    use ratatui::widgets::Borders;

    fn state(lines: usize, viewport: u16) -> MarkdownViewState {
        let mut s = MarkdownViewState::new(lines);
        s.sync_layout(viewport, lines);
        s
    }

    /// Render `text` into a fresh buffer of `width`×`height`, returning the
    /// rows as strings (no block).
    fn render_rows(
        text: &Text,
        st: &mut MarkdownViewState,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        MarkdownView::new(text).render(area, &mut buf, st);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    // ---- wrapped_height: the pure wrap computation ------------------------

    #[test]
    fn long_unbroken_line_breaks_at_the_wrap_width() {
        let text = Text::from("a".repeat(25));
        assert_eq!(wrapped_height(&text, 10), 3); // 10 + 10 + 5
        assert_eq!(wrapped_height(&text, 25), 1);
        assert_eq!(wrapped_height(&text, 24), 2);
    }

    #[test]
    fn exactly_viewport_width_line_takes_one_row() {
        let text = Text::from("a".repeat(10));
        assert_eq!(wrapped_height(&text, 10), 1);
        assert_eq!(wrapped_height(&text, 9), 2);
    }

    #[test]
    fn empty_lines_take_one_row_each() {
        assert_eq!(wrapped_height(&Text::from(""), 10), 1);
        assert_eq!(wrapped_height(&Text::from("a\n\nb"), 10), 3);
        // A Text with no lines at all occupies no rows.
        assert_eq!(wrapped_height(&Text::default(), 10), 0);
    }

    #[test]
    fn words_wrap_at_word_boundaries() {
        let text = Text::from("hello world");
        assert_eq!(wrapped_height(&text, 11), 1);
        assert_eq!(wrapped_height(&text, 5), 2); // "hello" / "world"
        assert_eq!(wrapped_height(&text, 10), 2);
    }

    #[test]
    fn multi_span_styled_lines_wrap_as_one_stream() {
        // Span boundaries are invisible to wrapping: styled or not, the
        // graphemes flow as a single stream.
        let styled = Text::from(Line::from(vec![
            Span::raw("hello "),
            Span::styled("world", Style::new().bold()),
        ]));
        let plain = Text::from("hello world");
        for width in 1..=20 {
            assert_eq!(
                wrapped_height(&styled, width),
                wrapped_height(&plain, width),
                "styled vs plain diverged at width {width}"
            );
        }
    }

    #[test]
    fn wide_graphemes_wrap_by_display_width() {
        // CJK: each ideograph is 2 columns wide.
        let cjk = Text::from("你好世界");
        assert_eq!(wrapped_height(&cjk, 8), 1);
        assert_eq!(wrapped_height(&cjk, 4), 2);
        assert_eq!(wrapped_height(&cjk, 2), 4);
        // Emoji: 2 columns each.
        let emoji = Text::from("😀😀😀");
        assert_eq!(wrapped_height(&emoji, 6), 1);
        assert_eq!(wrapped_height(&emoji, 4), 2); // "😀😀" / "😀"
    }

    #[test]
    fn zero_width_viewport_renders_nothing() {
        assert_eq!(wrapped_height(&Text::from("anything"), 0), 0);
    }

    #[test]
    fn whitespace_predicate_matches_ratatui() {
        // ZWSP wraps like whitespace, NBSP refuses to, per ratatui's
        // StyledGrapheme::is_whitespace.
        assert!(is_wrap_whitespace(" "));
        assert!(is_wrap_whitespace("\t"));
        assert!(is_wrap_whitespace(ZWSP));
        assert!(!is_wrap_whitespace(NBSP));
        assert!(!is_wrap_whitespace("a"));
    }

    /// Pin `wrapped_height` to ratatui's own wrapped-row count
    /// (`Paragraph::line_count`, unstable feature enabled for **tests
    /// only** via dev-dependencies) across a corpus of tricky content and
    /// every width 0..=40. This is the equivalence proof that the scroll
    /// clamp agrees with what `Paragraph` actually draws.
    #[test]
    fn wrapped_height_matches_paragraph_line_count_oracle() {
        let corpus: Vec<Text> = vec![
            Text::from(""),
            Text::from(" "),
            Text::from("   \t  "),
            Text::from("hello world"),
            Text::from("a".repeat(25)),
            Text::from("a".repeat(200)),
            Text::from(format!("aaa{ZWSP}bbb")),
            Text::from(format!("aaa{NBSP}bbb")),
            Text::from("你好世界 hello 世界"),
            Text::from("😀😀😀 emoji wrap 😀"),
            Text::from("word  double  spaces   and trailing   "),
            Text::from("multi\nline\n\ntext with several words and runs"),
            Text::from(Line::from(vec![
                Span::raw("styled "),
                Span::styled("multi", Style::new().bold()),
                Span::styled("-span line with 你好 and 😀", Style::new().italic()),
            ])),
            crate::render::render_source(
                "# Heading\n\nA paragraph with **bold**, `code`, and a very \
                 long unbroken word aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\n\
                 - list item one\n- list item two\n",
            ),
        ];
        for text in &corpus {
            for width in 0..=40u16 {
                let oracle = Paragraph::new(text.clone())
                    .wrap(Wrap { trim: false })
                    .line_count(width);
                assert_eq!(
                    wrapped_height(text, width),
                    oracle,
                    "diverged from Paragraph::line_count at width {width} for {text:?}"
                );
            }
        }
    }

    // ---- state: clamping in visual rows -----------------------------------

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
        let s = state(3, 10); // 3 rows, 10-row viewport
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

    // ---- render: wrap-aware clamping against real buffers -----------------

    /// Regression test for #163: v1 clamped against the *unwrapped* line
    /// count, so a document that is one long logical line had
    /// `max_scroll == 0` and could never page to its true end. Wrap-aware,
    /// the same document scrolls through all of its visual rows.
    #[test]
    fn regression_163_long_line_pages_to_its_true_end() {
        let text = Text::from("a".repeat(25)); // 3 visual rows at width 10
        let mut st = MarkdownViewState::new(text.lines.len()); // v1 estimate: 1
        let rows = render_rows(&text, &mut st, 10, 2);
        assert_eq!(rows, vec!["aaaaaaaaaa", "aaaaaaaaaa"]);
        // Wrapped height (3) minus viewport (2): one row below the fold.
        assert_eq!(st.max_scroll(), 1, "clamp must use the wrapped height");
        assert!(!st.fits());

        st.scroll_to_bottom();
        let rows = render_rows(&text, &mut st, 10, 2);
        assert_eq!(st.scroll(), 1);
        // The last visual row lands exactly on the viewport's bottom row.
        assert_eq!(rows, vec!["aaaaaaaaaa", "aaaaa"]);
    }

    #[test]
    fn jump_to_end_lands_last_visual_row_on_the_bottom_row() {
        // Mixed content: short lines plus one long wrapping line.
        let text = Text::from(format!("top\n{}\nend", "b".repeat(35)));
        let width = 10u16;
        let mut st = MarkdownViewState::new(text.lines.len());
        let _ = render_rows(&text, &mut st, width, 3);
        // 1 (top) + 4 (35 b's at width 10) + 1 (end) = 6 visual rows.
        assert_eq!(wrapped_height(&text, width), 6);
        assert_eq!(st.max_scroll(), 3);

        st.scroll_to_bottom();
        let rows = render_rows(&text, &mut st, width, 3);
        // Bottom row is the document's last visual row — no blank overscroll.
        assert_eq!(rows, vec!["bbbbbbbbbb", "bbbbb", "end"]);
    }

    #[test]
    fn resize_reclamps_so_the_view_never_overscrolls() {
        let text = Text::from("a".repeat(25));
        let mut st = MarkdownViewState::new(text.lines.len());

        // Narrow: 3 visual rows in a 2-row viewport; jump to the end.
        let _ = render_rows(&text, &mut st, 10, 2);
        st.scroll_to_bottom();
        assert_eq!(st.scroll(), 1);

        // Resize wider: the line no longer wraps (1 row), the old offset
        // would point past the end. The render re-clamps before drawing, so
        // the content is on screen, not blank space.
        let rows = render_rows(&text, &mut st, 30, 2);
        assert_eq!(st.scroll(), 0, "wider resize must re-clamp the offset");
        assert_eq!(rows[0], "a".repeat(25));

        // Resize narrower again: more visual rows, the true end is reachable.
        st.scroll_to_bottom();
        assert_eq!(st.scroll(), 0, "estimate from last width until re-render");
        let _ = render_rows(&text, &mut st, 5, 2);
        st.scroll_to_bottom();
        let rows = render_rows(&text, &mut st, 5, 2);
        assert_eq!(st.scroll(), 3); // 5 rows at width 5, viewport 2
        assert_eq!(rows, vec!["aaaaa", "aaaaa"]);
    }

    #[test]
    fn block_frame_wraps_to_the_inner_width() {
        let text = Text::from("a".repeat(25));
        let mut st = MarkdownViewState::new(text.lines.len());
        let area = Rect::new(0, 0, 12, 4); // borders: inner 10×2
        let mut buf = Buffer::empty(area);
        MarkdownView::new(&text)
            .block(Block::default().borders(Borders::ALL))
            .render(area, &mut buf, &mut st);
        // Wrapped at the inner width (10 → 3 rows), viewport is inner height.
        assert_eq!(st.max_scroll(), 1);
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
        // Rendering learned the viewport height; nothing wraps at width 30,
        // so the wrapped height equals the logical line count.
        assert_eq!(wrapped_height(&text, 30), text.lines.len());
        assert_eq!(st.max_scroll(), (text.lines.len() as u16).saturating_sub(6));
    }
}
