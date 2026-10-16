// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The entry points: auth, list, export and import as methods on [`Gdocs`],
//! plus owned-argument free functions (each [`run_blocking`] over
//! [`Gdocs::real`]) that the tools layer and the app call.

use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};

use crate::drive::DriveClient;
use crate::http::HttpClient;
use crate::sidecar::{self, ManifestFigure, Sidecar};
use crate::token_store::Authenticator;
use crate::types::{AuthStatus, DocSummary, ExportAction, ExportReport, ImportReport};
use crate::{export, import, oauth, run_blocking, Gdocs, GdocsError, Result};

impl<H: HttpClient> Gdocs<H> {
    /// Run the browser consent flow and cache the token under `config_dir`.
    pub fn login(&self) -> Result<AuthStatus> {
        oauth::login(self)
    }

    /// Revoke (best effort) and forget the cached token.
    pub fn logout(&self) -> Result<AuthStatus> {
        oauth::logout(self)
    }

    /// Where sign-in stands, without talking to Google.
    pub fn status(&self) -> Result<AuthStatus> {
        oauth::status(self)
    }

    /// Every Google Doc Scrybe created, newest first.
    pub fn list_docs(&self) -> Result<Vec<DocSummary>> {
        let auth = Authenticator::new(self);
        DriveClient::new(&self.http, &self.endpoints, &auth).list_app_docs()
    }

    /// Export `source` (the Markdown of `path`) to a Google Doc in the Scrybe
    /// folder. The doc named by `path`'s sidecar is updated in place when it
    /// still exists and is not trashed; otherwise a new doc is created. The
    /// sidecar is (re)written either way. `title` overrides the doc name,
    /// which defaults to the first H1 and then to the file stem; on an
    /// update a name the user set in Drive since the last export is kept
    /// (see [`update_name`]).
    pub fn export_source(
        &self,
        source: &str,
        path: &Path,
        title: Option<&str>,
    ) -> Result<ExportReport> {
        // Local work first, so a bad or symlinked sidecar fails before any
        // Google round-trip.
        let prior = sidecar::load(path)?;
        let doc = export::build_doc_html(source, dir_of(path), &stem_of(path))?;
        let auth = Authenticator::new(self);
        let drive = DriveClient::new(&self.http, &self.endpoints, &auth);
        let folder_id = drive.ensure_scrybe_folder()?;

        let linked = match &prior {
            Some(prior) => match drive.get_file(&prior.doc_id) {
                Ok(file) if !file.trashed => Some(file),
                Ok(_) | Err(GdocsError::DocNotFound(_)) => None,
                Err(e) => return Err(e),
            },
            None => None,
        };
        let (file, action) = match linked {
            Some(live) => {
                let recorded = prior.as_ref().and_then(|p| p.name.as_deref());
                let name = update_name(title, &doc.title, &live.name, recorded);
                (
                    drive.update_doc_from_html(&live.id, name, &doc.html)?,
                    ExportAction::Updated,
                )
            }
            None => (
                drive.create_doc_from_html(title.unwrap_or(&doc.title), &doc.html, &folder_id)?,
                ExportAction::Created,
            ),
        };

        let url = file.url();
        let written = sidecar::save(
            path,
            &Sidecar {
                v: sidecar::VERSION,
                doc_id: file.id.clone(),
                url: url.clone(),
                folder_id: folder_id.clone(),
                exported_at: now_rfc3339(),
                source_sha256: sidecar::markdown_sha256(source),
                figures: doc
                    .figures
                    .iter()
                    .map(|f| ManifestFigure {
                        index: f.index,
                        uuid: f.uuid.clone(),
                        sha256: f.sha256.clone(),
                        source: f.source.clone(),
                    })
                    .collect(),
                name: Some(file.name.clone()),
            },
        );
        if let Err(e) = written {
            return Err(orphaned_doc(e, action, &file.id, &url));
        }
        Ok(ExportReport {
            path: path.display().to_string(),
            doc_id: file.id,
            url,
            folder_id,
            action,
            figures: doc.figures.len(),
            images_skipped: doc.skipped_images,
        })
    }

