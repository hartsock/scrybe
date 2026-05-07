// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe Tauri backend library.
//!
//! Exposes Tauri IPC commands that the TypeScript frontend (CodeMirror 6) calls
//! via `invoke(...)`.  This scaffold wires up the foundational commands:
//!
//! - [`render_markdown`]   — converts Markdown source to HTML via `scrybe-render`
//! - [`get_version`]       — returns the crate version string
//! - [`list_directory`]    — enumerate directory contents
//! - [`read_file`]         — read file text content
//! - [`get_builtin_agents`] — enumerate built-in AI agents
//! - [`set_agent_enabled`] — toggle an agent on/off (in-memory)
//! - [`list_plugins`]      — enumerate Python plugins from ~/.config/scrybe/plugins/
//! - [`run_plugin`]        — invoke a Python plugin with document source on stdin
//! - [`terminal_start`]    — spawn a shell process (P4.11)
//! - [`terminal_write`]    — write data to shell stdin (P4.11 scaffold)
//! - [`terminal_run`]      — run a one-shot command and return its output (P4.11)
//! - [`mcp_server_start`]  — launch scrybe-mcp-server as a stdio sidecar (P4.7)
//! - [`mcp_server_status`] — report whether the MCP sidecar is running (P4.7)
//! - [`mcp_connection_info`] — return connection instructions for MCP clients (P4.7)
//! - [`vcs_open`]          — open a git repository and cache it (P4.8)
//! - [`vcs_status`]        — return working-tree status entries (P4.8)
//! - [`vcs_stage_all`]     — stage all changes (`git add -A`) (P4.8)
//! - [`vcs_commit`]        — create a commit from the staged index (P4.8)
//! - [`vcs_fetch`]         — fetch from a named remote (P4.8)
//! - [`vcs_log`]           — return recent commit summaries (P4.8)
//! - [`vcs_remotes`]       — list configured remotes with roles (P4.8)
//!
//! Full editor integration (P4.2–P4.11) builds on this IPC bridge.

use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{Emitter as _, Manager as _};

/// Simple shell state — one shell process per app window for P4.11.
/// A full async PTY with multiplexed I/O is deferred to a follow-up node.
struct ShellState {
    child: Option<Child>,
}

static SHELL: Mutex<ShellState> = Mutex::new(ShellState { child: None });

// ─── P4.7 — In-app MCP server ────────────────────────────────────────────────

/// Tracks whether the in-app MCP sidecar is running.
static MCP_RUNNING: AtomicBool = AtomicBool::new(false);

// ─── P4.8 — VCS state ────────────────────────────────────────────────────────

use scrybe_vcs::{GitAuthor, ScrybeRepo};
use std::sync::Mutex as VcsMutex;

struct VcsState {
    repo: Option<ScrybeRepo>,
}

static VCS: VcsMutex<VcsState> = VcsMutex::new(VcsState { repo: None });

/// Render a Markdown `source` string to HTML.
///
/// Called from the frontend on every editor change.  The heavy lifting is done
/// by `scrybe_render::render_html`; this command is just the Tauri IPC glue.
#[tauri::command]
fn render_markdown(source: String) -> String {
    use scrybe_core::Document;
    use scrybe_render::{render_html, Theme};
    let doc = Document::new(source);
    render_html(&doc, Theme::Default).html
}

/// Return the application version string baked in at compile time.
#[tauri::command]
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// List the entries of a directory, returning name, path, and isDir for each.
#[tauri::command]
fn list_directory(path: String) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(&path) else {
        return vec![];
    };
    entries
        .flatten()
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let name = e.file_name().to_string_lossy().into_owned();
            let full = format!("{}/{}", path.trim_end_matches('/'), name);
            serde_json::json!({ "name": name, "path": full, "isDir": is_dir })
        })
        .collect()
}

/// Poll for a pending `close_tab` signal from the MCP server.
///
/// The `close_tab` MCP tool writes to `/tmp/scrybe-close-tab.txt`. This
/// command reads and deletes it atomically, returning the path to close
/// (empty string = close active tab) or `null` if no signal is pending.
#[tauri::command]
fn poll_close_tab() -> Option<String> {
    let p = std::path::Path::new("/tmp/scrybe-close-tab.txt");
    if !p.exists() { return None; }
    let content = std::fs::read_to_string(p).ok()?;
    let _ = std::fs::remove_file(p);
    Some(content.trim().to_string())
}

/// Return whether `path` is a `"file"`, `"dir"`, or `"missing"`.
/// Trims trailing slashes so URLs like `file:///foo/bar/` resolve correctly.
#[tauri::command]
fn path_type(path: String) -> &'static str {
    let trimmed = path.trim_end_matches('/');
    let p = std::path::Path::new(trimmed);
    if p.is_dir() { "dir" } else if p.is_file() { "file" } else { "missing" }
}

