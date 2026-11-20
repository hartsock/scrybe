// SPDX-License-Identifier: Apache-2.0
export const WELCOME = `<p align="center"><img src="scrybe-logo.png" alt="Scrybe" width="120" /></p>

# Welcome to Scrybe

**MCP-native Markdown editor.** The document is the conversation —
connect AI agents, preview in real time, and keep full control of your files.

\`\`\`mermaid
graph LR
    You([You]) <-->|reads & edits| Doc([Your document])
    Doc <-->|MCP tools| Scrybe[Scrybe]
    Scrybe <-->|open · read · edit| Agent1([Claude])
    Scrybe <-->|open · read · edit| Agent2([Codex])
    Scrybe <-->|open · read · edit| Agent3([Ollama])
\`\`\`

## Get started

- **Open a file** — click in the sidebar, or run \`scrybe path/to/file.md\`
- **Links navigate** — relative links open in a new tab; directories load in the sidebar
- **Autosave** — edits save automatically; the original is preserved as \`.bak\`
- **View modes** — click \`◧◨\` on any tab to cycle both / editor / preview / WYSIWYG

## Connect an AI agent

\`\`\`bash
pip install scrybe-mcp-server
claude mcp add scrybe -- scrybe-mcp-server stdio
\`\`\`

Tools: \`open\` · \`read\` · \`edit\` · \`find\` · \`render\` · \`lint\` · \`close_tab\`
`;
