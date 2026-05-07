// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Plugin trait — the extension point for Python and native plugins.

use crate::document::Document;
use crate::error::Result;

/// A Scrybe plugin can observe and transform documents.
///
/// Implementations live in `scrybe-py` (PyO3 bindings) and in native
/// Rust for first-party extensions. The `scrybe-app` frontend invokes
/// plugins via `scrybe-panels` (P3.3).
pub trait Plugin: Send + Sync {
    /// Human-readable plugin name.
    fn name(&self) -> &str;

    /// Called when a document is opened or modified.
    ///
    /// Returns an optionally transformed document. Return `None` to
    /// pass through unchanged.
    fn on_change(&self, doc: &Document) -> Result<Option<Document>>;
}
