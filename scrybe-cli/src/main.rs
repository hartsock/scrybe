// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe — headless Markdown render/lint/mermaid/panel CLI + RPC client.

mod gui_cmds;
mod mermaid_cmd;
mod shell_command;
mod table_cmd;
mod util;

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

use clap::{Parser, Subcommand};
use scrybe_core::{ContentAddressable, Document};
use scrybe_render::{render_html_with, Theme};

use gui_cmds::{handle_edit, handle_find, handle_read, handle_section};
use mermaid_cmd::{digest_short, run_mermaid_extract, MermaidCmd};
use table_cmd::{run_table, TableCmd};
use util::{inject_open_if_path, launch_scrybe, read_input, write_output};

use scrybe_cli::{
    active_features, lint_document, render_settings, rpc_client, select_render_output,
    version_string, CliRenderProfile,
};

#[derive(Parser)]
#[command(
    name = "scrybe",
    version,
    about = "Scrybe — MCP-native Markdown editor",
    long_about = "\
Scrybe — MCP-native Markdown editor (Apache-2.0)
https://github.com/hartsock/scrybe

The `scrybe` command is the universal surface: humans drive the GUI from
the shell, agents drive Scrybe without an MCP client. The same subcommands
mirror the MCP tool surface, so `pip install scrybe-cli` is the only thing
an agent needs to integrate.

USAGE
    scrybe <SUBCOMMAND> [ARGS] [--json]
    scrybe <PATH>                  shortcut for: scrybe open <PATH>

SUBCOMMANDS
  GUI control (require running Scrybe app):
    open       Open or refresh a tab. If the file is already open, reloads
               from disk (force-refresh — no duplicate tabs).
    save       Save an open tab's buffer to disk. Silent no-op if not open.
    close      Close a tab. Silent no-op if not open.
    quit       Quit the running Scrybe app. --force skips dirty-buffer prompt.
    tabs       List the tabs open in the running app (path, dirty, active).

  Buffer-aware (require running Scrybe app + file open in a tab):
    read       Print the in-memory contents of an open buffer (sees
               unsaved edits — use `cat` for disk content).
    find       Search across open tabs for a regex (or --literal string).
    section    Extract a section by heading (case-insensitive substring).
    edit       Apply a structured edit to lines start..=end of an open buffer.

  Standalone (no GUI required):
    render     Render Markdown to HTML.
    view       View a Markdown or CSV/TSV file in the terminal (TUI viewer; q quits).
    lint       Lint a Markdown file and report statistics.
    mermaid    Embed/extract/verify Mermaid source in PNG iTXt metadata.
    table      Read and edit CSV/TSV rows and cells (get, set-cell, insert-row,
               delete-row). Edits the live buffer when the file is open in the
               app, else the file on disk. Row and column numbers start at 1.
    embed      Top-level shortcut for `mermaid embed`.
    extract    Top-level shortcut for `mermaid extract`.

  Meta:
    shell-command  Install, inspect, repair, or remove the user-level command link.
    version    Print version and active feature flags.

CONNECTION MODEL
    When the Scrybe GUI is running, GUI/buffer-aware subcommands talk to
    it over local IPC (~/.scrybe/sock on Unix; a per-user named pipe on
    Windows; override either with $SCRYBE_SOCK).

    When it isn't:
      - `open`  launches the GUI with the given path (macOS: `open -a Scrybe`;
                Linux: $SCRYBE_APP_BIN or scrybe-app on PATH).
      - `save`, `close`, `quit`  silent no-op (nothing open, nothing to do).
      - `read`, `find`, `section`, `edit`  error: no Scrybe running.
      - `render`, `view`, `lint`, `mermaid`, `embed`, `extract`  run inline — no GUI required.

