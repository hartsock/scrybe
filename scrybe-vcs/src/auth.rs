// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Authentication helpers for git2 remote operations.
//!
//! Strategy (tried in order):
//! 1. SSH key forwarded via the ssh-agent.
//! 2. Token-based HTTPS: reads `SCRYBE_GITEA_TOKEN` from the environment;
//!    user = `"git"`, password = token.

use git2::{Cred, RemoteCallbacks};

/// Builds [`RemoteCallbacks`] that handle SSH-agent and HTTPS-token auth.
///
/// The returned callbacks have a `credentials` handler that:
/// 1. Tries `Cred::ssh_key_from_agent` for SSH URLs.
/// 2. Falls back to `Cred::userpass_plaintext` using `SCRYBE_GITEA_TOKEN`
///    for HTTPS URLs.
pub fn make_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, allowed_types| {
        // 1. SSH via agent.
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            let username = username_from_url.unwrap_or("git");
            return Cred::ssh_key_from_agent(username);
        }

        // 2. HTTPS via token env var.
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            let token = std::env::var("SCRYBE_GITEA_TOKEN").unwrap_or_default();
            return Cred::userpass_plaintext("git", &token);
        }

        Err(git2::Error::from_str(
            "no supported credential type available",
        ))
    });
    callbacks
}
