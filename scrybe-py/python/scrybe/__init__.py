# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 Shawn Hartsock and contributors
"""Scrybe — MCP-native Markdown editor library (Rust-backed).

Architecture: "Python on the outside, Rust on the inside."
See docs/adr/0001-python-outside-rust-inside.md for the full decision record.

The public API is implemented in Rust (scrybe._rust). If the compiled
extension is absent, every public symbol raises ImportError on first use
with clear instructions — there is no silent pure-Python fallback.
"""

# Track the installed package version automatically so we never ship a stale
# string. importlib.metadata is stdlib on Python 3.8+; falls back to a
# placeholder when the source tree is run uninstalled.
try:
    from importlib.metadata import PackageNotFoundError, version as _pkg_version

    try:
        __version__ = _pkg_version("scrybe-py")
    except PackageNotFoundError:
        __version__ = "0.0.0+unknown"
except ImportError:
    __version__ = "0.0.0+unknown"

try:
    from scrybe._rust import ContentDigest, Document, render_markdown

    # Deprecated alias — the old name over-promised: this is a bare BLAKE3
    # hex digest, not an IPFS/IPLD CID. Prefer ContentDigest.
    ContentId = ContentDigest

    _RUST_AVAILABLE = True

except ImportError as _import_error:
    _RUST_AVAILABLE = False

    # Python 3 deletes the `as` variable when the except block exits, so save
    # the cause to a module-level name before the class methods need it.
    _import_cause: BaseException = _import_error

    _MISSING_MSG = (
        "scrybe requires the compiled Rust extension.\n"
        "  Install a pre-built wheel :  pip install scrybe\n"
        "  Build from source          :  maturin develop --features python,extension-module"
    )

    class _Missing:
        """Proxy that raises ImportError on any access.

        Used to give a clear, actionable error when scrybe._rust is absent
        rather than propagating AttributeError or TypeError from a None value.
        """

        def __init__(self, name: str) -> None:
            self._name = name

        def __getattr__(self, _attr: str) -> None:  # type: ignore[override]
            raise ImportError(f"{self._name}: {_MISSING_MSG}") from _import_cause

        def __call__(self, *args: object, **kwargs: object) -> None:
            raise ImportError(f"{self._name}: {_MISSING_MSG}") from _import_cause

        def __repr__(self) -> str:
            return f"<scrybe.{self._name}: Rust extension not built>"

    ContentDigest = _Missing("ContentDigest")  # type: ignore[assignment,misc]
    ContentId = _Missing("ContentId")  # type: ignore[assignment,misc]
    Document = _Missing("Document")  # type: ignore[assignment,misc]
    render_markdown = _Missing("render_markdown")  # type: ignore[assignment]

__all__ = [
    "ContentDigest",
    "ContentId",
    "Document",
    "render_markdown",
    "_RUST_AVAILABLE",
    "__version__",
]
