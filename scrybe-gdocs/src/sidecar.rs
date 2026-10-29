// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! The per-document sidecar: `<stem>.gdoc.json` beside the Markdown file.
//!
//! It links the file to its Google Doc (so a re-export updates in place) and
//! carries the figure manifest that import falls back on when Google strips
//! both the PNG payload and the image alt text.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{GdocsError, Result, SIDECAR_SUFFIX};

/// Current sidecar schema version.
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ManifestFigure {
    /// 0-based position among the `<img>` tags of the uploaded HTML (what
    /// Google sees: Markdown images, raw `<img>` tags and Mermaid figures
    /// together, in document order), which `import::recover_figures`
    /// re-derives from the images in Google's export.
    pub index: usize,
    pub uuid: String,
    /// SHA-256 hex of `source`.
    pub sha256: String,
    pub source: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Sidecar {
    /// Schema version, [`VERSION`].
    pub v: u32,
    pub doc_id: String,
    pub url: String,
    pub folder_id: String,
    /// RFC 3339 timestamp of the export that wrote this sidecar.
    pub exported_at: String,
    /// SHA-256 hex of the whole Markdown as exported ([`markdown_sha256`]).
    pub source_sha256: String,
    pub figures: Vec<ManifestFigure>,
    /// The doc's name as Scrybe last set or observed it, so the next export
    /// can tell a rename made in Drive from a changed H1. Absent in
    /// sidecars written before names were tracked (then the H1 is followed).
    #[serde(default)]
    pub name: Option<String>,
}

/// `notes.md` → `notes.gdoc.json`; only the final extension is replaced
/// (`a.b.md` → `a.b.gdoc.json`).
pub fn path_for(doc_path: &Path) -> PathBuf {
    let mut os = doc_path.with_extension("").into_os_string();
    os.push(SIDECAR_SUFFIX);
    PathBuf::from(os)
}

/// `Invalid` when `path` is itself a symlink. The sidecar and the files
/// written beside a document have predictable names, so a pre-planted link
/// would redirect a read (to another doc's sidecar) or a write (to any file
/// the user can write) to a path the caller never named. The link is what
/// is refused, so a dangling one is caught too.
pub fn refuse_symlink(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_symlink() => Err(GdocsError::Invalid(format!(
            "refusing to read or write through a symlink: {}",
            path.display()
        ))),
        _ => Ok(()),
    }
}

/// Read the sidecar beside `doc_path`. A missing sidecar is `Ok(None)`; a
/// malformed or symlinked one is `GdocsError::Invalid`.
pub fn load(doc_path: &Path) -> Result<Option<Sidecar>> {
    let path = path_for(doc_path);
    refuse_symlink(&path)?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(GdocsError::Io(e)),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| GdocsError::Invalid(format!("{}: malformed sidecar: {e}", path.display())))
}

/// Write the sidecar beside `doc_path` as pretty JSON with a trailing
/// newline. A symlinked sidecar is refused; an I/O error names the
/// sidecar's path.
pub fn save(doc_path: &Path, sidecar: &Sidecar) -> Result<()> {
    let mut json = serde_json::to_string_pretty(sidecar)?;
    json.push('\n');
    let path = path_for(doc_path);
    refuse_symlink(&path)?;
    std::fs::write(&path, json).map_err(|e| {
        GdocsError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", path.display()),
        ))
    })
}

/// Lowercase hex SHA-256 of the Markdown bytes.
pub fn markdown_sha256(md: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(md.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sidecar {
        Sidecar {
            v: VERSION,
            doc_id: "1AbCdEfGhIjKlMnOpQrStUvWxYz".into(),
            url: "https://docs.google.com/document/d/1AbCdEfGhIjKlMnOpQrStUvWxYz/edit".into(),
            folder_id: "folder123".into(),
            exported_at: "2026-10-16T12:00:00Z".into(),
            source_sha256: markdown_sha256("# Hi\n"),
            figures: vec![ManifestFigure {
                index: 0,
                uuid: "123e4567-e89b-12d3-a456-426614174000".into(),
                sha256: "abc".into(),
                source: "graph TD; A-->B".into(),
            }],
            name: Some("Hi".into()),
        }
    }

    #[test]
    fn path_for_replaces_only_the_final_extension() {
        assert_eq!(
            path_for(Path::new("notes.md")),
            PathBuf::from("notes.gdoc.json")
        );
        assert_eq!(
            path_for(Path::new("a.b.md")),
            PathBuf::from("a.b.gdoc.json")
        );
        assert_eq!(
            path_for(Path::new("dir/sub/notes.markdown")),
            PathBuf::from("dir/sub/notes.gdoc.json")
        );
        assert_eq!(
            path_for(Path::new("dir/README")),
            PathBuf::from("dir/README.gdoc.json")
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        let sidecar = sample();
        save(&doc, &sidecar).unwrap();

        let written = std::fs::read_to_string(dir.path().join("notes.gdoc.json")).unwrap();
        assert!(written.ends_with('\n'));
        assert!(
            written.contains("\n  \"doc_id\""),
            "pretty-printed: {written}"
        );

        assert_eq!(load(&doc).unwrap(), Some(sidecar));
    }

    #[test]
    fn load_missing_is_none_and_malformed_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        assert_eq!(load(&doc).unwrap(), None);

        std::fs::write(path_for(&doc), "{ not json").unwrap();
        assert!(matches!(load(&doc), Err(GdocsError::Invalid(_))));
    }

    /// Sidecars written before `name` existed still load, with no name.
    #[test]
    fn load_accepts_a_sidecar_without_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        let mut v = serde_json::to_value(sample()).unwrap();
        v.as_object_mut().unwrap().remove("name");
        std::fs::write(path_for(&doc), v.to_string()).unwrap();
        let loaded = load(&doc).unwrap().unwrap();
        assert_eq!(loaded.name, None);
        assert_eq!(loaded.doc_id, sample().doc_id);
    }

    #[test]
    fn save_failure_names_the_sidecar_path() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("missing-dir").join("notes.md");
        let err = save(&doc, &sample()).unwrap_err();
        assert_eq!(err.code(), "io_error");
        let msg = err.to_string();
        assert!(msg.contains("notes.gdoc.json"), "{msg}");
        assert!(msg.contains("missing-dir"), "{msg}");
    }

    /// A planted `<stem>.gdoc.json` link is refused on read and on write,
    /// whether it dangles or points at a real sidecar, and nothing lands
    /// where it points.
    #[cfg(unix)]
    #[test]
    fn load_and_save_refuse_a_symlinked_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        let target = dir.path().join("elsewhere.json");
        std::os::unix::fs::symlink(&target, path_for(&doc)).unwrap();

        assert_eq!(load(&doc).unwrap_err().code(), "invalid_argument");
        assert_eq!(
            save(&doc, &sample()).unwrap_err().code(),
            "invalid_argument"
        );
        assert!(!target.exists(), "nothing written through a dangling link");

        std::fs::write(&target, serde_json::to_string(&sample()).unwrap()).unwrap();
        let before = std::fs::read(&target).unwrap();
        assert_eq!(load(&doc).unwrap_err().code(), "invalid_argument");
        assert_eq!(
            save(&doc, &sample()).unwrap_err().code(),
            "invalid_argument"
        );
        assert_eq!(std::fs::read(&target).unwrap(), before);
    }

    #[test]
    fn markdown_sha256_matches_known_vectors() {
        assert_eq!(
            markdown_sha256(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            markdown_sha256("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
