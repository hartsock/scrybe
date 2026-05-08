// SPDX-License-Identifier: Apache-2.0
import "./styles/tabs.css";
import "./styles/toast.css";
import "./styles/preview.css";
import "./styles/sidebar.css";
import "./styles/mcp_panel.css";
import "./styles/vcs_panel.css";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { showToast } from "./toast";
import { AppState } from "./state";
import { renderTabBar } from "./tabs";
import { createEditor, swapDocument } from "./editor";
import { PreviewPane } from "./preview";
import { buildToolbar } from "./toolbar";
import { Sidebar } from "./sidebar";
import { PluginManager } from "./plugins";
import { McpPanel } from "./mcp_panel";
import { VcsPanel } from "./vcs_panel";

// Forward console output to /tmp/scrybe-debug.log so the scrybe MCP `logs`
// tool can surface errors to Claude Code without needing DevTools open.
function patchConsole() {
  const orig = { error: console.error.bind(console), warn: console.warn.bind(console), log: console.log.bind(console) };
  function fwd(level: string, args: unknown[]) {
    const msg = args.map(a => (a instanceof Error ? a.stack ?? a.message : typeof a === "object" ? JSON.stringify(a) : String(a))).join(" ");
    invoke("log_append", { level, message: msg }).catch(() => {});
  }
  console.error = (...a) => { orig.error(...a); fwd("ERROR", a); };
  console.warn  = (...a) => { orig.warn(...a);  fwd("WARN",  a); };
  console.log   = (...a) => { orig.log(...a);   fwd("LOG",   a); };
}
patchConsole();
invoke<string>("get_version").then(v => console.log("Scrybe version:", v)).catch(console.error);

const WELCOME = `<p align="center"><img src="scrybe-logo.png" alt="Scrybe" width="120" /></p>

# Welcome to Scrybe

**MCP-native Markdown editor.** The document is the conversation —
connect AI agents, preview in real time, and keep full control of your files.

\`\`\`mermaid
graph LR
    You([You]) <-->|reads & edits| Doc([Your document])
    Doc <-->|MCP tools| Scrybe[Scrybe]
    Scrybe <-->|open · read · edit| Agent1([Claude])
    Scrybe <-->|open · read · edit| Agent2([Codex])
    Scrybe <-->|open · read · edit| Agent3([Ollama])
\`\`\`

## Get started

- **Open a file** — click in the sidebar, or run \`scrybe path/to/file.md\`
- **Links navigate** — relative links open in a new tab; directories load in the sidebar
- **Autosave** — edits save automatically; the original is preserved as \`.bak\`
- **View modes** — click \`◧◨\` on any tab to toggle editor / preview / both

## Connect an AI agent

\`\`\`bash
pip install scrybe-mcp-server
claude mcp add scrybe -- scrybe-mcp-server stdio
\`\`\`

Tools: \`open\` · \`read\` · \`edit\` · \`find\` · \`render\` · \`lint\` · \`close_tab\`
`;
const pluginManager = new PluginManager();
pluginManager.load();

const state = new AppState();
const tabBarEl = document.getElementById("tab-bar")!;
const editorEl = document.getElementById("editor")!;
const previewEl = document.getElementById("preview")!;
const toolbarEl = document.getElementById("toolbar")!;
const sidebarEl = document.getElementById("sidebar")!;

const preview = new PreviewPane(previewEl);

function flashSidebar(dir: string): void {
  sidebar.loadDirectory(dir);
  sidebarEl.classList.add("sidebar-folder-flash");
  sidebarEl.addEventListener("animationend",
    () => sidebarEl.classList.remove("sidebar-folder-flash"), { once: true });
}

// Link navigation: directories load in the sidebar, files open in a new tab,
// http(s) goes to the system browser, missing paths show a toast.
previewEl.addEventListener("scrybe:open-link", async (e: Event) => {
  const href = (e as CustomEvent<{ href: string }>).detail.href;
  if (/^https?:\/\//.test(href)) {
    openExternal(href).catch(console.error);
    return;
  }
  let resolved: string;
  if (href.startsWith("/")) {
    resolved = href;
  } else {
    const tab = state.activeTab();
    const dir = tab?.path
      ? tab.path.substring(0, tab.path.lastIndexOf("/"))
      : "";
    resolved = dir ? new URL(href, `file://${dir}/`).pathname : href;
  }
  const kind = await invoke<string>("path_type", { path: resolved }).catch(() => "missing");
  if (kind === "file") {
    openFileByPath(resolved);
  } else if (kind === "dir") {
    flashSidebar(resolved);
  } else {
    // File missing — fall back to the parent directory if it exists.
    const parent = resolved.replace(/\/[^/]*\/?$/, "");
    const parentKind = parent
      ? await invoke<string>("path_type", { path: parent }).catch(() => "missing")
      : "missing";
    const label = href.split("/").pop() ?? href;
    if (parentKind === "dir") {
      flashSidebar(parent);
      showToast(`${label} doesn't exist`, "info");
    } else {
      showToast(`${label} doesn't exist`);
    }
  }
});

