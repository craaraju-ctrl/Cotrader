//! Background ML Training Loop
//!
//! Runs periodically to retrain ML models from accumulated trade data.
//! Triggered after N new episodes or on a nightly schedule.
//! Models only deploy if they outperform the current model on validation data.

use crate::training::trainer::{Trainer, TrainingReport};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Background ML trainer that runs periodically.
pub struct BackgroundTrainer {
    trainer: Trainer,
    models_dir: PathBuf,
    db_path: PathBuf,
    last_episode_count: Arc<AtomicU64>,
    min_new_episodes: u64,      // retrain after this many new episodes
    min_total_episodes: u64,    // minimum episodes before any training
}

impl BackgroundTrainer {
    pub fn new(models_dir: &Path, db_path: &Path) -> Self {
        Self {
            trainer: Trainer::new(models_dir, db_path),
            models_dir: models_dir.to_path_buf(),
            db_path: db_path.to_path_buf(),
            last_episode_count: Arc::new(AtomicU64::new(0)),
            min_new_episodes: 50,     // retrain after 50 new trades
            min_total_episodes: 100,  // need at least 100 trades to start
        }
    }

    /// Check if retraining is needed based on episode count.
    pub fn should_retrain(&self) -> bool {
        let current_count = self.count_episodes().unwrap_or(0);
        let last_count = self.last_episode_count.load(Ordering::Relaxed);

        if current_count < self.min_total_episodes {
            return false;
        }

        let new_episodes = current_count.saturating_sub(last_count);
        new_episodes >= self.min_new_episodes
    }

    /// Run training and update the last episode count.
    pub async fn train(&mut self) -> TrainingReport {
        let report = self.trainer.train_all().await;
        let current_count = self.count_episodes().unwrap_or(0);
        self.last_episode_count.store(current_count, Ordering::Relaxed);
        report
    }

    /// Count total episodes in the database.
    fn count_episodes(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM closed_episodes", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// List deployed model files with their sizes.
    pub fn list_deployed_models(&self) -> Vec<(String, u64)> {
        let mut models = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(meta) = entry.metadata() {
                        models.push((name.to_string(), meta.len()));
                    }
                }
            }
        }
        models
    }
}

/// Start the background ML training loop.
/// Spawns a tokio task that checks for retraining every 30 minutes.
pub fn start_background_ml_training(
    models_dir: PathBuf,
    db_path: PathBuf,
) {
    tokio::spawn(async move {
        // Initial delay: let the system settle and accumulate some trades
        tokio::time::sleep(std::time::Duration::from_secs(300)).await; // 5 minutes

        let mut trainer = BackgroundTrainer::new(&models_dir, &db_path);
        println!("[ML Trainer] Background training loop started (models_dir={:?})", models_dir);

        loop {
            // Check every 30 minutes
            tokio::time::sleep(std::time::Duration::from_secs(1800)).await;

            if trainer.should_retrain() {
                println!("[ML Trainer] New episodes detected — starting retraining...");
                let report = trainer.train().await;

                // Log results
                if let Some(wp) = &report.win_probability {
                    println!(
                        "[ML Trainer] Win probability model: status={}, accuracy={:.1}%, samples={}",
                        wp.status, wp.accuracy * 100.0, wp.samples_used
                    );
                }
                if let Some(ss) = &report.strategy_selector {
                    println!(
                        "[ML Trainer] Strategy selector: status={}, samples={}",
                        ss.status, ss.samples_used
                    );
                }
                if !report.errors.is_empty() {
                    for err in &report.errors {
                        println!("[ML Trainer] Error: {}", err);
                    }
                }

                // List deployed models
                let models = trainer.list_deployed_models();
                if !models.is_empty() {
                    println!("[ML Trainer] Deployed models:");
                    for (name, size) in &models {
                        println!("  {} ({} bytes)", name, size);
                    }
                }
            }
        }
    });
}
