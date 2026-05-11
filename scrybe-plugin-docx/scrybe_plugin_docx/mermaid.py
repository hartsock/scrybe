# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright 2026 Shawn Hartsock and contributors

import shutil
import subprocess
import tempfile
from pathlib import Path


class MermaidUnavailable(Exception):
    pass


def render_mermaid_to_png(source: str) -> bytes:
    """Render a Mermaid diagram source string to PNG bytes via the mmdc CLI."""
    mmdc = shutil.which("mmdc")
    if mmdc is None:
        raise MermaidUnavailable(
            "mmdc not found on PATH. Install @mermaid-js/mermaid-cli "
            "(`npm install -g @mermaid-js/mermaid-cli`) or use --no-diagrams."
        )

    with tempfile.TemporaryDirectory() as tmpdir:
        src = Path(tmpdir) / "diagram.mmd"
        out = Path(tmpdir) / "diagram.png"
        src.write_text(source, encoding="utf-8")

        result = subprocess.run(
            [mmdc, "-i", str(src), "-o", str(out)],
            capture_output=True,
            timeout=60,
        )
        if result.returncode != 0:
            raise MermaidUnavailable(
                f"mmdc exited {result.returncode}: {result.stderr.decode(errors='replace')}"
            )
        return out.read_bytes()
