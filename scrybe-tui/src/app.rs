// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The viewer binary's driver: one or more document panes (split horizontally
//! or vertically) around the reusable [`MarkdownView`] widget, with live reload
//! — a pane re-renders when its backing file changes on disk. Another project
//! embeds [`crate::view`] directly; this is Scrybe's own thin host.

use anyhow::{Context, Result};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::render;
use crate::render::LinkSpan;
use crate::view::{MarkdownView, MarkdownViewState};
use scrybe_core::ast::Ast;
use scrybe_core::table::{Dialect, Table};
use scrybe_ratatui::{GridView, GridViewState};

/// A CSV/TSV body: the parsed table plus its scroll state.
struct Grid {
    dialect: Dialect,
    table: Table,
    state: GridViewState,
}

/// One document pane in the viewer.
///
/// A pane is Markdown (`text` / `links` / `state`) unless `grid` is set, in
/// which case it is a CSV/TSV table and the Markdown fields stay empty.
struct Pane {
    title: String,
    /// Backing file, if this pane was loaded from disk (enables live reload).
    path: Option<PathBuf>,
    mtime: Option<SystemTime>,
    text: Text<'static>,
    /// Hyperlink locations in `text`, for click-to-open (#244).
    links: Vec<LinkSpan>,
    state: MarkdownViewState,
    /// Set for a `.csv` / `.tsv` pane.
    grid: Option<Grid>,
}

impl Pane {
    fn from_source(source: &str, title: impl Into<String>) -> Self {
        let (text, links) = render::render_with_links(&Ast::parse(source));
        let state = MarkdownViewState::new(text.lines.len());
        Self {
            title: title.into(),
            path: None,
            mtime: None,
            text,
            links,
            state,
            grid: None,
        }
    }

    fn from_table(source: &str, dialect: Dialect, title: impl Into<String>) -> Self {
        let table = Table::parse(source, dialect);
        let state = GridViewState::new(&table);
        let mut pane = Self::from_source("", title);
        pane.grid = Some(Grid {
            dialect,
            table,
            state,
        });
        pane
    }

    fn from_file(path: PathBuf) -> Result<Self> {
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let title = path.display().to_string();
        let mut pane = match Dialect::from_path(&path) {
            Some(dialect) => Self::from_table(&source, dialect, title),
            None => Self::from_source(&source, title),
        };
        pane.mtime = file_mtime(&path);
        pane.path = Some(path);
        Ok(pane)
    }

    /// Re-render from `source`, preserving the scroll position where possible.
    fn set_source(&mut self, source: &str) {
        if let Some(grid) = &mut self.grid {
            grid.table = Table::parse(source, grid.dialect);
            grid.state.sync(&grid.table);
            return;
        }
        let (text, links) = render::render_with_links(&Ast::parse(source));
        self.text = text;
        self.links = links;
        self.state.set_line_count(self.text.lines.len());
    }

    /// If the backing file changed on disk, re-read + re-render. Returns whether
    /// the view was updated.
    fn reload_if_changed(&mut self) -> bool {
        let Some(path) = self.path.clone() else {
            return false;
        };
        let current = file_mtime(&path);
        if current == self.mtime {
            return false;
        }
        self.mtime = current;
        match fs::read_to_string(&path) {
            Ok(source) => {
                self.set_source(&source);
                true
            }
            Err(_) => false,
        }
    }
}

/// Forward a scroll call to whichever state the pane's body uses.
macro_rules! on_state {
    ($pane:expr, $s:ident => $call:expr) => {
        match &mut $pane.grid {
            Some(g) => {
                let $s = &mut g.state;
                $call
            }
            None => {
                let $s = &mut $pane.state;
                $call
            }
        }
    };
}

impl Pane {
    fn scroll_by(&mut self, delta: i32) {
        on_state!(self, s => s.scroll_by(delta));
    }

    /// Column scrolling only exists on a grid; no-op on Markdown.
    fn scroll_cols_by(&mut self, delta: i32) {
        if let Some(g) = &mut self.grid {
            g.state.scroll_cols_by(delta);
        }
    }

    fn half_page(&mut self, down: bool) {
        on_state!(self, s => s.half_page(down));
    }

    fn page(&mut self, down: bool) {
        on_state!(self, s => s.page(down));
    }

    fn scroll_to_top(&mut self) {
        on_state!(self, s => s.scroll_to_top());
    }

    fn scroll_to_bottom(&mut self) {
        on_state!(self, s => s.scroll_to_bottom());
    }

