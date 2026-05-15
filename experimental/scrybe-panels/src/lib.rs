// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! scrybe-panels — bake-off orchestrator, drake-swarm runner, and calibration log.

pub mod calibration;
pub mod orchestrator;
pub mod phase;

pub use calibration::{CalibrationEvent, CalibrationLog};
pub use orchestrator::{PanelOrchestrator, RoundResult};
pub use phase::SwarmConfig;
