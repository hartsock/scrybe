// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";

export type Theme = "default" | "dark" | "solarized";

export function buildToolbar(
  container: HTMLElement,
  onThemeChange: (theme: Theme) => void,
  onTogglePreview: () => void,
  onOpenFile: (path: string) => void,
  onOpenFolder: (path: string) => void,
): void {
  container.innerHTML = `
    <span style="font-weight:600;letter-spacing:0.5px;">Scrybe</span>
    <div style="margin-left:auto;display:flex;align-items:center;gap:8px;">
      <button id="open-file" title="Open file (⌘O)" style="background:transparent;border:1px solid #666;color:#ccc;padding:2px 8px;border-radius:3px;font-size:12px;cursor:pointer;">Open…</button>
      <button id="open-folder" title="Open folder" style="background:transparent;border:1px solid #666;color:#ccc;padding:2px 8px;border-radius:3px;font-size:12px;cursor:pointer;">Open Folder…</button>
      <select id="theme-select" style="background:#444;color:#eee;border:none;padding:2px 6px;border-radius:3px;font-size:12px;">
        <option value="default">Light</option>
        <option value="dark">Dark</option>
        <option value="solarized">Solarized</option>
      </select>
      <button id="toggle-preview" style="background:transparent;border:1px solid #666;color:#ccc;padding:2px 8px;border-radius:3px;font-size:12px;cursor:pointer;">Preview</button>
      <button id="toggle-devtools" title="Toggle DevTools inspector" style="background:transparent;border:1px solid #555;color:#888;padding:2px 6px;border-radius:3px;font-size:13px;cursor:pointer;line-height:1;">🐞</button>
    </div>
  `;
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
      if (result) onOpenFile(result as string);
    });
  container.querySelector<HTMLButtonElement>("#open-folder")!
    .addEventListener("click", async () => {
      const home = await homeDir();
      const result = await open({ directory: true, multiple: false, defaultPath: home });
      if (result) onOpenFolder(result as string);
    });
  container.querySelector<HTMLSelectElement>("#theme-select")!
    .addEventListener("change", e => onThemeChange((e.target as HTMLSelectElement).value as Theme));
  container.querySelector<HTMLButtonElement>("#toggle-preview")!
    .addEventListener("click", onTogglePreview);
  container.querySelector<HTMLButtonElement>("#toggle-devtools")!
    .addEventListener("click", () => invoke("toggle_devtools").catch(console.error));
}
