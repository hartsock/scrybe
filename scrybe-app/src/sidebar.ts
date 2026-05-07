// SPDX-License-Identifier: AGPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";

export interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
}

export class Sidebar {
  private container: HTMLElement;
  private cwd: string = "";
  private entries: FileEntry[] = [];
  private onOpenFile: (path: string) => void;
  private onDirectoryLoad: ((path: string) => void) | undefined;

  constructor(
    container: HTMLElement,
    onOpenFile: (path: string) => void,
    onDirectoryLoad?: (path: string) => void,
  ) {
    this.container = container;
    this.onOpenFile = onOpenFile;
    this.onDirectoryLoad = onDirectoryLoad;
  }

  async loadDirectory(path: string): Promise<void> {
    this.cwd = path;
    try {
      const entries: FileEntry[] = await invoke("list_directory", { path });
      this.entries = entries.sort((a, b) => {
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
    } catch {
      this.entries = [];
    }
    this.render();
    this.onDirectoryLoad?.(path);
  }

  render(): void {
    this.container.innerHTML = `
      <div class="sidebar-body">
        ${this.renderFiles()}
      </div>
    `;
    this.container.querySelectorAll<HTMLElement>(".file-entry").forEach(el => {
      const path = el.dataset.path!;
      const isDir = el.dataset.isDir === "true";
      el.onclick = () => isDir ? this.loadDirectory(path) : this.onOpenFile(path);
    });
  }

  private renderFiles(): string {
    if (!this.cwd) return `<div class="sb-hint">No folder open.<br><small>File &rarr; Open Folder</small></div>`;
    const up = this.cwd.includes("/")
      ? `<div class="file-entry dir-entry" data-path="${parentDir(this.cwd)}" data-is-dir="true">&larr; ..</div>`
      : "";
    const items = this.entries.map(e => `
      <div class="file-entry${e.isDir ? " dir-entry" : ""}" data-path="${e.path}" data-is-dir="${e.isDir}">
        ${e.isDir ? "&#128193;" : "&#128196;"} ${e.name}
      </div>`).join("");
    return `<div class="sb-path">${this.cwd}</div>${up}${items}`;
  }
}

function parentDir(path: string): string {
  const parts = path.replace(/\/$/, "").split("/");
  parts.pop();
  return parts.join("/") || "/";
}
