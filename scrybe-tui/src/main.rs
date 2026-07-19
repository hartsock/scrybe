// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `scrybe-tui` — view one or more Markdown files in scrollable terminal panes.

use anyhow::Result;
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Direction;
use ratatui::Terminal;
use scrybe_tui::app::App;
use std::io::stdout;
use std::path::PathBuf;

/// Scrybe TUI — a Markdown viewer for the terminal. Two or more files open in a
/// split screen (Tab switches panes, `o` toggles the split orientation). Panes
/// reload live when their file changes on disk.
#[derive(Parser)]
#[command(name = "scrybe-tui", version, about)]
struct Cli {
    /// Markdown file(s) to view. Two or more open as a split screen.
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Start with a vertical (stacked) split instead of horizontal (side-by-side).
    #[arg(long)]
    vertical: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut app = App::from_files(cli.files)?;
    if cli.vertical {
        app = app.orientation(Direction::Vertical);
    }

    let mut terminal = setup_terminal()?;
    let res = app.run(&mut terminal);
    restore_terminal();
    res
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
