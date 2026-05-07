// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Activity feed — NATS subject: `scrybe.activity.<doc_cid>`.

use serde::{Deserialize, Serialize};

/// A single activity event from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub agent: String,
    pub action: String,
    pub doc_cid: String,
}

/// Subscribes to activity events for a document.
pub struct ActivityFeed {
    doc_cid: String,
}

impl ActivityFeed {
    pub fn new(doc_cid: impl Into<String>) -> Self {
        Self {
            doc_cid: doc_cid.into(),
        }
    }

    pub fn subject(&self) -> String {
        format!("scrybe.activity.{}", self.doc_cid)
    }
}