    fn fits(&self) -> bool {
        match &self.grid {
            Some(g) => g.state.fits(),
            None => self.state.fits(),
        }
    }

    fn percent(&self) -> u16 {
        match &self.grid {
            Some(g) => g.state.percent(),
            None => self.state.percent(),
        }
    }
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Whether terminal cell `(col, row)` falls within `area`.
fn rect_contains(area: Rect, col: u16, row: u16) -> bool {
    col >= area.x && col < area.x + area.width && row >= area.y && row < area.y + area.height
}

/// The viewer: one or more document panes, split horizontally or vertically,
/// with a focused pane that receives scroll keys. Panes loaded from files
/// reload live when the file changes on disk.
pub struct App {
    panes: Vec<Pane>,
    focus: usize,
    orientation: Direction,
    quit: bool,
    /// Each pane's content area (post-border) from the last draw, for mapping
    /// a mouse click's absolute terminal coordinates to a pane (#244).
    pane_areas: Vec<Rect>,
    /// The buffer actually drawn last frame — click resolution reads back
    /// what's really on screen (link text is styled Blue+Underlined; see
    /// `scrybe_ratatui::render`) rather than re-deriving word-wrap offsets.
    last_buffer: Option<Buffer>,
}

impl App {
    /// A single-pane viewer over in-memory source (no live reload).
    pub fn from_source(source: &str, title: impl Into<String>) -> Self {
        Self::with_panes(vec![Pane::from_source(source, title)])
    }

    /// A viewer with one pane per `(source, title)` — a split screen over
    /// in-memory sources (no live reload).
    pub fn from_documents<T: Into<String>>(docs: impl IntoIterator<Item = (String, T)>) -> Self {
        let panes: Vec<Pane> = docs
            .into_iter()
            .map(|(s, t)| Pane::from_source(&s, t))
            .collect();
        Self::with_panes(panes)
    }

    /// A viewer with one pane per file — a split screen with **live reload**.
    pub fn from_files(paths: Vec<PathBuf>) -> Result<Self> {
        let mut panes = Vec::with_capacity(paths.len());
        for path in paths {
            panes.push(Pane::from_file(path)?);
        }
        Ok(Self::with_panes(panes))
    }

    fn with_panes(mut panes: Vec<Pane>) -> Self {
        if panes.is_empty() {
            panes.push(Pane::from_source("", "(empty)"));
        }
        let pane_areas = vec![Rect::default(); panes.len()];
        Self {
            panes,
            focus: 0,
            orientation: Direction::Horizontal,
            quit: false,
            pane_areas,
            last_buffer: None,
        }
    }

    /// Set the initial split orientation (default horizontal / side-by-side).
    pub fn orientation(mut self, dir: Direction) -> Self {
        self.orientation = dir;
        self
    }

