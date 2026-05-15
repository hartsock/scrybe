// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! SQLite-backed calibration log for panel and drake-swarm round results.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A single calibration event (human feedback or automated round result).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationEvent {
    pub prompt_hash: String,
    pub agent_name: String,
    pub thumbs_up: bool,
    // Drake-swarm extensions (None for plain bake-off events)
    pub phase_id: Option<String>,
    pub round: Option<u32>,
    pub ssim_score: Option<f64>,
    pub structural_pass: Option<bool>,
}

/// SQLite-backed log of feedback and automated grading results.
pub struct CalibrationLog {
    conn: Connection,
}

impl CalibrationLog {
    /// Opens (or creates) a calibration database at *path*.
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS calibration (
                id               INTEGER PRIMARY KEY,
                ts               TEXT NOT NULL DEFAULT (datetime('now')),
                prompt_hash      TEXT NOT NULL,
                agent_name       TEXT NOT NULL,
                thumbs_up        INTEGER NOT NULL,
                phase_id         TEXT,
                round            INTEGER,
                ssim_score       REAL,
                structural_pass  INTEGER
            );",
        )?;
        Ok(Self { conn })
    }

    /// Records a feedback or round-result event.
    pub fn record(&self, event: &CalibrationEvent) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO calibration
             (prompt_hash, agent_name, thumbs_up, phase_id, round, ssim_score, structural_pass)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                event.prompt_hash,
                event.agent_name,
                event.thumbs_up as i32,
                event.phase_id,
                event.round,
                event.ssim_score,
                event.structural_pass.map(|b| b as i32),
            ],
        )?;
        Ok(())
    }

    /// Returns SSIM scores for a given phase, ordered by round.
    pub fn ssim_history(&self, phase_id: &str) -> rusqlite::Result<Vec<(u32, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT round, ssim_score FROM calibration
             WHERE phase_id = ?1 AND ssim_score IS NOT NULL
             ORDER BY round ASC",
        )?;
        let rows = stmt.query_map([phase_id], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, f64>(1)?))
        })?;
        rows.collect()
    }
}
