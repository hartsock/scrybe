// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Render a `scrybe-core` Markdown document as styled `ratatui` text.
//!
//! Another lens onto the same AST as the Scrybe desktop app, CLI, and MCP
//! server — extracted from `scrybe-tui` (#194) so any ratatui application can
//! display Markdown consistent with Scrybe's look by *just depending on this
//! crate*: no crossterm, no file IO, no event loop, no terminal backend.
//! Consumers own their own loop and drop the widget into their own layout.
//!
//! - [`render`] / [`render_source`] — walk the AST (or parse source first)
//!   into an owned, styled [`ratatui::text::Text`].
//! - [`MarkdownView`] — a `StatefulWidget` for one half of a split, a popup,
//!   a sidebar; [`MarkdownViewState`] holds the scroll position and adapts to
//!   the viewport at render time.
//!
//! ```no_run
//! # use ratatui::{Frame, layout::Rect, widgets::{Block, Borders}};
//! use scrybe_ratatui::{render_source, MarkdownView, MarkdownViewState};
//!
//! # fn draw(f: &mut Frame, area: Rect, state: &mut MarkdownViewState) {
//! let text = render_source("# Hello\n\nworld");
//! let view = MarkdownView::new(&text).block(Block::default().borders(Borders::ALL));
//! f.render_stateful_widget(view, area, state);
//! # }
//! ```
//!
//! ## Cargo features
//!
//! The default dependency surface is deliberately tiny — `scrybe-core` +
//! `ratatui`, nothing else (#194) — and stays that way unless you opt in:
//!
//! - **`highlight`** (off by default, #164): syntect syntax highlighting for
//!   fenced code blocks. Adds the `syntect` dependency. Languages syntect
//!   recognizes render with per-line color; unknown languages fall back to
//!   the plain rendering, byte-identical to a build without the feature. No
//!   API changes — [`render`]/[`render_source`] simply gain the behavior.
//!
//! Future rendering concerns (terminal-graphics Mermaid, #167) land here too,
//! so every consumer inherits them.
//!
//! **ratatui compatibility:** `MarkdownView: StatefulWidget` ties this crate to
//! a ratatui major line (currently 0.29); a ratatui bump is a semver event.

pub mod render;
pub mod view;

pub use render::{render, render_source};
pub use view::{MarkdownView, MarkdownViewState};