// ─── P4.8 — VCS panel (constructed before Sidebar so loadDirectory can open it)

const vcsContainer = document.createElement("div");
vcsContainer.id = "vcs-panel-container";
const vcsPanel = new VcsPanel(vcsContainer);

// Sidebar — file browser + agent registration panel
const sidebar = new Sidebar(sidebarEl, async (path) => {
  try {
    const content: string = await invoke("read_file", { path });
    state.addTab(path, content);
    swapDocument(view, content);
    preview.render(content);
    redrawTabs();
  } catch (err) {
    console.error("Failed to open file:", err);
  }
}, async (dirPath) => {
  // When a folder is opened, also attempt to open as a git repo.
  vcsPanel.openRepo(dirPath).catch(() => { /* not a git repo — panel shows hint */ });
});


// MCP panel — shown in sidebar agents tab (future: dedicated MCP tab)
const mcpContainer = document.createElement("div");
mcpContainer.id = "mcp-panel-container";
sidebarEl.appendChild(mcpContainer);
const mcpPanel = new McpPanel(mcpContainer);
mcpPanel.refresh();

// Append VCS panel below MCP panel in the sidebar
sidebarEl.appendChild(vcsContainer);

/// Save the active tab to disk now (manual / Ctrl+S / toolbar 💾).
/// Bypasses the autosave debounce timer; takes effect immediately.
async function saveActiveTabNow(): Promise<void> {
  const tab = state.activeTab();
  if (!tab?.path) {
    showToast("No file to save", "info");
    return;
  }
  try {
    await invoke("save_file", { path: tab.path, content: tab.content });
    invoke("note_autosave", { path: tab.path }).catch(() => {});
    state.markClean(tab.id);
    redrawTabs();
    showToast(`Saved ${tab.path.split("/").pop()}`, "info");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    showToast(`Save failed: ${msg}`);
  }
}

/// Force-reload the active tab from disk (manual / Ctrl+R / toolbar 🔄).
/// If the buffer has unsaved changes, routes through the existing
/// conflict-bar (Keep mine / Take theirs) instead of clobbering them.
async function reloadActiveTabNow(): Promise<void> {
  const tab = state.activeTab();
  if (!tab?.path) {
    showToast("No file to reload", "info");
    return;
  }
  await reloadTabFromDisk(tab.path);
}

buildToolbar(toolbarEl, {
  onThemeChange: (theme) => {
    preview.setTheme(theme);
    const tab = state.activeTab();
    if (tab) preview.render(tab.content);
  },
  onTogglePreview: () => {
    previewEl.style.display = previewEl.style.display === "none" ? "" : "none";
  },
  onOpenFile: openFileByPath,
  onOpenFolder: (path) => sidebar.loadDirectory(path),
  onSave: () => { void saveActiveTabNow(); },
  onReload: () => { void reloadActiveTabNow(); },
});

// Keyboard shortcuts for save + reload (mirrors the toolbar buttons).
window.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  if (e.key === "s" || e.key === "S") {
    e.preventDefault();
    void saveActiveTabNow();
  } else if (e.key === "r" || e.key === "R") {
    // Block the default reload (which would refresh the whole webview)
    // and instead reload the active tab's disk content.
    e.preventDefault();
    void reloadActiveTabNow();
  }
});

let saveTimer: ReturnType<typeof setTimeout> | null = null;

const view = createEditor(editorEl, WELCOME, async (content) => {
  if (state.activeTabId) {
    state.updateContent(state.activeTabId, content);
    redrawTabs();
  }
  // Capture tab BEFORE any awaits — prevents a race where onChange(WELCOME)
  // completes after onChange(file) and re-schedules a save with stale content.
  const tab = state.activeTab();
  const savePath = tab?.path ?? null;
  const saveId   = tab?.id   ?? null;

  const processed = await pluginManager.runAll(content);
  preview.render(processed || content);

  // Autosave: 1 s debounce. At fire time, re-read content from state so
  // the saved bytes are always the latest keystrokes, not a stale closure.
  if (savePath && saveId) {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      const current = state.tabs.find(t => t.id === saveId);
      if (!current?.path) return;
      console.log("Autosave:", current.path);
      try {
        await invoke("save_file", { path: current.path, content: current.content });
        invoke("note_autosave", { path: current.path }).catch(() => {});
        state.markClean(saveId);
        redrawTabs();
      } catch (err) {
        console.error("Autosave failed:", err);
      }
    }, 1000);
  }
});

