// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Export every Mermaid diagram in a document to sibling PNG figures.
//!
//! For `foo.md`, figures are written next to the document as
//! `foo_fig_01.png`, `foo_fig_02.png`, … numbered 1-based in DOCUMENT order.
//! The zero-pad width is `max(2, digits(total))` so a listing sorts the
//! figures adjacent to their parent doc (because `_` (0x5F) sorts after `.`
//! (0x2E), `foo.md` immediately precedes its `foo_fig_NN.png` siblings).
//!
//! Each PNG embeds its Mermaid source (per-artifact UUID + SHA-256) via the
//! same `render_png` → `embed_with_uuid` path as the `mermaid_to_png` tool,
//! so the diagrams are losslessly round-trippable with `extract`.
//!
//! [`plan_figures`] is PURE (enumeration + naming, no IO); [`export_figures`]
//! is the IO shell (render + embed + write).

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};
use scrybe_core::Ast;
use scrybe_mermaid_render::{render_png, source_sha256};
use serde_json::{json, Value};

use crate::{Ctx, DataSchema, EngineFault, Facet, ToolError, ToolOutcome, ToolSpec};

/// Version of the `export_figures` tool's `data` payload.
const DATA_VERSION: u32 = 1;
const MAX_CAPTURED_PNG_BYTES: usize = 8 * 1024 * 1024;
const MAX_MERMAID_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_EMBEDDED_PNG_BYTES: usize = 16 * 1024 * 1024;

/// A planned figure: where its PNG will be written and the Mermaid source it
/// renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FigurePlan {
    /// Sibling PNG path: `<stem>_fig_<NN>.png` in the document's directory.
    pub path: PathBuf,
    /// The Mermaid source for this figure.
    pub source: String,
}

/// The result of writing one figure.
#[derive(Debug, Clone)]
pub struct FigureResult {
    /// The PNG path that was written (as a lossy UTF-8 string).
    pub path: String,
    /// The per-artifact UUID embedded in the PNG.
    pub uuid: String,
    /// SHA-256 of the Mermaid source.
    pub sha256: String,
    /// Size of the written PNG in bytes.
    pub bytes: usize,
}

struct EmbeddedPng {
    uuid: String,
    sha256: String,
    bytes: Vec<u8>,
}

/// Embed Mermaid provenance into already-rendered PNG bytes and write them.
///
/// This is the WYSIWYG seam used by the desktop preview: JavaScript rasterizes
/// the live Mermaid.js SVG, then this helper adds the exact source, UUID, and
/// digest without re-rendering the image. Embedding completes in memory before
/// the destination is opened, so invalid PNG bytes cannot clobber an existing
/// file. A successful write deliberately replaces an existing destination.
pub fn write_embedded_png(
    png_bytes: &[u8],
    source: &str,
    output_path: &Path,
) -> anyhow::Result<FigureResult> {
    ensure!(
        png_bytes.len() <= MAX_CAPTURED_PNG_BYTES,
        "captured PNG is too large ({} bytes; limit is {} bytes)",
        png_bytes.len(),
        MAX_CAPTURED_PNG_BYTES
    );
    ensure!(
        source.len() <= MAX_MERMAID_SOURCE_BYTES,
        "Mermaid source is too large ({} bytes; limit is {} bytes)",
        source.len(),
        MAX_MERMAID_SOURCE_BYTES
    );
    let prepared = prepare_embedded_png(png_bytes, source)
        .with_context(|| format!("embed source for {}", output_path.display()))?;
    ensure!(
        prepared.bytes.len() <= MAX_EMBEDDED_PNG_BYTES,
        "source-bearing PNG is too large ({} bytes; limit is {} bytes)",
        prepared.bytes.len(),
        MAX_EMBEDDED_PNG_BYTES
    );
    atomic_replace(output_path, &prepared.bytes)?;
    Ok(FigureResult {
        path: output_path.to_string_lossy().into_owned(),
        uuid: prepared.uuid,
        sha256: prepared.sha256,
        bytes: prepared.bytes.len(),
    })
}

