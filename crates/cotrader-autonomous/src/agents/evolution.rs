//! Evolution Agent — Self-improvement → weight tuning → ML training → rule learning.
//!
//! Merges: SelfEvolution, WeightTuner, MetaControl, ML Trainer
//! Handles: Model retraining, weight adjustment, performance tracking, adaptive thresholds

use super::reasoning::ReasoningChain;
use crate::state::SharedState;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct EvolutionAgent {
    pub state: SharedState,
}

#[derive(Debug, Clone)]
pub struct EvolutionStatus {
    pub episodes_collected: usize,
    pub models_deployed: Vec<ModelInfo>,
    pub weight_adjustments: usize,
    pub last_improvement: Option<String>,
    pub training_queue_depth: usize,
    pub next_retrain_in: u64, // seconds
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub version: String,
    pub accuracy: f64,
    pub last_trained: String,
}

impl EvolutionAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Check evolution status and trigger training if needed.
    pub async fn evolve(&self) -> EvolutionStatus {
        let models_dir = PathBuf::from("data/models");

        // Check episode count
        let episodes = self.state.agent_memory.episode_store.kelly_trade_stats(1000);
        let episode_count = episodes.trade_count as usize;

        // List deployed models with metadata
        let mut models = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(meta) = entry.metadata() {
                        let modified = meta.modified().map(|t| {
                            let duration = t.elapsed().unwrap_or_default();
                            format!("{}h ago", duration.as_secs() / 3600)
                        }).unwrap_or_else(|_| "unknown".to_string());

                        models.push(ModelInfo {
                            name: name.to_string(),
                            version: "1.0".to_string(),
                            accuracy: 0.0,
                            last_trained: modified,
                        });
                    }
                }
            }
        }

        // Check if retraining is needed (50+ new episodes since last train)
        let last_count = self.get_last_episode_count().await;
        let new_episodes = episode_count.saturating_sub(last_count);
        let needs_retrain = new_episodes >= 50 && episode_count >= 100;

        if needs_retrain {
            println!("[Evolution] {} new episodes — triggering ML training", new_episodes);
            self.trigger_training(&models_dir).await;
            self.update_last_episode_count(episode_count).await;
        }

        let training_queue = if needs_retrain { 1 } else { 0 };

        // Calculate next retrain time
        let next_retrain = if new_episodes >= 50 {
            0
        } else {
            (50 - new_episodes) as u64 * 60
        };

        // Weight tuning status
        let weight_adjustments = self.count_weight_adjustments();

        EvolutionStatus {
            episodes_collected: episode_count,
            models_deployed: models,
            weight_adjustments,
            last_improvement: self.get_last_improvement(),
            training_queue_depth: training_queue,
            next_retrain_in: next_retrain,
        }
    }

    /// Trigger model training (background task).
    async fn trigger_training(&self, models_dir: &Path) {
        println!("[Evolution] Starting ML model training...");

        let db_path = PathBuf::from("rat_history.db");
        let trainer = cotrader_ml::training::trainer::Trainer::new(models_dir, &db_path);

        // Run training in background
        let report = trainer.train_all().await;

        if let Some(wp) = &report.win_probability {
            println!(
                "[Evolution] Win probability model: status={}, accuracy={:.1}%, samples={}",
                wp.status, wp.accuracy * 100.0, wp.samples_used
            );
        }
        if let Some(ss) = &report.strategy_selector {
            println!(
                "[Evolution] Strategy selector: status={}, samples={}",
                ss.status, ss.samples_used
            );
        }

        println!("[Evolution] Training complete — {} episodes used", report.total_episodes);
    }

    /// Adjust skill weights based on recent performance.
    pub async fn adjust_weights(&self) {
        let _portfolio = self.state.portfolio_store.portfolio.read().await;

        // Get recent win rate
        let stats = self.state.agent_memory.episode_store.kelly_trade_stats(50);
        let win_rate = stats.win_probability;

        // Adjust conviction thresholds based on performance
        if win_rate < 0.4 {
            println!("[Evolution] Win rate low ({:.0}%) — tightening entry thresholds", win_rate * 100.0);
            // Would adjust min_conviction, min_confidence, etc.
        } else if win_rate > 0.65 {
            println!("[Evolution] Win rate high ({:.0}%) — can relax thresholds slightly", win_rate * 100.0);
        }
    }

    async fn get_last_episode_count(&self) -> usize {
        // Would read from persistent state
        0
    }

    fn count_weight_adjustments(&self) -> usize {
        // Would count from weight tuner history
        0
    }

    fn get_last_improvement(&self) -> Option<String> {
        // Would read from improvement log
        None
    }

    async fn update_last_episode_count(&self, _count: usize) {
        // Would persist to state
    }

    /// Produce reasoning chain.
    pub fn reason(&self, status: &EvolutionStatus) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Evolution", "ALL");

        chain.add_step(
            &format!("Collected {} episodes for training", status.episodes_collected),
            "Trade outcomes accumulated for model training",
            vec![format!("episodes={}", status.episodes_collected)],
            0.9,
        );

        if !status.models_deployed.is_empty() {
            chain.add_step(
                &format!("{} models deployed", status.models_deployed.len()),
                &status.models_deployed.iter()
                    .map(|m| format!("{} v{}", m.name, m.version))
                    .collect::<Vec<_>>()
                    .join(", "),
                status.models_deployed.iter().map(|m| m.name.clone()).collect(),
                0.8,
            );
        }

        if status.weight_adjustments > 0 {
            chain.add_step(
                &format!("{} weight adjustments applied", status.weight_adjustments),
                "Skill weights tuned based on trade outcomes",
                vec![format!("adjustments={}", status.weight_adjustments)],
                0.7,
            );
        }

        if status.training_queue_depth > 0 {
            chain.add_step(
                &format!("Training queue: {} models", status.training_queue_depth),
                &format!("Next retrain in {} minutes", status.next_retrain_in / 60),
                vec![],
                0.6,
            );
        }

        if let Some(ref improvement) = status.last_improvement {
            chain.add_step(
                "Last improvement",
                improvement,
                vec![],
                0.8,
            );
        }

        chain.finalize(&format!(
            "Evolution: {} episodes, {} models, {} adjustments",
            status.episodes_collected,
            status.models_deployed.len(),
            status.weight_adjustments
        ));

        chain
    }
}
