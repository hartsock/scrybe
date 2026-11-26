// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use crate::SUBCOMMANDS;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn read_input(path: Option<std::path::PathBuf>) -> anyhow::Result<String> {
    match path {
        Some(p) => Ok(std::fs::read_to_string(&p)?),
        None => {
            use std::io::Read;
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            Ok(s)
        }
    }
}

pub(crate) fn write_output(path: Option<std::path::PathBuf>, content: &str) -> anyhow::Result<()> {
    match path {
        Some(p) => std::fs::write(p, content)?,
        None => print!("{content}"),
    }
    Ok(())
}

/// Launch Scrybe with `path` as the file/directory argument.
///
/// On macOS, prefers the `.app` bundle via `open -n -a` so WebKit gets its
/// required bundle entitlements. Falls back to the raw binary on other
/// platforms (or when no bundle is found on macOS).
pub(crate) fn launch_scrybe(path: Option<&std::path::Path>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = find_scrybe_app_bundle() {
            let mut cmd = std::process::Command::new("open");
            cmd.args(["-n", "-a", &bundle]);
            if let Some(p) = path {
                cmd.arg("--args").arg(p);
            }
            cmd.spawn()
                .map_err(|e| format!("failed to open Scrybe.app: {e}"))?;
            return Ok(());
        }
    }
    // Non-macOS or no bundle found: launch the raw binary.
    let bin = which_scrybe_bin()?;
    let mut cmd = std::process::Command::new(&bin);
    if let Some(p) = path {
        cmd.arg(p);
    }
    cmd.spawn()
        .map_err(|e| format!("failed to launch scrybe-app: {e}"))?;
    Ok(())
}

/// Find `Scrybe.app` in standard macOS locations.
#[cfg(target_os = "macos")]
pub(crate) fn find_scrybe_app_bundle() -> Option<String> {
    let home_apps = std::env::var("HOME").ok().map(|h| {
        std::path::PathBuf::from(h)
            .join("Applications")
            .join("Scrybe.app")
    });
    let candidates: &[Option<std::path::PathBuf>] = &[
        home_apps,
        Some(std::path::PathBuf::from("/Applications/Scrybe.app")),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// If argv[1] is absent or not a known subcommand/flag, inject "open" so that
/// `scrybe`, `scrybe file.md`, and `scrybe ./` all route through the open handler.
pub(crate) fn inject_open_if_path(mut args: Vec<String>) -> Vec<String> {
    match args.get(1).map(|s| s.as_str()) {
        None => args.push("open".to_string()),
        Some(first) if !first.starts_with('-') && !SUBCOMMANDS.contains(&first) => {
            args.insert(1, "open".to_string());
        }
        _ => {}
    }
    args
}

/// Locate the raw `scrybe-app` binary: sibling of current exe first, then PATH.
pub(crate) fn which_scrybe_bin() -> Result<String, String> {
    if let Ok(exe) = std::env::current_exe() {
        let name = if cfg!(windows) {
            "scrybe-app.exe"
        } else {
            "scrybe-app"
        };
        let sibling = exe.with_file_name(name);
        if sibling.exists() {
            return Ok(sibling.to_string_lossy().into_owned());
        }
    }
    let output = std::process::Command::new("which")
        .arg("scrybe-app")
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !p.is_empty() {
            return Ok(p);
        }
    }
    Err(
        "scrybe-app not found. Install Scrybe.app to ~/Applications or build with: just app"
            .to_string(),
    )
}
