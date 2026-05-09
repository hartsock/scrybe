# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright 2026 Shawn Hartsock and contributors

"""scrybe-docx — render Markdown to a Word (.docx) file.

Usage:
    scrybe-docx [INPUT] -o OUTPUT [--no-diagrams]
    cat file.md | scrybe-docx -o output.docx
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .renderer import MarkdownToDocx


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="scrybe-docx",
        description="Render a Markdown file to Word (.docx) with embedded Mermaid diagrams.",
    )
    parser.add_argument(
        "input",
        nargs="?",
        type=Path,
        metavar="FILE",
        help="Input Markdown file (default: stdin).",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=Path("output.docx"),
        metavar="OUTPUT",
        help="Output .docx path (default: output.docx).",
    )
    parser.add_argument(
        "--no-diagrams",
        action="store_true",
        help="Skip Mermaid rendering; keep fenced blocks as monospace text.",
    )

    args = parser.parse_args(argv)

    if args.input:
        source = args.input.read_text(encoding="utf-8")
    else:
        source = sys.stdin.read()

    try:
        doc = MarkdownToDocx(source, render_diagrams=not args.no_diagrams).build()
    except ImportError as exc:
        print(f"error: missing dependency — {exc}", file=sys.stderr)
        print("  pip install scrybe-plugin-docx", file=sys.stderr)
        return 1

    doc.save(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
