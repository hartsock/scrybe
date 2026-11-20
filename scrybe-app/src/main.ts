// SPDX-License-Identifier: Apache-2.0
import "./styles/tabs.css";
import "./styles/toast.css";
import "./styles/preview.css";
import "./styles/sidebar.css";
import "./styles/mcp_panel.css";
import "./styles/vcs_panel.css";
import "./styles/pathbar.css";
import "./styles/grid.css";
import "./styles/print.css";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { showToast } from "./toast";
import { AppState, isViewMode, viewModeFor } from "./state";
import type { TabEntry } from "./state";
import type { DocInfo, LanguageHint } from "./kinds";
import { renderTabBar } from "./tabs";
import { createCodeMirrorSurface, knownLanguage, languageNames } from "./codemirror-surface";
import { LazySurface, WYSIWYG_CAPABILITIES, pickSurface } from "./surfaces";
import { createGridSurface } from "./grid-surface";
import type { Surface, SurfaceChange } from "./surface";
import { PreviewPane } from "./preview";
import type { MermaidPreviewDocument } from "./mermaid_png";
import { buildToolbar, setToolbarViewMode, setToolbarTheme, setToolbarVim, setToolbarWrap, setToolbarSurfaceControls, setToolbarExplorer } from "./toolbar";
import type { Theme } from "./toolbar";
import { renderPathBar } from "./pathbar";
import { Sidebar } from "./sidebar";
import { PluginManager } from "./plugins";
import { McpPanel } from "./mcp_panel";
import { VcsPanel } from "./vcs_panel";
import { patchConsole } from "./console-log";
import { WELCOME } from "./welcome";
import { showModal } from "./modal";
import { createExporters } from "./export-doc";
import { registerMermaidExport } from "./mermaid-export";
import { registerLinkNavigation } from "./link-nav";
import { createConflictBar } from "./conflict";
import { registerMenu } from "./menu";
import { registerRpcTabs } from "./rpc-tabs";
import { registerRpcContent } from "./rpc-content";
import { registerRpcUi } from "./rpc-ui";

patchConsole();
invoke<string>("get_version").then(v => console.log("Scrybe version:", v)).catch(console.error);

const pluginManager = new PluginManager();
pluginManager.load();

const state = new AppState();
const tabBarEl = document.getElementById("tab-bar")!;
const pathBarEl = document.getElementById("path-bar")!;
const editorEl = document.getElementById("editor")!;
const wysiwygEl = document.getElementById("editor-wysiwyg")!;
const gridEl = document.getElementById("editor-grid")!;
const previewEl = document.getElementById("preview")!;
const toolbarEl = document.getElementById("toolbar")!;
const sidebarEl = document.getElementById("sidebar")!;
const contentEl = document.getElementById("content")!;
const editorColEl = document.getElementById("editor-col")!;

const preview = new PreviewPane(previewEl);
let previewRequestGeneration = 0;

registerMermaidExport(previewEl);


// Current editor/preview theme and Vim state. These are app-wide (not
// per-tab) and are mirrored to the MCP `state` tool via `publishState`.
let currentTheme: Theme = "default";
let vimEnabled = false;
let wrapEnabled = true;

// Whether the Explorer sidebar is showing. App-wide, remembered across
// launches on this machine (a viewing convenience, so local storage is
// enough; it falls back to showing when storage is unavailable).
const EXPLORER_PREF = "scrybe.explorerVisible";
let explorerVisible = (() => {
  try { return localStorage.getItem(EXPLORER_PREF) !== "false"; } catch { return true; }
})();

/// Show or hide the Explorer sidebar. The single entry point for the toolbar
/// ☰ button, the header « button, View ▸ Show Explorer (⇧⌘E), and the MCP
/// `set_explorer` tool.
function setExplorerVisible(on: boolean): void {
  explorerVisible = on;
  contentEl.classList.toggle("explorer-hidden", !on);
  setToolbarExplorer(toolbarEl, on);
  try { localStorage.setItem(EXPLORER_PREF, String(on)); } catch { /* not remembered */ }
  publishState();
}

function flashSidebar(dir: string): void {
  // Navigating to a folder shows it, so reveal a hidden Explorer.
  if (!explorerVisible) setExplorerVisible(true);
  sidebar.loadDirectory(dir);
  sidebarEl.classList.add("sidebar-folder-flash");
  sidebarEl.addEventListener("animationend",
    () => sidebarEl.classList.remove("sidebar-folder-flash"), { once: true });
}

