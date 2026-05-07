// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The Document type — the central editing unit in Scrybe.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ast::Ast;
use crate::content::{ContentAddressable, ContentId};
use crate::error::ScrybeError;

/// A Scrybe document — Markdown source with associated metadata.
///
/// Holds raw source text and a lazily-computed content identifier.
/// Rendering happens in `scrybe-render` (P1.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// The Markdown source text.
    pub source: String,
    /// Optional on-disk path. `None` for untitled / in-memory documents.
    pub path: Option<PathBuf>,
    /// Document title extracted from the first H1, if present.
    pub title: Option<String>,
}

impl Document {
    /// Creates a new in-memory document with the given source.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            path: None,
            title: None,
        }
    }

    /// Creates a document from a file path and its source.
    ///
    /// The title is populated automatically from the first H1 heading.
    pub fn from_file(path: PathBuf, source: impl Into<String>) -> Self {
        let source = source.into();
        let title = Ast::parse(&source).title();
        Self {
            source,
            path: Some(path),
            title,
        }
    }

    /// Returns the byte length of the source.
    pub fn len(&self) -> usize {
        self.source.len()
    }

    /// Returns `true` if the source is empty.
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    // -----------------------------------------------------------------------
    // AST helpers
    // -----------------------------------------------------------------------

    /// Parses the document source and returns its AST.
    pub fn ast(&self) -> Ast {
        Ast::parse(&self.source)
    }

    /// Returns the title extracted from the first H1 in the source, if any.
    pub fn title_from_ast(&self) -> Option<String> {
        self.ast().title()
    }

    // -----------------------------------------------------------------------
    // CBOR serialization
    // -----------------------------------------------------------------------

    /// Serialises this document to deterministic CBOR bytes.
    pub fn to_cbor(&self) -> Result<Vec<u8>, ScrybeError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).map_err(|e| ScrybeError::Cbor(e.to_string()))?;
        Ok(buf)
    }

    /// Deserialises a document from CBOR bytes.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, ScrybeError> {
        ciborium::from_reader(bytes).map_err(|e| ScrybeError::Cbor(e.to_string()))
    }
}

impl ContentAddressable for Document {
    fn content_id(&self) -> ContentId {
        ContentId::of(self.source.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_content_id_stable() {
        let doc = Document::new("# Hello\n\nWorld.");
        let id1 = doc.content_id();
        let id2 = doc.content_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_document_is_empty() {
        assert!(Document::new("").is_empty());
        assert!(!Document::new("x").is_empty());
    }

    // -----------------------------------------------------------------------
    // CBOR roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_cbor_roundtrip_basic() {
        let doc = Document::new("# Hello\n\nWorld.");
        let bytes = doc.to_cbor().expect("encode");
        let doc2 = Document::from_cbor(&bytes).expect("decode");
        assert_eq!(doc.source, doc2.source);
        assert_eq!(doc.title, doc2.title);
        assert_eq!(doc.path, doc2.path);
    }

    #[test]
    fn test_cbor_roundtrip_with_path() {
        let doc = Document::from_file(PathBuf::from("/tmp/test.md"), "# Test\n\nContent.");
        let bytes = doc.to_cbor().expect("encode");
        let doc2 = Document::from_cbor(&bytes).expect("decode");
        assert_eq!(doc.source, doc2.source);
        assert_eq!(doc.path, doc2.path);
    }

    #[test]
    fn test_cbor_roundtrip_empty() {
        let doc = Document::new("");
        let bytes = doc.to_cbor().expect("encode");
        let doc2 = Document::from_cbor(&bytes).expect("decode");
        assert!(doc2.is_empty());
    }

    #[test]
    fn test_cbor_invalid_bytes() {
        let result = Document::from_cbor(b"\xff\xfe garbage");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // AST integration
    // -----------------------------------------------------------------------

    #[test]
    fn test_ast_returns_parsed_ast() {
        let doc = Document::new("# Title\n\nParagraph.\n");
        let ast = doc.ast();
        assert!(!ast.nodes.is_empty());
    }

    #[test]
    fn test_title_from_ast_h1() {
        let doc = Document::new("# My Document\n\nSome text.\n");
        assert_eq!(doc.title_from_ast(), Some("My Document".to_string()));
    }

    #[test]
    fn test_title_from_ast_none_when_no_h1() {
        let doc = Document::new("## Just a subheading\n");
        assert_eq!(doc.title_from_ast(), None);
    }

    #[test]
    fn test_from_file_populates_title() {
        let doc = Document::from_file(PathBuf::from("/tmp/doc.md"), "# Auto Title\n\nBody text.\n");
        assert_eq!(doc.title, Some("Auto Title".to_string()));
    }

    #[test]
    fn test_from_file_no_h1_title_is_none() {
        let doc = Document::from_file(PathBuf::from("/tmp/doc.md"), "No heading here.\n");
        assert_eq!(doc.title, None);
    }
}
