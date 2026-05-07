// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe MCP client -- outbound MCP, registers N agent servers.
//!
//! Each registered server exposes tools that Scrybe can invoke
//! on behalf of the user. Supported transports: stdio (P3.1), SSE (future).

pub mod client;
pub mod config;
pub mod harness;
pub mod jsonrpc;
pub mod registry;
pub mod stdio_transport;
pub mod transport;

pub use client::{McpClient, ServerInfo, ToolDef};
pub use config::{load_agent_config, AgentConfigEntry};
pub use harness::{builtin_presets, get_preset, HarnessPreset};
pub use registry::AgentRegistry;
pub use transport::Transport;
