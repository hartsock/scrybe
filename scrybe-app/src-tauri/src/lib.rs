// SPDX-License-Identifier: Apache-2.0
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
//! - [`plugins::get_builtin_agents`] — enumerate built-in AI agents
//! - [`plugins::set_agent_enabled`] — toggle an agent on/off (in-memory)
//! - [`plugins::list_plugins`]      — enumerate Python plugins from ~/.config/scrybe/plugins/
//! - [`plugins::run_plugin`]        — invoke a Python plugin with document source on stdin
//! - [`terminal::terminal_start`]    — spawn a shell process (P4.11)
//! - [`terminal::terminal_write`]    — write data to shell stdin (P4.11 scaffold)
//! - [`terminal::terminal_run`]      — run a one-shot command and return its output (P4.11)
//! - [`mcp::mcp_server_start`]  — launch scrybe-mcp-server as a stdio sidecar (P4.7)
//! - [`mcp::mcp_server_status`] — report whether the MCP sidecar is running (P4.7)
//! - [`mcp::mcp_connection_info`] — return connection instructions for MCP clients (P4.7)
//! - [`vcs::vcs_open`]          — open a git repository and cache it (P4.8)
//! - [`vcs::vcs_status`]        — return working-tree status entries (P4.8)
//! - [`vcs::vcs_stage_all`]     — stage all changes (`git add -A`) (P4.8)
//! - [`vcs::vcs_commit`]        — create a commit from the staged index (P4.8)
//! - [`vcs::vcs_fetch`]         — fetch from a named remote (P4.8)
//! - [`vcs::vcs_log`]           — return recent commit summaries (P4.8)
//! - [`vcs::vcs_remotes`]       — list configured remotes with roles (P4.8)
//!
//! Full editor integration (P4.2–P4.11) builds on this IPC bridge.

mod cli_rpc;
mod mcp;
mod menu;
mod plugins;
mod terminal;
mod vcs;

use std::collections::{HashMap, HashSet};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{Emitter as _, Manager as _};
use tauri_plugin_shell::ShellExt as _;

// ─── File watcher state ───────────────────────────────────────────────────────

struct WatchState {
    watcher: Option<notify::RecommendedWatcher>,
    /// Paths currently being watched.
    watched: HashSet<String>,
    /// Timestamp of the last autosave per path — used to filter self-writes.
    last_save: HashMap<String, Instant>,
}

impl WatchState {
    fn new() -> Self {
        Self {
            watcher: None,
            watched: HashSet::new(),
            last_save: HashMap::new(),
        }
    }
}

static WATCH: Mutex<Option<WatchState>> = Mutex::new(None);

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

/// Return the application version string baked in at compile time,
/// including the Git commit (and `-dirty` marker) it was built from.
#[tauri::command]
fn get_version() -> &'static str {
    scrybe_core::build_info::VERSION_WITH_COMMIT
}

/// Terminate the app process (#232). The socket `quit` path used to close the
/// main window and trust window-close-exits-app; in practice the close request
/// can leave the process (and the cli_rpc socket) alive — a half-dead app an
/// agent can still drive. `AppHandle::exit` ends the process deterministically
/// after the frontend has run its dirty checks; no process plugin needed.
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    // `exit` skips destructors, so the socket file would linger. A stale
    // socket is harmless (clients type it as not-running via ConnectionRefused,
    // #211) but tidy is better — best-effort unlink before exiting.
    #[cfg(unix)]
    let _ = std::fs::remove_file(scrybe_rpc::default_socket_path());
    app.exit(0);
}

/// Open the OS print dialog (which also offers "Save as PDF") for the active
/// window. WKWebView on macOS silently ignores JavaScript `window.print()`,
/// so the frontend routes here; `WebviewWindow::print()` invokes the native
/// print operation. The `@media print` stylesheet still applies, so only the
/// rendered document prints. Mirrors the toolbar 🖨️ and File ▸ Print.
#[tauri::command]
fn print_document(window: tauri::WebviewWindow) -> Result<(), String> {
    window.print().map_err(|e| e.to_string())
}

// ─── macOS shell-command lifecycle ──────────────────────────────────────────

