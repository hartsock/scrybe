<p align="center">
  <img src="scrybe-logo.png" alt="Scrybe" width="160" />
</p>

# Scrybe

**MCP-native cross-platform Markdown editor.** The document is the
conversation: the desktop app, the CLI, and AI agents all drive the same live
document through one tool contract.

## Install

```bash
brew install --cask hartsock/scrybe/scrybe    # macOS (Apple silicon)
npm install -g scrybe-ai                      # CLI via npm
pip install scrybe.ai                         # Python toolkit
cargo install scrybe-cli scrybe-mcp-server    # from source via crates.io
```

> **First Homebrew install (Homebrew 6+):** a cask from a third-party tap must be
> trusted once. If you hit *"Refusing to load cask from untrusted tap"*, run
> `brew trust hartsock/scrybe` (or the exact command Homebrew prints) and re-run
> the install.

Windows: `choco install scrybe` (pending community moderation).
All installers are on [GitHub Releases](https://github.com/hartsock/scrybe/releases):
dmg · setup.exe/msi · AppImage · deb · rpm.

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
