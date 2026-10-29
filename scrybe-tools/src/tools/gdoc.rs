// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! `gdoc_auth`, `gdoc_list`, `gdoc_export`, `gdoc_import` — the Google Docs
//! integration (`scrybe-gdocs`) on the shared registry, so the CLI's
//! `scrybe gdoc …`, the MCP tools, and the app's File ▸ Google Docs controls
//! are one implementation.
//!
//! In-process (no live app needed). Every `scrybe-gdocs` entry point runs on
//! its own OS thread, so calling it from the MCP server's tokio runtime is
//! safe. A `GdocsError` is always a business failure: its stable `code()`
//! (`not_configured`, `not_logged_in`, `auth_failed`, `doc_not_found`,
//! `api_error`, `http_error`, `render_failed`, `invalid_argument`, `io_error`)
//! becomes the `tool_error` code verbatim.
//!
//! [`validate_export_path`] and [`validate_output`] are the path gates both
//! handlers run before touching the crate; the Tauri commands call the same
//! two, so the human controls and the agent tools refuse the same inputs.

use std::path::Path;

/// Extensions the app opens as its Markdown surface (`editor.ts`
/// `MARKDOWN_EXTS`). `.mmd` is a bare Mermaid diagram the app previews
/// inside a fence; export wraps it the same way.
const MARKDOWN_EXTS: [&str; 7] = ["md", "markdown", "mdown", "mkd", "mkdn", "mdx", "mmd"];

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

fn is_mermaid_file(path: &Path) -> bool {
    extension_of(path).as_deref() == Some("mmd")
}

fn symlinked(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|md| md.file_type().is_symlink())
}

use serde_json::{json, Value};

use crate::{Ctx, DataSchema, EngineFault, Facet, ToolError, ToolOutcome, ToolSpec};

/// Version of these tools' `data` payloads.
const DATA_VERSION: u32 = 1;

/// All Google Docs tools, in one call for registration.
pub(crate) fn specs() -> Vec<ToolSpec> {
    vec![auth_spec(), list_spec(), export_spec(), import_spec()]
}

/// A `scrybe-gdocs` failure as a business outcome: the crate's stable code
/// and human message, verbatim.
fn gdocs_fail(base: Value, e: &scrybe_gdocs::GdocsError) -> ToolOutcome {
    ToolOutcome::fail(base, ToolError::new(e.code(), e.to_string()))
}

// ── gdoc_auth ────────────────────────────────────────────────────────────────

fn auth_spec() -> ToolSpec {
    ToolSpec {
        name: "gdoc_auth",
        description: "Manage the Google sign-in that `gdoc_list`, `gdoc_export` \
             and `gdoc_import` use. `action` is one of: `status` — report the \
             cached-token state WITHOUT contacting Google (safe to call any \
             time); `login` — OPENS A BROWSER ON THE HOST for Google's consent \
             screen and caches the token (blocks until the user approves, \
             denies, or the flow times out — only call it when a human is \
             present); `logout` — revoke and forget the cached token. Scrybe \
             asks for the `drive.file` scope only. Requires a bring-your-own \
             OAuth client (docs/google-docs.md); `not_configured` means none is \
             set up yet. Human equivalent: File ▸ Google Docs ▸ Sign in / Sign \
             out. Returns `{ status: logged_in|logged_out|not_configured, \
             expires_at (RFC 3339 or null), config_dir }`.",
        input_schema: auth_input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: auth_data_schema,
        },
        // `login` and `logout` change the cached token (and `login` opens a
        // browser); `status` alone would be read-only, but the flag is per
        // tool, so it must be honest for the worst case.
        mutates: true,
        facet: Facet::Integrations,
        handler: auth_handler,
    }
}

fn auth_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["login", "logout", "status"],
                "description": "`status` (offline check), `login` (opens a browser on the host), or `logout`."
            }
        },
        "required": ["action"]
    })
}

fn auth_data_schema() -> Value {
    crate::schema::envelope(
        "gdoc_auth",
        DATA_VERSION,
        json!({
            "status": {
                "type": "string",
                "enum": ["logged_in", "logged_out", "not_configured"],
                "description": "Sign-in state after the action."
            },
            "expires_at": {
                "type": ["string", "null"],
                "description": "RFC 3339 expiry of the cached access token (refreshed automatically); null unless logged_in."
            },
            "config_dir": {
                "type": "string",
                "description": "Directory holding client.json / token.json."
            }
        }),
        &["status", "expires_at", "config_dir"],
    )
}

