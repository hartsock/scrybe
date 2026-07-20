// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-mermaid — standalone PNG iTXt codec.
//!
//! Embeds Mermaid diagram source as an iTXt metadata chunk inside a PNG.
//! The PNG is fully valid; any viewer shows the rendered image. The source
//! travels with the image and can be extracted later.
//!
//! # Codec format
//!
//! iTXt chunk key: `scrybe-mermaid`
//! Value: JSON `{ "source": "<mermaid source>", "sha256": "<hex>", "uuid": "<v4>" }`
//!
//! # Verification
//!
//! [`extract`] verifies by default: it recomputes the SHA-256 of the
//! extracted source and compares it against the stored digest. A mismatch is
//! [`MermaidError::VerificationFailed`] (with both digests populated); a
//! payload that carries no digest extracts successfully but is explicitly
//! flagged [`VerificationStatus::NoDigest`] — it is never reported as
//! verified. Use [`extract_unverified`] to read the raw stored fields
//! without any check (forensics on tampered or foreign payloads).

pub mod codec;
pub mod error;

pub use codec::{embed, embed_with_uuid, extract, extract_unverified};
pub use error::MermaidError;

/// The raw stored fields of an embedded payload, exactly as read from the
/// PNG — no verification has been performed. Returned by
/// [`extract_unverified`]; [`extract`] returns [`VerifiedPayload`] instead.
#[derive(Debug, Clone)]
pub struct MermaidPayload {
    /// The Mermaid diagram source text.
    pub source: String,
    /// SHA-256 of the source bytes as *stored at embed time*. Empty string
    /// when the payload carries no digest. NOT checked against `source` —
    /// use [`extract`] for that.
    pub sha256: String,
    /// Per-artifact UUID identifying this embed instance. Empty string when the
    /// PNG was embedded before UUIDs were added (older payloads carry only
    /// `source` + `sha256`).
    pub uuid: String,
}

/// Outcome of the digest check [`extract`] performs.
///
/// Verification *failure* (stored digest present but mismatched) is not a
/// status — it is the error [`MermaidError::VerificationFailed`], so the
/// three outcomes are structurally distinct:
///
/// 1. digest present and matching → `Ok` + [`VerificationStatus::Verified`]
/// 2. digest present and mismatched → `Err(MermaidError::VerificationFailed)`
/// 3. no digest stored → `Ok` + [`VerificationStatus::NoDigest`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    /// The stored digest was recomputed from the extracted source and matched.
    Verified {
        /// Digest algorithm used ("sha256").
        algorithm: &'static str,
        /// The verified hex digest (stored == recomputed).
        digest: String,
    },
    /// The payload carries no digest (an older or foreign embedder), so there
    /// was nothing to verify. This is explicitly *not* "verified".
    NoDigest,
}

impl VerificationStatus {
    /// True only when a stored digest was recomputed and matched.
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }

    /// The verified digest, when one was present and matched.
    pub fn digest(&self) -> Option<&str> {
        match self {
            Self::Verified { digest, .. } => Some(digest),
            Self::NoDigest => None,
        }
    }
}

/// An extracted payload whose digest check has been performed. Returned by
/// [`extract`].
#[derive(Debug, Clone)]
pub struct VerifiedPayload {
    /// The Mermaid diagram source text.
    pub source: String,
    /// Per-artifact UUID identifying this embed instance (empty string for
    /// pre-UUID payloads).
    pub uuid: String,
    /// How the digest check went. Never a mismatch — a mismatch is
    /// [`MermaidError::VerificationFailed`], not a successful extraction.
    pub verification: VerificationStatus,
}

impl VerifiedPayload {
    /// True only when the stored digest was recomputed and matched.
    pub fn is_verified(&self) -> bool {
        self.verification.is_verified()
    }

    /// The verified SHA-256 hex digest, when one was present and matched.
    pub fn sha256(&self) -> Option<&str> {
        self.verification.digest()
    }
}

