//! Evolution Agent — Self-improvement → weight tuning → ML training → rule learning.

use super::reasoning::ReasoningChain;
use crate::episode_store::EpisodeStore;
use crate::types::{AgentOutputEvent, CacheFrame};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct EvolutionAgent {
    pub cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
    pub episode_store: Arc<EpisodeStore>,
}

#[derive(Debug, Clone)]
pub struct EvolutionStatus {
    pub episodes_collected: usize,
    pub models_deployed: Vec<ModelInfo>,
    pub weight_adjustments: usize,
    pub last_improvement: Option<String>,
    pub training_queue_depth: usize,
    pub next_retrain_in: u64,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub version: String,
    pub accuracy: f64,
    pub last_trained: String,
}

impl EvolutionAgent {
    pub fn new(
        cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
        episode_store: Arc<EpisodeStore>,
    ) -> Self {
        Self {
            cot_tx,
            episode_store,
        }
    }

    /// Check evolution status from CacheFrame + episode store.
    pub async fn evolve(&self, frame: &CacheFrame) -> EvolutionStatus {
        let models_dir = PathBuf::from("data/models");

        // Check episode count from episode store
        let stats = self.episode_store.kelly_trade_stats(1000);
        let episode_count = stats.trade_count as usize;

        // List deployed models
        let mut models = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(meta) = entry.metadata() {
                        let modified = meta
                            .modified()
                            .map(|t| {
                                let duration = t.elapsed().unwrap_or_default();
                                format!("{}h ago", duration.as_secs() / 3600)
                            })
                            .unwrap_or_else(|_| "unknown".to_string());

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

        // Check if retraining is needed
        let last_count = 0usize; // Would read from persistent state
        let new_episodes = episode_count.saturating_sub(last_count);
        let needs_retrain = new_episodes >= 50 && episode_count >= 100;

        if needs_retrain {
            println!(
                "[Evolution] {} new episodes — triggering ML training",
                new_episodes
            );
            self.trigger_training(&models_dir).await;
        }

        let training_queue = if needs_retrain { 1 } else { 0 };
        let next_retrain = if new_episodes >= 50 {
            0
        } else {
            (50 - new_episodes) as u64 * 60
        };

        // Emit COT event
        let _ = self
            .cot_tx
            .send(AgentOutputEvent::Cot {
                agent: "Evolution".to_string(),
                symbol: "ALL".to_string(),
                action: if needs_retrain {
                    "TRAINING".to_string()
                } else {
                    "IDLE".to_string()
                },
                reason: format!(
                    "Episodes: {}, models: {}, needs_retrain: {}",
                    episode_count,
                    models.len(),
                    needs_retrain
                ),
                confidence: 0.8,
            });

        EvolutionStatus {
            episodes_collected: episode_count,
            models_deployed: models,
            weight_adjustments: 0,
            last_improvement: None,
            training_queue_depth: training_queue,
            next_retrain_in: next_retrain,
        }
    }

    /// Trigger model training (background task).
    async fn trigger_training(&self, models_dir: &PathBuf) {
        println!("[Evolution] Starting ML model training...");
        let db_path = PathBuf::from(cotrader_core::StorageConfig::default().main_db());
        let trainer = cotrader_ml::training::trainer::Trainer::new(models_dir, &db_path);

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
        println!(
            "[Evolution] Training complete — {} episodes used",
            report.total_episodes
        );
    }

    /// Adjust skill weights based on recent performance using AttributionEngine.
    pub async fn adjust_weights(&self, frame: &CacheFrame) {
        use crate::weight_tuner::AttributionEngine;
        
        let stats = &frame.daily_stats;
        let total = stats.winning_trades_today + stats.losing_trades_today;
        let win_rate = if total > 0 {
            stats.winning_trades_today as f64 / total as f64
        } else {
            0.0
        };

        // Initialize AttributionEngine for weight tuning
        let engine = AttributionEngine::new(0.05); // 5% learning rate
        
        // Build skill predictions from daily stats
        let mut skill_predictions = std::collections::HashMap::new();
        skill_predictions.insert("Analysis".to_string(), win_rate);
        skill_predictions.insert("Planning".to_string(), win_rate * 0.9);
        skill_predictions.insert("Decision".to_string(), win_rate * 0.95);
        skill_predictions.insert("Risk".to_string(), 1.0 - win_rate);
        
        // Build active weights from frame
        let mut active_weights = std::collections::HashMap::new();
        active_weights.insert("Analysis".to_string(), 0.25);
        active_weights.insert("Planning".to_string(), 0.25);
        active_weights.insert("Decision".to_string(), 0.25);
        active_weights.insert("Risk".to_string(), 0.25);
        
        // Apply weight tuning
        let snapshot = engine.tune_skill_weights(
            "current_episode",
            stats.daily_pnl,
            "BUY", // Simplified - would come from trade context
            &skill_predictions,
            &active_weights,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
        
        // Log the adjustments
        println!(
            "[Evolution] Weight adjustments applied: win_rate={:.0}%",
            win_rate * 100.0
        );
        for (skill, weight) in &snapshot.updated_weights {
            println!("  → {}: {:.3}", skill, weight);
        }
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
                &status
                    .models_deployed
                    .iter()
                    .map(|m| format!("{} v{}", m.name, m.version))
                    .collect::<Vec<_>>()
                    .join(", "),
                status.models_deployed.iter().map(|m| m.name.clone()).collect(),
                0.8,
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

        chain.finalize(&format!(
            "Evolution: {} episodes, {} models, {} adjustments",
            status.episodes_collected,
            status.models_deployed.len(),
            status.weight_adjustments
        ));

        chain
    }
}
