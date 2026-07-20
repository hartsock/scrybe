<p align="center">
  <img src="scrybe-logo.png" alt="Scrybe" width="160" />
</p>

# Scrybe

**MCP-native cross-platform Markdown editor.** The document is the
conversation: the desktop app, the CLI, and AI agents all drive the same live
document through one tool contract.

## Install

```bash
npm install -g scrybe-ai        # CLI via npm (no Rust or Python needed)
pip install scrybe.ai           # full Python toolkit (library + CLI + MCP server + extras)
cargo install scrybe-cli scrybe-mcp-server   # build the binaries from crates.io
```

The **desktop app** (macOS / Windows / Linux) ships via
[GitHub Releases](https://github.com/hartsock/scrybe/releases).

All packages version in lock-step: one release, one version, every channel.

## Quick start

```bash
scrybe file.md          # open a file in the GUI
scrybe ./               # open a directory
scrybe --help           # everything the CLI can do

# Connect to Claude Code as an MCP server
claude mcp add scrybe -- scrybe-mcp-server stdio
```

Every human control has an agent equivalent and vice versa. The full MCP tool
surface — names, schemas, semantics — is frozen per release in
[`docs/mcp-contract-0.6.json`](docs/mcp-contract-0.6.json); the CLI↔GUI socket
contract lives in [`docs/rpc-contract-0.6.md`](docs/rpc-contract-0.6.md).
Agent workflow guide: [`AGENTS.md`](AGENTS.md).

## Development

```bash
git clone https://github.com/hartsock/scrybe && cd scrybe
just check              # full lint + test suite
just dev                # Tauri dev server
```

Architecture, crate map, and conventions: [`CLAUDE.md`](CLAUDE.md).
Direction: [`ROADMAP.md`](ROADMAP.md) — GitHub issues are the ground truth.

## License

Apache-2.0. Use it, build on it, ship it. Your documents stay plain text and
belong to you.
