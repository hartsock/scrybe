// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors
//
// P4.8 — VCS panel: git status, stage, commit, and recent-log sidebar widget.
// Talks to the Tauri backend via the six vcs_* commands registered in lib.rs.
import { invoke } from "@tauri-apps/api/core";

interface StatusEntry { path: string; status: string; }
interface CommitEntry { sha: string; message: string; author: string; }
interface RemoteEntry { name: string; url: string; role: string; }
interface RepoInfo { path: string; head: string | null; branch: string | null; }

export class VcsPanel {
  private container: HTMLElement;
  private repoPath: string | null = null;

  constructor(container: HTMLElement) {
    this.container = container;
    this.renderEmpty();
  }

  /** Open a repository at the given filesystem path and render the panel. */
  async openRepo(path: string): Promise<void> {
    try {
      const info = await invoke<RepoInfo>("vcs_open", { path });
      this.repoPath = info.path;
      await this.refresh();
    } catch (e) {
      this.renderError(String(e));
    }
  }

  /** Reload status, log, and remotes from the currently open repository. */
  async refresh(): Promise<void> {
    if (!this.repoPath) { this.renderEmpty(); return; }
    try {
      const [status, log, remotes] = await Promise.all([
        invoke<StatusEntry[]>("vcs_status"),
        invoke<CommitEntry[]>("vcs_log", { max: 5 }),
        invoke<RemoteEntry[]>("vcs_remotes"),
      ]);
      this.renderPanel(status, log, remotes);
    } catch (e) {
      this.renderError(String(e));
    }
  }

  private renderEmpty(): void {
    this.container.innerHTML =
      `<div class="vcs-hint">Open a folder containing a git repository to see VCS status.</div>`;
  }

  private renderError(msg: string): void {
    this.container.innerHTML = `<div class="vcs-error">${escapeHtml(msg)}</div>`;
  }

  private renderPanel(
    status: StatusEntry[],
    log: CommitEntry[],
    remotes: RemoteEntry[],
  ): void {
    const origin = remotes.find(r => r.role === "Origin");
    this.container.innerHTML = `
      <div class="vcs-panel">
        ${origin
          ? `<div class="vcs-remote">${escapeHtml(origin.url)}</div>`
          : ""}
        <div class="vcs-section">
          <div class="vcs-section-title">Changes (${status.length})</div>
          ${status.length === 0
            ? `<div class="vcs-clean">Nothing to commit</div>`
            : status.map(e => `
              <div class="vcs-entry">
                <span class="vcs-status">${statusIcon(e.status)}</span>
                <span class="vcs-path" title="${escapeHtml(e.path)}">${escapeHtml(e.path)}</span>
              </div>`).join("")}
        </div>
        ${status.length > 0 ? `
          <div class="vcs-actions">
            <button class="vcs-btn" id="vcs-stage-all">Stage All</button>
            <input id="vcs-msg" class="vcs-input" placeholder="Commit message…">
            <button class="vcs-btn vcs-btn-primary" id="vcs-commit">Commit</button>
          </div>` : ""}
        <div class="vcs-section">
          <div class="vcs-section-title">Recent commits</div>
          ${log.length === 0
            ? `<div class="vcs-clean">No commits yet</div>`
            : log.map(c => `
              <div class="vcs-commit">
                <code class="vcs-sha">${escapeHtml(c.sha)}</code>
                <span class="vcs-msg" title="${escapeHtml(c.message)}">${escapeHtml(c.message)}</span>
              </div>`).join("")}
        </div>
      </div>
    `;

    this.container.querySelector("#vcs-stage-all")?.addEventListener("click", async () => {
      try {
        await invoke("vcs_stage_all");
        await this.refresh();
      } catch (e) {
        this.renderError(String(e));
      }
    });

    this.container.querySelector("#vcs-commit")?.addEventListener("click", async () => {
      const input = this.container.querySelector("#vcs-msg") as HTMLInputElement | null;
      const msg = input?.value?.trim();
      if (!msg) return;
      try {
        await invoke("vcs_commit", {
          message: msg,
          authorName: "Scrybe User",
          authorEmail: "user@scrybe.local",
        });
        await this.refresh();
      } catch (e) {
        this.renderError(String(e));
      }
    });
  }
}

/** Map a FileStatus debug string to a single-character indicator. */
function statusIcon(status: string): string {
  if (status.includes("Modified")) return "M";
  if (status.includes("Added")) return "A";
  if (status.includes("Deleted")) return "D";
  if (status.includes("Renamed")) return "R";
  if (status.includes("Conflicted")) return "!";
  if (status.includes("Untracked")) return "?";
  return "~";
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
