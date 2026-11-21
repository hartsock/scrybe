// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Offline test doubles. Public (but `doc(hidden)`) so downstream crates'
//! tests — scrybe-tools, scrybe-app — can script Google without a network.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::http::{HttpClient, Method, Request, Response};
use crate::{Gdocs, GdocsError, Result, TokenProvider};

/// A [`Gdocs`] over a fresh [`FakeHttp`] that resolves its OAuth client from
/// `<config_dir>/client.json` only (never the process environment) and whose
/// browser opener fails, so a test that unexpectedly reaches the consent
/// step fails at once instead of waiting minutes for a redirect. Override
/// either with the usual builders.
pub fn offline_gdocs(config_dir: impl Into<PathBuf>) -> Gdocs<FakeHttp> {
    Gdocs::new(FakeHttp::new(), config_dir.into())
        .with_client_source(crate::config::load_client_file)
        .with_browser(|url| {
            Err(GdocsError::AuthFailed(format!(
                "tests open no browser (asked for {url})"
            )))
        })
}

type Matcher = Box<dyn Fn(&Request) -> bool + Send + Sync>;
type Responder = Box<dyn Fn(&Request) -> Response + Send + Sync>;

struct Rule {
    matcher: Matcher,
    responder: Responder,
    /// `None` = unlimited; `Some(n)` = serve n more times then fall through.
    remaining: Option<usize>,
}

/// A scripted [`HttpClient`]: rules are tried in registration order; the
/// first live match answers. Unmatched requests get a 599 whose body names
/// the request, so a test fails loudly instead of hanging on a guess.
#[derive(Default)]
pub struct FakeHttp {
    rules: Mutex<Vec<Rule>>,
    requests: Mutex<Vec<Request>>,
}

impl FakeHttp {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a rule for requests matching `matcher`.
    pub fn on(
        &self,
        matcher: impl Fn(&Request) -> bool + Send + Sync + 'static,
    ) -> RuleBuilder<'_> {
        RuleBuilder {
            fake: self,
            matcher: Box::new(matcher),
            remaining: None,
        }
    }

    /// Register a rule for `method` requests whose URL contains `needle`.
    pub fn on_url(&self, method: Method, needle: &str) -> RuleBuilder<'_> {
        let needle = needle.to_string();
        self.on(move |req| req.method == method && req.url.contains(&needle))
    }

    /// Every request seen so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().expect("FakeHttp lock").clone()
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().expect("FakeHttp lock").len()
    }
}

pub struct RuleBuilder<'a> {
    fake: &'a FakeHttp,
    matcher: Matcher,
    remaining: Option<usize>,
}

impl RuleBuilder<'_> {
    pub fn once(mut self) -> Self {
        self.remaining = Some(1);
        self
    }

    pub fn times(mut self, n: usize) -> Self {
        self.remaining = Some(n);
        self
    }

    /// Reply with a JSON body.
    pub fn json(self, status: u16, body: serde_json::Value) {
        let bytes = serde_json::to_vec(&body).expect("serialize fake JSON");
        self.bytes(status, bytes, "application/json");
    }

    pub fn text(self, status: u16, body: &str) {
        self.bytes(
            status,
            body.as_bytes().to_vec(),
            "text/plain; charset=utf-8",
        );
    }

    pub fn bytes(self, status: u16, body: Vec<u8>, content_type: &str) {
        let content_type = content_type.to_string();
        self.reply(move |_| Response {
            status,
            headers: vec![("Content-Type".into(), content_type.clone())],
            body: body.clone(),
        });
    }

    /// Reply with a response computed from the request.
    pub fn reply(self, f: impl Fn(&Request) -> Response + Send + Sync + 'static) {
        self.fake.rules.lock().expect("FakeHttp lock").push(Rule {
            matcher: self.matcher,
            responder: Box::new(f),
            remaining: self.remaining,
        });
    }
}

impl HttpClient for FakeHttp {
    fn send(&self, req: Request) -> Result<Response> {
        self.requests
            .lock()
            .expect("FakeHttp lock")
            .push(req.clone());
        let mut rules = self.rules.lock().expect("FakeHttp lock");
        for rule in rules.iter_mut() {
            if rule.remaining == Some(0) || !(rule.matcher)(&req) {
                continue;
            }
            if let Some(n) = rule.remaining.as_mut() {
                *n -= 1;
            }
            return Ok((rule.responder)(&req));
        }
        Ok(Response {
            status: 599,
            headers: Vec::new(),
            body: format!(
                "FakeHttp: no rule matched {} {}",
                req.method.as_str(),
                req.url
            )
            .into_bytes(),
        })
    }
}

/// A [`TokenProvider`] that always hands out the same token.
pub struct StaticToken(pub String);

impl TokenProvider for StaticToken {
    fn access_token(&self) -> Result<String> {
        Ok(self.0.clone())
    }

    fn refresh(&self) -> Result<String> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_match_in_order_and_once_expires() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/files/abc")
            .once()
            .text(200, "first");
        fake.on_url(Method::Get, "/files").text(200, "fallback");

        let a = fake
            .send(Request::get("https://x.test/drive/v3/files/abc"))
            .unwrap();
        let b = fake
            .send(Request::get("https://x.test/drive/v3/files/abc"))
            .unwrap();
        assert_eq!(a.text(), "first");
        assert_eq!(b.text(), "fallback");
        assert_eq!(fake.request_count(), 2);
    }

    #[test]
    fn unmatched_requests_fail_loudly() {
        let fake = FakeHttp::new();
        let resp = fake.send(Request::post("https://x.test/nope")).unwrap();
        assert_eq!(resp.status, 599);
        assert!(resp.text().contains("POST https://x.test/nope"));
    }

    #[test]
    fn offline_gdocs_reads_no_environment_and_opens_no_browser() {
        let dir = tempfile::tempdir().unwrap();
        let g = offline_gdocs(dir.path());
        assert_eq!(g.config_dir, dir.path());
        assert_eq!(
            (g.load_client)(&g.config_dir).unwrap_err().code(),
            "not_configured"
        );
        let err = (g.open_browser)("http://x.test/consent").unwrap_err();
        assert_eq!(err.code(), "auth_failed");
        assert!(err.to_string().contains("http://x.test/consent"), "{err}");
        assert_eq!(g.http.request_count(), 0);
    }

    #[test]
    fn reply_sees_the_request() {
        let fake = FakeHttp::new();
        fake.on(|r| r.method == Method::Post).reply(|r| Response {
            status: 201,
            headers: vec![],
            body: r.body.clone(),
        });
        let resp = fake
            .send(Request::post("https://x.test/echo").body("hi"))
            .unwrap();
        assert_eq!(resp.status, 201);
        assert_eq!(resp.text(), "hi");
    }
}
