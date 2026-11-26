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
use scrybe_core::{AppliedChange, DocumentChange, DocumentHistory};
use scrybe_ratatui::{GridView, GridViewState};

/// Shown inline at the caret while a cell is being edited.
const CARET: char = '\u{258F}';

/// An open cell edit: the buffer and the caret as a char index into it.
struct Edit {
    buf: String,
    caret: usize,
}

/// A CSV/TSV body: the parsed table, its view state and its edit state.
///
/// The table text is the source of truth. Each edit is one splice planned by
/// the `Table` and applied through [`Table::apply`]; undo and redo replay the
/// inverse through `DocumentHistory`.
struct Grid {
    dialect: Dialect,
    table: Table,
    state: GridViewState,
    history: DocumentHistory,
    /// The text last loaded or saved: the pane is dirty when `table` differs.
    baseline: String,
    /// The text last seen on disk, for conflict detection.
    disk: String,
    /// The open cell edit, if any.
    edit: Option<Edit>,
    /// A `d` was pressed and a second one would delete the row.
    pending_d: bool,
    /// The disk text of an unanswered "file changed on disk" prompt.
    conflict: Option<String>,
    /// The last read of the file failed (reported once, not every tick).
    read_failed: bool,
}

impl Grid {
    fn new(table: Table, dialect: Dialect) -> Self {
        let mut state = GridViewState::new(&table);
        state.set_cursor(0, 0);
        let text = table.source().to_owned();
        Self {
            dialect,
            table,
            state,
            history: DocumentHistory::new(),
            baseline: text.clone(),
            disk: text,
            edit: None,
            pending_d: false,
            conflict: None,
            read_failed: false,
        }
    }

    fn cursor(&self) -> (usize, usize) {
        self.state.cursor().unwrap_or((0, 0))
    }

    /// Whether the text differs from what was last loaded or saved.
    fn dirty(&self) -> bool {
        // ponytail: O(file) compare per call; track a version counter if big files lag.
        self.table.source() != self.baseline
    }

    /// Unsaved edits, an open edit or an unanswered prompt: never replaced silently.
    fn guarded(&self) -> bool {
        self.dirty() || self.edit.is_some() || self.conflict.is_some()
    }

    /// Replace everything with `source` as read from disk: clean, no history.
    fn reset(&mut self, source: &str) {
        self.table = Table::parse(source, self.dialect);
        self.state.sync(&self.table);
        self.history = DocumentHistory::new();
        self.baseline = source.to_owned();
        self.disk = source.to_owned();
        self.edit = None;
        self.pending_d = false;
        self.conflict = None;
    }

    /// Apply one planned change and record it for undo.
    fn apply(&mut self, change: &DocumentChange) -> Result<(), String> {
        let inverse = self.table.apply(change).map_err(|e| e.to_string())?;
        self.history.push_applied(&AppliedChange {
            new_text: self.table.source().to_owned(),
            inverse,
        });
        self.state.sync(&self.table);
        Ok(())
    }

    fn undo_redo(&mut self, undo: bool) -> Option<String> {
        let step = if undo {
            self.history.undo()
        } else {
            self.history.redo()
        };
        let Some(change) = step.cloned() else {
            return Some(format!("nothing to {}", if undo { "undo" } else { "redo" }));
        };
        if let Err(e) = self.table.apply(&change) {
            // Keep the history in step with the unchanged table.
            if undo {
                self.history.redo();
            } else {
                self.history.undo();
            }
            return Some(e.to_string());
        }
        self.state.sync(&self.table);
        None
    }

    fn begin_edit(&mut self) -> Option<String> {
        if self.table.row_count() == 0 || self.table.col_count() == 0 {
            return Some("no cells".into());
        }
        let (r, c) = self.cursor();
        let buf = self.table.cell(r, c).map(|v| v.into_owned());
        let buf = buf.unwrap_or_default();
        let caret = buf.chars().count();
        self.edit = Some(Edit { buf, caret });
        None
    }