    /// [`Self::export_source`] for a Markdown file on disk.
    pub fn export_file(&self, path: &Path, title: Option<&str>) -> Result<ExportReport> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            GdocsError::Io(std::io::Error::new(
                e.kind(),
                format!("{}: {e}", path.display()),
            ))
        })?;
        self.export_source(&source, path, title)
    }

    /// Import a Google Doc (id or URL) as Markdown at `output`, restoring
    /// Mermaid fences and writing any other images beside it. The sidecar
    /// written for `output` links it to the doc so a later export updates
    /// that doc in place.
    pub fn import_doc(&self, doc_ref: &str, output: &Path) -> Result<ImportReport> {
        let id = import::parse_doc_ref(doc_ref)?;
        // Local checks first, so a symlinked output or sidecar fails before
        // any Google round-trip. Only a sidecar for this same doc can vouch
        // for figure indices.
        sidecar::refuse_symlink(output)?;
        let prior = sidecar::load(output)?.filter(|s| s.doc_id == id);
        let auth = Authenticator::new(self);
        let drive = DriveClient::new(&self.http, &self.endpoints, &auth);
        let file = drive.get_file(&id)?;
        let md = drive.export_markdown(&id)?;

        let manifest: &[ManifestFigure] = match &prior {
            Some(s) => &s.figures,
            None => &[],
        };
        let stem = stem_of(output);
        let mut recovered = import::recover_figures(&md, &stem, manifest, None)?;
        // Google's Markdown export can drop alt text that its HTML export
        // keeps. An image that fell through to a sibling file with no
        // manifest to claim it has the HTML alt as its last carrier; with a
        // manifest present, falling through means it really is a plain image.
        if !recovered.images.is_empty() && manifest.is_empty() {
            let alts = import::harvest_html_alts(&drive.export_html(&id)?);
            recovered = import::recover_figures(&md, &stem, manifest, Some(&alts))?;
        }

        let dir = dir_of(output);
        std::fs::create_dir_all(dir)?;
        // Sibling image names are predictable too (`<stem>_img_NN.<ext>`):
        // check every target before writing anything, so a refusal leaves
        // no half-written import behind.
        let image_paths: Vec<PathBuf> = recovered
            .images
            .iter()
            .map(|image| dir.join(&image.file_name))
            .collect();
        for image_path in &image_paths {
            sidecar::refuse_symlink(image_path)?;
        }
        // Removing Google's trailing `[imageN]:` definitions leaves blank
        // lines at the end; settle on one newline.
        let markdown = format!("{}\n", recovered.markdown.trim_end());
        std::fs::write(output, &markdown)?;
        for (image, image_path) in recovered.images.iter().zip(&image_paths) {
            std::fs::write(image_path, &image.bytes)?;
        }
        let image_paths = image_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        let (folder_id, figures) = prior.map(|s| (s.folder_id, s.figures)).unwrap_or_default();
        sidecar::save(
            output,
            &Sidecar {
                v: sidecar::VERSION,
                doc_id: id.clone(),
                url: file.url(),
                // Unknown on a fresh import (`get_file` does not ask for
                // parents); the next export fills it in.
                folder_id,
                exported_at: now_rfc3339(),
                source_sha256: sidecar::markdown_sha256(&markdown),
                figures,
                name: Some(file.name),
            },
        )?;
        Ok(ImportReport {
            path: output.display().to_string(),
            doc_id: id,
            diagrams: recovered.figures.len(),
            images: recovered.images.len(),
            figures: recovered.figures,
            image_paths,
        })
    }
}

/// Which name an update sends. The explicit `title` always wins; otherwise
/// the Markdown-derived name (`derived`) is sent unless the user renamed the
/// doc in Drive since the sidecar `recorded` its name, in which case the
/// rename stands until the H1 changes too (the Markdown is the source of
/// truth). A sidecar with no recorded name, written before names were
/// tracked, follows the H1 as it always did. `None` leaves the name alone.
fn update_name<'a>(
    title: Option<&'a str>,
    derived: &'a str,
    live: &str,
    recorded: Option<&str>,
) -> Option<&'a str> {
    if let Some(title) = title {
        return Some(title);
    }
    if derived == live {
        return None;
    }
    match recorded {
        Some(recorded) if live != recorded && derived == recorded => None,
        _ => Some(derived),
    }
}

/// The upload went through but the sidecar did not, so nothing on disk
/// links the doc: say where it is, or a retry quietly makes another.
fn orphaned_doc(e: GdocsError, action: ExportAction, doc_id: &str, url: &str) -> GdocsError {
    let kind = match &e {
        GdocsError::Io(io) => io.kind(),
        _ => std::io::ErrorKind::Other,
    };
    let consequence = match action {
        ExportAction::Created => {
            "was created but its sidecar could not be written, so the next export will create \
             another doc"
        }
        ExportAction::Updated => {
            "was updated but its sidecar could not be rewritten, so its figure manifest is stale"
        }
    };
    GdocsError::Io(std::io::Error::new(
        kind,
        format!("{e}; the Google Doc {doc_id} ({url}) {consequence}"),
    ))
}

