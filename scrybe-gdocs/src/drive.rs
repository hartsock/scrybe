// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The handful of Drive v3 calls Scrybe needs: find-or-create the Scrybe
//! folder, upload HTML as a native Google Doc, export a Doc back out, and
//! list the Docs Scrybe can see. Generic over [`HttpClient`] and
//! [`TokenProvider`] so every call runs offline in tests.

use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

use crate::http::{HttpClient, Method, Request, Response};
use crate::{DocSummary, Endpoints, GdocsError, Result, TokenProvider, SCRYBE_FOLDER_NAME};

/// Metadata fields requested on every create / update / get.
pub const FIELDS: &str = "id,name,webViewLink,trashed,mimeType,modifiedTime";
pub const DOC_MIME: &str = "application/vnd.google-apps.document";
pub const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

const LIST_FIELDS: &str = "nextPageToken,files(id,name,webViewLink,modifiedTime)";
const HTML_MEDIA_TYPE: &str = "text/html; charset=UTF-8";
/// How much of a non-JSON error body is kept in the error message.
const ERROR_PREVIEW_CHARS: usize = 300;

/// The subset of a Drive `File` resource Scrybe reads.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub web_view_link: Option<String>,
    #[serde(default)]
    pub trashed: bool,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub modified_time: Option<String>,
}

impl DriveFile {
    /// `webViewLink` when Drive returned one, else the canonical editor URL.
    pub fn url(&self) -> String {
        self.web_view_link
            .clone()
            .unwrap_or_else(|| doc_url(&self.id))
    }
}

