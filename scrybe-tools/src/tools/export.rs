// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `export` — export a Markdown document to Word (.docx) by driving the
//! `scrybe-docx` exporter (the Python toolkit's entry point). Mermaid blocks
//! are rendered to PNGs with their source embedded in the PNG metadata, so
//! the exported document's figures stay losslessly round-trippable.
//!
//! In-process from the registry's point of view (no live app needed — works
//! headless); the exporter itself is a subprocess, resolved in a fixed order:
//! `SCRYBE_DOCX_BIN` env → sibling of the current exe → `PATH` → `~/venv`
//! bin. Ported off the legacy MCP `ToolRegistry` (A2a) — this is now the only
//! implementation.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::{Ctx, DataSchema, EngineFault, Facet, ToolError, ToolOutcome, ToolSpec};

/// Version of this tool's `data` payload.
const DATA_VERSION: u32 = 1;

/// The `export` tool spec.
pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "export",
        description: "Export a Markdown file to a Word (.docx) document with \
             Mermaid diagrams rendered to PNGs (source embedded in PNG \
             metadata). Human equivalent: the toolbar Export button. Input: \
             `path` (the Markdown file); optional `output` (.docx path, \
             default: `path` with a .docx extension) and `no_diagrams` (keep \
             fenced Mermaid blocks as monospace text). Drives the `scrybe-docx` \
             exporter — install the Scrybe Python toolkit or set \
             SCRYBE_DOCX_BIN. Returns `{ path, output }`.",
        input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: data_schema,
        },
        mutates: true,
        // A whole-document operation like `render`, not a Mermaid-specific one
        // (diagram handling is incidental) — `Core` is the least-surprising home.
        facet: Facet::Core,
        handler,
    }
}

fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path to the Markdown file to export." },
            "output": { "type": "string", "description": "Output .docx path (default: `path` with a .docx extension)." },
            "no_diagrams": { "type": "boolean", "description": "Skip Mermaid rendering; keep fenced blocks as monospace text." }
        },
        "required": ["path"]
    })
}

fn data_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "v": { "const": DATA_VERSION },
            "kind": { "const": "export" },
            "path": { "type": "string", "description": "The Markdown file that was exported." },
            "output": { "type": "string", "description": "The .docx file that was written." }
        },
        "required": ["v", "kind", "path", "output"]
    })
}

fn handler(_ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
    // Required args are gated by the dispatcher.
    let path = args.get("path").and_then(Value::as_str).unwrap_or_default();
    let output = match args.get("output").and_then(Value::as_str) {
        Some(o) => o.to_string(),
        None => Path::new(path)
            .with_extension("docx")
            .to_string_lossy()
            .into_owned(),
    };
    let no_diagrams = args
        .get("no_diagrams")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let base = json!({ "v": DATA_VERSION, "kind": "export", "path": path, "output": output });

    // ── input/output validation (release-gate requirement) ──────────────────
    // The exporter is a subprocess fed caller-controlled paths; validate them
    // here so a bad call fails as a clear business error instead of an opaque
    // subprocess failure — and so the write target can't be redirected.
    if let Some(err) = validate_paths(path, &output) {
        return Ok(ToolOutcome::fail(base, err));
    }

    let bin = match which_scrybe_docx() {
        Ok(bin) => bin,
        Err(e) => {
            return Ok(ToolOutcome::fail(
                base,
                ToolError::new("exporter_not_found", e),
            ))
        }
    };

    let mut cmd = std::process::Command::new(bin);
    cmd.arg(path).arg("-o").arg(&output);
    if no_diagrams {
        cmd.arg("--no-diagrams");
    }
    Ok(match cmd.output() {
        Ok(out) if out.status.success() => ToolOutcome::ok(json!({
            "v": DATA_VERSION,
            "kind": "export",
            "path": path,
            "output": output,
        })),
        Ok(out) => ToolOutcome::fail(
            base,
            ToolError::new(
                "export_failed",
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ),
        ),
        Err(e) => ToolOutcome::fail(
            base,
            ToolError::new("export_failed", format!("failed to run scrybe-docx ({e})")),
        ),
    })
}

/// Validate the input document and the output target. Returns the business
/// error to report, or `None` when both are acceptable.
fn validate_paths(path: &str, output: &str) -> Option<ToolError> {
    // The input must be an existing regular file.
    if !Path::new(path).is_file() {
        return Some(ToolError::new(
            "input_not_found",
            format!("input Markdown file not found (or not a file): {path}"),
        ));
    }

    // The output's parent directory must already exist — the tool writes a
    // file, it does not invent directory trees.
    let out = Path::new(output);
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.is_dir() {
            return Some(ToolError::new(
                "bad_output",
                format!(
                    "output directory does not exist: {}",
                    parent.to_string_lossy()
                ),
            ));
        }
    }

    // Refuse a symlinked output target. Writing "through" a pre-planted
    // symlink would redirect the exporter's write to whatever the link points
    // at — a path the caller never named (classic symlink-planting attack on
    // a predictable output path). `symlink_metadata` inspects the link itself
    // without following it, so the check cannot be fooled by the target.
    if let Ok(md) = std::fs::symlink_metadata(out) {
        if md.file_type().is_symlink() {
            return Some(ToolError::new(
                "symlinked_output",
                format!("refusing to write through a symlinked output target: {output}"),
            ));
        }
    }

    None
}

