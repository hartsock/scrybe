"""scrybe-mcp-server — Scrybe inbound MCP server binary.

This package installs the `scrybe-mcp-server` command on your PATH.
The MCP server speaks the Model Context Protocol over stdio, exposing
Scrybe's tool surface (open / read / section / edit / find / render /
embed / extract / lint / logs / quit / close_tab) to MCP clients like
Claude Code:

    claude mcp add scrybe -- scrybe-mcp-server stdio

For programmatic Python library access to the underlying render/lint
operations (without speaking MCP), install scrybe-py:

    pip install scrybe-py
    >>> import scrybe
    >>> scrybe.render_markdown("# Hi", theme=None)

This module re-exports scrybe-py's library surface when it's installed,
for convenience.
"""

try:
    from importlib.metadata import PackageNotFoundError, version as _pkg_version

    try:
        __version__ = _pkg_version("scrybe-mcp-server")
    except PackageNotFoundError:
        __version__ = "0.0.0+unknown"
except ImportError:
    __version__ = "0.0.0+unknown"


# Soft dependency on scrybe-py: re-export when present.
try:
    from scrybe import ContentId, Document, render_markdown  # type: ignore[import-not-found]

    _SCRYBE_LIB_AVAILABLE = True
except ImportError:
    _SCRYBE_LIB_AVAILABLE = False

    _MISSING_MSG = (
        "scrybe-mcp-server's library surface re-exports `scrybe-py` when "
        "it's installed. To get programmatic access:\n"
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
