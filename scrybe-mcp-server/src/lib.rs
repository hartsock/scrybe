// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe MCP server — inbound MCP, exposes editor tools to external agents.
//!
//! Tools: open, read, section, edit, find, render, embed, extract, lint.
//! Transport: stdio (primary), SSE (future).

pub mod server;
pub mod tools;

pub use server::McpServer;
pub use tools::ToolRegistry;
