// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-py — PyO3 bindings for the Scrybe library.
//!
//! Module name: `scrybe._rust`
//! Build: `maturin develop --features python,extension-module`

pub use scrybe_core::{ContentAddressable, ContentId, Document, Workspace};
pub use scrybe_render::{render_html, Theme};

#[cfg(feature = "python")]
mod python {
    use pyo3::prelude::*;
    use scrybe_core::{ContentAddressable, ContentId, Document};
    use scrybe_render::{render_html, Theme};

    fn parse_theme(s: Option<&str>) -> Theme {
        match s {
            Some("dark") => Theme::Dark,
            Some("solarized") => Theme::Solarized,
            _ => Theme::Default,
        }
    }

    #[pyclass(name = "Document")]
    pub struct PyDocument {
        pub inner: Document,
    }

    #[pymethods]
    impl PyDocument {
        #[new]
        fn new(source: String) -> Self {
            Self {
                inner: Document::new(source),
            }
        }

        #[staticmethod]
        fn from_file(path: String) -> PyResult<Self> {
            let source = std::fs::read_to_string(&path)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
            Ok(Self {
                inner: Document::from_file(path.into(), source),
            })
        }

        #[getter]
        fn source(&self) -> &str {
            &self.inner.source
        }

        #[getter]
        fn title(&self) -> Option<String> {
            self.inner.title.clone()
        }

        fn content_id(&self) -> String {
            self.inner.content_id().as_hex().to_string()
        }

        fn render_html(&self, theme: Option<String>) -> String {
            render_html(&self.inner, parse_theme(theme.as_deref())).html
        }

        fn ast_title(&self) -> Option<String> {
            self.inner.title_from_ast()
        }

        fn __repr__(&self) -> String {
            let preview = &self.inner.source[..self.inner.source.len().min(40)];
            format!("Document({preview:?}…)")
        }

        fn __len__(&self) -> usize {
            self.inner.len()
        }
    }

    #[pyclass(name = "ContentId")]
    pub struct PyContentId {
        pub inner: ContentId,
    }

    #[pymethods]
    impl PyContentId {
        #[staticmethod]
        fn of(data: &[u8]) -> Self {
            Self {
                inner: ContentId::of(data),
            }
        }

        fn as_hex(&self) -> &str {
            self.inner.as_hex()
        }

        fn verify(&self, data: &[u8]) -> bool {
            self.inner.verify(data)
        }

        fn __str__(&self) -> &str {
            self.inner.as_hex()
        }

        fn __repr__(&self) -> String {
            format!("ContentId({:?})", self.inner.as_hex())
        }

        fn __eq__(&self, other: &Self) -> bool {
            self.inner == other.inner
        }

        fn __hash__(&self) -> u64 {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            self.inner.as_hex().hash(&mut h);
            h.finish()
        }
    }

    #[pyfunction]
    fn render_markdown(source: String, theme: Option<String>) -> String {
        let doc = Document::new(source);
        render_html(&doc, parse_theme(theme.as_deref())).html
    }

    #[pymodule]
    pub fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PyDocument>()?;
        m.add_class::<PyContentId>()?;
        m.add_function(wrap_pyfunction!(render_markdown, m)?)?;
        Ok(())
    }
}

#[cfg(feature = "python")]
pub use python::_rust;
