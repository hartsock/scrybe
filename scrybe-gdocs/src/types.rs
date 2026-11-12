// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Public report types returned by the crate's entry points. These are the
//! shapes the tools layer serialises into `data` payloads, so field names are
//! part of the agent contract — rename with care.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    LoggedIn,
    LoggedOut,
    NotConfigured,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthStatus {
    pub status: AuthState,
    /// RFC 3339 expiry of the cached access token, when signed in.
    pub expires_at: Option<String>,
    /// Directory holding `client.json` / `token.json`.
    pub config_dir: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportAction {
    Created,
    Updated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportReport {
    /// The Markdown file that was exported.
    pub path: String,
    pub doc_id: String,
    /// `webViewLink` — opens the doc in a browser.
    pub url: String,
    /// The Drive folder the doc lives in (the Scrybe folder).
    pub folder_id: String,
    pub action: ExportAction,
    /// Number of Mermaid diagrams rendered to PNG.
    pub figures: usize,
    /// Local images left as written because they could not be embedded: a
    /// path outside the document's directory, an absolute path, a missing
    /// file or an unknown extension. Google shows these as broken images.
    #[serde(default)]
    pub images_skipped: Vec<String>,
}

/// Which carrier the Mermaid source was recovered from on import.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Verification {
    /// PNG iTXt payload present and its SHA-256 matched.
    Verified,
    /// PNG iTXt payload present but carried no digest.
    Unverified,
    /// iTXt was gone (Google re-encoded the image); recovered from alt text.
    AltText,
    /// Neither iTXt nor alt text survived; recovered from the sidecar manifest.
    Manifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportedFigure {
    /// 0-based position among the document's images.
    pub index: usize,
    pub verification: Verification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportReport {
    /// The Markdown file that was written.
    pub path: String,
    pub doc_id: String,
    /// Mermaid fences restored.
    pub diagrams: usize,
    /// Non-Mermaid images written as sibling files.
    pub images: usize,
    pub figures: Vec<ImportedFigure>,
    pub image_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    pub modified_time: Option<String>,
}
