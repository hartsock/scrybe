// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum MermaidCmd {
    /// Embed Mermaid source into a PNG as an iTXt metadata chunk.
    Embed {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
        #[arg(value_name = "SOURCE")]
        source: String,
        #[arg(short, long)]
        out: Option<std::path::PathBuf>,
    },
    /// Extract Mermaid source from a PNG, verifying its sha256 by default.
    ///
    /// The stored digest is checked against a freshly-computed hash of the
    /// extracted source. Exit codes: 0 = success (a payload with no stored
    /// digest prints a note to stderr), 1 = extraction failed (no payload /
    /// unreadable PNG), 2 = verification failed (tampered — stored sha256
    /// does not match the extracted source). Use --unverified to skip the
    /// check and print the raw stored source for forensics.
    Extract {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
        /// Skip digest verification and print the raw stored source
        /// (forensics on tampered payloads).
        #[arg(long)]
        unverified: bool,
    },
    /// Verify the sha256 integrity of the embedded Mermaid source in a PNG.
    ///
    /// Extracts the iTXt payload and checks that the stored sha256 matches
    /// a freshly-computed hash of the source bytes. Exits 0 if the integrity
    /// check passes, exits 1 if tampered or missing.
    Verify {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
    },
    /// Render a Mermaid diagram to PNG with its source embedded (uuid + sha256).
    ///
    /// Pure-Rust rendering (no `mmdc`) via the shared `mermaid_to_png` tool, so
    /// the produced PNG is losslessly round-trippable — recover the source with
    /// `scrybe mermaid extract`.
    Png {
        /// File containing the Mermaid source (e.g. a `.mmd` file).
        #[arg(value_name = "SOURCE_FILE")]
        input: std::path::PathBuf,
        /// Output PNG path.
        #[arg(short, long, value_name = "OUT")]
        out: std::path::PathBuf,
    },
    /// Export every Mermaid diagram in a document to sibling PNG figures.
    ///
    /// For `foo.md`, writes `foo_fig_01.png`, `foo_fig_02.png`, … next to the
    /// document in document order; each PNG embeds its Mermaid source (uuid +
    /// sha256) so it round-trips via `scrybe mermaid extract`. Pure-Rust
    /// rendering (no `mmdc`) via the shared `export_figures` tool.
    Export {
        /// Markdown document whose Mermaid diagrams to export.
        #[arg(value_name = "FILE")]
        input: std::path::PathBuf,
        /// Output JSON (`{count, figures}`) instead of one path per line.
        #[arg(long)]
        json: bool,
    },
}

// ---------------------------------------------------------------------------
// Mermaid extract handler (shared by `mermaid extract` and top-level `extract`)
// ---------------------------------------------------------------------------

/// First 16 hex chars of a digest for display — safe on short or garbage
/// stored values (attacker-controlled input must never panic the CLI).
pub(crate) fn digest_short(d: &str) -> &str {
    d.get(..16).unwrap_or(d)
}

/// Extract embedded Mermaid source from `png` and print it.
///
/// Verifies the stored sha256 against the extracted source by default.
/// Exit codes: 0 = success (no-digest payloads print a stderr note),
/// 1 = extraction failure (no payload / unreadable PNG, via the returned
/// error), 2 = verification failure (tampered).
pub(crate) fn run_mermaid_extract(png: &std::path::Path, unverified: bool) -> anyhow::Result<()> {
    let bytes = std::fs::read(png)?;

    if unverified {
        // Forensics path: raw stored fields, no digest check.
        let payload = scrybe_mermaid::extract_unverified(&bytes)?;
        println!("{}", payload.source);
        return Ok(());
    }

    match scrybe_mermaid::extract(&bytes) {
        Ok(payload) => {
            if !payload.is_verified() {
                eprintln!("note: payload carries no sha256 digest — source is unverified");
            }
            println!("{}", payload.source);
            Ok(())
        }
        Err(e @ scrybe_mermaid::MermaidError::VerificationFailed { .. }) => {
            eprintln!("scrybe mermaid extract: {e}");
            eprintln!("(use --unverified to print the stored source anyway)");
            std::process::exit(2);
        }
        Err(e) => Err(e.into()),
    }
}