EXAMPLES
    scrybe foo.md                  # open or refresh foo.md in the GUI
    scrybe save foo.md             # save foo.md if it's open
    scrybe read foo.md             # print the in-memory buffer (incl. unsaved edits)
    scrybe find 'TODO' foo.md      # grep TODO in open foo.md (or disk if not open)
    scrybe find -c 'fn main' --literal      # case-sensitive literal search across open tabs
    scrybe section foo.md --heading 'Install'
    scrybe edit foo.md --start-line 10 --end-line 12 --content '## Updated heading\n'
    scrybe close foo.md            # close the foo.md tab
    scrybe quit                    # quit (prompts on dirty buffers)
    scrybe quit --force            # quit unconditionally
    scrybe render foo.md | tee foo.html
    scrybe view foo.md             # read foo.md in the terminal (q quits)
    scrybe lint foo.md --json
    scrybe extract diagram.png > diagram.mmd

ENVIRONMENT
    SCRYBE_SOCK     Override the local endpoint (~/.scrybe/sock on Unix;
                    \\\\.\\pipe\\scrybe-<user> on Windows)
    SCRYBE_APP_BIN  Path to the GUI binary for launch-when-no-app on Linux
                    (macOS uses `open -a Scrybe`)

INSTALL
    pip install scrybe.ai            # full Python toolkit (metapackage)
    pip install scrybe-cli           # just this CLI
    pip install scrybe-mcp-server    # standalone MCP server
    brew install scrybe              # macOS (once tap is live)
    choco install scrybe             # Windows (once package is live)