registerLinkNavigation({ previewEl, state, openFileByPath, flashSidebar });

// ─── P4.8 — VCS panel (constructed before Sidebar so loadDirectory can open it)

const vcsContainer = document.createElement("div");
vcsContainer.id = "vcs-panel-container";
const vcsPanel = new VcsPanel(vcsContainer);

// Sidebar — file browser + agent registration panel
const sidebar = new Sidebar(sidebarEl, async (path) => {
  // Route through openFileByPath so sidebar opens get the same
  // language/kind classification, image/.mmd handling, and buffer-restore
  // as every other open path.
  await openFileByPath(path);
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
  onOpenFolder: (path) => flashSidebar(path),
  onSave: () => { void saveActiveTabNow(); },
  onReload: () => { void reloadActiveTabNow(); },
  onExport: () => { void exportActiveTabToWord(); },
  onPrint: () => { void printActiveTab(); },
  onToggleVim: () => toggleVim(),
  onToggleWrap: () => toggleWrap(),
  onToggleExplorer: () => setExplorerVisible(!explorerVisible),
});
document.getElementById("explorer-hide")!.addEventListener("click", () => setExplorerVisible(false));
document.getElementById("empty-new")!.addEventListener("click", () => newTab());
document.getElementById("empty-open")!.addEventListener("click", () => document.getElementById("open-file")?.click());

/// Print the active tab. Render its current buffer into the preview first — so
/// printing works even from editor-only view and always reflects unsaved edits —
/// let async KaTeX/Mermaid rendering settle, then open the OS print dialog
/// (which also offers "Save as PDF"). styles/print.css shows only the rendered
/// document, laid out for paper. Mirrored by the ⌘P/Ctrl+P shortcut.
async function printActiveTab(): Promise<void> {
  const tab = state.activeTab();
  if (!tab) return;
  // Print renders the Markdown preview; a code/text tab has none.
  if (tab.kind !== "markdown") {
    showToast("Printing is available for Markdown documents", "info");
    return;
  }
  const request = beginPreviewRequest(tab.id, tab.path);
  await preview.render(previewSource(tab, tab.content), request.document);
  // KaTeX/Mermaid post-processing is async; give it a beat before the snapshot.
  await new Promise((r) => setTimeout(r, 250));
  // WKWebView ignores JS window.print(); route to the native print dialog.
  try {
    await invoke("print_document");
  } catch (err) {
    showToast(`Print failed: ${err instanceof Error ? err.message : err}`);
  }
}

/// Apply a theme to BOTH panes so the editor chrome matches the preview,
/// update the toolbar dropdown, and mirror the choice to the MCP `state`
/// tool. This is the single entry point for theme changes — the toolbar
/// dropdown and the MCP `set_theme` poller both route through here.
function applyTheme(theme: Theme): void {
  currentTheme = theme;
  preview.setTheme(theme);
  source.setTheme(theme);
  wysiwyg.setTheme(theme);
  grid.setTheme(theme);
  setToolbarTheme(toolbarEl, theme);
  const tab = state.activeTab();
  if (tab) renderPreview(tab.content);
  publishState();
}

/// Cycle the active tab's view mode (both → edit → preview → wysiwyg → both). Shared
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
  // A code/text tab has no rendered preview — it stays edit-only regardless of
  // what an agent requests, mirroring the guard in state.cycleViewMode so the
  // MCP concrete-mode path can't flip a source file into a blank preview. A
  // table has its grid (`wysiwyg`) and its raw text (`edit`).
  if (tab.kind !== "markdown") {
    tab.viewMode = viewModeFor(tab.kind, isViewMode(mode) ? mode : "edit");
    applyViewMode(tab.viewMode);
    redrawTabs();
    return;
  }
  if (isViewMode(mode)) {
    tab.viewMode = mode;
    applyViewMode(mode);
    redrawTabs();
  }
}

/// Enable/disable Vim in the editor, update the toolbar button, mirror to
/// MCP. Shared by the toolbar toggle and the MCP `set_vim` poller.
function setVimEnabled(on: boolean): void {
  vimEnabled = on;
  source.setVim(on);
  setToolbarVim(toolbarEl, vimApplied());
  publishState();
}

