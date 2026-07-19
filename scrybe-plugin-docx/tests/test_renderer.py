# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright 2026 Shawn Hartsock and contributors

"""Unit tests for scrybe_plugin_docx.renderer.

Requires python-docx and mistune. Tests are skipped when deps are absent.
"""

from __future__ import annotations

import pytest

docx = pytest.importorskip("docx", reason="python-docx not installed")
mistune = pytest.importorskip("mistune", reason="mistune not installed")

from scrybe_plugin_docx.renderer import MarkdownToDocx, _inline_text  # noqa: E402

# ---------------------------------------------------------------------------
# _inline_text helper
# ---------------------------------------------------------------------------


def test_inline_text_plain():
    tokens = [{"type": "raw", "raw": "hello world"}]
    assert _inline_text(tokens) == "hello world"


def test_inline_text_nested_strong():
    tokens = [
        {"type": "raw", "raw": "foo "},
        {"type": "strong", "children": [{"type": "raw", "raw": "bar"}]},
    ]
    assert _inline_text(tokens) == "foo bar"


def test_inline_text_softline():
    tokens = [
        {"type": "raw", "raw": "a"},
        {"type": "softline"},
        {"type": "raw", "raw": "b"},
    ]
    assert _inline_text(tokens) == "a b"


# ---------------------------------------------------------------------------
# MarkdownToDocx.build()
# ---------------------------------------------------------------------------


def _build(md: str, **kwargs):
    return MarkdownToDocx(md, **kwargs).build()


def test_heading_level1():
    doc = _build("# Hello")
    paras = [p for p in doc.paragraphs if p.text.strip()]
    assert paras[0].text == "Hello"
    assert paras[0].style.name.startswith("Heading")


def test_heading_levels():
    doc = _build("# H1\n\n## H2\n\n### H3")
    headings = [p for p in doc.paragraphs if p.text.strip()]
    assert len(headings) == 3
    assert headings[0].style.name == "Heading 1"
    assert headings[1].style.name == "Heading 2"
    assert headings[2].style.name == "Heading 3"


def test_paragraph_plain():
    doc = _build("Hello world.")
    paras = [p for p in doc.paragraphs if p.text.strip()]
    assert paras[0].text == "Hello world."


def test_paragraph_bold_italic():
    doc = _build("**bold** and *italic*")
    p = next(p for p in doc.paragraphs if p.text.strip())
    runs = [r for r in p.runs if r.text.strip()]
    bold_runs = [r for r in runs if r.bold]
    italic_runs = [r for r in runs if r.italic]
    assert any(r.text == "bold" for r in bold_runs)
    assert any(r.text == "italic" for r in italic_runs)


def test_bullet_list():
    doc = _build("- apple\n- banana\n- cherry")
    list_paras = [p for p in doc.paragraphs if p.style.name == "List Bullet"]
    assert len(list_paras) == 3
    texts = [p.text for p in list_paras]
    assert "apple" in texts
    assert "cherry" in texts


def test_ordered_list():
    doc = _build("1. first\n2. second")
    list_paras = [p for p in doc.paragraphs if p.style.name == "List Number"]
    assert len(list_paras) == 2


def test_code_block_monospace():
    doc = _build("```python\nprint('hi')\n```")
    paras = [p for p in doc.paragraphs if p.text.strip()]
    assert any(p.text == "print('hi')" for p in paras)
    code_para = next(p for p in paras if p.text == "print('hi')")
    assert code_para.runs[0].font.name == "Courier New"


def test_table():
    md = "| A | B |\n|---|---|\n| 1 | 2 |"
    doc = _build(md)
    assert len(doc.tables) == 1
    table = doc.tables[0]
    assert table.cell(0, 0).text.strip() == "A"
    assert table.cell(1, 1).text.strip() == "2"


def test_mermaid_no_diagrams_fallback():
    md = "```mermaid\ngraph TD; A-->B\n```"
    doc = _build(md, render_diagrams=False)
    paras = [p for p in doc.paragraphs if p.text.strip()]
    assert any("A-->B" in p.text for p in paras)


def test_embed_mermaid_source_warns_and_falls_back_on_failure(monkeypatch, capsys):
    """When the scrybe_mermaid codec is unavailable or raises, degrade LOUDLY:
    keep the raw PNG so export never crashes, but emit a warning to stderr so
    the un-embedded (non-round-trippable) figure is visible, not silent."""
    import sys
    import types

    import scrybe_plugin_docx.renderer as r

    fake = types.ModuleType("scrybe_mermaid")

    def _boom(_png, _source):
        raise RuntimeError("codec unavailable")

    fake.embed = _boom  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "scrybe_mermaid", fake)

    raw = b"\x89PNG\r\n\x1a\n-rawbytes"
    out = r._embed_mermaid_source(raw, "graph TD; A-->B")

    # Graceful: the original PNG bytes come back unchanged (export never fails).
    assert out == raw
    # Loud: a warning naming the degradation reaches stderr.
    err = capsys.readouterr().err
    assert "WARNING" in err
    assert "scrybe-mermaid" in err


def test_mermaid_mmdc_unavailable_falls_back(monkeypatch):
    """When mmdc is not on PATH, mermaid block falls back to monospace text."""
    import scrybe_plugin_docx.mermaid as m
    import scrybe_plugin_docx.renderer as r

    def _raise(_):
        raise m.MermaidUnavailable("no mmdc")

    # renderer.py imports `render_mermaid_to_png` by value, so patch the name
    # bound in the renderer module (patching the mermaid module's attribute
    # would not affect the already-imported reference).
    monkeypatch.setattr(r, "render_mermaid_to_png", _raise)

    md = "```mermaid\ngraph TD; A-->B\n```"
    doc = _build(md, render_diagrams=True)
    paras = [p for p in doc.paragraphs if p.text.strip()]
    assert any("A-->B" in p.text for p in paras)


def test_empty_input():
    doc = _build("")
    # Should produce a Document without errors.
    assert doc is not None


def test_thematic_break():
    doc = _build("above\n\n---\n\nbelow")
    texts = [p.text for p in doc.paragraphs if p.text.strip()]
    assert any("─" in t for t in texts)


def test_inline_code():
    doc = _build("use `config.toml` here")
    p = next(p for p in doc.paragraphs if p.text.strip())
    runs = {r.text: r for r in p.runs if r.text.strip()}
    assert "config.toml" in runs
    assert runs["config.toml"].font.name == "Courier New"


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def test_cli_stdin(tmp_path, monkeypatch):
    import io
    import sys

    from scrybe_plugin_docx.main import main

    out = tmp_path / "out.docx"
    monkeypatch.setattr(sys, "stdin", io.StringIO("# Title\n\nBody text."))
    rc = main(["-o", str(out)])
    assert rc == 0
    assert out.exists()


def test_cli_file_input(tmp_path):
    from scrybe_plugin_docx.main import main

    src = tmp_path / "input.md"
    src.write_text("# Hello\n\nWorld.", encoding="utf-8")
    out = tmp_path / "out.docx"
    rc = main([str(src), "-o", str(out)])
    assert rc == 0
    assert out.exists()
