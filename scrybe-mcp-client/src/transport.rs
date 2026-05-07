// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! MCP transport types.

use serde::{Deserialize, Serialize};

/// How to connect to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Transport {
    /// Spawn a subprocess and communicate via stdin/stdout.
    Stdio { command: String, args: Vec<String> },
    /// Connect to an SSE endpoint.
    Sse { url: String },
}
