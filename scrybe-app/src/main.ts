// SPDX-License-Identifier: Apache-2.0
import "./styles/tabs.css";
import "./styles/toast.css";
import "./styles/preview.css";
import "./styles/sidebar.css";
import "./styles/mcp_panel.css";
import "./styles/vcs_panel.css";
import "./styles/pathbar.css";
import "./styles/print.css";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { showToast } from "./toast";
import { AppState } from "./state";
import { renderTabBar } from "./tabs";
import { createEditor, swapDocument, shouldSuppressAutosave, setEditorTheme, setVim, setWrap } from "./editor";
import { PreviewPane } from "./preview";
import { buildToolbar, setToolbarViewMode, setToolbarTheme, setToolbarVim, setToolbarWrap } from "./toolbar";
import type { Theme } from "./toolbar";
import { renderPathBar } from "./pathbar";
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
const pathBarEl = document.getElementById("path-bar")!;
const editorEl = document.getElementById("editor")!;
const previewEl = document.getElementById("preview")!;
const toolbarEl = document.getElementById("toolbar")!;
const sidebarEl = document.getElementById("sidebar")!;

const preview = new PreviewPane(previewEl);

// Current editor/preview theme and Vim state. These are app-wide (not
// per-tab) and are mirrored to the MCP `state` tool via `publishState`.
let currentTheme: Theme = "default";
let vimEnabled = false;
let wrapEnabled = false;

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
  onThemeChange: (theme) => applyTheme(theme),
  onCyclePreview: () => cyclePreviewMode(),
  onOpenFile: openFileByPath,
  onOpenFolder: (path) => sidebar.loadDirectory(path),
  onSave: () => { void saveActiveTabNow(); },
  onReload: () => { void reloadActiveTabNow(); },
  onExport: () => { void exportActiveTabToWord(); },
  onPrint: () => { void printActiveTab(); },
  onToggleVim: () => setVimEnabled(!vimEnabled),
  onToggleWrap: () => setWrapEnabled(!wrapEnabled),
});

/// Print the active tab. Render its current buffer into the preview first — so
/// printing works even from editor-only view and always reflects unsaved edits —
/// let async KaTeX/Mermaid rendering settle, then open the OS print dialog
/// (which also offers "Save as PDF"). styles/print.css shows only the rendered
/// document, laid out for paper. Mirrored by the ⌘P/Ctrl+P shortcut.
async function printActiveTab(): Promise<void> {
  const tab = state.activeTab();
  if (!tab) return;
  await preview.render(tab.content);
  // KaTeX/Mermaid post-processing is async; give it a beat before the snapshot.
  await new Promise((r) => setTimeout(r, 250));
  window.print();
}

/// Apply a theme to BOTH panes so the editor chrome matches the preview,
/// update the toolbar dropdown, and mirror the choice to the MCP `state`
/// tool. This is the single entry point for theme changes — the toolbar
/// dropdown and the MCP `set_theme` poller both route through here.
function applyTheme(theme: Theme): void {
  currentTheme = theme;
  preview.setTheme(theme);
  setEditorTheme(view, theme);
  setToolbarTheme(toolbarEl, theme);
  const tab = state.activeTab();
  if (tab) preview.render(tab.content);
  publishState();
}

/// Cycle the active tab's view mode (both → edit → preview → both). Shared
/// by the toolbar View button and the MCP `view_mode` poller.
function cyclePreviewMode(): void {
  const id = state.activeTabId;
  if (!id) return;
  applyViewMode(state.cycleViewMode(id));
  redrawTabs();
}

/// Set a specific view mode on the active tab (used by the MCP poller when
/// a concrete mode, not "cycle", is requested).
function setViewMode(mode: string): void {
  const tab = state.activeTab();
  if (!tab) return;
  if (mode === "both" || mode === "edit" || mode === "preview") {
    tab.viewMode = mode;
    applyViewMode(mode);
    redrawTabs();
  }
}

