# scrybe-ai

Umbrella installer for [Scrybe.ai](https://github.com/hartsock/scrybe) — the
MCP-native Markdown editor where the document is the conversation.

```bash
npm install -g scrybe-ai
scrybe --help
```

This is the friendly single-name install. It depends on
[`@scrybe-ai/cli`](https://www.npmjs.com/package/@scrybe-ai/cli), which delivers
the prebuilt `scrybe` Rust binary for your platform via per-platform
`optionalDependencies` (the `uv` / `esbuild` pattern — no `postinstall`, no
network at install time). It mirrors the PyPI `scrybe.ai` metapackage.

Other channels:

```bash
pip install scrybe.ai          # PyPI (all platforms)
cargo install scrybe-cli       # crates.io
```

Desktop app installers (DMG / exe / AppImage) are on the
[releases page](https://github.com/hartsock/scrybe/releases).

License: Apache-2.0
