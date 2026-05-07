// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Scrybe panels — bake-off orchestrator and calibration log.
//!
//! Sends the same prompt to N registered agents and presents their
//! responses side-by-side. Human thumbs-up/down feedback is recorded
//! in a local SQLite calibration database.
//!

pub mod calibration;
pub mod orchestrator;

pub use calibration::CalibrationLog;
pub use orchestrator::PanelOrchestrator;
