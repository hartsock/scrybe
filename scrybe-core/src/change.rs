// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Change tracking — byte-range edits and undo/redo history for documents.

/// A byte range within a document's source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl TextRange {
    /// Creates a new `TextRange`.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the number of bytes covered by this range.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns `true` if the range covers zero bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A single text edit: replace `range` in the source with `new_text`.
///
/// `old_text` records the bytes that were replaced so the change can be
/// inverted (undone).
#[derive(Debug, Clone)]
pub struct DocumentChange {
    /// The byte range that is replaced.
    pub range: TextRange,
    /// The text that replaces `range`.
    pub new_text: String,
    /// The original text at `range` (used for undo).
    pub old_text: String,
}

impl DocumentChange {
    /// Creates a new `DocumentChange`.
    pub fn new(range: TextRange, new_text: impl Into<String>, old_text: impl Into<String>) -> Self {
        Self {
            range,
            new_text: new_text.into(),
            old_text: old_text.into(),
        }
    }

    /// Applies this change to `source`, returning the modified string.
    ///
    /// Panics if `range` refers to a byte offset outside of `source` or
    /// does not sit on a character boundary.
    pub fn apply(&self, source: &str) -> String {
        let mut result =
            String::with_capacity(source.len() - self.range.len() + self.new_text.len());
        result.push_str(&source[..self.range.start]);
        result.push_str(&self.new_text);
        result.push_str(&source[self.range.end..]);
        result
    }

    /// Returns the inverse of this change (suitable for undoing).
    ///
    /// The inverse replaces the region `[start .. start + new_text.len()]`
    /// (i.e. the bytes written by `apply`) back with `old_text`.
    pub fn inverse(&self) -> Self {
        Self {
            range: TextRange::new(self.range.start, self.range.start + self.new_text.len()),
            new_text: self.old_text.clone(),
            old_text: self.new_text.clone(),
        }
    }
}

/// Undo/redo history for a document.
///
/// Changes are pushed as they are applied. [`undo`](Self::undo) returns
/// the inverse of the most-recent change; [`redo`](Self::redo) re-applies
/// a change that was undone.
#[derive(Debug, Default)]
pub struct DocumentHistory {
    past: Vec<DocumentChange>,
    future: Vec<DocumentChange>,
}

impl DocumentHistory {
    /// Creates an empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a new change. Any future (redo) stack is cleared.
    pub fn push(&mut self, change: DocumentChange) {
        self.past.push(change);
        self.future.clear();
    }

    /// Removes the most-recent change from the undo stack and returns a
    /// reference to it, or `None` if the stack is empty.
    ///
    /// The inverse of the returned change is placed on the redo stack.
    pub fn undo(&mut self) -> Option<&DocumentChange> {
        let change = self.past.pop()?;
        self.future.push(change.inverse());
        // Return a reference into the redo stack (the last item is the one
        // that the caller should apply to the document).
        self.future.last()
    }

    /// Re-applies the most-recently-undone change and returns a reference
    /// to it, or `None` if the redo stack is empty.
    pub fn redo(&mut self) -> Option<&DocumentChange> {
        let change = self.future.pop()?;
        // The redo change is the inverse-of-the-inverse, i.e. the original.
        // Push it back onto the undo stack.
        self.past.push(change.inverse());
        self.past.last()
    }

    /// Returns `true` if there is at least one change to undo.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Returns `true` if there is at least one change to redo.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- TextRange -----------------------------------------------------------

    #[test]
    fn test_text_range_len() {
        assert_eq!(TextRange::new(2, 7).len(), 5);
        assert_eq!(TextRange::new(3, 3).len(), 0);
    }

    #[test]
    fn test_text_range_is_empty() {
        assert!(TextRange::new(5, 5).is_empty());
        assert!(!TextRange::new(5, 6).is_empty());
    }

    // -- DocumentChange::apply ----------------------------------------------

    #[test]
    fn test_apply_replaces_range() {
        let source = "Hello, world!";
        let change = DocumentChange::new(TextRange::new(7, 12), "Rust", "world");
        assert_eq!(change.apply(source), "Hello, Rust!");
    }

    #[test]
    fn test_apply_insert_at_start() {
        let source = "world";
        let change = DocumentChange::new(TextRange::new(0, 0), "Hello ", "");
        assert_eq!(change.apply(source), "Hello world");
    }

