// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Unit tests for the sidecar autosave buffer commands.
//!
//! These exercise the file-level contract — the Tauri IPC layer is a
//! thin wrapper around the underlying functions, so direct calls are
//! sufficient. All tests use `tempfile::tempdir` to avoid touching
//! real user files.
use super::*;

#[test]
fn navigation_guard_allows_only_scrybe_frontend_origins() {
    for trusted in [
        "tauri://localhost/",
        "tauri://localhost/#section",
        "http://tauri.localhost/",
    ] {
        let url = tauri::Url::parse(trusted).unwrap();
        assert!(is_trusted_app_navigation(&url), "should allow {trusted}");
    }

    for remote in [
        "https://login.microsoftonline.com/",
        "https://tauri.localhost.example.com/",
        "https://tauri.localhost/",
        "http://tauri.localhost:8080/",
        "tauri://remote-host/",
        "file:///tmp/replacement.html",
        "data:text/html,replacement",
        "javascript:document.body.textContent='replacement'",
        "about:blank",
    ] {
        let url = tauri::Url::parse(remote).unwrap();
        assert!(!is_trusted_app_navigation(&url), "should reject {remote}");
    }
}

#[test]
fn navigation_guard_limits_the_debug_vite_origin() {
    for remote in [
        "http://localhost:5174/",
        "https://localhost:5173/",
        "http://127.0.0.1:5173/",
        "http://127.0.0.1:5174/",
    ] {
        let url = tauri::Url::parse(remote).unwrap();
        assert!(!is_trusted_app_navigation(&url), "should reject {remote}");
    }

    let vite = tauri::Url::parse("http://localhost:5173/").unwrap();
    assert_eq!(is_trusted_app_navigation(&vite), cfg!(debug_assertions));
}

#[test]
fn navigation_guard_debug_vite_origin_matches_tauri_configuration() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let configured = config["build"]["devUrl"].as_str().unwrap();
    assert_eq!(configured, DEBUG_FRONTEND_ORIGIN);

    let url = tauri::Url::parse(configured).unwrap();
    assert_eq!(
        is_trusted_app_navigation(&url),
        cfg!(debug_assertions),
        "the configured Vite origin must be the debug exception"
    );
}

fn temp_file(name: &str, content: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("seed write");
    let s = path.to_string_lossy().into_owned();
    (dir, s)
}

#[test]
fn save_buffer_writes_sidecar_not_real_file() {
    let (_dir, path) = temp_file("doc.md", "original on disk");
    save_buffer(path.clone(), "in-progress edits".into()).unwrap();

    // Real file must be untouched
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original on disk");
    // Sidecar must hold the buffer
    let bp = buffer_path_for(&path);
    assert_eq!(std::fs::read_to_string(&bp).unwrap(), "in-progress edits");
}

#[test]
fn save_file_clears_sidecar() {
    let (_dir, path) = temp_file("doc.md", "v1");
    save_buffer(path.clone(), "v2-buffered".into()).unwrap();
    assert!(buffer_path_for(&path).exists());

    save_file(path.clone(), "v2-committed".into()).unwrap();

    // Real file got the committed bytes; sidecar is gone.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2-committed");
    assert!(!buffer_path_for(&path).exists());
}

#[test]
fn clear_buffer_removes_sidecar_when_present() {
    let (_dir, path) = temp_file("doc.md", "v1");
    save_buffer(path.clone(), "drafted".into()).unwrap();
    assert!(buffer_path_for(&path).exists());

    clear_buffer(path.clone()).unwrap();
    assert!(!buffer_path_for(&path).exists());
}

#[test]
fn clear_buffer_is_silent_noop_when_missing() {
    let (_dir, path) = temp_file("doc.md", "v1");
    // No sidecar yet — must not error.
    clear_buffer(path).unwrap();
}

#[test]
fn read_buffer_returns_none_when_no_sidecar() {
    let (_dir, path) = temp_file("doc.md", "v1");
    assert_eq!(read_buffer_if_exists(path).unwrap(), None);
}

#[test]
fn read_buffer_returns_none_when_sidecar_matches_disk() {
    let (_dir, path) = temp_file("doc.md", "same content");
    save_buffer(path.clone(), "same content".into()).unwrap();
    // Sidecar exists but matches disk — no recovery needed.
    assert_eq!(read_buffer_if_exists(path).unwrap(), None);
}

#[test]
fn read_buffer_returns_content_when_sidecar_differs() {
    let (_dir, path) = temp_file("doc.md", "v1");
    save_buffer(path.clone(), "unsaved v2".into()).unwrap();
    assert_eq!(
        read_buffer_if_exists(path).unwrap(),
        Some("unsaved v2".into())
    );
}

#[test]
fn read_buffer_returns_none_for_empty_sidecar() {
    let (_dir, path) = temp_file("doc.md", "v1");
    save_buffer(path.clone(), "".into()).unwrap();
    // Empty sidecar — discard rather than offer to restore-to-nothing.
    assert_eq!(read_buffer_if_exists(path).unwrap(), None);
}

#[test]
fn remove_backup_clears_both_backup_and_sidecar() {
    let (_dir, path) = temp_file("doc.md", "v1");
    save_buffer(path.clone(), "drafted".into()).unwrap();
    // Trigger the ~ backup via save_file
    save_file(path.clone(), "v2".into()).unwrap();
    // save_file clears the sidecar, so re-create one to test cleanup
    save_buffer(path.clone(), "drafted again".into()).unwrap();

    let bak = std::path::PathBuf::from(format!("{path}~"));
    let bp = buffer_path_for(&path);
    assert!(bak.exists());
    assert!(bp.exists());

    remove_backup(path).unwrap();

    assert!(!bak.exists());
    assert!(!bp.exists());
}

#[test]
fn docx_binary_name_is_platform_specific() {
    let name = executable_name("scrybe-docx");
    if cfg!(windows) {
        assert_eq!(name, "scrybe-docx.exe");
    } else {
        assert_eq!(name, "scrybe-docx");
    }
}

#[test]
fn existing_file_returns_candidate_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(executable_name("scrybe-docx"));
    std::fs::write(&path, "#!/bin/sh\n").expect("seed exporter");

    assert_eq!(
        existing_file(path.clone()),
        Some(path.to_string_lossy().into_owned())
    );
}

#[test]
fn shell_command_report_decodes_from_bundled_cli_json() {
    let stdout = br#"{
      "outcome":"repaired",
      "command_path":"/Users/test/.local/bin/scrybe",
      "target_path":"/Applications/Scrybe.app/Contents/MacOS/scrybe",
      "path_winner":null,
      "other_installations":[],
      "app_bundles":["/Applications/Scrybe.app"],
      "warnings":[],
      "message":"Repaired the scrybe command"
    }"#;
    let report = decode_shell_command_report(true, stdout, b"").unwrap();
    assert_eq!(report.outcome, "repaired");
    assert_eq!(report.warnings, Vec::<String>::new());
}

#[test]
fn shell_command_failure_preserves_cli_explanation() {
    let error =
        decode_shell_command_report(false, b"", b"refusing to overwrite existing non-symlink")
            .unwrap_err();
    assert_eq!(error, "refusing to overwrite existing non-symlink");
}