// ── scrybe-docx resolution ──────────────────────────────────────────────────

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn home_venv_bin(name: &str) -> Option<PathBuf> {
    let bin_dir = if cfg!(windows) { "Scripts" } else { "bin" };
    home_dir().map(|home| home.join("venv").join(bin_dir).join(name))
}

fn existing_file(path: PathBuf) -> Option<String> {
    if path.is_file() {
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Resolve the `scrybe-docx` exporter: `SCRYBE_DOCX_BIN` env → sibling of the
/// current exe → `PATH` → `~/venv` bin.
fn which_scrybe_docx() -> Result<String, String> {
    if let Ok(path) = std::env::var("SCRYBE_DOCX_BIN") {
        if let Some(path) = existing_file(PathBuf::from(path)) {
            return Ok(path);
        }
    }

    let name = executable_name("scrybe-docx");
    if let Ok(exe) = std::env::current_exe() {
        if let Some(path) = existing_file(exe.with_file_name(&name)) {
            return Ok(path);
        }
    }
    if let Ok(path) = which::which(&name) {
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Some(path) = home_venv_bin(&name).and_then(existing_file) {
        return Ok(path);
    }

    Err(
        "scrybe-docx not found. Reinstall the Scrybe Python toolkit with docx export support or set SCRYBE_DOCX_BIN to the exporter executable."
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry};

    #[test]
    fn spec_is_mutating_core_facet() {
        let reg = Registry::default();
        let s = reg.get("export").expect("export registered");
        assert!(s.mutates);
        assert_eq!(s.facet, Facet::Core);
    }

    #[test]
    fn missing_path_is_engine_fault() {
        let err = Registry::default()
            .call("export", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert!(
            matches!(err, crate::EngineFault::BadArgs(ref m) if m.contains("path")),
            "expected BadArgs for missing path, got {err:?}"
        );
    }

    #[test]
    fn missing_input_file_is_business_input_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope.md");
        let outcome = Registry::default()
            .call(
                "export",
                &Ctx::headless(),
                &json!({ "path": missing.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "input_not_found");
    }

    #[test]
    fn directory_input_is_business_input_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = Registry::default()
            .call(
                "export",
                &Ctx::headless(),
                &json!({ "path": dir.path().to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "input_not_found");
    }

    #[test]
    fn nonexistent_output_directory_is_business_bad_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("doc.md");
        std::fs::write(&input, "# Hi\n").expect("seed input");
        let bad_out = dir.path().join("no-such-dir").join("doc.docx");
        let outcome = Registry::default()
            .call(
                "export",
                &Ctx::headless(),
                &json!({
                    "path": input.to_string_lossy(),
                    "output": bad_out.to_string_lossy()
                }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "bad_output");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_output_target_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("doc.md");
        std::fs::write(&input, "# Hi\n").expect("seed input");
        // Pre-plant a symlink where the output would land — the tool must
        // refuse rather than write through it.
        let victim = dir.path().join("victim.txt");
        std::fs::write(&victim, "precious").expect("seed victim");
        let out = dir.path().join("doc.docx");
        std::os::unix::fs::symlink(&victim, &out).expect("plant symlink");

        let outcome = Registry::default()
            .call(
                "export",
                &Ctx::headless(),
                &json!({
                    "path": input.to_string_lossy(),
                    "output": out.to_string_lossy()
                }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "symlinked_output");
        // The symlink target was never touched.
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
    }

    /// End-to-end through a MOCK exporter (never the real scrybe-docx): a tiny
    /// shell script pointed at by SCRYBE_DOCX_BIN writes its `-o` argument.
    /// The only test that touches the env var, so parallel tests don't race.
    #[cfg(unix)]
    #[test]
    fn export_runs_the_resolved_exporter_and_defaults_output() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("doc.md");
        std::fs::write(&input, "# Hi\n").expect("seed input");

        let script = dir.path().join("fake-scrybe-docx");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'FAKE-DOCX %s' \"$1\" > \"$3\"\n",
        )
        .expect("write mock exporter");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod +x");

        std::env::set_var("SCRYBE_DOCX_BIN", &script);
        let outcome = Registry::default()
            .call(
                "export",
                &Ctx::headless(),
                &json!({ "path": input.to_string_lossy() }),
            )
            .expect("dispatch");
        std::env::remove_var("SCRYBE_DOCX_BIN");

        assert!(outcome.is_ok(), "tool_error: {:?}", outcome.tool_error);
        assert_eq!(outcome.data["kind"], "export");
        // Default output: input with a .docx extension.
        let expected_out = input.with_extension("docx");
        assert_eq!(
            outcome.data["output"].as_str().unwrap(),
            expected_out.to_string_lossy()
        );
        let written = std::fs::read_to_string(&expected_out).expect("output written");
        assert!(
            written.starts_with("FAKE-DOCX"),
            "mock exporter ran: {written}"
        );
        assert!(written.contains("doc.md"), "input path forwarded as $1");
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
}