/// Write complete bytes to a same-directory temporary file, flush them, then
/// atomically replace the destination. A late write failure therefore leaves a
/// pre-existing image intact instead of truncating it.
fn atomic_replace(output_path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let directory = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let existing_permissions = match std::fs::metadata(output_path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("inspect {}", output_path.display()));
        }
    };
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Match a normal newly-created image (0666 filtered by umask), not
        // tempfile's private 0600 default.
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut temporary = builder
        .tempfile_in(directory)
        .with_context(|| format!("create temporary file in {}", directory.display()))?;
    temporary
        .write_all(bytes)
        .with_context(|| format!("write temporary PNG for {}", output_path.display()))?;
    if let Some(permissions) = existing_permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .with_context(|| format!("preserve permissions for {}", output_path.display()))?;
    }
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("flush temporary PNG for {}", output_path.display()))?;
    temporary
        .persist(output_path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace {}", output_path.display()))?;
    Ok(())
}

fn prepare_embedded_png(png_bytes: &[u8], source: &str) -> anyhow::Result<EmbeddedPng> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let bytes = scrybe_mermaid::embed_with_uuid(png_bytes, source, &uuid)?;
    let payload = scrybe_mermaid::extract(&bytes).context("verify embedded Mermaid source")?;
    ensure!(
        payload.source == source,
        "embedded Mermaid source did not round-trip"
    );
    ensure!(
        payload.uuid == uuid,
        "embedded Mermaid UUID did not round-trip"
    );
    ensure!(
        payload.is_verified(),
        "embedded Mermaid digest was not verified"
    );
    Ok(EmbeddedPng {
        uuid,
        sha256: source_sha256(source),
        bytes,
    })
}

/// Plan the sibling PNG figures for every Mermaid block in `doc_source`.
///
/// PURE — enumerates diagrams (via [`Ast::mermaid_blocks`]) and builds sibling
/// names in `doc_path`'s directory; performs no IO. Returns an empty vec when
/// the document has no Mermaid blocks. When `doc_path` has no parent directory
/// component (e.g. `foo.md`), figures are named relative to the current
/// directory.
pub fn plan_figures(doc_source: &str, doc_path: &Path) -> Vec<FigurePlan> {
    let ast = Ast::parse(doc_source);
    let blocks = ast.mermaid_blocks();
    let total = blocks.len();
    if total == 0 {
        return Vec::new();
    }
    let width = figure_width(total);
    let stem = doc_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    // A path like `foo.md` has an empty parent component; treat that as the
    // current directory so the figure name stays relative.
    let dir = doc_path.parent().filter(|p| !p.as_os_str().is_empty());
    blocks
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let name = format!("{stem}_fig_{:0width$}.png", i + 1, width = width);
            let path = match dir {
                Some(d) => d.join(&name),
                None => PathBuf::from(&name),
            };
            FigurePlan {
                path,
                source: (*source).to_string(),
            }
        })
        .collect()
}

/// Zero-pad width for `total` figures: `max(2, digits(total))`.
pub(crate) fn figure_width(total: usize) -> usize {
    total.to_string().len().max(2)
}

/// Render and write every planned figure. The IO shell around [`plan_figures`].
///
/// For each plan: `render_png` → `embed_with_uuid` (fresh UUID) → `fs::write`.
/// Uses the exact render+embed pattern of the `mermaid_to_png` tool, so every
/// written PNG carries its embedded source.
pub fn export_figures(doc_source: &str, doc_path: &Path) -> anyhow::Result<Vec<FigureResult>> {
    let plans = plan_figures(doc_source, doc_path);
    // Render + embed the whole set into memory FIRST, so a render/embed failure
    // aborts before we delete or write any file on disk.
    let mut prepared = Vec::with_capacity(plans.len());
    for plan in plans {
        // Same render → embed path as the `mermaid_to_png` tool.
        let png = render_png(&plan.source)
            .with_context(|| format!("render mermaid for {}", plan.path.display()))?;
        let embedded = prepare_embedded_png(&png, &plan.source)
            .with_context(|| format!("embed source for {}", plan.path.display()))?;
        prepared.push((plan.path, embedded));
    }
    // Prune this document's prior figure set before writing the fresh one, so a
    // re-export after the diagram count shrinks (5 → 2) or crosses a zero-pad
    // width boundary (…_fig_08 → …_fig_100) never leaves orphaned/duplicate
    // figures interleaved beside the document. Only done when there IS a new set
    // to write: exporting a diagram-less document is a no-op, never a deletion.
    if !prepared.is_empty() {
        prune_figures(doc_path)?;
    }
    let mut results = Vec::with_capacity(prepared.len());
    for (path, embedded) in prepared {
        std::fs::write(&path, &embedded.bytes)
            .with_context(|| format!("write {}", path.display()))?;
        results.push(FigureResult {
            path: path.to_string_lossy().into_owned(),
            uuid: embedded.uuid,
            sha256: embedded.sha256,
            bytes: embedded.bytes.len(),
        });
    }
    Ok(results)
}

