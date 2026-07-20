// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe swarm — NATS-based swarm chat and activity feed.
//!
//! The swarm sidebar shows live activity from other agents working on
//! the same document or workspace. Messages are scoped to the open
//! document's ContentDigest (BLAKE3 hex).
//!

pub mod activity;
pub mod chat;

pub use activity::ActivityFeed;
pub use chat::SwarmChat;