impl From<DriveFile> for DocSummary {
    fn from(file: DriveFile) -> Self {
        let url = file.url();
        Self {
            id: file.id,
            name: file.name,
            url,
            modified_time: file.modified_time,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdOnly {
    id: String,
}

/// The editor URL for a Google Doc id.
pub fn doc_url(id: &str) -> String {
    format!("https://docs.google.com/document/d/{id}/edit")
}

/// Drive v3 over any transport, authenticating with any token source.
pub struct DriveClient<'a, H: HttpClient, T: TokenProvider> {
    http: &'a H,
    endpoints: &'a Endpoints,
    token: &'a T,
}

impl<'a, H: HttpClient, T: TokenProvider> DriveClient<'a, H, T> {
    pub fn new(http: &'a H, endpoints: &'a Endpoints, token: &'a T) -> Self {
        Self {
            http,
            endpoints,
            token,
        }
    }

    /// Id of the `Scrybe` folder, found by name wherever the user has moved
    /// it (under `drive.file` only folders Scrybe created are visible), and
    /// created in My Drive root only when none exists. Should two exist,
    /// the oldest wins, deterministically.
    pub fn ensure_scrybe_folder(&self) -> Result<String> {
        let q = format!(
            "name = '{SCRYBE_FOLDER_NAME}' and mimeType = '{FOLDER_MIME}' and trashed = false"
        );
        let mut url = files_url(&self.endpoints.drive, &[])?;
        with_all_drives(&mut url, true);
        url.query_pairs_mut()
            .append_pair("q", &q)
            .append_pair("fields", "files(id,name)")
            .append_pair("spaces", "drive")
            .append_pair("orderBy", "createdTime")
            .append_pair("pageSize", "1");
        let found: FileList = self
            .call(None, |token| Request::get(url.as_str()).bearer(token))?
            .json()?;
        if let Some(folder) = found.files.into_iter().next() {
            return Ok(folder.id);
        }

        let mut url = files_url(&self.endpoints.drive, &[])?;
        with_all_drives(&mut url, false);
        url.query_pairs_mut().append_pair("fields", "id");
        let metadata = json!({
            "name": SCRYBE_FOLDER_NAME,
            "mimeType": FOLDER_MIME,
            "parents": ["root"],
        });
        let created: IdOnly = self
            .call(None, |token| {
                Request::post(url.as_str()).bearer(token).json(&metadata)
            })?
            .json()?;
        Ok(created.id)
    }

    /// Create a native Google Doc from HTML inside `parent_id`.
    pub fn create_doc_from_html(
        &self,
        name: &str,
        html: &str,
        parent_id: &str,
    ) -> Result<DriveFile> {
        let mut url = files_url(&self.endpoints.upload, &[])?;
        with_all_drives(&mut url, false);
        url.query_pairs_mut()
            .append_pair("uploadType", "multipart")
            .append_pair("fields", FIELDS);
        let metadata = json!({
            "name": name,
            "mimeType": DOC_MIME,
            "parents": [parent_id],
        });
        self.upload(None, Method::Post, &url, &metadata, html)
    }

    /// Replace an existing Doc's content from HTML, and its name when one is
    /// given. Drive treats every metadata field sent as an explicit write,
    /// so `None` is the only way to leave a name the user set in Drive alone.
    pub fn update_doc_from_html(
        &self,
        id: &str,
        name: Option<&str>,
        html: &str,
    ) -> Result<DriveFile> {
        let mut url = files_url(&self.endpoints.upload, &[id])?;
        with_all_drives(&mut url, false);
        url.query_pairs_mut()
            .append_pair("uploadType", "multipart")
            .append_pair("fields", FIELDS);
        let metadata = match name {
            Some(name) => json!({ "name": name }),
            None => json!({}),
        };
        self.upload(Some(id), Method::Patch, &url, &metadata, html)
    }

    pub fn get_file(&self, id: &str) -> Result<DriveFile> {
        let mut url = files_url(&self.endpoints.drive, &[id])?;
        with_all_drives(&mut url, false);
        url.query_pairs_mut().append_pair("fields", FIELDS);
        self.call(Some(id), |token| Request::get(url.as_str()).bearer(token))?
            .json()
    }

    pub fn export_markdown(&self, id: &str) -> Result<String> {
        self.export(id, "text/markdown")
    }

    pub fn export_html(&self, id: &str) -> Result<String> {
        self.export(id, "text/html")
    }

    /// Every non-trashed Google Doc visible under `drive.file`, newest first.
    pub fn list_app_docs(&self) -> Result<Vec<DocSummary>> {
        let q = format!("mimeType = '{DOC_MIME}' and trashed = false");
        let mut docs = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = files_url(&self.endpoints.drive, &[])?;
            with_all_drives(&mut url, true);
            {
                let mut pairs = url.query_pairs_mut();
                pairs
                    .append_pair("q", &q)
                    .append_pair("fields", LIST_FIELDS)
                    .append_pair("orderBy", "modifiedTime desc")
                    .append_pair("pageSize", "100")
                    .append_pair("spaces", "drive");
                if let Some(token) = &page_token {
                    pairs.append_pair("pageToken", token);
                }
            }
            let page: FileList = self
                .call(None, |token| Request::get(url.as_str()).bearer(token))?
                .json()?;
            docs.extend(page.files.into_iter().map(DocSummary::from));
            match page.next_page_token.filter(|t| !t.is_empty()) {
                Some(next) => page_token = Some(next),
                None => return Ok(docs),
            }
        }
    }

    fn export(&self, id: &str, mime_type: &str) -> Result<String> {
        let mut url = files_url(&self.endpoints.drive, &[id, "export"])?;
        url.query_pairs_mut().append_pair("mimeType", mime_type);
        let resp = self.call(Some(id), |token| Request::get(url.as_str()).bearer(token))?;
        Ok(resp.text())
    }

    fn upload(
        &self,
        doc_id: Option<&str>,
        method: Method,
        url: &Url,
        metadata: &Value,
        html: &str,
    ) -> Result<DriveFile> {
        let boundary = uuid::Uuid::new_v4().simple().to_string();
        let body = multipart_related(
            &boundary,
            &metadata.to_string(),
            HTML_MEDIA_TYPE,
            html.as_bytes(),
        );
        let content_type = format!("multipart/related; boundary={boundary}");
        self.call(doc_id, |token| {
            Request::new(method, url.as_str())
                .bearer(token)
                .header("Content-Type", content_type.as_str())
                .body(body.clone())
        })?
        .json()
    }

    /// Send with the current token; on 401 refresh once and resend. Then map
    /// the status: 2xx → `Ok`, 404 → `DocNotFound` (for `doc_id`), a second
    /// 401 → `NotLoggedIn`, anything else → `Api`.
    fn call(&self, doc_id: Option<&str>, build: impl Fn(&str) -> Request) -> Result<Response> {
        let token = self.token.access_token()?;
        let mut resp = self.http.send(build(&token))?;
        if resp.status == 401 {
            let fresh = self.token.refresh()?;
            resp = self.http.send(build(&fresh))?;
        }
        if resp.is_success() {
            return Ok(resp);
        }
        Err(match resp.status {
            401 => GdocsError::NotLoggedIn(format!(
                "Google rejected the access token even after refreshing it ({}); sign in again",
                api_message(&resp)
            )),
            404 => GdocsError::DocNotFound(match doc_id {
                Some(id) => format!(
                    "Google Doc {id} not found — it may be deleted, or not created by Scrybe \
                     (the drive.file scope only sees Scrybe's own files)"
                ),
                None => format!("Google Drive returned 404: {}", api_message(&resp)),
            }),
            status => GdocsError::Api {
                status,
                message: api_message(&resp),
            },
        })
    }
}

/// Drive hides shared-drive items from requests that do not opt in (files.get
/// and files.update 404, files.list omits them), so a doc the user moved into
/// a shared drive would look deleted. `listing` adds the files.list twin.
fn with_all_drives(url: &mut Url, listing: bool) {
    let mut pairs = url.query_pairs_mut();
    pairs.append_pair("supportsAllDrives", "true");
    if listing {
        pairs.append_pair("includeItemsFromAllDrives", "true");
    }
}

/// `<base>/files[/<segment>...]`, with each segment percent-encoded.
fn files_url(base: &str, segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse(base)
        .map_err(|e| GdocsError::Invalid(format!("bad Drive endpoint {base:?}: {e}")))?;
    url.path_segments_mut()
        .map_err(|()| GdocsError::Invalid(format!("Drive endpoint {base:?} cannot carry a path")))?
        .pop_if_empty()
        .push("files")
        .extend(segments);
    Ok(url)
}

/// Google's `error.message` when the body is its JSON error envelope, else a
/// trimmed preview of the raw body.
fn api_message(resp: &Response) -> String {
    let text = resp.text();
    let google_message = serde_json::from_str::<Value>(&text).ok().and_then(|v| {
        v.pointer("/error/message")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    if let Some(message) = google_message {
        return message;
    }
    let preview: String = text.trim().chars().take(ERROR_PREVIEW_CHARS).collect();
    if preview.is_empty() {
        "empty response body".to_string()
    } else {
        preview
    }
}

/// RFC 2387 `multipart/related` body: a JSON metadata part then the media.
pub(crate) fn multipart_related(
    boundary: &str,
    metadata_json: &str,
    media_type: &str,
    media: &[u8],
) -> Vec<u8> {
    let head = format!(
        "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata_json}\r\n\
         --{boundary}\r\nContent-Type: {media_type}\r\n\r\n"
    );
    let tail = format!("\r\n--{boundary}--\r\n");
    let mut body = Vec::with_capacity(head.len() + media.len() + tail.len());
    body.extend_from_slice(head.as_bytes());
    body.extend_from_slice(media);
    body.extend_from_slice(tail.as_bytes());
    body
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::testing::{FakeHttp, StaticToken};

    const HTML: &str = "<h1>Notes</h1>\n<p>hello &amp; goodbye</p>\n";

    fn endpoints() -> Endpoints {
        Endpoints {
            drive: "https://fake.test/drive/v3".into(),
            upload: "https://fake.test/upload/drive/v3".into(),
            ..Endpoints::default()
        }
    }

    fn file_json(id: &str, name: &str) -> Value {
        json!({
            "id": id,
            "name": name,
            "webViewLink": doc_url(id),
            "mimeType": DOC_MIME,
            "modifiedTime": "2026-10-16T12:00:00.000Z",
        })
    }

    fn google_error(code: u16, message: &str) -> Value {
        json!({ "error": { "code": code, "message": message } })
    }

    /// Always hands out `stale`; `refresh` counts calls and hands out `fresh`
    /// (or fails, when asked to).
    struct CountingToken {
        refreshes: AtomicUsize,
        refresh_fails: bool,
    }

    impl CountingToken {
        fn new(refresh_fails: bool) -> Self {
            Self {
                refreshes: AtomicUsize::new(0),
                refresh_fails,
            }
        }

        fn refreshes(&self) -> usize {
            self.refreshes.load(Ordering::SeqCst)
        }
    }

    impl TokenProvider for CountingToken {
        fn access_token(&self) -> Result<String> {
            Ok("stale".to_string())
        }

        fn refresh(&self) -> Result<String> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            if self.refresh_fails {
                Err(GdocsError::NotLoggedIn("refresh token revoked".into()))
            } else {
                Ok("fresh".to_string())
            }
        }
    }

    struct NoToken;

    impl TokenProvider for NoToken {
        fn access_token(&self) -> Result<String> {
            Err(GdocsError::NotLoggedIn("not signed in".into()))
        }

        fn refresh(&self) -> Result<String> {
            Err(GdocsError::NotLoggedIn("not signed in".into()))
        }
    }

    fn boundary_of(req: &Request) -> String {
        let content_type = req
            .header_value("content-type")
            .expect("Content-Type header");
        let (kind, boundary) = content_type
            .split_once("; boundary=")
            .expect("multipart/related with a boundary");
        assert_eq!(kind, "multipart/related");
        assert!(!boundary.is_empty());
        boundary.to_string()
    }

    /// `(headers, content)` of each part, asserting the RFC 2387 framing.
    fn multipart_parts(body: &[u8], boundary: &str) -> Vec<(String, String)> {
        let text = std::str::from_utf8(body).expect("UTF-8 body");
        let delimiter = format!("--{boundary}");
        let mut segments = text.split(delimiter.as_str());
        assert_eq!(segments.next(), Some(""), "body starts with the boundary");
        let mut parts = Vec::new();
        let mut closed = false;
        for segment in segments {
            assert!(!closed, "nothing may follow the closing boundary");
            if segment == "--\r\n" {
                closed = true;
                continue;
            }
            let segment = segment.strip_prefix("\r\n").expect("CRLF after boundary");
            let (headers, content) = segment
                .split_once("\r\n\r\n")
                .expect("blank line after part headers");
            let content = content
                .strip_suffix("\r\n")
                .expect("CRLF before the next boundary");
            parts.push((headers.to_string(), content.to_string()));
        }
        assert!(closed, "closing boundary present");
        parts
    }

    const FOLDER_QUERY: &str =
        "name = 'Scrybe' and mimeType = 'application/vnd.google-apps.folder' and trashed = false";

    fn assert_supports_all_drives(req: &Request) {
        assert_eq!(
            req.query_param("supportsAllDrives").as_deref(),
            Some("true"),
            "{}",
            req.url
        );
    }

    fn assert_lists_all_drives(req: &Request) {
        assert_supports_all_drives(req);
        assert_eq!(
            req.query_param("includeItemsFromAllDrives").as_deref(),
            Some("true"),
            "{}",
            req.url
        );
    }

    #[test]
    fn ensure_scrybe_folder_returns_the_existing_folder() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files").json(
            200,
            json!({ "files": [{ "id": "folder1", "name": "Scrybe" }] }),
        );
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        assert_eq!(client.ensure_scrybe_folder().unwrap(), "folder1");

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.path(), "/drive/v3/files");
        assert_eq!(req.header_value("authorization"), Some("Bearer tok"));
        assert_eq!(req.query_param("q").as_deref(), Some(FOLDER_QUERY));
        assert_eq!(req.query_param("fields").as_deref(), Some("files(id,name)"));
        assert_eq!(req.query_param("spaces").as_deref(), Some("drive"));
        assert_eq!(req.query_param("orderBy").as_deref(), Some("createdTime"));
        assert_eq!(req.query_param("pageSize").as_deref(), Some("1"));
        assert_lists_all_drives(req);
        assert!(
            req.url.contains("q=name+%3D+%27Scrybe%27"),
            "q is percent-encoded: {}",
            req.url
        );
    }

    #[test]
    fn ensure_scrybe_folder_creates_when_missing() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files")
            .json(200, json!({ "files": [] }));
        fake.on_url(Method::Post, "/drive/v3/files")
            .json(200, json!({ "id": "folder2" }));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        assert_eq!(client.ensure_scrybe_folder().unwrap(), "folder2");

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].query_param("q").as_deref(), Some(FOLDER_QUERY));
        assert_eq!(
            reqs[0].query_param("orderBy").as_deref(),
            Some("createdTime")
        );
        let create = &reqs[1];
        assert_eq!(create.method, Method::Post);
        assert_eq!(create.path(), "/drive/v3/files");
        assert_eq!(create.query_param("fields").as_deref(), Some("id"));
        assert_eq!(create.query_param("q"), None);
        assert_supports_all_drives(create);
        assert_eq!(
            create.header_value("content-type"),
            Some("application/json")
        );
        assert_eq!(create.header_value("authorization"), Some("Bearer tok"));
        let body: Value = serde_json::from_slice(&create.body).unwrap();
        assert_eq!(
            body,
            json!({ "name": "Scrybe", "mimeType": FOLDER_MIME, "parents": ["root"] })
        );
    }

    #[test]
    fn create_doc_sends_multipart_related_upload() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Post, "/upload/drive/v3/files")
            .json(200, file_json("doc1", "Notes"));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        let file = client
            .create_doc_from_html("Notes", HTML, "folder1")
            .unwrap();
        assert_eq!(file.id, "doc1");
        assert_eq!(file.name, "Notes");
        assert_eq!(
            file.web_view_link.as_deref(),
            Some(doc_url("doc1").as_str())
        );
        assert_eq!(file.mime_type.as_deref(), Some(DOC_MIME));

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.path(), "/upload/drive/v3/files");
        assert_eq!(req.query_param("uploadType").as_deref(), Some("multipart"));
        assert_eq!(req.query_param("fields").as_deref(), Some(FIELDS));
        assert_eq!(req.header_value("authorization"), Some("Bearer tok"));
        assert_supports_all_drives(req);

        let boundary = boundary_of(req);
        let parts = multipart_parts(&req.body, &boundary);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, "Content-Type: application/json; charset=UTF-8");
        let metadata: Value = serde_json::from_str(&parts[0].1).unwrap();
        assert_eq!(
            metadata,
            json!({ "name": "Notes", "mimeType": DOC_MIME, "parents": ["folder1"] })
        );
        assert_eq!(parts[1].0, "Content-Type: text/html; charset=UTF-8");
        assert_eq!(parts[1].1, HTML);

        let expected_metadata =
            json!({ "name": "Notes", "mimeType": DOC_MIME, "parents": ["folder1"] }).to_string();
        assert_eq!(
            req.body,
            multipart_related(
                &boundary,
                &expected_metadata,
                HTML_MEDIA_TYPE,
                HTML.as_bytes()
            )
        );
    }

    #[test]
    fn update_doc_patches_without_parents_or_mime_type() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Patch, "/upload/drive/v3/files/doc1")
            .json(200, file_json("doc1", "Renamed"));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        let file = client
            .update_doc_from_html("doc1", Some("Renamed"), HTML)
            .unwrap();
        assert_eq!(file.name, "Renamed");

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 1);
        let req = &reqs[0];
        assert_eq!(req.method, Method::Patch);
        assert_eq!(req.path(), "/upload/drive/v3/files/doc1");
        assert_eq!(req.query_param("uploadType").as_deref(), Some("multipart"));
        assert_eq!(req.query_param("fields").as_deref(), Some(FIELDS));
        assert_supports_all_drives(req);

        let parts = multipart_parts(&req.body, &boundary_of(req));
        assert_eq!(parts.len(), 2);
        let metadata: Value = serde_json::from_str(&parts[0].1).unwrap();
        assert_eq!(metadata, json!({ "name": "Renamed" }));
        assert!(metadata.get("parents").is_none());
        assert!(metadata.get("mimeType").is_none());
        assert_eq!(parts[1].0, "Content-Type: text/html; charset=UTF-8");
        assert_eq!(parts[1].1, HTML);

        // Without a name only the content is replaced.
        client.update_doc_from_html("doc1", None, HTML).unwrap();
        let req = &fake.requests()[1];
        let parts = multipart_parts(&req.body, &boundary_of(req));
        let metadata: Value = serde_json::from_str(&parts[0].1).unwrap();
        assert_eq!(metadata, json!({}));
        assert_eq!(parts[1].1, HTML);
    }

    #[test]
    fn get_file_requests_fields_and_deserializes() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files/doc1")
            .json(200, file_json("doc1", "Notes"));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        let file = client.get_file("doc1").unwrap();
        assert_eq!(file.id, "doc1");
        assert!(!file.trashed);
        assert_eq!(
            file.modified_time.as_deref(),
            Some("2026-10-16T12:00:00.000Z")
        );

        let req = &fake.requests()[0];
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.path(), "/drive/v3/files/doc1");
        assert_eq!(req.query_param("fields").as_deref(), Some(FIELDS));
        assert_supports_all_drives(req);
    }

    #[test]
    fn get_file_maps_404_to_doc_not_found() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files/gone")
            .json(404, google_error(404, "File not found: gone."));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        let err = client.get_file("gone").unwrap_err();
        assert_eq!(err.code(), "doc_not_found");
        match err {
            GdocsError::DocNotFound(message) => {
                assert!(
                    message.starts_with("Google Doc gone not found"),
                    "{message}"
                );
                assert!(message.contains("drive.file"), "{message}");
            }
            other => panic!("expected DocNotFound, got {other:?}"),
        }
    }

    #[test]
    fn get_file_maps_other_statuses_to_api_with_google_message() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files/doc1")
            .once()
            .json(500, google_error(500, "Backend Error"));
        let long_body = "x".repeat(400);
        fake.on_url(Method::Get, "/drive/v3/files/doc1")
            .once()
            .text(503, &long_body);
        fake.on_url(Method::Get, "/drive/v3/files/doc1")
            .once()
            .text(502, "   ");
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        let err = client.get_file("doc1").unwrap_err();
        assert_eq!(err.code(), "api_error");
        assert!(
            matches!(&err, GdocsError::Api { status: 500, message } if message == "Backend Error"),
            "{err:?}"
        );

        match client.get_file("doc1").unwrap_err() {
            GdocsError::Api { status, message } => {
                assert_eq!(status, 503);
                assert_eq!(message.chars().count(), ERROR_PREVIEW_CHARS);
                assert!(message.chars().all(|c| c == 'x'));
            }
            other => panic!("expected Api, got {other:?}"),
        }

        match client.get_file("doc1").unwrap_err() {
            GdocsError::Api { status, message } => {
                assert_eq!(status, 502);
                assert_eq!(message, "empty response body");
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn a_401_refreshes_once_and_retries_with_the_new_token() {
        let fake = FakeHttp::new();
        fake.on(|r| r.header_value("authorization") == Some("Bearer stale"))
            .json(401, google_error(401, "Invalid Credentials"));
        fake.on(|r| r.header_value("authorization") == Some("Bearer fresh"))
            .json(200, file_json("doc1", "Notes"));
        let ep = endpoints();
        let tok = CountingToken::new(false);
        let client = DriveClient::new(&fake, &ep, &tok);

        let file = client.get_file("doc1").unwrap();
        assert_eq!(file.id, "doc1");
        assert_eq!(tok.refreshes(), 1);

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].header_value("authorization"), Some("Bearer stale"));
        assert_eq!(reqs[1].header_value("authorization"), Some("Bearer fresh"));
        assert_eq!(reqs[0].url, reqs[1].url);
        assert_eq!(reqs[0].method, reqs[1].method);
    }

    /// Two 401s in a row mean the refreshed credentials are bad too, so the
    /// caller is told to sign in again rather than seeing a generic API error.
    #[test]
    fn a_second_401_is_not_logged_in() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files/doc1")
            .json(401, google_error(401, "Invalid Credentials"));
        let ep = endpoints();
        let tok = CountingToken::new(false);
        let client = DriveClient::new(&fake, &ep, &tok);

        let err = client.get_file("doc1").unwrap_err();
        assert_eq!(err.code(), "not_logged_in");
        assert!(err.to_string().contains("Invalid Credentials"), "{err}");
        assert_eq!(tok.refreshes(), 1);
        assert_eq!(fake.request_count(), 2);
    }

    #[test]
    fn a_failed_refresh_propagates_without_a_retry() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files/doc1")
            .json(401, google_error(401, "Invalid Credentials"));
        let ep = endpoints();
        let tok = CountingToken::new(true);
        let client = DriveClient::new(&fake, &ep, &tok);

        let err = client.get_file("doc1").unwrap_err();
        assert!(
            matches!(&err, GdocsError::NotLoggedIn(m) if m == "refresh token revoked"),
            "{err:?}"
        );
        assert_eq!(tok.refreshes(), 1);
        assert_eq!(fake.request_count(), 1);
    }

    #[test]
    fn no_request_is_sent_without_an_access_token() {
        let fake = FakeHttp::new();
        let ep = endpoints();
        let client = DriveClient::new(&fake, &ep, &NoToken);

        let err = client.get_file("doc1").unwrap_err();
        assert_eq!(err.code(), "not_logged_in");
        assert_eq!(fake.request_count(), 0);
    }

    #[test]
    fn export_returns_the_body_for_the_requested_mime_type() {
        let fake = FakeHttp::new();
        fake.on_url(Method::Get, "/drive/v3/files/doc1/export")
            .reply(|r| {
                let mime = r.query_param("mimeType").expect("mimeType query param");
                Response {
                    status: 200,
                    headers: vec![],
                    body: format!("body for {mime}").into_bytes(),
                }
            });
        fake.on_url(Method::Get, "/drive/v3/files/gone/export")
            .json(404, google_error(404, "File not found: gone."));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        assert_eq!(
            client.export_markdown("doc1").unwrap(),
            "body for text/markdown"
        );
        assert_eq!(client.export_html("doc1").unwrap(), "body for text/html");
        assert_eq!(
            client.export_markdown("gone").unwrap_err().code(),
            "doc_not_found"
        );

        let reqs = fake.requests();
        assert_eq!(reqs[0].path(), "/drive/v3/files/doc1/export");
        assert_eq!(reqs[0].header_value("authorization"), Some("Bearer tok"));
        assert_eq!(
            reqs[0].query_param("supportsAllDrives"),
            None,
            "files.export documents only mimeType"
        );
    }

    #[test]
    fn list_app_docs_follows_next_page_token_and_falls_back_to_doc_url() {
        let fake = FakeHttp::new();
        fake.on(|r| r.query_param("pageToken").is_none()).json(
            200,
            json!({
                "nextPageToken": "page2",
                "files": [{
                    "id": "a",
                    "name": "Alpha",
                    "webViewLink": "https://docs.google.com/document/d/a/edit?usp=drivesdk",
                    "modifiedTime": "2026-10-16T12:00:00.000Z",
                }],
            }),
        );
        fake.on(|r| r.query_param("pageToken").as_deref() == Some("page2"))
            .json(200, json!({ "files": [{ "id": "b", "name": "Beta" }] }));
        let ep = endpoints();
        let tok = StaticToken("tok".into());
        let client = DriveClient::new(&fake, &ep, &tok);

        let docs = client.list_app_docs().unwrap();
        assert_eq!(
            docs,
            vec![
                DocSummary {
                    id: "a".into(),
                    name: "Alpha".into(),
                    url: "https://docs.google.com/document/d/a/edit?usp=drivesdk".into(),
                    modified_time: Some("2026-10-16T12:00:00.000Z".into()),
                },
                DocSummary {
                    id: "b".into(),
                    name: "Beta".into(),
                    url: doc_url("b"),
                    modified_time: None,
                },
            ]
        );

        let reqs = fake.requests();
        assert_eq!(reqs.len(), 2);
        let first = &reqs[0];
        assert_eq!(first.method, Method::Get);
        assert_eq!(first.path(), "/drive/v3/files");
        assert_eq!(
            first.query_param("q").as_deref(),
            Some("mimeType = 'application/vnd.google-apps.document' and trashed = false")
        );
        assert_eq!(first.query_param("fields").as_deref(), Some(LIST_FIELDS));
        assert_eq!(
            first.query_param("orderBy").as_deref(),
            Some("modifiedTime desc")
        );
        assert_eq!(first.query_param("pageSize").as_deref(), Some("100"));
        assert_eq!(first.query_param("spaces").as_deref(), Some("drive"));
        assert_eq!(reqs[1].query_param("pageToken").as_deref(), Some("page2"));
        assert_eq!(reqs[1].query_param("q"), first.query_param("q"));
        for req in &reqs {
            assert_lists_all_drives(req);
        }
    }

    #[test]
    fn multipart_related_exact_bytes() {
        let body = multipart_related(
            "abc",
            r#"{"name":"x"}"#,
            "text/html; charset=UTF-8",
            b"<p>hi</p>",
        );
        let expected = b"--abc\r\n\
            Content-Type: application/json; charset=UTF-8\r\n\
            \r\n\
            {\"name\":\"x\"}\r\n\
            --abc\r\n\
            Content-Type: text/html; charset=UTF-8\r\n\
            \r\n\
            <p>hi</p>\r\n\
            --abc--\r\n";
        assert_eq!(body, expected.to_vec());
    }

    #[test]
    fn doc_url_is_the_editor_link() {
        assert_eq!(
            doc_url("1AbC_-x"),
            "https://docs.google.com/document/d/1AbC_-x/edit"
        );
    }

    #[test]
    fn drive_file_deserializes_camel_case_with_defaults() {
        let file: DriveFile = serde_json::from_value(json!({ "id": "x", "name": "n" })).unwrap();
        assert!(file.web_view_link.is_none());
        assert!(!file.trashed);
        assert!(file.mime_type.is_none());
        assert!(file.modified_time.is_none());
        assert_eq!(file.url(), doc_url("x"));

        let file: DriveFile = serde_json::from_value(json!({
            "id": "x",
            "name": "n",
            "trashed": true,
            "webViewLink": "https://w.test/x",
            "mimeType": DOC_MIME,
            "modifiedTime": "2026-01-01T00:00:00Z",
        }))
        .unwrap();
        assert!(file.trashed);
        assert_eq!(file.url(), "https://w.test/x");
        assert_eq!(file.mime_type.as_deref(), Some(DOC_MIME));
        let summary = DocSummary::from(file);
        assert_eq!(summary.url, "https://w.test/x");
        assert_eq!(
            summary.modified_time.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn file_ids_are_percent_encoded_in_the_path() {
        let url = files_url("https://fake.test/drive/v3/", &["a b/c", "export"]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://fake.test/drive/v3/files/a%20b%2Fc/export"
        );
    }
}
