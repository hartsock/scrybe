// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Change tracking — byte-range edits and undo/redo history for documents.
//!
//! The checked entry points are [`TextRange::try_new`] and
//! [`DocumentChange::try_apply`]. `try_apply` validates the edit against the
//! actual document (bounds, UTF-8 character boundaries, and — when supplied —
//! the expected `old_text`), then returns an [`AppliedChange`] whose
//! [`inverse`](AppliedChange::inverse) is derived from the document itself,
//! so undo evidence is never caller-authored fiction.
//!
//! The panicking [`TextRange::new`] / [`DocumentChange::apply`] entry points
//! are kept for compatibility but deprecated; they delegate to the checked
//! implementations so there is exactly one validation path.

/// Error constructing a [`TextRange`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RangeError {
    /// The range's start offset is greater than its end offset.
    #[error("invalid text range: start {start} > end {end}")]
    StartAfterEnd {
        /// The offending start offset.
        start: usize,
        /// The offending end offset.
        end: usize,
    },
}

/// Error applying a [`DocumentChange`] to a source string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The change's range is not a valid range (start > end).
    #[error(transparent)]
    Range(#[from] RangeError),
    /// The change's range extends past the end of the source.
    #[error("range end {end} is out of bounds for source of length {len}")]
    OutOfBounds {
        /// The offending (exclusive) end offset.
        end: usize,
        /// The source length in bytes.
        len: usize,
    },
    /// A range endpoint does not sit on a UTF-8 character boundary.
    #[error("byte offset {offset} is not on a UTF-8 character boundary")]
    NotCharBoundary {
        /// The offending byte offset.
        offset: usize,
    },
    /// The change's `old_text` does not match what the source actually
    /// contains at the range — the edit was built against a stale version
    /// of the document. Precondition failure, not a panic.
    #[error("stale edit: expected {expected:?} at range, found {found:?}")]
    StaleEdit {
        /// What the change claimed the range contained.
        expected: String,
        /// What the source actually contains at the range.
        found: String,
    },
}

/// A byte range within a document's source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl TextRange {
    /// Creates a new `TextRange`, validating that `start <= end`.
    ///
    /// Bounds and UTF-8 boundary checks require the source string, so they
    /// happen in [`DocumentChange::try_apply`], not here.
    pub fn try_new(start: usize, end: usize) -> Result<Self, RangeError> {
        if start > end {
            return Err(RangeError::StartAfterEnd { start, end });
        }
        Ok(Self { start, end })
    }

    /// Creates a new `TextRange`.
    #[deprecated(note = "panics on invalid input (start > end); use try_new")]
    pub fn new(start: usize, end: usize) -> Self {
        Self::try_new(start, end).expect("TextRange::new called with start > end; use try_new")
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
/// `old_text` optionally records what the caller believes the range
/// currently contains. When present, [`try_apply`](Self::try_apply)
/// verifies it against the source and rejects the edit as
/// [`EditError::StaleEdit`] on mismatch. Callers should not author undo
/// evidence themselves — apply the change with `try_apply` and use the
/// derived [`AppliedChange::inverse`] instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentChange {
    /// The byte range that is replaced.
    pub range: TextRange,
    /// The text that replaces `range`.
    pub new_text: String,
    /// Optional precondition: the text the caller expects at `range`.
    /// Verified by [`try_apply`](Self::try_apply) when present.
    pub old_text: Option<String>,
}

impl DocumentChange {
    /// Creates a change that replaces `range` with `new_text`, with no
    /// `old_text` precondition. The true removed text is derived from the
    /// source by [`try_apply`](Self::try_apply).
    pub fn replace(range: TextRange, new_text: impl Into<String>) -> Self {
        Self {
            range,
            new_text: new_text.into(),
            old_text: None,
        }
    }

    /// Creates a change that replaces `range` with `new_text`, asserting
    /// that the range currently contains `old_text`.
    /// [`try_apply`](Self::try_apply) rejects the edit as
    /// [`EditError::StaleEdit`] if the assertion does not hold.
    pub fn replace_expecting(
        range: TextRange,
        new_text: impl Into<String>,
        old_text: impl Into<String>,
    ) -> Self {
        Self {
            range,
            new_text: new_text.into(),
            old_text: Some(old_text.into()),
        }
    }