/// Enable/disable Vim in the editor, update the toolbar button, mirror to
/// MCP. Shared by the toolbar toggle and the MCP `set_vim` poller.
function setVimEnabled(on: boolean): void {
  vimEnabled = on;
  setVim(view, on);
  setToolbarVim(toolbarEl, on);
  publishState();
}

/// Enable/disable soft word-wrap in the editor, update the toolbar button,
/// mirror to MCP. Shared by the toolbar toggle and the MCP `set_wrap` poller.
function setWrapEnabled(on: boolean): void {
  wrapEnabled = on;
  setWrap(view, on);
  setToolbarWrap(toolbarEl, on);
  publishState();
}

/// Publish the current UI state to `/tmp/scrybe-state.json` so the MCP
/// `state` tool can report what the human is looking at (active path,
/// view mode, theme, vim). The human-side equivalents are the path bar,
/// the tab mode icon, the theme dropdown, and the Vim toggle.
let lastMenuSync = "";
function publishState(): void {
  const tab = state.activeTab();
  invoke("publish_state", {
    state: {
      active_path: tab?.path ?? null,
      active_title: tab?.title ?? null,
      is_dirty: tab?.isDirty ?? false,
      view_mode: tab?.viewMode ?? "both",
      theme: currentTheme,
      vim: vimEnabled,
      wrap: wrapEnabled,
      open_paths: state.tabs.map(t => t.path).filter((p): p is string => !!p),
    },
  }).catch(() => { /* state mirror is best-effort */ });
  // Mirror the same state onto the native menu's check items (theme radio,
  // Vim, Wrap) so menu, toolbar, and MCP can never disagree for long.
  // publishState fires per keystroke (via redrawTabs), so skip the IPC and
  // the five native menu mutations unless one of the three values changed.
  const menuState = `${currentTheme}|${vimEnabled}|${wrapEnabled}`;
  if (menuState !== lastMenuSync) {
    lastMenuSync = menuState;
    invoke("menu_sync", { theme: currentTheme, vim: vimEnabled, wrap: wrapEnabled })
      .catch(() => { /* menu mirror is best-effort */ });
  }
}

/// Update the selectable path bar to show the active tab's full path, and
/// re-publish the MCP state mirror so the `state` tool tracks tab opens and
/// switches in real time (not just theme/view/vim changes). Called from
/// `redrawTabs`, which fires on every tab mutation.
function updatePathBar(): void {
  renderPathBar(pathBarEl, state.activeTab());
  publishState();
}

/// Export the active tab's current buffer to a Word (.docx) file. Prompts
/// for a destination, then shells to `scrybe-docx` via the backend. The
/// MCP `export` tool is the agent-side equivalent.
async function exportActiveTabToWord(): Promise<void> {
  const tab = state.activeTab();
  if (!tab) { showToast("No tab to export", "info"); return; }
  const base = tab.path
    ? tab.path.replace(/\.[^/.]+$/, "").split("/").pop() ?? "document"
    : "document";
  const home = await homeDir();
  const dest = await saveDialog({
    defaultPath: `${home}/${base}.docx`,
    filters: [{ name: "Word document", extensions: ["docx"] }],
  });
  if (!dest) return;
  try {
    await invoke("export_docx", { content: tab.content, output: dest, noDiagrams: false });
    showToast(`Exported ${dest.split("/").pop()}`, "info");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    showToast(`Export failed: ${msg}`);
  }
}

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
  } else if (e.key === "p" || e.key === "P") {
    // Intercept the default print so we can render the current buffer to the
    // preview first (styles/print.css then prints only the rendered document).
    e.preventDefault();
    void printActiveTab();
  }
});

let saveTimer: ReturnType<typeof setTimeout> | null = null;

