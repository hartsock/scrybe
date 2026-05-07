# SPDX-License-Identifier: AGPL-3.0-or-later
"""scrybe-mcp-server — standalone Scrybe MCP server.

Install: pip install scrybe-mcp-server
Use:     python -m scrybe_mcp_server stdio
Or:      claude mcp add scrybe -- python -m scrybe_mcp_server stdio
"""
__version__ = "0.5.20260506"

import subprocess
import sys


def _run() -> None:
    """Entry point: delegates to the compiled scrybe-mcp-server binary."""
    import shutil
    binary = shutil.which("scrybe-mcp-server")
    if binary is None:
        print("scrybe-mcp-server binary not found. Reinstall: pip install scrybe-mcp-server", file=sys.stderr)
        sys.exit(1)
    sys.exit(subprocess.call([binary] + sys.argv[1:]))


if __name__ == "__main__":
    _run()