fn auth_handler(_ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
    // Required args are gated by the dispatcher; the enum is not, so an
    // unknown action is rejected here as a business error.
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let base = json!({ "v": DATA_VERSION, "kind": "gdoc_auth" });
    let result = match action {
        "login" => scrybe_gdocs::login(),
        "logout" => scrybe_gdocs::logout(),
        "status" => scrybe_gdocs::status(),
        other => {
            return Ok(ToolOutcome::fail(
                base,
                ToolError::new(
                    "invalid_argument",
                    format!("unknown action {other:?}: expected login, logout or status"),
                ),
            ))
        }
    };
    Ok(match result {
        Ok(st) => ToolOutcome::ok(json!({
            "v": DATA_VERSION,
            "kind": "gdoc_auth",
            "status": st.status,
            "expires_at": st.expires_at,
            "config_dir": st.config_dir,
        })),
        Err(e) => gdocs_fail(base, &e),
    })
}

// ── gdoc_list ────────────────────────────────────────────────────────────────

fn list_spec() -> ToolSpec {
    ToolSpec {
        name: "gdoc_list",
        description: "List the Google Docs Scrybe created, newest first — the \
             candidates for `gdoc_import`. The `drive.file` scope sees only \
             docs Scrybe made, never the rest of the user's Drive. Requires a \
             signed-in session (`gdoc_auth`); contacts Google. No input. Human \
             equivalent: the File ▸ Import from Google Docs… picker list. \
             Returns `{ count, docs: [{ id, name, url, modified_time }] }`.",
        input_schema: || json!({ "type": "object", "properties": {} }),
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: list_data_schema,
        },
        mutates: false,
        facet: Facet::Integrations,
        handler: list_handler,
    }
}

fn list_data_schema() -> Value {
    crate::schema::envelope(
        "gdoc_list",
        DATA_VERSION,
        json!({
            "count": { "type": "integer" },
            "docs": {
                "type": "array",
                "description": "Newest first.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Drive file id — pass to `gdoc_import`." },
                        "name": { "type": "string" },
                        "url": { "type": "string", "description": "Opens the doc in a browser." },
                        "modified_time": { "type": ["string", "null"], "description": "RFC 3339, when Drive reports it." }
                    },
                    "required": ["id", "name", "url", "modified_time"]
                }
            }
        }),
        &["count", "docs"],
    )
}

fn list_handler(_ctx: &Ctx, _args: &Value) -> Result<ToolOutcome, EngineFault> {
    let base = json!({ "v": DATA_VERSION, "kind": "gdoc_list" });
    Ok(match scrybe_gdocs::list_docs() {
        Ok(docs) => ToolOutcome::ok(json!({
            "v": DATA_VERSION,
            "kind": "gdoc_list",
            "count": docs.len(),
            "docs": docs,
        })),
        Err(e) => gdocs_fail(base, &e),
    })
}

// ── gdoc_export ──────────────────────────────────────────────────────────────

fn export_spec() -> ToolSpec {
    ToolSpec {
        name: "gdoc_export",
        description: "Export a Markdown file ON DISK to a native Google Doc in \
             the Scrybe folder of the user's Drive. Mermaid fences become PNG \
             figures that carry their source (iTXt metadata + alt text), so \
             `gdoc_import` restores them. The first export of a file creates \
             the doc and writes a sidecar `<stem>.gdoc.json` beside it; later \
             exports UPDATE THAT SAME DOC IN PLACE (its URL and sharing \
             survive; delete the sidecar to force a fresh doc). Reads the file \
             from disk like `export` — unsaved editor buffers are not seen, \
             so `save` first. Input: `path` (a Markdown file: .md, .markdown, \
             .mdown, .mkd, .mkdn, .mdx, or a bare-Mermaid .mmd, which exports \
             as a one-figure doc); optional `title` overrides the doc name \
             (default: the first H1, then the file stem). Local images are \
             embedded only when they are files under the document's own \
             directory; others are left as written (Google shows them broken) \
             and listed in `images_skipped`. Requires a signed-in session \
             (`gdoc_auth`); contacts Google. Human equivalent: the ☁️ Google \
             Docs toolbar button / File ▸ Export to Google Docs…. Returns `{ \
             path, doc_id, url, folder_id, action: created|updated, figures, \
             images_skipped }`.",
        input_schema: export_input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: export_data_schema,
        },
        mutates: true,
        facet: Facet::Integrations,
        handler: export_handler,
    }
}

