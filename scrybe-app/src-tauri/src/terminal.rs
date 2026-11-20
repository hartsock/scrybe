// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Terminal panel IPC (P4.11): shell state and the `terminal_*` commands.

use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// Simple shell state — one shell process per app window for P4.11.
/// A full async PTY with multiplexed I/O is deferred to a follow-up node.
struct ShellState {
    child: Option<Child>,
}

static SHELL: Mutex<ShellState> = Mutex::new(ShellState { child: None });

// ─── P4.11 — Terminal panel IPC ─────────────────────────────────────────────

/// Spawn the user's default shell and store it in the global [`SHELL`] state.
///
/// Subsequent calls replace the previous child (the old process is abandoned;
/// a graceful kill is a follow-up concern).  The shell's stdin/stdout/stderr
/// are piped so the process doesn't inherit the Tauri host's file descriptors.
/// Full bidirectional I/O multiplexing (PTY) is deferred to a follow-up node.
#[tauri::command]
pub fn terminal_start() -> Result<(), String> {
    let shell = if cfg!(windows) {
        "cmd".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    };
    let child = Command::new(&shell)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start shell {shell}: {e}"))?;
    SHELL.lock().unwrap().child = Some(child);
    Ok(())
}

/// Write `data` to the running shell's stdin.
///
/// This is a P4.11 scaffold — the write path works but reading back async
/// output is not yet wired (requires a spawned reader thread or async runtime
/// integration).  Real interactive PTY output will be addressed in the
/// follow-up node that adds `tauri::async_runtime` + event emission.
#[tauri::command]
pub fn terminal_write(data: String) -> Result<(), String> {
    use std::io::Write as _;
    let mut guard = SHELL.lock().unwrap();
    if let Some(child) = guard.child.as_mut() {
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(data.as_bytes())
                .map_err(|e| format!("write to shell stdin failed: {e}"))?;
        }
    }
    Ok(())
}

/// Run a one-shot command in a fresh subshell and return its combined output.
///
/// This is what the xterm.js frontend calls for each Enter-terminated command
/// entered by the user.  The shell from [`terminal_start`] is not used here —
/// each call spawns a fresh `$SHELL -c <cmd>` so the result is synchronous and
/// easy to return as a plain string.  Persistent session state (cd, env changes)
/// is a follow-up concern.
#[tauri::command]
pub fn terminal_run(cmd: String) -> Result<String, String> {
    let shell = if cfg!(windows) {
        "cmd".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    };
    let flag = if cfg!(windows) { "/C" } else { "-c" };
    let output = Command::new(&shell)
        .arg(flag)
        .arg(&cmd)
        .current_dir(std::env::current_dir().unwrap_or_default())
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok(if stderr.is_empty() {
        stdout
    } else {
        format!("{stdout}{stderr}")
    })
}