    /// Creates a new `DocumentChange` from caller-authored parts.
    #[deprecated(note = "trusts caller-authored old_text (unverified undo evidence); \
                use replace/replace_expecting + try_apply")]
    pub fn new(range: TextRange, new_text: impl Into<String>, old_text: impl Into<String>) -> Self {
        Self::replace_expecting(range, new_text, old_text)
    }

    /// Applies this change to `source`, validating it first.
    ///
    /// Checks, in order:
    /// 1. the range is well-formed (`start <= end`) — [`EditError::Range`];
    /// 2. the range lies within `source` — [`EditError::OutOfBounds`];
    /// 3. both endpoints sit on UTF-8 character boundaries —
    ///    [`EditError::NotCharBoundary`];
    /// 4. if `old_text` is present, it matches the source at the range —
    ///    [`EditError::StaleEdit`].
    ///
    /// On success, returns an [`AppliedChange`] carrying the modified
    /// document and an inverse derived from the *actual* removed text, so
    /// the inverse is truthful even if the caller supplied no `old_text`.
    ///
    /// ```
    /// use scrybe_core::change::{DocumentChange, TextRange};
    ///
    /// let range = TextRange::try_new(7, 12).unwrap();
    /// let change = DocumentChange::replace(range, "Rust");
    /// let applied = change.try_apply("Hello, world!").unwrap();
    /// assert_eq!(applied.new_text, "Hello, Rust!");
    /// // The inverse is derived from the document, not authored by us.
    /// let undone = applied.inverse.try_apply(&applied.new_text).unwrap();
    /// assert_eq!(undone.new_text, "Hello, world!");
    /// ```
    pub fn try_apply(&self, source: &str) -> Result<AppliedChange, EditError> {
        let TextRange { start, end } = self.range;
        // Re-validate even though try_new checks this: the fields are public,
        // so a range may not have gone through try_new.
        TextRange::try_new(start, end)?;
        if end > source.len() {
            return Err(EditError::OutOfBounds {
                end,
                len: source.len(),
            });
        }
        for offset in [start, end] {
            if !source.is_char_boundary(offset) {
                return Err(EditError::NotCharBoundary { offset });
            }
        }

        let removed = &source[start..end];
        if let Some(expected) = &self.old_text {
            if expected != removed {
                return Err(EditError::StaleEdit {
                    expected: expected.clone(),
                    found: removed.to_string(),
                });
            }
        }

        let mut new_text =
            String::with_capacity(source.len() - self.range.len() + self.new_text.len());
        new_text.push_str(&source[..start]);
        new_text.push_str(&self.new_text);
        new_text.push_str(&source[end..]);

        let inverse = Self {
            range: TextRange {
                start,
                end: start + self.new_text.len(),
            },
            new_text: removed.to_string(),
            old_text: Some(self.new_text.clone()),
        };

        Ok(AppliedChange { new_text, inverse })
    }

    /// Applies this change to `source`, returning the modified string.
    ///
    /// Panics if the change is invalid for `source`: range out of bounds or
    /// off a character boundary, or (unlike the historical behavior) a
    /// present `old_text` that does not match the source.
    #[deprecated(note = "panics on invalid input; use try_apply")]
    pub fn apply(&self, source: &str) -> String {
        self.try_apply(source)
            .expect("DocumentChange::apply called with an invalid change; use try_apply")
            .new_text
    }

    /// Builds the inverse from this change's own (possibly caller-authored,
    /// possibly absent) `old_text`, without consulting the document. A
    /// missing `old_text` is treated as empty.
    ///
    /// This is exact for changes that came out of
    /// [`try_apply`](Self::try_apply) (their `old_text` is derived and
    /// verified); for anything else it is only as truthful as the caller.
    fn inverse_unverified(&self) -> Self {
        Self {
            range: TextRange {
                start: self.range.start,
                end: self.range.start + self.new_text.len(),
            },
            new_text: self.old_text.clone().unwrap_or_default(),
            old_text: Some(self.new_text.clone()),
        }
    }

    /// Returns the inverse of this change (suitable for undoing).
    ///
    /// The inverse replaces the region `[start .. start + new_text.len()]`
    /// (i.e. the bytes written by `apply`) back with `old_text`.
    #[deprecated(note = "built from caller-authored old_text, which may not match the \
                document; use the derived AppliedChange::inverse from try_apply")]
    pub fn inverse(&self) -> Self {
        self.inverse_unverified()
    }
}

