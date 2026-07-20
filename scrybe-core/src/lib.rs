// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe core — foundational types for the Scrybe Markdown editor.
//!
//! Provides the building blocks all other scrybe crates depend on:
//!
//! - [`Document`] — the central editing unit: parsed AST + metadata
//! - [`ContentDigest`] — bare BLAKE3 digest of raw content bytes, lowercase hex
//! - [`ContentAddressable`] — trait for content-addressed storage
//! - [`Plugin`] — trait for Python and native plugins
//! - [`Workspace`] — collection of open documents + shared state
//! - [`Ast`] / [`Node`] — Markdown AST types
//! - [`DocumentChange`] / [`DocumentHistory`] / [`TextRange`] — change tracking

pub mod ast;
pub mod change;
pub mod content;
pub mod document;
pub mod error;
pub mod plugin;
pub mod workspace;

pub use ast::{Ast, Node};
pub use change::{DocumentChange, DocumentHistory, TextRange};
#[allow(deprecated)] // compat shim: keep the old name importable downstream
pub use content::ContentId;
pub use content::{ContentAddressable, ContentDigest};
pub use document::Document;
pub use error::ScrybeError;
pub use plugin::Plugin;
pub use workspace::Workspace;
