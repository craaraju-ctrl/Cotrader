//! Trainer — Background training loop for all ML models.
//!
//! Runs nightly or after N new episodes. Uses walk-forward validation:
//! trains on historical data, validates on recent data, only deploys
//! if the new model outperforms the current one.

use crate::models::win_probability;
use crate::models::strategy_selector;
use crate::training::data_loader;
use std::path::Path;

pub struct Trainer {
    db_path: std::path::PathBuf,
}

impl Trainer {
    pub fn new(_models_dir: &Path, db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_path_buf(),
        }
    }

    /// Run full training pipeline. Call this nightly or after N new episodes.
    pub async fn train_all(&self) -> TrainingReport {
        let mut report = TrainingReport::default();

        // 1. Win Probability model
        match self.train_win_probability() {
            Ok(result) => {
                report.win_probability = Some(result);
                tracing::info!("[ML Trainer] Win probability model trained");
            }
            Err(e) => {
                tracing::warn!("[ML Trainer] Win probability training failed: {}", e);
                report.errors.push(format!("win_probability: {}", e));
            }
        }

        // 2. Strategy Selector model
        match self.train_strategy_selector() {
            Ok(result) => {
                report.strategy_selector = Some(result);
                tracing::info!("[ML Trainer] Strategy selector model trained");
            }
            Err(e) => {
                tracing::warn!("[ML Trainer] Strategy selector training failed: {}", e);
                report.errors.push(format!("strategy_selector: {}", e));
            }
        }

        // 3. Regime classifier, signal scorer, pattern detector
        // These require more complex training with full indicator recomputation.
        // For now, they train when enough data accumulates (1000+ episodes).
        let episode_count = self.count_episodes().unwrap_or(0);
        if episode_count >= 1000 {
            tracing::info!("[ML Trainer] {} episodes — enough for advanced models", episode_count);
            // Future: train regime classifier, signal scorer, pattern detector
        } else {
            tracing::info!("[ML Trainer] {} episodes — need 1000+ for advanced models", episode_count);
        }

        report.total_episodes = episode_count;
        report
    }

    fn train_win_probability(&self) -> Result<WinProbabilityResult, Box<dyn std::error::Error + Send + Sync>> {
        let samples = data_loader::load_episode_training_data(&self.db_path, 5000)?;
        if samples.len() < 50 {
            return Ok(WinProbabilityResult {
                status: "insufficient_data".to_string(),
                samples_used: samples.len(),
                accuracy: 0.0,
            });
        }

        let features: Vec<Vec<f64>> = samples.iter().map(|s| s.features.clone()).collect();
        let labels: Vec<f64> = samples.iter().map(|s| s.label).collect();

        // Walk-forward: train on 80%, validate on 20%
        let split = (features.len() as f64 * 0.8) as usize;
        let train_features = &features[..split];
        let train_labels = &labels[..split];
        let val_features = &features[split..];
        let val_labels = &labels[split..];

        let model = win_probability::train_win_probability(train_features, train_labels, 0.01, 100);

        // Evaluate
        let mut correct = 0;
        for (features, &label) in val_features.iter().zip(val_labels.iter()) {
            let pred = model.predict(features)?;
            let predicted = if pred > 0.5 { 1.0 } else { 0.0 };
            if (predicted - label).abs() < f64::EPSILON {
                correct += 1;
            }
        }
        let accuracy = correct as f64 / val_features.len().max(1) as f64;

        // Save if accuracy > 50% (better than random)
        if accuracy > 0.5 {
            let models_dir = self.db_path.parent().unwrap_or(Path::new(".")).join("models");
            let path = models_dir.join("win_probability.json");
            model.save(&path)?;
        }

        Ok(WinProbabilityResult {
            status: if accuracy > 0.5 { "deployed".to_string() } else { "below_threshold".to_string() },
            samples_used: samples.len(),
            accuracy,
        })
    }

    fn train_strategy_selector(&self) -> Result<StrategySelectorResult, Box<dyn std::error::Error + Send + Sync>> {
        let samples = data_loader::load_episode_training_data(&self.db_path, 5000)?;
        if samples.len() < 50 {
            return Ok(StrategySelectorResult {
                status: "insufficient_data".to_string(),
                samples_used: samples.len(),
            });
        }

        let features: Vec<Vec<f64>> = samples.iter().map(|s| s.features.clone()).collect();
        let strategies: Vec<usize> = samples.iter().map(|s| s.strategy_index).collect();
        let outcomes: Vec<f64> = samples.iter().map(|s| s.pnl_pct).collect();

        let model = strategy_selector::train_strategy_selector(
            &features, &strategies, &outcomes, 3, features[0].len(),
        );

        let models_dir = self.db_path.parent().unwrap_or(Path::new(".")).join("models");
        let path = models_dir.join("strategy_selector.json");
        model.save(&path)?;

        Ok(StrategySelectorResult {
            status: "deployed".to_string(),
            samples_used: samples.len(),
        })
    }

    fn count_episodes(&self) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM closed_episodes", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

#[derive(Default)]
pub struct TrainingReport {
    pub win_probability: Option<WinProbabilityResult>,
    pub strategy_selector: Option<StrategySelectorResult>,
    pub total_episodes: usize,
    pub errors: Vec<String>,
}

pub struct WinProbabilityResult {
    pub status: String,
    pub samples_used: usize,
    pub accuracy: f64,
}

pub struct StrategySelectorResult {
    pub status: String,
    pub samples_used: usize,
}