fn export_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path to the Markdown file on disk to export (.md, .markdown, .mdown, .mkd, .mkdn, .mdx, or a bare-Mermaid .mmd)." },
            "title": { "type": "string", "description": "Doc name override (default: first H1, then the file stem)." }
        },
        "required": ["path"]
    })
}

fn export_data_schema() -> Value {
    crate::schema::envelope(
        "gdoc_export",
        DATA_VERSION,
        json!({
            "path": { "type": "string", "description": "The Markdown file that was exported." },
            "doc_id": { "type": "string", "description": "Drive file id of the Google Doc." },
            "url": { "type": "string", "description": "Opens the doc in a browser." },
            "folder_id": { "type": "string", "description": "The Scrybe folder in Drive that holds the doc." },
            "action": {
                "type": "string",
                "enum": ["created", "updated"],
                "description": "`updated` when the sidecar's doc still existed and was rewritten in place."
            },
            "figures": { "type": "integer", "description": "Mermaid diagrams rendered to PNG." },
            "images_skipped": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Local images left as written because they could not be embedded (outside the document's directory, absolute, missing, or not an image); Google shows them as broken images."
            }
        }),
        &[
            "path",
            "doc_id",
            "url",
            "folder_id",
            "action",
            "figures",
            "images_skipped",
        ],
    )
}

fn export_handler(_ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
    // Required args are gated by the dispatcher.
    let path = args.get("path").and_then(Value::as_str).unwrap_or_default();
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string);
    let base = json!({ "v": DATA_VERSION, "kind": "gdoc_export", "path": path });

    if let Some(err) = validate_export_path(path) {
        return Ok(ToolOutcome::fail(base, err));
    }

    let file = Path::new(path).to_path_buf();
    let result = if is_mermaid_file(&file) {
        // A bare diagram exports as the one-figure doc the app previews.
        read_file(&file).and_then(|source| {
            scrybe_gdocs::export_markdown_source(fence_mermaid(&source), file, title)
        })
    } else {
        scrybe_gdocs::export_markdown_file(file, title)
    };
    Ok(match result {
        Ok(r) => ToolOutcome::ok(json!({
            "v": DATA_VERSION,
            "kind": "gdoc_export",
            "path": r.path,
            "doc_id": r.doc_id,
            "url": r.url,
            "folder_id": r.folder_id,
            "action": r.action,
            "figures": r.figures,
            "images_skipped": r.images_skipped,
        })),
        Err(e) => gdocs_fail(base, &e),
    })
}

/// Read a document, naming it in the `io_error` like the crate does.
fn read_file(path: &Path) -> scrybe_gdocs::Result<String> {
    std::fs::read_to_string(path).map_err(|e| {
        scrybe_gdocs::GdocsError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", path.display()),
        ))
    })
}

/// A `.mmd` file's content as the Markdown the app previews it as.
fn fence_mermaid(source: &str) -> String {
    format!("```mermaid\n{source}\n```\n")
}

/// Validate an export source: a Markdown path (see [`MARKDOWN_EXTS`]) whose
/// sidecar `<stem>.gdoc.json` is not a symlink — the crate reads it to find
/// the doc to update and rewrites it after the upload. Returns the business
/// error to report, or `None` when acceptable. The Tauri `gdoc_export`
/// command runs this same gate.
pub fn validate_export_path(path: &str) -> Option<ToolError> {
    let file = Path::new(path);
    let is_markdown = extension_of(file).is_some_and(|e| MARKDOWN_EXTS.contains(&e.as_str()));
    if !is_markdown {
        return Some(ToolError::new(
            "invalid_argument",
            format!(
                "path must be a Markdown file ({}): {path}",
                MARKDOWN_EXTS.map(|e| format!(".{e}")).join(", ")
            ),
        ));
    }
    let sidecar = scrybe_gdocs::sidecar::path_for(file);
    if symlinked(&sidecar) {
        return Some(ToolError::new(
            "invalid_argument",
            format!(
                "refusing to read or write through a symlinked sidecar: {}",
                sidecar.display()
            ),
        ));
    }
    None
}

