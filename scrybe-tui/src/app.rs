// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The viewer binary's driver: one or more document panes (split horizontally
//! or vertically) around the reusable [`MarkdownView`] widget, with live reload
//! — a pane re-renders when its backing file changes on disk. Another project
//! embeds [`crate::view`] directly; this is Scrybe's own thin host.

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::render;
use crate::view::{MarkdownView, MarkdownViewState};
use scrybe_core::ast::Ast;

/// One document pane in the viewer.
struct Pane {
    title: String,
    /// Backing file, if this pane was loaded from disk (enables live reload).
    path: Option<PathBuf>,
    mtime: Option<SystemTime>,
    text: Text<'static>,
    state: MarkdownViewState,
}

impl Pane {
    fn from_source(source: &str, title: impl Into<String>) -> Self {
        let text = render::render(&Ast::parse(source));
        let state = MarkdownViewState::new(text.lines.len());
        Self {
            title: title.into(),
            path: None,
            mtime: None,
            text,
            state,
        }
    }

    fn from_file(path: PathBuf) -> Result<Self> {
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut pane = Self::from_source(&source, path.display().to_string());
        pane.mtime = file_mtime(&path);
        pane.path = Some(path);
        Ok(pane)
    }

    /// Re-render from `source`, preserving the scroll position where possible.
    fn set_source(&mut self, source: &str) {
        self.text = render::render(&Ast::parse(source));
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

fn file_mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The viewer: one or more document panes, split horizontally or vertically,
/// with a focused pane that receives scroll keys. Panes loaded from files
/// reload live when the file changes on disk.
pub struct App {
    panes: Vec<Pane>,
    focus: usize,
    orientation: Direction,
    quit: bool,
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
        Self {
            panes,
            focus: 0,
            orientation: Direction::Horizontal,
            quit: false,
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
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
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

    fn focused(&mut self) -> &mut MarkdownViewState {
        &mut self.panes[self.focus].state
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
            KeyCode::Char('j') | KeyCode::Down => self.focused().scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.focused().scroll_by(-1),
            KeyCode::Char('d') if ctrl => self.focused().half_page(true),
            KeyCode::Char('u') if ctrl => self.focused().half_page(false),
            KeyCode::PageDown | KeyCode::Char(' ') => self.focused().page(true),
            KeyCode::PageUp => self.focused().page(false),
            KeyCode::Char('g') | KeyCode::Home => self.focused().scroll_to_top(),
            KeyCode::Char('G') | KeyCode::End => self.focused().scroll_to_bottom(),
            _ => {}
        }
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
            let view = MarkdownView::new(&pane.text).block(block);
            f.render_stateful_widget(view, areas[i], &mut pane.state);
        }

        let fp = &self.panes[focus];
        let progress = if fp.state.fits() {
            " All ".to_string()
        } else {
            format!(" {}% ", fp.state.percent())
        };
        let hint = if n > 1 {
            " j/k  ^d/^u  g/G  Tab:pane  o:split  q:quit "
        } else {
            " j/k ↑↓  ^d/^u  space  g/G  q:quit "
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
}
