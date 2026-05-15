// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Phase and goal configuration loaded from `drake-swarm.toml`.

use serde::{Deserialize, Serialize};

/// Top-level drake-swarm configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SwarmConfig {
    pub swarm: SwarmMeta,
    #[serde(rename = "phase")]
    pub phases: Vec<PhaseConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SwarmMeta {
    pub name: String,
    pub worker: String,
    pub graders: Vec<String>,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
    #[serde(default = "default_ssim_threshold")]
    pub ssim_threshold: f64,
    #[serde(default = "default_true")]
    pub structural_required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PhaseConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub goals: Vec<String>,
}

impl SwarmConfig {
    /// Load from a `drake-swarm.toml` file.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Return phases in dependency order (topological sort).
    pub fn phases_in_order(&self) -> Vec<&PhaseConfig> {
        let mut remaining: Vec<&PhaseConfig> = self.phases.iter().collect();
        let mut ordered: Vec<&PhaseConfig> = Vec::new();
        let mut completed: std::collections::HashSet<&str> = std::collections::HashSet::new();

        let limit = remaining.len() * remaining.len() + 1;
        let mut iterations = 0;
        while !remaining.is_empty() && iterations < limit {
            iterations += 1;
            remaining.retain(|p| {
                if p.depends_on.iter().all(|dep| completed.contains(dep.as_str())) {
                    completed.insert(&p.id);
                    ordered.push(p);
                    false
                } else {
                    true
                }
            });
        }
        ordered
    }
}

fn default_max_rounds() -> u32 { 5 }
fn default_ssim_threshold() -> f64 { 0.85 }
fn default_true() -> bool { true }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phases_in_order() {
        let config = SwarmConfig {
            swarm: SwarmMeta {
                name: "test".into(),
                worker: "ollama".into(),
                graders: vec![],
                max_rounds: 5,
                ssim_threshold: 0.85,
                structural_required: true,
            },
            phases: vec![
                PhaseConfig { id: "p3".into(), name: "".into(), depends_on: vec!["p1".into(), "p2".into()], goals: vec![] },
                PhaseConfig { id: "p1".into(), name: "".into(), depends_on: vec![], goals: vec![] },
                PhaseConfig { id: "p2".into(), name: "".into(), depends_on: vec!["p1".into()], goals: vec![] },
            ],
        };
        let ordered = config.phases_in_order();
        assert_eq!(ordered[0].id, "p1");
        assert_eq!(ordered[1].id, "p2");
        assert_eq!(ordered[2].id, "p3");
    }
}
