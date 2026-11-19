// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! A read-only ratatui widget for viewing a CSV/TSV [`Table`].
//!
//! [`GridView`] draws row 0 as a pinned bold header, a dim 1-based row-number
//! gutter, and the body rows beneath it. Column widths are measured once
//! ([`GridViewState::new`] / [`GridViewState::sync`]), not per frame, and only
//! the visible window of rows and columns is built each render.
//!
//! Cell text is sanitized before it is measured or drawn: `\n` shows as `⏎`,
//! `\r` as `␍`, `\t` as `⇥`, any other control character as `·`. A cell wider
//! than `MAX_COL_WIDTH` (30 columns) is truncated with `…`.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Cell, Row, StatefulWidget, Table as RatTable, Widget};
use scrybe_core::table::Table;
use unicode_width::UnicodeWidthStr;

use crate::render::{truncate, MAX_COL_WIDTH};

/// Spaces between two columns (ratatui `Table`'s default spacing).
const SPACING: usize = 1;

/// Scroll and layout state for a [`GridView`] — one per pane.
///
/// Row offsets count *body* rows (the header is pinned and never scrolls);
/// the column offset is the first visible data column. The viewport is
/// learned at render time and every render re-clamps the offsets.
#[derive(Debug, Clone, Default)]
pub struct GridViewState {
    row_offset: usize,
    col_offset: usize,
    widths: Vec<u16>,
    rows: usize,
    cols: usize,
    /// Body rows that fit below the header at the last render.
    viewport_rows: usize,
    /// Whether a render has set the viewport (before that, scrolling is free).
    rendered: bool,
    /// Columns available to the gutter plus data columns at the last render.
    viewport_width: usize,
    /// Optional cursor `(file row, column)`, both 0-based.
    cursor: Option<(usize, usize)>,
    /// The cursor moved since the last render: scroll it into view once.
    follow: bool,
}

impl GridViewState {
    /// State for `table`, with column widths measured.
    pub fn new(table: &Table) -> Self {
        let mut state = Self::default();
        state.sync(table);
        state
    }

    /// Re-measure after the table changed (live reload): recompute widths and
    /// re-clamp the offsets, keeping the scroll position.
    pub fn sync(&mut self, table: &Table) {
        self.rows = table.row_count();
        self.cols = table.col_count();
        let mut widths = vec![1usize; self.cols];
        for r in 0..self.rows {
            for (c, w) in widths.iter_mut().enumerate() {
                if let Some(cell) = table.cell(r, c) {
                    *w = (*w).max(sanitize(&cell).width().min(MAX_COL_WIDTH));
                }
            }
        }
        self.widths = widths.into_iter().map(|w| w as u16).collect();
        self.clamp();
        if let Some((r, c)) = self.cursor {
            self.set_cursor(r, c);
        }
    }

