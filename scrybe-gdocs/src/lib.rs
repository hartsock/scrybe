// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe ⇄ Google Docs.
//!
//! Export a Markdown document to a native Google Doc in Drive (Mermaid
//! fences become PNGs carrying their source in iTXt metadata *and* in the
//! image alt text), and import a Google Doc back to Markdown (those PNGs
//! become ```` ```mermaid ```` fences again).
//!
//! Layout:
//! - [`http`] / [`reqwest_http`] / [`testing`] — the transport seam. Every
//!   Google call goes through [`HttpClient`], so tests run offline.
//! - [`config`], [`pkce`], [`loopback`], [`oauth`], [`token_store`] — the
//!   OAuth 2.0 installed-app flow (bring-your-own client, `drive.file` scope).
//! - [`drive`] — the handful of Drive v3 calls Scrybe needs.
//! - [`export`], [`import`], [`sidecar`] — Markdown ⇄ Doc content, pure.
//! - The entry points on [`Gdocs`] tie those together; [`run_blocking`] is
//!   how async callers (MCP server, Tauri) must invoke them.

use std::path::{Path, PathBuf};

pub mod error;
pub mod http;
pub mod reqwest_http;
#[doc(hidden)]
pub mod testing;
pub mod types;

pub mod config;
pub mod drive;
pub mod export;
pub mod import;
pub mod loopback;
pub mod oauth;
pub mod pkce;
pub mod sidecar;
pub mod token_store;

mod api;

pub use api::{
    export_markdown_file, export_markdown_source, import_doc, list_docs, login, logout, status,
};
pub use error::{GdocsError, Result};
pub use http::{HttpClient, Method, Request, Response};
pub use import::parse_doc_ref;
pub use reqwest_http::ReqwestHttp;
pub use types::*;

/// The only Drive scope Scrybe asks for: files it created or opened.
pub const SCOPE_DRIVE_FILE: &str = "https://www.googleapis.com/auth/drive.file";
/// Name of the folder in My Drive that exported docs are kept in.
pub const SCRYBE_FOLDER_NAME: &str = "Scrybe";
/// Suffix of the per-document sidecar (`notes.md` → `notes.gdoc.json`).
pub const SIDECAR_SUFFIX: &str = ".gdoc.json";
/// Prefix of the image alt-text payload (`scrybe-mermaid:v1:<base64url JSON>`).
pub const ALT_PREFIX: &str = "scrybe-mermaid:v1:";

/// Google endpoints, overridable so tests can point at a fake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
    /// Drive v3 base, no trailing slash.
    pub drive: String,
    /// Drive v3 upload base, no trailing slash.
    pub upload: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
            drive: "https://www.googleapis.com/drive/v3".into(),
            upload: "https://www.googleapis.com/upload/drive/v3".into(),
        }
    }
}

/// Hands out a valid bearer token, refreshing when needed.
pub trait TokenProvider: Send + Sync {
    /// A currently valid access token (refreshed if at/near expiry).
    fn access_token(&self) -> Result<String>;
    /// Force a refresh (after a 401) and return the new token.
    fn refresh(&self) -> Result<String>;
}

/// Opens a URL in the user's browser; injectable so tests use a fake.
pub type BrowserOpener = Box<dyn Fn(&str) -> Result<()> + Send + Sync>;

/// Resolves the OAuth client for a config directory; injectable so tests
/// never depend on the process environment (`config::load_client` reads
/// `SCRYBE_GOOGLE_CLIENT_ID`/`_SECRET` first).
pub type ClientSource = Box<dyn Fn(&Path) -> Result<config::ClientConfig> + Send + Sync>;

/// Everything an entry point needs: transport, where config lives, which
/// Google to talk to, how the OAuth client is resolved, and how to open a
/// browser for consent.
pub struct Gdocs<H: HttpClient> {
    pub http: H,
    /// Directory holding `client.json` and `token.json`.
    pub config_dir: PathBuf,
    pub endpoints: Endpoints,
    pub open_browser: BrowserOpener,
    pub load_client: ClientSource,
}

impl<H: HttpClient> Gdocs<H> {
    pub fn new(http: H, config_dir: PathBuf) -> Self {
        Self {
            http,
            config_dir,
            endpoints: Endpoints::default(),
            open_browser: Box::new(|url| {
                open::that(url).map_err(|e| GdocsError::AuthFailed(format!("open browser: {e}")))
            }),
            load_client: Box::new(config::load_client),
        }
    }

    pub fn with_endpoints(mut self, endpoints: Endpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn with_browser(
        mut self,
        open_browser: impl Fn(&str) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        self.open_browser = Box::new(open_browser);
        self
    }

    pub fn with_client_source(
        mut self,
        load_client: impl Fn(&Path) -> Result<config::ClientConfig> + Send + Sync + 'static,
    ) -> Self {
        self.load_client = Box::new(load_client);
        self
    }
}

impl Gdocs<ReqwestHttp> {
    /// The production configuration: real HTTP, platform config dir.
    pub fn real() -> Result<Self> {
        Ok(Self::new(ReqwestHttp::new()?, config::default_dir()?))
    }
}

/// Run `f` on a dedicated OS thread and wait for it.
///
/// The MCP server is `#[tokio::main]` and Tauri commands run on its runtime;
/// a blocking reqwest client created on a tokio worker panics, so every
/// async caller wraps its scrybe-gdocs call in this.
pub fn run_blocking<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| {
        s.spawn(f)
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_endpoints_are_google() {
        let e = Endpoints::default();
        assert!(e.auth.starts_with("https://accounts.google.com/"));
        assert!(e.token.starts_with("https://oauth2.googleapis.com/"));
        assert_eq!(e.drive, "https://www.googleapis.com/drive/v3");
        assert_eq!(e.upload, "https://www.googleapis.com/upload/drive/v3");
    }

    #[test]
    fn run_blocking_returns_value_from_worker_thread() {
        let main_id = std::thread::current().id();
        let (value, worker_id) = run_blocking(|| (42, std::thread::current().id()));
        assert_eq!(value, 42);
        assert_ne!(worker_id, main_id);
    }

    #[test]
    fn builder_overrides_apply() {
        let g = Gdocs::new(testing::FakeHttp::new(), PathBuf::from("/tmp/x"))
            .with_endpoints(Endpoints {
                drive: "http://fake/drive".into(),
                ..Endpoints::default()
            })
            .with_browser(|_| Ok(()))
            .with_client_source(|dir| {
                Err(GdocsError::NotConfigured(format!(
                    "no client under {}",
                    dir.display()
                )))
            });
        assert_eq!(g.endpoints.drive, "http://fake/drive");
        assert!((g.open_browser)("http://anything").is_ok());
        let err = (g.load_client)(&g.config_dir).unwrap_err();
        assert_eq!(err.code(), "not_configured");
        assert!(err.to_string().contains("/tmp/x"), "{err}");
    }
}