// ── gdoc_import ──────────────────────────────────────────────────────────────

fn import_spec() -> ToolSpec {
    ToolSpec {
        name: "gdoc_import",
        description: "Import a Google Doc as Markdown. Input: `doc` (a doc id, \
             or a docs.google.com / drive.google.com URL) and `output` (the \
             .md/.markdown path to write; its directory must already exist; \
             an existing file is overwritten). Mermaid PNGs that carry their \
             source become ```mermaid fences again (`figures[].verification` \
             says which carrier vouched for each: `verified` iTXt + matching \
             sha256, `unverified` iTXt without a digest, `alt-text`, or the \
             sidecar `manifest`); other images are written beside the output \
             as `<stem>_img_NN.<ext>`. A sidecar `<stem>.gdoc.json` links the \
             file to the doc so a later `gdoc_export` updates it in place. \
             Requires a signed-in session (`gdoc_auth`); contacts Google; with \
             `drive.file` only docs Scrybe created are visible (`gdoc_list`). \
             Human equivalent: File ▸ Import from Google Docs…. Returns `{ \
             path, doc_id, diagrams, images, figures: [{ index, verification \
             }], image_paths }`.",
        input_schema: import_input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: import_data_schema,
        },
        mutates: true,
        facet: Facet::Integrations,
        handler: import_handler,
    }
}

fn import_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "doc": { "type": "string", "description": "Google Doc id, or a docs.google.com / drive.google.com URL." },
            "output": { "type": "string", "description": "Markdown path to write (.md or .markdown); its directory must exist." }
        },
        "required": ["doc", "output"]
    })
}

fn import_data_schema() -> Value {
    crate::schema::envelope(
        "gdoc_import",
        DATA_VERSION,
        json!({
            "path": { "type": "string", "description": "The Markdown file that was written." },
            "doc_id": { "type": "string" },
            "diagrams": { "type": "integer", "description": "Mermaid fences restored." },
            "images": { "type": "integer", "description": "Non-Mermaid images written as sibling files." },
            "figures": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "index": { "type": "integer", "description": "0-based position among the doc's images." },
                        "verification": {
                            "type": "string",
                            "enum": ["verified", "unverified", "alt-text", "manifest"],
                            "description": "Which carrier the Mermaid source was recovered from."
                        }
                    },
                    "required": ["index", "verification"]
                }
            },
            "image_paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Sibling image files written beside the output."
            }
        }),
        &[
            "path",
            "doc_id",
            "diagrams",
            "images",
            "figures",
            "image_paths",
        ],
    )
}

fn import_handler(_ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
    // Required args are gated by the dispatcher.
    let doc = args.get("doc").and_then(Value::as_str).unwrap_or_default();
    let output = args
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let base = json!({ "v": DATA_VERSION, "kind": "gdoc_import", "doc": doc, "output": output });

    // The crate writes wherever it is pointed (and would create directory
    // trees); gate the target here so a bad call fails before any network
    // round-trip and the write can't be redirected.
    if let Some(err) = validate_output(output) {
        return Ok(ToolOutcome::fail(base, err));
    }

    Ok(
        match scrybe_gdocs::import_doc(doc.to_string(), Path::new(output).to_path_buf()) {
            Ok(r) => ToolOutcome::ok(json!({
                "v": DATA_VERSION,
                "kind": "gdoc_import",
                "path": r.path,
                "doc_id": r.doc_id,
                "diagrams": r.diagrams,
                "images": r.images,
                "figures": r.figures,
                "image_paths": r.image_paths,
            })),
            Err(e) => gdocs_fail(base, &e),
        },
    )
}