const view = createEditor(editorEl, WELCOME, async (content) => {
  // Snapshot the flag synchronously — `shouldSuppressAutosave()` returns
  // true only while CodeMirror is mid-dispatch from `swapDocument`. By
  // the time we hit our first `await` below, the flag will have been
  // reset, so capture it now.
  const isProgrammaticLoad = shouldSuppressAutosave();

  if (state.activeTabId) {
    state.updateContent(state.activeTabId, content);
    // A programmatic load (file open, tab switch, external-change reload)
    // is not a user edit. Setting isDirty=true would (a) show a misleading
    // dirty badge, and (b) chain into the autosave path below. Mark the
    // tab clean to undo the dirty-flip that `updateContent` always sets.
    if (isProgrammaticLoad) state.markClean(state.activeTabId);
    redrawTabs();
  }
  // Capture tab BEFORE any awaits — prevents a race where onChange(WELCOME)
  // completes after onChange(file) and re-schedules a save with stale content.
  const tab = state.activeTab();
  const savePath = tab?.path ?? null;
  const saveId   = tab?.id   ?? null;

  const processed = await pluginManager.runAll(content);
  preview.render(processed || content);

  // Programmatic loads must not schedule an autosave: doing so would
  // immediately call `note_autosave()` and open a 2 s self-write window
  // in the OS file watcher, swallowing any genuine external edit that
  // lands inside that window. See `editor.ts::swapDocument`.
  if (isProgrammaticLoad) return;

  // Autosave: 1 s debounce, writes to the `<path>.scrybe-buffer` sidecar
  // (not the real file). At fire time, re-read content from state so the
  // saved bytes are always the latest keystrokes, not a stale closure.
  //
  // The tab stays `isDirty: true` until an explicit save (Ctrl+S / 💾)
  // flushes the buffer to the real file. Sidecar writes do not change
  // dirty state because the real file is still out of sync.
  if (savePath && saveId) {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      const current = state.tabs.find(t => t.id === saveId);
      if (!current?.path) return;
      console.log("Autosave to buffer:", current.path);
      try {
        await invoke("save_buffer", { path: current.path, content: current.content });
        // No `note_autosave` here — buffer writes don't touch the real
        // file, so the OS fs-watch never sees them. The self-write
        // filter only protects explicit `save_file` calls.
        // No `markClean` either — tab stays dirty until explicit save.
      } catch (err) {
        console.error("Buffer autosave failed:", err);
      }
    }, 1000);
  }
});

const editorAndPreview = document.getElementById("editor-and-preview")!;

function applyViewMode(mode: string): void {
  editorAndPreview.classList.remove("mode-both", "mode-edit", "mode-preview");
  editorAndPreview.classList.add(`mode-${mode}`);
  setToolbarViewMode(toolbarEl, mode);
  publishState();
}

function redrawTabs(): void {
  renderTabBar(tabBarEl, state,
    id => selectTab(id),
    id => closeTab(id),
    () => newTab(),
    id => { applyViewMode(state.cycleViewMode(id)); redrawTabs(); },
  );
  updatePathBar();
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

/// 3-button modal: returns "primary" | "secondary" | "cancel".
///
/// The dialog is opaque to its callers — they pick the labels and act
/// on the return value. Used for save-on-close ("Save" / "Discard")
/// and restore-on-open ("Restore" / "Discard"). Enter selects primary,
/// Escape returns cancel.
type ModalChoice = "primary" | "secondary" | "cancel";

function showModal(
  title: string,
  message: string,
  primaryLabel: string,
  secondaryLabel: string,
): Promise<ModalChoice> {
  return new Promise<ModalChoice>(resolve => {
    const overlay = document.getElementById("modal-overlay")!;
    const titleEl = document.getElementById("modal-title")!;
    const msgEl = document.getElementById("modal-message")!;
    const primary = document.getElementById("modal-primary") as HTMLButtonElement;
    const secondary = document.getElementById("modal-secondary") as HTMLButtonElement;
    const cancel = document.getElementById("modal-cancel") as HTMLButtonElement;

    titleEl.textContent = title;
    msgEl.textContent = message;
    primary.textContent = primaryLabel;
    secondary.textContent = secondaryLabel;
    overlay.style.display = "flex";
    primary.focus();

    const cleanup = () => {
      overlay.style.display = "none";
      primary.onclick = null;
      secondary.onclick = null;
      cancel.onclick = null;
      document.removeEventListener("keydown", onKey);
    };
    const finish = (choice: ModalChoice) => { cleanup(); resolve(choice); };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); finish("cancel"); }
      else if (e.key === "Enter") { e.preventDefault(); finish("primary"); }
    };
    primary.onclick = () => finish("primary");
    secondary.onclick = () => finish("secondary");
    cancel.onclick = () => finish("cancel");
    document.addEventListener("keydown", onKey);
  });
}

