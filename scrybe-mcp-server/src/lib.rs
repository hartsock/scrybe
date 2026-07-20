// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe MCP server — inbound MCP, exposes editor tools to external agents.
//!
//! A thin transport shim over the shared `scrybe_tools::Registry` — the ONE
//! tool registry, dispatch, and schema source (shared with the CLI). See
//! `server.rs`.
//! Transport: stdio (primary), SSE (future).

pub mod server;

pub use server::McpServer;
