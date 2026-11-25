// SPDX-License-Identifier: Apache-2.0
//! Files handed to Scrybe by the operating system: Finder's "Open With",
//! double-clicking an associated file, or dropping one on the Dock icon.
//!
//! On macOS these arrive as `RunEvent::Opened` rather than as command-line
//! arguments. A cold launch delivers them before the frontend has registered
//! its `scrybe://open` listener, so they wait in [`OpenQueue`] until the
//! frontend calls `take_opened_files`. After that, each one is emitted as it
//! arrives.

use std::sync::Mutex;

/// Paths waiting for the frontend, or nothing once it is listening.
#[derive(Debug)]
pub struct OpenQueue {
    /// `Some` until the frontend drains the queue; `None` afterwards.
    pending: Option<Vec<String>>,
}

impl Default for OpenQueue {
    fn default() -> Self {
        Self {
            pending: Some(Vec::new()),
        }
    }
}

impl OpenQueue {
    /// Record a path the OS asked us to open. Returns the path when the
    /// frontend is already listening and the caller should emit it now.
    pub fn push(&mut self, path: String) -> Option<String> {
        match &mut self.pending {
            Some(queue) => {
                queue.push(path);
                None
            }
            None => Some(path),
        }
    }

    /// Hand every queued path to the frontend and switch to live delivery.
    pub fn drain(&mut self) -> Vec<String> {
        self.pending.take().unwrap_or_default()
    }
}

pub static OPENED: Mutex<OpenQueue> = Mutex::new(OpenQueue {
    pending: Some(Vec::new()),
});

/// The local file paths among the URLs the OS handed over. Anything that is
/// not a `file://` URL is ignored.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))] // only macOS delivers RunEvent::Opened
pub fn file_paths(urls: &[tauri::Url]) -> Vec<String> {
    urls.iter()
        .filter(|u| u.scheme() == "file")
        .filter_map(|u| u.to_file_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Called by the frontend once its `scrybe://open` listener is registered.
/// Returns the files the OS asked us to open before then.
#[tauri::command]
pub fn take_opened_files() -> Vec<String> {
    OPENED.lock().map(|mut q| q.drain()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_wait_until_the_frontend_drains_them() {
        let mut q = OpenQueue::default();
        assert_eq!(q.push("/a.md".into()), None);
        assert_eq!(q.push("/b.json".into()), None);
        assert_eq!(q.drain(), vec!["/a.md".to_string(), "/b.json".to_string()]);
    }

    #[test]
    fn after_draining_paths_are_delivered_immediately() {
        let mut q = OpenQueue::default();
        assert!(q.drain().is_empty());
        assert_eq!(q.push("/c.rs".into()), Some("/c.rs".to_string()));
        assert!(q.drain().is_empty());
    }

    #[test]
    #[cfg(unix)] // a drive-less file URL is not a path on Windows
    fn only_file_urls_become_paths() {
        let urls = [
            tauri::Url::parse("file:///tmp/notes%20one.md").unwrap(),
            tauri::Url::parse("https://example.com/x.md").unwrap(),
        ];
        assert_eq!(file_paths(&urls), vec!["/tmp/notes one.md".to_string()]);
    }
}