    /// Put the cursor on file row `row`, column `col` (row 0 is the header),
    /// clamped to the table. The next render scrolls it into view.
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        let row = row.min(self.rows.saturating_sub(1));
        let col = col.min(self.cols.saturating_sub(1));
        self.cursor = Some((row, col));
        self.follow = true;
    }

    /// The cursor `(file row, column)`, if one is set.
    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }

    /// Move the cursor by a signed delta, clamped to the table; a cursor that
    /// is not set starts from the top-left cell.
    pub fn move_cursor(&mut self, drow: i32, dcol: i32) {
        let (r, c) = self.cursor.unwrap_or((0, 0));
        let r = (r as i64 + i64::from(drow)).max(0) as usize;
        let c = (c as i64 + i64::from(dcol)).max(0) as usize;
        self.set_cursor(r, c);
    }

    /// Scroll rows and columns so the cursor cell is visible (once per move).
    fn follow_cursor(&mut self) {
        let Some((r, c)) = self.cursor else { return };
        if r > 0 && self.viewport_rows > 0 {
            let body = r - 1;
            if body < self.row_offset {
                self.row_offset = body;
            } else if body >= self.row_offset + self.viewport_rows {
                self.row_offset = body + 1 - self.viewport_rows;
            }
        }
        if c < self.col_offset {
            self.col_offset = c;
        }
        // Bounded by `cols`: at `col_offset == c` the column is always shown.
        while self.col_offset < c && !self.visible_cols().iter().any(|&(v, _)| v == c) {
            self.col_offset += 1;
        }
    }

    /// Scroll body rows by a signed delta, clamped into range.
    pub fn scroll_by(&mut self, delta: i32) {
        self.row_offset = offset_by(self.row_offset, delta, self.max_row_offset());
    }

    /// Scroll columns by a signed delta, clamped into range.
    pub fn scroll_cols_by(&mut self, delta: i32) {
        self.col_offset = offset_by(self.col_offset, delta, self.max_col_offset());
    }

    /// Half-viewport scroll. `down = false` scrolls up.
    pub fn half_page(&mut self, down: bool) {
        let half = (self.viewport_rows / 2).max(1) as i32;
        self.scroll_by(if down { half } else { -half });
    }

    /// Near-full-viewport scroll. `down = false` scrolls up.
    pub fn page(&mut self, down: bool) {
        let page = self.viewport_rows.saturating_sub(1).max(1) as i32;
        self.scroll_by(if down { page } else { -page });
    }

    /// Jump to the first body row.
    pub fn scroll_to_top(&mut self) {
        self.row_offset = 0;
    }

    /// Jump so the last body row sits on the bottom row of the viewport.
    pub fn scroll_to_bottom(&mut self) {
        self.row_offset = self.max_row_offset();
    }

    /// Vertical scroll progress, 0–100%; 0 when every body row is visible.
    pub fn percent(&self) -> u16 {
        match self.max_row_offset() {
            0 => 0,
            max => (self.row_offset * 100 / max).min(100) as u16,
        }
    }

    /// Whether every body row and every column is visible.
    pub fn fits(&self) -> bool {
        self.max_row_offset() == 0 && self.max_col_offset() == 0
    }

    fn body_rows(&self) -> usize {
        self.rows.saturating_sub(1)
    }

    fn max_row_offset(&self) -> usize {
        // No body row is drawn when the viewport is 0 rows: nothing to scroll.
        if self.rendered && self.viewport_rows == 0 {
            return 0;
        }
        self.body_rows().saturating_sub(self.viewport_rows.max(1))
    }

    /// Width of the row-number gutter: the digits of the last row number.
    fn gutter(&self) -> usize {
        self.rows.max(1).to_string().len()
    }

    fn data_width(&self) -> usize {
        self.viewport_width.saturating_sub(self.gutter() + SPACING)
    }

    /// The smallest column offset at which every later column fits.
    fn max_col_offset(&self) -> usize {
        let avail = self.data_width();
        let mut start = self.cols;
        let mut used = 0;
        while start > 0 {
            let need = self.widths.get(start - 1).copied().unwrap_or(1) as usize
                + if used > 0 { SPACING } else { 0 };
            if used + need > avail && start < self.cols {
                break;
            }
            used += need;
            start -= 1;
        }
        start
    }

    fn clamp(&mut self) {
        self.row_offset = self.row_offset.min(self.max_row_offset());
        self.col_offset = self.col_offset.min(self.max_col_offset());
    }

    /// Visible `(column, width)` pairs from the column offset. The first is
    /// always included (narrowed to fit); later ones only when they fit whole.
    fn visible_cols(&self) -> Vec<(usize, usize)> {
        let avail = self.data_width();
        let mut out = Vec::new();
        let mut used = 0;
        for c in self.col_offset..self.cols {
            let w = self.widths.get(c).copied().unwrap_or(1) as usize;
            let need = w + if out.is_empty() { 0 } else { SPACING };
            if out.is_empty() {
                out.push((c, w.min(avail.max(1))));
            } else if used + need <= avail {
                out.push((c, w));
            } else {
                break;
            }
            used += need;
        }
        out
    }
}

fn offset_by(offset: usize, delta: i32, max: usize) -> usize {
    let next = (offset as i64 + i64::from(delta)).clamp(0, max as i64);
    next as usize
}