/// Remove this document's existing auto-generated figures (`<stem>_fig_<NN>.png`
/// for any zero-pad width), so a re-export produces exactly the current set.
/// Only files matching the generated pattern for this document's stem are
/// touched; a missing directory is not an error.
fn prune_figures(doc_path: &Path) -> anyhow::Result<()> {
    let stem = doc_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let dir = match doc_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(d) => d.to_path_buf(),
        None => PathBuf::from("."),
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(anyhow::Error::new(e).context(format!("read dir {}", dir.display())));
        }
    };
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if is_figure_name(name, stem) {
                let p = entry.path();
                std::fs::remove_file(&p)
                    .with_context(|| format!("remove stale figure {}", p.display()))?;
            }
        }
    }
    Ok(())
}

/// True when `name` is an auto-generated figure for `stem`: exactly
/// `<stem>_fig_<NN>.png` where `NN` is one or more ASCII digits (any width).
pub(crate) fn is_figure_name(name: &str, stem: &str) -> bool {
    let Some(rest) = name.strip_prefix(stem) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix("_fig_") else {
        return false;
    };
    let Some(digits) = rest.strip_suffix(".png") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Tool spec
// ---------------------------------------------------------------------------

/// The `export_figures` tool spec (shared registry; `Mermaid` facet; mutating).
pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: "export_figures",
        description: "Export EVERY Mermaid diagram in a Markdown document to \
             sibling PNG figures. For `foo.md`, writes `foo_fig_01.png`, \
             `foo_fig_02.png`, … in the SAME directory, numbered 1-based in \
             document order (zero-padded so they sort next to the document). \
             Each PNG embeds its Mermaid source (a per-artifact UUID + the \
             source's SHA-256), so the diagrams are losslessly round-trippable \
             with `extract`. Re-exporting replaces the document's prior figure \
             set (stale `<stem>_fig_NN.png` siblings are pruned), so the output \
             always matches the current document. Input: `path` (the Markdown \
             document on disk). Returns `{ count, figures: [{ path, uuid, \
             sha256, bytes }] }`.",
        input_schema,
        data_schema: DataSchema {
            version: DATA_VERSION,
            schema: data_schema,
        },
        mutates: true,
        facet: Facet::Mermaid,
        handler,
    }
}

fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Markdown document whose Mermaid diagrams to export."
            }
        },
        "required": ["path"]
    })
}

fn data_schema() -> Value {
    crate::schema::envelope(
        "export_figures",
        DATA_VERSION,
        json!({
            "count": { "type": "integer" },
            "figures": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "uuid": { "type": "string" },
                        "sha256": { "type": "string" },
                        "bytes": { "type": "integer" }
                    },
                    "required": ["path", "uuid", "sha256", "bytes"]
                }
            }
        }),
        &["count", "figures"],
    )
}

