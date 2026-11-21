// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The loopback redirect receiver for the installed-app flow: a tiny HTTP
//! listener on `127.0.0.1` that waits for Google to send the browser back
//! with `?code=...&state=...`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::{GdocsError, Result};

const ACCEPT_POLL: Duration = Duration::from_millis(25);
/// A browser that connects but never sends a request must not hold us up.
const MAX_READ_WAIT: Duration = Duration::from_secs(5);
const MAX_REQUEST_BYTES: u64 = 16 * 1024;
const MAX_HEADER_LINES: usize = 100;

const SUCCESS_HTML: &str =
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>Scrybe</title></head>\
<body style=\"font-family:system-ui,sans-serif;margin:3em\">\
<h1>Signed in to Scrybe — you can close this tab.</h1></body></html>";
const FAILED_HTML: &str =
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>Scrybe</title></head>\
<body style=\"font-family:system-ui,sans-serif;margin:3em\">\
<h1>Sign-in failed</h1><p>Scrybe did not receive a Google authorization code. \
Go back to the terminal or app and try again.</p></body></html>";
const NOT_FOUND_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"></head>\
<body>Not found</body></html>";

/// The pieces of Google's redirect query that matter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedRedirect {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Decode `code`, `state` and `error` from a redirect query string.
pub fn parse_redirect_query(query: &str) -> ParsedRedirect {
    let mut parsed = ParsedRedirect::default();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match &*key {
            "code" => &mut parsed.code,
            "state" => &mut parsed.state,
            "error" => &mut parsed.error,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(value.into_owned());
        }
    }
    parsed
}

/// A one-shot listener bound to an ephemeral loopback port.
pub struct Listener {
    inner: TcpListener,
    port: u16,
}

