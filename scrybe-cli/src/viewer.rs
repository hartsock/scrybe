// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `scrybe view` — a thin, single-file TUI Markdown viewer.
//!
//! Renders one document with the published `scrybe-ratatui` crate
//! ([`render_source`] + [`MarkdownView`]/[`MarkdownViewState`]) and drives a
//! minimal read-only event loop: scroll and quit. Deliberately the single-pane
//! subset of `scrybe-tui` — no splits, no live reload — because the published
//! `scrybe-cli` must not depend on the unpublished `scrybe-tui` crate (#194).
//! Groundwork for exposing the same viewer loop to other hosts (#165).
//!
//! Key handling is factored into pure functions ([`key_action`] /
//! [`apply_action`]) so the mapping is unit-testable without a terminal.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use scrybe_ratatui::{render_source, MarkdownView, MarkdownViewState};
use std::io::{stdin, stdout, IsTerminal};
use std::path::Path;

/// What a key press means to the viewer — pure data, so the key mapping is
/// testable headlessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Leave the viewer (`q`, `Esc`, `Ctrl-c`).
    Quit,
    /// Scroll by a signed number of lines (`j`/`k`, arrows).
    ScrollBy(i32),
    /// Half-viewport scroll; `true` = down (`Ctrl-d` / `Ctrl-u`).
    HalfPage(bool),
    /// Near-full-viewport scroll; `true` = down (`PageDown`/`Space` / `PageUp`).
    Page(bool),
    /// Jump to the top (`g`, `Home`).
    Top,
    /// Jump to the bottom (`G`, `End`).
    Bottom,
}

/// Map a key event to a viewer action — the single-pane subset of the
/// `scrybe-tui` bindings (no pane switching, no split toggling).
pub fn key_action(key: KeyEvent) -> Option<KeyAction> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    Some(match key.code {
        KeyCode::Char('q') | KeyCode::Esc => KeyAction::Quit,
        KeyCode::Char('c') if ctrl => KeyAction::Quit,
        KeyCode::Char('j') | KeyCode::Down => KeyAction::ScrollBy(1),
        KeyCode::Char('k') | KeyCode::Up => KeyAction::ScrollBy(-1),
        KeyCode::Char('d') if ctrl => KeyAction::HalfPage(true),
        KeyCode::Char('u') if ctrl => KeyAction::HalfPage(false),
        KeyCode::PageDown | KeyCode::Char(' ') => KeyAction::Page(true),
        KeyCode::PageUp => KeyAction::Page(false),
        KeyCode::Char('g') | KeyCode::Home => KeyAction::Top,
        KeyCode::Char('G') | KeyCode::End => KeyAction::Bottom,
        _ => return None,
    })
}

/// Apply an action to the scroll state. Returns `true` when the viewer
/// should exit.
pub fn apply_action(action: KeyAction, state: &mut MarkdownViewState) -> bool {
    match action {
        KeyAction::Quit => return true,
        KeyAction::ScrollBy(delta) => state.scroll_by(delta),
        KeyAction::HalfPage(down) => state.half_page(down),
        KeyAction::Page(down) => state.page(down),
        KeyAction::Top => state.scroll_to_top(),
        KeyAction::Bottom => state.scroll_to_bottom(),
    }
    false
}

/// The interactive viewer needs a real terminal on both ends: raw-mode key
/// input from stdin, and the alternate screen on stdout.
fn check_tty(stdin_tty: bool, stdout_tty: bool) -> Result<(), &'static str> {
    if stdin_tty && stdout_tty {
        Ok(())
    } else {
        Err("scrybe view requires a terminal (stdin and stdout must be a TTY)")
    }
}

/// Run `scrybe view <FILE>`: render the file and drive the viewer loop until
/// the user quits.
///
/// A missing or unreadable file is an ordinary CLI error; running without an
/// interactive terminal prints a friendly message and exits 2.
pub fn run(path: &Path) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("scrybe view: cannot read {}: {e}", path.display()))?;
    if let Err(msg) = check_tty(stdin().is_terminal(), stdout().is_terminal()) {
        eprintln!("{msg}");
        std::process::exit(2);
    }

    let text = render_source(&source);
    let mut state = MarkdownViewState::new(text.lines.len());
    let title = path.display().to_string();

    let mut terminal = setup_terminal()?;
    let res = event_loop(&mut terminal, &text, &title, &mut state);
    restore_terminal();
    res
}