/// Result emitted by the bundled CLI's idempotent shell-link manager.
///
/// The backend intentionally delegates filesystem ownership decisions to the
/// exact same CLI code used by just install. This prevents the app and
/// developer installer from drifting into two subtly different repair rules.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
struct ShellCommandReport {
    outcome: String,
    command_path: String,
    target_path: Option<String>,
    path_winner: Option<String>,
    other_installations: Vec<String>,
    app_bundles: Vec<String>,
    warnings: Vec<String>,
    message: String,
}

async fn run_shell_command_lifecycle(
    app: &tauri::AppHandle,
    action: &str,
) -> Result<ShellCommandReport, String> {
    let output = app
        .shell()
        .sidecar("scrybe")
        .map_err(|error| format!("bundled scrybe command is unavailable: {error}"))?
        .args(["shell-command", action, "--json"])
        .output()
        .await
        .map_err(|error| format!("could not run the bundled scrybe command: {error}"))?;
    decode_shell_command_report(output.status.success(), &output.stdout, &output.stderr)
}

fn decode_shell_command_report(
    success: bool,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<ShellCommandReport, String> {
    if !success {
        let detail = String::from_utf8_lossy(stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "the bundled scrybe command failed without an error message".to_string()
        } else {
            detail
        });
    }
    serde_json::from_slice(stdout)
        .map_err(|error| format!("bundled scrybe command returned an invalid report: {error}"))
}

/// Install the command or repair a broken/older managed link. Repeated calls
/// are a no-op. Conflicting regular files and unrelated symlinks fail closed.
#[tauri::command]
async fn install_shell_command(app: tauri::AppHandle) -> Result<ShellCommandReport, String> {
    run_shell_command_lifecycle(&app, "install").await
}

/// Inspect the command link without changing the filesystem.
#[tauri::command]
async fn shell_command_status(app: tauri::AppHandle) -> Result<ShellCommandReport, String> {
    run_shell_command_lifecycle(&app, "status").await
}

/// Remove only a link recognized as Scrybe-managed.
#[tauri::command]
async fn uninstall_shell_command(app: tauri::AppHandle) -> Result<ShellCommandReport, String> {
    run_shell_command_lifecycle(&app, "uninstall").await
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

/// Return whether `path` is a `"file"`, `"dir"`, or `"missing"`.
/// Trims trailing slashes so URLs like `file:///foo/bar/` resolve correctly.
#[tauri::command]
fn path_type(path: String) -> &'static str {
    let trimmed = path.trim_end_matches('/');
    let p = std::path::Path::new(trimmed);
    if p.is_dir() {
        "dir"
    } else if p.is_file() {
        "file"
    } else {
        "missing"
    }
}

/// Read the full text content of a file.
#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Path of the autosave sidecar buffer for `path`.
///
/// The sidecar is the "in-progress edit buffer" — autosaves go here so
/// the real file is never touched by debounced keystroke saves. The
/// real file is committed only on explicit save (`save_file`).
fn buffer_path_for(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{path}.scrybe-buffer"))
}

/// Write `content` to `path`, creating `<path>~` on the first save of each
/// session so the original on-disk version is always recoverable. Also
/// clears the `<path>.scrybe-buffer` autosave sidecar (if present), since
/// an explicit save flushes the in-progress edits to the real file.
///
/// The `~` backup is written only once per path: if it already exists it is
/// left alone, preserving the true "opened from disk" snapshot across multiple
/// autosave cycles. The backup is removed when the tab is closed via
/// `remove_backup`.
#[tauri::command]
fn save_file(path: String, content: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let bak = std::path::PathBuf::from(format!("{path}~"));
    if p.exists() && !bak.exists() {
        std::fs::copy(p, &bak).map_err(|e| format!("backup failed: {e}"))?;
    }
    std::fs::write(p, content).map_err(|e| format!("write failed: {e}"))?;
    // Sidecar buffer is now stale — explicit save committed the bytes
    // to the real file. Remove it so future autosaves start fresh and
    // the buffer can't accidentally be restored over a saved file.
    let bp = buffer_path_for(&path);
    if bp.exists() {
        let _ = std::fs::remove_file(&bp);
    }
    Ok(())
}

