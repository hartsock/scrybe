# SPDX-License-Identifier: AGPL-3.0-or-later
"""scrybe-mermaid — standalone PNG iTXt codec (Rust-backed).

Architecture: ADR-0001 — Python on the outside, Rust on the inside.
"""
__version__ = "0.5.20260506"

_MISSING_MSG = (
    "scrybe-mermaid requires the compiled Rust extension.\n"
    "  Install a pre-built wheel :  pip install scrybe-mermaid\n"
    "  Build from source          :  maturin develop --features python,extension-module"
)

try:
    from scrybe_mermaid._rust import embed, extract, MermaidPayload
    _RUST_AVAILABLE = True
except ImportError as _import_error:
    _RUST_AVAILABLE = False
    _import_cause: BaseException = _import_error

    class _Missing:
        def __init__(self, name: str) -> None: self._name = name
        def __getattr__(self, _: str) -> None:  # type: ignore[override]
            raise ImportError(f"{self._name}: {_MISSING_MSG}") from _import_cause
        def __call__(self, *a: object, **kw: object) -> None:
            raise ImportError(f"{self._name}: {_MISSING_MSG}") from _import_cause

    embed = _Missing("embed")          # type: ignore[assignment]
    extract = _Missing("extract")      # type: ignore[assignment]
    MermaidPayload = _Missing("MermaidPayload")  # type: ignore[assignment,misc]

__all__ = ["embed", "extract", "MermaidPayload", "_RUST_AVAILABLE", "__version__"]