    fn close_edit(&mut self) {
        self.edit = None;
        self.state.sync(&self.table);
    }

    /// Commit the open edit. On an error the edit stays open.
    fn commit(&mut self) -> Option<String> {
        let buf = self.edit.as_ref()?.buf.clone();
        let (r, c) = self.cursor();
        match self.table.set_cell(r, c, &buf) {
            Ok(None) => self.close_edit(),
            Ok(Some(change)) => {
                if let Err(e) = self.apply(&change) {
                    return Some(e);
                }
                self.close_edit();
            }
            Err(e) => return Some(e.to_string()),
        }
        None
    }

    fn edit_key(&mut self, key: KeyEvent) -> Option<String> {
        self.pending_d = false;
        let mods = key.modifiers;
        let Some(edit) = &mut self.edit else {
            return None;
        };
        let len = edit.buf.chars().count();
        let at = |buf: &str, n: usize| buf.char_indices().nth(n).map_or(buf.len(), |(i, _)| i);
        match key.code {
            KeyCode::Enter if mods.contains(KeyModifiers::ALT) => {
                edit.buf.insert(at(&edit.buf, edit.caret), '\n');
                edit.caret += 1;
            }
            KeyCode::Enter | KeyCode::Esc => return self.commit(),
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => self.close_edit(),
            KeyCode::Char(c) if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                edit.buf.insert(at(&edit.buf, edit.caret), c);
                edit.caret += 1;
            }
            KeyCode::Backspace if edit.caret > 0 => {
                edit.caret -= 1;
                edit.buf.remove(at(&edit.buf, edit.caret));
            }
            KeyCode::Delete if edit.caret < len => {
                edit.buf.remove(at(&edit.buf, edit.caret));
            }
            KeyCode::Left => edit.caret = edit.caret.saturating_sub(1),
            KeyCode::Right => edit.caret = (edit.caret + 1).min(len),
            KeyCode::Home => edit.caret = 0,
            KeyCode::End => edit.caret = len,
            _ => {}
        }
        None
    }

    /// Insert an empty row below (or above) the cursor row and move onto it.
    fn insert_row(&mut self, below: bool) -> Option<String> {
        let (r, c) = self.cursor();
        let rows = self.table.row_count();
        let at = if below {
            (r + 1).min(rows)
        } else {
            r.min(rows)
        };
        match self.table.insert_row(at, &[]) {
            Ok(change) => match self.apply(&change) {
                Ok(()) => {
                    self.state.set_cursor(at, c);
                    None
                }
                Err(e) => Some(e),
            },
            Err(e) => Some(e.to_string()),
        }
    }

    fn delete_row(&mut self) -> Option<String> {
        if self.table.row_count() == 0 {
            return Some("no rows".into());
        }
        let (r, _) = self.cursor();
        match self.table.delete_row(r) {
            Ok(change) => self.apply(&change).err(),
            Err(e) => Some(e.to_string()),
        }
    }

    /// The table as drawn: the open edit shown in its cell with a caret.
    // ponytail: re-parses the whole file per frame while a cell is open.
    fn shown(&self) -> Option<Table> {
        let edit = self.edit.as_ref()?;
        let (r, c) = self.cursor();
        let at = edit
            .buf
            .char_indices()
            .nth(edit.caret)
            .map_or(edit.buf.len(), |(i, _)| i);
        let mut text = edit.buf.clone();
        text.insert(at, CARET);
        // The widget head-truncates at 30 columns: show a window holding the caret.
        // ponytail: counts chars, not columns; wide glyphs may still clip.
        let chars: Vec<char> = text.chars().collect();
        if chars.len() > 30 {
            let start = edit.caret.saturating_sub(20).min(chars.len() - 30);
            text = chars[start..start + 30].iter().collect();
        }
        let change = self.table.set_cell(r, c, &text).ok().flatten()?;
        let applied = change.try_apply(self.table.source()).ok()?;
        Some(Table::parse(applied.new_text, self.dialect))
    }
}

