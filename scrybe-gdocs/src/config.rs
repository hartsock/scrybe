// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Where Scrybe keeps its Google OAuth client credentials and cached token,
//! and how the bring-your-own client id/secret are resolved.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{GdocsError, Result};

/// Environment variable overriding the config directory (tests use a tempdir).
pub const DIR_ENV: &str = "SCRYBE_GOOGLE_DIR";
/// Environment variable carrying the OAuth client id (wins over `client.json`).
pub const CLIENT_ID_ENV: &str = "SCRYBE_GOOGLE_CLIENT_ID";
/// Environment variable carrying the OAuth client secret (wins over `client.json`).
pub const CLIENT_SECRET_ENV: &str = "SCRYBE_GOOGLE_CLIENT_SECRET";

const CLIENT_FILE: &str = "client.json";
const TOKEN_FILE: &str = "token.json";
const DOCS_HINT: &str = "see docs/google-docs.md";

/// `$SCRYBE_GOOGLE_DIR`, else `<platform config dir>/scrybe/google`
/// (`~/.config/scrybe/google` on Linux, `~/Library/Application Support/scrybe/google`
/// on macOS) — the same root scrybe-mcp-client uses for `agents.toml`.
pub fn default_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(DIR_ENV) {
        return Ok(PathBuf::from(dir));
    }
    dirs::config_dir()
        .map(|d| d.join("scrybe").join("google"))
        .ok_or_else(|| {
            GdocsError::NotConfigured(format!(
                "no platform config directory; set {DIR_ENV} to a directory for Google credentials"
            ))
        })
}

/// The user's own Google "Desktop app" OAuth client.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

/// `<dir>/client.json`.
pub fn client_path(dir: &Path) -> PathBuf {
    dir.join(CLIENT_FILE)
}

/// `<dir>/token.json`.
pub fn token_path(dir: &Path) -> PathBuf {
    dir.join(TOKEN_FILE)
}

/// Resolve the OAuth client: both env vars if set, else `<dir>/client.json`.
///
/// The file may be Google's downloaded shape (`{"installed": {...}}` or
/// `{"web": {...}}`) or a flat `{"client_id", "client_secret"}` object.
pub fn load_client(dir: &Path) -> Result<ClientConfig> {
    resolve_client(
        std::env::var(CLIENT_ID_ENV).ok(),
        std::env::var(CLIENT_SECRET_ENV).ok(),
        dir,
    )
}

/// Resolve the OAuth client from `<dir>/client.json` alone, ignoring the
/// environment. Tests inject this as the client source so they hold whether
/// or not the developer has the env vars set.
pub fn load_client_file(dir: &Path) -> Result<ClientConfig> {
    resolve_client(None, None, dir)
}

