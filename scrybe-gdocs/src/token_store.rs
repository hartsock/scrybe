// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The cached OAuth token on disk (`token.json`, mode 0600) and the
//! [`TokenProvider`] that keeps it fresh.

use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{self, ClientConfig};
use crate::http::HttpClient;
use crate::{oauth, Gdocs, GdocsError, Result, TokenProvider};

/// Refresh when the access token has less than this long to live.
pub const REFRESH_LEEWAY: Duration = Duration::from_secs(60);

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
    /// The client the token was issued to; a new client id needs a new login.
    pub client_id: String,
}

impl TokenSet {
    /// True when the access token expires within `leeway` of `now`.
    pub fn is_expiring(&self, now: DateTime<Utc>, leeway: Duration) -> bool {
        let leeway = chrono::Duration::from_std(leeway).unwrap_or(chrono::Duration::MAX);
        self.expires_at.signed_duration_since(now) <= leeway
    }
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .field("client_id", &self.client_id)
            .finish()
    }
}

/// `None` when there is no token file; `Invalid` when it cannot be parsed.
pub fn load(dir: &Path) -> Result<Option<TokenSet>> {
    let path = config::token_path(dir);
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    serde_json::from_slice(&raw).map(Some).map_err(|e| {
        GdocsError::Invalid(format!(
            "malformed token file {}: {e}; run `scrybe gdoc logout` then `scrybe gdoc login`",
            path.display()
        ))
    })
}

