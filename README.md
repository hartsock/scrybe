<p align="center">
  <img src="scrybe-logo.png" alt="Scrybe" width="160" />
</p>

# Scrybe

**MCP-native cross-platform Markdown editor.**

The document is the conversation. Connect AI agents (Claude, Codex, Ollama, and more)
as MCP peers. Scrybe is itself an MCP server, drivable by external agents.

## Install

```bash
pip install scrybe-cli scrybe-mcp-server   # CLI + MCP server
```

macOS (coming soon): `brew install scrybe` — see [issue #1](https://github.com/hartsock/scrybe/issues/1)  
Windows (coming soon): `choco install scrybe` — see [issue #2](https://github.com/hartsock/scrybe/issues/2)

## Quick start

```bash
scrybe file.md          # open a file in the GUI
scrybe ./               # open a directory
scrybe                  # open the welcome screen

# Connect to Claude Code as an MCP server
claude mcp add scrybe -- scrybe-mcp-server stdio
```

MCP tools: `open` · `read` · `section` · `edit` · `find` · `render` · `embed` · `extract` · `lint` · `logs` · `close_tab` · `quit`

## Development

```bash
git clone https://github.com/hartsock/scrybe
cd scrybe
just build          # all crates
just dev            # Tauri dev server (requires Node)
just install        # build + install to ~/Applications and ~/venv/bin
just check          # full lint + test suite
```

## Architecture

Python on the outside, Rust on the inside.

| Crate | Role |
|---|---|
| [`scrybe-core`](scrybe-core/README.md) | AST, `Document`, `ContentAddressable` (BLAKE3+CBOR), `Plugin` trait, `Workspace` |
| [`scrybe-render`](scrybe-render/README.md) | HTML pipeline, syntect syntax highlighting, KaTeX/Mermaid |
| [`scrybe-mcp-server`](scrybe-mcp-server/README.md) | Inbound MCP — 12 tools for agent document editing |
| [`scrybe-mcp-client`](scrybe-mcp-client/README.md) | Outbound MCP — registers external agent servers |
| [`scrybe-mermaid`](scrybe-mermaid/README.md) | Standalone PNG iTXt codec (Mermaid source in PNG metadata) |
| [`scrybe-panels`](scrybe-panels/README.md) | Bake-off orchestrator + SQLite calibration log |
| [`scrybe-vcs`](scrybe-vcs/README.md) | git2 multi-remote VCS wrapper |
| [`scrybe-swarm`](scrybe-swarm/README.md) | NATS swarm-chat sidebar + activity feed |
| [`scrybe-py`](scrybe-py/README.md) | PyO3 bindings (`scrybe._rust`) |
| [`scrybe-cli`](scrybe-cli/README.md) | Headless CLI binary (maturin wheel) |
| [`scrybe-app`](scrybe-app/README.md) | Tauri 2 desktop app (Rust + TypeScript + CodeMirror 6) |

## PyPI packages

| Package | Install | What |
|---|---|---|
| `scrybe-cli` | `pip install scrybe-cli` | `scrybe` CLI binary |
| `scrybe-mcp-server` | `pip install scrybe-mcp-server` | `scrybe-mcp-server` binary |
| `scrybe-mermaid` | `pip install scrybe-mermaid` | PNG iTXt codec |

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).

Free and open source — copyleft ensures this editor stays community-accessible forever.
