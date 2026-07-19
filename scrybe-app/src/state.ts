// SPDX-License-Identifier: Apache-2.0
import type { DocKind } from "./editor";

export type ViewMode = "both" | "edit" | "preview";
const VIEW_CYCLE: ViewMode[] = ["both", "edit", "preview"];

export interface TabEntry {
  id: string;
  path: string | null;
  title: string;
  isDirty: boolean;
  content: string;
  viewMode: ViewMode;
  /// How this document is treated. `code`/`text` tabs have no Markdown
  /// preview, so they open (and stay) in edit-only view.
  kind: DocKind;
}

export class AppState {
  tabs: TabEntry[] = [];
  activeTabId: string | null = null;

  addTab(path: string | null, content: string, kind: DocKind = "markdown"): string {
    if (path) {
      const existing = this.tabs.find(t => t.path === path);
      if (existing) {
        this.activeTabId = existing.id;
        return existing.id;
      }
    }
    const id = crypto.randomUUID();
    const title = path ? path.split("/").pop() ?? "Untitled" : "Untitled";
    // Code/text documents have no Markdown preview — open edit-only.
    const viewMode: ViewMode = kind === "markdown" ? "both" : "edit";
    this.tabs.push({ id, path, title, isDirty: false, content, viewMode, kind });
    this.activeTabId = id;
    return id;
  }

  activeTab(): TabEntry | undefined {
    return this.tabs.find(t => t.id === this.activeTabId);
  }

  updateContent(id: string, content: string): void {
    const tab = this.tabs.find(t => t.id === id);
    if (tab) { tab.content = content; tab.isDirty = true; }
  }

  markClean(id: string): void {
    const tab = this.tabs.find(t => t.id === id);
    if (tab) tab.isDirty = false;
  }

  markDirty(id: string): void {
    const tab = this.tabs.find(t => t.id === id);
    if (tab) tab.isDirty = true;
  }

  cycleViewMode(id: string): ViewMode {
    const tab = this.tabs.find(t => t.id === id);
    if (!tab) return "both";
    // A code/text document has no rendered preview to cycle to — stay edit-only.
    if (tab.kind !== "markdown") {
      tab.viewMode = "edit";
      return "edit";
    }
    tab.viewMode = VIEW_CYCLE[(VIEW_CYCLE.indexOf(tab.viewMode) + 1) % VIEW_CYCLE.length];
    return tab.viewMode;
  }

  closeTab(id: string): void {
    const idx = this.tabs.findIndex(t => t.id === id);
    if (idx === -1) return;
    this.tabs.splice(idx, 1);
    if (this.activeTabId === id) {
      this.activeTabId = this.tabs[Math.max(0, idx - 1)]?.id ?? null;
    }
  }
}