/// Write `bytes` to `path` through a temp file in the same directory and a
/// rename, keeping the original's permissions where possible.
fn write_atomic(path: &Path, bytes: &str) -> std::io::Result<()> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        name.unwrap_or_default(),
        std::process::id()
    ));
    let result = (|| {
        fs::write(&tmp, bytes)?;
        if let Ok(meta) = fs::metadata(&path) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        fs::rename(&tmp, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
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
    /// A message for the footer from a reload, taken by the app.
    notice: Option<String>,
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
            notice: None,
        }
    }

    fn from_table(source: &str, dialect: Dialect, title: impl Into<String>) -> Self {
        let mut pane = Self::from_source("", title);
        pane.grid = Some(Grid::new(Table::parse(source, dialect), dialect));
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
            grid.reset(source);
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
        if self.grid.as_ref().is_some_and(Grid::guarded) {
            return self.reload_guarded(&path);
        }
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

    /// Reload for a pane holding unsaved work: never replaces it. The file is
    /// read on every tick and compared by content, never by mtime alone.
    // ponytail: reads the file each tick while guarded; fine for hand-edited tables.
    fn reload_guarded(&mut self, path: &Path) -> bool {
        let Some(g) = &mut self.grid else {
            return false;
        };
        if g.conflict.is_some() {
            return false; // unanswered: remembered until the user chooses
        }
        match fs::read_to_string(path) {
            Ok(text) => {
                g.read_failed = false;
                self.mtime = file_mtime(path);
                if text == g.disk {
                    return false;
                }
                if text == g.table.source() {
                    // Our own save, or an external write of the same text.
                    g.disk = text.clone();
                    g.baseline = text;
                } else {
                    g.conflict = Some(text);
                }
                true
            }
            Err(e) => {
                if !g.read_failed {
                    g.read_failed = true;
                    self.notice = Some(format!(
                        "cannot read {}: {e}; keeping your edits",
                        self.title
                    ));
                }
                false
            }
        }
    }

    /// Whether the pane has unsaved edits.
    fn dirty(&self) -> bool {
        self.grid.as_ref().is_some_and(Grid::dirty)
    }

    /// Write the table to its file. Returns the byte count, or why not.
    fn save(&mut self) -> Result<usize, String> {
        let (Some(path), Some(g)) = (self.path.clone(), self.grid.as_mut()) else {
            return Err("nothing to save".into());
        };
        if g.conflict.is_some() {
            return Err("file changed on disk: answer the prompt first".into());
        }
        if !g.dirty() {
            return Err("no changes to save".into());
        }
        // Content check right before overwriting: a change not yet noticed
        // by the tick must not be clobbered silently.
        match fs::read_to_string(&path) {
            Ok(text) if text != g.disk && text != g.table.source() => {
                g.conflict = Some(text);
                return Err("file changed on disk: answer the prompt first".into());
            }
            Ok(_) => {}
            // A deleted file is recreated; any other failure must not clobber it.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot read {} before saving: {e}", self.title)),
        }
        let text = g.table.source().to_owned();
        write_atomic(&path, &text).map_err(|e| format!("save failed: {e}"))?;
        g.baseline = text.clone();
        g.disk = text;
        g.read_failed = false;
        self.mtime = file_mtime(&path);
        Ok(g.table.source().len())
    }

    /// Answer the conflict prompt: `reload` takes the disk text, else keep ours.
    fn resolve_conflict(&mut self, reload: bool) -> Option<String> {
        let path = self.path.clone()?;
        let g = self.grid.as_mut()?;
        if !reload {
            // The baseline follows the disk, so undoing back to the old text
            // still differs from disk: dirty, never stale-clean.
            let disk = g.conflict.take()?;
            g.baseline = disk.clone();
            g.disk = disk;
            return Some("keeping your version".into());
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                g.reset(&text);
                self.mtime = file_mtime(&path);
                Some("reloaded from disk".into())
            }
            Err(e) => Some(format!("cannot read {}: {e}", self.title)),
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

/// The pane's border title, flagged when it holds unsaved or conflicting work.
/// The flag comes first: a path wider than the pane is truncated at its end, and
/// a trailing flag would be cut off with it, hiding that edits are unsaved.
fn pane_title(pane: &Pane) -> String {
    let conflict = pane.grid.as_ref().is_some_and(|g| g.conflict.is_some());
    let flag = match (conflict, pane.dirty()) {
        (true, _) => "[conflict] ",
        (false, true) => "[+] ",
        _ => "",
    };
    format!(" {flag}{} ", pane.title)
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
    /// The quit-guard prompt, open while a dirty pane blocks quitting.
    quit_prompt: bool,
    /// A one-line footer message, cleared by the next key.
    status: Option<String>,
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
            quit_prompt: false,
            status: None,
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
            if let Some(notice) = pane.notice.take() {
                self.status = Some(notice);
            }
        }
    }

    /// Quit, unless a pane has unsaved edits: then ask first.
    fn request_quit(&mut self) {
        if self.panes.iter().any(Pane::dirty) {
            self.quit_prompt = true;
        } else {
            self.quit = true;
        }
    }

    fn quit_prompt_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return;
        }
        match key.code {
            KeyCode::Char('d') => self.quit = true,
            KeyCode::Char('c') | KeyCode::Esc => self.quit_prompt = false,
            KeyCode::Char('s') => {
                self.quit_prompt = false;
                for pane in self.panes.iter_mut().filter(|p| p.dirty()) {
                    if let Err(e) = pane.save() {
                        self.status = Some(e);
                        return; // a failed save cancels the quit
                    }
                }
                self.quit = true;
            }
            _ => {}
        }
    }

    /// Rows a page moves the cursor in the focused grid (last drawn height).
    fn page_rows(&self) -> i32 {
        let h = self.pane_areas.get(self.focus).map_or(0, |a| a.height);
        i32::from(h.saturating_sub(1)).max(1)
    }

    /// Normal-mode keys of a focused grid pane. False: not a grid key.
    fn grid_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.page_rows();
        let half = (page / 2).max(1);
        let Some(g) = self.panes[self.focus].grid.as_mut() else {
            return false;
        };
        let pending_d = std::mem::take(&mut g.pending_d);
        let (_, c) = g.cursor();
        let mut status = None;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => g.state.move_cursor(1, 0),
            KeyCode::Char('k') | KeyCode::Up => g.state.move_cursor(-1, 0),
            KeyCode::Char('h') | KeyCode::Left => g.state.move_cursor(0, -1),
            KeyCode::Char('l') | KeyCode::Right => g.state.move_cursor(0, 1),
            KeyCode::Char('g') | KeyCode::Home => g.state.set_cursor(0, c),
            KeyCode::Char('G') | KeyCode::End => g.state.set_cursor(usize::MAX, c),
            KeyCode::Char('d') if ctrl => g.state.move_cursor(half, 0),
            KeyCode::Char('u') if ctrl => g.state.move_cursor(-half, 0),
            KeyCode::PageDown | KeyCode::Char(' ') => g.state.move_cursor(page, 0),
            KeyCode::PageUp => g.state.move_cursor(-page, 0),
            KeyCode::Enter | KeyCode::Char('i') => status = g.begin_edit(),
            KeyCode::Char('o') if !ctrl => status = g.insert_row(true),
            KeyCode::Char('O') => status = g.insert_row(false),
            KeyCode::Char('d') if !ctrl => {
                if pending_d {
                    status = g.delete_row();
                } else {
                    g.pending_d = true;
                }
            }
            KeyCode::Char('u') if !ctrl => status = g.undo_redo(true),
            KeyCode::Char('r') if ctrl => status = g.undo_redo(false),
            KeyCode::Char('s') if ctrl => {
                status = Some(match self.panes[self.focus].save() {
                    Ok(n) => format!("saved {n} bytes"),
                    Err(e) => e,
                });
            }
            _ => return false,
        }
        if status.is_some() {
            self.status = status;
        }
        true
    }

    fn on_key(&mut self, key: KeyEvent) {
        self.status = None;
        if self.quit_prompt {
            return self.quit_prompt_key(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let n = self.panes.len();
        let nav = matches!(key.code, KeyCode::Tab | KeyCode::BackTab);
        // Modal states of the focused grid: an open edit, then a conflict prompt.
        if let Some(g) = self.panes[self.focus].grid.as_mut() {
            if g.edit.is_some() {
                if let Some(msg) = g.edit_key(key) {
                    self.status = Some(msg);
                }
                return;
            }
            if g.conflict.is_some() && !nav {
                g.pending_d = false;
                let reload = match key.code {
                    KeyCode::Char('r') => true,
                    KeyCode::Char('k') => false,
                    _ => return,
                };
                self.status = self.panes[self.focus].resolve_conflict(reload);
                return;
            }
        }
        if self.grid_key(key) {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.request_quit(),
            KeyCode::Char('c') if ctrl => self.request_quit(),
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

    /// The footer's modal prompt, else its status message.
    fn footer_message(&self) -> Option<String> {
        if self.quit_prompt {
            return Some(
                "Unsaved changes: s save all and quit · d discard and quit · c cancel".into(),
            );
        }
        let conflict = self.panes[self.focus]
            .grid
            .as_ref()
            .is_some_and(|g| g.conflict.is_some());
        if conflict {
            return Some("File changed on disk: r reload (discard my edits) · k keep mine".into());
        }
        self.status.clone().filter(|s| !s.is_empty())
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
                .title(Span::styled(pane_title(pane), title));
            // Content area post-border, for mapping a click to this pane
            // (#244) — computed before `block` moves into `.block(block)`.
            self.pane_areas[i] = block.inner(areas[i]);
            match &mut pane.grid {
                Some(g) => {
                    let shown = g.shown();
                    let table = shown.as_ref().unwrap_or(&g.table);
                    if shown.is_some() {
                        g.state.sync(table);
                    }
                    let view = GridView::new(table).block(block);
                    f.render_stateful_widget(view, areas[i], &mut g.state);
                }
                None => {
                    let view = MarkdownView::new(&pane.text).block(block);
                    f.render_stateful_widget(view, areas[i], &mut pane.state);
                }
            }
        }

        let fp = &self.panes[focus];
        let footer = match (self.footer_message(), &fp.grid) {
            (Some(msg), _) => Line::from(Span::styled(
                format!(" {msg} "),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            (None, grid) => {
                let progress = if fp.fits() {
                    " All ".to_string()
                } else {
                    format!(" {}% ", fp.percent())
                };
                let hint = match grid {
                    Some(g) if g.edit.is_some() => {
                        " Enter/Esc:commit  ^c:cancel  Alt-Enter:newline ".to_string()
                    }
                    Some(g) => {
                        let save = if g.dirty() { "  ^s save" } else { "" };
                        let tab = if n > 1 { "  Tab" } else { "" };
                        format!(" j/k h/l  i:edit  o/O:row  dd  u/^r{save}{tab}  ^o:split  q:quit ")
                    }
                    None if n > 1 => " j/k  ^d/^u  g/G  Tab:pane  o/^o:split  q:quit ".to_string(),
                    None => " j/k ↑↓  ^d/^u  space  g/G  q:quit ".to_string(),
                };
                Line::from(vec![
                    Span::styled(hint, Style::default().fg(Color::DarkGray)),
                    Span::styled(progress, Style::default().fg(Color::Yellow)),
                ])
            }
        };
        f.render_widget(Paragraph::new(footer), outer[1]);
    }
}

#[cfg(test)]
mod tests;
