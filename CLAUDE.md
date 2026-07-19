# CLAUDE.md — Scrybe

Agent instructions for Claude Code and other AI agents working in this repo.

## Design Philosophy

> "Python on the outside, Rust on the inside."

Scrybe is a Markdown editor where the document is the conversation.
Every design decision should make the *document* easier to read, write, and share —
not make the toolchain more impressive.

- **scrybe-core / scrybe-render / scrybe-***: Pure Rust. Zero Python runtime dependencies.
- **scrybe-cli / scrybe-mcp-server**: Rust binaries, packaged as Python binary wheels via maturin (`bindings = "bin"`). Python is the *distribution format*, not the runtime.
- **scrybe-py**: PyO3 bindings only if a library API is needed from Python. Keep thin.
- **scrybe-app**: Tauri 2 (Rust backend + TypeScript/CodeMirror frontend). No React, no heavy JS frameworks.

## Repo Layout

| Path | Language | Role |
|---|---|---|
| `scrybe-core/` | Rust | AST, Document, ContentAddressable (BLAKE3+CBOR), Plugin trait, Workspace |
| `scrybe-render/` | Rust | HTML pipeline, syntect highlighting, KaTeX/Mermaid |
| `scrybe-mcp-server/` | Rust | Inbound MCP server — tools: open/read/section/edit/save/find/render/embed/extract/lint/list_tabs/mermaid_to_png/export_figures/logs/reload/quit/close_tab + UI-parity tools state/set_theme/view_mode/set_vim/export |
| `scrybe-mcp-client/` | Rust | Outbound MCP — registers external agent servers |
| `scrybe-mermaid/` | Rust | PNG iTXt codec (Mermaid source embedded in PNG metadata) |
| `scrybe-panels/` | Rust | Bake-off orchestrator + SQLite calibration log |
| `scrybe-vcs/` | Rust | git2 wrapper, multi-remote VCS |
| `scrybe-swarm/` | Rust | NATS swarm-chat sidebar + activity feed |
| `scrybe-cli/` | Rust (maturin bin) | Headless CLI: render / lint / mermaid / open |
| `scrybe-py/` | Rust + PyO3 | Python library bindings |
| `scrybe-app/` | Rust + TypeScript | Tauri 2 desktop app |

## Build Commands

```bash
# Full workspace check (lint + test)
just check

# Build all crates
just build

# Release build
just release

# Build + install Tauri desktop app to ~/Applications and ~/venv/bin
just install

# Tauri dev server (live reload)
just dev

# Run all tests
just test

# Format
just fmt
```

## Python Packaging (maturin)

```bash
# Install a binary crate into ~/venv (development)
cd scrybe-mcp-server && ~/venv/bin/maturin develop --release
cd scrybe-cli && ~/venv/bin/maturin develop --release

# Build a distributable wheel
cd scrybe-mcp-server && maturin build --release
```

## Code Style

### Rust
- `cargo fmt` — formatting (enforced in CI)
- `cargo clippy -- -D warnings` — zero warnings policy
- `cargo test` — all tests must pass
- No `#[allow(dead_code)]` without a comment explaining why

### TypeScript (scrybe-app)
- No build-time type errors (`tsc --noEmit`)
- No `any` casts without a comment
- CSS: co-located per feature in `src/styles/`

### Python (scrybe-py, pyproject.toml scripts)
- `black` formatting
- `ruff` lint — zero warnings
- `mypy` type checking (non-blocking in CI, blocking on main)
- `pytest` — all tests must pass

## Testing Standards

### Rust
- Unit tests in `#[cfg(test)]` modules within each file
- Integration tests in `tests/` per crate
- Every public API must have at least one test
- Mock filesystem, network, and subprocess — never hit real external services in tests

### TypeScript (scrybe-app)
- Tauri IPC commands tested via unit tests in `src-tauri/src/` Rust tests
- Frontend logic tested where feasible; integration via the Rust command surface

### Python
- `pytest tests/ -x --no-header -q`
- Mock all external resources: network, file I/O, subprocess, interactive prompts
- Test business logic and data transformations, not implementation details

## Git Workflow

### Branch naming
- Features: `feat/short-description`
- Fixes: `fix/short-description`
- Release: `release/vX.Y.Z`

### Commit style
```
type(scope): short summary

Body explaining WHAT changed and WHY. Reference issues where relevant.

Co-Authored-By: <agent identity>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`

### Agent identity in commits
All agent commits must include a `Co-Authored-By` line identifying the agent:
```
Co-Authored-By: Beaver (MacBook agent, Claude Sonnet 4.6) <noreply@anthropic.com>
```

### Never
- Push directly to `main` — use PRs
- Force-push to `main`
- Skip the pre-push hook (`--no-verify`)
- Commit credentials, secrets, or API keys

## Release Process

Releases are triggered by pushing a `v*` tag:

```bash
git tag v0.6.0
git push origin v0.6.0
```

The release workflow (`.github/workflows/release.yml`) builds on Linux, Windows, and macOS,
producing:
- **macOS**: `Scrybe_<version>_aarch64.dmg` + `Scrybe_<version>_x86_64.dmg`
- **Windows**: `Scrybe_<version>_x86_64-setup.exe`
- **Linux**: `scrybe_<version>_amd64.AppImage` + `scrybe_<version>_amd64.deb`
- **Python wheels** (PyPI, all platforms via maturin): `scrybe-py`, `scrybe-cli`, `scrybe-mcp-server`, `scrybe-mermaid`
- **Python metapackage** (PyPI, pure Python): `scrybe.ai` — pulls in the four leaf packages

## MCP Integration

Scrybe is itself an MCP server. To connect Claude Code:

```bash
claude mcp add scrybe -- scrybe-mcp-server stdio
```

Available tools: `open`, `read`, `section`, `edit`, `save`, `find`,
`render`, `embed`, `extract`, `lint`, `list_tabs`, `mermaid_to_png`,
`export_figures`, `logs`, `reload`, `quit`, `close_tab`, `state`,
`set_theme`, `view_mode`, `set_vim`, `export`

Edits land in the in-memory buffer and leave the tab dirty; `save` is the
explicit persist (the agent-side twin of Cmd+S / 💾).

Every human control in scrybe-app has an MCP equivalent and vice versa
(`state`/`set_theme`/`view_mode`/`set_vim`/`export`/`export_figures` mirror
the path bar, theme dropdown, View button, Vim toggle, the Export button,
and the "Export Diagrams…" menu item).

See `AGENTS.md` for full agent interaction guide.

## Zero-Warning Policy

When merging to `main`:
- `cargo clippy -- -D warnings`: zero warnings
- `cargo fmt -- --check`: passes
- `ruff check`: zero warnings
- `black --check`: passes
- All tests pass

Warnings are not allowed to accumulate. If a warning exists, fix it.