/// Make a cell's text safe to measure and draw: one visible glyph per
/// control character.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' => '⏎',
            '\r' => '␍',
            '\t' => '⇥',
            c if c.is_control() => '·',
            c => c,
        })
        .collect()
}

/// A scrollable, read-only grid of a CSV/TSV [`Table`], optionally framed by
/// a `Block`. Render it with `render_stateful_widget`.
pub struct GridView<'a> {
    table: &'a Table,
    block: Option<Block<'a>>,
}

impl<'a> GridView<'a> {
    /// A grid over `table`.
    pub fn new(table: &'a Table) -> Self {
        Self { table, block: None }
    }

    /// Frame the grid in a bordered block (title, borders, …).
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl StatefulWidget for GridView<'_> {
    type State = GridViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let inner = match self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.render(area, buf);
                inner
            }
            None => area,
        };
        state.viewport_rows = (inner.height as usize).saturating_sub(1);
        state.viewport_width = inner.width as usize;
        state.rendered = true;
        state.clamp();
        if std::mem::take(&mut state.follow) {
            state.follow_cursor();
        }
        if state.rows == 0 || inner.is_empty() {
            return;
        }
        let gutter = state.gutter();
        let cols = state.visible_cols();
        let make_row = |r: usize, style: Style| {
            let here = state.cursor.filter(|&(cr, _)| cr == r).map(|(_, c)| c);
            let num_style = if here.is_some() {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new().add_modifier(Modifier::DIM)
            };
            let num = Cell::from(format!("{:>gutter$}", r + 1)).style(num_style);
            let cells = cols.iter().map(|&(c, w)| {
                let text = self
                    .table
                    .cell(r, c)
                    .map(|t| sanitize(&t))
                    .unwrap_or_default();
                let cell = Cell::from(truncate(&text, w));
                if here == Some(c) {
                    cell.style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    cell
                }
            });
            Row::new(std::iter::once(num).chain(cells)).style(style)
        };
        let header = make_row(0, Style::new().add_modifier(Modifier::BOLD));
        // Only the visible window of body rows is built.
        let first = 1 + state.row_offset;
        let last = (first + state.viewport_rows).min(state.rows);
        let body = (first..last).map(|r| make_row(r, Style::default()));
        let widths = std::iter::once(Constraint::Length(gutter as u16))
            .chain(cols.iter().map(|&(_, w)| Constraint::Length(w as u16)));
        Widget::render(RatTable::new(body, widths).header(header), inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrybe_core::table::Dialect;

    fn table(s: &str) -> Table {
        Table::parse(s, Dialect::CSV)
    }

    fn draw(t: &Table, st: &mut GridViewState, w: u16, h: u16) -> (Buffer, Vec<String>) {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        GridView::new(t).render(area, &mut buf, st);
        let rows = (0..h)
            .map(|y| {
                // A wide glyph's trailing cell is a filler; skip it.
                let mut line = String::new();
                let mut x = 0;
                while x < w {
                    let sym = buf[(x, y)].symbol();
                    line.push_str(sym);
                    x += (sym.width() as u16).max(1);
                }
                line.trim_end().to_string()
            })
            .collect();
        (buf, rows)
    }

    fn rows(t: &Table, st: &mut GridViewState, w: u16, h: u16) -> Vec<String> {
        draw(t, st, w, h).1
    }

    #[test]
    fn one_row_area_has_no_phantom_scroll() {
        let t = table("a\n1\n2\n");
        let mut st = GridViewState::new(&t);
        let _ = rows(&t, &mut st, 10, 1);
        assert!(st.fits());
        assert_eq!(st.percent(), 0);
    }

    #[test]
    fn shows_header_and_rows_with_gutter() {
        let t = table("name,age\nada,36\nbob,41\n");
        let mut st = GridViewState::new(&t);
        assert_eq!(
            rows(&t, &mut st, 20, 4),
            ["1 name age", "2 ada  36", "3 bob  41", ""]
        );
    }

    #[test]
    fn header_is_bold_and_gutter_dim() {
        let t = table("a,b\nc,d\n");
        let mut st = GridViewState::new(&t);
        let (buf, _) = draw(&t, &mut st, 10, 2);
        assert!(buf[(2, 0)].modifier.contains(Modifier::BOLD));
        assert!(!buf[(2, 1)].modifier.contains(Modifier::BOLD));
        assert!(buf[(0, 1)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn long_cell_clamps_with_ellipsis() {
        let t = table(&format!("{},x\n", "a".repeat(50)));
        let mut st = GridViewState::new(&t);
        let want = format!("1 {}… x", "a".repeat(MAX_COL_WIDTH - 1));
        assert_eq!(rows(&t, &mut st, 60, 2)[0], want);
    }

    #[test]
    fn column_scroll_hides_leading_columns() {
        let t = table("aaaa,bbbb,cccc\n1,2,3\n");
        let mut st = GridViewState::new(&t);
        assert_eq!(rows(&t, &mut st, 12, 2)[0], "1 aaaa bbbb");
        assert!(!st.fits());
        st.scroll_cols_by(1);
        assert_eq!(rows(&t, &mut st, 12, 2)[0], "1 bbbb cccc");
        st.scroll_cols_by(5);
        assert_eq!(rows(&t, &mut st, 12, 2)[0], "1 bbbb cccc");
    }

    #[test]
    fn row_scroll_keeps_header_pinned() {
        let t = table("h\n1\n2\n3\n4\n5\n");
        let mut st = GridViewState::new(&t);
        assert_eq!(rows(&t, &mut st, 8, 3), ["1 h", "2 1", "3 2"]);
        st.scroll_by(2);
        assert_eq!(rows(&t, &mut st, 8, 3), ["1 h", "4 3", "5 4"]);
        st.scroll_to_bottom();
        assert_eq!(rows(&t, &mut st, 8, 3), ["1 h", "5 4", "6 5"]);
        st.scroll_to_top();
        assert_eq!(rows(&t, &mut st, 8, 3)[1], "2 1");
    }

    #[test]
    fn ragged_row_renders_empty_cells() {
        let t = table("a,b,c\n1\n1,2,3\n");
        let mut st = GridViewState::new(&t);
        assert_eq!(rows(&t, &mut st, 12, 3), ["1 a b c", "2 1", "3 1 2 3"]);
    }

    #[test]
    fn wide_characters_keep_columns_aligned() {
        let t = table("名前,x\nab,y\n😀,z\n");
        let mut st = GridViewState::new(&t);
        assert_eq!(
            rows(&t, &mut st, 12, 3),
            ["1 名前 x", "2 ab   y", "3 😀   z"]
        );
    }

    #[test]
    fn control_characters_are_sanitized() {
        let t = table("\"a\nb\",\"c\rd\"\n\"e\tf\",g\u{7}h\n");
        let mut st = GridViewState::new(&t);
        assert_eq!(rows(&t, &mut st, 14, 2), ["1 a⏎b c␍d", "2 e⇥f g·h"]);
    }

    #[test]
    fn zero_rows_draws_only_the_block() {
        let t = table("");
        let mut st = GridViewState::new(&t);
        let area = Rect::new(0, 0, 8, 4);
        let mut buf = Buffer::empty(area);
        GridView::new(&t)
            .block(Block::bordered())
            .render(area, &mut buf, &mut st);
        assert_eq!(buf[(0, 0)].symbol(), "┌");
        assert_eq!(buf[(1, 1)].symbol(), " ");
        assert!(st.fits());
        assert_eq!(st.percent(), 0);
    }

    #[test]
    fn sync_keeps_scroll_and_reclamps_when_table_shrinks() {
        let big = table("h\n1\n2\n3\n4\n5\n6\n7\n");
        let mut st = GridViewState::new(&big);
        rows(&big, &mut st, 8, 3);
        st.scroll_by(4);
        let same = table("h\n1\n2\n3\n4\n5\n6\n7\n8\n");
        st.sync(&same);
        assert_eq!(rows(&same, &mut st, 8, 3)[1], "6 5");
        let small = table("h\n1\n2\n3\n");
        st.sync(&small);
        assert_eq!(rows(&small, &mut st, 8, 3)[1], "3 2");
    }

    #[test]
    fn cursor_cell_is_reversed_and_row_number_bold() {
        let t = table("a,b\nc,d\n");
        let mut st = GridViewState::new(&t);
        st.set_cursor(1, 1);
        let (buf, _) = draw(&t, &mut st, 10, 2);
        assert!(buf[(4, 1)].modifier.contains(Modifier::REVERSED));
        assert!(!buf[(2, 1)].modifier.contains(Modifier::REVERSED));
        assert!(!buf[(4, 0)].modifier.contains(Modifier::REVERSED));
        assert!(buf[(0, 1)].modifier.contains(Modifier::BOLD));
        assert!(!buf[(0, 1)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn cursorless_render_has_no_reversed_cell() {
        let t = table("a,b\nc,d\n");
        let mut st = GridViewState::new(&t);
        let (buf, _) = draw(&t, &mut st, 10, 2);
        assert_eq!(st.cursor(), None);
        assert!((0..10).all(|x| !buf[(x, 1)].modifier.contains(Modifier::REVERSED)));
        assert!(buf[(0, 1)].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn move_cursor_clamps_to_the_table() {
        let t = table("a,b\n1,2\n");
        let mut st = GridViewState::new(&t);
        st.move_cursor(-3, -3);
        assert_eq!(st.cursor(), Some((0, 0)));
        st.move_cursor(9, 9);
        assert_eq!(st.cursor(), Some((1, 1)));
        st.set_cursor(50, 50);
        assert_eq!(st.cursor(), Some((1, 1)));
    }

    #[test]
    fn viewport_follows_cursor_vertically() {
        let t = table("h\n1\n2\n3\n4\n5\n");
        let mut st = GridViewState::new(&t);
        rows(&t, &mut st, 8, 3);
        st.set_cursor(5, 0);
        assert_eq!(rows(&t, &mut st, 8, 3), ["1 h", "5 4", "6 5"]);
        st.set_cursor(1, 0);
        assert_eq!(rows(&t, &mut st, 8, 3)[1], "2 1");
        st.set_cursor(0, 0);
        assert_eq!(rows(&t, &mut st, 8, 3)[1], "2 1");
    }

    #[test]
    fn viewport_follows_cursor_horizontally() {
        let t = table("aaaa,bbbb,cccc\n1,2,3\n");
        let mut st = GridViewState::new(&t);
        rows(&t, &mut st, 12, 2);
        st.set_cursor(0, 2);
        assert_eq!(rows(&t, &mut st, 12, 2)[0], "1 bbbb cccc");
        st.set_cursor(0, 0);
        assert_eq!(rows(&t, &mut st, 12, 2)[0], "1 aaaa bbbb");
    }

    #[test]
    fn scrolling_is_not_undone_by_the_cursor() {
        let t = table("h\n1\n2\n3\n4\n5\n");
        let mut st = GridViewState::new(&t);
        rows(&t, &mut st, 8, 3);
        st.set_cursor(1, 0);
        rows(&t, &mut st, 8, 3);
        st.scroll_by(3);
        assert_eq!(rows(&t, &mut st, 8, 3)[1], "5 4");
    }

    #[test]
    fn sync_reclamps_the_cursor() {
        let big = table("a,b\n1,2\n3,4\n5,6\n");
        let mut st = GridViewState::new(&big);
        st.set_cursor(3, 1);
        st.sync(&table("a\n1\n"));
        assert_eq!(st.cursor(), Some((1, 0)));
    }

    #[test]
    fn percent_and_fits() {
        let t = table("h\n1\n2\n3\n4\n5\n");
        let mut st = GridViewState::new(&t);
        rows(&t, &mut st, 8, 3);
        assert!(!st.fits());
        assert_eq!(st.percent(), 0);
        st.scroll_to_bottom();
        assert_eq!(st.percent(), 100);
        st.scroll_to_top();
        st.half_page(true);
        assert_eq!(st.percent(), 33);
        st.page(true);
        assert_eq!(st.percent(), 66);
        rows(&t, &mut st, 8, 10);
        assert!(st.fits());
    }
}
