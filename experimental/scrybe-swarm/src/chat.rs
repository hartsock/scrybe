// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Swarm chat — NATS subject: `scrybe.chat.<doc_cid>`.

/// Publishes and subscribes to chat messages for a document.
pub struct SwarmChat {
    doc_cid: String,
}

impl SwarmChat {
    pub fn new(doc_cid: impl Into<String>) -> Self {
        Self {
            doc_cid: doc_cid.into(),
        }
    }

    pub fn subject(&self) -> String {
        format!("scrybe.chat.{}", self.doc_cid)
    }
}
