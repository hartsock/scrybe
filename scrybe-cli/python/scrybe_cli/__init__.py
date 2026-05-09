"""scrybe-cli — Scrybe headless CLI binary.

This package installs the `scrybe` command on your PATH, providing
render, lint, mermaid embed/extract, and the GUI integration RPC client.

For programmatic Python library access (Document, render_markdown,
ContentId, etc.), install scrybe-py:

    pip install scrybe-py
    >>> import scrybe
    >>> scrybe.render_markdown("# Hi", theme=None)

This module re-exports scrybe-py's library surface when it's installed,
for convenience — `import scrybe_cli; scrybe_cli.render_markdown(...)`
works the same as `import scrybe; scrybe.render_markdown(...)` if
scrybe-py is on the path.
"""

try:
    from importlib.metadata import PackageNotFoundError, version as _pkg_version

    try:
        __version__ = _pkg_version("scrybe-cli")
    except PackageNotFoundError:
        __version__ = "0.0.0+unknown"
except ImportError:
    __version__ = "0.0.0+unknown"


# Soft dependency on scrybe-py: re-export when present so users can drive
# the Rust library directly without juggling two imports.
try:
    from scrybe import ContentId, Document, render_markdown  # type: ignore[import-not-found]

    _SCRYBE_LIB_AVAILABLE = True
except ImportError:
    _SCRYBE_LIB_AVAILABLE = False

    _MISSING_MSG = (
        "scrybe-cli's library surface re-exports `scrybe-py` when it's "
        "installed. To get programmatic access:\n"
        "  pip install scrybe-py\n"
    )

    class _Missing:
        def __init__(self, name: str) -> None:
            self._name = name

        def __getattr__(self, _attr: str) -> None:
            raise ImportError(f"{self._name}: {_MISSING_MSG}")

        def __call__(self, *args: object, **kwargs: object) -> None:
            raise ImportError(f"{self._name}: {_MISSING_MSG}")

    ContentId = _Missing("ContentId")  # type: ignore[assignment,misc]
    Document = _Missing("Document")  # type: ignore[assignment,misc]
    render_markdown = _Missing("render_markdown")  # type: ignore[assignment]


__all__ = [
    "ContentId",
    "Document",
    "render_markdown",
    "_SCRYBE_LIB_AVAILABLE",
    "__version__",
]
