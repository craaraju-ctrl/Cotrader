//! ModelStore — Save/load ML models to disk.

use crate::models::regime_classifier::RegimeClassifierModel;
use crate::models::signal_scorer::SignalScorerModel;
use crate::models::win_probability::WinProbabilityModel;
use crate::models::pattern_detector::PatternDetectorModel;
use crate::models::strategy_selector::StrategySelectorModel;
use std::path::{Path, PathBuf};

pub struct ModelStore {
    models_dir: PathBuf,
}

impl ModelStore {
    pub fn new(models_dir: impl AsRef<Path>) -> Self {
        let models_dir = models_dir.as_ref().to_path_buf();
        // Ensure models directory exists
        let _ = std::fs::create_dir_all(&models_dir);
        Self { models_dir }
    }

    pub fn load_regime_classifier(&self) -> Result<RegimeClassifierModel, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.models_dir.join("regime_classifier.safetensors");
        if path.exists() {
            RegimeClassifierModel::load(&path)
        } else {
            Err("No regime classifier model found".into())
        }
    }

    pub fn load_signal_scorer(&self) -> Result<SignalScorerModel, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.models_dir.join("signal_scorer.safetensors");
        if path.exists() {
            SignalScorerModel::load(&path)
        } else {
            Err("No signal scorer model found".into())
        }
    }

    pub fn load_win_probability(&self) -> Result<WinProbabilityModel, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.models_dir.join("win_probability.json");
        if path.exists() {
            WinProbabilityModel::load(&path)
        } else {
            Err("No win probability model found".into())
        }
    }

    pub fn load_pattern_detector(&self) -> Result<PatternDetectorModel, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.models_dir.join("pattern_detector.safetensors");
        if path.exists() {
            PatternDetectorModel::load(&path)
        } else {
            Err("No pattern detector model found".into())
        }
    }

    pub fn load_strategy_selector(&self) -> Result<StrategySelectorModel, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.models_dir.join("strategy_selector.json");
        if path.exists() {
            StrategySelectorModel::load(&path)
        } else {
            Err("No strategy selector model found".into())
        }
    }

    pub fn list_models(&self) -> Vec<String> {
        let mut models = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    models.push(name.to_string());
                }
            }
        }
        models
    }
}