    /// Run the event loop until the user quits.
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while !self.quit {
            let completed = terminal.draw(|f| self.draw(f))?;
            // Snapshot what was actually drawn — click resolution (#244)
            // reads this back rather than re-deriving word-wrap offsets.
            self.last_buffer = Some(completed.buffer.clone());
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
                    Event::Mouse(mouse) => self.on_mouse(mouse),
                    _ => {}
                }
            }
            // Pick up external edits between ticks (~250ms latency).
            self.reload_changed();
        }
        Ok(())
    }

    fn reload_changed(&mut self) {
        for pane in &mut self.panes {
            pane.reload_if_changed();
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let n = self.panes.len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Tab => self.focus = (self.focus + 1) % n,
            KeyCode::BackTab => self.focus = (self.focus + n - 1) % n,
            KeyCode::Char('o') => {
                self.orientation = match self.orientation {
                    Direction::Horizontal => Direction::Vertical,
                    Direction::Vertical => Direction::Horizontal,
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.panes[self.focus].scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.panes[self.focus].scroll_by(-1),
            KeyCode::Char('h') | KeyCode::Left => self.panes[self.focus].scroll_cols_by(-1),
            KeyCode::Char('l') | KeyCode::Right => self.panes[self.focus].scroll_cols_by(1),
            KeyCode::Char('d') if ctrl => self.panes[self.focus].half_page(true),
            KeyCode::Char('u') if ctrl => self.panes[self.focus].half_page(false),
            KeyCode::PageDown | KeyCode::Char(' ') => self.panes[self.focus].page(true),
            KeyCode::PageUp => self.panes[self.focus].page(false),
            KeyCode::Char('g') | KeyCode::Home => self.panes[self.focus].scroll_to_top(),
            KeyCode::Char('G') | KeyCode::End => self.panes[self.focus].scroll_to_bottom(),
            _ => {}
        }
    }

    /// A left click that landed on link text (#244) opens it in the default
    /// browser via the `open` crate, which picks the right mechanism per OS
    /// (`open` on macOS, `xdg-open` on Linux, `cmd /c start` on Windows) —
    /// nothing here needs to know or care which platform it's running on.
    fn on_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        if let Some(href) = self.resolve_link_at(mouse.column, mouse.row) {
            // Best-effort: a browser failing to launch shouldn't crash the
            // viewer. Nothing to surface it to in a pure TUI without adding
            // a status-line error channel — a reasonable follow-up if this
            // turns out to fail often in practice.
            let _ = open::that(href);
        }
    }

    /// Resolve a click at absolute terminal coordinates to a link's href.
    ///
    /// Reads the actually-rendered [`Buffer`] from the last frame rather
    /// than re-deriving word-wrap row offsets: link text is styled
    /// `Color::Blue` + `Modifier::UNDERLINED` (see `scrybe_ratatui::render`)
    /// and nothing else in this renderer uses that combination, so the
    /// buffer itself is the ground truth for "what link text is at this
    /// screen cell" — walk outward from the click to the full contiguous
    /// run of link-styled cells on that row, then match the run's text
    /// against the clicked pane's recorded links. Two links with identical
    /// visible text on the same rendered row resolve to whichever appears
    /// first in that pane's link list (document order) — an accepted,
    /// rare-in-practice limitation of matching by text rather than by a
    /// tracked byte range.
    fn resolve_link_at(&self, col: u16, row: u16) -> Option<String> {
        let buf = self.last_buffer.as_ref()?;
        let pane_idx = self
            .pane_areas
            .iter()
            .position(|a| rect_contains(*a, col, row))?;
        if self.panes[pane_idx].grid.is_some() {
            return None;
        }
        let area = self.pane_areas[pane_idx];

        let is_link_style = |x: u16, y: u16| -> bool {
            let cell = &buf[(x, y)];
            cell.fg == Color::Blue && cell.modifier.contains(Modifier::UNDERLINED)
        };
        if !is_link_style(col, row) {
            return None;
        }
        let mut left = col;
        while left > area.x && is_link_style(left - 1, row) {
            left -= 1;
        }
        let mut right = col;
        while right + 1 < area.x + area.width && is_link_style(right + 1, row) {
            right += 1;
        }
        let mut text = String::new();
        for x in left..=right {
            text.push_str(buf[(x, row)].symbol());
        }
        let text = text.trim();

        self.panes[pane_idx]
            .links
            .iter()
            .find(|l| l.text == text)
            .map(|l| l.href.clone())
    }

    fn draw(&mut self, f: &mut Frame) {
        let outer = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());

        let n = self.panes.len();
        let focus = self.focus;
        let orientation = self.orientation;
        let constraints = vec![Constraint::Ratio(1, n as u32); n];
        let areas = Layout::new(orientation, constraints).split(outer[0]);

        for (i, pane) in self.panes.iter_mut().enumerate() {
            let focused = i == focus && n > 1;
            let border = Style::default().fg(if focused {
                Color::Cyan
            } else {
                Color::DarkGray
            });
            let title = Style::default()
                .fg(if focused { Color::Cyan } else { Color::Gray })
                .add_modifier(Modifier::BOLD);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(format!(" {} ", pane.title), title));
            // Content area post-border, for mapping a click to this pane
            // (#244) — computed before `block` moves into `.block(block)`.
            self.pane_areas[i] = block.inner(areas[i]);
            match &mut pane.grid {
                Some(g) => {
                    let view = GridView::new(&g.table).block(block);
                    f.render_stateful_widget(view, areas[i], &mut g.state);
                }
                None => {
                    let view = MarkdownView::new(&pane.text).block(block);
                    f.render_stateful_widget(view, areas[i], &mut pane.state);
                }
            }
        }

        let fp = &self.panes[focus];
        let progress = if fp.fits() {
            " All ".to_string()
        } else {
            format!(" {}% ", fp.percent())
        };
        let cols = if fp.grid.is_some() { "h/l  " } else { "" };
        let hint = if n > 1 {
            format!(" j/k  {cols}^d/^u  g/G  Tab:pane  o:split  q:quit ")
        } else {
            format!(" j/k ↑↓  {cols}^d/^u  space  g/G  q:quit ")
        };
        let footer = Line::from(vec![
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
            Span::styled(progress, Style::default().fg(Color::Yellow)),
        ]);
        f.render_widget(Paragraph::new(footer), outer[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    // -----------------------------------------------------------------------
    // Click-to-open (#244) — resolve_link_at against a real drawn Buffer.
    // `on_mouse` itself isn't tested here: it shells out via `open::that`,
    // which must never run in a test (no real subprocess / external
    // service). `resolve_link_at` is the pure part; that's what's covered.
    // -----------------------------------------------------------------------

    fn drawn(app: &mut App, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let completed = terminal.draw(|f| app.draw(f)).unwrap();
        app.last_buffer = Some(completed.buffer.clone());
    }

    #[test]
    fn click_on_rendered_link_resolves_its_href() {
        let mut app = App::from_source("[docs](https://example.com)\n", "t.md");
        drawn(&mut app, 40, 10);

        let area = app.pane_areas[0];
        let row = area.y;
        let buf = app.last_buffer.as_ref().unwrap();
        let link_col = (area.x..area.x + area.width)
            .find(|&x| {
                let cell = &buf[(x, row)];
                cell.fg == Color::Blue && cell.modifier.contains(Modifier::UNDERLINED)
            })
            .expect("link cell rendered somewhere on the content row");

        assert_eq!(
            app.resolve_link_at(link_col, row),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn click_on_plain_text_resolves_to_none() {
        let mut app = App::from_source("plain text, no links here\n", "t.md");
        drawn(&mut app, 40, 10);
        let area = app.pane_areas[0];
        assert_eq!(app.resolve_link_at(area.x, area.y), None);
    }

    #[test]
    fn click_outside_any_pane_resolves_to_none() {
        let mut app = App::from_source("[docs](https://example.com)\n", "t.md");
        drawn(&mut app, 40, 10);
        // Row 9 of a 10-row backend is the footer hint line — outside every
        // pane's content area.
        assert_eq!(app.resolve_link_at(0, 9), None);
    }

    #[test]
    fn two_links_with_identical_text_both_resolve_to_the_first_in_document_order() {
        // Documented, accepted limitation: resolution matches by visible
        // text, not by tracked position, so two links with byte-for-byte
        // identical text can't be told apart by which one was physically
        // clicked — both resolve to whichever comes first in the pane's
        // link list. Real-world duplicate-visible-text links on one screen
        // row are rare; this test exists so that behavior stays intentional
        // rather than silently changing.
        let mut app = App::from_source(
            "[dup](https://first.example) and [dup](https://second.example)\n",
            "t.md",
        );
        drawn(&mut app, 80, 10);
        let area = app.pane_areas[0];
        let row = area.y;
        let buf = app.last_buffer.as_ref().unwrap();
        let link_cols: Vec<u16> = (area.x..area.x + area.width)
            .filter(|&x| {
                let cell = &buf[(x, row)];
                cell.fg == Color::Blue && cell.modifier.contains(Modifier::UNDERLINED)
            })
            .collect();
        // Two separate "dup" runs, so there's a style gap between them.
        let first_run_start = link_cols[0];
        let second_run_start = *link_cols
            .iter()
            .find(|&&x| x > first_run_start + 3)
            .expect("second link run present");

        assert_eq!(
            app.resolve_link_at(first_run_start, row),
            Some("https://first.example".to_string())
        );
        assert_eq!(
            app.resolve_link_at(second_run_start, row),
            Some("https://first.example".to_string()),
            "known limitation: text-matching can't disambiguate identical link text"
        );
    }

    #[test]
    fn j_scrolls_down_q_quits() {
        let mut app = App::from_source("# A\n\nbody\nmore\nlines\nhere\n", "t.md");
        app.panes[0].state.set_line_count(20);
        let before = app.panes[0].state.scroll();
        app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(app.panes[0].state.scroll() >= before);
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.quit);
    }

    #[test]
    fn tab_cycles_focus_and_o_toggles_orientation() {
        let mut app =
            App::from_documents([("# A".to_string(), "a.md"), ("# B".to_string(), "b.md")]);
        assert_eq!(app.focus, 0);
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, 1);
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, 0); // wraps

        let before = app.orientation;
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_ne!(app.orientation, before);
    }

    #[test]
    fn scroll_keys_target_the_focused_pane() {
        let mut app =
            App::from_documents([("line\n".repeat(50), "a.md"), ("line\n".repeat(50), "b.md")]);
        app.panes[0].state.set_line_count(50);
        app.panes[1].state.set_line_count(50);
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // focus pane 1
        app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(
            app.panes[0].state.scroll(),
            0,
            "unfocused pane must not move"
        );
        assert!(app.panes[1].state.scroll() > 0, "focused pane scrolled");
    }

    #[test]
    fn set_source_re_renders_and_reclamps() {
        let mut pane = Pane::from_source("# One\n", "t.md");
        let before = pane.text.lines.len();
        pane.set_source("# One\n\n# Two\n\nmore body here\n");
        assert!(pane.text.lines.len() > before);
    }

    #[test]
    fn reload_picks_up_file_changes() {
        let path =
            std::env::temp_dir().join(format!("scrybe-tui-reload-{}.md", std::process::id()));
        fs::write(&path, "# One\n").unwrap();
        let mut pane = Pane::from_file(path.clone()).unwrap();
        let before = pane.text.lines.len();

        fs::write(&path, "# One\n\n# Two\n\nmore\n").unwrap();
        // Force detection regardless of filesystem mtime resolution.
        pane.mtime = None;
        assert!(pane.reload_if_changed(), "change should be detected");
        assert!(pane.text.lines.len() > before, "content should have grown");

        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // CSV / TSV grid panes (#273)
    // -----------------------------------------------------------------------

    fn key(app: &mut App, c: KeyCode) {
        app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        drawn(app, width, height);
        let buf = app.last_buffer.as_ref().unwrap();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn temp(name: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("scrybe-tui-{}-{name}", std::process::id()));
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn extension_picks_grid_or_markdown_case_insensitively() {
        for (name, grid) in [("g1.csv", true), ("g2.TSV", true), ("g3.md", false)] {
            let path = temp(name, "a,b\n1,2\n");
            let pane = Pane::from_file(path.clone()).unwrap();
            assert_eq!(pane.grid.is_some(), grid, "{name}");
            let _ = fs::remove_file(&path);
        }
    }

    #[test]
    fn csv_pane_draws_as_a_grid() {
        let path = temp("draw.csv", "name,qty\napple,3\n");
        let mut app = App::from_files(vec![path.clone()]).unwrap();
        let out = screen(&mut app, 40, 8);
        assert!(out.contains("name") && out.contains("apple"), "{out}");
        assert!(out.contains("h/l"), "{out}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn h_l_move_columns_on_grid_and_noop_on_markdown() {
        let path = temp("cols.csv", "alpha,bravo,charlie\n1,2,3\n");
        let mut app = App::from_files(vec![path.clone()]).unwrap();
        // Narrow enough that columns do not all fit.
        screen(&mut app, 16, 6);
        key(&mut app, KeyCode::Char('l'));
        let out = screen(&mut app, 16, 6);
        assert!(!out.contains("alpha"), "leading column scrolled off: {out}");
        key(&mut app, KeyCode::Char('h'));
        let out = screen(&mut app, 16, 6);
        assert!(out.contains("alpha"), "{out}");
        let _ = fs::remove_file(&path);

        let mut md = App::from_source("# T\n\nbody\n", "t.md");
        let before = md.panes[0].state.scroll();
        key(&mut md, KeyCode::Char('l'));
        key(&mut md, KeyCode::Char('h'));
        assert_eq!(md.panes[0].state.scroll(), before);
        assert!(md.panes[0].grid.is_none());
    }

    #[test]
    fn csv_reload_reparses_and_keeps_scroll() {
        let rows: String = (0..60).map(|i| format!("r{i},x\n")).collect();
        let path = temp("reload.csv", &rows);
        let mut app = App::from_files(vec![path.clone()]).unwrap();
        screen(&mut app, 30, 10);
        key(&mut app, KeyCode::Char('j'));
        key(&mut app, KeyCode::Char('j'));
        let scrolled = app.panes[0].percent();
        assert!(scrolled > 0);

        fs::write(&path, format!("{rows}extra,row\n")).unwrap();
        app.panes[0].mtime = None;
        assert!(app.panes[0].reload_if_changed());
        let out = screen(&mut app, 30, 10);
        assert!(app.panes[0].percent() > 0, "scroll kept: {out}");
        key(&mut app, KeyCode::Char('G'));
        let out = screen(&mut app, 30, 10);
        assert!(out.contains("extra"), "re-parsed: {out}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn split_of_markdown_and_csv_draws_both() {
        let md = temp("split.md", "# Heading\n\nbody\n");
        let csv = temp("split.csv", "name,qty\napple,3\n");
        let mut app = App::from_files(vec![md.clone(), csv.clone()]).unwrap();
        let out = screen(&mut app, 80, 8);
        assert!(out.contains("Heading") && out.contains("apple"), "{out}");
        let _ = fs::remove_file(&md);
        let _ = fs::remove_file(&csv);
    }
}
