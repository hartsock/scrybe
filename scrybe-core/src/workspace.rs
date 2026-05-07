// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Workspace — a collection of open documents and shared editor state.

use std::collections::HashMap;
use std::path::PathBuf;

use uuid::Uuid;

use crate::document::Document;

/// A unique identifier for an open document within a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(Uuid);

impl DocumentId {
    /// Creates a new random document ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

/// The editor workspace — open documents and their IDs.
#[derive(Debug, Default)]
pub struct Workspace {
    documents: HashMap<DocumentId, Document>,
    /// Root directory for the workspace (e.g. the repo root).
    pub root: Option<PathBuf>,
}

impl Workspace {
    /// Creates an empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a document and returns its ID.
    pub fn open(&mut self, doc: Document) -> DocumentId {
        let id = DocumentId::new();
        self.documents.insert(id, doc);
        id
    }

    /// Returns a reference to a document by ID.
    pub fn get(&self, id: &DocumentId) -> Option<&Document> {
        self.documents.get(id)
    }

    /// Returns a mutable reference to a document by ID.
    pub fn get_mut(&mut self, id: &DocumentId) -> Option<&mut Document> {
        self.documents.get_mut(id)
    }

    /// Closes a document, returning it if it was open.
    pub fn close(&mut self, id: &DocumentId) -> Option<Document> {
        self.documents.remove(id)
    }

    /// Returns the number of open documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Returns `true` if no documents are open.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_open_close() {
        let mut ws = Workspace::new();
        let doc = Document::new("# Test");
        let id = ws.open(doc);
        assert_eq!(ws.len(), 1);
        assert!(ws.get(&id).is_some());
        ws.close(&id);
        assert!(ws.is_empty());
    }
}