/// Tear down a tab without prompting. Split out from `closeTab` so the
/// save-on-close prompt can await the user's choice before invoking it.
function doCloseTab(id: string): void {
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

function closeTab(id: string): void {
  const tab = state.tabs.find(t => t.id === id);
  if (!tab) return;

  // Dirty buffer → prompt before closing. The sidecar still has the
  // edits, but `remove_backup` (called from doCloseTab) deletes it on
  // close, so dropping out without saving really does lose work.
  if (tab.isDirty && tab.path) {
    const name = tab.path.split("/").pop() ?? "(untitled)";
    const path = tab.path;
    void showModal(
      "Unsaved changes",
      `Save changes to "${name}" before closing?`,
      "Save",      // primary
      "Discard",   // secondary
    ).then(async choice => {
      if (choice === "cancel") return; // keep the tab open
      if (choice === "primary") {
        try {
          await invoke("save_file", { path, content: tab.content });
          invoke("note_autosave", { path }).catch(() => {});
        } catch (err) {
          showToast(`Save failed: ${err instanceof Error ? err.message : err}`);
          return; // bail — don't close on save failure
        }
      }
      // "primary" (Save succeeded) or "secondary" (Discard) — close
      doCloseTab(id);
    });
    return;
  }

  doCloseTab(id);
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

    const diskContent: string = await invoke("read_file", { path });

    // Check for an unsaved sidecar buffer left over from a prior session
    // (crash, force-close, or "Discard" not actually discarding). If the
    // buffer differs from disk, prompt the user to restore or discard.
    // The backend's `read_buffer_if_exists` returns null when the buffer
    // is missing, empty, or already matches disk.
    const buffer: string | null = await invoke<string | null>(
      "read_buffer_if_exists",
      { path },
    ).catch(() => null);

    let useContent = diskContent;
    let restoredFromBuffer = false;
    if (buffer !== null) {
      const name = path.split("/").pop() ?? path;
      const choice = await showModal(
        "Restore unsaved edits?",
        `An autosave buffer for "${name}" exists from a previous session.\n\n` +
          `Restore the unsaved edits, or discard them and open the file as-is on disk?`,
        "Restore",
        "Discard",
      );
      if (choice === "cancel") return; // user backed out of the open
      if (choice === "primary") {
        useContent = buffer;
        restoredFromBuffer = true;
      } else {
        // Discard — clear the sidecar so the prompt doesn't keep coming back
        invoke("clear_buffer", { path }).catch(() => {});
      }
    }

    const newTabId = state.addTab(path, useContent);
    invoke("watch_file", { path }).catch(() => {});
    swapDocument(view, useContent);
    preview.render(ext === "mmd" ? "```mermaid\n" + useContent + "\n```" : useContent);
    // Restored buffer differs from disk → flag the tab dirty so the user
    // gets the dirty indicator and the save-on-close prompt next time.
    if (restoredFromBuffer) state.markDirty(newTabId);
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
// Sync the toolbar widgets + MCP state mirror with the initial defaults.
applyViewMode(state.activeTab()?.viewMode ?? "both");
setToolbarTheme(toolbarEl, currentTheme);
setToolbarVim(toolbarEl, vimEnabled);
setToolbarWrap(toolbarEl, wrapEnabled);
invoke<string | null>("get_initial_directory").then(dir =>
  dir ? sidebar.loadDirectory(dir) : homeDir().then(home => sidebar.loadDirectory(home))
).catch(console.error);
invoke<string | null>("get_initial_file").then(file => {
  console.log("get_initial_file:", file);
  if (file) openFileByPath(file);
}).catch(err => console.error("get_initial_file failed:", err));

// Native menu bar (#184). Each item id routes to the same single-entry-point
// function its toolbar/keyboard twin uses, so the human ↔ MCP parity rule
// holds with no new tools. Predefined items (Edit menu, quit, …) are handled
// by the OS and never arrive here.
listen<string>("scrybe://menu", event => {
  switch (event.payload) {
    case "new_tab":     newTab(); break;
    case "open_file":   document.getElementById("open-file")?.click(); break;
    case "open_folder": document.getElementById("open-folder")?.click(); break;
    case "save":        void saveActiveTabNow(); break;
    case "reload":      void reloadActiveTabNow(); break;
    case "export_docx": void exportActiveTabToWord(); break;
    case "print":       void printActiveTab(); break;
    case "close_tab":
      // macOS convention: ⌘W closes the tab, and falls through to closing
      // the window when there is no tab left to close.
      if (state.activeTabId) {
        closeTab(state.activeTabId);
      } else {
        void import("@tauri-apps/api/window").then(w => w.getCurrentWindow().close());
      }
      break;
    case "cycle_view":  cyclePreviewMode(); break;
    case "theme_default":   applyTheme("default"); break;
    case "theme_dark":      applyTheme("dark"); break;
    case "theme_solarized": applyTheme("solarized"); break;
    case "toggle_vim":  setVimEnabled(!vimEnabled); break;
    case "toggle_wrap": setWrapEnabled(!wrapEnabled); break;
    case "close_window":
      void import("@tauri-apps/api/window").then(w => w.getCurrentWindow().close());
      break;
    default: console.warn("unknown menu action:", event.payload);
  }
}).catch(console.error);

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

// ─── MCP control pollers (human ↔ MCP parity) ────────────────────────────────
//
// The MCP server writes a signal file for each UI control it drives; the
// frontend polls and applies it through the same code path as the human
// toolbar control. See `scrybe-mcp-server/src/tools.rs`.
setInterval(async () => {
  const theme = await invoke<string | null>("poll_set_theme").catch(() => null);
  if (theme && (theme === "default" || theme === "dark" || theme === "solarized")) {
    applyTheme(theme);
  }
  const mode = await invoke<string | null>("poll_view_mode").catch(() => null);
  if (mode === "cycle") cyclePreviewMode();
  else if (mode) setViewMode(mode);
  const vim = await invoke<string | null>("poll_set_vim").catch(() => null);
  if (vim === "on") setVimEnabled(true);
  else if (vim === "off") setVimEnabled(false);
  const wrap = await invoke<string | null>("poll_set_wrap").catch(() => null);
  if (wrap === "on") setWrapEnabled(true);
  else if (wrap === "off") setWrapEnabled(false);
}, 500);

// ─── Reload: MCP-driven (poll) + OS file watcher (event) ─────────────────────

async function reloadTabFromDisk(path: string): Promise<void> {
  const tab = state.tabs.find(t => t.path === path);
  if (!tab) return;
  const content = await invoke<string>("read_file", { path }).catch(() => "");
  if (!content) return;
  // Nothing to do when disk already matches the buffer. Guards against
  // spurious/coalesced fs-watch events and MCP `open`-refresh of an
  // already-open tab from silently clobbering an in-flight MCP `edit`
  // (whose in-memory change hasn't been written to disk). See #140.
  if (content === tab.content) return;
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
// instead of duplicating. Request-with-reply so the caller (CLI/MCP) blocks
// until the tab is actually open and gets the real tab_id — this removes the
// open→edit race where a fire-and-forget open let a follow-up hit "not open"
// (#141). The `reply` id is only present on the reply-based (Phase 2) path.
listen<{ id: number; data: string }>("scrybe://cli-open", async event => {
  const { id, data: path } = event.payload;
  try {
    const existing = state.tabs.find(t => t.path === path);
    let reloaded = false;
    if (existing) {
      state.activeTabId = existing.id;
      await reloadTabFromDisk(path);
      redrawTabs();
      reloaded = true;
    } else {
      await openFileByPath(path);
    }
    await reply(id, { result: { tab_id: path, reloaded } });
  } catch (err) {
    await reply(id, {
      error: { code: -32603, message: `open failed: ${err instanceof Error ? err.message : err}` },
    });
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

// ─── Phase 2: read-side commands with reply correlation ────────────────────
//
// These events carry an `id` field that the server is blocking on. The
// frontend handler calls `cli_rpc_reply(id, {result|error})` to unblock
// the dispatcher, which packages the payload into a JSON-RPC response.

interface ReplyOk { result: unknown }
interface ReplyErr { error: { code: number; message: string } }

async function reply(id: number, body: ReplyOk | ReplyErr): Promise<void> {
  await invoke("cli_rpc_reply", { id, reply: body });
}

const ERR_TAB_NOT_OPEN = -32001;
const ERR_SECTION_NOT_FOUND = -32004;

// `scrybe read <path>` — return the in-memory buffer (which may differ
// from disk if there are unsaved edits).
listen<{ id: number; data: { path: string } }>("scrybe://cli-read", async event => {
  const { id, data } = event.payload;
  const tab = state.tabs.find(t => t.path === data.path);
  if (!tab) {
    await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` } });
    return;
  }
  await reply(id, {
    result: { path: tab.path, content: tab.content, is_dirty: tab.isDirty },
  });
}).catch(console.error);

// `scrybe tabs` / MCP `list_tabs` — the live set of open tabs (#46). No params;
// enumerate our tab state into `TabInfo`s and reply.
listen<{ id: number; data: unknown }>("scrybe://cli-list-tabs", async event => {
  const { id } = event.payload;
  const tabs = state.tabs.map(t => ({
    path: t.path ?? "",
    title: t.title,
    is_dirty: t.isDirty,
    view_mode: t.viewMode,
    active: t.id === state.activeTabId,
  }));
  await reply(id, { result: { tabs } });
}).catch(console.error);

// `scrybe reload <path>` / MCP `reload` — re-read an open tab from disk into its
// live buffer (first-class socket op, retiring the /tmp/scrybe-reload-tab.txt poll).
listen<{ id: number; data: { path: string; force: boolean } }>("scrybe://cli-reload", async event => {
  const { id, data } = event.payload;
  const tab = state.tabs.find(t => t.path === data.path);
  if (!tab) {
    await reply(id, { error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` } });
    return;
  }
  const wasDirty = tab.isDirty;
  if (wasDirty && !data.force) {
    await reply(id, { error: { code: -32005, message: `unsaved edits in ${data.path} — pass force to discard` } });
    return;
  }
  try {
    const content = await invoke<string>("read_file", { path: data.path });
    state.updateContent(tab.id, content);
    state.markClean(tab.id);
    if (state.activeTabId === tab.id) {
      swapDocument(view, content);
      preview.render(content);
    }
    redrawTabs();
    const bytes = new TextEncoder().encode(content).length;
    await reply(id, { result: { path: data.path, bytes, was_dirty: wasDirty } });
  } catch (err) {
    await reply(id, { error: { code: -32603, message: `reload failed: ${err instanceof Error ? err.message : err}` } });
  }
}).catch(console.error);

// `scrybe find <pattern> [paths...]` — regex/literal grep across open tabs
// (or named paths, falling back to disk for non-open ones).
interface FindRequest {
  pattern: string;
  paths: string[];
  literal: boolean;
  case_sensitive: boolean;
}

listen<{ id: number; data: FindRequest }>("scrybe://cli-find", async event => {
  const { id, data } = event.payload;
  let regex: RegExp;
  try {
    const pattern = data.literal ? data.pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") : data.pattern;
    const flags = data.case_sensitive ? "g" : "gi";
    regex = new RegExp(pattern, flags);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    await reply(id, { error: { code: -32602, message: `invalid pattern: ${msg}` } });
    return;
  }

  // If paths is empty, search every open tab. Otherwise, for each named
  // path: use the open buffer if any, else read from disk.
  type Hit = { path: string; line: number; column: number; text: string };
  const hits: Hit[] = [];
  const sources: { path: string; content: string }[] = [];

  if (data.paths.length === 0) {
    for (const t of state.tabs) {
      if (t.path) sources.push({ path: t.path, content: t.content });
    }
  } else {
    for (const p of data.paths) {
      const tab = state.tabs.find(t => t.path === p);
      if (tab) {
        sources.push({ path: p, content: tab.content });
      } else {
        try {
          const disk = await invoke<string>("read_file", { path: p });
          sources.push({ path: p, content: disk });
        } catch {
          // skip silently — non-existent paths just contribute zero hits
        }
      }
    }
  }

  for (const src of sources) {
    const lines = src.content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      regex.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = regex.exec(lines[i])) !== null) {
        hits.push({ path: src.path, line: i + 1, column: m.index + 1, text: lines[i] });
        // Avoid zero-width-match infinite loops.
        if (m.index === regex.lastIndex) regex.lastIndex++;
      }
    }
  }

  await reply(id, { result: { hits } });
}).catch(console.error);