/// Validate the import target: a `.md`/`.markdown` path whose parent directory
/// already exists and that is not a symlink, nor is its sidecar. Returns the
/// business error to report, or `None` when acceptable. The Tauri
/// `gdoc_import` command runs this same gate.
pub fn validate_output(output: &str) -> Option<ToolError> {
    let out = Path::new(output);
    let is_markdown = extension_of(out).is_some_and(|e| e == "md" || e == "markdown");
    if !is_markdown {
        return Some(ToolError::new(
            "invalid_argument",
            format!("output must be a .md or .markdown path: {output}"),
        ));
    }

    // The tool writes a file; it does not invent directory trees.
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.is_dir() {
            return Some(ToolError::new(
                "invalid_argument",
                format!(
                    "output directory does not exist: {}",
                    parent.to_string_lossy()
                ),
            ));
        }
    }

    // Refuse a symlinked output target or sidecar: writing through a
    // pre-planted link would land the doc somewhere the caller never named.
    if symlinked(out) {
        return Some(ToolError::new(
            "invalid_argument",
            format!("refusing to write through a symlinked output target: {output}"),
        ));
    }
    let sidecar = scrybe_gdocs::sidecar::path_for(out);
    if symlinked(&sidecar) {
        return Some(ToolError::new(
            "invalid_argument",
            format!(
                "refusing to read or write through a symlinked sidecar: {}",
                sidecar.display()
            ),
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry};

    const ALL: [&str; 4] = ["gdoc_auth", "gdoc_list", "gdoc_export", "gdoc_import"];

    fn required(schema: &Value) -> Vec<&str> {
        schema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    #[test]
    fn all_four_registered_in_the_integrations_facet() {
        let reg = Registry::default();
        for name in ALL {
            let s = reg
                .get(name)
                .unwrap_or_else(|| panic!("missing tool: {name}"));
            assert_eq!(s.facet, Facet::Integrations, "tool {name}");
            assert_eq!(s.data_schema.version, DATA_VERSION, "tool {name}");
            assert!(
                s.description.contains("Human equivalent"),
                "tool {name} must name its human twin"
            );
        }
        assert!(
            reg.names()
                .iter()
                .filter(|n| n.starts_with("gdoc_"))
                .count()
                == 4
        );
    }

    #[test]
    fn mutability_flags_are_honest() {
        let reg = Registry::default();
        for (name, mutates) in [
            ("gdoc_auth", true),
            ("gdoc_list", false),
            ("gdoc_export", true),
            ("gdoc_import", true),
        ] {
            assert_eq!(reg.get(name).unwrap().mutates, mutates, "tool {name}");
        }
    }

    #[test]
    fn input_schemas_are_objects_with_the_documented_required_keys() {
        let reg = Registry::default();
        for (name, req) in [
            ("gdoc_auth", vec!["action"]),
            ("gdoc_list", vec![]),
            ("gdoc_export", vec!["path"]),
            ("gdoc_import", vec!["doc", "output"]),
        ] {
            let schema = (reg.get(name).unwrap().input_schema)();
            assert_eq!(schema["type"], "object", "tool {name}");
            assert!(schema["properties"].is_object(), "tool {name}");
            assert_eq!(required(&schema), req, "tool {name}");
        }
        let auth = (reg.get("gdoc_auth").unwrap().input_schema)();
        assert_eq!(
            auth["properties"]["action"]["enum"],
            json!(["login", "logout", "status"])
        );
    }

    #[test]
    fn data_schemas_are_envelopes_with_the_report_fields() {
        let reg = Registry::default();
        for (name, keys) in [
            ("gdoc_auth", vec!["status", "expires_at", "config_dir"]),
            ("gdoc_list", vec!["count", "docs"]),
            (
                "gdoc_export",
                vec![
                    "path",
                    "doc_id",
                    "url",
                    "folder_id",
                    "action",
                    "figures",
                    "images_skipped",
                ],
            ),
            (
                "gdoc_import",
                vec![
                    "path",
                    "doc_id",
                    "diagrams",
                    "images",
                    "figures",
                    "image_paths",
                ],
            ),
        ] {
            let schema = (reg.get(name).unwrap().data_schema.schema)();
            assert_eq!(schema["type"], "object", "tool {name}");
            assert_eq!(schema["properties"]["kind"]["const"], name);
            assert_eq!(schema["properties"]["v"]["const"], DATA_VERSION);
            let req = required(&schema);
            for key in keys {
                assert!(
                    schema["properties"][key].is_object(),
                    "tool {name}: property {key}"
                );
                assert!(req.contains(&key), "tool {name}: {key} required");
            }
        }
    }

    #[test]
    fn auth_missing_action_is_engine_fault() {
        let err = Registry::default()
            .call("gdoc_auth", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert!(
            matches!(err, EngineFault::BadArgs(ref m) if m.contains("action")),
            "expected BadArgs for missing action, got {err:?}"
        );
    }

    #[test]
    fn auth_unknown_action_is_business_invalid_argument() {
        // The dispatcher gates `required` only; the enum is the handler's job.
        let outcome = Registry::default()
            .call(
                "gdoc_auth",
                &Ctx::headless(),
                &json!({ "action": "frobnicate" }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.data["kind"], "gdoc_auth");
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "invalid_argument");
        assert!(err.message.contains("frobnicate"), "{}", err.message);
    }

    /// The only test in this crate that touches SCRYBE_GOOGLE_DIR: a config
    /// dir with a client but no token, so the crate reports `not_logged_in`
    /// before any network access — whatever client env vars the developer
    /// has set (a client.json makes the outcome independent of them; with
    /// none at all the crate would say `not_configured` first).
    #[test]
    fn list_without_a_token_is_business_not_logged_in() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            scrybe_gdocs::config::client_path(dir.path()),
            r#"{"installed":{"client_id":"cid","client_secret":"sec"}}"#,
        )
        .expect("seed client.json");
        std::env::set_var(scrybe_gdocs::config::DIR_ENV, dir.path());
        let outcome = Registry::default()
            .call("gdoc_list", &Ctx::headless(), &json!({}))
            .expect("dispatch");
        std::env::remove_var(scrybe_gdocs::config::DIR_ENV);
        assert!(!outcome.is_ok());
        assert_eq!(outcome.data["kind"], "gdoc_list");
        assert_eq!(outcome.tool_error.unwrap().code, "not_logged_in");
    }

    #[test]
    fn import_rejects_a_bad_doc_ref_as_invalid_argument() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("imported.md");
        let outcome = Registry::default()
            .call(
                "gdoc_import",
                &Ctx::headless(),
                &json!({ "doc": "not a doc ref", "output": out.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.data["kind"], "gdoc_import");
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "invalid_argument");
        assert!(err.message.contains("not a doc ref"), "{}", err.message);
        assert!(!out.exists(), "nothing written for a rejected call");
    }

    #[test]
    fn import_rejects_a_non_markdown_output_before_calling_the_crate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("imported.txt");
        let outcome = Registry::default()
            .call(
                "gdoc_import",
                &Ctx::headless(),
                // A well-formed doc ref, so only the output gate can reject.
                &json!({
                    "doc": "1DocAbcdefghijklmnopqrstuvwxyz",
                    "output": out.to_string_lossy()
                }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "invalid_argument");
        assert!(err.message.contains(".md"), "{}", err.message);
    }

    #[test]
    fn import_rejects_a_missing_output_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("no-such-dir").join("imported.md");
        let outcome = Registry::default()
            .call(
                "gdoc_import",
                &Ctx::headless(),
                &json!({
                    "doc": "1DocAbcdefghijklmnopqrstuvwxyz",
                    "output": out.to_string_lossy()
                }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "invalid_argument");
        assert!(err.message.contains("directory"), "{}", err.message);
    }

    #[cfg(unix)]
    #[test]
    fn import_refuses_a_symlinked_output_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let victim = dir.path().join("victim.md");
        std::fs::write(&victim, "precious").expect("seed victim");
        let out = dir.path().join("imported.md");
        std::os::unix::fs::symlink(&victim, &out).expect("plant symlink");

        let outcome = Registry::default()
            .call(
                "gdoc_import",
                &Ctx::headless(),
                &json!({
                    "doc": "1DocAbcdefghijklmnopqrstuvwxyz",
                    "output": out.to_string_lossy()
                }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "invalid_argument");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
    }

    #[cfg(unix)]
    #[test]
    fn import_refuses_a_symlinked_sidecar() {
        let dir = tempfile::tempdir().expect("tempdir");
        let victim = dir.path().join("victim.json");
        std::fs::write(&victim, "precious").expect("seed victim");
        let out = dir.path().join("imported.md");
        std::os::unix::fs::symlink(&victim, dir.path().join("imported.gdoc.json"))
            .expect("plant symlink");

        let outcome = Registry::default()
            .call(
                "gdoc_import",
                &Ctx::headless(),
                &json!({
                    "doc": "1DocAbcdefghijklmnopqrstuvwxyz",
                    "output": out.to_string_lossy()
                }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "invalid_argument");
        assert!(err.message.contains("sidecar"), "{}", err.message);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
        assert!(!out.exists());
    }

    #[test]
    fn validate_output_accepts_markdown_extensions_case_insensitively() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["a.md", "b.markdown", "c.MD", "d.Markdown"] {
            let p = dir.path().join(name);
            assert!(
                validate_output(&p.to_string_lossy()).is_none(),
                "{name} accepted"
            );
        }
        // A bare file name has no parent to check.
        assert!(validate_output("notes.md").is_none());
    }

    #[test]
    fn export_rejects_a_non_markdown_path_before_calling_the_crate() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["photo.png", "main.rs", "README"] {
            let p = dir.path().join(name);
            std::fs::write(&p, b"x").expect("seed file");
            let outcome = Registry::default()
                .call(
                    "gdoc_export",
                    &Ctx::headless(),
                    &json!({ "path": p.to_string_lossy() }),
                )
                .expect("dispatch");
            assert!(!outcome.is_ok(), "{name}");
            let err = outcome.tool_error.unwrap();
            assert_eq!(err.code, "invalid_argument", "{name}");
            assert!(err.message.contains(".md"), "{name}: {}", err.message);
            assert!(
                !scrybe_gdocs::sidecar::path_for(&p).exists(),
                "{name}: no sidecar beside a non-Markdown file"
            );
        }
    }

    #[test]
    fn validate_export_path_accepts_the_apps_markdown_extensions() {
        for name in [
            "a.md",
            "b.markdown",
            "c.MD",
            "d.mdown",
            "e.mkd",
            "f.mkdn",
            "g.mdx",
            "h.mmd",
        ] {
            assert!(validate_export_path(name).is_none(), "{name} accepted");
        }
        assert!(is_mermaid_file(Path::new("diagram.MMD")));
        assert!(!is_mermaid_file(Path::new("notes.md")));
        assert_eq!(
            fence_mermaid("graph TD; A-->B"),
            "```mermaid\ngraph TD; A-->B\n```\n"
        );
    }

    /// A planted `<stem>.gdoc.json` link is refused here, before the crate
    /// would read it to pick the doc to update or write through it.
    #[cfg(unix)]
    #[test]
    fn export_refuses_a_symlinked_sidecar() {
        let dir = tempfile::tempdir().expect("tempdir");
        let victim = dir.path().join("victim.json");
        std::fs::write(&victim, "precious").expect("seed victim");
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "# Notes\n").expect("seed doc");
        std::os::unix::fs::symlink(&victim, dir.path().join("notes.gdoc.json"))
            .expect("plant symlink");

        let outcome = Registry::default()
            .call(
                "gdoc_export",
                &Ctx::headless(),
                &json!({ "path": doc.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.data["kind"], "gdoc_export");
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "invalid_argument");
        assert!(err.message.contains("sidecar"), "{}", err.message);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
    }

    #[test]
    fn import_missing_args_are_engine_faults() {
        let reg = Registry::default();
        let err = reg
            .call("gdoc_import", &Ctx::headless(), &json!({ "doc": "x" }))
            .unwrap_err();
        assert!(matches!(err, EngineFault::BadArgs(ref m) if m.contains("output")));
        let err = reg
            .call(
                "gdoc_import",
                &Ctx::headless(),
                &json!({ "output": "x.md" }),
            )
            .unwrap_err();
        assert!(matches!(err, EngineFault::BadArgs(ref m) if m.contains("doc")));
    }

    #[test]
    fn export_of_a_missing_file_is_business_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope.md");
        let outcome = Registry::default()
            .call(
                "gdoc_export",
                &Ctx::headless(),
                &json!({ "path": missing.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.data["kind"], "gdoc_export");
        assert_eq!(
            outcome.data["path"].as_str().unwrap(),
            missing.to_string_lossy()
        );
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "io_error");
        assert!(err.message.contains("nope.md"), "{}", err.message);
    }

    #[test]
    fn export_missing_path_is_engine_fault() {
        let err = Registry::default()
            .call("gdoc_export", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert!(matches!(err, EngineFault::BadArgs(ref m) if m.contains("path")));
    }

    #[test]
    fn gdocs_errors_map_code_and_message_verbatim() {
        let e = scrybe_gdocs::GdocsError::Api {
            status: 403,
            message: "forbidden".into(),
        };
        let outcome = gdocs_fail(json!({ "v": 1, "kind": "gdoc_list" }), &e);
        assert!(!outcome.is_ok());
        let err = outcome.tool_error.unwrap();
        assert_eq!(err.code, "api_error");
        assert_eq!(err.message, "Google API error 403: forbidden");
    }
}