See `scrybe <SUBCOMMAND> --help` for full details on any subcommand."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a Markdown file to HTML.
    Render {
        /// Input Markdown file (default: stdin).
        #[arg(value_name = "FILE")]
        input: Option<std::path::PathBuf>,

        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,

        /// Theme to apply: default, dark, or solarized.
        #[arg(long, default_value = "default", value_name = "THEME")]
        theme: String,

        /// Markdown dialect: app compatibility or site (smart punctuation, literal math).
        #[arg(long, value_enum, default_value_t = CliRenderProfile::App)]
        profile: CliRenderProfile,

        /// Write only the rendered body, without theme CSS or a document wrapper.
        #[arg(long, conflicts_with = "full_html")]
        body_only: bool,

        /// Wrap output in a complete <!DOCTYPE html> document with CDN tags.
        #[arg(long)]
        full_html: bool,

        /// Document title used when --full-html is set (default: filename or "Untitled").
        #[arg(long)]
        title: Option<String>,

        /// Watch for file changes and re-render automatically.
        ///
        /// NOTE: --watch is not yet implemented. It requires a file-watcher
        /// crate (e.g. `notify`) and an async event loop. This flag is
        /// documented and accepted by the parser but exits with an informative
        /// message rather than silently ignoring the flag.
        #[arg(long)]
        watch: bool,
    },

    /// View a Markdown or CSV/TSV file in the terminal (TUI viewer).
    ///
    /// A `.csv` or `.tsv` file opens as a scrollable grid (h/l or Left/Right
    /// scroll columns); anything else renders as Markdown.
    /// A single-pane, read-only viewer rendered with scrybe-ratatui — the
    /// same look as the scrybe-tui split viewer, without a second install.
    /// Keys: j/k or arrows scroll, Ctrl-d/Ctrl-u half-page, Space/PageDown/
    /// PageUp page, g/G (or Home/End) jump to top/bottom, q or Esc quits.
    /// Requires an interactive terminal on stdin and stdout; exits 2 when
    /// run in a pipe. For split-screen viewing and live reload, use the
    /// `scrybe-tui` binary.
    View {
        /// Markdown, CSV or TSV file to view.
        #[arg(value_name = "FILE")]
        file: std::path::PathBuf,
    },

    /// Lint a Markdown file and report statistics.
    ///
    /// Outputs a summary table to stderr. Exits 0 if clean (no broken
    /// links), exits 1 if broken links were found.
    Lint {
        /// Input Markdown file.
        #[arg(value_name = "FILE")]
        input: std::path::PathBuf,

        /// Output report as JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },

    /// Embed or extract Mermaid diagram source in a PNG.
    Mermaid {
        #[command(subcommand)]
        cmd: MermaidCmd,
    },

    /// Read and edit a CSV/TSV file's rows and cells.
    Table {
        #[command(subcommand)]
        cmd: TableCmd,
    },

    /// List the tabs open in the running Scrybe GUI (#46).
    Tabs {
        /// Output as JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },

    /// Open or refresh a tab in the running Scrybe GUI.
    ///
    /// With no path, opens the GUI welcome screen. With a path:
    ///   - if the GUI is running and the file isn't open: opens a new tab.
    ///   - if the GUI is running and the file is already open: refreshes
    ///     that tab from disk (one tab, one file — no duplicates).
    ///   - if the GUI isn't running: launches it with the file as argv.
    Open {
        /// File or directory to open (omit for welcome screen).
        #[arg(value_name = "PATH")]
        path: Option<std::path::PathBuf>,
    },

    /// Save an open tab's buffer to disk.
    ///
    /// Silent no-op if the file isn't currently open in the GUI, or if no
    /// GUI is running. Useful for scripts that want to flush autosave-pending
    /// changes before performing some operation on the file.
    Save {
        /// Path of the open tab to save.
        #[arg(value_name = "PATH")]
        path: std::path::PathBuf,
    },

    /// Close a tab in the running Scrybe GUI.
    ///
    /// Silent no-op if the file isn't open or no GUI is running.
    Close {
        /// Path of the tab to close.
        #[arg(value_name = "PATH")]
        path: std::path::PathBuf,
    },

    /// Quit the running Scrybe GUI.
    ///
    /// By default, the GUI prompts on dirty buffers. `--force` skips the
    /// prompt and quits unconditionally. Silent no-op if no GUI is running.
    Quit {
        /// Skip the dirty-buffer confirmation prompt and quit immediately.
        #[arg(long)]
        force: bool,
    },

    /// Read the contents of an open buffer (returns in-memory state, not disk).
    ///
    /// Errors if the file isn't open in the GUI or no GUI is running.
    /// For disk content use `cat` instead.
    Read {
        #[arg(value_name = "PATH")]
        path: std::path::PathBuf,
        /// Output as JSON (`{path, content, is_dirty}`) instead of plain text.
        #[arg(long)]
        json: bool,
    },

    /// Search across open tabs for a regex pattern (or a literal string with --literal).
    ///
    /// With no PATHS, searches every open tab. With explicit PATHS, searches
    /// each one — using the in-memory buffer if open, falling back to disk
    /// otherwise.
    Find {
        /// Regex pattern (or literal string with --literal).
        #[arg(value_name = "PATTERN")]
        pattern: String,
        /// Optional list of paths to scope the search to.
        #[arg(value_name = "PATHS")]
        paths: Vec<std::path::PathBuf>,
        /// Treat PATTERN as a literal string instead of a regex.
        #[arg(long)]
        literal: bool,
        /// Match case-sensitively (default: case-insensitive).
        #[arg(long = "case-sensitive", short = 'c')]
        case_sensitive: bool,
        /// Output JSON instead of grep-style `path:line:column: text`.
        #[arg(long)]
        json: bool,
    },

    /// Extract a section of a Markdown file by its heading text.
    ///
    /// Heading match is case-insensitive substring. Returns the section
    /// content from the heading down to (but not including) the next
    /// heading of the same or shallower level.
    Section {
        #[arg(value_name = "PATH")]
        path: std::path::PathBuf,
        /// Heading text to find (case-insensitive substring match).
        #[arg(long, value_name = "HEADING")]
        heading: String,
        /// Output as JSON (`{heading, level, content}`) instead of plain text.
        #[arg(long)]
        json: bool,
    },

    /// Apply a structured edit to an open buffer.
    ///
    /// Replaces lines `start..=end` (1-indexed, inclusive) with the given
    /// content. Errors if the file isn't open in the GUI.
    Edit {
        #[arg(value_name = "PATH")]
        path: std::path::PathBuf,
        /// First line of the range to replace (1-indexed).
        #[arg(long, value_name = "LINE")]
        start_line: u32,
        /// Last line of the range to replace, inclusive (1-indexed).
        #[arg(long, value_name = "LINE")]
        end_line: u32,
        /// New content for the range. Use `-` to read from stdin.
        #[arg(long, value_name = "TEXT")]
        content: String,
        /// Output as JSON (`{applied, size_after}`) instead of "ok".
        #[arg(long)]
        json: bool,
    },

    /// Embed Mermaid source into a PNG as an iTXt metadata chunk.
    ///
    /// Top-level shortcut for `scrybe mermaid embed`. Same args.
    Embed {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
        #[arg(value_name = "SOURCE")]
        source: String,
        #[arg(short, long)]
        out: Option<std::path::PathBuf>,
    },

    /// Extract Mermaid source from a PNG iTXt metadata chunk.
    ///
    /// Top-level shortcut for `scrybe mermaid extract`. Same args and exit
    /// codes: 0 = success, 1 = extraction failed (no payload / unreadable
    /// PNG), 2 = verification failed (stored sha256 does not match the
    /// extracted source).
    Extract {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
        /// Skip digest verification and print the raw stored source
        /// (forensics on tampered payloads).
        #[arg(long)]
        unverified: bool,
    },

    /// Print version and active feature flags.
    Version,

    /// Manage the user-level `scrybe` command installed by the desktop app.
    ///
    /// The default link is `~/.local/bin/scrybe`. Installation is idempotent: a
    /// correct link is left alone, a broken or known older Scrybe link is repaired
    /// atomically, and unrelated entries are never overwritten.
    #[command(name = "shell-command")]
    ShellLink {
        #[command(subcommand)]
        cmd: ShellCommandCmd,
    },
}

#[derive(Subcommand)]
enum ShellCommandCmd {
    /// Install the command, or report that it is already installed correctly.
    Install {
        /// Override the destination directory (default: ~/.local/bin).
        #[arg(long, value_name = "DIR")]
        bin_dir: Option<std::path::PathBuf>,
        /// Emit a machine-readable lifecycle report.
        #[arg(long)]
        json: bool,
    },
    /// Inspect the command link without changing it.
    Status {
        /// Override the destination directory (default: ~/.local/bin).
        #[arg(long, value_name = "DIR")]
        bin_dir: Option<std::path::PathBuf>,
        /// Emit a machine-readable lifecycle report.
        #[arg(long)]
        json: bool,
    },
    /// Repair a broken or older managed Scrybe link.
    Repair {
        /// Override the destination directory (default: ~/.local/bin).
        #[arg(long, value_name = "DIR")]
        bin_dir: Option<std::path::PathBuf>,
        /// Emit a machine-readable lifecycle report.
        #[arg(long)]
        json: bool,
    },
    /// Remove only a link managed by Scrybe; unrelated entries are preserved.
    Uninstall {
        /// Override the destination directory (default: ~/.local/bin).
        #[arg(long, value_name = "DIR")]
        bin_dir: Option<std::path::PathBuf>,
        /// Emit a machine-readable lifecycle report.
        #[arg(long)]
        json: bool,
    },
}

/// Known subcommand names. Anything else in argv[1] is treated as a path to open.
const SUBCOMMANDS: &[&str] = &[
    "render",
    "view",
    "lint",
    "mermaid",
    "table",
    "tabs",
    "open",
    "save",
    "close",
    "quit",
    "read",
    "find",
    "section",
    "edit",
    "embed",
    "extract",
    "shell-command",
    "version",
    "help",
];

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse_from(inject_open_if_path(std::env::args().collect::<Vec<_>>()));
    match cli.command {
        Command::Render {
            input,
            output,
            theme,
            profile,
            body_only,
            full_html,
            title,
            watch,
        } => {
            if watch {
                eprintln!(
                    "scrybe: --watch is not yet implemented.\n\
                     It requires a file-watcher crate (e.g. `notify`) and an async event loop.\n\
                     Track progress at https://github.com/hartsock/scrybe/issues/1546"
                );
                std::process::exit(1);
            }

            let path = input.clone();
            let source = read_input(input)?;
            let doc = Document::new(source);

            let theme_val = match theme.as_str() {
                "dark" => Theme::Dark,
                "solarized" => Theme::Solarized,
                _ => Theme::Default,
            };

            let (profile, body_only) = render_settings(profile, body_only);
            let rendered = render_html_with(&doc, theme_val, profile);
            let doc_title = title.unwrap_or_else(|| {
                path.as_ref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .map(String::from)
                    .unwrap_or_else(|| "Untitled".to_string())
            });
            let html = select_render_output(
                &rendered,
                body_only,
                full_html.then_some(doc_title.as_str()),
            );

            write_output(output, &html)?;
        }

        Command::View { file } => scrybe_cli::viewer::run(&file)?,

        Command::Lint { input, json } => {
            let source = std::fs::read_to_string(&input)?;
            let doc = Document::new(source);
            let report = lint_document(&doc);

            if json {
                // JSON output — emit a minimal JSON object.
                println!(
                    "{{\"word_count\":{},\"heading_count\":{},\"max_heading_depth\":{},\
                     \"code_block_count\":{},\"has_math\":{},\"has_mermaid\":{},\
                     \"broken_link_count\":{}}}",
                    report.word_count,
                    report.heading_count,
                    report.max_heading_depth,
                    report.code_block_count,
                    report.has_math,
                    report.has_mermaid,
                    report.broken_links.len(),
                );
            } else {
                // Print a table to stderr.
                eprintln!("=== scrybe lint: {} ===", input.display());
                eprintln!("{:<30} {}", "Words:", report.word_count);
                eprintln!("{:<30} {}", "Headings:", report.heading_count);
                eprintln!("{:<30} {}", "Max heading depth:", report.max_heading_depth);
                eprintln!("{:<30} {}", "Code blocks:", report.code_block_count);
                if report.code_block_langs.is_empty() {
                    eprintln!("{:<30} (none)", "Languages:");
                } else {
                    eprintln!(
                        "{:<30} {}",
                        "Languages:",
                        report.code_block_langs.join(", ")
                    );
                }
                eprintln!(
                    "{:<30} {}",
                    "Math present:",
                    if report.has_math { "yes" } else { "no" }
                );
                eprintln!(
                    "{:<30} {}",
                    "Mermaid present:",
                    if report.has_mermaid { "yes" } else { "no" }
                );
                eprintln!("{:<30} {}", "Broken links:", report.broken_links.len());
                for bl in &report.broken_links {
                    eprintln!("  - [{}]({})", bl.text, bl.url);
                }
                // Also print the content digest.
                eprintln!(
                    "{:<30} {} (digest {})",
                    "File:",
                    input.display(),
                    doc.content_digest()
                );
            }

            if !report.is_clean() {
                std::process::exit(1);
            }
        }

        Command::Mermaid { cmd } => match cmd {
            MermaidCmd::Embed { png, source, out } => {
                let bytes = std::fs::read(&png)?;
                let embedded = scrybe_mermaid::embed(&bytes, &source)?;
                let dest = out.unwrap_or_else(|| png.with_extension("embedded.png"));
                std::fs::write(&dest, &embedded)?;
                println!("Embedded into {}", dest.display());
            }
            MermaidCmd::Extract { png, unverified } => {
                run_mermaid_extract(&png, unverified)?;
            }
            MermaidCmd::Verify { png } => {
                let bytes = std::fs::read(&png)?;
                // `extract` verifies by default — a mismatch is a typed error.
                match scrybe_mermaid::extract(&bytes) {
                    Ok(payload) => match &payload.verification {
                        scrybe_mermaid::VerificationStatus::Verified { digest, .. } => {
                            println!(
                                "OK — sha256 {} matches embedded source",
                                digest_short(digest)
                            );
                        }
                        scrybe_mermaid::VerificationStatus::NoDigest => {
                            eprintln!(
                                "MISSING — payload carries no sha256 digest; nothing to verify"
                            );
                            std::process::exit(1);
                        }
                    },
                    Err(scrybe_mermaid::MermaidError::VerificationFailed {
                        expected,
                        actual,
                        ..
                    }) => {
                        eprintln!(
                            "TAMPERED — stored sha256 {} does not match computed {}",
                            digest_short(&expected),
                            digest_short(&actual)
                        );
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("scrybe mermaid verify: failed to extract payload: {e}");
                        std::process::exit(1);
                    }
                }
            }
            MermaidCmd::Png { input, out } => {
                let source = std::fs::read_to_string(&input)?;
                let reg = scrybe_tools::Registry::default();
                let outcome = reg.call(
                    "mermaid_to_png",
                    &scrybe_tools::Ctx::headless(),
                    &serde_json::json!({
                        "source": source,
                        "output_path": out.to_string_lossy(),
                    }),
                )?;
                if let Some(err) = outcome.tool_error {
                    eprintln!("scrybe mermaid png: {}", err.message);
                    std::process::exit(1);
                }
                let d = &outcome.data;
                println!("Wrote {}", d["png_path"].as_str().unwrap_or(""));
                println!("  uuid   {}", d["uuid"].as_str().unwrap_or(""));
                println!("  sha256 {}", d["sha256"].as_str().unwrap_or(""));
            }
            MermaidCmd::Export { input, json } => {
                let reg = scrybe_tools::Registry::default();
                let outcome = reg.call(
                    "export_figures",
                    &scrybe_tools::Ctx::headless(),
                    &serde_json::json!({ "path": input.to_string_lossy() }),
                )?;
                if let Some(err) = outcome.tool_error {
                    eprintln!("scrybe mermaid export: {}", err.message);
                    std::process::exit(1);
                }
                let d = &outcome.data;
                if json {
                    println!("{}", serde_json::to_string_pretty(d)?);
                } else {
                    let figures = d["figures"].as_array().cloned().unwrap_or_default();
                    for f in &figures {
                        println!("{}", f["path"].as_str().unwrap_or(""));
                    }
                    println!("{} figure(s)", d["count"].as_u64().unwrap_or(0));
                }
            }
        },

        Command::Open { path } => match path {
            Some(p) => {
                let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
                // Try the running GUI first — that's the path that produces
                // the "one tab, one file, refresh on re-open" semantics.
                match rpc_client::send("open", serde_json::json!({"path": canon.to_string_lossy()}))
                {
                    Ok(_) => println!("Opening {} in Scrybe", canon.display()),
                    Err(scrybe_rpc::ClientError::Remote(e)) => {
                        anyhow::bail!("scrybe open failed: {} ({})", e.message, e.code)
                    }
                    Err(e) if e.is_not_running() => {
                        // Fall through to launching the app.
                        launch_scrybe(Some(&canon)).map_err(|e| anyhow::anyhow!("{e}"))?;
                        println!("Opening {} in Scrybe", canon.display());
                    }
                    Err(e) => anyhow::bail!("scrybe open failed: {e}"),
                }
            }
            None => {
                // Bare `scrybe` with no path: just launch (or focus) the GUI.
                // No RPC needed — opening the welcome screen on an already-
                // running app is a launch-app concern handled by single-instance.
                launch_scrybe(None).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("Opening Scrybe");
            }
        },

        Command::Save { path } => {
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            match rpc_client::send("save", serde_json::json!({"path": canon.to_string_lossy()})) {
                // Reply-correlated save reports { path, bytes, was_dirty };
                // an older app replies the legacy {applied} ack with no
                // byte count — don't fabricate one.
                Ok(result) => {
                    let bytes = result.get("bytes").and_then(serde_json::Value::as_u64);
                    match bytes {
                        Some(b) => println!("Saved {} ({b} bytes)", canon.display()),
                        None => println!("Saved {}", canon.display()),
                    }
                }
                // Not open: silent no-op per the documented CLI contract.
                Err(scrybe_rpc::ClientError::Remote(e))
                    if e.code == scrybe_rpc::ERR_TAB_NOT_OPEN => {}
                Err(scrybe_rpc::ClientError::Remote(e)) => {
                    anyhow::bail!("scrybe save failed: {} ({})", e.message, e.code)
                }
                Err(e) if e.is_not_running() => {
                    // Silent no-op per design: nothing open, nothing to save.
                }
                Err(e) => anyhow::bail!("scrybe save failed: {e}"),
            }
        }

        Command::Close { path } => {
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            match rpc_client::send(
                "close",
                serde_json::json!({"path": canon.to_string_lossy()}),
            ) {
                Ok(_) => println!("Closed {}", canon.display()),
                Err(scrybe_rpc::ClientError::Remote(e)) => {
                    anyhow::bail!("scrybe close failed: {} ({})", e.message, e.code)
                }
                Err(e) if e.is_not_running() => {
                    // Silent no-op.
                }
                Err(e) => anyhow::bail!("scrybe close failed: {e}"),
            }
        }

        Command::Table { cmd } => run_table(cmd)?,

        Command::Tabs { json } => {
            // The same shared `list_tabs` handler the MCP server uses.
            let reg = scrybe_tools::Registry::default();
            let outcome = reg
                .call(
                    "list_tabs",
                    &scrybe_tools::Ctx::live(),
                    &serde_json::json!({}),
                )
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(err) = &outcome.tool_error {
                if err.code == "no_live_app" {
                    eprintln!("No Scrybe app is running.");
                    std::process::exit(1);
                }
                anyhow::bail!("scrybe tabs: {}", err.message);
            }
            let data = &outcome.data;
            if json {
                println!("{}", serde_json::to_string_pretty(data)?);
            } else {
                let tabs = data["tabs"].as_array().cloned().unwrap_or_default();
                if tabs.is_empty() {
                    println!("No open tabs.");
                } else {
                    for t in &tabs {
                        let active = if t["active"].as_bool().unwrap_or(false) {
                            "*"
                        } else {
                            " "
                        };
                        let dirty = if t["is_dirty"].as_bool().unwrap_or(false) {
                            "\u{25cf}"
                        } else {
                            " "
                        };
                        println!(
                            "{active} {dirty} {:<8} {}",
                            t["view_mode"].as_str().unwrap_or(""),
                            t["path"].as_str().unwrap_or("")
                        );
                    }
                }
            }
        }
        Command::Quit { force } => {
            match rpc_client::send("quit", serde_json::json!({"force": force})) {
                Ok(_) => println!("Quitting Scrybe"),
                Err(scrybe_rpc::ClientError::Remote(e)) => {
                    anyhow::bail!("scrybe quit failed: {} ({})", e.message, e.code)
                }
                Err(e) if e.is_not_running() => {
                    // Silent no-op.
                }
                Err(e) => anyhow::bail!("scrybe quit failed: {e}"),
            }
        }

        Command::Read { path, json } => handle_read(&path, json)?,
        Command::Find {
            pattern,
            paths,
            literal,
            case_sensitive,
            json,
        } => handle_find(&pattern, &paths, literal, case_sensitive, json)?,
        Command::Section {
            path,
            heading,
            json,
        } => handle_section(&path, &heading, json)?,
        Command::Edit {
            path,
            start_line,
            end_line,
            content,
            json,
        } => handle_edit(&path, start_line, end_line, &content, json)?,
        Command::Embed { png, source, out } => {
            // Top-level alias for `mermaid embed` — same code path.
            let bytes = std::fs::read(&png)?;
            let embedded = scrybe_mermaid::embed(&bytes, &source)?;
            let dest = out.unwrap_or_else(|| png.with_extension("embedded.png"));
            std::fs::write(&dest, &embedded)?;
            println!("Embedded into {}", dest.display());
        }
        Command::Extract { png, unverified } => {
            // Top-level alias for `mermaid extract` — same code path.
            run_mermaid_extract(&png, unverified)?;
        }

        Command::ShellLink { cmd } => {
            let (operation, bin_dir, json) = match cmd {
                ShellCommandCmd::Install { bin_dir, json } => {
                    (shell_command::Operation::Install, bin_dir, json)
                }
                ShellCommandCmd::Status { bin_dir, json } => {
                    (shell_command::Operation::Status, bin_dir, json)
                }
                ShellCommandCmd::Repair { bin_dir, json } => {
                    (shell_command::Operation::Repair, bin_dir, json)
                }
                ShellCommandCmd::Uninstall { bin_dir, json } => {
                    (shell_command::Operation::Uninstall, bin_dir, json)
                }
            };
            let report = shell_command::run(operation, bin_dir)?;
            if json {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                println!("{}", report.message);
                for warning in &report.warnings {
                    eprintln!("warning: {warning}");
                }
            }
        }

        Command::Version => {
            println!("scrybe {}", version_string());
            println!("Features: {}", active_features());
        }
    }
    Ok(())
}
