// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe — headless Markdown render/lint/mermaid/panel CLI + RPC client.

use clap::{Parser, Subcommand};
use scrybe_core::{ContentAddressable, Document};
use scrybe_render::{render_html, Theme};

use scrybe_cli::{active_features, lint_document, rpc_client, version_string, wrap_full_html};

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
    open       Open or refresh a tab. If the file is already open, reloads
               from disk (force-refresh — no duplicate tabs).
    save       Save an open tab's buffer to disk. Silent no-op if not open.
    close      Close a tab. Silent no-op if not open.
    quit       Quit the running Scrybe app. --force skips dirty-buffer prompt.
    render     Render Markdown to HTML.
    lint       Lint a Markdown file and report statistics.
    mermaid    Embed/extract/verify Mermaid source in PNG iTXt metadata.
    version    Print version and active feature flags.

CONNECTION MODEL
    When the Scrybe GUI is running, GUI subcommands talk to it over a
    Unix socket (~/.scrybe/sock; override with $SCRYBE_SOCK).

    When it isn't:
      - `open`  launches the GUI with the given path (macOS: `open -a Scrybe`;
                Linux: $SCRYBE_APP_BIN or scrybe-app on PATH).
      - `save`, `close`, `quit`  silent no-op (nothing open, nothing to do).
      - `render`, `lint`, `mermaid`  run inline — no GUI required.

EXAMPLES
    scrybe foo.md                  # open or refresh foo.md in the GUI
    scrybe save foo.md             # save foo.md if it's open
    scrybe close foo.md            # close the foo.md tab
    scrybe quit                    # quit (prompts on dirty buffers)
    scrybe quit --force            # quit unconditionally
    scrybe render foo.md | tee foo.html
    scrybe lint foo.md --json
    scrybe mermaid extract diagram.png > diagram.mmd

ENVIRONMENT
    SCRYBE_SOCK     Override the default socket path (~/.scrybe/sock)
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

    /// Print version and active feature flags.
    Version,
}

#[derive(Subcommand)]
enum MermaidCmd {
    /// Embed Mermaid source into a PNG as an iTXt metadata chunk.
    Embed {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
        #[arg(value_name = "SOURCE")]
        source: String,
        #[arg(short, long)]
        out: Option<std::path::PathBuf>,
    },
    /// Extract Mermaid source from a PNG.
    Extract {
        #[arg(value_name = "PNG")]
        png: std::path::PathBuf,
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
}

/// Known subcommand names. Anything else in argv[1] is treated as a path to open.
const SUBCOMMANDS: &[&str] = &[
    "render", "lint", "mermaid", "open", "save", "close", "quit", "version", "help",
];

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse_from(inject_open_if_path(std::env::args().collect::<Vec<_>>()));
    match cli.command {
        Command::Render {
            input,
            output,
            theme,
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

            let rendered = render_html(&doc, theme_val);

            let html = if full_html {
                let doc_title = title.unwrap_or_else(|| {
                    path.as_ref()
                        .and_then(|p| p.file_stem())
                        .and_then(|s| s.to_str())
                        .map(String::from)
                        .unwrap_or_else(|| "Untitled".to_string())
                });
                wrap_full_html(&rendered, &doc_title)
            } else {
                rendered.html.clone()
            };

            write_output(output, &html)?;
        }

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
                // Also print content-id.
                eprintln!(
                    "{:<30} {} (CID {})",
                    "File:",
                    input.display(),
                    doc.content_id()
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
            MermaidCmd::Extract { png } => {
                let bytes = std::fs::read(&png)?;
                let payload = scrybe_mermaid::extract(&bytes)?;
                println!("{}", payload.source);
            }
            MermaidCmd::Verify { png } => {
                let bytes = std::fs::read(&png)?;
                match scrybe_mermaid::extract(&bytes) {
                    Err(e) => {
                        eprintln!("scrybe mermaid verify: failed to extract payload: {e}");
                        std::process::exit(1);
                    }
                    Ok(payload) => {
                        // Recompute the sha256 of the extracted source.
                        use sha2::{Digest, Sha256};
                        let mut hasher = Sha256::new();
                        hasher.update(payload.source.as_bytes());
                        let computed = hex::encode(hasher.finalize());

                        if computed == payload.sha256 {
                            println!("OK — sha256 {} matches embedded source", &computed[..16]);
                        } else {
                            eprintln!(
                                "TAMPERED — stored sha256 {} does not match computed {}",
                                &payload.sha256[..16],
                                &computed[..16]
                            );
                            std::process::exit(1);
                        }
                    }
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
                    Ok(resp) => match resp.error {
                        None => println!("Opening {} in Scrybe", canon.display()),
                        Some(e) => {
                            anyhow::bail!("scrybe open failed: {} ({})", e.message, e.code)
                        }
                    },
                    Err(e) if e.contains("no Scrybe running") => {
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
                Ok(resp) => match resp.error {
                    None => println!("Saved {}", canon.display()),
                    Some(e) => {
                        anyhow::bail!("scrybe save failed: {} ({})", e.message, e.code)
                    }
                },
                Err(e) if e.contains("no Scrybe running") => {
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
                Ok(resp) => match resp.error {
                    None => println!("Closed {}", canon.display()),
                    Some(e) => {
                        anyhow::bail!("scrybe close failed: {} ({})", e.message, e.code)
                    }
                },
                Err(e) if e.contains("no Scrybe running") => {
                    // Silent no-op.
                }
                Err(e) => anyhow::bail!("scrybe close failed: {e}"),
            }
        }

        Command::Quit { force } => {
            match rpc_client::send("quit", serde_json::json!({"force": force})) {
                Ok(resp) => match resp.error {
                    None => println!("Quitting Scrybe"),
                    Some(e) => {
                        anyhow::bail!("scrybe quit failed: {} ({})", e.message, e.code)
                    }
                },
                Err(e) if e.contains("no Scrybe running") => {
                    // Silent no-op.
                }
                Err(e) => anyhow::bail!("scrybe quit failed: {e}"),
            }
        }

        Command::Version => {
            println!("scrybe {}", version_string());
            println!("Features: {}", active_features());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_input(path: Option<std::path::PathBuf>) -> anyhow::Result<String> {
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

fn write_output(path: Option<std::path::PathBuf>, content: &str) -> anyhow::Result<()> {
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
fn launch_scrybe(path: Option<&std::path::Path>) -> Result<(), String> {
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
fn find_scrybe_app_bundle() -> Option<String> {
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
fn inject_open_if_path(mut args: Vec<String>) -> Vec<String> {
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
fn which_scrybe_bin() -> Result<String, String> {
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
