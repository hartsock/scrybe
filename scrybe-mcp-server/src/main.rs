// SPDX-License-Identifier: AGPL-3.0-or-later
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
            let reg = scrybe_mcp_server::ToolRegistry::new();
            let tools = reg.list_tools_json();
            for tool in tools["tools"].as_array().unwrap_or(&vec![]) {
                println!("{}", tool["name"].as_str().unwrap_or("?"));
            }
        }
    }
}
