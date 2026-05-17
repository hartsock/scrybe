// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! PyO3 Python bindings — exposed as `scrybe_mermaid_render._rust`.
//!
//! Drake Phase 6: wire up once `render_to_svg` and `render_to_png` are implemented.
//! Pattern mirrors `scrybe-mermaid/src/lib.rs`.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Render Mermaid source to an SVG string.
#[pyfunction]
#[pyo3(name = "render_to_svg")]
fn py_render_to_svg(source: &str) -> PyResult<String> {
    crate::render_to_svg(source).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Render Mermaid source to PNG bytes.
///
/// Requires the crate to be compiled with `--features png`.
#[pyfunction]
#[pyo3(name = "render_to_png")]
fn py_render_to_png<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(feature = "png")]
    {
        let bytes =
            crate::render_to_png(source).map_err(|e| PyValueError::new_err(e.to_string()))?;
        return Ok(PyBytes::new(py, &bytes));
    }
    #[cfg(not(feature = "png"))]
    {
        let _ = (py, source);
        Err(PyValueError::new_err(
            "render_to_png requires the `png` feature (rebuild with --features png)",
        ))
    }
}

#[pymodule]
pub fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_render_to_svg, m)?)?;
    m.add_function(wrap_pyfunction!(py_render_to_png, m)?)?;
    Ok(())
}