/// Vim and wrap are app-wide preferences, but only some surfaces honour them.
/// These are the values actually in effect for the active tab — what the
/// toolbar, the native menu, and the MCP `state` / `set_vim` replies report.
function vimApplied(): boolean {
  return vimEnabled && surface.capabilities.vim;
}
function wrapApplied(): boolean {
  return wrapEnabled && surface.capabilities.wrap;
}

/// Human toggles (toolbar + menu). The preference cannot be changed while the
/// active surface does not support it, so a hidden change never surprises the
/// user when they return to the source editor.
function toggleVim(): void {
  if (!surface.capabilities.vim) {
    showToast("Vim keybindings are not available in WYSIWYG view", "info");
    return;
  }
  setVimEnabled(!vimEnabled);
}
function toggleWrap(): void {
  if (!surface.capabilities.wrap) {
    showToast("Word wrap is always on in WYSIWYG view", "info");
    return;
  }
  setWrapEnabled(!wrapEnabled);
}

/// Re-point the Vim/Wrap controls at the active surface: disabled when it
/// does not support them, and showing the value actually in effect.
function syncSurfaceControls(): void {
  setToolbarSurfaceControls(toolbarEl, surface.capabilities);
  setToolbarVim(toolbarEl, vimApplied());
  setToolbarWrap(toolbarEl, wrapApplied());
}

/// Enable/disable soft word-wrap in the editor, update the toolbar button,
/// mirror to MCP. Shared by the toolbar toggle and the MCP `set_wrap` poller.
function setWrapEnabled(on: boolean): void {
  wrapEnabled = on;
  source.setWrap(on);
  setToolbarWrap(toolbarEl, wrapApplied());
  publishState();
}

/// Snapshot the current UI state — what the human is looking at (active
/// path, view mode, theme, vim). Served live over the socket `state` method
/// (A2: replaces the `/tmp/scrybe-state.json` mirror — no file, no staleness).
function uiStateSnapshot(): Record<string, unknown> {
  const tab = state.activeTab();
  return {
    active_path: tab?.path ?? null,
    active_title: tab?.title ?? null,
    is_dirty: tab?.isDirty ?? false,
    view_mode: tab?.viewMode ?? "both",
    doc_kind: tab?.kind ?? "markdown",
    language: tab?.language ?? null,
    theme: currentTheme,
    vim: vimApplied(),
    wrap: wrapApplied(),
    explorer: explorerVisible,
    open_paths: state.tabs.map(t => t.path).filter((p): p is string => !!p),
  };
}

/// Keep the native menu's check items (theme radio, Vim, Wrap) in sync so
/// menu, toolbar, and agents can never disagree for long. Fires per keystroke
/// (via redrawTabs), so skip the IPC unless one of the values changed.
let lastMenuSync = "";
function publishState(): void {
  const vim = vimApplied();
  const wrap = wrapApplied();
  const menuState = `${currentTheme}|${vim}|${wrap}|${explorerVisible}`;
  if (menuState !== lastMenuSync) {
    lastMenuSync = menuState;
    invoke("menu_sync", { theme: currentTheme, vim, wrap, explorer: explorerVisible })
      .catch(() => { /* menu mirror is best-effort */ });
  }
}

/// Update the selectable path bar to show the active tab's full path, and
/// re-publish the MCP state mirror so the `state` tool tracks tab opens and
/// switches in real time (not just theme/view/vim changes). Called from
/// `redrawTabs`, which fires on every tab mutation.
function updatePathBar(): void {
  const badge = surface.reformatPending?.()
    ? {
        text: "Reformat pending",
        title: surface === source
          ? "This file has CR or CRLF line endings. Editing in the text view " +
            "converts every line ending to LF; the grid and WYSIWYG views do not. " +
            "Nothing changes until you edit."
          : "This document's Markdown is not in Scrybe's house style. " +
            "Your first edit in WYSIWYG view will also rewrite its formatting " +
            "(list markers, spacing, escapes). Nothing changes until you edit.",
      }
    : undefined;
  const tab = state.activeTab();
  const language = tab && {
    label: tab.language ?? "Plain Text",
    auto: tab.languageOverride === null,
    names: LANGUAGE_NAMES,
    onPick: (choice: string) => {
      if (!setActiveLanguage(choice)) showToast(`Unknown language: ${choice}`);
    },
  };
  renderPathBar(pathBarEl, tab, badge, language);
  publishState();
}

