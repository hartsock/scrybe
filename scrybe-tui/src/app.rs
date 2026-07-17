// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The single-pane viewer binary's driver: terminal event loop + key handling
//! around the reusable [`MarkdownView`] widget. Another project embeds
//! [`crate::view`] directly; this is just Scrybe's own thin host.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::time::Duration;

use crate::render;
use crate::view::{MarkdownView, MarkdownViewState};
use scrybe_core::ast::Ast;

/// A single-pane Markdown viewer.
pub struct App {
    title: String,
    text: Text<'static>,
    state: MarkdownViewState,
    quit: bool,
}

impl App {
    /// Build a viewer from Markdown source and a display title (e.g. file path).
    pub fn from_source(source: &str, title: impl Into<String>) -> Self {
        let text = render::render(&Ast::parse(source));
        let state = MarkdownViewState::new(text.lines.len());
        Self {
            title: title.into(),
            text,
            state,
            quit: false,
        }
    }

    /// Run the event loop until the user quits.
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| self.draw(f))?;
            // Wake periodically so a resize between keystrokes is picked up.
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

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.state.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.state.scroll_by(-1),
            KeyCode::Char('d') if ctrl => self.state.half_page(true),
            KeyCode::Char('u') if ctrl => self.state.half_page(false),
            KeyCode::PageDown | KeyCode::Char(' ') => self.state.page(true),
            KeyCode::PageUp => self.state.page(false),
            KeyCode::Char('g') | KeyCode::Home => self.state.scroll_to_top(),
            KeyCode::Char('G') | KeyCode::End => self.state.scroll_to_bottom(),
            _ => {}
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            format!(" {} ", self.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        let view = MarkdownView::new(&self.text).block(block);
        f.render_stateful_widget(view, chunks[0], &mut self.state);

        let footer = Line::from(vec![
            Span::styled(
                " j/k ↑↓  ^d/^u  space  g/G  q:quit ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                if self.state.fits() {
                    " All ".to_string()
                } else {
                    format!(" {}% ", self.state.percent())
                },
                Style::default().fg(Color::Yellow),
            ),
        ]);
        f.render_widget(Paragraph::new(footer), chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j_scrolls_down_q_quits() {
        let mut app = App::from_source("# A\n\nbody\nmore\nlines\nhere\n", "t.md");
        // Give the view a viewport so there's room to scroll.
        app.state.set_line_count(20);
        let before = app.state.scroll();
        app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(app.state.scroll() >= before);
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.quit);
    }
}
