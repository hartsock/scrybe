// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `scrybe-tui` — view a Markdown file in a scrollable terminal pane.

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use scrybe_tui::app::App;
use std::fs;
use std::io::stdout;
use std::path::PathBuf;

/// Scrybe TUI — a single-pane Markdown viewer for the terminal.
#[derive(Parser)]
#[command(name = "scrybe-tui", version, about)]
struct Cli {
    /// Markdown file to view.
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source =
        fs::read_to_string(&cli.file).with_context(|| format!("reading {}", cli.file.display()))?;
    let mut app = App::from_source(&source, cli.file.display().to_string());

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
