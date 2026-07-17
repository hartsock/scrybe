// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The viewer binary's driver: one or more document panes (split horizontally
//! or vertically) around the reusable [`MarkdownView`] widget. Another project
//! embeds [`crate::view`] directly; this is Scrybe's own thin host.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::time::Duration;

use crate::render;
use crate::view::{MarkdownView, MarkdownViewState};
use scrybe_core::ast::Ast;

/// One document pane in the viewer.
struct Pane {
    title: String,
    text: Text<'static>,
    state: MarkdownViewState,
}

impl Pane {
    fn new(source: &str, title: impl Into<String>) -> Self {
        let text = render::render(&Ast::parse(source));
        let state = MarkdownViewState::new(text.lines.len());
        Self {
            title: title.into(),
            text,
            state,
        }
    }
}

/// The viewer: one or more document panes, split horizontally or vertically,
/// with a focused pane that receives scroll keys.
pub struct App {
    panes: Vec<Pane>,
    focus: usize,
    orientation: Direction,
    quit: bool,
}

impl App {
    /// A single-pane viewer.
    pub fn from_source(source: &str, title: impl Into<String>) -> Self {
        Self {
            panes: vec![Pane::new(source, title)],
            focus: 0,
            orientation: Direction::Horizontal,
            quit: false,
        }
    }

    /// A viewer with one pane per `(source, title)` — a split screen. Falls back
    /// to a single placeholder pane if given nothing.
    pub fn from_documents<T: Into<String>>(docs: impl IntoIterator<Item = (String, T)>) -> Self {
        let mut panes: Vec<Pane> = docs.into_iter().map(|(s, t)| Pane::new(&s, t)).collect();
        if panes.is_empty() {
            panes.push(Pane::new("", "(empty)"));
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
        }
        Ok(())
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
}