impl Listener {
    pub fn bind() -> Result<Self> {
        let inner = TcpListener::bind(("127.0.0.1", 0))?;
        let port = inner.local_addr()?.port();
        Ok(Self { inner, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// `http://127.0.0.1:<port>/` — what the authorization request names.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    /// Serve connections until the redirect arrives or `timeout` elapses.
    ///
    /// Only a request carrying the expected `state` is Google's redirect;
    /// everything else (a favicon probe, a stray tab, any local process
    /// poking the port) gets a 404 and is ignored, so it cannot abort the
    /// pending login. A state-matching redirect that carries `error=` or
    /// lacks a code answers with a failure page and returns `AuthFailed`.
    ///
    /// Each connection is served on its own thread: browsers open
    /// speculative connections that never send a request, and those must
    /// not delay the one carrying the code.
    pub fn wait_for_code(self, expected_state: &str, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        self.inner.set_nonblocking(true)?;
        let (tx, rx) = mpsc::channel::<Outcome>();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(GdocsError::AuthFailed(
                    "timed out waiting for Google consent".into(),
                ));
            }
            match self.inner.accept() {
                Ok((stream, _)) => {
                    let tx = tx.clone();
                    let expected = expected_state.to_string();
                    std::thread::spawn(move || {
                        // The receiver is gone once we have an answer; nothing to report.
                        let _ = tx.send(handle_connection(&stream, &expected));
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
            match rx.recv_timeout(ACCEPT_POLL.min(remaining)) {
                Ok(Outcome::Code(code)) => return Ok(code),
                Ok(Outcome::Failed(err)) => return Err(err),
                Ok(Outcome::Ignored)
                | Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
            }
        }
    }
}

enum Outcome {
    Code(String),
    Failed(GdocsError),
    Ignored,
}

fn handle_connection(stream: &TcpStream, expected_state: &str) -> Outcome {
    // Accepted sockets inherit the listener's non-blocking flag on some
    // platforms (macOS); make reads blocking-with-timeout instead.
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(MAX_READ_WAIT)).is_err()
    {
        return Outcome::Ignored;
    }
    let Some(request_line) = read_request_line(stream) else {
        return Outcome::Ignored;
    };
    let (outcome, status, body) = classify_request(&request_line, expected_state);
    write_response(stream, status, body);
    outcome
}

/// The request line, after draining the headers so the browser sees a
/// clean response rather than a reset.
fn read_request_line(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream.take(MAX_REQUEST_BYTES));
    let mut first = String::new();
    if reader.read_line(&mut first).ok()? == 0 {
        return None;
    }
    let mut line = String::new();
    for _ in 0..MAX_HEADER_LINES {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(n) if n > 0 && !line.trim().is_empty() => {}
            _ => break,
        }
    }
    Some(first)
}

fn classify_request(request_line: &str, expected_state: &str) -> (Outcome, u16, &'static str) {
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return (Outcome::Ignored, 404, NOT_FOUND_HTML);
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if method != "GET" || path != "/" {
        return (Outcome::Ignored, 404, NOT_FOUND_HTML);
    }
    let redirect = parse_redirect_query(query);
    // Without our state this is not Google's redirect, whatever else it
    // carries; acting on it would let any local request end the login.
    if redirect.state.as_deref() != Some(expected_state) {
        return (Outcome::Ignored, 404, NOT_FOUND_HTML);
    }
    let failed = |msg: String| {
        (
            Outcome::Failed(GdocsError::AuthFailed(msg)),
            400,
            FAILED_HTML,
        )
    };
    if let Some(error) = redirect.error {
        return failed(format!("Google sign-in failed: {}", sanitize_error(&error)));
    }
    match redirect.code {
        Some(code) if !code.is_empty() => (Outcome::Code(code), 200, SUCCESS_HTML),
        _ => failed("Google redirect carried no authorization code".into()),
    }
}

/// Google's `error` values come from a fixed snake_case set
/// (`access_denied`, `invalid_request`, ...); the message reaches the
/// terminal and logs, so anything else is dropped rather than echoed.
fn sanitize_error(error: &str) -> String {
    error
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(64)
        .collect()
}

fn write_response(mut stream: &TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    // The browser may already be gone; nothing useful to do about it.
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .and_then(|()| stream.flush());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::{self, JoinHandle};

    /// A fake browser: one raw HTTP GET per target, returning each response.
    fn fake_browser(port: u16, targets: Vec<String>) -> JoinHandle<Vec<String>> {
        thread::spawn(move || {
            targets
                .into_iter()
                .map(|target| {
                    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                    let req = format!(
                        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
                         User-Agent: fake\r\nConnection: close\r\n\r\n"
                    );
                    s.write_all(req.as_bytes()).expect("write");
                    let mut out = String::new();
                    s.read_to_string(&mut out).expect("read");
                    out
                })
                .collect()
        })
    }

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn parses_redirect_query() {
        let p = parse_redirect_query("code=4%2FabC&scope=x&state=s1");
        assert_eq!(p.code.as_deref(), Some("4/abC"));
        assert_eq!(p.state.as_deref(), Some("s1"));
        assert_eq!(p.error, None);
        let p = parse_redirect_query("error=access_denied&state=s1");
        assert_eq!(p.error.as_deref(), Some("access_denied"));
        assert_eq!(p.code, None);
        assert_eq!(parse_redirect_query(""), ParsedRedirect::default());
    }

    #[test]
    fn redirect_uri_names_the_port() {
        let l = Listener::bind().unwrap();
        assert_eq!(l.redirect_uri(), format!("http://127.0.0.1:{}/", l.port()));
        assert_ne!(l.port(), 0);
    }

    #[test]
    fn success_returns_code_and_serves_signed_in_page() {
        let l = Listener::bind().unwrap();
        let browser = fake_browser(l.port(), vec!["/?code=the-code&state=st&scope=d".into()]);
        let code = l.wait_for_code("st", WAIT).unwrap();
        assert_eq!(code, "the-code");
        let pages = browser.join().unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 200 OK\r\n"), "{}", pages[0]);
        assert!(pages[0].contains("Signed in to Scrybe"), "{}", pages[0]);
    }

    #[test]
    fn state_mismatch_is_ignored_then_success() {
        let l = Listener::bind().unwrap();
        let browser = fake_browser(
            l.port(),
            vec![
                "/?code=c&state=wrong".into(),
                "/?code=right&state=st".into(),
            ],
        );
        let code = l.wait_for_code("st", WAIT).unwrap();
        assert_eq!(code, "right");
        let pages = browser.join().unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 404"), "{}", pages[0]);
        assert!(pages[1].starts_with("HTTP/1.1 200"), "{}", pages[1]);
    }

    #[test]
    fn error_without_matching_state_is_ignored() {
        let l = Listener::bind().unwrap();
        let browser = fake_browser(
            l.port(),
            vec![
                "/?error=pwned".into(),
                "/?error=pwned&state=wrong".into(),
                "/?code=ok&state=st".into(),
            ],
        );
        let code = l.wait_for_code("st", WAIT).unwrap();
        assert_eq!(code, "ok");
        let pages = browser.join().unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 404"), "{}", pages[0]);
        assert!(pages[1].starts_with("HTTP/1.1 404"), "{}", pages[1]);
        assert!(pages[2].starts_with("HTTP/1.1 200"), "{}", pages[2]);
    }

    #[test]
    fn access_denied_is_auth_failed() {
        let l = Listener::bind().unwrap();
        let browser = fake_browser(l.port(), vec!["/?error=access_denied&state=st".into()]);
        let err = l.wait_for_code("st", WAIT).unwrap_err();
        assert_eq!(err.code(), "auth_failed");
        assert!(err.to_string().contains("access_denied"), "{err}");
        let pages = browser.join().unwrap();
        assert!(pages[0].contains("Sign-in failed"), "{}", pages[0]);
    }

    #[test]
    fn favicon_is_ignored_then_success() {
        let l = Listener::bind().unwrap();
        let browser = fake_browser(
            l.port(),
            vec![
                "/favicon.ico".into(),
                "/".into(),
                "/?code=later&state=st".into(),
            ],
        );
        let code = l.wait_for_code("st", WAIT).unwrap();
        assert_eq!(code, "later");
        let pages = browser.join().unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 404"), "{}", pages[0]);
        assert!(pages[1].starts_with("HTTP/1.1 404"), "{}", pages[1]);
        assert!(pages[2].starts_with("HTTP/1.1 200"), "{}", pages[2]);
    }

    #[test]
    fn stalled_client_does_not_block_the_real_redirect() {
        let l = Listener::bind().unwrap();
        let port = l.port();
        // Connects first and never sends a request, like a browser preconnect.
        let stalled = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let browser = fake_browser(port, vec!["/?code=after-stall&state=st".into()]);
        let started = Instant::now();
        let code = l.wait_for_code("st", Duration::from_secs(3)).unwrap();
        assert_eq!(code, "after-stall");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        drop(stalled);
        browser.join().unwrap();
    }

    #[test]
    fn times_out_without_a_redirect() {
        let l = Listener::bind().unwrap();
        let started = Instant::now();
        let err = l
            .wait_for_code("st", Duration::from_millis(150))
            .unwrap_err();
        assert_eq!(err.code(), "auth_failed");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn classify_covers_edge_cases() {
        let (o, status, _) = classify_request("POST /?code=c&state=s HTTP/1.1", "s");
        assert!(matches!(o, Outcome::Ignored));
        assert_eq!(status, 404);
        let (o, status, _) = classify_request("GET /?state=s HTTP/1.1", "s");
        assert!(matches!(o, Outcome::Failed(GdocsError::AuthFailed(_))));
        assert_eq!(status, 400);
        let (o, _, _) = classify_request("garbage", "s");
        assert!(matches!(o, Outcome::Ignored));
        for probe in [
            "GET /?error=x HTTP/1.1",
            "GET /?code=c HTTP/1.1",
            "GET /?code=c&state=wrong HTTP/1.1",
            "GET /?error=x&state=wrong HTTP/1.1",
        ] {
            let (o, status, _) = classify_request(probe, "s");
            assert!(matches!(o, Outcome::Ignored), "{probe}");
            assert_eq!(status, 404, "{probe}");
        }
    }

    #[test]
    fn echoed_error_is_sanitized() {
        let (o, status, _) = classify_request("GET /?error=a%0A%0Db%3Cs%3E&state=s HTTP/1.1", "s");
        assert_eq!(status, 400);
        let Outcome::Failed(err) = o else {
            panic!("expected Failed");
        };
        let msg = err.to_string();
        assert!(msg.contains("ab"), "{msg}");
        assert!(!msg.contains(['\n', '\r', '<', '>']), "{msg:?}");
        assert_eq!(sanitize_error("access_denied"), "access_denied");
        assert_eq!(sanitize_error(&"x".repeat(100)).len(), 64);
    }
}