const editorAndPreview = document.getElementById("editor-and-preview")!;

function applyViewMode(mode: string): void {
  editorAndPreview.classList.remove("mode-both", "mode-edit", "mode-preview");
  editorAndPreview.classList.add(`mode-${mode}`);
}

function redrawTabs(): void {
  renderTabBar(tabBarEl, state,
    id => selectTab(id),
    id => closeTab(id),
    () => newTab(),
    id => { applyViewMode(state.cycleViewMode(id)); redrawTabs(); },
  );
}

function selectTab(id: string): void {
  state.activeTabId = id;
  const tab = state.activeTab();
  if (tab) {
    swapDocument(view, tab.content);
    preview.render(tab.content);
    applyViewMode(tab.viewMode);
  }
  redrawTabs();
}

function newTab(content = WELCOME): void {
  state.addTab(null, content);
  swapDocument(view, content);
  preview.render(content);
  redrawTabs();
}

function closeTab(id: string): void {
  const tab = state.tabs.find(t => t.id === id);
  if (tab?.path) {
    invoke("remove_backup", { path: tab.path }).catch(() => {});
    invoke("unwatch_file", { path: tab.path }).catch(() => {});
  }
  state.closeTab(id);
  const active = state.activeTab();
  const content = active?.content ?? "";
  swapDocument(view, content);
  if (content) preview.render(content);
  redrawTabs();
}

function extOf(path: string): string {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

async function openFileByPath(path: string): Promise<void> {
  try {
    const ext = extOf(path);

    // PNG — binary file, render as image directly
    if (ext === "png" || ext === "jpg" || ext === "jpeg" || ext === "gif" || ext === "webp") {
      const existing = state.tabs.find(t => t.path === path);
      if (existing) {
        state.activeTabId = existing.id;
        preview.renderImage(convertFileSrc(path));
        redrawTabs();
        return;
      }
      state.addTab(path, "");
      swapDocument(view, "");
      preview.renderImage(convertFileSrc(path));
      redrawTabs();
      return;
    }

    const existing = state.tabs.find(t => t.path === path);
    if (existing) {
      state.activeTabId = existing.id;
      swapDocument(view, existing.content);
      preview.render(ext === "mmd" ? "```mermaid\n" + existing.content + "\n```" : existing.content);
      redrawTabs();
      return;
    }

    const content: string = await invoke("read_file", { path });
    state.addTab(path, content);
    invoke("watch_file", { path }).catch(() => {});
    swapDocument(view, content);
    preview.render(ext === "mmd" ? "```mermaid\n" + content + "\n```" : content);
    redrawTabs();
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    const label = path.split("/").pop() ?? path;
    if (msg.includes("os error 2") || msg.includes("No such file") || msg.includes("not found")) {
      showToast(`Not found: ${label}`);
    } else {
      showToast(`Could not open ${label}: ${msg}`);
      console.error("Failed to open file:", err);
    }
  }
}

document.addEventListener("keydown", e => {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key === "n") { e.preventDefault(); newTab(); }
  if (mod && e.key === "w") { e.preventDefault(); if (state.activeTabId) closeTab(state.activeTabId); }
  if (mod && e.key === "o") { e.preventDefault(); document.getElementById("open-file")?.click(); }
});

// ─── P4.11 — terminal panel disabled (re-enable when MCP session control is wired) ───

newTab();
invoke<string | null>("get_initial_directory").then(dir =>
  dir ? sidebar.loadDirectory(dir) : homeDir().then(home => sidebar.loadDirectory(home))
).catch(console.error);
invoke<string | null>("get_initial_file").then(file => {
  console.log("get_initial_file:", file);
  if (file) openFileByPath(file);
}).catch(err => console.error("get_initial_file failed:", err));

// When a second `scrybe open <path>` is run, the single-instance plugin
// forwards the path here instead of spawning a new window.
listen<string>("scrybe://open", event => {
  openFileByPath(event.payload);
}).catch(console.error);

// Poll for close_tab signals from the MCP server (500 ms).
function closeTabByPath(path: string): void {
  if (!path) {
    if (state.activeTabId) closeTab(state.activeTabId);
  } else {
    const tab = state.tabs.find(t => t.path === path);
    if (tab) closeTab(tab.id);
  }
}
setInterval(async () => {
  const path = await invoke<string | null>("poll_close_tab").catch(() => null);
  if (path !== null) closeTabByPath(path);
}, 500);

// ─── Reload: MCP-driven (poll) + OS file watcher (event) ─────────────────────