fn handler(_ctx: &Ctx, args: &Value) -> Result<ToolOutcome, EngineFault> {
    // Required args are gated by the dispatcher.
    let path = args.get("path").and_then(Value::as_str).unwrap_or_default();
    let base = json!({ "v": DATA_VERSION, "kind": "export_figures", "path": path });

    // Read the document from disk (a missing/unreadable file is a business
    // failure, not an engine fault).
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return Ok(ToolOutcome::fail(
                base,
                ToolError::new("read_failed", format!("could not read {path}: {e}")),
            ))
        }
    };

    Ok(match export_figures(&source, Path::new(path)) {
        Ok(figs) => {
            let figures: Vec<Value> = figs
                .iter()
                .map(|f| {
                    json!({
                        "path": f.path,
                        "uuid": f.uuid,
                        "sha256": f.sha256,
                        "bytes": f.bytes,
                    })
                })
                .collect();
            ToolOutcome::ok(json!({
                "v": DATA_VERSION,
                "kind": "export_figures",
                "count": figures.len(),
                "figures": figures,
            }))
        }
        Err(e) => ToolOutcome::fail(
            base,
            ToolError::new("export_failed", format!("could not export figures: {e}")),
        ),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ctx, Registry};

    // -- pure: figure_width --------------------------------------------------

    #[test]
    fn width_is_at_least_two() {
        assert_eq!(figure_width(1), 2);
        assert_eq!(figure_width(2), 2);
        assert_eq!(figure_width(9), 2);
        assert_eq!(figure_width(10), 2);
        assert_eq!(figure_width(99), 2);
    }

    #[test]
    fn width_grows_with_magnitude() {
        assert_eq!(figure_width(100), 3);
        assert_eq!(figure_width(120), 3);
        assert_eq!(figure_width(999), 3);
        assert_eq!(figure_width(1000), 4);
    }

    // -- pure: plan_figures --------------------------------------------------

    fn two_diagram_doc() -> &'static str {
        "# Report\n\n```mermaid\ngraph TD; A-->B\n```\n\n\
         Prose.\n\n```mermaid\ngraph LR; C-->D\n```\n"
    }

    #[test]
    fn plan_names_siblings_with_two_pad() {
        let plans = plan_figures(two_diagram_doc(), Path::new("/a/b/report.md"));
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].path, PathBuf::from("/a/b/report_fig_01.png"));
        assert_eq!(plans[1].path, PathBuf::from("/a/b/report_fig_02.png"));
    }

    #[test]
    fn plan_preserves_document_order() {
        let plans = plan_figures(two_diagram_doc(), Path::new("/a/b/report.md"));
        assert_eq!(plans[0].source, "graph TD; A-->B");
        assert_eq!(plans[1].source, "graph LR; C-->D");
    }

    #[test]
    fn plan_zero_blocks_is_empty() {
        let plans = plan_figures("# Just prose\n\nNo diagrams.\n", Path::new("/x/doc.md"));
        assert!(plans.is_empty());
    }

    #[test]
    fn plan_stem_comes_from_file_stem() {
        let plans = plan_figures(
            "```mermaid\ngraph TD; A-->B\n```\n",
            Path::new("/deep/nested/my.notes.md"),
        );
        // `file_stem` strips only the final extension → stem is `my.notes`.
        assert_eq!(
            plans[0].path,
            PathBuf::from("/deep/nested/my.notes_fig_01.png")
        );
    }

    #[test]
    fn plan_no_parent_uses_current_dir() {
        let plans = plan_figures("```mermaid\ngraph TD; A-->B\n```\n", Path::new("foo.md"));
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].path, PathBuf::from("foo_fig_01.png"));
    }

    #[test]
    fn plan_pads_to_three_for_many() {
        // Build a document with 100 diagrams; width must widen to 3.
        let mut src = String::new();
        for _ in 0..100 {
            src.push_str("```mermaid\ngraph TD; A-->B\n```\n\n");
        }
        let plans = plan_figures(&src, Path::new("/d/big.md"));
        assert_eq!(plans.len(), 100);
        assert_eq!(plans[0].path, PathBuf::from("/d/big_fig_001.png"));
        assert_eq!(plans[99].path, PathBuf::from("/d/big_fig_100.png"));
    }

    // -- IO shell: export_figures round-trip --------------------------------

    #[test]
    fn export_writes_embedded_pngs_that_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc_path = dir.path().join("report.md");
        let source = two_diagram_doc();

        let results = export_figures(source, &doc_path).expect("export");
        assert_eq!(results.len(), 2);

        let fig1 = dir.path().join("report_fig_01.png");
        let fig2 = dir.path().join("report_fig_02.png");
        assert!(fig1.exists(), "fig 1 written");
        assert!(fig2.exists(), "fig 2 written");

        for (fig, expected) in [(&fig1, "graph TD; A-->B"), (&fig2, "graph LR; C-->D")] {
            let bytes = std::fs::read(fig).expect("read png");
            assert!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "real PNG signature"
            );
            let payload = scrybe_mermaid::extract(&bytes).expect("extract embedded");
            assert_eq!(payload.source, expected, "source round-trips in order");
            assert!(uuid::Uuid::parse_str(&payload.uuid).is_ok(), "uuid parses");
        }
    }

    #[test]
    fn export_zero_blocks_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc_path = dir.path().join("plain.md");
        let results = export_figures("# No diagrams here\n", &doc_path).expect("export");
        assert!(results.is_empty());
        // No sibling PNGs materialized.
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert!(
            entries.is_empty(),
            "no files written for a diagram-free doc"
        );
    }

    #[test]
    fn captured_png_overwrites_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("chosen-name.png");
        std::fs::write(&output, b"old image").expect("seed destination");
        let source = "graph TD; Live-->Preview";
        let png = render_png(source).expect("render fixture");

        let result = write_embedded_png(&png, source, &output).expect("write captured png");
        let written = std::fs::read(&output).expect("read output");
        assert!(written.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(result.bytes, written.len());
        assert_eq!(result.sha256, source_sha256(source));
        assert!(uuid::Uuid::parse_str(&result.uuid).is_ok());

        let payload = scrybe_mermaid::extract(&written).expect("extract embedded source");
        assert_eq!(payload.source, source);
        assert_eq!(payload.uuid, result.uuid);
        assert!(matches!(
            payload.verification,
            scrybe_mermaid::VerificationStatus::Verified { .. }
        ));
    }

    #[test]
    fn invalid_captured_png_does_not_clobber_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("precious.png");
        std::fs::write(&output, b"keep me").expect("seed destination");

        let error = write_embedded_png(b"not a png", "graph TD; A-->B", &output)
            .expect_err("invalid PNG must fail");
        assert!(error.to_string().contains("embed source"));
        assert_eq!(std::fs::read(&output).unwrap(), b"keep me");
    }

    #[test]
    fn captured_png_without_iend_does_not_clobber_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("precious.png");
        std::fs::write(&output, b"keep me").expect("seed destination");
        let mut png = render_png("graph TD; A-->B").expect("render fixture");
        png.truncate(png.len() - 12);

        let error = write_embedded_png(&png, "graph TD; A-->B", &output)
            .expect_err("PNG without IEND must fail");
        assert!(error.to_string().contains("embed source"));
        assert_eq!(std::fs::read(&output).unwrap(), b"keep me");
    }

    #[test]
    fn oversized_captured_png_does_not_clobber_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("precious.png");
        std::fs::write(&output, b"keep me").expect("seed destination");
        let oversized = vec![0; MAX_CAPTURED_PNG_BYTES + 1];

        let error = write_embedded_png(&oversized, "graph TD; A-->B", &output)
            .expect_err("oversized PNG must fail");
        assert!(error.to_string().contains("captured PNG is too large"));
        assert_eq!(std::fs::read(&output).unwrap(), b"keep me");
    }

    #[cfg(unix)]
    #[test]
    fn captured_png_uses_normal_new_file_mode_and_preserves_overwrite_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let control = dir.path().join("control.png");
        std::fs::File::create(&control).expect("create mode control");
        let expected_new_mode = std::fs::metadata(&control)
            .expect("control metadata")
            .permissions()
            .mode()
            & 0o777;
        let source = "graph TD; A-->B";
        let png = render_png(source).expect("render fixture");
        let new_output = dir.path().join("new.png");

        write_embedded_png(&png, source, &new_output).expect("write new PNG");
        let new_mode = std::fs::metadata(&new_output)
            .expect("new metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(new_mode, expected_new_mode);

        std::fs::set_permissions(&new_output, std::fs::Permissions::from_mode(0o640))
            .expect("set distinctive mode");
        write_embedded_png(&png, source, &new_output).expect("overwrite PNG");
        let overwritten_mode = std::fs::metadata(&new_output)
            .expect("overwrite metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(overwritten_mode, 0o640);
    }

    #[test]
    fn captured_png_write_does_not_touch_siblings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("report_fig_01_flow.png");
        let sibling = dir.path().join("report_fig_02_keep.png");
        std::fs::write(&sibling, b"unrelated").expect("seed sibling");
        let source = "graph LR; A-->B";
        let png = render_png(source).expect("render fixture");

        write_embedded_png(&png, source, &output).expect("write selected image");
        assert_eq!(std::fs::read(&sibling).unwrap(), b"unrelated");
    }

    // -- tool spec + handler -------------------------------------------------

    #[test]
    fn spec_is_mutating_mermaid_facet() {
        let s = spec();
        assert_eq!(s.name, "export_figures");
        assert!(s.mutates);
        assert_eq!(s.facet, Facet::Mermaid);
    }

    #[test]
    fn tool_exports_from_disk_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc_path = dir.path().join("doc.md");
        std::fs::write(&doc_path, two_diagram_doc()).expect("write doc");

        let outcome = Registry::default()
            .call(
                "export_figures",
                &Ctx::headless(),
                &json!({ "path": doc_path.to_string_lossy() }),
            )
            .expect("dispatch");
        assert!(outcome.is_ok(), "tool_error: {:?}", outcome.tool_error);

        let d = &outcome.data;
        assert_eq!(d["kind"], "export_figures");
        assert_eq!(d["count"], 2);
        assert_eq!(d["figures"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn tool_missing_file_is_business_error() {
        let outcome = Registry::default()
            .call(
                "export_figures",
                &Ctx::headless(),
                &json!({ "path": "/no/such/path/doc.md" }),
            )
            .expect("dispatch");
        assert!(!outcome.is_ok());
        assert_eq!(outcome.tool_error.unwrap().code, "read_failed");
    }

    #[test]
    fn tool_missing_path_arg_is_engine_fault() {
        let err = Registry::default()
            .call("export_figures", &Ctx::headless(), &json!({}))
            .unwrap_err();
        assert!(
            matches!(err, crate::EngineFault::BadArgs(ref m) if m.contains("path")),
            "expected BadArgs for missing path, got {err:?}"
        );
    }

    // -- prune / re-export cleanliness --------------------------------------

    #[test]
    fn is_figure_name_matches_only_the_generated_pattern() {
        // Generated figures for stem "foo", any zero-pad width.
        assert!(is_figure_name("foo_fig_01.png", "foo"));
        assert!(is_figure_name("foo_fig_001.png", "foo"));
        assert!(is_figure_name("foo_fig_7.png", "foo"));
        // Not figures.
        assert!(!is_figure_name("foo.png", "foo"));
        assert!(!is_figure_name("foo_fig_.png", "foo")); // no digits
        assert!(!is_figure_name("foo_fig_ab.png", "foo")); // non-digit
        assert!(!is_figure_name("foo_fig_01.jpg", "foo")); // wrong ext
                                                           // A different document's figures must not match this stem.
        assert!(!is_figure_name("foo_bar_fig_01.png", "foo"));
        assert!(!is_figure_name("other_fig_01.png", "foo"));
    }

    #[test]
    fn re_export_prunes_orphans_when_the_diagram_count_shrinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc_path = dir.path().join("report.md");

        // First export: two diagrams → report_fig_01/02.png.
        std::fs::write(&doc_path, two_diagram_doc()).expect("write doc");
        assert_eq!(
            export_figures(two_diagram_doc(), &doc_path).unwrap().len(),
            2
        );
        // Simulate a stale figure from a prior, larger export (and a different
        // width) that the current run must not leave behind.
        std::fs::write(dir.path().join("report_fig_03.png"), b"stale").unwrap();
        std::fs::write(dir.path().join("report_fig_003.png"), b"stale-wide").unwrap();
        // An unrelated sibling and another doc's figure must survive.
        std::fs::write(dir.path().join("keepme.png"), b"keep").unwrap();
        std::fs::write(dir.path().join("other_fig_01.png"), b"other").unwrap();

        // Re-export with a single diagram → exactly report_fig_01.png, orphans gone.
        let one = "# One\n\n```mermaid\ngraph TD; A-->B\n```\n";
        let results = export_figures(one, &doc_path).expect("re-export");
        assert_eq!(results.len(), 1);
        assert!(dir.path().join("report_fig_01.png").exists());
        assert!(
            !dir.path().join("report_fig_02.png").exists(),
            "shrunk orphan pruned"
        );
        assert!(
            !dir.path().join("report_fig_03.png").exists(),
            "stale orphan pruned"
        );
        assert!(
            !dir.path().join("report_fig_003.png").exists(),
            "wide-width orphan pruned"
        );
        assert!(
            dir.path().join("keepme.png").exists(),
            "unrelated file kept"
        );
        assert!(
            dir.path().join("other_fig_01.png").exists(),
            "other doc's figure kept"
        );
    }

    #[test]
    fn exporting_a_diagramless_document_never_deletes_siblings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc_path = dir.path().join("notes.md");
        std::fs::write(&doc_path, "# Notes\n\nNo diagrams here.\n").expect("write doc");
        // A pre-existing figure-shaped sibling must NOT be deleted by a no-op export.
        std::fs::write(dir.path().join("notes_fig_01.png"), b"preexisting").unwrap();

        let results = export_figures("# Notes\n\nNo diagrams here.\n", &doc_path).expect("export");
        assert!(results.is_empty(), "no diagrams → no figures written");
        assert!(
            dir.path().join("notes_fig_01.png").exists(),
            "a diagram-less export must not delete siblings"
        );
    }
}