/// The directory `path` lives in; `.` when it has none.
fn dir_of(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

/// File name without its final extension; `Untitled` when there is none.
fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

// The owned-argument entry points for async callers (MCP server, Tauri):
// each builds the production `Gdocs` on a dedicated thread and runs there.

/// `scrybe gdoc login`.
pub fn login() -> Result<AuthStatus> {
    run_blocking(|| Gdocs::real()?.login())
}

/// `scrybe gdoc logout`.
pub fn logout() -> Result<AuthStatus> {
    run_blocking(|| Gdocs::real()?.logout())
}

/// `scrybe gdoc status`.
pub fn status() -> Result<AuthStatus> {
    run_blocking(|| Gdocs::real()?.status())
}

/// The docs Scrybe created, for an import picker.
pub fn list_docs() -> Result<Vec<DocSummary>> {
    run_blocking(|| Gdocs::real()?.list_docs())
}

/// Export Markdown held in memory (an unsaved tab) as the doc for `path`.
pub fn export_markdown_source(
    source: String,
    path: PathBuf,
    title: Option<String>,
) -> Result<ExportReport> {
    run_blocking(|| Gdocs::real()?.export_source(&source, &path, title.as_deref()))
}

/// Export a Markdown file on disk.
pub fn export_markdown_file(path: PathBuf, title: Option<String>) -> Result<ExportReport> {
    run_blocking(|| Gdocs::real()?.export_file(&path, title.as_deref()))
}

/// Import a Google Doc (id or URL) to `output`.
pub fn import_doc(doc_ref: String, output: PathBuf) -> Result<ImportReport> {
    run_blocking(|| Gdocs::real()?.import_doc(&doc_ref, &output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Method, Request, Response};
    use crate::testing::{offline_gdocs, FakeHttp};
    use crate::token_store::TokenSet;
    use crate::types::{AuthState, Verification};
    use crate::{config, drive, token_store, Endpoints, SCOPE_DRIVE_FILE};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde_json::{json, Value};

    const DOC_ID: &str = "1DocAbcdefghijklmnopqrstuvwxyz";
    const FOLDER_ID: &str = "folder-scrybe";

    fn signed_in(dir: &Path) -> Gdocs<FakeHttp> {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            config::client_path(dir),
            r#"{"installed":{"client_id":"cid","client_secret":"sec"}}"#,
        )
        .unwrap();
        token_store::save(
            dir,
            &TokenSet {
                access_token: "tok".into(),
                refresh_token: Some("rt".into()),
                expires_at: Utc::now() + chrono::Duration::days(30),
                scopes: vec![SCOPE_DRIVE_FILE.into()],
                client_id: "cid".into(),
            },
        )
        .unwrap();
        offline_gdocs(dir).with_endpoints(Endpoints {
            drive: "https://fake.test/drive/v3".into(),
            upload: "https://fake.test/upload/drive/v3".into(),
            ..Endpoints::default()
        })
    }

    fn file_json(id: &str, trashed: bool) -> Value {
        named_file_json(id, "Notes", trashed)
    }

    fn named_file_json(id: &str, name: &str, trashed: bool) -> Value {
        json!({
            "id": id,
            "name": name,
            "webViewLink": format!("https://docs.google.com/document/d/{id}/edit?usp=drivesdk"),
            "trashed": trashed,
            "mimeType": drive::DOC_MIME,
        })
    }

    fn doc_url(id: &str) -> String {
        format!("https://docs.google.com/document/d/{id}/edit?usp=drivesdk")
    }

    /// Link `path` to `DOC_ID` with a sidecar whose recorded name is `name`.
    fn link(path: &Path, name: Option<&str>) {
        sidecar::save(
            path,
            &Sidecar {
                v: sidecar::VERSION,
                doc_id: DOC_ID.into(),
                url: doc_url(DOC_ID),
                folder_id: FOLDER_ID.into(),
                exported_at: "2026-01-01T00:00:00Z".into(),
                source_sha256: String::new(),
                figures: vec![],
                name: name.map(str::to_string),
            },
        )
        .unwrap();
    }

    fn stub_folder(fake: &FakeHttp) {
        fake.on(|r| r.method == Method::Get && r.path() == "/drive/v3/files")
            .json(
                200,
                json!({ "files": [{ "id": FOLDER_ID, "name": "Scrybe" }] }),
            );
    }

    fn stub_create(fake: &FakeHttp, id: &'static str) {
        fake.on(|r| r.method == Method::Post && r.path() == "/upload/drive/v3/files")
            .json(200, file_json(id, false));
    }

    fn stub_get(fake: &FakeHttp, id: &'static str, status: u16, body: Value) {
        let path = format!("/drive/v3/files/{id}");
        fake.on(move |r| r.method == Method::Get && r.path() == path)
            .json(status, body);
    }

    fn stub_update(fake: &FakeHttp, id: &'static str) {
        let path = format!("/upload/drive/v3/files/{id}");
        fake.on(move |r| r.method == Method::Patch && r.path() == path)
            .json(200, file_json(id, false));
    }

    /// Like Drive: the PATCH answers with the name the request set, else the
    /// name the doc already had (`live`).
    fn stub_update_echoing_name(fake: &FakeHttp, id: &'static str, live: &'static str) {
        let path = format!("/upload/drive/v3/files/{id}");
        fake.on(move |r| r.method == Method::Patch && r.path() == path)
            .reply(move |r| {
                let name = upload_metadata(r)["name"]
                    .as_str()
                    .unwrap_or(live)
                    .to_string();
                Response {
                    status: 200,
                    headers: vec![],
                    body: serde_json::to_vec(&named_file_json(id, &name, false)).unwrap(),
                }
            });
    }

    fn stub_export(fake: &FakeHttp, id: &'static str, mime: &'static str, body: &str) {
        let path = format!("/drive/v3/files/{id}/export");
        fake.on(move |r| {
            r.method == Method::Get
                && r.path() == path
                && r.query_param("mimeType").as_deref() == Some(mime)
        })
        .text(200, body);
    }

    fn uploads(fake: &FakeHttp) -> Vec<Request> {
        fake.requests()
            .into_iter()
            .filter(|r| r.path().starts_with("/upload/"))
            .collect()
    }

    /// The JSON metadata part of a multipart upload.
    fn upload_metadata(req: &Request) -> Value {
        let body = std::str::from_utf8(&req.body).unwrap();
        let start = body.find("\r\n\r\n").unwrap() + 4;
        let end = body[start..].find("\r\n--").unwrap() + start;
        serde_json::from_str(&body[start..end]).unwrap()
    }

    fn html_export_requested(fake: &FakeHttp) -> bool {
        fake.requests()
            .iter()
            .any(|r| r.query_param("mimeType").as_deref() == Some("text/html"))
    }

    #[test]
    fn export_creates_without_a_sidecar_then_updates_with_one() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, DOC_ID);
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        stub_update(&g.http, DOC_ID);
        let path = docs.path().join("notes.md");
        let source = "# Hello\n\nSome text.\n";

        let report = g.export_source(source, &path, None).unwrap();
        assert_eq!(report.action, ExportAction::Created);
        assert_eq!(report.doc_id, DOC_ID);
        assert_eq!(report.folder_id, FOLDER_ID);
        assert_eq!(report.figures, 0);
        assert!(report.images_skipped.is_empty());
        assert_eq!(report.path, path.display().to_string());
        assert_eq!(
            report.url,
            format!("https://docs.google.com/document/d/{DOC_ID}/edit?usp=drivesdk")
        );
        let saved = sidecar::load(&path).unwrap().unwrap();
        assert_eq!(saved.v, sidecar::VERSION);
        assert_eq!(saved.doc_id, DOC_ID);
        assert_eq!(saved.url, report.url);
        assert_eq!(saved.folder_id, FOLDER_ID);
        assert_eq!(saved.source_sha256, sidecar::markdown_sha256(source));
        assert!(saved.figures.is_empty());
        assert!(saved.exported_at.ends_with('Z'));
        assert_eq!(saved.name.as_deref(), Some("Notes"), "as Drive reported it");
        let first = uploads(&g.http);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].method, Method::Post);
        assert_eq!(
            upload_metadata(&first[0]),
            json!({ "name": "Hello", "mimeType": drive::DOC_MIME, "parents": [FOLDER_ID] })
        );

        let again = g.export_source(source, &path, None).unwrap();
        assert_eq!(again.action, ExportAction::Updated);
        assert_eq!(again.doc_id, DOC_ID);
        let second = uploads(&g.http);
        assert_eq!(second.len(), 2);
        assert_eq!(second[1].method, Method::Patch);
        assert_eq!(second[1].path(), format!("/upload/drive/v3/files/{DOC_ID}"));
        assert_eq!(upload_metadata(&second[1]), json!({ "name": "Hello" }));
    }

    #[test]
    fn export_recreates_when_the_linked_doc_is_trashed_or_gone() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, "fresh-doc-id");
        stub_get(&g.http, "trashed-doc", 200, file_json("trashed-doc", true));
        stub_get(
            &g.http,
            "gone-doc",
            404,
            json!({ "error": { "code": 404, "message": "File not found" } }),
        );
        let path = docs.path().join("notes.md");

        for old in ["trashed-doc", "gone-doc"] {
            sidecar::save(
                &path,
                &Sidecar {
                    v: sidecar::VERSION,
                    doc_id: old.into(),
                    url: drive::doc_url(old),
                    folder_id: FOLDER_ID.into(),
                    exported_at: "2026-01-01T00:00:00Z".into(),
                    source_sha256: String::new(),
                    figures: vec![],
                    name: None,
                },
            )
            .unwrap();
            let report = g.export_source("# T\n", &path, None).unwrap();
            assert_eq!(report.action, ExportAction::Created, "{old}");
            assert_eq!(report.doc_id, "fresh-doc-id");
            assert_eq!(
                sidecar::load(&path).unwrap().unwrap().doc_id,
                "fresh-doc-id"
            );
        }
        assert!(uploads(&g.http).iter().all(|r| r.method == Method::Post));
    }

    #[test]
    fn export_propagates_other_get_file_failures_without_uploading() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_get(
            &g.http,
            DOC_ID,
            500,
            json!({ "error": { "code": 500, "message": "Backend Error" } }),
        );
        let path = docs.path().join("notes.md");
        link(&path, None);

        let err = g.export_source("# T\n", &path, None).unwrap_err();
        assert_eq!(err.code(), "api_error");
        assert!(uploads(&g.http).is_empty());
    }

    #[test]
    fn update_name_rules() {
        // --title always wins.
        assert_eq!(update_name(Some("T"), "H1", "Live", Some("Rec")), Some("T"));
        // Nothing to change.
        assert_eq!(update_name(None, "Same", "Same", None), None);
        assert_eq!(update_name(None, "Same", "Same", Some("Old")), None);
        // Renamed in Drive, H1 as recorded: the rename stands.
        assert_eq!(update_name(None, "Q4", "Q4 (final)", Some("Q4")), None);
        // Renamed in Drive and the H1 changed too: Markdown wins.
        assert_eq!(
            update_name(None, "Q4 v2", "Q4 (final)", Some("Q4")),
            Some("Q4 v2")
        );
        // Drive still has the recorded name: follow the H1.
        assert_eq!(update_name(None, "Q4 v2", "Q4", Some("Q4")), Some("Q4 v2"));
        // Legacy sidecar without a record: follow the H1 as before.
        assert_eq!(
            update_name(None, "Q4 v2", "Q4 (final)", None),
            Some("Q4 v2")
        );
    }

    #[test]
    fn export_keeps_a_drive_rename_until_the_h1_changes() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_get(
            &g.http,
            DOC_ID,
            200,
            named_file_json(DOC_ID, "Q4 Plan (final)", false),
        );
        stub_update_echoing_name(&g.http, DOC_ID, "Q4 Plan (final)");
        let path = docs.path().join("plan.md");

        // Renamed in Drive, H1 unchanged: the rename stands and is recorded.
        link(&path, Some("Q4 Plan"));
        let report = g.export_source("# Q4 Plan\n", &path, None).unwrap();
        assert_eq!(report.action, ExportAction::Updated);
        assert_eq!(upload_metadata(&uploads(&g.http)[0]), json!({}));
        assert_eq!(
            sidecar::load(&path).unwrap().unwrap().name.as_deref(),
            Some("Q4 Plan (final)")
        );

        // The H1 changed as well: the Markdown wins.
        link(&path, Some("Q4 Plan"));
        g.export_source("# Q4 Plan v2\n", &path, None).unwrap();
        assert_eq!(
            upload_metadata(&uploads(&g.http)[1]),
            json!({ "name": "Q4 Plan v2" })
        );
        assert_eq!(
            sidecar::load(&path).unwrap().unwrap().name.as_deref(),
            Some("Q4 Plan v2")
        );

        // --title always wins.
        link(&path, Some("Q4 Plan"));
        g.export_source("# Q4 Plan\n", &path, Some("Explicit"))
            .unwrap();
        assert_eq!(
            upload_metadata(&uploads(&g.http)[2]),
            json!({ "name": "Explicit" })
        );
    }

    #[test]
    fn export_follows_the_h1_when_drive_still_has_the_recorded_name() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_get(
            &g.http,
            DOC_ID,
            200,
            named_file_json(DOC_ID, "Q4 Plan", false),
        );
        stub_update_echoing_name(&g.http, DOC_ID, "Q4 Plan");
        let path = docs.path().join("plan.md");

        link(&path, Some("Q4 Plan"));
        g.export_source("# Q4 Plan v2\n", &path, None).unwrap();
        assert_eq!(
            upload_metadata(&uploads(&g.http)[0]),
            json!({ "name": "Q4 Plan v2" })
        );

        // A legacy sidecar (no recorded name) follows the H1 too.
        link(&path, None);
        g.export_source("# Q4 Plan v2\n", &path, None).unwrap();
        assert_eq!(
            upload_metadata(&uploads(&g.http)[1]),
            json!({ "name": "Q4 Plan v2" })
        );

        // Same name on both sides: nothing to send.
        link(&path, Some("Whatever"));
        g.export_source("# Q4 Plan\n", &path, None).unwrap();
        assert_eq!(upload_metadata(&uploads(&g.http)[2]), json!({}));
        assert_eq!(
            sidecar::load(&path).unwrap().unwrap().name.as_deref(),
            Some("Q4 Plan")
        );
    }

    /// The upload succeeded but the sidecar could not be written: the error
    /// must still tell the user where the doc went.
    #[cfg(unix)]
    #[test]
    fn export_reports_the_doc_when_the_sidecar_cannot_be_written() {
        use std::os::unix::fs::PermissionsExt;
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, DOC_ID);
        let path = docs.path().join("notes.md");
        std::fs::write(&path, "# Hello\n").unwrap();

        std::fs::set_permissions(docs.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores directory modes; nothing to test there.
        let writable = std::fs::write(docs.path().join("probe"), b"").is_ok();
        let result = (!writable).then(|| g.export_source("# Hello\n", &path, None));
        std::fs::set_permissions(docs.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let Some(result) = result else {
            return;
        };

        let err = result.unwrap_err();
        assert_eq!(uploads(&g.http).len(), 1, "the upload itself went through");
        assert_eq!(err.code(), "io_error");
        let msg = err.to_string();
        assert!(msg.contains(DOC_ID), "{msg}");
        assert!(msg.contains(&doc_url(DOC_ID)), "{msg}");
        assert!(msg.contains("notes.gdoc.json"), "{msg}");
        assert!(msg.contains("was created"), "{msg}");
        assert!(!path.with_extension("gdoc.json").exists());
    }

    #[test]
    fn export_names_the_doc_from_title_h1_or_stem() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, DOC_ID);
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        stub_update(&g.http, DOC_ID);
        let path = docs.path().join("my-notes.md");

        g.export_source("no heading here\n", &path, None).unwrap();
        g.export_source("no heading here\n", &path, Some("Custom Name"))
            .unwrap();
        g.export_source("# From H1\n", &path, None).unwrap();
        let names: Vec<String> = uploads(&g.http)
            .iter()
            .map(|r| upload_metadata(r)["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["my-notes", "Custom Name", "From H1"]);
    }

    #[test]
    fn export_file_reads_the_file_and_names_it_on_error() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, DOC_ID);
        let path = docs.path().join("on-disk.md");

        let err = g.export_file(&path, None).unwrap_err();
        assert_eq!(err.code(), "io_error");
        assert!(err.to_string().contains("on-disk.md"), "{err}");
        assert_eq!(g.http.request_count(), 0);

        std::fs::write(&path, "# Disk\n\nbody\n").unwrap();
        let report = g.export_file(&path, None).unwrap();
        assert_eq!(report.action, ExportAction::Created);
        assert_eq!(upload_metadata(&uploads(&g.http)[0])["name"], "Disk");
        assert_eq!(
            sidecar::load(&path).unwrap().unwrap().source_sha256,
            sidecar::markdown_sha256("# Disk\n\nbody\n")
        );
    }

    #[test]
    fn export_manifest_lists_each_figure_by_image_index() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        std::fs::write(docs.path().join("logo.png"), b"LOGO").unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, DOC_ID);
        let path = docs.path().join("notes.md");
        let source = "# Figures\n\n![logo](logo.png)\n\n![shared](../shared/logo.png)\n\n```mermaid\ngraph TD; A-->B\n```\n";

        let report = g.export_source(source, &path, None).unwrap();
        assert_eq!(report.figures, 1);
        assert_eq!(
            report.images_skipped,
            vec!["../shared/logo.png"],
            "outside the document's directory: left as written and reported"
        );
        let saved = sidecar::load(&path).unwrap().unwrap();
        assert_eq!(saved.figures.len(), 1);
        let fig = &saved.figures[0];
        assert_eq!(fig.index, 2, "the logo and the skipped image come first");
        assert_eq!(fig.source, "graph TD; A-->B");
        assert_eq!(
            fig.sha256,
            scrybe_mermaid_render::source_sha256("graph TD; A-->B")
        );
        assert!(uuid::Uuid::parse_str(&fig.uuid).is_ok());
        let body = String::from_utf8_lossy(&uploads(&g.http)[0].body).into_owned();
        assert!(
            body.contains(&format!(
                "src=\"data:image/png;base64,{}\"",
                STANDARD.encode(b"LOGO")
            )),
            "local image inlined relative to the Markdown's directory"
        );
    }

    /// A pre-planted `<stem>.gdoc.json` link must redirect neither the
    /// sidecar read (to another doc, which the PATCH would then overwrite)
    /// nor the write: refused before Google is contacted, whether the link
    /// dangles or points at a real sidecar, which stays as it was.
    #[cfg(unix)]
    #[test]
    fn export_refuses_a_symlinked_sidecar_before_any_request() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_folder(&g.http);
        stub_create(&g.http, DOC_ID);
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        stub_update(&g.http, DOC_ID);
        let path = docs.path().join("notes.md");
        let other = docs.path().join("other.md");
        std::os::unix::fs::symlink(sidecar::path_for(&other), sidecar::path_for(&path)).unwrap();

        let err = g.export_source("# T\n", &path, None).unwrap_err();
        assert_eq!(err.code(), "invalid_argument", "dangling: {err}");
        assert!(!sidecar::path_for(&other).exists());

        link(&other, Some("Other"));
        let err = g.export_source("# T\n", &path, None).unwrap_err();
        assert_eq!(err.code(), "invalid_argument", "valid target: {err}");
        assert_eq!(
            sidecar::load(&other).unwrap().unwrap().name.as_deref(),
            Some("Other"),
            "the other doc's sidecar is untouched"
        );
        assert_eq!(g.http.request_count(), 0);
    }

    /// Import writes `<stem>_img_NN.<ext>` beside the output: a planted
    /// link there (or at the output itself) must not redirect the write.
    #[cfg(unix)]
    #[test]
    fn import_refuses_a_symlinked_sibling_image_or_output() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        let jpeg = b"\xFF\xD8\xFF\xE0fake-jpeg".to_vec();
        let md = format!(
            "# Imported\n\n![photo][image1]\n\n[image1]: <data:image/jpeg;base64,{}>\n",
            STANDARD.encode(&jpeg)
        );
        stub_export(&g.http, DOC_ID, "text/markdown", &md);
        stub_export(
            &g.http,
            DOC_ID,
            "text/html",
            "<html><body><p><img src=\"x\" alt=\"photo\"></p></body></html>",
        );
        let victim = docs.path().join("victim.bin");
        std::fs::write(&victim, b"precious").unwrap();

        let out = docs.path().join("out.md");
        std::os::unix::fs::symlink(&victim, docs.path().join("out_img_01.jpg")).unwrap();
        let err = g.import_doc(DOC_ID, &out).unwrap_err();
        assert_eq!(err.code(), "invalid_argument", "{err}");
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        assert!(
            !out.exists(),
            "nothing is written when a sibling is refused"
        );
        assert!(!sidecar::path_for(&out).exists());

        let seen = g.http.request_count();
        let linked = docs.path().join("linked.md");
        std::os::unix::fs::symlink(&victim, &linked).unwrap();
        let err = g.import_doc(DOC_ID, &linked).unwrap_err();
        assert_eq!(err.code(), "invalid_argument", "{err}");
        assert_eq!(g.http.request_count(), seen, "refused before any request");
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
    }

    #[test]
    fn import_rejects_bad_refs_before_any_request() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        let err = g
            .import_doc("https://example.com/nope", &docs.path().join("x.md"))
            .unwrap_err();
        assert_eq!(err.code(), "invalid_argument");
        assert_eq!(g.http.request_count(), 0);
    }

    #[test]
    fn import_of_a_missing_doc_is_doc_not_found() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_get(
            &g.http,
            DOC_ID,
            404,
            json!({ "error": { "code": 404, "message": "File not found" } }),
        );
        let out = docs.path().join("x.md");
        let err = g.import_doc(DOC_ID, &out).unwrap_err();
        assert_eq!(err.code(), "doc_not_found");
        assert!(!out.exists());
    }

    #[test]
    fn import_writes_markdown_images_and_sidecar_and_consults_html_alts() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        let jpeg = b"\xFF\xD8\xFF\xE0fake-jpeg".to_vec();
        let md = format!(
            "# Imported\n\n![photo][image1]\n\nBody.\n\n[image1]: <data:image/jpeg;base64,{}>\n",
            STANDARD.encode(&jpeg)
        );
        stub_export(&g.http, DOC_ID, "text/markdown", &md);
        stub_export(
            &g.http,
            DOC_ID,
            "text/html",
            "<html><body><p><img src=\"x\" alt=\"photo\"></p></body></html>",
        );
        let out = docs.path().join("nested").join("out.md");

        let report = g
            .import_doc(
                &format!("https://docs.google.com/document/d/{DOC_ID}/edit"),
                &out,
            )
            .unwrap();
        assert_eq!(report.doc_id, DOC_ID);
        assert_eq!(report.path, out.display().to_string());
        assert_eq!(report.diagrams, 0);
        assert_eq!(report.images, 1);
        assert!(report.figures.is_empty());
        let image_path = docs.path().join("nested").join("out_img_01.jpg");
        assert_eq!(report.image_paths, vec![image_path.display().to_string()]);
        assert_eq!(std::fs::read(&image_path).unwrap(), jpeg);
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "# Imported\n\n![photo](out_img_01.jpg)\n\nBody.\n"
        );
        let saved = sidecar::load(&out).unwrap().unwrap();
        assert_eq!(saved.doc_id, DOC_ID);
        assert_eq!(
            saved.url,
            format!("https://docs.google.com/document/d/{DOC_ID}/edit?usp=drivesdk")
        );
        assert_eq!(saved.folder_id, "");
        assert_eq!(saved.name.as_deref(), Some("Notes"));
        assert!(saved.figures.is_empty());
        assert_eq!(
            saved.source_sha256,
            sidecar::markdown_sha256("# Imported\n\n![photo](out_img_01.jpg)\n\nBody.\n")
        );
        assert!(
            html_export_requested(&g.http),
            "an image fell through with no manifest, so the HTML alts were tried"
        );
    }

    #[test]
    fn import_skips_the_html_export_when_nothing_falls_through() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        stub_export(
            &g.http,
            DOC_ID,
            "text/markdown",
            "# Plain\n\nNo images.\n\n\n",
        );
        let out = docs.path().join("plain.md");
        // A sidecar for some other doc is not a manifest for this one.
        sidecar::save(
            &out,
            &Sidecar {
                v: sidecar::VERSION,
                doc_id: "another-doc".into(),
                url: drive::doc_url("another-doc"),
                folder_id: FOLDER_ID.into(),
                exported_at: "2026-01-01T00:00:00Z".into(),
                source_sha256: String::new(),
                figures: vec![ManifestFigure {
                    index: 0,
                    uuid: "u".into(),
                    sha256: String::new(),
                    source: "graph TD; X-->Y".into(),
                }],
                name: None,
            },
        )
        .unwrap();

        let report = g.import_doc(DOC_ID, &out).unwrap();
        assert_eq!((report.diagrams, report.images), (0, 0));
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "# Plain\n\nNo images.\n"
        );
        assert!(!html_export_requested(&g.http));
        let saved = sidecar::load(&out).unwrap().unwrap();
        assert_eq!(saved.doc_id, DOC_ID);
        assert_eq!(saved.folder_id, "");
        assert!(
            saved.figures.is_empty(),
            "the other doc's manifest is dropped"
        );
    }

    #[test]
    fn import_uses_and_carries_forward_a_matching_sidecar_manifest() {
        let cfg = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        stub_get(&g.http, DOC_ID, 200, file_json(DOC_ID, false));
        let stripped = scrybe_mermaid_render::render_png("graph TD; A-->B").unwrap();
        let md = format!(
            "# Doc\n\n![][image1]\n\n[image1]: <data:image/png;base64,{}>\n",
            STANDARD.encode(&stripped)
        );
        stub_export(&g.http, DOC_ID, "text/markdown", &md);
        let out = docs.path().join("doc.md");
        let manifest = vec![ManifestFigure {
            index: 0,
            uuid: "u".into(),
            sha256: scrybe_mermaid_render::source_sha256("graph TD; A-->B"),
            source: "graph TD; A-->B".into(),
        }];
        sidecar::save(
            &out,
            &Sidecar {
                v: sidecar::VERSION,
                doc_id: DOC_ID.into(),
                url: drive::doc_url(DOC_ID),
                folder_id: FOLDER_ID.into(),
                exported_at: "2026-01-01T00:00:00Z".into(),
                source_sha256: String::new(),
                figures: manifest.clone(),
                name: None,
            },
        )
        .unwrap();

        let report = g.import_doc(DOC_ID, &out).unwrap();
        assert_eq!(report.diagrams, 1);
        assert_eq!(report.figures[0].verification, Verification::Manifest);
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "# Doc\n\n```mermaid\ngraph TD; A-->B\n```\n"
        );
        assert!(!html_export_requested(&g.http));
        let saved = sidecar::load(&out).unwrap().unwrap();
        assert_eq!(saved.folder_id, FOLDER_ID);
        assert_eq!(saved.figures, manifest);
    }

    #[test]
    fn list_docs_and_status_delegate() {
        let cfg = tempfile::tempdir().unwrap();
        let g = signed_in(cfg.path());
        g.http
            .on(|r| r.method == Method::Get && r.path() == "/drive/v3/files")
            .json(200, json!({ "files": [{ "id": "a", "name": "Alpha" }] }));
        let docs = g.list_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].url, drive::doc_url("a"));
        assert_eq!(g.status().unwrap().status, AuthState::LoggedIn);

        // No OAuth client: every call says so, not "sign in" to nothing.
        let empty = tempfile::tempdir().unwrap();
        let unconfigured = offline_gdocs(empty.path());
        assert_eq!(
            unconfigured.status().unwrap().status,
            AuthState::NotConfigured
        );
        assert_eq!(
            unconfigured.list_docs().unwrap_err().code(),
            "not_configured"
        );
        assert_eq!(unconfigured.http.request_count(), 0);
    }

    #[test]
    fn free_functions_build_the_real_client_from_the_config_dir_env() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var(config::DIR_ENV, dir.path());
        let st = status().unwrap();
        std::env::remove_var(config::DIR_ENV);
        // `Gdocs::real` reads the client env vars, which a developer may
        // have set; a fresh dir holds no token either way.
        assert_ne!(st.status, AuthState::LoggedIn);
        assert_eq!(st.config_dir, dir.path().display().to_string());
    }

    #[test]
    fn path_helpers() {
        assert_eq!(dir_of(Path::new("notes.md")), Path::new("."));
        assert_eq!(dir_of(Path::new("a/b/notes.md")), Path::new("a/b"));
        assert_eq!(stem_of(Path::new("a/b/notes.md")), "notes");
        assert_eq!(stem_of(Path::new("a.b.md")), "a.b");
        assert_eq!(stem_of(Path::new("README")), "README");
        assert_eq!(stem_of(Path::new("")), "Untitled");
    }
}
