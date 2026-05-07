// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Built-in harness adapter presets.
//!
//! Each preset describes how to launch a known agent as an MCP stdio server.
//! Presets are disabled by default -- operators enable them via
//! `~/.config/scrybe/agents.toml` or the Scrybe workspace UI (P4.4).

use crate::registry::AgentEntry;
use crate::transport::Transport;

/// A named preset for a standard agent harness.
#[derive(Debug, Clone)]
pub struct HarnessPreset {
    /// Short identifier used in config files and the registry.
    pub id: &'static str,
    /// Human-readable name shown in the UI.
    pub display_name: &'static str,
    /// One-line description shown in the agent registration panel.
    pub description: &'static str,
    /// Default `AgentEntry` (always `enabled: false`).
    pub entry: AgentEntry,
}

/// Returns all five built-in harness presets.
pub fn builtin_presets() -> Vec<HarnessPreset> {
    vec![
        claude_code(),
        codex(),
        anthropic_api(),
        openai_api(),
        ollama(),
    ]
}

/// Looks up a preset by its `id`.
pub fn get_preset(id: &str) -> Option<HarnessPreset> {
    builtin_presets().into_iter().find(|p| p.id == id)
}

fn claude_code() -> HarnessPreset {
    HarnessPreset {
        id: "claude-code",
        display_name: "Claude Code",
        description: "Anthropic Claude Code CLI (requires `claude` on PATH + ANTHROPIC_API_KEY).",
        entry: AgentEntry {
            name: "claude-code".to_string(),
            transport: Transport::Stdio {
                command: "claude".to_string(),
                args: vec!["mcp".to_string(), "serve".to_string()],
            },
            enabled: false,
        },
    }
}

fn codex() -> HarnessPreset {
    HarnessPreset {
        id: "codex",
        display_name: "OpenAI Codex CLI",
        description: "OpenAI Codex via `codex` CLI (requires `codex` on PATH + OPENAI_API_KEY).",
        entry: AgentEntry {
            name: "codex".to_string(),
            transport: Transport::Stdio {
                command: "codex".to_string(),
                args: vec!["mcp-server".to_string()],
            },
            enabled: false,
        },
    }
}

fn anthropic_api() -> HarnessPreset {
    HarnessPreset {
        id: "anthropic-api",
        display_name: "Anthropic API (direct)",
        description: "Direct Anthropic Messages API via local SSE bridge on :3001 (requires ANTHROPIC_API_KEY).",
        entry: AgentEntry {
            name: "anthropic-api".to_string(),
            transport: Transport::Sse {
                url: "http://localhost:3001/anthropic/mcp".to_string(),
            },
            enabled: false,
        },
    }
}

fn openai_api() -> HarnessPreset {
    HarnessPreset {
        id: "openai-api",
        display_name: "OpenAI API (direct)",
        description:
            "Direct OpenAI Chat API via local SSE bridge on :3001 (requires OPENAI_API_KEY).",
        entry: AgentEntry {
            name: "openai-api".to_string(),
            transport: Transport::Sse {
                url: "http://localhost:3001/openai/mcp".to_string(),
            },
            enabled: false,
        },
    }
}

fn ollama() -> HarnessPreset {
    HarnessPreset {
        id: "ollama",
        display_name: "Ollama (local inference)",
        description:
            "Local Ollama on :11434 (requires `ollama serve`; default model: qwen3-coder:30b).",
        entry: AgentEntry {
            name: "ollama".to_string(),
            transport: Transport::Sse {
                url: "http://localhost:11434/api/mcp".to_string(),
            },
            enabled: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_count() {
        assert_eq!(builtin_presets().len(), 5);
    }

    #[test]
    fn test_all_disabled_by_default() {
        for p in builtin_presets() {
            assert!(!p.entry.enabled, "{} should be disabled by default", p.id);
        }
    }

    #[test]
    fn test_get_preset_found() {
        for id in &[
            "claude-code",
            "codex",
            "anthropic-api",
            "openai-api",
            "ollama",
        ] {
            assert!(get_preset(id).is_some(), "preset {id} not found");
        }
    }

    #[test]
    fn test_get_preset_not_found() {
        assert!(get_preset("nonexistent").is_none());
    }

    #[test]
    fn test_registry_load_presets() {
        let mut r = crate::registry::AgentRegistry::new();
        r.load_presets();
        assert_eq!(r.list().count(), 5);
    }

    #[test]
    fn test_registry_load_presets_idempotent() {
        let mut r = crate::registry::AgentRegistry::new();
        r.load_presets();
        r.load_presets();
        assert_eq!(r.list().count(), 5);
    }

    #[test]
    fn test_registry_existing_entry_not_overwritten() {
        use crate::transport::Transport;
        let mut r = crate::registry::AgentRegistry::new();
        r.register(crate::registry::AgentEntry {
            name: "claude-code".to_string(),
            transport: Transport::Stdio {
                command: "custom".to_string(),
                args: vec![],
            },
            enabled: true,
        });
        r.load_presets();
        let entry = r.get("claude-code").unwrap();
        assert!(entry.enabled, "custom entry should not be overwritten");
        if let Transport::Stdio { ref command, .. } = entry.transport {
            assert_eq!(command, "custom");
        }
    }
}
