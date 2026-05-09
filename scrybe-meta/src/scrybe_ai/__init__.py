"""scrybe.ai — metapackage installing the full Scrybe Python toolkit.

`pip install scrybe.ai` pulls in:

- ``scrybe-py``  — PyO3 library, exposes ``import scrybe``
- ``scrybe-cli`` — the ``scrybe`` command-line tool
- ``scrybe-mcp-server`` — standalone MCP server binary
- ``scrybe-mermaid`` — PNG iTXt codec for Mermaid source embedding

The real APIs live in those packages; this module exists only as a
distribution anchor so the metapackage has a valid wheel.
"""

try:
    from importlib.metadata import PackageNotFoundError, version as _pkg_version

    try:
        __version__ = _pkg_version("scrybe.ai")
    except PackageNotFoundError:
        __version__ = "0.0.0+unknown"
except ImportError:
    __version__ = "0.0.0+unknown"

__all__ = ["__version__"]
