// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Export → Google → import, end to end against a scripted Drive. The PNGs
//! Scrybe uploaded come back in "Google's" Markdown export either exactly as
//! sent (payload intact), re-rendered without the payload (alt text carries
//! the source), or stripped of both (the sidecar manifest does).

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};
use tempfile::TempDir;

use scrybe_gdocs::export::parse_alt_payload;
use scrybe_gdocs::testing::FakeHttp;
use scrybe_gdocs::token_store::{self, TokenSet};
use scrybe_gdocs::{
    config, drive, sidecar, Endpoints, ExportAction, Gdocs, Method, Request, Verification,
    SCOPE_DRIVE_FILE,
};

const DOC_ID: &str = "1RoundTripDocIdAbcdefghijklmnop";
const FOLDER_ID: &str = "folder-scrybe-1";
const DIAGRAM_A: &str = "graph TD; A-->B";
const DIAGRAM_B: &str = "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi";

fn source() -> String {
    format!(
        "# Round Trip\n\nIntro paragraph.\n\n```mermaid\n{DIAGRAM_A}\n```\n\n\
         Middle text with *emphasis*.\n\n```mermaid\n{DIAGRAM_B}\n```\n\nEnd.\n"
    )
}

struct Harness {
    _config: TempDir,
    docs: TempDir,
    gdocs: Gdocs<FakeHttp>,
}

impl Harness {
    fn doc_path(&self, name: &str) -> PathBuf {
        self.docs.path().join(name)
    }

    fn fake(&self) -> &FakeHttp {
        &self.gdocs.http
    }
}

fn file_json(id: &str) -> Value {
    json!({
        "id": id,
        "name": "Round Trip",
        "webViewLink": format!("https://docs.google.com/document/d/{id}/edit?usp=drivesdk"),
        "trashed": false,
        "mimeType": drive::DOC_MIME,
        "modifiedTime": "2026-10-16T12:00:00.000Z",
    })
}

/// A signed-in `Gdocs` over a Drive that already has the Scrybe folder and
/// answers create / get / update for `DOC_ID`.
fn harness() -> Harness {
    let config_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        config::client_path(config_dir.path()),
        r#"{"installed":{"client_id":"cid.apps.googleusercontent.com","client_secret":"GOCSPX-x"}}"#,
    )
    .unwrap();
    token_store::save(
        config_dir.path(),
        &TokenSet {
            access_token: "access-token".into(),
            refresh_token: Some("refresh-token".into()),
            expires_at: Utc::now() + chrono::Duration::days(365),
            scopes: vec![SCOPE_DRIVE_FILE.into()],
            client_id: "cid.apps.googleusercontent.com".into(),
        },
    )
    .unwrap();

    let fake = FakeHttp::new();
    fake.on(|r| r.method == Method::Get && r.path() == "/drive/v3/files")
        .json(
            200,
            json!({ "files": [{ "id": FOLDER_ID, "name": "Scrybe" }] }),
        );
    fake.on(|r| r.method == Method::Post && r.path() == "/upload/drive/v3/files")
        .json(200, file_json(DOC_ID));
    fake.on(|r| r.method == Method::Get && r.path() == format!("/drive/v3/files/{DOC_ID}"))
        .json(200, file_json(DOC_ID));
    fake.on(|r| {
        r.method == Method::Patch && r.path() == format!("/upload/drive/v3/files/{DOC_ID}")
    })
    .json(200, file_json(DOC_ID));

    let gdocs = Gdocs::new(fake, config_dir.path().to_path_buf())
        .with_endpoints(Endpoints {
            drive: "https://fake.test/drive/v3".into(),
            upload: "https://fake.test/upload/drive/v3".into(),
            ..Endpoints::default()
        })
        .with_client_source(config::load_client_file)
        .with_browser(|_| panic!("no browser is opened during export/import"));
    Harness {
        _config: config_dir,
        docs: tempfile::tempdir().unwrap(),
        gdocs,
    }
}

fn stub_export(fake: &FakeHttp, mime: &'static str, body: &str) {
    fake.on(move |r| {
        r.method == Method::Get
            && r.path() == format!("/drive/v3/files/{DOC_ID}/export")
            && r.query_param("mimeType").as_deref() == Some(mime)
    })
    .text(200, body);
}

fn html_export_requested(fake: &FakeHttp) -> bool {
    fake.requests()
        .iter()
        .any(|r| r.query_param("mimeType").as_deref() == Some("text/html"))
}

