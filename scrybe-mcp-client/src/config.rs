// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! User-level agent configuration.

use serde::Deserialize;

/// One `[[agents]]` entry in `agents.toml`.
#[derive(Debug, Deserialize)]
pub struct AgentConfigEntry {
    pub id: String,
    #[serde(default)]
    pub enabled: bool,
    /// Override the command (stdio only).
    pub command: Option<String>,
    /// Override the args (stdio only).
    pub args: Option<Vec<String>>,
    /// Override the URL (SSE only).
    pub url: Option<String>,
}

/// Loads agent configuration from `~/.config/scrybe/agents.toml`.
/// Returns an empty vec if the file doesn't exist or can't be parsed.
pub fn load_agent_config() -> Vec<AgentConfigEntry> {
    let path = dirs::config_dir().map(|d| d.join("scrybe").join("agents.toml"));
    let path = match path.filter(|p| p.exists()) {
        Some(p) => p,
        None => return vec![],
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    #[derive(Deserialize)]
    struct Root {
        #[serde(default)]
        agents: Vec<AgentConfigEntry>,
    }
    toml::from_str::<Root>(&content)
        .map(|r| r.agents)
        .unwrap_or_default()
}