// ── Python bindings ─────────────────────────────────────────────────────────
//
// Exposes `embed`, `extract`, `extract_unverified`, and `MermaidPayload` to
// Python under the `scrybe_mermaid._rust` module. Mirrors `scrybe-py`'s pyo3
// conventions.
//
// Python developers can:
//
//     >>> import scrybe_mermaid
//     >>> with open("diagram.png", "rb") as f: png = f.read()
//     >>> embedded = scrybe_mermaid.embed(png, "graph TD; A-->B")
//     >>> payload = scrybe_mermaid.extract(embedded)  # verifies sha256
//     >>> payload.source
//     'graph TD; A-->B'
//     >>> payload.sha256
//     '83af36...'
//     >>> payload.verified
//     True

#[cfg(feature = "python")]
mod python {
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;

    /// Embed Mermaid source as an iTXt metadata chunk in a PNG.
    /// Returns the new PNG bytes; the original is unchanged.
    #[pyfunction]
    #[pyo3(name = "embed")]
    fn py_embed<'py>(
        py: Python<'py>,
        png_bytes: &[u8],
        source: &str,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let out = crate::codec::embed(png_bytes, source)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(PyBytes::new(py, &out))
    }

    /// Extract embedded Mermaid source from a PNG, verifying its digest.
    /// Raises `ValueError` if the PNG has no iTXt chunk with key `scrybe-mermaid`
    /// or if the embedded sha256 doesn't match the source bytes. A payload
    /// that carries no digest (older or foreign embedder) extracts with
    /// `verified == False` and `sha256 == ""` — it is never reported verified.
    #[pyfunction]
    #[pyo3(name = "extract")]
    fn py_extract(png_bytes: &[u8]) -> PyResult<PyMermaidPayload> {
        crate::codec::extract(png_bytes)
            .map(|p| PyMermaidPayload {
                source: p.source,
                sha256: p.sha256().unwrap_or_default().to_string(),
                uuid: p.uuid,
                verified: p.is_verified(),
            })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Extract the raw stored payload WITHOUT verifying the digest — for
    /// forensics on tampered or foreign payloads. `sha256` is the stored
    /// value as-is (possibly wrong, possibly empty) and `verified` is always
    /// `False`. Prefer `extract`, which verifies.
    #[pyfunction]
    #[pyo3(name = "extract_unverified")]
    fn py_extract_unverified(png_bytes: &[u8]) -> PyResult<PyMermaidPayload> {
        crate::codec::extract_unverified(png_bytes)
            .map(|p| PyMermaidPayload {
                source: p.source,
                sha256: p.sha256,
                uuid: p.uuid,
                verified: false,
            })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Result of `extract` / `extract_unverified`. `source` is the Mermaid
    /// diagram text; `sha256` is the hex digest of `source.encode('utf-8')`
    /// at embed time (empty when the payload carries none); `uuid` is the
    /// per-artifact id (empty for pre-UUID payloads); `verified` is `True`
    /// only when `extract` recomputed the digest and it matched.
    #[pyclass(name = "MermaidPayload", skip_from_py_object)]
    #[derive(Clone)]
    struct PyMermaidPayload {
        #[pyo3(get)]
        source: String,
        #[pyo3(get)]
        sha256: String,
        #[pyo3(get)]
        uuid: String,
        #[pyo3(get)]
        verified: bool,
    }

    #[pymethods]
    impl PyMermaidPayload {
        fn __repr__(&self) -> String {
            format!(
                "MermaidPayload(source={:?}, sha256={:?}, uuid={:?}, verified={})",
                self.source, self.sha256, self.uuid, self.verified
            )
        }
    }

    #[pymodule]
    pub fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PyMermaidPayload>()?;
        m.add_function(wrap_pyfunction!(py_embed, m)?)?;
        m.add_function(wrap_pyfunction!(py_extract, m)?)?;
        m.add_function(wrap_pyfunction!(py_extract_unverified, m)?)?;
        Ok(())
    }
}

#[cfg(feature = "python")]
pub use python::_rust;
