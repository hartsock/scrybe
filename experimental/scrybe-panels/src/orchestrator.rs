// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Bake-off orchestrator — broadcasts prompt to N agents, collects responses.
//! Extended for drake-swarm phase/round execution.

use scrybe_mcp_client::AgentRegistry;

use crate::calibration::{CalibrationEvent, CalibrationLog};
use crate::phase::{PhaseConfig, SwarmConfig};

/// Result of a single drake-swarm round.
#[derive(Debug, Clone)]
pub struct RoundResult {
    pub phase_id: String,
    pub goal_index: usize,
    pub round: u32,
    pub goal: String,
    pub worker_output: String,
    pub claude_code_grade: String,
    pub codex_grade: String,
    pub ssim_score: Option<f64>,
    pub structural_pass: bool,
}

impl RoundResult {
    /// Whether this round should be considered passing (advance to next goal).
    pub fn is_passing(&self, config: &SwarmConfig) -> bool {
        if config.swarm.structural_required && !self.structural_pass {
            return false;
        }
        if let Some(ssim) = self.ssim_score {
            return ssim >= config.swarm.ssim_threshold;
        }
        // No SSIM available (oracle not generated yet) — pass on structural only.
        true
    }
}

/// Orchestrates phase/round execution for a drake-swarm project.
pub struct PanelOrchestrator {
    registry: AgentRegistry,
}

impl PanelOrchestrator {
    pub fn new(registry: AgentRegistry) -> Self {
        Self { registry }
    }

    /// Run all phases from `config` in dependency order, recording results
    /// in `calibration_log`.
    ///
    /// Drake calls this entry point. For each phase and each goal in the phase,
    /// it runs up to `config.swarm.max_rounds` rounds until the goal passes.
    pub async fn run_swarm(
        &self,
        config: &SwarmConfig,
        calibration_log: &CalibrationLog,
    ) -> anyhow::Result<Vec<RoundResult>> {
        let mut all_results = Vec::new();

        for phase in config.phases_in_order() {
            tracing::info!("==> Phase: {} — {}", phase.id, phase.name);
            for (goal_idx, goal) in phase.goals.iter().enumerate() {
                let result = self
                    .run_goal_with_retry(phase, goal_idx, goal, config, calibration_log)
                    .await?;
                all_results.push(result);
            }
        }

        Ok(all_results)
    }

    async fn run_goal_with_retry(
        &self,
        phase: &PhaseConfig,
        goal_index: usize,
        goal: &str,
        config: &SwarmConfig,
        calibration_log: &CalibrationLog,
    ) -> anyhow::Result<RoundResult> {
        for round in 1..=config.swarm.max_rounds {
            tracing::info!("  Goal {}/{} round {}", goal_index + 1, phase.goals.len(), round);

            let result = self.execute_round(phase, goal_index, round, goal, config).await?;

            // Record in calibration log.
            calibration_log.record(&CalibrationEvent {
                prompt_hash: format!("{:x}", md5_hash(&format!("{}{}", phase.id, goal_index))),
                agent_name: config.swarm.worker.clone(),
                thumbs_up: result.is_passing(config),
                phase_id: Some(phase.id.clone()),
                round: Some(round),
                ssim_score: result.ssim_score,
                structural_pass: Some(result.structural_pass),
            })?;

            if result.is_passing(config) {
                tracing::info!("    PASS (round {round})");
                return Ok(result);
            }
            tracing::warn!("    FAIL (round {round}) — retrying with grader feedback");
        }

        anyhow::bail!(
            "Goal {}/{} in phase '{}' did not pass after {} rounds",
            goal_index + 1,
            phase.goals.len(),
            phase.id,
            config.swarm.max_rounds
        )
    }

    async fn execute_round(
        &self,
        phase: &PhaseConfig,
        goal_index: usize,
        round: u32,
        goal: &str,
        _config: &SwarmConfig,
    ) -> anyhow::Result<RoundResult> {
        // TODO (Drake): wire up actual MCP agent calls via self.registry.
        // Pseudocode:
        //   let worker = self.registry.get("ollama")?;
        //   let code = worker.call_tool("code", &goal_prompt(goal)).await?;
        //
        //   let cc_grader = self.registry.get("claude-code")?;
        //   let cc_grade = cc_grader.call_tool("review", &review_prompt(&code)).await?;
        //
        //   let codex_grader = self.registry.get("codex")?;
        //   let codex_grade = codex_grader.call_tool("review", &correctness_prompt(&code)).await?;
        //
        //   let (ssim, structural) = run_trace_tests_and_parse_output()?;

        Ok(RoundResult {
            phase_id: phase.id.clone(),
            goal_index,
            round,
            goal: goal.to_string(),
            worker_output: String::new(),    // populated by MCP call
            claude_code_grade: String::new(), // populated by MCP call
            codex_grade: String::new(),       // populated by MCP call
            ssim_score: None,
            structural_pass: false,
        })
    }
}

fn md5_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