    #[test]
    fn test_apply_delete_range() {
        let source = "Hello, world!";
        let change = DocumentChange::new(TextRange::new(5, 12), "", ", world");
        assert_eq!(change.apply(source), "Hello!");
    }

    // -- DocumentChange::inverse --------------------------------------------

    #[test]
    fn test_inverse_undoes_change() {
        let source = "Hello, world!";
        let change = DocumentChange::new(TextRange::new(7, 12), "Rust", "world");
        let modified = change.apply(source);
        assert_eq!(modified, "Hello, Rust!");

        let undo = change.inverse();
        let restored = undo.apply(&modified);
        assert_eq!(restored, source);
    }

    #[test]
    fn test_inverse_of_inverse_is_original() {
        let c = DocumentChange::new(TextRange::new(0, 5), "new", "old_t");
        let inv = c.inverse();
        let inv_inv = inv.inverse();
        assert_eq!(inv_inv.range.start, c.range.start);
        assert_eq!(inv_inv.new_text, c.new_text);
        assert_eq!(inv_inv.old_text, c.old_text);
    }

    // -- DocumentHistory -----------------------------------------------------

    #[test]
    fn test_history_can_undo_after_push() {
        let mut h = DocumentHistory::new();
        assert!(!h.can_undo());
        h.push(DocumentChange::new(TextRange::new(0, 0), "x", ""));
        assert!(h.can_undo());
    }

    #[test]
    fn test_history_undo_returns_inverse() {
        let source = "Hello, world!";
        let change = DocumentChange::new(TextRange::new(7, 12), "Rust", "world");
        let modified = change.apply(source);

        let mut h = DocumentHistory::new();
        h.push(change);

        let undo_change = h.undo().expect("should have undo");
        let restored = undo_change.apply(&modified);
        assert_eq!(restored, source);
    }

    #[test]
    fn test_history_undo_enables_redo() {
        let mut h = DocumentHistory::new();
        h.push(DocumentChange::new(TextRange::new(0, 1), "b", "a"));
        assert!(!h.can_redo());
        h.undo();
        assert!(h.can_redo());
        assert!(!h.can_undo());
    }

    #[test]
    fn test_history_redo_re_applies() {
        let source = "a";
        let change = DocumentChange::new(TextRange::new(0, 1), "b", "a");
        let modified = change.apply(source); // "b"

        let mut h = DocumentHistory::new();
        h.push(change);

        let undo = h.undo().expect("undo").clone();
        let after_undo = undo.apply(&modified); // back to "a"
        assert_eq!(after_undo, "a");

        let redo = h.redo().expect("redo").clone();
        let after_redo = redo.apply(&after_undo);
        assert_eq!(after_redo, "b");
    }

    #[test]
    fn test_push_clears_redo_stack() {
        let mut h = DocumentHistory::new();
        h.push(DocumentChange::new(TextRange::new(0, 1), "b", "a"));
        h.undo();
        assert!(h.can_redo());

        // A new push should wipe the redo stack.
        h.push(DocumentChange::new(TextRange::new(0, 1), "c", "a"));
        assert!(!h.can_redo());
    }

    #[test]
    fn test_multi_step_undo_redo() {
        let mut source = String::from("a");
        let mut h = DocumentHistory::new();

        let c1 = DocumentChange::new(TextRange::new(1, 1), "b", "");
        source = c1.apply(&source); // "ab"
        h.push(c1);

        let c2 = DocumentChange::new(TextRange::new(2, 2), "c", "");
        source = c2.apply(&source); // "abc"
        h.push(c2);

        // Undo c2
        let u2 = h.undo().expect("undo c2").clone();
        source = u2.apply(&source); // "ab"
        assert_eq!(source, "ab");

        // Undo c1
        let u1 = h.undo().expect("undo c1").clone();
        source = u1.apply(&source); // "a"
        assert_eq!(source, "a");

        // Redo c1
        let r1 = h.redo().expect("redo c1").clone();
        source = r1.apply(&source); // "ab"
        assert_eq!(source, "ab");

        // Redo c2
        let r2 = h.redo().expect("redo c2").clone();
        source = r2.apply(&source); // "abc"
        assert_eq!(source, "abc");
    }
}
