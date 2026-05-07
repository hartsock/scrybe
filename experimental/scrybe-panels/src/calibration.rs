// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! SQLite-backed calibration log for panel thumbs-up/down.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A single calibration event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationEvent {
    pub prompt_hash: String,
    pub agent_name: String,
    pub thumbs_up: bool,
}

/// SQLite-backed log of human feedback on bake-off results.
pub struct CalibrationLog {
    conn: Connection,
}

impl CalibrationLog {
    /// Opens (or creates) a calibration database at *path*.
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS calibration (
                id          INTEGER PRIMARY KEY,
                ts          TEXT NOT NULL DEFAULT (datetime('now')),
                prompt_hash TEXT NOT NULL,
                agent_name  TEXT NOT NULL,
                thumbs_up   INTEGER NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    /// Records a feedback event.
    pub fn record(&self, event: &CalibrationEvent) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO calibration (prompt_hash, agent_name, thumbs_up) VALUES (?1, ?2, ?3)",
            rusqlite::params![event.prompt_hash, event.agent_name, event.thumbs_up as i32],
        )?;
        Ok(())
    }
}