const LANGUAGE_NAMES = languageNames();

/// What a tab's language is detected from, beyond its file name.
function languageHint(tab: TabEntry): LanguageHint {
  return { firstLine: firstLineOf(tab.content), override: tab.languageOverride };
}

function firstLineOf(content: string): string {
  const end = content.indexOf("\n");
  return end === -1 ? content : content.slice(0, end);
}

/// Set the active tab's language by name, or `auto` to return to detection.
/// The single entry point for the path-bar language chip and the MCP
/// `set_language` tool. Returns null (and changes nothing) for an unknown
/// language name.
function setActiveLanguage(choice: string): DocInfo | null {
  const tab = state.activeTab();
  if (!tab) return null;
  let override: string | null = null;
  if (choice.trim().toLowerCase() !== "auto") {
    const known = knownLanguage(choice);
    if (!known) return null;
    override = known.name;
  }
  tab.languageOverride = override;
  const info = source.classify(tab.path, languageHint(tab));
  const wasTable = tab.kind === "table";
  tab.kind = info.kind;
  tab.language = info.language;
  // Only Markdown has a preview or a WYSIWYG view; a table has its grid,
  // which a tab newly classified as a table opens in.
  if (tab.kind !== "markdown") {
    tab.viewMode = tab.kind === "table" && !wasTable ? "wysiwyg" : viewModeFor(tab.kind, tab.viewMode);
  }
  applyViewMode(tab.viewMode);
  void surface.setLanguage(tab.path, languageHint(tab));
  void renderPreview(tab.content);
  redrawTabs();
  return info;
}

const { exportActiveTabToWord, exportActiveTabFigures } = createExporters({ state });

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

// The one place the concrete editors are named. `source` (CodeMirror) backs
// the both / edit / preview view modes and every code or text tab; `wysiwyg`
// (Milkdown) backs the wysiwyg view mode and is fetched on first use.
// `surface` is whichever of the two the active tab is showing.
const onSurfaceChange = (owner: () => Surface) => ({ content }: SurfaceChange): void => {
  // Only the surface on screen speaks for the active tab.
  if (surface !== owner()) return;
  if (state.activeTabId) {
    state.updateContent(state.activeTabId, content);
    redrawTabs();
  }
  const tab = state.activeTab();
  const savePath = tab?.path ?? null;
  const saveId   = tab?.id   ?? null;

  void renderPreview(content);

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
};
const source: Surface = createCodeMirrorSurface(editorEl, WELCOME, onSurfaceChange(() => source));
const wysiwyg: LazySurface = new LazySurface(
  wysiwygEl,
  () => import("./wysiwyg-surface").then(m => m.createMilkdownSurface),
  onSurfaceChange(() => wysiwyg),
  WYSIWYG_CAPABILITIES,
  (path, hint) => source.classify(path, hint),
);
const grid: Surface = createGridSurface(gridEl, "", onSurfaceChange(() => grid), (path, hint) => source.classify(path, hint));
let surface: Surface = source;

const editorAndPreview = document.getElementById("editor-and-preview")!;

/// Lay out the panes for `mode` and make the matching surface the active one.
/// When that swaps surfaces on the SAME tab (a view-mode change), the incoming
/// surface is loaded with the tab's buffer. Callers that are switching tabs
/// pass `loadOnSwitch = false` and set the content themselves right after.
function applyViewMode(mode: string, loadOnSwitch = true): void {
  editorAndPreview.classList.remove("mode-both", "mode-edit", "mode-preview", "mode-wysiwyg");
  editorAndPreview.classList.add(`mode-${mode}`);
  const picked = pickSurface(mode, state.activeTab()?.kind ?? "markdown");
  // The grid shares the wysiwyg mode class; this one tells the CSS which pane.
  editorAndPreview.classList.toggle("is-grid", picked === "grid");
  activateSurface(picked === "grid" ? grid : picked === "wysiwyg" ? wysiwyg : source, loadOnSwitch);
  setToolbarViewMode(toolbarEl, mode, state.activeTab()?.kind);
  syncSurfaceControls();
  publishState();
}

