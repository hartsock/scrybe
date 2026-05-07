// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Agent registry -- named MCP server connections.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::transport::Transport;

/// A registered agent server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Human-readable name (e.g. "claude-code", "codex", "ollama-qwen").
    pub name: String,
    pub transport: Transport,
    /// Whether this agent is currently enabled.
    pub enabled: bool,
}

/// Registry of all configured MCP agent servers.
#[derive(Debug, Default)]
pub struct AgentRegistry {
    agents: HashMap<String, AgentEntry>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, entry: AgentEntry) {
        self.agents.insert(entry.name.clone(), entry);
    }

    pub fn get(&self, name: &str) -> Option<&AgentEntry> {
        self.agents.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &AgentEntry> {
        self.agents.values()
    }

    /// Registers all built-in presets as disabled entries.
    /// Existing entries (e.g. from a user config) are not overwritten.
    pub fn load_presets(&mut self) {
        use crate::harness::builtin_presets;
        for preset in builtin_presets() {
            self.agents
                .entry(preset.entry.name.clone())
                .or_insert(preset.entry);
        }
    }
}
