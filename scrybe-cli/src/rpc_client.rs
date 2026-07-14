//! Thin JSON-RPC client to talk to the running Scrybe GUI.
//!
//! The implementation now lives in [`scrybe_rpc::client`] so the CLI and the
//! MCP server dial the live app through **one** shared dialer (two divergent
//! clients is exactly the split `scrybe-rpc` exists to prevent). This module
//! re-exports it to keep the existing `rpc_client::*` call sites unchanged.

pub use scrybe_rpc::client::{is_live, send, send_to, socket_path, try_connect, try_connect_at};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_reexported() {
        // The re-export links and resolves to the shared implementation.
        assert!(!socket_path().as_os_str().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn send_to_errors_when_no_server() {
        let path = std::path::PathBuf::from("/tmp/scrybe-nonexistent-sock-cli-reexport-test");
        let err = send_to(&path, "open", serde_json::json!({"path": "/tmp/foo.md"})).unwrap_err();
        assert!(err.contains("no Scrybe running"));
    }
}
