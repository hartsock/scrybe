// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";

export type Theme = "default" | "dark" | "solarized";

export interface ToolbarHandlers {
  onThemeChange: (theme: Theme) => void;
  /// Cycle the active tab's view mode (both → edit → preview → both),
  /// mirroring the per-tab mode icon in the tab bar.
  onCyclePreview: () => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
  /// Save the active tab to disk. No-op when there is no active tab or
  /// it has no path yet (e.g. an unsaved scratch buffer).
  onSave: () => void;
  /// Reload the active tab from disk. If the buffer is dirty, the
  /// existing conflict-bar flow takes over (Keep mine / Take theirs).
  onReload: () => void;
  /// Export the active tab to a Word (.docx) document.
  onExport: () => void;
  /// Toggle the Vim keymap in the editor on/off.
  onToggleVim: () => void;
}

export function buildToolbar(container: HTMLElement, handlers: ToolbarHandlers): void {
  const btn = "background:transparent;border:1px solid #666;color:#ccc;padding:2px 8px;border-radius:3px;font-size:12px;cursor:pointer;";
  container.innerHTML = `
    <span style="font-weight:600;letter-spacing:0.5px;">Scrybe</span>
    <div style="margin-left:auto;display:flex;align-items:center;gap:8px;">
      <button id="tb-save" title="Save active tab (⌘S / Ctrl+S)" style="${btn}">💾 Save</button>
      <button id="tb-reload" title="Reload active tab from disk (⌘R / Ctrl+R)" style="${btn}">🔄 Reload</button>
      <button id="tb-export" title="Export active tab to Word (.docx)" style="${btn}">📄 Export…</button>
      <button id="open-file" title="Open file (⌘O)" style="${btn}">Open…</button>
      <button id="open-folder" title="Open folder" style="${btn}">Open Folder…</button>
      <select id="theme-select" title="Editor + preview theme" style="background:#444;color:#eee;border:none;padding:2px 6px;border-radius:3px;font-size:12px;">
        <option value="default">Light</option>
        <option value="dark">Dark</option>
        <option value="solarized">Solarized</option>
      </select>
      <button id="toggle-vim" title="Toggle Vim keybindings" aria-pressed="false" style="${btn}">Vim: off</button>
      <button id="toggle-preview" title="Cycle view: both / editor / preview" style="${btn}">View: ◧◨</button>
      <button id="toggle-devtools" title="Toggle DevTools inspector" style="background:transparent;border:1px solid #555;color:#888;padding:2px 6px;border-radius:3px;font-size:13px;cursor:pointer;line-height:1;">🐞</button>
    </div>
  `;
  container.querySelector<HTMLButtonElement>("#tb-save")!
    .addEventListener("click", () => handlers.onSave());
  container.querySelector<HTMLButtonElement>("#tb-reload")!
    .addEventListener("click", () => handlers.onReload());
  container.querySelector<HTMLButtonElement>("#tb-export")!
    .addEventListener("click", () => handlers.onExport());
  container.querySelector<HTMLButtonElement>("#open-file")!
    .addEventListener("click", async () => {
      const home = await homeDir();
      const result = await open({
        multiple: false,
        defaultPath: home,
        filters: [
          { name: "All supported", extensions: ["md", "markdown", "txt", "mmd", "png", "jpg", "jpeg", "gif", "webp"] },
          { name: "Markdown", extensions: ["md", "markdown", "txt"] },
          { name: "Mermaid", extensions: ["mmd"] },
          { name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] },
        ],
      });
      if (result) handlers.onOpenFile(result as string);
    });
  container.querySelector<HTMLButtonElement>("#open-folder")!
    .addEventListener("click", async () => {
      const home = await homeDir();
      const result = await open({ directory: true, multiple: false, defaultPath: home });
      if (result) handlers.onOpenFolder(result as string);
    });
  container.querySelector<HTMLSelectElement>("#theme-select")!
    .addEventListener("change", e => handlers.onThemeChange((e.target as HTMLSelectElement).value as Theme));
  container.querySelector<HTMLButtonElement>("#toggle-vim")!
    .addEventListener("click", () => handlers.onToggleVim());
  container.querySelector<HTMLButtonElement>("#toggle-preview")!
    .addEventListener("click", () => handlers.onCyclePreview());
  container.querySelector<HTMLButtonElement>("#toggle-devtools")!
    .addEventListener("click", () => invoke("toggle_devtools").catch(console.error));
}

const MODE_GLYPH: Record<string, string> = { both: "◧◨", edit: "◧", preview: "◨" };

/// Reflect the active tab's view mode on the toolbar's View button so the
/// label matches what the per-tab mode icon shows.
export function setToolbarViewMode(container: HTMLElement, mode: string): void {
  const b = container.querySelector<HTMLButtonElement>("#toggle-preview");
  if (b) b.textContent = `View: ${MODE_GLYPH[mode] ?? "◧◨"}`;
}

/// Reflect the current theme selection on the toolbar dropdown.
export function setToolbarTheme(container: HTMLElement, theme: string): void {
  const s = container.querySelector<HTMLSelectElement>("#theme-select");
  if (s) s.value = theme;
}

/// Reflect the Vim on/off state on the toolbar toggle button.
export function setToolbarVim(container: HTMLElement, enabled: boolean): void {
  const b = container.querySelector<HTMLButtonElement>("#toggle-vim");
  if (b) {
    b.textContent = `Vim: ${enabled ? "on" : "off"}`;
    b.setAttribute("aria-pressed", String(enabled));
    b.style.borderColor = enabled ? "#1577c4" : "#666";
    b.style.color = enabled ? "#fff" : "#ccc";
  }
}