/// Write the in-progress edit buffer to the autosave sidecar
/// (`<path>.scrybe-buffer`). The real file is never touched by this
/// command — that's the point. The sidecar is cleared by `save_file`
/// (explicit save) and by `clear_buffer` (tab close).
///
/// This is the Phase 2 replacement for the previous autosave path which
/// wrote keystrokes directly to the real file. Writing to a sidecar
/// instead eliminates the entire class of "external edit → fs-watch
/// event → self-write-filter swallows it" bugs by making autosave
/// never write to the watched file.
#[tauri::command]
fn save_buffer(path: String, content: String) -> Result<(), String> {
    let bp = buffer_path_for(&path);
    std::fs::write(&bp, content).map_err(|e| format!("buffer write failed: {e}"))
}

/// Remove the autosave sidecar buffer for `path`. Silent no-op if it
/// doesn't exist. Called on tab close.
#[tauri::command]
fn clear_buffer(path: String) -> Result<(), String> {
    let bp = buffer_path_for(&path);
    if bp.exists() {
        std::fs::remove_file(&bp).map_err(|e| format!("clear buffer failed: {e}"))?;
    }
    Ok(())
}

/// Return the sidecar buffer content if it exists and differs from disk.
///
/// Used on tab open to detect a previous session's unsaved edits — for
/// example, when Scrybe crashed or the tab was closed without an
/// explicit save. Returns None when: no sidecar exists, the sidecar is
/// empty, or the sidecar content matches what's on disk (already
/// saved). The caller decides whether to restore, prompt, or discard.
#[tauri::command]
fn read_buffer_if_exists(path: String) -> Result<Option<String>, String> {
    let bp = buffer_path_for(&path);
    if !bp.exists() {
        return Ok(None);
    }
    let buffer = std::fs::read_to_string(&bp).map_err(|e| e.to_string())?;
    if buffer.is_empty() {
        return Ok(None);
    }
    let real = std::fs::read_to_string(&path).unwrap_or_default();
    if buffer == real {
        Ok(None)
    } else {
        Ok(Some(buffer))
    }
}

/// Remove the `<path>~` backup file when a tab is closed. Also clears
/// the autosave sidecar (`<path>.scrybe-buffer`) so closing a tab
/// always leaves the filesystem clean.
///
/// If the tab was dirty (unsaved edits in the sidecar), those edits are
/// dropped on close. Phase 3 will add a "save changes?" prompt before
/// reaching this command; for Phase 2 we match the existing close
/// behavior of dropping unsaved edits silently.
#[tauri::command]
fn remove_backup(path: String) -> Result<(), String> {
    let bak = std::path::PathBuf::from(format!("{path}~"));
    if bak.exists() {
        std::fs::remove_file(&bak).map_err(|e| format!("remove backup failed: {e}"))?;
    }
    let bp = buffer_path_for(&path);
    if bp.exists() {
        let _ = std::fs::remove_file(&bp);
    }
    Ok(())
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
        dir.canonicalize()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
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
        path.canonicalize()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    }
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