function activateSurface(next: Surface, loadContent: boolean): void {
  if (next === surface) return;
  // `tab.content` is always current: every user edit in either surface
  // reaches it synchronously through `onSurfaceChange`.
  if (loadContent) stashEditor();
  surface = next;
  if (loadContent) loadEditor(state.activeTab());
  if (next !== wysiwyg) return;
  wysiwyg.ensure().then(
    () => {
      if (surface !== wysiwyg) return;
      updatePathBar(); // the "Reformat pending" badge is known only once mounted
      wysiwyg.focus();
    },
    (err: unknown) => {
      console.error("WYSIWYG editor failed to load:", err);
      showToast("Could not load the WYSIWYG editor");
      const tab = state.activeTab();
      if (tab?.viewMode === "wysiwyg") {
        tab.viewMode = "edit";
        applyViewMode("edit");
        redrawTabs();
      }
    },
  );
}

/// Render `content` into the preview pane — but ONLY for Markdown documents.
/// Code/text tabs open edit-only and have no rendered preview, so this is a
/// no-op for them; that stops source files from being parsed as Markdown into
/// garbled HTML. The single gate every render-on-load/edit site routes through.
function beginPreviewRequest(id: string, path: string | null): {
  generation: number;
  document: MermaidPreviewDocument;
} {
  return {
    generation: ++previewRequestGeneration,
    document: { id, path },
  };
}

/// The Markdown to render for a tab's buffer. A `.mmd` file is bare Mermaid
/// source, shown as one diagram.
function previewSource(tab: TabEntry, content: string): string {
  return tab.path && extOf(tab.path) === "mmd" ? "```mermaid\n" + content + "\n```" : content;
}

async function renderPreview(content: string): Promise<void> {
  const tab = state.activeTab();
  if (!tab) {
    preview.clear();
    return;
  }
  if (tab.kind === "markdown") {
    const request = beginPreviewRequest(tab.id, tab.path);
    const markdown = previewSource(tab, content);
    const processed = await pluginManager.runAll(markdown);
    // A newer render or tab switch must not publish stale plugin output.
    if (request.generation !== previewRequestGeneration ||
        request.document.id !== state.activeTabId) return;
    await preview.render(processed || markdown, request.document);
  }
}

// Each tab keeps its own editing session in the source editor (undo history,
// cursor, selection, scroll), set aside while another tab is showing. A
// session is only put back if the tab's text is still what it was when the
// session was set aside — an agent edit or a reload in the meantime drops it.
const editorSessions = new Map<string, { content: string; saved: unknown }>();

/// Set aside the active tab's editing session. Call BEFORE the active tab
/// changes or the source editor is swapped for another surface.
function stashEditor(): void {
  const tab = state.activeTab();
  if (tab && surface === source && source.saveState) {
    editorSessions.set(tab.id, { content: tab.content, saved: source.saveState() });
  }
}

/// Put `tab`'s buffer on the active surface, with its editing session when
/// one is still valid, and highlight it for its language.
function loadEditor(tab: TabEntry | undefined): void {
  const content = tab?.content ?? "";
  const session = tab && editorSessions.get(tab.id);
  if (session && session.content === content && surface === source && source.restoreState) {
    source.restoreState(session.saved);
  } else {
    surface.setContent(content);
  }
  void surface.setLanguage(tab?.path ?? null, tab && languageHint(tab));
}

/// Show the active tab: its view mode decides which surface is on screen,
/// then the buffer, language, preview and tab bar follow. Callers that are
/// leaving another tab call `stashEditor()` first.
function showActiveTab(): void {
  const tab = state.activeTab();
  applyViewMode(tab?.viewMode ?? "both", false);
  loadEditor(tab);
  void renderPreview(tab?.content ?? "");
  redrawTabs();
}

function redrawTabs(): void {
  // With no tab, show the empty screen: an editor with no tab behind it
  // would take typing and then lose it. Every tab change passes through here.
  editorColEl.classList.toggle("no-tabs", state.tabs.length === 0);
  renderTabBar(tabBarEl, state,
    id => selectTab(id),
    id => closeTab(id),
    () => newTab(),
    id => {
      // The mode icon works on any tab, but only the active tab is on screen.
      const mode = state.cycleViewMode(id);
      if (id === state.activeTabId) applyViewMode(mode);
      redrawTabs();
    },
  );
  updatePathBar();
}

function selectTab(id: string): void {
  stashEditor();
  state.activeTabId = id;
  showActiveTab();
}

