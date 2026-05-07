<!--
SPDX-License-Identifier: AGPL-3.0-or-later
Copyright 2026 Shawn Hartsock and contributors
-->

# scrybe-py

PyO3 bindings that expose `scrybe-core` and `scrybe-render` to Python as the
`scrybe._rust` native extension module. Python on the outside, Rust on the
inside — this crate is the seam between the two.

## What it does

Wraps the Rust `Document`, `ContentId`, and `render_html` in Python-callable
classes and functions. Python code (the `scrybe` package, plugins, and the
`gila scrybe` CLI integration) calls these via the compiled extension module
without any Rust toolchain at runtime.

## Role in the architecture

`scrybe-py` is a thin translation layer. It re-exports `scrybe-core` and
`scrybe-render` types directly and adds only the PyO3 `#[pyclass]` wrappers
needed for the Python boundary. The module is built with `maturin` and
installed as `scrybe/_rust.so` (Linux/macOS) or `scrybe/_rust.pyd` (Windows).

## Key public types and entry points

| Python name | Rust source | Description |
|-------------|-------------|-------------|
| `scrybe._rust.Document` | `scrybe_core::Document` | Load, inspect, and render Markdown; `content_id()`, `render_html(theme?)`, `ast_title()` |
| `scrybe._rust.ContentId` | `scrybe_core::ContentId` | `ContentId.of(bytes)`, `verify(bytes)`, `as_hex()` |
| `scrybe._rust.render_markdown(source, theme?)` | `scrybe_render::render_html` | One-shot render without opening a Document object |

The `python` feature gate controls whether PyO3 bindings are compiled in. The
crate can also be used as a plain Rust library (re-exporting core types) without
the feature.

## Build and test

```sh
# Build the Python extension (editable install into active venv)
maturin develop --features python,extension-module

# Run Rust unit tests (no Python required)
cargo test -p scrybe-py

# Smoke test from Python
python -c "from scrybe._rust import Document; d = Document('# Hi'); print(d.content_id())"
```

Requires `maturin` (`pip install maturin`) and a Rust toolchain. The workspace
`Cargo.toml` pins `pyo3 = "0.28"` with `auto-initialize`.
