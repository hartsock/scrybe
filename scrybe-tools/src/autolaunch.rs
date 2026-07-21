// Copyright 2026 Shawn Hartsock and contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Opt-in auto-launch of the Scrybe desktop app (issue #225).
//!
//! A stateful tool (`open`, `state`, `list_tabs`, …) reaches the running editor
//! over `~/.scrybe/sock`. When nothing is listening, [`crate::LiveApp::call`]
//! reports `NoApp` and the tool returns a clean `no_live_app` — correct, but it
//! means "open this file in Scrybe" fails whenever the app happens to be closed.
//!
//! This module restores an *opt-in* launch: when `SCRYBE_MCP_AUTOLAUNCH` is set,
//! a no-app call first tries to start the installed app, waits for its socket,
//! and retries once. It is **off by default** on purpose — an MCP server
//! spawning a GUI window is a deliberate choice, not a surprise. It also
//! deliberately does *not* repeat the old `which scrybe-app` mistake: a cask- or
//! DMG-installed app lives in a `.app` bundle that is never on `PATH`, so we
//! resolve it by **install location**, not by `which`.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Env var that opts INTO auto-launch. Truthy = `1`/`true`/`yes`/`on`.
pub const AUTOLAUNCH_ENV: &str = "SCRYBE_MCP_AUTOLAUNCH";

/// Optional explicit `Scrybe.app` bundle path, checked before the standard
/// install locations (a non-default install, or CI).
pub const APP_ENV: &str = "SCRYBE_APP";

/// Liveness probes to make after launching before giving up (the app has to
/// start and bind its socket). 20 × [`POLL_INTERVAL`] ≈ a 5 s budget.
const DEFAULT_TRIES: u32 = 20;
/// Delay between liveness probes while waiting for the app to come up.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Is auto-launch opted in? True only when `SCRYBE_MCP_AUTOLAUNCH` is truthy.
pub fn enabled() -> bool {
    is_truthy(std::env::var(AUTOLAUNCH_ENV).ok().as_deref())
}

fn is_truthy(v: Option<&str>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// Resolve the installed `Scrybe.app`, in priority order:
/// `$SCRYBE_APP` → `/Applications/Scrybe.app` → `$HOME/Applications/Scrybe.app`.
///
/// Pure over the injected `env` lookup and `exists` probe, so the resolution
/// order is unit-testable without touching the real filesystem.
pub fn resolve_app(
    env: impl Fn(&str) -> Option<String>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(p) = env(APP_ENV) {
        let p = PathBuf::from(p);
        if exists(&p) {
            return Some(p);
        }
    }
    let mut candidates = vec![PathBuf::from("/Applications/Scrybe.app")];
    if let Some(home) = env("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/Scrybe.app"));
    }
    candidates.into_iter().find(|p| exists(p))
}

/// Ensure a reachable app, launching it if it is not already up. Pure
/// orchestration over the injected `is_live` probe, `launch` action, and
/// `sleep` — no real subprocess, socket, or wall-clock — so the retry/timeout
/// logic is unit-testable. Returns `true` if the app is (or became) reachable.
pub fn ensure_running(
    is_live: impl Fn() -> bool,
    launch: impl Fn() -> bool,
    sleep: impl Fn(),
    tries: u32,
) -> bool {
    if is_live() {
        return true;
    }
    if !launch() {
        return false;
    }
    for _ in 0..tries {
        sleep();
        if is_live() {
            return true;
        }
    }
    false
}

/// Launch the resolved `Scrybe.app` detached and in the background. macOS-only;
/// returns `false` if no app was found or the spawn failed. On every other OS
/// auto-launch is a no-op — there is no bundle to `open`.
pub fn launch_installed_app() -> bool {
    launch_app(|k| std::env::var(k).ok(), |p| p.exists())
}

