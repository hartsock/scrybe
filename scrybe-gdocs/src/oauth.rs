// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The OAuth 2.0 installed-app flow with PKCE: build the consent URL, trade
//! the code for tokens, refresh, revoke, and the `login`/`logout`/`status`
//! entry points over them.

use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;

use crate::config::ClientConfig;
use crate::http::{HttpClient, Request, Response};
use crate::loopback::Listener;
use crate::pkce::{random_state, Pkce};
use crate::token_store::{self, TokenSet};
use crate::types::{AuthState, AuthStatus};
use crate::{Endpoints, Gdocs, GdocsError, Result, SCOPE_DRIVE_FILE};

/// How long `login` waits for the user to finish Google's consent screen.
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(300);

const DEFAULT_EXPIRES_IN: i64 = 3600;
/// Longest TTL believed from a token endpoint (access tokens live an hour; a
/// year is already absurd). Keeps the expiry arithmetic in chrono's range.
const MAX_EXPIRES_IN: i64 = 365 * 24 * 3600;

/// The authorization URL to open in the browser.
pub fn auth_url(
    endpoints: &Endpoints,
    client: &ClientConfig,
    pkce: &Pkce,
    state: &str,
    redirect_uri: &str,
) -> String {
    let params = [
        ("client_id", client.client_id.as_str()),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", SCOPE_DRIVE_FILE),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    match url::Url::parse(&endpoints.auth) {
        Ok(mut url) => {
            url.query_pairs_mut().extend_pairs(params);
            url.into()
        }
        Err(_) => {
            let mut ser = url::form_urlencoded::Serializer::new(String::new());
            ser.extend_pairs(params);
            let sep = if endpoints.auth.contains('?') {
                '&'
            } else {
                '?'
            };
            format!("{}{sep}{}", endpoints.auth, ser.finish())
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

impl TokenResponse {
    fn into_token_set(
        self,
        client_id: &str,
        fallback_refresh: Option<&str>,
        now: DateTime<Utc>,
    ) -> TokenSet {
        let expires_in = self
            .expires_in
            .unwrap_or(DEFAULT_EXPIRES_IN)
            .clamp(0, MAX_EXPIRES_IN);
        let scopes: Vec<String> = match self.scope.as_deref() {
            Some(s) if !s.trim().is_empty() => s.split_whitespace().map(str::to_string).collect(),
            _ => vec![SCOPE_DRIVE_FILE.to_string()],
        };
        TokenSet {
            access_token: self.access_token,
            refresh_token: self
                .refresh_token
                .or_else(|| fallback_refresh.map(str::to_string)),
            expires_at: now + chrono::Duration::seconds(expires_in),
            scopes,
            client_id: client_id.to_string(),
        }
    }
}

/// Google's `{error, error_description}` body, or the raw text.
struct GoogleError {
    code: Option<String>,
    detail: String,
}

fn google_error(resp: &Response) -> GoogleError {
    let text = resp.text();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        let code = v
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let description = v
            .get("error_description")
            .and_then(serde_json::Value::as_str);
        if let Some(code) = code {
            let detail = match description {
                Some(d) => format!("{code}: {d}"),
                None => code.clone(),
            };
            return GoogleError {
                code: Some(code),
                detail,
            };
        }
    }
    let mut detail: String = text.trim().chars().take(200).collect();
    if detail.is_empty() {
        detail = format!("HTTP {}", resp.status);
    }
    GoogleError { code: None, detail }
}

/// Trade the authorization code for tokens.
///
/// Google Desktop-app clients require `client_secret` even with PKCE.
pub fn exchange_code(
    http: &impl HttpClient,
    endpoints: &Endpoints,
    client: &ClientConfig,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<TokenSet> {
    let req = Request::post(&endpoints.token).form(&[
        ("client_id", client.client_id.as_str()),
        ("client_secret", client.client_secret.as_str()),
        ("code", code),
        ("code_verifier", verifier),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ]);
    let resp = http.send(req)?;
    if !resp.is_success() {
        let err = google_error(&resp);
        return Err(GdocsError::AuthFailed(format!(
            "Google token exchange failed (HTTP {}): {}",
            resp.status, err.detail
        )));
    }
    let body: TokenResponse = resp
        .json()
        .map_err(|e| GdocsError::AuthFailed(format!("Google token response: {e}")))?;
    Ok(body.into_token_set(&client.client_id, None, Utc::now()))
}

/// Get a new access token. Google omits `refresh_token` in refresh
/// responses, so the one passed in is carried forward.
pub fn refresh(
    http: &impl HttpClient,
    endpoints: &Endpoints,
    client: &ClientConfig,
    refresh_token: &str,
) -> Result<TokenSet> {
    let req = Request::post(&endpoints.token).form(&[
        ("client_id", client.client_id.as_str()),
        ("client_secret", client.client_secret.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ]);
    let resp = http.send(req)?;
    if !resp.is_success() {
        let err = google_error(&resp);
        if matches!(resp.status, 400 | 401) && err.code.as_deref() == Some("invalid_grant") {
            return Err(GdocsError::NotLoggedIn(
                "Google rejected the saved token (revoked, expired, or the OAuth client \
                 changed); run `scrybe gdoc login` again"
                    .into(),
            ));
        }
        return Err(GdocsError::AuthFailed(format!(
            "Google token refresh failed (HTTP {}): {}",
            resp.status, err.detail
        )));
    }
    let body: TokenResponse = resp
        .json()
        .map_err(|e| GdocsError::AuthFailed(format!("Google token response: {e}")))?;
    Ok(body.into_token_set(&client.client_id, Some(refresh_token), Utc::now()))
}

/// Revoke a token (access or refresh; Google revokes the whole grant).
pub fn revoke(http: &impl HttpClient, endpoints: &Endpoints, token: &str) -> Result<()> {
    let resp = http.send(Request::post(&endpoints.revoke).form(&[("token", token)]))?;
    if resp.is_success() {
        Ok(())
    } else {
        Err(GdocsError::Api {
            status: resp.status,
            message: format!("revoke: {}", google_error(&resp).detail),
        })
    }
}

/// Run the browser consent flow and cache the resulting token.
pub fn login(gdocs: &Gdocs<impl HttpClient>) -> Result<AuthStatus> {
    let client = (gdocs.load_client)(&gdocs.config_dir)?;
    let pkce = Pkce::generate();
    let state = random_state();
    let listener = Listener::bind()?;
    let redirect_uri = listener.redirect_uri();
    let url = auth_url(&gdocs.endpoints, &client, &pkce, &state, &redirect_uri);
    (gdocs.open_browser)(&url)?;
    let code = listener.wait_for_code(&state, CONSENT_TIMEOUT)?;
    let tokens = exchange_code(
        &gdocs.http,
        &gdocs.endpoints,
        &client,
        &code,
        &pkce.verifier,
        &redirect_uri,
    )?;
    token_store::save(&gdocs.config_dir, &tokens)?;
    status(gdocs)
}

/// Revoke (best effort) and forget the cached token.
pub fn logout(gdocs: &Gdocs<impl HttpClient>) -> Result<AuthStatus> {
    if let Ok(Some(tokens)) = token_store::load(&gdocs.config_dir) {
        let token = tokens
            .refresh_token
            .as_deref()
            .unwrap_or(&tokens.access_token);
        // Already revoked, offline, or a stale client: the local token goes either way.
        let _ = revoke(&gdocs.http, &gdocs.endpoints, token);
    }
    token_store::clear(&gdocs.config_dir)?;
    status(gdocs)
}

/// Where sign-in stands, without talking to Google.
///
/// `LoggedIn` means the saved token is one the call path would use: issued
/// to the configured client, and either refreshable or not yet expiring
/// (the same test [`crate::TokenProvider::access_token`] applies).
pub fn status(gdocs: &Gdocs<impl HttpClient>) -> Result<AuthStatus> {
    let config_dir = gdocs.config_dir.display().to_string();
    let Ok(client) = (gdocs.load_client)(&gdocs.config_dir) else {
        return Ok(AuthStatus {
            status: AuthState::NotConfigured,
            expires_at: None,
            config_dir,
        });
    };
    let usable = token_store::load(&gdocs.config_dir)?.filter(|tokens| {
        token_store::check_client(tokens, &client).is_ok()
            && (tokens.refresh_token.is_some()
                || !tokens.is_expiring(Utc::now(), token_store::REFRESH_LEEWAY))
    });
    let status = match usable {
        Some(tokens) => AuthStatus {
            status: AuthState::LoggedIn,
            expires_at: Some(tokens.expires_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
            config_dir,
        },
        None => AuthStatus {
            status: AuthState::LoggedOut,
            expires_at: None,
            config_dir,
        },
    };
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::http::Method;
    use crate::testing::{offline_gdocs, FakeHttp};
    use serde_json::json;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    fn client() -> ClientConfig {
        ClientConfig {
            client_id: "cid.apps.googleusercontent.com".into(),
            client_secret: "GOCSPX-secret".into(),
        }
    }

    fn endpoints() -> Endpoints {
        Endpoints {
            auth: "http://fake.test/auth".into(),
            token: "http://fake.test/token".into(),
            revoke: "http://fake.test/revoke".into(),
            drive: "http://fake.test/drive/v3".into(),
            upload: "http://fake.test/upload/drive/v3".into(),
        }
    }

    fn query_map(url: &str) -> HashMap<String, String> {
        url::Url::parse(url)
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    fn fields(req: &Request) -> HashMap<String, String> {
        req.form_fields().into_iter().collect()
    }

    fn write_client(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            config::client_path(dir),
            r#"{"installed":{"client_id":"cid.apps.googleusercontent.com","client_secret":"GOCSPX-secret"}}"#,
        )
        .unwrap();
    }

    fn token_json(access: &str, refresh: Option<&str>) -> serde_json::Value {
        let mut v = json!({
            "access_token": access,
            "expires_in": 3599,
            "scope": SCOPE_DRIVE_FILE,
            "token_type": "Bearer"
        });
        if let Some(r) = refresh {
            v["refresh_token"] = json!(r);
        }
        v
    }

    #[test]
    fn auth_url_carries_every_parameter_encoded() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        let url = auth_url(
            &endpoints(),
            &client(),
            &pkce,
            "st&ate",
            "http://127.0.0.1:4242/",
        );
        assert!(url.starts_with("http://fake.test/auth?"), "{url}");
        let q = query_map(&url);
        assert_eq!(q["client_id"], "cid.apps.googleusercontent.com");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:4242/");
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["scope"], SCOPE_DRIVE_FILE);
        assert_eq!(
            q["code_challenge"],
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["state"], "st&ate");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["prompt"], "consent");
        assert!(!url.contains("client_secret"));
        assert!(url.contains("state=st%26ate"), "{url}");
    }

    #[test]
    fn exchange_code_posts_exact_form() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token")
            .json(200, token_json("at-1", Some("rt-1")));
        let before = Utc::now();
        let t = exchange_code(
            &fake,
            &endpoints(),
            &client(),
            "4/the-code",
            "verifier-xyz",
            "http://127.0.0.1:5555/",
        )
        .unwrap();
        assert_eq!(t.access_token, "at-1");
        assert_eq!(t.refresh_token.as_deref(), Some("rt-1"));
        assert_eq!(t.scopes, vec![SCOPE_DRIVE_FILE.to_string()]);
        assert_eq!(t.client_id, "cid.apps.googleusercontent.com");
        let ttl = t.expires_at.signed_duration_since(before).num_seconds();
        assert!((3590..=3600).contains(&ttl), "ttl {ttl}");

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "http://fake.test/token");
        assert_eq!(
            reqs[0].header_value("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        let mut got = reqs[0].form_fields();
        got.sort();
        let mut want: Vec<(String, String)> = [
            ("client_id", "cid.apps.googleusercontent.com"),
            ("client_secret", "GOCSPX-secret"),
            ("code", "4/the-code"),
            ("code_verifier", "verifier-xyz"),
            ("grant_type", "authorization_code"),
            ("redirect_uri", "http://127.0.0.1:5555/"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn absurd_expires_in_is_clamped_not_panicked_on() {
        for huge in [i64::MAX, 100_000_000_000_000] {
            let fake = FakeHttp::new();
            let mut body = token_json("at", Some("rt"));
            body["expires_in"] = json!(huge);
            fake.on_url(Method::Post, "/token").json(200, body);
            let before = Utc::now();
            let exchanged = exchange_code(&fake, &endpoints(), &client(), "c", "v", "r").unwrap();
            let refreshed = refresh(&fake, &endpoints(), &client(), "rt").unwrap();
            for t in [exchanged, refreshed] {
                let ttl = t.expires_at.signed_duration_since(before).num_seconds();
                assert!((0..=MAX_EXPIRES_IN).contains(&ttl), "ttl {ttl} for {huge}");
            }
        }
    }

    #[test]
    fn exchange_code_failure_reports_googles_reason() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token").json(
            400,
            json!({"error": "invalid_grant", "error_description": "Bad Request"}),
        );
        let err = exchange_code(&fake, &endpoints(), &client(), "c", "v", "r").unwrap_err();
        assert_eq!(err.code(), "auth_failed");
        let msg = err.to_string();
        assert!(msg.contains("invalid_grant: Bad Request"), "{msg}");
        assert!(msg.contains("400"), "{msg}");
    }

    #[test]
    fn exchange_code_non_json_failure_still_explains() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token")
            .text(502, "<html>bad gateway</html>");
        let err = exchange_code(&fake, &endpoints(), &client(), "c", "v", "r").unwrap_err();
        assert!(err.to_string().contains("bad gateway"), "{err}");
    }

    #[test]
    fn refresh_keeps_old_refresh_token_and_posts_grant() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token")
            .json(200, token_json("at-2", None));
        let t = refresh(&fake, &endpoints(), &client(), "rt-old").unwrap();
        assert_eq!(t.access_token, "at-2");
        assert_eq!(t.refresh_token.as_deref(), Some("rt-old"));
        let f = fields(&fake.requests()[0]);
        assert_eq!(f["grant_type"], "refresh_token");
        assert_eq!(f["refresh_token"], "rt-old");
        assert_eq!(f["client_id"], "cid.apps.googleusercontent.com");
        assert_eq!(f["client_secret"], "GOCSPX-secret");
        assert!(!f.contains_key("code"));
    }

    #[test]
    fn refresh_uses_new_refresh_token_when_google_sends_one() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token")
            .json(200, token_json("at-2", Some("rt-new")));
        let t = refresh(&fake, &endpoints(), &client(), "rt-old").unwrap();
        assert_eq!(t.refresh_token.as_deref(), Some("rt-new"));
    }

    #[test]
    fn refresh_invalid_grant_is_not_logged_in() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token").json(
            400,
            json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."}),
        );
        let err = refresh(&fake, &endpoints(), &client(), "rt").unwrap_err();
        assert_eq!(err.code(), "not_logged_in");
        assert!(err.to_string().contains("scrybe gdoc login"), "{err}");
    }

    #[test]
    fn refresh_other_failures_are_auth_failed() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token")
            .json(500, json!({"error": "internal"}));
        let err = refresh(&fake, &endpoints(), &client(), "rt").unwrap_err();
        assert_eq!(err.code(), "auth_failed");
        assert!(err.to_string().contains("internal"), "{err}");

        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/token")
            .json(401, json!({"error": "invalid_client"}));
        let err = refresh(&fake, &endpoints(), &client(), "rt").unwrap_err();
        assert_eq!(err.code(), "auth_failed");
    }

    #[test]
    fn revoke_posts_token_and_maps_status() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/revoke").once().text(200, "");
        fake.on_url(Method::Post, "/revoke")
            .json(400, json!({"error": "invalid_token"}));
        revoke(&fake, &endpoints(), "tok").unwrap();
        assert_eq!(fields(&fake.requests()[0])["token"], "tok");
        let err = revoke(&fake, &endpoints(), "tok").unwrap_err();
        assert_eq!(err.code(), "api_error");
    }

    #[test]
    fn login_end_to_end_through_real_loopback() {
        let dir = tempfile::tempdir().unwrap();
        write_client(dir.path());
        let seen_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let record = Arc::clone(&seen_url);
        let g = offline_gdocs(dir.path())
            .with_endpoints(endpoints())
            .with_browser(move |url| {
                *record.lock().unwrap() = Some(url.to_string());
                let q = query_map(url);
                let redirect = url::Url::parse(&q["redirect_uri"]).unwrap();
                let port = redirect.port().unwrap();
                let state = q["state"].clone();
                std::thread::spawn(move || {
                    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                    let req = format!(
                        "GET /?state={state}&code=4%2Fcode-123&scope=drive.file HTTP/1.1\r\n\
                         Host: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
                    );
                    s.write_all(req.as_bytes()).expect("write");
                    let mut out = String::new();
                    let _ = s.read_to_string(&mut out);
                });
                Ok(())
            });
        g.http
            .on_url(Method::Post, "/token")
            .json(200, token_json("at-login", Some("rt-login")));

        let st = login(&g).unwrap();
        assert_eq!(st.status, AuthState::LoggedIn);
        assert!(st.expires_at.is_some());
        assert_eq!(st.config_dir, dir.path().display().to_string());

        let saved = token_store::load(dir.path()).unwrap().unwrap();
        assert_eq!(saved.access_token, "at-login");
        assert_eq!(saved.refresh_token.as_deref(), Some("rt-login"));

        let url = seen_url.lock().unwrap().clone().unwrap();
        let q = query_map(&url);
        let reqs = g.http.requests();
        assert_eq!(reqs.len(), 1);
        let f = fields(&reqs[0]);
        assert_eq!(f["code"], "4/code-123");
        assert_eq!(f["grant_type"], "authorization_code");
        assert_eq!(f["redirect_uri"], q["redirect_uri"]);
        assert_eq!(
            Pkce::from_verifier(&f["code_verifier"]).challenge,
            q["code_challenge"],
            "verifier sent matches the challenge in the consent URL"
        );
    }

    #[test]
    fn login_without_client_is_not_configured_and_opens_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let opened = Arc::new(Mutex::new(0));
        let count = Arc::clone(&opened);
        // Failing (not just counting) means a regression surfaces here
        // instead of in a five-minute wait for a redirect.
        let g = offline_gdocs(dir.path()).with_browser(move |_| {
            *count.lock().unwrap() += 1;
            Err(GdocsError::AuthFailed("browser must not open".into()))
        });
        let err = login(&g).unwrap_err();
        assert_eq!(err.code(), "not_configured");
        assert_eq!(*opened.lock().unwrap(), 0);
    }

    #[test]
    fn login_browser_failure_propagates() {
        let dir = tempfile::tempdir().unwrap();
        write_client(dir.path());
        let g = offline_gdocs(dir.path())
            .with_endpoints(endpoints())
            .with_browser(|_| Err(GdocsError::AuthFailed("no browser".into())));
        let err = login(&g).unwrap_err();
        assert_eq!(err.code(), "auth_failed");
        assert!(err.to_string().contains("no browser"));
    }

    #[test]
    fn logout_revokes_and_clears_even_if_revoke_fails() {
        let dir = tempfile::tempdir().unwrap();
        write_client(dir.path());
        let g = offline_gdocs(dir.path()).with_endpoints(endpoints());
        g.http
            .on_url(Method::Post, "/revoke")
            .json(400, json!({"error": "invalid_token"}));
        token_store::save(
            dir.path(),
            &TokenSet {
                access_token: "at".into(),
                refresh_token: Some("rt".into()),
                expires_at: Utc::now(),
                scopes: vec![],
                client_id: "cid".into(),
            },
        )
        .unwrap();

        let st = logout(&g).unwrap();
        assert_eq!(st.status, AuthState::LoggedOut);
        assert!(!config::token_path(dir.path()).exists());
        let reqs = g.http.requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(fields(&reqs[0])["token"], "rt");

        let st = logout(&g).unwrap();
        assert_eq!(st.status, AuthState::LoggedOut);
        assert_eq!(
            g.http.request_count(),
            1,
            "nothing to revoke the second time"
        );
    }

    #[test]
    fn status_transitions() {
        let dir = tempfile::tempdir().unwrap();
        let g = offline_gdocs(dir.path()).with_endpoints(endpoints());

        let st = status(&g).unwrap();
        assert_eq!(st.status, AuthState::NotConfigured);
        assert_eq!(st.expires_at, None);
        assert_eq!(st.config_dir, dir.path().display().to_string());

        write_client(dir.path());
        assert_eq!(status(&g).unwrap().status, AuthState::LoggedOut);

        let expires_at = Utc::now() + chrono::Duration::seconds(600);
        token_store::save(
            dir.path(),
            &TokenSet {
                access_token: "at".into(),
                refresh_token: Some("rt".into()),
                expires_at,
                scopes: vec![SCOPE_DRIVE_FILE.into()],
                client_id: "cid.apps.googleusercontent.com".into(),
            },
        )
        .unwrap();
        let st = status(&g).unwrap();
        assert_eq!(st.status, AuthState::LoggedIn);
        assert_eq!(
            st.expires_at.as_deref(),
            Some(
                expires_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
                    .as_str()
            )
        );
        assert_eq!(g.http.request_count(), 0, "status never talks to Google");
    }

    /// `status` and `access_token` must agree: a token the call path would
    /// refuse is reported as logged out, still without talking to Google.
    #[test]
    fn status_is_logged_out_for_a_token_the_call_path_would_refuse() {
        let dir = tempfile::tempdir().unwrap();
        write_client(dir.path());
        let g = offline_gdocs(dir.path()).with_endpoints(endpoints());
        let token = |client_id: &str, refresh: Option<&str>, ttl_secs: i64| TokenSet {
            access_token: "at".into(),
            refresh_token: refresh.map(str::to_string),
            expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs),
            scopes: vec![SCOPE_DRIVE_FILE.into()],
            client_id: client_id.into(),
        };

        token_store::save(dir.path(), &token("other-client", Some("rt"), 3600)).unwrap();
        let st = status(&g).unwrap();
        assert_eq!(st.status, AuthState::LoggedOut, "issued to another client");
        assert_eq!(st.expires_at, None);

        token_store::save(
            dir.path(),
            &token("cid.apps.googleusercontent.com", None, 30),
        )
        .unwrap();
        let st = status(&g).unwrap();
        assert_eq!(
            st.status,
            AuthState::LoggedOut,
            "no refresh token and inside the refresh leeway"
        );
        assert_eq!(st.expires_at, None);

        token_store::save(
            dir.path(),
            &token("cid.apps.googleusercontent.com", None, 3600),
        )
        .unwrap();
        assert_eq!(
            status(&g).unwrap().status,
            AuthState::LoggedIn,
            "no refresh token but still fresh"
        );
        token_store::save(
            dir.path(),
            &token("cid.apps.googleusercontent.com", Some("rt"), -5),
        )
        .unwrap();
        assert_eq!(
            status(&g).unwrap().status,
            AuthState::LoggedIn,
            "expired but refreshable"
        );
        assert_eq!(g.http.request_count(), 0);

        // A mismatched token is still cleaned up by logout.
        token_store::save(dir.path(), &token("other-client", Some("rt"), 3600)).unwrap();
        g.http.on_url(Method::Post, "/revoke").text(200, "");
        assert_eq!(logout(&g).unwrap().status, AuthState::LoggedOut);
        assert!(!config::token_path(dir.path()).exists());
    }
}
