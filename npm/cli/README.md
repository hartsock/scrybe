# @scrybe-ai/cli

The `scrybe` command-line binary for [Scrybe](https://github.com/hartsock/scrybe) —
headless render / lint / mermaid / edit plus the MCP-native live-tab driver.

```bash
npm install -g @scrybe-ai/cli
scrybe --help
# or run once, no install:
npx @scrybe-ai/cli render README.md
```

## How it works

This package ships **no binary itself**. It declares the per-platform packages
`@scrybe-ai/cli-<os>-<arch>` as `optionalDependencies`; your package manager
installs only the one matching your machine, and the `scrybe` bin shim execs it.
There is **no `postinstall` download step** — the binary arrives as a normal
package, so installs are hermetic and offline-cacheable. This is the same
pattern used by `uv`, `esbuild`, and `@biomejs/biome`.

Prefer a single install name? Use the umbrella:

```bash
npm install -g scrybe-ai      # depends on @scrybe-ai/cli
```

## Not your platform?

If no prebuilt binary matches your platform, install from source:

```bash
cargo install scrybe-cli      # crates.io
pip install scrybe.ai         # PyPI (all platforms)
```

License: Apache-2.0
