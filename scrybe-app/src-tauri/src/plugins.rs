// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Python plugin loader (P4.6) and the built-in agent registry.

use std::process::{Command, Stdio};

// ─── P4.6 — Python plugin loader ────────────────────────────────────────────

/// Enumerate Python plugin files found in `~/.config/scrybe/plugins/`.
///
/// Returns a JSON array of objects with `name`, `path`, and `enabled` fields.
/// Returns an empty array if the directory does not exist or cannot be read.
#[tauri::command]
pub fn list_plugins() -> Vec<serde_json::Value> {
    let plugin_dir = dirs::config_dir()
        .map(|d| d.join("scrybe").join("plugins"))
        .filter(|p| p.exists());

    match plugin_dir {
        None => vec![],
        Some(dir) => {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                return vec![];
            };
            entries
                .flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("py"))
                .map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let path = e.path().to_string_lossy().into_owned();
                    serde_json::json!({ "name": name, "path": path, "enabled": true })
                })
                .collect()
        }
    }
}

/// Invoke a Python plugin script with `source` on stdin.
///
/// The plugin must read from stdin and write modified content to stdout.
/// A non-zero exit code is surfaced as an `Err`.
#[tauri::command]
pub fn run_plugin(path: String, source: String) -> Result<String, String> {
    use std::io::Write as PluginWrite;

    let python = which_python()?;
    let mut child = Command::new(&python)
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn plugin {path}: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{source}");
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("plugin {path} failed: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// Locate a Python interpreter on the system PATH.
fn which_python() -> Result<String, String> {
    for candidate in &["python3", "python"] {
        if let Ok(output) = Command::new("which").arg(candidate).output() {
            if output.status.success() {
                return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
        }
    }
    Err("python3 not found on PATH".to_string())
}

/// Return the built-in agent registry (static list; persistence is P4.5+).
#[tauri::command]
pub fn get_builtin_agents() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id":"claude-code","displayName":"Claude Code","description":"Anthropic Claude Code CLI","enabled":false}),
        serde_json::json!({"id":"codex","displayName":"OpenAI Codex CLI","description":"OpenAI Codex via codex CLI","enabled":false}),
        serde_json::json!({"id":"anthropic-api","displayName":"Anthropic API","description":"Direct Anthropic Messages API","enabled":false}),
        serde_json::json!({"id":"openai-api","displayName":"OpenAI API","description":"Direct OpenAI Chat API","enabled":false}),
        serde_json::json!({"id":"ollama","displayName":"Ollama (local)","description":"Local Ollama inference","enabled":false}),
    ]
}

/// Toggle an agent's enabled state (in-memory only for P4.4; persistence is P4.5+).
#[tauri::command]
pub fn set_agent_enabled(id: String, enabled: bool) -> bool {
    tracing::info!("agent {} enabled={}", id, enabled);
    true
}
