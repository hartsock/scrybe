"""Tests for scrybe PyO3 bindings.
Run after: maturin develop --features python,extension-module
"""

import pytest

try:
    from scrybe._rust import ContentDigest, ContentId, Document, render_markdown

    RUST_AVAILABLE = True
except ImportError:
    RUST_AVAILABLE = False

skip_if_no_rust = pytest.mark.skipif(
    not RUST_AVAILABLE, reason="Rust extension not built"
)


@skip_if_no_rust
def test_document_creates():
    doc = Document("# Hello")
    assert len(doc) == 7


@skip_if_no_rust
def test_document_content_digest_stable():
    assert Document("x").content_digest() == Document("x").content_digest()


@skip_if_no_rust
def test_document_content_digest_differs():
    assert Document("a").content_digest() != Document("b").content_digest()


@skip_if_no_rust
def test_document_render_html():
    html = Document("# H1").render_html()
    assert "<h1" in html


@skip_if_no_rust
def test_document_ast_title():
    assert Document("# My Title\n\nBody.").ast_title() == "My Title"


@skip_if_no_rust
def test_content_digest_verify():
    digest = ContentDigest.of(b"hello")
    assert digest.verify(b"hello")
    assert not digest.verify(b"world")


@skip_if_no_rust
def test_content_digest_str():
    digest = ContentDigest.of(b"hello")
    assert len(str(digest)) == 64  # 32-byte BLAKE3 hex


# --- Deprecated-name compatibility (rename, not a migration) ---------------


@skip_if_no_rust
def test_deprecated_content_id_alias_is_content_digest():
    """`ContentId` was a false name (bare BLAKE3 hex, not a CID); the alias
    must keep working and produce identical values."""
    assert ContentId is ContentDigest
    assert str(ContentId.of(b"hello")) == str(ContentDigest.of(b"hello"))


@skip_if_no_rust
def test_deprecated_document_content_id_matches_digest():
    doc = Document("# Hello")
    assert doc.content_id() == doc.content_digest()


@skip_if_no_rust
def test_render_markdown():
    html = render_markdown("**bold**")
    assert "<strong>" in html


@skip_if_no_rust
def test_render_markdown_dark_theme():
    html = render_markdown("# H", "dark")
    assert "<h1" in html
