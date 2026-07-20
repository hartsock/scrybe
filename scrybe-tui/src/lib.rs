// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe TUI — a single-pane Markdown viewer for the terminal.
//!
//! Another lens onto the same `scrybe-core` AST as the desktop app, the CLI,
//! and the MCP server: parse Markdown into `scrybe_core::ast::Ast`, render it to
//! styled `ratatui` text ([`render`]), and view it in a scrollable pane
//! ([`app::App`]). No tabs — one document, one pane.
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
