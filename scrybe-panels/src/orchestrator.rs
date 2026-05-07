// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Bake-off orchestrator — broadcasts prompt to N agents, collects responses.

use scrybe_mcp_client::AgentRegistry;

/// Sends a prompt to all enabled agents and collects their responses.
pub struct PanelOrchestrator {
    registry: AgentRegistry,
}

impl PanelOrchestrator {
    pub fn new(registry: AgentRegistry) -> Self {
        Self { registry }
    }
}