/// The HTML part of the most recent multipart upload.
fn last_uploaded_html(fake: &FakeHttp) -> String {
    let req: Request = fake
        .requests()
        .into_iter()
        .filter(|r| r.path().starts_with("/upload/"))
        .next_back()
        .expect("an upload was sent");
    let content_type = req.header_value("content-type").expect("Content-Type");
    let (_, boundary) = content_type
        .split_once("; boundary=")
        .expect("multipart boundary");
    let body = std::str::from_utf8(&req.body).expect("UTF-8 body");
    let marker = "Content-Type: text/html; charset=UTF-8\r\n\r\n";
    let start = body.find(marker).expect("HTML part") + marker.len();
    let end = body[start..]
        .find(&format!("\r\n--{boundary}--"))
        .expect("closing boundary")
        + start;
    body[start..end].to_string()
}

fn attr(tag: &str, name: &str) -> String {
    let key = format!("{name}=\"");
    let start = tag.find(&key).expect("attribute present") + key.len();
    let end = tag[start..].find('"').expect("attribute closes") + start;
    tag[start..end].to_string()
}

/// `(alt, png bytes)` of every `<img>` in the uploaded HTML, in order.
fn uploaded_images(fake: &FakeHttp) -> Vec<(String, Vec<u8>)> {
    let html = last_uploaded_html(fake);
    let mut out = Vec::new();
    let mut rest = html.as_str();
    while let Some(i) = rest.find("<img ") {
        let end = rest[i..].find('>').expect("img tag closes") + i;
        let tag = &rest[i..end];
        let data = attr(tag, "src");
        let b64 = data
            .strip_prefix("data:image/png;base64,")
            .expect("figures are PNG data URIs");
        out.push((attr(tag, "alt"), STANDARD.decode(b64).expect("base64")));
        rest = &rest[end..];
    }
    out
}

/// What Google's Markdown export looks like for the round-trip document:
/// reference-style images with `data:` definitions at the end.
fn google_markdown(images: &[(String, Vec<u8>)]) -> String {
    assert_eq!(images.len(), 2);
    let mut md = format!(
        "# Round Trip\n\nIntro paragraph.\n\n![{}][image1]\n\n\
         Middle text with *emphasis*.\n\n![{}][image2]\n\nEnd.\n\n",
        images[0].0, images[1].0
    );
    for (n, (_, png)) in images.iter().enumerate() {
        md.push_str(&format!(
            "[image{}]: <data:image/png;base64,{}>\n",
            n + 1,
            STANDARD.encode(png)
        ));
    }
    md
}

fn stripped_png(diagram: &str) -> Vec<u8> {
    let png = scrybe_mermaid_render::render_png(diagram).unwrap();
    assert!(
        scrybe_mermaid::extract_unverified(&png).is_err(),
        "a bare render carries no payload"
    );
    png
}

fn verifications(report: &scrybe_gdocs::ImportReport) -> Vec<Verification> {
    report.figures.iter().map(|f| f.verification).collect()
}

fn export_once(h: &Harness, path: &Path) -> Vec<(String, Vec<u8>)> {
    let report = h.gdocs.export_source(&source(), path, None).unwrap();
    assert_eq!(report.action, ExportAction::Created);
    assert_eq!(report.figures, 2);
    let images = uploaded_images(h.fake());
    assert_eq!(images.len(), 2);
    images
}

