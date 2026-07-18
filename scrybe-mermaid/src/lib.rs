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
//! Value: JSON `{ "source": "<mermaid source>", "sha256": "<hex>" }`

pub mod codec;
pub mod error;

pub use codec::{embed, extract};
pub use error::MermaidError;

/// The result of embedding or extracting Mermaid source.
#[derive(Debug, Clone)]
pub struct MermaidPayload {
    /// The Mermaid diagram source text.
    pub source: String,
    /// SHA-256 of the source bytes (for integrity verification).
    pub sha256: String,
    /// Per-artifact UUID identifying this embed instance. Empty string when the
    /// PNG was embedded before UUIDs were added (older payloads carry only
    /// `source` + `sha256`).
    pub uuid: String,
}

// ── Python bindings ─────────────────────────────────────────────────────────
//
// Exposes `embed`, `extract`, and `MermaidPayload` to Python under the
// `scrybe_mermaid._rust` module. Mirrors `scrybe-py`'s pyo3 v0.28 conventions.
//
// Python developers can:
//
//     >>> import scrybe_mermaid
//     >>> with open("diagram.png", "rb") as f: png = f.read()
//     >>> embedded = scrybe_mermaid.embed(png, "graph TD; A-->B")
//     >>> payload = scrybe_mermaid.extract(embedded)
//     >>> payload.source
//     'graph TD; A-->B'
//     >>> payload.sha256
//     '83af36...'

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

    /// Extract embedded Mermaid source from a PNG.
    /// Raises `ValueError` if the PNG has no iTXt chunk with key `scrybe-mermaid`
    /// or if the embedded sha256 doesn't match the source bytes.
    #[pyfunction]
    #[pyo3(name = "extract")]
    fn py_extract(png_bytes: &[u8]) -> PyResult<PyMermaidPayload> {
        crate::codec::extract(png_bytes)
            .map(|p| PyMermaidPayload {
                source: p.source,
                sha256: p.sha256,
                uuid: p.uuid,
            })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Result of `extract`. `source` is the Mermaid diagram text; `sha256`
    /// is the hex digest of `source.encode('utf-8')` at embed time; `uuid` is
    /// the per-artifact id (empty for pre-UUID payloads).
    #[pyclass(name = "MermaidPayload", skip_from_py_object)]
    #[derive(Clone)]
    struct PyMermaidPayload {
        #[pyo3(get)]
        source: String,
        #[pyo3(get)]
        sha256: String,
        #[pyo3(get)]
        uuid: String,
    }

    #[pymethods]
    impl PyMermaidPayload {
        fn __repr__(&self) -> String {
            format!(
                "MermaidPayload(source={:?}, sha256={:?}, uuid={:?})",
                self.source, self.sha256, self.uuid
            )
        }
    }

    #[pymodule]
    pub fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PyMermaidPayload>()?;
        m.add_function(wrap_pyfunction!(py_embed, m)?)?;
        m.add_function(wrap_pyfunction!(py_extract, m)?)?;
        Ok(())
    }
}

#[cfg(feature = "python")]
pub use python::_rust;