#[cfg(target_os = "macos")]
fn launch_app(env: impl Fn(&str) -> Option<String>, exists: impl Fn(&Path) -> bool) -> bool {
    let Some(app) = resolve_app(env, exists) else {
        return false;
    };
    // `open -g` launches the bundle in the background without stealing focus.
    std::process::Command::new("open")
        .arg("-g")
        .arg(&app)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn launch_app(_env: impl Fn(&str) -> Option<String>, _exists: impl Fn(&Path) -> bool) -> bool {
    false
}

/// The production launch-and-wait the live transport calls on a no-app request:
/// opt-in gated, launches the installed app, and polls the real socket up to the
/// budget. Returns `true` if the app became reachable.
pub fn autolaunch_and_wait() -> bool {
    if !enabled() {
        return false;
    }
    ensure_running(
        scrybe_rpc::client::is_live,
        launch_installed_app,
        || std::thread::sleep(POLL_INTERVAL),
        DEFAULT_TRIES,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn enabled_is_truthy_only_for_yes_values() {
        for v in ["1", "true", "TRUE", "yes", "on", " on "] {
            assert!(is_truthy(Some(v)), "{v:?} should be truthy");
        }
        for v in ["0", "false", "no", "off", ""] {
            assert!(!is_truthy(Some(v)), "{v:?} should be falsy");
        }
        assert!(!is_truthy(None));
    }

    #[test]
    fn resolve_prefers_scrybe_app_env_when_it_exists() {
        let app = resolve_app(
            |k| (k == APP_ENV).then(|| "/custom/Scrybe.app".to_string()),
            |p| p == Path::new("/custom/Scrybe.app"),
        );
        assert_eq!(app, Some(PathBuf::from("/custom/Scrybe.app")));
    }

    #[test]
    fn resolve_skips_a_missing_env_override_and_falls_through() {
        let app = resolve_app(
            |k| (k == APP_ENV).then(|| "/gone/Scrybe.app".to_string()),
            |p| p == Path::new("/Applications/Scrybe.app"),
        );
        assert_eq!(app, Some(PathBuf::from("/Applications/Scrybe.app")));
    }

    #[test]
    fn resolve_falls_back_to_system_then_user_applications() {
        let sys = resolve_app(|_| None, |p| p == Path::new("/Applications/Scrybe.app"));
        assert_eq!(sys, Some(PathBuf::from("/Applications/Scrybe.app")));

        let user = resolve_app(
            |k| (k == "HOME").then(|| "/Users/me".to_string()),
            |p| p == Path::new("/Users/me/Applications/Scrybe.app"),
        );
        assert_eq!(user, Some(PathBuf::from("/Users/me/Applications/Scrybe.app")));
    }

    #[test]
    fn resolve_is_none_when_nothing_is_installed() {
        assert_eq!(resolve_app(|_| None, |_| false), None);
    }

    #[test]
    fn ensure_running_short_circuits_without_launching_when_already_live() {
        let launched = Cell::new(false);
        let ok = ensure_running(
            || true,
            || {
                launched.set(true);
                true
            },
            || {},
            20,
        );
        assert!(ok);
        assert!(!launched.get(), "must not launch when already live");
    }

    #[test]
    fn ensure_running_is_false_when_launch_fails() {
        assert!(!ensure_running(|| false, || false, || {}, 20));
    }

    #[test]
    fn ensure_running_polls_until_the_app_comes_up() {
        let ticks = Cell::new(0u32);
        let ok = ensure_running(
            || ticks.get() >= 3, // live on the 3rd probe after launch
            || true,
            || ticks.set(ticks.get() + 1),
            20,
        );
        assert!(ok);
        assert_eq!(ticks.get(), 3);
    }

    #[test]
    fn ensure_running_gives_up_after_the_budget() {
        let sleeps = Cell::new(0u32);
        let ok = ensure_running(|| false, || true, || sleeps.set(sleeps.get() + 1), 5);
        assert!(!ok);
        assert_eq!(sleeps.get(), 5, "polls exactly `tries` times then gives up");
    }
}
