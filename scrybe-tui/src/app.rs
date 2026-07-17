// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The single-pane viewer: state + the ratatui draw / key-handling loop.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::time::Duration;

use crate::render;
use scrybe_core::ast::Ast;

/// A single-pane Markdown viewer.
pub struct App {
    title: String,
    text: Text<'static>,
    line_count: usize,
    scroll: u16,
    /// Content-area height from the last draw — used for paging and clamping.
    viewport: u16,
    quit: bool,
}

impl App {
    /// Build a viewer from Markdown source and a display title (e.g. file path).
    pub fn from_source(source: &str, title: impl Into<String>) -> Self {
        let text = render::render(&Ast::parse(source));
        let line_count = text.lines.len();
        Self {
            title: title.into(),
            text,
            line_count,
            scroll: 0,
            viewport: 1,
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

    fn max_scroll(&self) -> u16 {
        (self.line_count as u16).saturating_sub(self.viewport)
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (self.viewport / 2).max(1) as i32;
        let page = self.viewport.saturating_sub(1).max(1) as i32;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),
            KeyCode::Char('d') if ctrl => self.scroll_by(half),
            KeyCode::Char('u') if ctrl => self.scroll_by(-half),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_by(page),
            KeyCode::PageUp => self.scroll_by(-page),
            KeyCode::Char('g') | KeyCode::Home => self.scroll = 0,
            KeyCode::Char('G') | KeyCode::End => self.scroll = self.max_scroll(),
            _ => {}
        }
    }

    fn scroll_by(&mut self, delta: i32) {
        let next = (self.scroll as i32 + delta).clamp(0, self.max_scroll() as i32);
        self.scroll = next as u16;
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            format!(" {} ", self.title),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        // Content viewport = the block's inner height; keep scroll in range on resize.
        self.viewport = block.inner(chunks[0]).height.max(1);
        self.scroll = self.scroll.min(self.max_scroll());

        let body = Paragraph::new(self.text.clone())
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        f.render_widget(body, chunks[0]);

        let max = self.max_scroll();
        let pct = if max == 0 {
            100
        } else {
            (self.scroll as usize * 100 / max as usize).min(100)
        };
        let footer = Line::from(vec![
            Span::styled(
                " j/k ↑↓  ^d/^u  space  g/G  q:quit ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!(" {pct}% "), Style::default().fg(Color::Yellow)),
        ]);
        f.render_widget(Paragraph::new(footer), chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_clamps_within_bounds() {
        let mut app = App::from_source("# A\n\nbody\n", "t.md");
        app.viewport = 1;
        app.line_count = 5;
        app.scroll_by(100);
        assert_eq!(app.scroll, app.max_scroll());
        app.scroll_by(-100);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn quit_key_sets_flag() {
        let mut app = App::from_source("x\n", "t.md");
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.quit);
    }
}
