// SPDX-License-Identifier: Apache-2.0
import { listen } from "@tauri-apps/api/event";
import type { AppState } from "./state";
import type { Theme } from "./toolbar";
import { installOrRepairShellCommand, removeShellCommand } from "./shell-command";

export interface MenuDeps {
  state: AppState;
  newTab: () => void;
  saveActiveTabNow: () => Promise<void>;
  reloadActiveTabNow: () => Promise<void>;
  exportActiveTabToWord: () => Promise<void>;
  exportActiveTabFigures: () => Promise<void>;
  printActiveTab: () => Promise<void>;
  closeTab: (id: string) => void;
  cyclePreviewMode: () => void;
  setViewMode: (mode: string) => void;
  applyTheme: (theme: Theme) => void;
  toggleVim: () => void;
  toggleWrap: () => void;
  setExplorerVisible: (on: boolean) => void;
  isExplorerVisible: () => boolean;
}

export function registerMenu(deps: MenuDeps): void {
  const {
    state, newTab, saveActiveTabNow, reloadActiveTabNow, exportActiveTabToWord,
    exportActiveTabFigures, printActiveTab, closeTab, cyclePreviewMode, setViewMode,
    applyTheme, toggleVim, toggleWrap, setExplorerVisible, isExplorerVisible,
  } = deps;

  // Native menu bar (#184). Editor item ids route to the same single-entry-point
  // function its toolbar/keyboard twin uses, so the human ↔ MCP parity rule
  // holds with no new tools. PATH installation is intentionally host-local and
  // never exposed to MCP. Predefined items are handled by the OS.
  listen<string>("scrybe://menu", event => {
    switch (event.payload) {
      case "new_tab":     newTab(); break;
      case "open_file":   document.getElementById("open-file")?.click(); break;
      case "open_folder": document.getElementById("open-folder")?.click(); break;
      case "save":        void saveActiveTabNow(); break;
      case "reload":      void reloadActiveTabNow(); break;
      case "export_docx": void exportActiveTabToWord(); break;
      case "export_figures": void exportActiveTabFigures(); break;
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
      case "view_wysiwyg": setViewMode("wysiwyg"); break;
      case "theme_default":   applyTheme("default"); break;
      case "theme_dark":      applyTheme("dark"); break;
      case "theme_solarized": applyTheme("solarized"); break;
      case "toggle_vim":  toggleVim(); break;
      case "toggle_wrap": toggleWrap(); break;
      case "toggle_explorer": setExplorerVisible(!isExplorerVisible()); break;
      case "install_shell_command":
        void installOrRepairShellCommand();
        break;
      case "uninstall_shell_command":
        void removeShellCommand();
        break;
      case "close_window":
        void import("@tauri-apps/api/window").then(w => w.getCurrentWindow().close());
        break;
      default: console.warn("unknown menu action:", event.payload);
    }
  }).catch(console.error);
}
