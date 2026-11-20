// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe TUI — a Markdown viewer and CSV/TSV editor for the terminal.
//!
//! Another lens onto the same `scrybe-core` AST as the desktop app, the CLI,
//! and the MCP server: parse Markdown into `scrybe_core::ast::Ast`, render it to
//! styled `ratatui` text ([`render`]), and view it in a scrollable pane
//! ([`app::App`]). `.csv` / `.tsv` files open as an editable grid instead: a
//! cursor, cell edits, row insert and delete, undo and redo, and a save that
//! rewrites only the bytes you edited. A pane with unsaved edits is never
//! replaced by a reload and never lost on quit. No
//! tabs — documents sit in side-by-side or stacked panes.
//!
//! The rendering layer lives in the standalone [`scrybe-ratatui`] crate
//! (#194) so other ratatui apps can depend on it without this viewer's
//! event loop; `render` and `view` are re-exported here verbatim, so every
//! pre-extraction `scrybe_tui::render::…` / `scrybe_tui::view::…` path keeps
//! working.
//!
//! [`scrybe-ratatui`]: https://crates.io/crates/scrybe-ratatui

pub mod app;
pub use scrybe_ratatui::render;
pub use scrybe_ratatui::view;
