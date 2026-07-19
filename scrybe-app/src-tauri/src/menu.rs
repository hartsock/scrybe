// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Native menu bar — File / Edit / View (#184).
//!
//! Every custom item routes to the frontend over a single `scrybe://menu`
//! event carrying the item id; the frontend dispatches to the same
//! single-entry-point functions the toolbar buttons and the MCP pollers
//! already share, so the human ↔ MCP parity rule holds with no new tools.
//! The Edit menu is all predefined items — required once the default menu
//! is replaced, or the webview loses its clipboard shortcuts.
//!
//! Check-item state (theme radio, Vim, Wrap) is synced from the frontend's
//! existing `publishState` mirror via the `menu_sync` command, so toolbar,
//! MCP, and menu can never disagree for long.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime, Wry};

/// Event the frontend listens on; payload is the menu item id.
pub const MENU_EVENT: &str = "scrybe://menu";

/// Every custom (non-predefined) menu item id, in menu order. The single
/// source of truth for the id ↔ frontend-action contract: `build` creates
/// exactly these ids and the frontend dispatches on exactly these strings.
pub const MENU_IDS: &[&str] = &[
    "new_tab",
    "open_file",
    "open_folder",
    "save",
    "reload",
    "export_docx",
    "print",
    "close_tab",
    "cycle_view",
    "theme_default",
    "theme_dark",
    "theme_solarized",
    "toggle_vim",
    "toggle_wrap",
];

/// Validate a menu event id against the contract. Returns the id back as
/// the frontend action string, or `None` for predefined/unknown items
/// (which the OS handles itself and the frontend must never see).
pub fn menu_action(id: &str) -> Option<&'static str> {
    MENU_IDS.iter().find(|&&known| known == id).copied()
}

/// Handles to the stateful check items so `menu_sync` can mirror frontend
/// state. `None` until `build` runs.
struct CheckItems {
    theme_default: CheckMenuItem<Wry>,
    theme_dark: CheckMenuItem<Wry>,
    theme_solarized: CheckMenuItem<Wry>,
    vim: CheckMenuItem<Wry>,
    wrap: CheckMenuItem<Wry>,
}

static CHECK_ITEMS: Mutex<Option<CheckItems>> = Mutex::new(None);

/// Build the application menu. Called from `Builder::menu`.
pub fn build(app: &AppHandle<Wry>) -> tauri::Result<Menu<Wry>> {
    // ── File ────────────────────────────────────────────────────────────
    let new_tab = MenuItem::with_id(app, "new_tab", "New Tab", true, Some("CmdOrCtrl+N"))?;
    let open_file = MenuItem::with_id(app, "open_file", "Open…", true, Some("CmdOrCtrl+O"))?;
    let open_folder = MenuItem::with_id(
        app,
        "open_folder",
        "Open Folder…",
        true,
        Some("CmdOrCtrl+Shift+O"),
    )?;
    let save = MenuItem::with_id(app, "save", "Save", true, Some("CmdOrCtrl+S"))?;
    let reload = MenuItem::with_id(app, "reload", "Reload from Disk", true, Some("CmdOrCtrl+R"))?;
    let export_docx = MenuItem::with_id(app, "export_docx", "Export to Word…", true, None::<&str>)?;
    let print = MenuItem::with_id(app, "print", "Print…", true, Some("CmdOrCtrl+P"))?;
    let close_tab = MenuItem::with_id(app, "close_tab", "Close Tab", true, Some("CmdOrCtrl+W"))?;

    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new_tab,
            &open_file,
            &open_folder,
            &PredefinedMenuItem::separator(app)?,
            &save,
            &reload,
            &PredefinedMenuItem::separator(app)?,
            &export_docx,
            &print,
            &PredefinedMenuItem::separator(app)?,
            &close_tab,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(app)?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // ── Edit — predefined items keep webview clipboard shortcuts alive ──
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    // ── View ────────────────────────────────────────────────────────────
    let cycle_view = MenuItem::with_id(
        app,
        "cycle_view",
        "Cycle View Mode",
        true,
        Some("CmdOrCtrl+Shift+V"),
    )?;
    let theme_default = CheckMenuItem::with_id(
        app,
        "theme_default",
        "Default Theme",
        true,
        true,
        None::<&str>,
    )?;
    let theme_dark =
        CheckMenuItem::with_id(app, "theme_dark", "Dark Theme", true, false, None::<&str>)?;
    let theme_solarized = CheckMenuItem::with_id(
        app,
        "theme_solarized",
        "Solarized Theme",
        true,
        false,
        None::<&str>,
    )?;
    let toggle_vim = CheckMenuItem::with_id(
        app,
        "toggle_vim",
        "Vim Keybindings",
        true,
        false,
        None::<&str>,
    )?;
    let toggle_wrap =
        CheckMenuItem::with_id(app, "toggle_wrap", "Word Wrap", true, false, None::<&str>)?;

    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &cycle_view,
            &PredefinedMenuItem::separator(app)?,
            &theme_default,
            &theme_dark,
            &theme_solarized,
            &PredefinedMenuItem::separator(app)?,
            &toggle_vim,
            &toggle_wrap,
        ],
    )?;

    // ── Window ──────────────────────────────────────────────────────────
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;

    *CHECK_ITEMS.lock().unwrap() = Some(CheckItems {
        theme_default,
        theme_dark,
        theme_solarized,
        vim: toggle_vim,
        wrap: toggle_wrap,
    });

    // On macOS the first submenu becomes the application menu.
    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::with_items(
            app,
            "Scrybe",
            true,
            &[
                &PredefinedMenuItem::about(app, None, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, None)?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::show_all(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, None)?,
            ],
        )?;
        Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window])
    }
    #[cfg(not(target_os = "macos"))]
    {
        Menu::with_items(app, &[&file, &edit, &view, &window])
    }
}

/// Forward a custom menu click to the frontend. Predefined items never get
/// here (the OS handles them); unknown ids are ignored defensively.
pub fn handle_event<R: Runtime>(app: &AppHandle<R>, event: &tauri::menu::MenuEvent) {
    if let Some(action) = menu_action(event.id().as_ref()) {
        let _ = app.emit(MENU_EVENT, action);
    }
}

/// Mirror frontend state onto the menu's check items. Invoked from the
/// frontend's `publishState` — the same single mirror point that feeds the
/// MCP `state` tool — so menu, toolbar, and MCP stay in agreement no
/// matter which surface changed the setting.
#[tauri::command]
pub fn menu_sync(theme: String, vim: bool, wrap: bool) {
    if let Some(items) = CHECK_ITEMS.lock().unwrap().as_ref() {
        let _ = items.theme_default.set_checked(theme == "default");
        let _ = items.theme_dark.set_checked(theme == "dark");
        let _ = items.theme_solarized.set_checked(theme == "solarized");
        let _ = items.vim.set_checked(vim);
        let _ = items.wrap.set_checked(wrap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_contract_id_maps_to_itself() {
        for id in MENU_IDS {
            assert_eq!(menu_action(id), Some(*id));
        }
    }

    #[test]
    fn predefined_and_unknown_ids_are_ignored() {
        // Predefined items get OS-generated ids; none may leak to the
        // frontend as an action.
        for id in ["quit", "copy", "paste", "", "bogus", "SAVE"] {
            assert_eq!(menu_action(id), None);
        }
    }

    #[test]
    fn theme_ids_cover_every_toolbar_theme() {
        // The toolbar offers default/dark/solarized; the menu must too.
        for theme in ["default", "dark", "solarized"] {
            assert!(MENU_IDS.contains(&format!("theme_{theme}").as_str()));
        }
    }
}
