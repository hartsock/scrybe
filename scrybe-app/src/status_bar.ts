// SPDX-License-Identifier: Apache-2.0
import type { AppState, TabEntry } from "./state";

/// Always-present footer showing where the active tab lives on disk.
///
/// Default view: a compact path the user can paste into a CLI agent
/// (relative to the content root if the file is under it, otherwise
/// `~/...` for the home directory, otherwise the absolute path).
///
/// On hover or click, a popover reveals:
///   - full absolute path
///   - relative path (when applicable)
///   - basename
/// Each row has a copy button. Click toggles the popover into a pinned
/// state; click outside dismisses it. Hover is a transient preview.
export class StatusBar {
  private root: HTMLElement;
  private label: HTMLElement;
  private popover: HTMLElement;
  private contentRoot: string | null = null;
  private home: string | null = null;
  private currentTab: TabEntry | undefined;
  private pinned = false;

  constructor(host: HTMLElement) {
    this.root = document.createElement("div");
    this.root.className = "status-bar";

    this.label = document.createElement("span");
    this.label.className = "status-bar-label";
    this.label.textContent = "(no file)";
    this.root.appendChild(this.label);

    this.popover = document.createElement("div");
    this.popover.className = "status-bar-popover";
    this.popover.style.display = "none";
    this.root.appendChild(this.popover);

    this.root.addEventListener("mouseenter", () => {
      if (!this.pinned) this.showPopover();
    });
    this.root.addEventListener("mouseleave", () => {
      if (!this.pinned) this.hidePopover();
    });
    this.label.addEventListener("click", (e) => {
      e.stopPropagation();
      this.pinned = !this.pinned;
      if (this.pinned) this.showPopover();
      else this.hidePopover();
    });
    document.addEventListener("click", (e) => {
      if (this.pinned && !this.root.contains(e.target as Node)) {
        this.pinned = false;
        this.hidePopover();
      }
    });

    host.appendChild(this.root);
  }

  setContentRoot(root: string | null): void {
    this.contentRoot = root ? trimSlash(root) : null;
    this.refresh();
  }

  setHome(home: string | null): void {
    this.home = home ? trimSlash(home) : null;
    this.refresh();
  }

  update(state: AppState): void {
    this.currentTab = state.activeTab();
    this.refresh();
  }

  private refresh(): void {
    const tab = this.currentTab;
    if (!tab || !tab.path) {
      this.label.textContent = tab ? "(unsaved)" : "(no file)";
      this.label.title = tab ? "This tab has no path on disk yet" : "No file open";
      this.popover.innerHTML = "";
      return;
    }
    this.label.textContent = this.compactDisplay(tab.path);
    this.label.title = "Click for full path and copy options";
    this.renderPopover(tab.path);
  }

  private compactDisplay(path: string): string {
    const rel = this.relativeTo(path, this.contentRoot);
    if (rel !== null) return `./${rel}`;
    const homeRel = this.relativeTo(path, this.home);
    if (homeRel !== null) return `~/${homeRel}`;
    return path;
  }

  private relativeTo(path: string, anchor: string | null): string | null {
    if (!anchor) return null;
    if (path === anchor) return ".";
    const prefix = anchor.endsWith("/") ? anchor : `${anchor}/`;
    if (path.startsWith(prefix)) return path.slice(prefix.length);
    return null;
  }

  private renderPopover(path: string): void {
    this.popover.innerHTML = "";
    const rows: { label: string; value: string }[] = [
      { label: "Full path", value: path },
    ];
    const rel = this.relativeTo(path, this.contentRoot);
    if (rel !== null) {
      rows.push({ label: "Relative to root", value: rel });
    }
    const base = path.split("/").pop();
    if (base) rows.push({ label: "File name", value: base });
    if (this.contentRoot) {
      rows.push({ label: "Content root", value: this.contentRoot });
    }

    for (const row of rows) {
      const r = document.createElement("div");
      r.className = "status-bar-row";

      const k = document.createElement("span");
      k.className = "status-bar-key";
      k.textContent = row.label;
      r.appendChild(k);

      const v = document.createElement("span");
      v.className = "status-bar-value";
      v.textContent = row.value;
      v.title = row.value;
      r.appendChild(v);

      const btn = document.createElement("button");
      btn.className = "status-bar-copy";
      btn.type = "button";
      btn.textContent = "Copy";
      btn.onclick = (e) => {
        e.stopPropagation();
        copyToClipboard(row.value).then(() => {
          btn.textContent = "Copied";
          setTimeout(() => { btn.textContent = "Copy"; }, 1200);
        });
      };
      r.appendChild(btn);

      this.popover.appendChild(r);
    }
  }

  private showPopover(): void {
    if (this.currentTab?.path) this.popover.style.display = "block";
  }

  private hidePopover(): void {
    this.popover.style.display = "none";
  }
}

function trimSlash(p: string): string {
  return p.length > 1 && p.endsWith("/") ? p.slice(0, -1) : p;
}

async function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // fall through to the textarea fallback below
    }
  }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  try {
    document.execCommand("copy");
  } finally {
    ta.remove();
  }
}