#[test]
fn export_create_update_then_import_restores_verified_fences() {
    let h = harness();
    let notes = h.doc_path("notes.md");

    let created = h.gdocs.export_source(&source(), &notes, None).unwrap();
    assert_eq!(created.action, ExportAction::Created);
    assert_eq!(created.doc_id, DOC_ID);
    assert_eq!(created.folder_id, FOLDER_ID);
    assert_eq!(created.figures, 2);
    assert_eq!(
        created.url,
        format!("https://docs.google.com/document/d/{DOC_ID}/edit?usp=drivesdk")
    );
    let saved = sidecar::load(&notes).unwrap().unwrap();
    assert_eq!(saved.doc_id, DOC_ID);
    assert_eq!(saved.folder_id, FOLDER_ID);
    assert_eq!(saved.source_sha256, sidecar::markdown_sha256(&source()));
    let manifest: Vec<(usize, &str)> = saved
        .figures
        .iter()
        .map(|f| (f.index, f.source.as_str()))
        .collect();
    assert_eq!(manifest, vec![(0, DIAGRAM_A), (1, DIAGRAM_B)]);

    // Google keeps the image bytes: the upload's own PNGs come back.
    let images = uploaded_images(h.fake());
    assert_eq!(images.len(), 2);
    for ((alt, png), diagram) in images.iter().zip([DIAGRAM_A, DIAGRAM_B]) {
        assert_eq!(parse_alt_payload(alt).unwrap().source, diagram);
        let payload = scrybe_mermaid::extract(png).unwrap();
        assert!(payload.is_verified());
        assert_eq!(payload.source, diagram);
    }

    let updated = h.gdocs.export_source(&source(), &notes, None).unwrap();
    assert_eq!(updated.action, ExportAction::Updated);
    assert_eq!(updated.doc_id, DOC_ID);
    assert!(h
        .fake()
        .requests()
        .iter()
        .any(|r| r.method == Method::Patch));
    assert_eq!(sidecar::load(&notes).unwrap().unwrap().doc_id, DOC_ID);

    let google_md = google_markdown(&images);
    stub_export(h.fake(), "text/markdown", &google_md);
    let imported_path = h.doc_path("imported.md");
    let imported = h
        .gdocs
        .import_doc(
            &format!("https://docs.google.com/document/d/{DOC_ID}/edit?usp=sharing"),
            &imported_path,
        )
        .unwrap();
    assert_eq!(imported.doc_id, DOC_ID);
    assert_eq!(imported.path, imported_path.display().to_string());
    assert_eq!(imported.diagrams, 2);
    assert_eq!(imported.images, 0);
    assert!(imported.image_paths.is_empty());
    assert_eq!(
        verifications(&imported),
        vec![Verification::Verified, Verification::Verified]
    );
    assert_eq!(
        std::fs::read_to_string(&imported_path).unwrap(),
        source(),
        "both fences come back verbatim"
    );
    let imported_sidecar = sidecar::load(&imported_path).unwrap().unwrap();
    assert_eq!(imported_sidecar.doc_id, DOC_ID);
    assert_eq!(imported_sidecar.url, created.url);
    assert!(!html_export_requested(h.fake()));

    // The imported file is now linked to the same doc: exporting it updates.
    let reexport = h.gdocs.export_file(&imported_path, None).unwrap();
    assert_eq!(reexport.action, ExportAction::Updated);
    assert_eq!(reexport.doc_id, DOC_ID);
}

#[test]
fn reencoded_pngs_with_alt_kept_recover_as_alt_text() {
    let h = harness();
    let images = export_once(&h, &h.doc_path("notes.md"));
    let reencoded: Vec<(String, Vec<u8>)> = images
        .iter()
        .zip([DIAGRAM_A, DIAGRAM_B])
        .map(|((alt, _), diagram)| (alt.clone(), stripped_png(diagram)))
        .collect();
    stub_export(h.fake(), "text/markdown", &google_markdown(&reencoded));

    let out = h.doc_path("from-alt.md");
    let report = h.gdocs.import_doc(DOC_ID, &out).unwrap();
    assert_eq!(
        verifications(&report),
        vec![Verification::AltText, Verification::AltText]
    );
    assert_eq!(report.images, 0);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), source());
    assert!(!html_export_requested(h.fake()));
}

#[test]
fn stripped_pngs_without_alt_recover_from_the_output_sidecar_manifest() {
    let h = harness();
    let notes = h.doc_path("notes.md");
    export_once(&h, &notes);
    let stripped = vec![
        (String::new(), stripped_png(DIAGRAM_A)),
        (String::new(), stripped_png(DIAGRAM_B)),
    ];
    stub_export(h.fake(), "text/markdown", &google_markdown(&stripped));

    // The manifest is read from the sidecar beside the OUTPUT path.
    let out = h.doc_path("from-manifest.md");
    let exported = sidecar::load(&notes).unwrap().unwrap();
    sidecar::save(&out, &exported).unwrap();

    let report = h.gdocs.import_doc(DOC_ID, &out).unwrap();
    assert_eq!(
        verifications(&report),
        vec![Verification::Manifest, Verification::Manifest]
    );
    assert_eq!(report.images, 0);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), source());
    assert!(!html_export_requested(h.fake()));
    let rewritten = sidecar::load(&out).unwrap().unwrap();
    assert_eq!(rewritten.figures, exported.figures);
    assert_eq!(rewritten.folder_id, FOLDER_ID);
    assert_eq!(rewritten.source_sha256, sidecar::markdown_sha256(&source()));
}

