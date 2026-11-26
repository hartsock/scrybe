// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use scrybe_cli::rpc_client;

// ---------------------------------------------------------------------------
// Phase 2 read-side handlers
// ---------------------------------------------------------------------------

pub(crate) fn require_running_gui<F: FnOnce(serde_json::Value) -> anyhow::Result<()>>(
    method: &str,
    params: serde_json::Value,
    on_ok: F,
) -> anyhow::Result<()> {
    // The client types every failure (A3): in-band app errors are `Remote`,
    // no-app is `is_not_running()`, and malformed replies arrive pre-typed
    // (envelope/frame/JSON violations) — no message-text matching anywhere.
    match rpc_client::send(method, params) {
        Ok(result) => on_ok(result),
        Err(scrybe_rpc::ClientError::Remote(e)) => {
            anyhow::bail!("scrybe {method}: {} ({})", e.message, e.code)
        }
        Err(e) if e.is_not_running() => {
            anyhow::bail!(
                "scrybe {method}: no Scrybe running — start the app first, or open the file with `scrybe <path>`"
            )
        }
        Err(e) => anyhow::bail!("scrybe {method}: {e}"),
    }
}

pub(crate) fn handle_read(path: &std::path::Path, json: bool) -> anyhow::Result<()> {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    require_running_gui(
        "read",
        serde_json::json!({"path": canon.to_string_lossy()}),
        |result| {
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let content = result["content"].as_str().unwrap_or("");
                print!("{content}");
            }
            Ok(())
        },
    )
}

pub(crate) fn handle_find(
    pattern: &str,
    paths: &[std::path::PathBuf],
    literal: bool,
    case_sensitive: bool,
    json: bool,
) -> anyhow::Result<()> {
    let path_strs: Vec<String> = paths
        .iter()
        .map(|p| {
            p.canonicalize()
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    require_running_gui(
        "find",
        serde_json::json!({
            "pattern": pattern,
            "paths": path_strs,
            "literal": literal,
            "case_sensitive": case_sensitive,
        }),
        |result| {
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let hits = result["hits"].as_array().cloned().unwrap_or_default();
                for hit in hits {
                    let path = hit["path"].as_str().unwrap_or("?");
                    let line = hit["line"].as_u64().unwrap_or(0);
                    let column = hit["column"].as_u64().unwrap_or(0);
                    let text = hit["text"].as_str().unwrap_or("");
                    println!("{path}:{line}:{column}: {text}");
                }
            }
            Ok(())
        },
    )
}

pub(crate) fn handle_section(
    path: &std::path::Path,
    heading: &str,
    json: bool,
) -> anyhow::Result<()> {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    require_running_gui(
        "section",
        serde_json::json!({"path": canon.to_string_lossy(), "heading": heading}),
        |result| {
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let content = result["content"].as_str().unwrap_or("");
                print!("{content}");
            }
            Ok(())
        },
    )
}

pub(crate) fn handle_edit(
    path: &std::path::Path,
    start_line: u32,
    end_line: u32,
    content: &str,
    json: bool,
) -> anyhow::Result<()> {
    if start_line == 0 || end_line == 0 || end_line < start_line {
        anyhow::bail!("scrybe edit: start_line and end_line must be ≥ 1 and start ≤ end");
    }
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // `--content -` reads from stdin (so multi-line content can be piped in).
    let resolved_content = if content == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        content.to_string()
    };
    require_running_gui(
        "edit",
        serde_json::json!({
            "path": canon.to_string_lossy(),
            "start_line": start_line,
            "end_line": end_line,
            "content": resolved_content,
        }),
        |result| {
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let applied = result["applied"].as_bool().unwrap_or(false);
                println!("{}", if applied { "ok" } else { "no-op" });
            }
            Ok(())
        },
    )
}
