// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The one error type shared by every scrybe-gdocs module. Each variant maps
//! to a stable business-error `code` that the tools layer reports to agents.

use thiserror::Error;

/// Everything that can go wrong talking to Google or preparing a document.
#[derive(Debug, Error)]
pub enum GdocsError {
    /// No OAuth client credentials are configured (see `docs/google-docs.md`).
    #[error("{0}")]
    NotConfigured(String),
    /// No usable token: never signed in, or the refresh token was rejected.
    #[error("{0}")]
    NotLoggedIn(String),
    /// The browser consent step failed (denied, state mismatch, timed out).
    #[error("{0}")]
    AuthFailed(String),
    /// Drive returned 404 — the doc is gone, or not visible under `drive.file`.
    #[error("{0}")]
    DocNotFound(String),
    /// Google returned a non-success status other than the mapped 401/404.
    #[error("Google API error {status}: {message}")]
    Api { status: u16, message: String },
    /// Transport-level failure (DNS, TLS, connection reset).
    #[error("{0}")]
    Http(String),
    /// Mermaid render or PNG embed failed.
    #[error("{0}")]
    Render(String),
    /// Malformed input or response (bad doc reference, unparsable JSON).
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl GdocsError {
    /// Stable machine-readable code for the tools layer.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured(_) => "not_configured",
            Self::NotLoggedIn(_) => "not_logged_in",
            Self::AuthFailed(_) => "auth_failed",
            Self::DocNotFound(_) => "doc_not_found",
            Self::Api { .. } => "api_error",
            Self::Http(_) => "http_error",
            Self::Render(_) => "render_failed",
            Self::Invalid(_) => "invalid_argument",
            Self::Io(_) => "io_error",
        }
    }
}

impl From<serde_json::Error> for GdocsError {
    fn from(e: serde_json::Error) -> Self {
        Self::Invalid(format!("malformed JSON: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, GdocsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_snake_case() {
        let cases: Vec<(GdocsError, &str)> = vec![
            (GdocsError::NotConfigured("x".into()), "not_configured"),
            (GdocsError::NotLoggedIn("x".into()), "not_logged_in"),
            (GdocsError::AuthFailed("x".into()), "auth_failed"),
            (GdocsError::DocNotFound("x".into()), "doc_not_found"),
            (
                GdocsError::Api {
                    status: 500,
                    message: "x".into(),
                },
                "api_error",
            ),
            (GdocsError::Http("x".into()), "http_error"),
            (GdocsError::Render("x".into()), "render_failed"),
            (GdocsError::Invalid("x".into()), "invalid_argument"),
        ];
        for (err, code) in cases {
            assert_eq!(err.code(), code, "{err}");
        }
    }

    #[test]
    fn api_error_display_includes_status() {
        let e = GdocsError::Api {
            status: 403,
            message: "forbidden".into(),
        };
        assert_eq!(e.to_string(), "Google API error 403: forbidden");
    }
}