async function reloadTabFromDisk(path: string): Promise<void> {
  const tab = state.tabs.find(t => t.path === path);
  if (!tab) return;
  const content = await invoke<string>("read_file", { path }).catch(() => "");
  if (!content) return;
  if (tab.isDirty) {
    // Dirty buffer — show conflict bar instead of silently clobbering
    showConflict(path, content);
  } else {
    // Clean buffer — auto-reload and toast
    state.updateContent(tab.id, content);
    state.markClean(tab.id);
    if (state.activeTabId === tab.id) swapDocument(view, content);
    preview.render(content);
    redrawTabs();
    showToast(`Reloaded ${path.split("/").pop()} (changed externally)`, "info");
  }
}

// Conflict bar — shown when an external edit arrives on a dirty buffer
let conflictPath: string | null = null;
let conflictDiskContent: string = "";

function showConflict(path: string, diskContent: string): void {
  conflictPath = path;
  conflictDiskContent = diskContent;
  const bar = document.getElementById("conflict-bar")!;
  const name = path.split("/").pop() ?? path;
  bar.innerHTML = `
    <span class="conflict-msg">⚠ <strong>${name}</strong> changed on disk — your buffer has unsaved edits.</span>
    <button id="conflict-keep">Keep mine</button>
    <button id="conflict-take">Take theirs</button>
  `;
  bar.style.display = "flex";
  document.getElementById("conflict-keep")!.onclick = () => dismissConflict();
  document.getElementById("conflict-take")!.onclick = () => applyConflictDisk();
}

function dismissConflict(): void {
  conflictPath = null;
  conflictDiskContent = "";
  document.getElementById("conflict-bar")!.style.display = "none";
}

function applyConflictDisk(): void {
  const path = conflictPath;
  const content = conflictDiskContent;
  dismissConflict();
  if (!path) return;
  const tab = state.tabs.find(t => t.path === path);
  if (!tab) return;
  state.updateContent(tab.id, content);
  state.markClean(tab.id);
  if (state.activeTabId === tab.id) swapDocument(view, content);
  preview.render(content);
  redrawTabs();
}

// MCP reload poll
setInterval(async () => {
  const path = await invoke<string | null>("poll_reload_tab").catch(() => null);
  if (path !== null) await reloadTabFromDisk(path);
}, 500);

// OS file watcher events
listen<string>("scrybe://file-changed", async event => {
  await reloadTabFromDisk(event.payload);
}).catch(console.error);

// ─── CLI ↔ GUI RPC events (Phase 1) ──────────────────────────────────────────
//
// Emitted by the Rust socket server in `src-tauri/src/cli_rpc.rs`. Each event
// payload carries a canonicalized path the Rust side already resolved.

// `scrybe foo.md` (or `scrybe open foo.md`) — open or refresh.
// "One tab, one file": if the path is already open, refresh from disk
// instead of duplicating. Reuses the same code path as the file watcher
// for consistency.
listen<string>("scrybe://cli-open", async event => {
  const path = event.payload;
  const existing = state.tabs.find(t => t.path === path);
  if (existing) {
    state.activeTabId = existing.id;
    await reloadTabFromDisk(path);
    redrawTabs();
  } else {
    openFileByPath(path);
  }
}).catch(console.error);

// `scrybe save foo.md` — save if open, else silent no-op.
listen<string>("scrybe://cli-save", async event => {
  const tab = state.tabs.find(t => t.path === event.payload);
  if (!tab) return; // not open: no-op per design
  try {
    await invoke("save_file", { path: tab.path, content: tab.content });
    invoke("note_autosave", { path: tab.path }).catch(() => {});
    state.markClean(tab.id);
    redrawTabs();
  } catch (err) {
    console.error("scrybe://cli-save failed:", err);
  }
}).catch(console.error);

// `scrybe close foo.md` — close if open, else silent no-op.
listen<string>("scrybe://cli-close", event => {
  const tab = state.tabs.find(t => t.path === event.payload);
  if (tab) closeTab(tab.id);
}).catch(console.error);

// `scrybe quit [--force]` — quit the app; `force=true` skips dirty prompts.
listen<boolean>("scrybe://cli-quit", async event => {
  const force = event.payload === true;
  if (!force) {
    const dirty = state.tabs.filter(t => t.isDirty);
    if (dirty.length > 0) {
      const names = dirty.map(t => t.path?.split("/").pop() ?? "(untitled)").join(", ");
      const proceed = window.confirm(
        `Unsaved changes in: ${names}. Quit anyway? (use \`scrybe quit --force\` to skip this prompt)`,
      );
      if (!proceed) return;
    }
  }
  // Tauri 2: closing the main window quits the app. The exit() plugin is the
  // strongest hammer but pulls in the process plugin; a window close keeps
  // dependency surface small and matches the standard menu Quit path.
  const win = (await import("@tauri-apps/api/window")).getCurrentWindow();
  await win.close();
}).catch(console.error);
