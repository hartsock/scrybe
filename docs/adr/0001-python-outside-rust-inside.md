# ADR-0001: Python on the Outside, Rust on the Inside

## Status

Accepted

## Context

Scrybe is a Rust-native project: the core algorithms (content addressing, Markdown
AST, PNG codec, MCP protocol, VCS operations) are written in Rust for correctness,
performance, and memory safety. At the same time, Python is the dominant language
for editor tooling, AI pipelines, and developer automation. Requiring users to
install a Rust toolchain to use Scrybe would be a significant adoption barrier.

We need a distribution model that gives Python users a first-class experience
while keeping Rust as the source of truth for all non-trivial logic.

## Decision

**"Python on the outside, Rust on the inside"** is Scrybe's signature architecture.

Every crate with independent value to a Python user ships a corresponding PyPI
package built with [maturin](https://github.com/PyPA-/maturin). The Rust
implementation is the only implementation; the Python surface is a thin PyO3
wrapper, not a separate port.

### The pattern (four rules)

**1. One implementation, two surfaces.**
There is no pure-Python fallback for business logic. The Rust extension *is* the
implementation. Python tests exercise the same code paths as Rust tests — they
are not testing a separate reimplementation.

**2. Fail fast and loudly at the boundary.**
If the compiled extension is absent, every public API raises immediately:

```python
# scrybe/__init__.py
_MISSING_MSG = (
    "scrybe requires the compiled Rust extension.\n"
    "  Install a pre-built wheel :  pip install scrybe\n"
    "  Build from source          :  maturin develop --features python,extension-module"
)

try:
    from scrybe._rust import ContentId, Document, render_markdown
    _RUST_AVAILABLE = True
except ImportError as _import_error:
    _RUST_AVAILABLE = False
    # Python 3 deletes the `as` variable when the except block exits.
    # Save the cause to a module-level name so class methods can chain it.
    _import_cause: BaseException = _import_error

    class _Missing:
        """Raises ImportError on any attribute access or call."""
        def __init__(self, name: str) -> None:
            self._name = name
        def __getattr__(self, _: str) -> None:  # type: ignore[override]
            raise ImportError(f"{self._name}: {_MISSING_MSG}") from _import_cause
        def __call__(self, *a: object, **kw: object) -> None:
            raise ImportError(f"{self._name}: {_MISSING_MSG}") from _import_cause

    ContentId = _Missing("ContentId")      # type: ignore[assignment,misc]
    Document = _Missing("Document")        # type: ignore[assignment,misc]
    render_markdown = _Missing("render_markdown")  # type: ignore[assignment]
```

The `_RUST_AVAILABLE` flag exists for tooling that needs to probe (CI, feature
detection). End users should never need to check it; they either have a working
install or they get a clear error the moment they try to use the API.

**3. The PyO3 layer is API design.**
A feature that cannot be expressed cleanly through `#[pymethods]` is a signal
that the Rust API needs refactoring, not that we should add complexity to the
binding layer. Keep `#[pymethods]` blocks thin: convert types, handle errors,
delegate to Rust.

**4. Arrow IPC for bulk data, bytes for raw content.**
When Rust needs to hand large structured data to Python (e.g. a list of AST
nodes, a batch of document metadata), use Arrow IPC (`pyarrow`). For raw binary
content (images, file bytes), use `&[u8]` / `bytes`. Never materialise large
data structures as Python dicts in a hot path.

### PyPI package map

| PyPI name | Rust crate | PyO3 module | Ships |
|---|---|---|---|
| `scrybe` | `scrybe-py` | `scrybe._rust` | Library: Document, render, panels, MCP client |
| `scrybe-cli` | `scrybe-cli` | — (binary) | Headless CLI: `scrybe render/lint/mermaid` |
| `scrybe-mermaid` | `scrybe-mermaid` | `scrybe_mermaid._rust` | Standalone PNG iTXt codec |
| `scrybe-mcp-server` | `scrybe-mcp-server` | — (binary) | Standalone MCP server |

### Build convention

Each published package has its own `pyproject.toml` at the crate root:

```toml
[build-system]
requires = ["maturin>=1.4,<2.0"]
build-backend = "maturin"

[project]
name = "scrybe"                       # PyPI package name
version = "0.5.20260506"

[tool.maturin]
module-name  = "scrybe._rust"         # import path inside Python
python-source = "python"              # Python package root
features     = ["python", "extension-module"]
manifest-path = "Cargo.toml"
```

The `python-source` directory contains only `__init__.py` (with the fail-fast
stub), `py.typed` (PEP 561 marker), and any pure-Python helpers that truly cannot
live in Rust.

### Cargo feature flags

Every PyO3 crate uses two feature flags:

| Flag | Purpose |
|---|---|
| `python` | Compiles the `#[pymodule]` and `#[pymethods]` blocks |
| `extension-module` | Passed to `pyo3` to build a `cdylib` compatible with Python's import system |

The default feature set is empty. CI runs `cargo check --all-features` to ensure
both flags compile cleanly even when running the Rust test suite.

## Consequences

### Positive

- `pip install scrybe` is the complete on-ramp for Python users. No Rust
  toolchain required.
- The Rust core is tested independently of the Python surface. Correctness
  is owned by Rust; API ergonomics are owned by the thin PyO3 wrapper.
- A clear failure mode: missing extension → immediate `ImportError` with
  actionable instructions. No silent `None`-propagation.
- Enforces API discipline: if something is hard to wrap, it's probably
  a design smell in the Rust layer.

### Negative / trade-offs

- Two test suites to maintain (Rust unit tests + Python integration tests).
  Mitigated by the fact that the Python tests are thin — they verify the
  binding, not the algorithm.
- `PyO3` panics surface as `RuntimeError` in Python with opaque tracebacks.
  All `#[pymethods]` that call fallible Rust must convert errors to
  `PyErr` explicitly; panics in the Rust layer are a bug.
- Platform wheel matrix (Linux x86_64, macOS arm64, macOS x86_64, Windows
  x86_64) requires a multi-platform CI job on every release tag.

## Alternatives considered

**Pure Python port** — rejected. Two implementations diverge silently and
double the maintenance surface without doubling capability.

**C extension via cffi** — rejected. PyO3 provides safe, ergonomic bindings
with no manual memory management and no separate header files.

**Single binary distribution (no Python package)** — rejected for `scrybe`
the library. `scrybe-cli` ships as a binary wheel, but the library surface
must be importable as a normal Python package so AI pipelines and `gila-plugin-*`
can `import scrybe` without spawning a subprocess.

## See also

- `scrybe-py/pyproject.toml` — reference `pyproject.toml` implementing this pattern
- `scrybe-py/python/scrybe/__init__.py` — fail-fast `_Missing` stub
- `kyln-scm` — predecessor project using the same pattern (`pip install kyln`)
- [maturin documentation](https://www.maturin.rs/)
- [PyO3 guide](https://pyo3.rs/)
