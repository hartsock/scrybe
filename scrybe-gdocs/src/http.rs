// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! A minimal HTTP abstraction so every Google call is testable offline.
//!
//! [`crate::ReqwestHttp`] is the real transport; [`crate::testing::FakeHttp`]
//! is the scripted one used by tests. Implementations return `Err` only for
//! transport failures — any HTTP status, including 4xx/5xx, is `Ok(Response)`
//! so callers map statuses to business errors themselves.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{GdocsError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self::new(Method::Post, url)
    }

    pub fn patch(url: impl Into<String>) -> Self {
        Self::new(Method::Patch, url)
    }

    pub fn delete(url: impl Into<String>) -> Self {
        Self::new(Method::Delete, url)
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// `Authorization: Bearer <token>`.
    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// JSON body with `Content-Type: application/json`.
    pub fn json(self, value: &impl Serialize) -> Self {
        let bytes = serde_json::to_vec(value).unwrap_or_default();
        self.header("Content-Type", "application/json").body(bytes)
    }

    /// `application/x-www-form-urlencoded` body.
    pub fn form(self, fields: &[(&str, &str)]) -> Self {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in fields {
            ser.append_pair(k, v);
        }
        self.header("Content-Type", "application/x-www-form-urlencoded")
            .body(ser.finish().into_bytes())
    }

    /// First header value whose name matches case-insensitively.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        header_lookup(&self.headers, name)
    }

    /// A query parameter parsed from `url`, percent-decoded.
    pub fn query_param(&self, key: &str) -> Option<String> {
        let parsed = url::Url::parse(&self.url).ok()?;
        parsed
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    }

    /// The URL path (no scheme/host/query).
    pub fn path(&self) -> String {
        url::Url::parse(&self.url)
            .map(|u| u.path().to_string())
            .unwrap_or_default()
    }

    /// Body decoded as a form-urlencoded field list.
    pub fn form_fields(&self) -> Vec<(String, String)> {
        url::form_urlencoded::parse(&self.body)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body)
            .map_err(|e| GdocsError::Invalid(format!("malformed JSON response: {e}")))
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        header_lookup(&self.headers, name)
    }
}

fn header_lookup<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// The transport every Google call goes through.
pub trait HttpClient: Send + Sync {
    /// Perform the request. `Err` is reserved for transport failures
    /// (`GdocsError::Http`); every HTTP status comes back as `Ok`.
    fn send(&self, req: Request) -> Result<Response>;
}

impl<T: HttpClient + ?Sized> HttpClient for &T {
    fn send(&self, req: Request) -> Result<Response> {
        (**self).send(req)
    }
}

impl<T: HttpClient + ?Sized> HttpClient for Box<T> {
    fn send(&self, req: Request) -> Result<Response> {
        (**self).send(req)
    }
}

impl<T: HttpClient + ?Sized> HttpClient for std::sync::Arc<T> {
    fn send(&self, req: Request) -> Result<Response> {
        (**self).send(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encodes_and_decodes() {
        let req = Request::post("https://example.test/token").form(&[("a b", "c&d"), ("e", "f")]);
        assert_eq!(
            req.header_value("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(req.body, b"a+b=c%26d&e=f".to_vec());
        assert_eq!(
            req.form_fields(),
            vec![
                ("a b".to_string(), "c&d".to_string()),
                ("e".to_string(), "f".to_string())
            ]
        );
    }

    #[test]
    fn bearer_and_query_helpers() {
        let req =
            Request::get("https://www.googleapis.com/drive/v3/files?q=name%3D%27Scrybe%27&x=1")
                .bearer("tok");
        assert_eq!(req.header_value("AUTHORIZATION"), Some("Bearer tok"));
        assert_eq!(req.query_param("q").as_deref(), Some("name='Scrybe'"));
        assert_eq!(req.query_param("x").as_deref(), Some("1"));
        assert_eq!(req.query_param("missing"), None);
        assert_eq!(req.path(), "/drive/v3/files");
    }

    #[test]
    fn response_json_and_status() {
        let ok = Response {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: br#"{"id":"abc"}"#.to_vec(),
        };
        assert!(ok.is_success());
        let v: serde_json::Value = ok.json().unwrap();
        assert_eq!(v["id"], "abc");
        assert_eq!(ok.header_value("content-type"), Some("application/json"));

        let bad = Response {
            status: 500,
            headers: vec![],
            body: b"not json".to_vec(),
        };
        assert!(!bad.is_success());
        assert!(matches!(
            bad.json::<serde_json::Value>(),
            Err(GdocsError::Invalid(_))
        ));
        assert_eq!(bad.text(), "not json");
    }
}