// `scrybe section <path> --heading <h>` — extract a section by heading.
// Heading match is case-insensitive substring; section runs from the
// matched heading to the next heading of the same or shallower level.
listen<{ id: number; data: { path: string; heading: string } }>(
  "scrybe://cli-section",
  async event => {
    const { id, data } = event.payload;
    const tab = state.tabs.find(t => t.path === data.path);
    if (!tab) {
      await reply(id, {
        error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` },
      });
      return;
    }

    const lines = tab.content.split("\n");
    const headingRe = /^(#{1,6})\s+(.*)$/;
    const needle = data.heading.toLowerCase();

    let startIdx = -1;
    let level = 0;
    let actualHeading = "";
    for (let i = 0; i < lines.length; i++) {
      const m = headingRe.exec(lines[i]);
      if (m && m[2].toLowerCase().includes(needle)) {
        startIdx = i;
        level = m[1].length;
        actualHeading = m[2];
        break;
      }
    }
    if (startIdx === -1) {
      await reply(id, {
        error: {
          code: ERR_SECTION_NOT_FOUND,
          message: `no heading matching '${data.heading}' in ${data.path}`,
        },
      });
      return;
    }
    let endIdx = lines.length;
    for (let i = startIdx + 1; i < lines.length; i++) {
      const m = headingRe.exec(lines[i]);
      if (m && m[1].length <= level) {
        endIdx = i;
        break;
      }
    }
    const sectionContent = lines.slice(startIdx, endIdx).join("\n");
    await reply(id, {
      result: { heading: actualHeading, level, content: sectionContent },
    });
  },
).catch(console.error);

// `scrybe edit <path> --start-line N --end-line M --content '...'` — replace lines.
listen<{ id: number; data: { path: string; start_line: number; end_line: number; content: string } }>(
  "scrybe://cli-edit",
  async event => {
    const { id, data } = event.payload;
    const tab = state.tabs.find(t => t.path === data.path);
    if (!tab) {
      await reply(id, {
        error: { code: ERR_TAB_NOT_OPEN, message: `not open: ${data.path}` },
      });
      return;
    }
    const lines = tab.content.split("\n");
    if (data.start_line < 1 || data.end_line > lines.length || data.end_line < data.start_line) {
      await reply(id, {
        error: {
          code: -32602,
          message: `invalid line range ${data.start_line}..${data.end_line} for buffer with ${lines.length} lines`,
        },
      });
      return;
    }
    // Replace lines [start_line-1 .. end_line-1] (inclusive) with the new content.
    // Content may contain its own newlines; we preserve them as-is.
    const newLines = data.content.split("\n");
    const before = lines.slice(0, data.start_line - 1);
    const after = lines.slice(data.end_line);
    const merged = [...before, ...newLines, ...after].join("\n");

    state.updateContent(tab.id, merged);
    if (state.activeTabId === tab.id) {
      swapDocument(view, merged);
      preview.render(merged);
    }
    redrawTabs();
    await reply(id, { result: { applied: true, size_after: merged.length } });
  },
).catch(console.error);