/// The result of successfully applying a [`DocumentChange`] to a source
/// string via [`DocumentChange::try_apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChange {
    /// The full document text after the change.
    pub new_text: String,
    /// A change that, applied to [`new_text`](Self::new_text), restores the
    /// original document exactly. Its `old_text` and replacement text are
    /// derived from the source by the library, never caller-authored.
    pub inverse: DocumentChange,
}

/// Undo/redo history for a document.
///
/// Changes are pushed as they are applied — prefer
/// [`push_applied`](Self::push_applied) with the result of
/// [`DocumentChange::try_apply`], so the recorded undo evidence is derived
/// from the document rather than caller-authored. [`undo`](Self::undo)
/// returns the inverse of the most-recent change; [`redo`](Self::redo)
/// re-applies a change that was undone.
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

    /// Records a verified change. Any future (redo) stack is cleared.
    ///
    /// The forward change is reconstructed from the applied change's
    /// derived inverse, so both undo and redo evidence come from the
    /// document itself.
    pub fn push_applied(&mut self, applied: &AppliedChange) {
        // The inverse of the derived inverse is the forward change with its
        // old_text filled in from the actual document — exact, because both
        // sides of the derived inverse are fully specified.
        self.push(applied.inverse.inverse_unverified());
    }

    /// Records a new change. Any future (redo) stack is cleared.
    ///
    /// The change's `old_text` is trusted as-is; prefer
    /// [`push_applied`](Self::push_applied), which records evidence derived
    /// from the document.
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
        self.future.push(change.inverse_unverified());
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
        self.past.push(change.inverse_unverified());
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

    fn range(start: usize, end: usize) -> TextRange {
        TextRange::try_new(start, end).expect("test range must be well-formed")
    }

    // -- TextRange -----------------------------------------------------------

    #[test]
    fn test_try_new_accepts_ordered_and_empty_ranges() {
        assert_eq!(TextRange::try_new(2, 7), Ok(TextRange { start: 2, end: 7 }));
        assert_eq!(TextRange::try_new(3, 3), Ok(TextRange { start: 3, end: 3 }));
        assert_eq!(TextRange::try_new(0, 0), Ok(TextRange { start: 0, end: 0 }));
    }

    #[test]
    fn test_try_new_rejects_reversed_range() {
        assert_eq!(
            TextRange::try_new(7, 2),
            Err(RangeError::StartAfterEnd { start: 7, end: 2 })
        );
    }

    #[test]
    fn test_range_error_display() {
        let err = RangeError::StartAfterEnd { start: 7, end: 2 };
        assert_eq!(err.to_string(), "invalid text range: start 7 > end 2");
    }

    #[test]
    fn test_text_range_len() {
        assert_eq!(range(2, 7).len(), 5);
        assert_eq!(range(3, 3).len(), 0);
    }

    #[test]
    fn test_text_range_is_empty() {
        assert!(range(5, 5).is_empty());
        assert!(!range(5, 6).is_empty());
    }

    // -- DocumentChange::try_apply — success paths ---------------------------

    #[test]
    fn test_try_apply_replaces_range() {
        let change = DocumentChange::replace_expecting(range(7, 12), "Rust", "world");
        let applied = change.try_apply("Hello, world!").expect("valid edit");
        assert_eq!(applied.new_text, "Hello, Rust!");
    }

    #[test]
    fn test_try_apply_empty_insertion_at_start() {
        let change = DocumentChange::replace(range(0, 0), "Hello ");
        let applied = change.try_apply("world").expect("valid edit");
        assert_eq!(applied.new_text, "Hello world");
    }

    #[test]
    fn test_try_apply_empty_insertion_at_end() {
        let source = "Hello";
        let change = DocumentChange::replace(range(source.len(), source.len()), "!");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "Hello!");
    }

    #[test]
    fn test_try_apply_empty_insertion_into_empty_source() {
        let change = DocumentChange::replace(range(0, 0), "seed");
        let applied = change.try_apply("").expect("valid edit");
        assert_eq!(applied.new_text, "seed");
    }

    #[test]
    fn test_try_apply_delete_whole_source() {
        let source = "Hello, world!";
        let change = DocumentChange::replace(range(0, source.len()), "");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "");
        // The derived inverse restores everything.
        assert_eq!(applied.inverse.new_text, source);
    }

    #[test]
    fn test_try_apply_no_op_change() {
        let change = DocumentChange::replace(range(3, 3), "");
        let applied = change.try_apply("abcdef").expect("valid edit");
        assert_eq!(applied.new_text, "abcdef");
    }

    #[test]
    fn test_try_apply_multibyte_content_replacement() {
        // "héllo 🦀" — é is 2 bytes (1..3), 🦀 is 4 bytes (7..11).
        let source = "héllo 🦀";
        let change = DocumentChange::replace(range(7, 11), "🐍");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "héllo 🐍");
        assert_eq!(applied.inverse.new_text, "🦀");
    }

    // -- DocumentChange::try_apply — rejected edits --------------------------

    #[test]
    fn test_try_apply_rejects_reversed_range() {
        // Public fields let a caller bypass try_new; try_apply re-validates.
        let change = DocumentChange {
            range: TextRange { start: 5, end: 2 },
            new_text: "x".to_string(),
            old_text: None,
        };
        assert_eq!(
            change.try_apply("Hello, world!"),
            Err(EditError::Range(RangeError::StartAfterEnd {
                start: 5,
                end: 2
            }))
        );
    }

    #[test]
    fn test_try_apply_rejects_out_of_bounds_end() {
        let change = DocumentChange::replace(range(0, 99), "x");
        assert_eq!(
            change.try_apply("short"),
            Err(EditError::OutOfBounds { end: 99, len: 5 })
        );
    }

    #[test]
    fn test_try_apply_rejects_out_of_bounds_empty_insertion() {
        let change = DocumentChange::replace(range(6, 6), "x");
        assert_eq!(
            change.try_apply("short"),
            Err(EditError::OutOfBounds { end: 6, len: 5 })
        );
    }

    #[test]
    fn test_try_apply_rejects_split_two_byte_char() {
        // é occupies bytes 1..3 of "café" → offset 4 splits it.
        let source = "café";
        assert_eq!(source.len(), 5);
        let change = DocumentChange::replace(range(0, 4), "");
        assert_eq!(
            change.try_apply(source),
            Err(EditError::NotCharBoundary { offset: 4 })
        );
    }

    #[test]
    fn test_try_apply_rejects_split_emoji() {
        // 🦀 occupies bytes 0..4 → offsets 1..=3 are interior.
        let change = DocumentChange::replace(range(0, 2), "");
        assert_eq!(
            change.try_apply("🦀"),
            Err(EditError::NotCharBoundary { offset: 2 })
        );
    }

    #[test]
    fn test_try_apply_rejects_split_start_offset() {
        let change = DocumentChange::replace(range(1, 4), "");
        assert_eq!(
            change.try_apply("🦀!"),
            Err(EditError::NotCharBoundary { offset: 1 })
        );
    }

    #[test]
    fn test_try_apply_rejects_split_combining_mark() {
        // "e\u{0301}" — 'e' at byte 0, U+0301 occupies bytes 1..3.
        // Offset 2 lands inside the combining mark's UTF-8 encoding.
        let source = "e\u{0301}";
        let change = DocumentChange::replace(range(0, 2), "");
        assert_eq!(
            change.try_apply(source),
            Err(EditError::NotCharBoundary { offset: 2 })
        );
    }

    #[test]
    fn test_try_apply_allows_char_boundary_between_base_and_combining_mark() {
        // Byte 1 is a *character* boundary (between 'e' and U+0301) even
        // though it is inside a grapheme cluster. The API validates char
        // boundaries, not grapheme clusters — documented behavior.
        let source = "e\u{0301}";
        let change = DocumentChange::replace(range(1, 1), "x");
        let applied = change.try_apply(source).expect("char boundary is valid");
        assert_eq!(applied.new_text, "ex\u{0301}");
    }

    #[test]
    fn test_try_apply_rejects_stale_old_text() {
        let change = DocumentChange::replace_expecting(range(7, 12), "Rust", "world");
        // The document moved on: the range now holds "there".
        assert_eq!(
            change.try_apply("Hello, there!"),
            Err(EditError::StaleEdit {
                expected: "world".to_string(),
                found: "there".to_string(),
            })
        );
    }

    #[test]
    fn test_try_apply_without_old_text_skips_stale_check() {
        let change = DocumentChange::replace(range(7, 12), "Rust");
        let applied = change.try_apply("Hello, there!").expect("no precondition");
        assert_eq!(applied.new_text, "Hello, Rust!");
        // ...but the derived inverse still records the truth.
        assert_eq!(applied.inverse.new_text, "there");
    }

    #[test]
    fn test_edit_error_display() {
        let stale = EditError::StaleEdit {
            expected: "a".to_string(),
            found: "b".to_string(),
        };
        assert_eq!(
            stale.to_string(),
            "stale edit: expected \"a\" at range, found \"b\""
        );
        let oob = EditError::OutOfBounds { end: 9, len: 3 };
        assert_eq!(
            oob.to_string(),
            "range end 9 is out of bounds for source of length 3"
        );
        let boundary = EditError::NotCharBoundary { offset: 2 };
        assert_eq!(
            boundary.to_string(),
            "byte offset 2 is not on a UTF-8 character boundary"
        );
        // Range errors pass through transparently.
        let range_err = EditError::Range(RangeError::StartAfterEnd { start: 3, end: 1 });
        assert_eq!(range_err.to_string(), "invalid text range: start 3 > end 1");
    }

    // -- Derived inverses round-trip -----------------------------------------

    #[test]
    fn test_apply_then_inverse_restores_original() {
        let source = "Hello, world!";
        let change = DocumentChange::replace(range(7, 12), "Rust");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "Hello, Rust!");

        let restored = applied
            .inverse
            .try_apply(&applied.new_text)
            .expect("derived inverse is valid");
        assert_eq!(restored.new_text, source);
    }

    #[test]
    fn test_inverse_of_insertion_deletes() {
        let source = "ab";
        let change = DocumentChange::replace(range(1, 1), "XYZ");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "aXYZb");

        let restored = applied
            .inverse
            .try_apply(&applied.new_text)
            .expect("derived inverse is valid");
        assert_eq!(restored.new_text, source);
    }

    #[test]
    fn test_inverse_of_deletion_reinserts_exact_bytes() {
        let source = "héllo 🦀 wörld";
        let change = DocumentChange::replace(range(6, 11), "");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "héllo wörld");

        let restored = applied
            .inverse
            .try_apply(&applied.new_text)
            .expect("derived inverse is valid");
        assert_eq!(restored.new_text, source);
        // Byte-exact restoration.
        assert_eq!(restored.new_text.as_bytes(), source.as_bytes());
    }

    #[test]
    fn test_inverse_round_trip_multibyte_replacement() {
        let source = "🦀🐍🦀";
        let change = DocumentChange::replace(range(4, 8), "e\u{0301}");
        let applied = change.try_apply(source).expect("valid edit");
        assert_eq!(applied.new_text, "🦀e\u{0301}🦀");

        let restored = applied
            .inverse
            .try_apply(&applied.new_text)
            .expect("derived inverse is valid");
        assert_eq!(restored.new_text.as_bytes(), source.as_bytes());
    }

    #[test]
    fn test_derived_inverse_carries_verified_evidence() {
        let change = DocumentChange::replace(range(0, 5), "Howdy");
        let applied = change.try_apply("Hello, world!").expect("valid edit");
        // The inverse's precondition is what the forward change wrote...
        assert_eq!(applied.inverse.old_text.as_deref(), Some("Howdy"));
        // ...and its replacement is what the document really contained.
        assert_eq!(applied.inverse.new_text, "Hello");
        assert_eq!(applied.inverse.range, TextRange { start: 0, end: 5 });
    }

    // -- DocumentHistory -----------------------------------------------------

    #[test]
    fn test_history_can_undo_after_push_applied() {
        let mut h = DocumentHistory::new();
        assert!(!h.can_undo());
        let applied = DocumentChange::replace(range(0, 0), "x")
            .try_apply("")
            .expect("valid edit");
        h.push_applied(&applied);
        assert!(h.can_undo());
    }

    #[test]
    fn test_history_undo_returns_verified_inverse() {
        let source = "Hello, world!";
        let applied = DocumentChange::replace(range(7, 12), "Rust")
            .try_apply(source)
            .expect("valid edit");

        let mut h = DocumentHistory::new();
        h.push_applied(&applied);

        let undo_change = h.undo().expect("should have undo");
        // The undo carries evidence derived from the document.
        assert_eq!(undo_change.new_text, "world");
        assert_eq!(undo_change.old_text.as_deref(), Some("Rust"));
        let restored = undo_change
            .try_apply(&applied.new_text)
            .expect("undo is valid against the edited document");
        assert_eq!(restored.new_text, source);
    }

    #[test]
    fn test_history_undo_enables_redo() {
        let applied = DocumentChange::replace(range(0, 1), "b")
            .try_apply("a")
            .expect("valid edit");
        let mut h = DocumentHistory::new();
        h.push_applied(&applied);
        assert!(!h.can_redo());
        h.undo();
        assert!(h.can_redo());
        assert!(!h.can_undo());
    }

    #[test]
    fn test_history_redo_re_applies() {
        let source = "a";
        let applied = DocumentChange::replace(range(0, 1), "b")
            .try_apply(source)
            .expect("valid edit"); // "b"

        let mut h = DocumentHistory::new();
        h.push_applied(&applied);

        let undo = h.undo().expect("undo").clone();
        let after_undo = undo.try_apply(&applied.new_text).expect("valid undo");
        assert_eq!(after_undo.new_text, "a");

        let redo = h.redo().expect("redo").clone();
        let after_redo = redo.try_apply(&after_undo.new_text).expect("valid redo");
        assert_eq!(after_redo.new_text, "b");
    }

    #[test]
    fn test_push_clears_redo_stack() {
        let applied = DocumentChange::replace(range(0, 1), "b")
            .try_apply("a")
            .expect("valid edit");
        let mut h = DocumentHistory::new();
        h.push_applied(&applied);
        h.undo();
        assert!(h.can_redo());

        // A new push should wipe the redo stack.
        let applied2 = DocumentChange::replace(range(0, 1), "c")
            .try_apply("a")
            .expect("valid edit");
        h.push_applied(&applied2);
        assert!(!h.can_redo());
    }

    #[test]
    fn test_multi_step_undo_redo() {
        let mut source = String::from("a");
        let mut h = DocumentHistory::new();

        let a1 = DocumentChange::replace(range(1, 1), "b")
            .try_apply(&source)
            .expect("valid edit");
        h.push_applied(&a1);
        source = a1.new_text; // "ab"

        let a2 = DocumentChange::replace(range(2, 2), "c")
            .try_apply(&source)
            .expect("valid edit");
        h.push_applied(&a2);
        source = a2.new_text; // "abc"

        // Undo c2
        let u2 = h.undo().expect("undo c2").clone();
        source = u2.try_apply(&source).expect("valid undo").new_text;
        assert_eq!(source, "ab");

        // Undo c1
        let u1 = h.undo().expect("undo c1").clone();
        source = u1.try_apply(&source).expect("valid undo").new_text;
        assert_eq!(source, "a");

        // Redo c1
        let r1 = h.redo().expect("redo c1").clone();
        source = r1.try_apply(&source).expect("valid redo").new_text;
        assert_eq!(source, "ab");

        // Redo c2
        let r2 = h.redo().expect("redo c2").clone();
        source = r2.try_apply(&source).expect("valid redo").new_text;
        assert_eq!(source, "abc");
    }

    // -- Deprecated compatibility shims --------------------------------------
    //
    // These tests exercise the deprecated panicking APIs on purpose, so the
    // allow(deprecated) is scoped to this module only.
    mod deprecated_compat {
        #![allow(deprecated)]

        use super::super::*;

        #[test]
        fn test_deprecated_apply_still_works_on_valid_input() {
            let change = DocumentChange::new(TextRange::new(7, 12), "Rust", "world");
            assert_eq!(change.apply("Hello, world!"), "Hello, Rust!");
        }

        #[test]
        #[should_panic(expected = "invalid change")]
        fn test_deprecated_apply_still_panics_on_out_of_bounds() {
            let change = DocumentChange::new(TextRange::new(0, 99), "x", "y");
            change.apply("short");
        }

        #[test]
        #[should_panic(expected = "start > end")]
        fn test_deprecated_text_range_new_panics_on_reversed_range() {
            TextRange::new(7, 2);
        }

        #[test]
        fn test_deprecated_inverse_round_trips_when_old_text_accurate() {
            let source = "Hello, world!";
            let change = DocumentChange::new(TextRange::new(7, 12), "Rust", "world");
            let modified = change.apply(source);
            let restored = change.inverse().apply(&modified);
            assert_eq!(restored, source);
        }

        #[test]
        fn test_deprecated_new_populates_old_text_precondition() {
            let change = DocumentChange::new(TextRange::new(0, 5), "new", "old_t");
            assert_eq!(change.old_text.as_deref(), Some("old_t"));
        }
    }
}