/// Register a file path with the OS file watcher.
/// Call when a tab is opened. Safe to call multiple times for the same path.
#[tauri::command]
fn watch_file(path: String) -> Result<(), String> {
    use notify::Watcher as _;
    let mut guard = WATCH.lock().unwrap();
    if let Some(state) = guard.as_mut() {
        if state.watched.insert(path.clone()) {
            if let Some(w) = state.watcher.as_mut() {
                w.watch(
                    std::path::Path::new(&path),
                    notify::RecursiveMode::NonRecursive,
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Unregister a file path from the OS file watcher.
/// Call when a tab is closed.
#[tauri::command]
fn unwatch_file(path: String) -> Result<(), String> {
    use notify::Watcher as _;
    let mut guard = WATCH.lock().unwrap();
    if let Some(state) = guard.as_mut() {
        if state.watched.remove(&path) {
            if let Some(w) = state.watcher.as_mut() {
                let _ = w.unwatch(std::path::Path::new(&path));
            }
        }
        state.last_save.remove(&path);
    }
    Ok(())
}

/// Record that an autosave just wrote `path` to disk, so the file watcher
/// can ignore the resulting modify event (self-write filtering).
#[tauri::command]
fn note_autosave(path: String) {
    if let Some(state) = WATCH.lock().unwrap().as_mut() {
        state.last_save.insert(path, Instant::now());
    }
}

// ─── UI-control parity: state mirror + MCP signal pollers ────────────────────
//
// Project rule: every human control has an MCP equivalent and vice versa.
// The frontend mirrors its UI state here so the MCP `state` tool can read
// it, and polls the signal files below so the MCP `set_theme`/`view_mode`/
// `set_vim` tools can drive the same controls a human clicks.

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn home_venv_bin(name: &str) -> Option<std::path::PathBuf> {
    let bin_dir = if cfg!(windows) { "Scripts" } else { "bin" };
    dirs::home_dir().map(|home| home.join("venv").join(bin_dir).join(name))
}

fn existing_file(path: std::path::PathBuf) -> Option<String> {
    if path.is_file() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Locate the `scrybe-docx` CLI (from the `scrybe-plugin-docx` package).
fn which_scrybe_docx() -> Result<String, String> {
    if let Ok(path) = std::env::var("SCRYBE_DOCX_BIN") {
        if let Some(path) = existing_file(std::path::PathBuf::from(path)) {
            return Ok(path);
        }
    }

    let name = executable_name("scrybe-docx");
    if let Ok(exe) = std::env::current_exe() {
        if let Some(path) = existing_file(exe.with_file_name(&name)) {
            return Ok(path);
        }
    }
    if let Ok(path) = which::which(&name) {
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Some(path) = home_venv_bin(&name).and_then(existing_file) {
        return Ok(path);
    }

    Err(
        "scrybe-docx not found. Reinstall the Scrybe Python toolkit with docx export support or set SCRYBE_DOCX_BIN to the exporter executable."
            .to_string(),
    )
}

/// Export Markdown `content` to a Word (.docx) file at `output` by shelling
/// to `scrybe-docx`. The buffer is piped on stdin so unsaved edits export
/// too; `scrybe-docx` renders Mermaid fenced blocks to PNGs with the source
/// embedded in PNG metadata. Mirrored by the MCP `export` tool.
#[tauri::command]
fn export_docx(content: String, output: String, no_diagrams: bool) -> Result<String, String> {
    use std::io::Write as _;
    let bin = which_scrybe_docx()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("-o").arg(&output);
    if no_diagrams {
        cmd.arg("--no-diagrams");
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn scrybe-docx: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(content.as_bytes())
            .map_err(|e| format!("write to scrybe-docx stdin failed: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("scrybe-docx failed: {e}"))?;
    if out.status.success() {
        Ok(output)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Export every Mermaid diagram in the active buffer to sibling PNG figures.
///
/// Uses the LIVE buffer `content` (so unsaved edits are included) and the
/// tab's on-disk `path` to place `<stem>_fig_NN.png` siblings next to the
/// document, numbered in document order; each PNG embeds its Mermaid source.
/// Returns the written figure paths. Mirrored by the MCP/CLI `export_figures`
/// tool.
#[tauri::command]
fn export_figures(content: String, path: String) -> Result<Vec<String>, String> {
    let results = scrybe_tools::export_figures(&content, std::path::Path::new(&path))
        .map_err(|e| e.to_string())?;
    Ok(results.into_iter().map(|r| r.path).collect())
}

const DEBUG_FRONTEND_ORIGIN: &str = "http://localhost:5173";

/// Allow only Scrybe's own frontend to occupy the application WebView.
///
/// Preview links are routed to the system browser in TypeScript. This native
/// guard is the non-bypassable fallback for generated markup, redirects, and
/// any future frontend regression: a remote page must never replace the editor.
fn is_trusted_app_navigation(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" if url.host_str() == Some("tauri.localhost") && url.port().is_none() => true,
        "http" if cfg!(debug_assertions) => {
            url.origin().ascii_serialization() == DEBUG_FRONTEND_ORIGIN
        }
        _ => false,
    }
}

fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("scrybe-navigation-guard")
        .on_navigation(|_webview, url| is_trusted_app_navigation(url))
        .build()
}

/// Save the PNG rasterized from the live Mermaid.js preview.
///
/// The frontend owns rendering so the pixels match what the user sees. This
/// command does not render again: it embeds the original Mermaid source into
/// the supplied PNG bytes, then writes (or replaces) the selected destination.
///
/// The image arrives as base64, not `Vec<u8>` — see
/// [`scrybe_tools::write_embedded_png_base64`] for why.
#[tauri::command]
fn save_mermaid_png(output: String, source: String, png_base64: String) -> Result<usize, String> {
    let result = scrybe_tools::write_embedded_png_base64(
        &png_base64,
        &source,
        std::path::Path::new(&output),
    )
    .map_err(|e| e.to_string())?;
    Ok(result.bytes)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(navigation_guard())
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
            quit_app,
            list_directory,
            export_docx,
            export_figures,
            save_mermaid_png,
            watch_file,
            unwatch_file,
            note_autosave,
            path_type,
            read_file,
            save_file,
            save_buffer,
            clear_buffer,
            read_buffer_if_exists,
            remove_backup,
            plugins::get_builtin_agents,
            plugins::set_agent_enabled,
            plugins::list_plugins,
            plugins::run_plugin,
            terminal::terminal_start,
            terminal::terminal_write,
            terminal::terminal_run,
            mcp::mcp_server_start,
            mcp::mcp_server_status,
            mcp::mcp_connection_info,
            vcs::vcs_open,
            vcs::vcs_status,
            vcs::vcs_stage_all,
            vcs::vcs_commit,
            vcs::vcs_fetch,
            vcs::vcs_log,
            vcs::vcs_remotes,
            toggle_devtools,
            get_initial_directory,
            get_initial_file,
            cli_rpc::cli_rpc_reply,
            menu::menu_sync,
            print_document,
            install_shell_command,
            shell_command_status,
            uninstall_shell_command,
        ])
        .menu(menu::build)
        .on_menu_event(|app, event| menu::handle_event(app, &event))
        .setup(|app| {
            use notify::EventKind;

            let handle = app.handle().clone();
            let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

            let mut ws = WatchState::new();
            ws.watcher = Some(
                notify::recommended_watcher(tx)
                    .map_err(|e| format!("file watcher init failed: {e}"))?,
            );
            *WATCH.lock().unwrap() = Some(ws);

            // Background thread: forward external modify events to the frontend.
            // Self-writes (autosave) are filtered out if they occurred < 2 s ago.
            std::thread::spawn(move || {
                for result in rx {
                    let Ok(event) = result else { continue };
                    if !matches!(event.kind, EventKind::Modify(_)) {
                        continue;
                    }
                    for path in &event.paths {
                        let path_str = path.display().to_string();
                        // On macOS the watcher operates at directory granularity, so
                        // we see events for any sibling file (including our own
                        // `.scrybe-buffer` autosave sidecars). Skip anything we
                        // didn't ask to watch — buffer writes must never trigger
                        // a reload of the real file.
                        let is_watched = {
                            let guard = WATCH.lock().unwrap();
                            guard
                                .as_ref()
                                .map(|s| s.watched.contains(&path_str))
                                .unwrap_or(false)
                        };
                        if !is_watched {
                            continue;
                        }
                        // Check and remove self-write record without overlapping borrows.
                        let self_write = {
                            let mut guard = WATCH.lock().unwrap();
                            guard
                                .as_mut()
                                .and_then(|s| s.last_save.get(&path_str))
                                .map(|t| t.elapsed().as_millis() < 2000)
                                .unwrap_or(false)
                        };
                        if self_write {
                            continue;
                        }
                        {
                            let mut guard = WATCH.lock().unwrap();
                            if let Some(s) = guard.as_mut() {
                                s.last_save.remove(&path_str);
                            }
                        }
                        let _ = handle.emit("scrybe://file-changed", &path_str);
                    }
                }
            });

            // CLI ↔ GUI RPC server (Phase 1 — open/save/close/quit).
            // A failure to bind is logged but not fatal: the GUI can still
            // run without the CLI surface. The most common failure is
            // "another Scrybe is already running and owns the socket",
            // which is the right outcome anyway (single-instance plugin
            // also catches this from a different angle).
            match cli_rpc::spawn(app.handle().clone()) {
                Ok(path) => {
                    tracing::info!(socket = %path.display(), "scrybe-cli RPC ready");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "scrybe-cli RPC unavailable");
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Scrybe");
}

#[cfg(test)]
mod tests;
