# SPDX-License-Identifier: Apache-2.0
"""Integration test: README Python quick-start example round-trips correctly."""
import struct
import zlib
from pathlib import Path

import pytest

scrybe_mermaid = pytest.importorskip("scrybe_mermaid")


def _minimal_png() -> bytes:
    """Build a 1×1 white RGB PNG from stdlib primitives — no external deps."""
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(name: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(name + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + name + data + struct.pack(">I", crc)

    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)  # 1×1, 8-bit RGB
    idat = zlib.compress(b"\x00\xff\xff\xff")             # filter=0, R G B = white
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def test_readme_quickstart_roundtrip(tmp_path: Path) -> None:
    """README quick-start example embeds and extracts Mermaid source correctly."""
    if not scrybe_mermaid._RUST_AVAILABLE:
        pytest.skip("Rust extension not built — run: maturin develop --release")

    (tmp_path / "diagram.png").write_bytes(_minimal_png())

    # ── verbatim from README ──────────────────────────────────────────────────
    source = """
graph TD
    A[Christmas] -->|Get money| B(Go shopping)
    B --> C{Let me think}
    C -->|One| D[Laptop]
    C -->|Two| E[iPhone]
"""

    png_in = (tmp_path / "diagram.png").read_bytes()
    png_out = scrybe_mermaid.embed(png_in, source)
    (tmp_path / "diagram-with-source.png").write_bytes(png_out)

    payload = scrybe_mermaid.extract(png_out)
    if payload.source != source:
        raise ValueError("Round-trip mismatch")
    print(f"Round-tripped {len(payload.source)} chars; sha256={payload.sha256[:12]}…")
    # ─────────────────────────────────────────────────────────────────────────

    assert (tmp_path / "diagram-with-source.png").exists()
    assert payload.source == source
    assert len(payload.sha256) == 64  # SHA-256 hex is always 64 chars