/// Read the full text content of a file.
#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Write `content` to `path`, creating `<path>.bak` on the first save of each
/// session so the original on-disk version is always recoverable.
///
/// The `.bak` is written only once per path: if it already exists it is left
/// alone, preserving the true "opened from disk" snapshot across multiple
/// autosave cycles.
#[tauri::command]
fn save_file(path: String, content: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let bak = std::path::PathBuf::from(format!("{}.bak", path));
    if p.exists() && !bak.exists() {
        std::fs::copy(p, &bak).map_err(|e| format!("backup failed: {e}"))?;
    }
    std::fs::write(p, content).map_err(|e| format!("write failed: {e}"))
}

/// Return the built-in agent registry (static list; persistence is P4.5+).
#[tauri::command]
fn get_builtin_agents() -> Vec<serde_json::Value> {
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
fn set_agent_enabled(id: String, enabled: bool) -> bool {
    tracing::info!("agent {} enabled={}", id, enabled);
    true
}

// ─── P4.7 — MCP server commands ─────────────────────────────────────────────

/// Start the in-app MCP server as a sidecar process.
///
/// Spawns `scrybe-mcp-server stdio` and keeps it alive.  The server
/// communicates with external agents (Claude Code, Codex, etc.) over
/// stdio via the MCP JSON-RPC protocol.  Calling this when the server is
/// already running is a no-op (returns `"already running"`).
#[tauri::command]
fn mcp_server_start() -> Result<String, String> {
    if MCP_RUNNING.load(Ordering::Relaxed) {
        return Ok("already running".to_string());
    }
    let binary = which_scrybe_mcp_server()?;
    Command::new(&binary)
        .arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start scrybe-mcp-server: {e}"))?;
    MCP_RUNNING.store(true, Ordering::Relaxed);
    Ok(format!("started: {binary}"))
}

/// Returns whether the MCP server is running and its active transport.
#[tauri::command]
fn mcp_server_status() -> serde_json::Value {
    serde_json::json!({
        "running": MCP_RUNNING.load(Ordering::Relaxed),
        "transport": "stdio",
        "info": "Use `claude mcp add scrybe -- scrybe-mcp-server stdio` to connect."
    })
}

/// Returns the MCP connection instructions for the active session.
#[tauri::command]
fn mcp_connection_info() -> serde_json::Value {
    serde_json::json!({
        "stdio": {
            "command": "scrybe-mcp-server",
            "args": ["stdio"],
            "description": "Connect any MCP client via stdio transport"
        },
        "claude_code": "claude mcp add scrybe -- scrybe-mcp-server stdio",
        "codex": "codex mcp add scrybe -- scrybe-mcp-server stdio",
        "tools": [
            "open", "read", "section", "edit", "find",
            "render", "embed", "extract", "lint"
        ]
    })
}

/// Locate the `scrybe-mcp-server` binary on PATH or next to the current executable.
fn which_scrybe_mcp_server() -> Result<String, String> {
    if let Ok(path) = which::which("scrybe-mcp-server") {
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Ok(exe) = std::env::current_exe() {
        let name = if cfg!(windows) {
            "scrybe-mcp-server.exe"
        } else {
            "scrybe-mcp-server"
        };
        let sibling = exe.with_file_name(name);
        if sibling.exists() {
            return Ok(sibling.to_string_lossy().into_owned());
        }
    }
    Err(
        "scrybe-mcp-server not found on PATH. Install with: pip install scrybe-mcp-server"
            .to_string(),
    )
}

// ─── Debug tools ─────────────────────────────────────────────────────────────

/// Return the directory to open on startup.
///
/// If the first CLI argument is a path that exists, return it (canonicalized).
/// Otherwise return None so the frontend falls back to $HOME.
#[tauri::command]
fn get_initial_directory() -> Option<String> {
    let arg = std::env::args().nth(1)?;
    let path = std::path::Path::new(&arg);
    if path.exists() {
        let dir = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()?.to_path_buf()
        };
        dir.canonicalize().ok().map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Return the file to open in a tab on startup.
///
/// If the first CLI argument is an existing file (not a directory), return its
/// canonicalized path so the frontend can open it in a tab. Returns None for
/// directories and missing paths (the sidebar handles those via get_initial_directory).
#[tauri::command]
fn get_initial_file() -> Option<String> {
    let arg = std::env::args().nth(1)?;
    let path = std::path::Path::new(&arg);
    if path.exists() && path.is_file() {
        path.canonicalize().ok().map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Append a log entry from the frontend to the shared debug log file.
///
/// Both this command and the `logs` MCP tool read/write `/tmp/scrybe-debug.log`
/// so Claude Code can tail the app's console output without DevTools.
#[tauri::command]
fn log_append(level: String, message: String) -> Result<(), String> {
    use std::io::Write as _;
    let log_path = std::path::Path::new("/tmp/scrybe-debug.log");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open(log_path)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{ts} [{level}] {message}").map_err(|e| e.to_string())
}

/// Toggle the WebKit/Chromium DevTools inspector window.
/// Only opens the inspector in debug builds; no-op in release.
#[tauri::command]
fn toggle_devtools(window: tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
    #[cfg(not(debug_assertions))]
    let _ = window;
}

// ─── P4.6 — Python plugin loader ────────────────────────────────────────────

/// Enumerate Python plugin files found in `~/.config/scrybe/plugins/`.
///
/// Returns a JSON array of objects with `name`, `path`, and `enabled` fields.
/// Returns an empty array if the directory does not exist or cannot be read.
#[tauri::command]
fn list_plugins() -> Vec<serde_json::Value> {
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
fn run_plugin(path: String, source: String) -> Result<String, String> {
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
        let _ = writeln!(stdin, "{}", source);
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

// ─── P4.11 — Terminal panel IPC ─────────────────────────────────────────────

/// Spawn the user's default shell and store it in the global [`SHELL`] state.
///
/// Subsequent calls replace the previous child (the old process is abandoned;
/// a graceful kill is a follow-up concern).  The shell's stdin/stdout/stderr
/// are piped so the process doesn't inherit the Tauri host's file descriptors.
/// Full bidirectional I/O multiplexing (PTY) is deferred to a follow-up node.
#[tauri::command]
fn terminal_start() -> Result<(), String> {
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
fn terminal_write(data: String) -> Result<(), String> {
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
fn terminal_run(cmd: String) -> Result<String, String> {
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

// ─── P4.8 — VCS IPC commands ─────────────────────────────────────────────────

/// Open a git repository at `path` and store it for subsequent VCS commands.
///
/// Returns a JSON object with `path`, `head` (SHA or null), and `branch` (name
/// or null when HEAD is detached).  Subsequent calls replace the cached repo.
#[tauri::command]
fn vcs_open(path: String) -> Result<serde_json::Value, String> {
    let repo = ScrybeRepo::open(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    let head = repo.head_sha().unwrap_or(None);
    let branch = repo.current_branch().unwrap_or(None);
    VCS.lock().unwrap().repo = Some(repo);
    Ok(serde_json::json!({ "path": path, "head": head, "branch": branch }))
}

/// Returns the git status of the open repository as a JSON array.
///
/// Each element has `path` (relative) and `status` (debug string of
/// [`scrybe_vcs::FileStatus`]).
#[tauri::command]
fn vcs_status() -> Result<Vec<serde_json::Value>, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let entries = repo.status().map_err(|e| e.to_string())?;
    Ok(entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "path": e.path.to_string_lossy(),
                "status": format!("{:?}", e.status),
            })
        })
        .collect())
}

/// Stage all working-tree changes (equivalent to `git add -A`).
#[tauri::command]
fn vcs_stage_all() -> Result<(), String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    repo.stage_all().map_err(|e| e.to_string())
}

/// Create a commit from the current index. Returns the new commit SHA.
#[tauri::command]
fn vcs_commit(
    message: String,
    author_name: String,
    author_email: String,
) -> Result<String, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let author = GitAuthor {
        name: author_name,
        email: author_email,
    };
    repo.commit(&message, &author).map_err(|e| e.to_string())
}

/// Fetch from the named remote (no checkout or merge).
#[tauri::command]
fn vcs_fetch(remote: String) -> Result<(), String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    repo.fetch(&remote).map_err(|e| e.to_string())
}

/// Return the most recent `max` commits starting from HEAD.
///
/// Each element has `sha` (8 chars), `message` (first line), and `author`.
#[tauri::command]
fn vcs_log(max: usize) -> Result<Vec<serde_json::Value>, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let commits = repo.log(max).map_err(|e| e.to_string())?;
    Ok(commits
        .iter()
        .map(|c| {
            serde_json::json!({
                "sha": &c.sha[..8.min(c.sha.len())],
                "message": c.message.lines().next().unwrap_or(""),
                "author": &c.author_name,
            })
        })
        .collect())
}

/// Return the configured remotes with their names, URLs, and inferred roles.
#[tauri::command]
fn vcs_remotes() -> Result<Vec<serde_json::Value>, String> {
    let guard = VCS.lock().unwrap();
    let repo = guard.repo.as_ref().ok_or("no repository open")?;
    let remotes = repo.remotes().map_err(|e| e.to_string())?;
    Ok(remotes
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name,
                "url": r.url,
                "role": format!("{:?}", r.role),
            })
        })
        .collect())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // A second instance was launched. Forward its first path arg to the
            // running frontend so it can open the file in an existing tab.
            if let Some(path) = argv.get(1) {
                let _ = app.emit("scrybe://open", path.clone());
            }
            // Bring the existing window to the front.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            render_markdown,
            get_version,
            list_directory,
            poll_close_tab,
            path_type,
            read_file,
            save_file,
            get_builtin_agents,
            set_agent_enabled,
            list_plugins,
            run_plugin,
            terminal_start,
            terminal_write,
            terminal_run,
            mcp_server_start,
            mcp_server_status,
            mcp_connection_info,
            vcs_open,
            vcs_status,
            vcs_stage_all,
            vcs_commit,
            vcs_fetch,
            vcs_log,
            vcs_remotes,
            toggle_devtools,
            log_append,
            get_initial_directory,
            get_initial_file,
        ])
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running Scrybe");
}