function newTab(content = WELCOME): void {
  stashEditor();
  state.addTab(null, content); // untitled → Markdown
  showActiveTab();
}


/// Tear down a tab without prompting. Split out from `closeTab` so the
/// save-on-close prompt can await the user's choice before invoking it.
function doCloseTab(id: string): void {
  const tab = state.tabs.find(t => t.id === id);
  if (tab?.path) {
    invoke("remove_backup", { path: tab.path }).catch(() => {});
    invoke("unwatch_file", { path: tab.path }).catch(() => {});
  }
  // Closing a background tab leaves the active one on screen; keep its
  // session current so re-showing it below loses nothing.
  if (id !== state.activeTabId) stashEditor();
  editorSessions.delete(id);
  state.closeTab(id);
  showActiveTab();
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
      // An image tab has an empty buffer; addTab re-activates one already open.
      stashEditor();
      state.addTab(path, "");
      const tab = state.activeTab();
      applyViewMode(tab?.viewMode ?? "both", false);
      loadEditor(tab);
      preview.renderImage(convertFileSrc(path));
      redrawTabs();
      return;
    }

    const existing = state.tabs.find(t => t.path === path);
    if (existing) {
      selectTab(existing.id);
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

    // Kind and language come from the file name, else a `#!` first line.
    // Code/text open edit-only.
    const { kind, language } = source.classify(path, { firstLine: firstLineOf(useContent) });
    stashEditor();
    const newTabId = state.addTab(path, useContent, kind, language);
    invoke("watch_file", { path }).catch(() => {});
    // Restored buffer differs from disk → flag the tab dirty so the user
    // gets the dirty indicator and the save-on-close prompt next time.
    if (restoredFromBuffer) state.markDirty(newTabId);
    showActiveTab();
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
// Word wrap defaults on — apply it to the editor (not just the toolbar), so
// long lines wrap to the pane instead of scrolling off-screen out of the box.
source.setWrap(wrapEnabled);
syncSurfaceControls();
setExplorerVisible(explorerVisible);
invoke<string | null>("get_initial_directory").then(dir =>
  dir ? sidebar.loadDirectory(dir) : homeDir().then(home => sidebar.loadDirectory(home))
).catch(console.error);
invoke<string | null>("get_initial_file").then(file => {
  console.log("get_initial_file:", file);
  if (file) openFileByPath(file);
}).catch(err => console.error("get_initial_file failed:", err));

registerMenu({
  state, newTab, saveActiveTabNow, reloadActiveTabNow, exportActiveTabToWord,
  exportActiveTabFigures, printActiveTab, closeTab, cyclePreviewMode, setViewMode,
  applyTheme, toggleVim, toggleWrap, setExplorerVisible,
  isExplorerVisible: () => explorerVisible,
});



// When a second `scrybe open <path>` is run, the single-instance plugin
// forwards the path here instead of spawning a new window.
listen<string>("scrybe://open", event => {
  openFileByPath(event.payload);
}).catch(console.error);

// A2: the /tmp signal-file pollers (close-tab, theme, view-mode, vim, wrap)
// are gone — agents drive every UI control through typed socket methods
// (`scrybe://cli-*` listeners below), the same code paths the human toolbar
// uses. No predictable /tmp channels, no 500 ms polling loops.

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
    if (state.activeTabId === tab.id) {
      surface.setContent(content);
      renderPreview(content);
    }
    redrawTabs();
    showToast(`Reloaded ${path.split("/").pop()} (changed externally)`, "info");
  }
}


const { showConflict } = createConflictBar({
  state, getSurface: () => surface, renderPreview, redrawTabs,
});

// OS file watcher events
listen<string>("scrybe://file-changed", async event => {
  await reloadTabFromDisk(event.payload);
}).catch(console.error);

registerRpcTabs({
  state, selectTab, reloadTabFromDisk, openFileByPath, closeTab, redrawTabs,
  getSurface: () => surface, renderPreview,
});
registerRpcContent({ state, redrawTabs, getSurface: () => surface, renderPreview });
registerRpcUi({
  state, applyTheme, cyclePreviewMode, setViewMode, setVimEnabled, vimApplied,
  setActiveLanguage, setExplorerVisible, isExplorerVisible: () => explorerVisible,
  uiStateSnapshot, getSurface: () => surface,
});
