// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-mcp-server — standalone MCP server binary.
//!
//! Usage: `scrybe-mcp-server stdio`
//! Or via Python: `python -m scrybe_mcp_server stdio`

use clap::{Parser, Subcommand};
use scrybe_mcp_server::McpServer;

#[derive(Parser)]
#[command(name = "scrybe-mcp-server", version, about = "Scrybe MCP server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run in stdio transport mode (default, for MCP clients).
    Stdio,
    /// Print the list of available tools.
    Tools,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Stdio => {
            let mut server = McpServer::new();
            server.run_stdio();
        }
        Command::Tools => {
            // The shared scrybe-tools registry is the one source of tool names.
            for name in scrybe_tools::Registry::default().names() {
                println!("{name}");
            }
        }
    }
}