fn resolve_client(
    env_id: Option<String>,
    env_secret: Option<String>,
    dir: &Path,
) -> Result<ClientConfig> {
    if let (Some(client_id), Some(client_secret)) = (non_blank(env_id), non_blank(env_secret)) {
        return Ok(ClientConfig {
            client_id,
            client_secret,
        });
    }
    let path = client_path(dir);
    let shown = path.display();
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(GdocsError::NotConfigured(format!(
                "no Google OAuth client configured: {CLIENT_ID_ENV}/{CLIENT_SECRET_ENV} are not set \
                 and {shown} does not exist; download a Desktop-app OAuth client from Google Cloud \
                 and save it there ({DOCS_HINT})"
            )));
        }
        Err(e) => {
            return Err(GdocsError::NotConfigured(format!(
                "cannot read Google OAuth client file {shown}: {e} ({DOCS_HINT})"
            )));
        }
    };
    parse_client_json(&raw).map_err(|reason| {
        GdocsError::NotConfigured(format!(
            "Google OAuth client file {shown} is not usable: {reason}; expected the JSON \
             downloaded from Google Cloud ({{\"installed\": {{\"client_id\": ..., \
             \"client_secret\": ...}}}}) ({DOCS_HINT})"
        ))
    })
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn parse_client_json(raw: &str) -> std::result::Result<ClientConfig, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("malformed JSON: {e}"))?;
    let section = ["installed", "web"]
        .into_iter()
        .find_map(|key| value.get(key))
        .unwrap_or(&value);
    let field = |name: &str| {
        section
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("missing \"{name}\""))
    };
    Ok(ClientConfig {
        client_id: field("client_id")?,
        client_secret: field("client_secret")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn paths_live_under_dir() {
        let dir = Path::new("/cfg/google");
        assert_eq!(client_path(dir), PathBuf::from("/cfg/google/client.json"));
        assert_eq!(token_path(dir), PathBuf::from("/cfg/google/token.json"));
    }

    #[test]
    fn parses_googles_installed_shape() {
        let cfg = parse_client_json(
            r#"{"installed":{"client_id":"id.apps.googleusercontent.com","client_secret":"GOCSPX-x","redirect_uris":["http://localhost"]}}"#,
        )
        .unwrap();
        assert_eq!(cfg.client_id, "id.apps.googleusercontent.com");
        assert_eq!(cfg.client_secret, "GOCSPX-x");
    }

    #[test]
    fn parses_web_and_flat_shapes() {
        let web = parse_client_json(r#"{"web":{"client_id":"w","client_secret":"ws"}}"#).unwrap();
        assert_eq!(
            (web.client_id.as_str(), web.client_secret.as_str()),
            ("w", "ws")
        );
        let flat = parse_client_json(r#"{"client_id":"f","client_secret":"fs"}"#).unwrap();
        assert_eq!(
            (flat.client_id.as_str(), flat.client_secret.as_str()),
            ("f", "fs")
        );
    }

    #[test]
    fn rejects_missing_fields_and_bad_json() {
        let err = parse_client_json(r#"{"installed":{"client_id":"only"}}"#).unwrap_err();
        assert!(err.contains("client_secret"), "{err}");
        let err = parse_client_json(r#"{"client_id":"","client_secret":"s"}"#).unwrap_err();
        assert!(err.contains("client_id"), "{err}");
        let err = parse_client_json("not json").unwrap_err();
        assert!(err.contains("malformed JSON"), "{err}");
    }

    #[test]
    fn env_wins_when_both_set() {
        let dir = tmp();
        std::fs::write(
            client_path(dir.path()),
            r#"{"client_id":"file","client_secret":"fs"}"#,
        )
        .unwrap();
        let cfg = resolve_client(Some("env".into()), Some("es".into()), dir.path()).unwrap();
        assert_eq!(cfg.client_id, "env");
        assert_eq!(cfg.client_secret, "es");
    }

    #[test]
    fn half_set_or_blank_env_falls_back_to_file() {
        let dir = tmp();
        std::fs::write(
            client_path(dir.path()),
            r#"{"client_id":"file","client_secret":"fs"}"#,
        )
        .unwrap();
        let cfg = resolve_client(Some("env".into()), None, dir.path()).unwrap();
        assert_eq!(cfg.client_id, "file");
        let cfg = resolve_client(Some("  ".into()), Some("es".into()), dir.path()).unwrap();
        assert_eq!(cfg.client_id, "file");
    }

    #[test]
    fn load_client_file_reads_only_the_file() {
        let dir = tmp();
        assert_eq!(
            load_client_file(dir.path()).unwrap_err().code(),
            "not_configured"
        );
        std::fs::write(
            client_path(dir.path()),
            r#"{"installed":{"client_id":"file","client_secret":"fs"}}"#,
        )
        .unwrap();
        let cfg = load_client_file(dir.path()).unwrap();
        assert_eq!(cfg.client_id, "file");
        assert_eq!(cfg.client_secret, "fs");
    }

    #[test]
    fn missing_file_is_not_configured_with_hint() {
        let dir = tmp();
        let err = resolve_client(None, None, dir.path()).unwrap_err();
        assert_eq!(err.code(), "not_configured");
        let msg = err.to_string();
        assert!(
            msg.contains(&client_path(dir.path()).display().to_string()),
            "{msg}"
        );
        assert!(msg.contains("see docs/google-docs.md"), "{msg}");
        assert!(msg.contains(CLIENT_ID_ENV), "{msg}");
    }

    #[test]
    fn malformed_file_is_not_configured_with_hint() {
        let dir = tmp();
        std::fs::write(client_path(dir.path()), "{").unwrap();
        let err = resolve_client(None, None, dir.path()).unwrap_err();
        assert_eq!(err.code(), "not_configured");
        let msg = err.to_string();
        assert!(msg.contains("client.json"), "{msg}");
        assert!(msg.contains("see docs/google-docs.md"), "{msg}");
    }

    #[test]
    fn debug_redacts_secret() {
        let cfg = ClientConfig {
            client_id: "id".into(),
            client_secret: "hunter2".into(),
        };
        let shown = format!("{cfg:?}");
        assert!(shown.contains("id"));
        assert!(!shown.contains("hunter2"));
    }
}
