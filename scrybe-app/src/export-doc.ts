// SPDX-License-Identifier: Apache-2.0
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { showToast } from "./toast";
import type { AppState } from "./state";

export interface ExportDeps {
  state: AppState;
}

export function createExporters(deps: ExportDeps) {
  const { state } = deps;

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

  /// Export every Mermaid diagram in the active tab to sibling PNG figures
  /// (`<stem>_fig_NN.png`) next to the document. Requires a saved tab — the
  /// figures are placed beside the file on disk, so an unsaved scratch buffer
  /// has nowhere to write them. Uses the live buffer so unsaved edits are
  /// included. The MCP/CLI `export_figures` tool is the agent-side equivalent.
  async function exportActiveTabFigures(): Promise<void> {
    const tab = state.activeTab();
    if (!tab) { showToast("No tab to export", "info"); return; }
    if (!tab.path) { showToast("Save the document first", "info"); return; }
    try {
      const paths = await invoke<string[]>("export_figures", {
        content: tab.content,
        path: tab.path,
      });
      showToast(`Exported ${paths.length} diagram(s)`, "info");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast(`Export failed: ${msg}`);
    }
  }

  return { exportActiveTabToWord, exportActiveTabFigures };
}
