// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The real transport: `reqwest::blocking` behind [`HttpClient`].
//!
//! Blocking on purpose — the tools layer is synchronous. Callers inside an
//! async runtime (the MCP server, Tauri commands) must go through
//! [`crate::run_blocking`], because a blocking reqwest client panics when it
//! is created on a tokio worker thread.

use std::time::Duration;

use crate::http::{HttpClient, Method, Request, Response};
use crate::{GdocsError, Result};

pub struct ReqwestHttp {
    client: reqwest::blocking::Client,
}

impl ReqwestHttp {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("scrybe-gdocs/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| GdocsError::Http(format!("build HTTP client: {e}")))?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestHttp {
    fn send(&self, req: Request) -> Result<Response> {
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };
        let label = format!("{} {}", req.method.as_str(), req.url);
        let mut builder = self.client.request(method, &req.url);
        for (k, v) in &req.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        if !req.body.is_empty() {
            builder = builder.body(req.body);
        }
        let resp = builder
            .send()
            .map_err(|e| GdocsError::Http(format!("{label}: {e}")))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = resp
            .bytes()
            .map_err(|e| GdocsError::Http(format!("{label}: read body: {e}")))?
            .to_vec();
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}