/// Block on events, redrawing after each one, until a key maps to
/// [`KeyAction::Quit`]. Resize events simply fall through to the next draw.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    text: &Text<'_>,
    title: &str,
    state: &mut MarkdownViewState,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, text, title, state))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                if let Some(action) = key_action(key) {
                    if apply_action(action, state) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// One bordered document pane over a one-line key-hint + progress footer —
/// the same look as `scrybe-tui`'s single-pane mode.
fn draw(f: &mut Frame, text: &Text<'_>, title: &str, state: &mut MarkdownViewState) {
    let outer = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_stateful_widget(MarkdownView::new(text).block(block), outer[0], state);

    let progress = if state.fits() {
        " All ".to_string()
    } else {
        format!(" {}% ", state.percent())
    };
    let footer = Line::from(vec![
        Span::styled(
            " j/k ↑↓  ^d/^u  space  g/G  q:quit ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(progress, Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(footer), outer[1]);
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    // Restore the terminal even if a later panic unwinds past our teardown.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        prev(info);
    }));
    Ok(Terminal::new(CrosstermBackend::new(stdout()))?)
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn quit_keys_map_to_quit() {
        assert_eq!(key_action(key(KeyCode::Char('q'))), Some(KeyAction::Quit));
        assert_eq!(key_action(key(KeyCode::Esc)), Some(KeyAction::Quit));
        assert_eq!(key_action(ctrl('c')), Some(KeyAction::Quit));
    }

    #[test]
    fn scroll_keys_map_to_state_deltas() {
        assert_eq!(
            key_action(key(KeyCode::Char('j'))),
            Some(KeyAction::ScrollBy(1))
        );
        assert_eq!(key_action(key(KeyCode::Down)), Some(KeyAction::ScrollBy(1)));
        assert_eq!(
            key_action(key(KeyCode::Char('k'))),
            Some(KeyAction::ScrollBy(-1))
        );
        assert_eq!(key_action(key(KeyCode::Up)), Some(KeyAction::ScrollBy(-1)));
        assert_eq!(key_action(ctrl('d')), Some(KeyAction::HalfPage(true)));
        assert_eq!(key_action(ctrl('u')), Some(KeyAction::HalfPage(false)));
        assert_eq!(
            key_action(key(KeyCode::PageDown)),
            Some(KeyAction::Page(true))
        );
        assert_eq!(
            key_action(key(KeyCode::Char(' '))),
            Some(KeyAction::Page(true))
        );
        assert_eq!(
            key_action(key(KeyCode::PageUp)),
            Some(KeyAction::Page(false))
        );
        assert_eq!(key_action(key(KeyCode::Char('g'))), Some(KeyAction::Top));
        assert_eq!(key_action(key(KeyCode::Home)), Some(KeyAction::Top));
        assert_eq!(key_action(key(KeyCode::Char('G'))), Some(KeyAction::Bottom));
        assert_eq!(key_action(key(KeyCode::End)), Some(KeyAction::Bottom));
    }

    #[test]
    fn unmapped_keys_do_nothing() {
        assert_eq!(key_action(key(KeyCode::Char('x'))), None);
        // Multi-pane bindings from scrybe-tui are deliberately absent here.
        assert_eq!(key_action(key(KeyCode::Tab)), None);
        assert_eq!(key_action(key(KeyCode::Char('o'))), None);
        // d/u scroll only with Ctrl held.
        assert_eq!(key_action(key(KeyCode::Char('d'))), None);
        assert_eq!(key_action(key(KeyCode::Char('u'))), None);
    }

    #[test]
    fn apply_action_moves_state_and_signals_quit() {
        let mut s = MarkdownViewState::new(100);
        assert!(!apply_action(KeyAction::ScrollBy(3), &mut s));
        assert_eq!(s.scroll(), 3);
        assert!(!apply_action(KeyAction::ScrollBy(-1), &mut s));
        assert_eq!(s.scroll(), 2);
        assert!(!apply_action(KeyAction::Bottom, &mut s));
        assert_eq!(s.scroll(), s.max_scroll());
        assert!(!apply_action(KeyAction::Top, &mut s));
        assert_eq!(s.scroll(), 0);
        assert!(apply_action(KeyAction::Quit, &mut s), "quit signals exit");
    }

    #[test]
    fn check_tty_requires_both_ends() {
        assert!(check_tty(true, true).is_ok());
        for (stdin_tty, stdout_tty) in [(false, true), (true, false), (false, false)] {
            let err = check_tty(stdin_tty, stdout_tty).unwrap_err();
            assert!(err.contains("requires a terminal"), "{err}");
        }
    }

    #[test]
    fn missing_file_errors_before_any_terminal_setup() {
        // `run` must fail on the unreadable file *before* the TTY gate or any
        // raw-mode setup — this test runs headlessly and must not exit(2).
        let path = std::env::temp_dir().join(format!(
            "scrybe-view-missing-{}-{}.md",
            std::process::id(),
            line!()
        ));
        let err = run(&path).unwrap_err().to_string();
        assert!(err.contains("scrybe view: cannot read"), "{err}");
    }
}
