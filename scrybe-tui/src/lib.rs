// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe TUI — a single-pane Markdown viewer for the terminal.
//!
//! Another lens onto the same `scrybe-core` AST as the desktop app, the CLI,
//! and the MCP server: parse Markdown into `scrybe_core::ast::Ast`, render it to
//! styled `ratatui` text ([`render`]), and view it in a scrollable pane
//! ([`app::App`]). No tabs — one document, one pane.

pub mod app;
pub mod render;