/// Write atomically (temp file + rename), owner-only on Unix.
pub fn save(dir: &Path, tokens: &TokenSet) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = config::token_path(dir);
    let tmp = dir.join(format!(".token.json.{}.tmp", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(tokens)?;
    let written = write_private(&tmp, &bytes).and_then(|()| std::fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.map_err(Into::into)
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Remove the token file; already absent is fine.
pub fn clear(dir: &Path) -> Result<()> {
    match std::fs::remove_file(config::token_path(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn not_signed_in() -> GdocsError {
    GdocsError::NotLoggedIn("not signed in to Google; run `scrybe gdoc login`".into())
}

/// `NotLoggedIn` when `tokens` was issued to a client other than `client`:
/// its refresh token is useless with the configured secret, so a new login
/// is the only way forward.
pub fn check_client(tokens: &TokenSet, client: &ClientConfig) -> Result<()> {
    if tokens.client_id == client.client_id {
        Ok(())
    } else {
        Err(GdocsError::NotLoggedIn(
            "saved Google token was issued to a different OAuth client; run `scrybe gdoc login` \
             again"
                .into(),
        ))
    }
}

/// A [`TokenProvider`] over the token file: caches in memory, refreshes
/// near expiry (or on demand after a 401), and persists what it refreshes.
pub struct Authenticator<'a, H: HttpClient> {
    gdocs: &'a Gdocs<H>,
    cached: Mutex<Option<TokenSet>>,
}

impl<'a, H: HttpClient> Authenticator<'a, H> {
    pub fn new(gdocs: &'a Gdocs<H>) -> Self {
        Self {
            gdocs,
            cached: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<TokenSet>> {
        self.cached.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Fill the cache from disk if empty; error when no OAuth client is
    /// configured, there is no token, or it belongs to another client. The
    /// client is resolved first, as `oauth::status` does, so a missing one
    /// is `NotConfigured` with or without a token: the first-run answer is
    /// "set up a client", never "sign in" to a client that does not exist.
    /// The check runs once, here: the cache is only ever refilled by
    /// `refresh_into`, whose result carries the configured client's id by
    /// construction.
    fn ensure_loaded<'c>(&self, cache: &'c mut Option<TokenSet>) -> Result<&'c TokenSet> {
        if cache.is_none() {
            let client = (self.gdocs.load_client)(&self.gdocs.config_dir)?;
            let loaded = load(&self.gdocs.config_dir)?;
            if let Some(tokens) = &loaded {
                check_client(tokens, &client)?;
            }
            *cache = loaded;
        }
        cache.as_ref().ok_or_else(not_signed_in)
    }

    fn refresh_into(&self, cache: &mut Option<TokenSet>) -> Result<String> {
        let current = self.ensure_loaded(cache)?;
        let refresh_token = current.refresh_token.clone().ok_or_else(|| {
            GdocsError::NotLoggedIn(
                "saved Google token has no refresh token; run `scrybe gdoc login` again".into(),
            )
        })?;
        let client = (self.gdocs.load_client)(&self.gdocs.config_dir)?;
        let fresh = match oauth::refresh(
            &self.gdocs.http,
            &self.gdocs.endpoints,
            &client,
            &refresh_token,
        ) {
            Ok(fresh) => fresh,
            Err(e @ GdocsError::NotLoggedIn(_)) => {
                // Google rejected the grant itself, so the saved token is
                // dead: forget it, or `status` keeps saying logged_in and
                // every call re-sends it. An outage (AuthFailed/Http) must
                // not clear anything; the token may be fine.
                *cache = None;
                let _ = clear(&self.gdocs.config_dir);
                return Err(e);
            }
            Err(e) => return Err(e),
        };
        save(&self.gdocs.config_dir, &fresh)?;
        let token = fresh.access_token.clone();
        *cache = Some(fresh);
        Ok(token)
    }
}

impl<H: HttpClient> TokenProvider for Authenticator<'_, H> {
    fn access_token(&self) -> Result<String> {
        let mut cache = self.lock();
        let current = self.ensure_loaded(&mut cache)?;
        if current.is_expiring(Utc::now(), REFRESH_LEEWAY) {
            return self.refresh_into(&mut cache);
        }
        Ok(current.access_token.clone())
    }

    fn refresh(&self) -> Result<String> {
        let mut cache = self.lock();
        self.refresh_into(&mut cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Method;
    use crate::testing::FakeHttp;
    use crate::{Endpoints, SCOPE_DRIVE_FILE};
    use serde_json::json;

    fn tokens(access: &str, refresh: Option<&str>, ttl_secs: i64) -> TokenSet {
        TokenSet {
            access_token: access.into(),
            refresh_token: refresh.map(str::to_string),
            expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs),
            scopes: vec![SCOPE_DRIVE_FILE.into()],
            client_id: "cid".into(),
        }
    }

    fn gdocs(dir: &Path) -> Gdocs<FakeHttp> {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            config::client_path(dir),
            r#"{"installed":{"client_id":"cid","client_secret":"sec"}}"#,
        )
        .unwrap();
        crate::testing::offline_gdocs(dir).with_endpoints(Endpoints {
            token: "http://fake.test/token".into(),
            ..Endpoints::default()
        })
    }

    fn stub_refresh_failure(fake: &FakeHttp, status: u16, body: serde_json::Value) {
        fake.on_url(Method::Post, "/token").json(status, body);
    }

    fn stub_refresh(fake: &FakeHttp, access: &str) {
        fake.on_url(Method::Post, "/token").json(
            200,
            json!({
                "access_token": access,
                "expires_in": 3600,
                "scope": SCOPE_DRIVE_FILE,
                "token_type": "Bearer"
            }),
        );
    }

    #[test]
    fn save_load_round_trip_is_private_and_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("nested");
        let t = tokens("a1", Some("r1"), 3600);
        save(&sub, &t).unwrap();
        assert_eq!(load(&sub).unwrap(), Some(t));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(config::token_path(&sub))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "mode {mode:o}");
        }
        let names: Vec<String> = std::fs::read_dir(&sub)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["token.json".to_string()],
            "no temp file left behind"
        );

        let t2 = tokens("a2", Some("r1"), 3600);
        save(&sub, &t2).unwrap();
        assert_eq!(load(&sub).unwrap().unwrap().access_token, "a2");
    }

    #[test]
    fn load_missing_is_none_and_malformed_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), None);
        std::fs::write(config::token_path(dir.path()), "{nope").unwrap();
        let err = load(dir.path()).unwrap_err();
        assert_eq!(err.code(), "invalid_argument");
        clear(dir.path()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), None);
        clear(dir.path()).unwrap();
    }

    #[test]
    fn expiry_leeway() {
        let now = Utc::now();
        let t = tokens("a", None, 120);
        assert!(!t.is_expiring(now, Duration::from_secs(60)));
        assert!(t.is_expiring(now, Duration::from_secs(121)));
        let stale = tokens("a", None, -5);
        assert!(stale.is_expiring(now, Duration::ZERO));
    }

    #[test]
    fn debug_redacts_tokens() {
        let shown = format!("{:?}", tokens("secret-access", Some("secret-refresh"), 10));
        assert!(!shown.contains("secret-access"));
        assert!(!shown.contains("secret-refresh"));
        assert!(shown.contains("cid"));
    }

    #[test]
    fn fresh_token_is_served_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        save(dir.path(), &tokens("a1", Some("r1"), 3600)).unwrap();
        let auth = Authenticator::new(&g);
        assert_eq!(auth.access_token().unwrap(), "a1");
        assert_eq!(auth.access_token().unwrap(), "a1");
        assert_eq!(g.http.request_count(), 0);
    }

    #[test]
    fn expiring_token_is_refreshed_saved_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        stub_refresh(&g.http, "a2");
        save(dir.path(), &tokens("a1", Some("r1"), 30)).unwrap();

        let auth = Authenticator::new(&g);
        assert_eq!(auth.access_token().unwrap(), "a2");
        assert_eq!(g.http.request_count(), 1);
        let fields = g.http.requests()[0].form_fields();
        assert!(fields.contains(&("grant_type".into(), "refresh_token".into())));
        assert!(fields.contains(&("refresh_token".into(), "r1".into())));

        let on_disk = load(dir.path()).unwrap().unwrap();
        assert_eq!(on_disk.access_token, "a2");
        assert_eq!(on_disk.refresh_token.as_deref(), Some("r1"));
        assert_eq!(on_disk.client_id, "cid");

        assert_eq!(auth.access_token().unwrap(), "a2");
        assert_eq!(g.http.request_count(), 1, "second call hits the cache");
    }

    #[test]
    fn forced_refresh_ignores_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        stub_refresh(&g.http, "a2");
        save(dir.path(), &tokens("a1", Some("r1"), 3600)).unwrap();
        let auth = Authenticator::new(&g);
        assert_eq!(auth.access_token().unwrap(), "a1");
        assert_eq!(auth.refresh().unwrap(), "a2");
        assert_eq!(auth.access_token().unwrap(), "a2");
        assert_eq!(g.http.request_count(), 1);
    }

    #[test]
    fn no_token_file_is_not_logged_in() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        let auth = Authenticator::new(&g);
        let err = auth.access_token().unwrap_err();
        assert_eq!(err.code(), "not_logged_in");
        assert!(err.to_string().contains("scrybe gdoc login"), "{err}");
        assert_eq!(auth.refresh().unwrap_err().code(), "not_logged_in");
    }

    /// No OAuth client is `not_configured` whether or not a token exists —
    /// the same answer `oauth::status` gives — so a fresh machine is told
    /// what to set up rather than to sign in to nothing.
    #[test]
    fn no_client_is_not_configured_before_any_token_check() {
        let dir = tempfile::tempdir().unwrap();
        let g = crate::testing::offline_gdocs(dir.path());
        let auth = Authenticator::new(&g);
        assert_eq!(auth.access_token().unwrap_err().code(), "not_configured");
        assert_eq!(auth.refresh().unwrap_err().code(), "not_configured");

        save(dir.path(), &tokens("a1", Some("r1"), 3600)).unwrap();
        let err = auth.access_token().unwrap_err();
        assert_eq!(err.code(), "not_configured", "a token is no substitute");
        assert!(err.to_string().contains("client.json"), "{err}");
        assert_eq!(g.http.request_count(), 0);
    }

    #[test]
    fn missing_refresh_token_is_not_logged_in() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        save(dir.path(), &tokens("a1", None, 10)).unwrap();
        let auth = Authenticator::new(&g);
        assert_eq!(auth.access_token().unwrap_err().code(), "not_logged_in");
        assert_eq!(g.http.request_count(), 0);
    }

    /// The user revoked Scrybe (or Google expired the grant): the token
    /// endpoint says `invalid_grant`, and the dead token must go so that
    /// `status` agrees and no call re-sends it.
    #[test]
    fn invalid_grant_clears_the_dead_token() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        stub_refresh_failure(
            &g.http,
            400,
            json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."}),
        );
        save(dir.path(), &tokens("dead", Some("dead-refresh"), 30)).unwrap();

        let auth = Authenticator::new(&g);
        let err = auth.access_token().unwrap_err();
        assert_eq!(err.code(), "not_logged_in");
        assert!(err.to_string().contains("scrybe gdoc login"), "{err}");
        assert_eq!(g.http.request_count(), 1);
        assert_eq!(load(dir.path()).unwrap(), None, "token.json is gone");
        assert!(!config::token_path(dir.path()).exists());
        assert_eq!(
            oauth::status(&g).unwrap().status,
            crate::types::AuthState::LoggedOut
        );

        let again = auth.access_token().unwrap_err();
        assert_eq!(again.code(), "not_logged_in");
        assert_eq!(auth.refresh().unwrap_err().code(), "not_logged_in");
        assert_eq!(g.http.request_count(), 1, "a dead token is never re-sent");
    }

    #[test]
    fn a_token_endpoint_outage_keeps_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        stub_refresh_failure(&g.http, 500, json!({"error": "internal"}));
        let saved = tokens("a1", Some("r1"), 30);
        save(dir.path(), &saved).unwrap();

        let auth = Authenticator::new(&g);
        assert_eq!(auth.access_token().unwrap_err().code(), "auth_failed");
        assert_eq!(load(dir.path()).unwrap(), Some(saved));
        assert_eq!(auth.refresh().unwrap_err().code(), "auth_failed");
        assert_eq!(g.http.request_count(), 2, "each call retries Google");
    }

    #[test]
    fn token_from_another_client_is_not_logged_in() {
        let dir = tempfile::tempdir().unwrap();
        let g = gdocs(dir.path());
        stub_refresh(&g.http, "never");
        let mut other = tokens("a1", Some("r1"), 3600);
        other.client_id = "other".into();
        save(dir.path(), &other).unwrap();

        let auth = Authenticator::new(&g);
        for err in [
            auth.access_token().unwrap_err(),
            auth.refresh().unwrap_err(),
        ] {
            assert_eq!(err.code(), "not_logged_in");
            assert!(err.to_string().contains("scrybe gdoc login"), "{err}");
        }
        assert_eq!(g.http.request_count(), 0);
        assert_eq!(
            load(dir.path()).unwrap(),
            Some(other),
            "a mismatch is not a revocation; logout cleans it up"
        );
    }
}
