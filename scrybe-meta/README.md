# scrybe.ai

**Scrybe — MCP-native Markdown editor.** Metapackage that installs the full Python toolkit:

```bash
pip install scrybe.ai
```

This pulls in:

| Package | Role |
|---|---|
| [`scrybe-py`](https://pypi.org/project/scrybe-py/) | PyO3 library — `import scrybe` for embedding/scripting |
| [`scrybe-cli`](https://pypi.org/project/scrybe-cli/) | `scrybe` command-line tool — render / lint / mermaid |
| [`scrybe-mcp-server`](https://pypi.org/project/scrybe-mcp-server/) | Standalone MCP server binary |
| [`scrybe-mermaid`](https://pypi.org/project/scrybe-mermaid/) | PNG iTXt codec — embeds Mermaid source in PNG metadata |
| [`scrybe-plugin-docx`](https://pypi.org/project/scrybe-plugin-docx/) | Word (.docx) exporter used by the desktop Export button and MCP `export` tool |

Each component is also installable on its own if you only need one. This metapackage exists so `pip install scrybe.ai` Just Works for users who want the whole kit.

## Desktop app

The Scrybe **desktop application** (Tauri 2 — macOS / Windows / Linux) is distributed via [GitHub Releases](https://github.com/hartsock/scrybe/releases), not PyPI. Download the platform installer from the latest release.

## Project home

Source, issues, and documentation: <https://github.com/hartsock/scrybe>

License: Apache-2.0
