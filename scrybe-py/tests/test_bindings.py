"""Tests for scrybe PyO3 bindings.
Run after: maturin develop --features python,extension-module
"""
import pytest

try:
    from scrybe._rust import Document, ContentId, render_markdown
    RUST_AVAILABLE = True
except ImportError:
    RUST_AVAILABLE = False

skip_if_no_rust = pytest.mark.skipif(not RUST_AVAILABLE, reason="Rust extension not built")

@skip_if_no_rust
def test_document_creates():
    doc = Document("# Hello")
    assert len(doc) == 7

@skip_if_no_rust
def test_document_content_id_stable():
    assert Document("x").content_id() == Document("x").content_id()

@skip_if_no_rust
def test_document_content_id_differs():
    assert Document("a").content_id() != Document("b").content_id()

@skip_if_no_rust
def test_document_render_html():
    html = Document("# H1").render_html()
    assert "<h1" in html

@skip_if_no_rust
def test_document_ast_title():
    assert Document("# My Title\n\nBody.").ast_title() == "My Title"

@skip_if_no_rust
def test_content_id_verify():
    cid = ContentId.of(b"hello")
    assert cid.verify(b"hello")
    assert not cid.verify(b"world")

@skip_if_no_rust
def test_content_id_str():
    cid = ContentId.of(b"hello")
    assert len(str(cid)) == 64  # 32-byte BLAKE3 hex

@skip_if_no_rust
def test_render_markdown():
    html = render_markdown("**bold**")
    assert "<strong>" in html

@skip_if_no_rust
def test_render_markdown_dark_theme():
    html = render_markdown("# H", "dark")
    assert "<h1" in html