#[test]
fn stripped_pngs_without_markdown_alt_fall_back_to_the_html_export() {
    let h = harness();
    export_once(&h, &h.doc_path("notes.md"));
    let stripped = vec![
        (String::new(), stripped_png(DIAGRAM_A)),
        (String::new(), stripped_png(DIAGRAM_B)),
    ];
    stub_export(h.fake(), "text/markdown", &google_markdown(&stripped));
    // Google's HTML export keeps the alt attributes the Markdown one dropped.
    stub_export(h.fake(), "text/html", &last_uploaded_html(h.fake()));

    let out = h.doc_path("from-html.md");
    let report = h.gdocs.import_doc(DOC_ID, &out).unwrap();
    assert_eq!(
        verifications(&report),
        vec![Verification::AltText, Verification::AltText]
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), source());
    assert!(html_export_requested(h.fake()));
}

/// A raw `<img>` in the Markdown is an image in the Google Doc too, so the
/// manifest numbers figures the way Google does, and the manifest fallback
/// restores the figure, not the logo.
#[test]
fn a_raw_html_image_before_the_fence_keeps_the_manifest_aligned() {
    let h = harness();
    let notes = h.doc_path("notes.md");
    let src = "# Logo\n\n<img src=\"https://example.test/logo.png\" width=\"200\">\n\n\
               ```mermaid\ngraph TD; A-->B\n```\n";
    let report = h.gdocs.export_source(src, &notes, None).unwrap();
    assert_eq!(report.figures, 1);
    let exported = sidecar::load(&notes).unwrap().unwrap();
    assert_eq!(exported.figures[0].index, 1, "the raw <img> is image 0");

    // Google: the logo as a plain PNG, the figure re-encoded, both alts gone.
    let logo = stripped_png("graph TD; L");
    let figure = stripped_png(DIAGRAM_A);
    let google_md = format!(
        "# Logo\n\n![][image1]\n\n![][image2]\n\n\
         [image1]: <data:image/png;base64,{}>\n[image2]: <data:image/png;base64,{}>\n",
        STANDARD.encode(&logo),
        STANDARD.encode(&figure)
    );
    stub_export(h.fake(), "text/markdown", &google_md);
    let out = h.doc_path("from-manifest.md");
    sidecar::save(&out, &exported).unwrap();

    let report = h.gdocs.import_doc(DOC_ID, &out).unwrap();
    assert_eq!((report.diagrams, report.images), (1, 1));
    assert_eq!(report.figures[0].index, 1);
    assert_eq!(report.figures[0].verification, Verification::Manifest);
    let logo_path = h.doc_path("from-manifest_img_01.png");
    assert_eq!(report.image_paths, vec![logo_path.display().to_string()]);
    assert_eq!(std::fs::read(&logo_path).unwrap(), logo);
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "# Logo\n\n![](from-manifest_img_01.png)\n\n```mermaid\ngraph TD; A-->B\n```\n"
    );
    assert!(!html_export_requested(h.fake()));
}

#[test]
fn images_with_no_carrier_left_become_sibling_files() {
    let h = harness();
    let stripped = vec![
        (String::new(), stripped_png(DIAGRAM_A)),
        ("a caption".to_string(), stripped_png(DIAGRAM_B)),
    ];
    stub_export(h.fake(), "text/markdown", &google_markdown(&stripped));
    stub_export(
        h.fake(),
        "text/html",
        "<html><body><p><img src=\"a\"></p><p><img src=\"b\" alt=\"a caption\"></p></body></html>",
    );

    let out = h.doc_path("photos.md");
    let report = h.gdocs.import_doc(DOC_ID, &out).unwrap();
    assert_eq!(report.diagrams, 0);
    assert_eq!(report.images, 2);
    assert_eq!(
        report.image_paths,
        vec![
            h.doc_path("photos_img_01.png").display().to_string(),
            h.doc_path("photos_img_02.png").display().to_string(),
        ]
    );
    assert_eq!(
        std::fs::read(h.doc_path("photos_img_01.png")).unwrap(),
        stripped[0].1
    );
    assert_eq!(
        std::fs::read(h.doc_path("photos_img_02.png")).unwrap(),
        stripped[1].1
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "# Round Trip\n\nIntro paragraph.\n\n![](photos_img_01.png)\n\n\
         Middle text with *emphasis*.\n\n![a caption](photos_img_02.png)\n\nEnd.\n"
    );
    assert!(html_export_requested(h.fake()));
}
