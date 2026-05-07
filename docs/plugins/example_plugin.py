#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Example Scrybe plugin.

Plugins receive the document source on stdin and write the
(optionally modified) source to stdout. Return empty to pass through.

Install: copy to ~/.config/scrybe/plugins/my_plugin.py
"""
import sys


def on_change(source: str) -> str:
    """Called on every document change. Return modified source or empty string."""
    # Example: add a word count footer
    words = len(source.split())
    if not source.endswith("\n"):
        source += "\n"
    source += f"\n---\n*{words} words*\n"
    return source


if __name__ == "__main__":
    source = sys.stdin.read()
    print(on_change(source), end="")
