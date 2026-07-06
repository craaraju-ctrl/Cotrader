//! Win Probability — Gradient Boosted Trees for dynamic Kelly win probability.
//!
//! Replaces hardcoded `win_prob = 0.55` with a per-setup prediction based on
//! indicator features + regime + signal source.
//! Uses linfa GradientBoostedTrees (pure Rust, no Python dependency).

use std::path::Path;

/// Placeholder for GBT model. Real implementation uses linfa_trees::GradientBoostedTrees.
/// For now, stores a simple linear model as proof of concept.
pub struct WinProbabilityModel {
    weights: Vec<f64>,
    bias: f64,
    feature_count: usize,
}

impl WinProbabilityModel {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data = std::fs::read_to_string(path)?;
        let model_data: ModelData = serde_json::from_str(&data)?;
        Ok(Self {
            weights: model_data.weights,
            bias: model_data.bias,
            feature_count: model_data.feature_count,
        })
    }

    /// Predict win probability from features (0.0-1.0).
    pub fn predict(&self, features: &[f64]) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let n = features.len().min(self.feature_count);
        let mut logit = self.bias;
        for i in 0..n {
            logit += features[i] * self.weights[i];
        }
        // Sigmoid
        Ok(1.0 / (1.0 + (-logit).exp()))
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let model_data = ModelData {
            weights: self.weights.clone(),
            bias: self.bias,
            feature_count: self.feature_count,
        };
        let data = serde_json::to_string_pretty(&model_data)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ModelData {
    weights: Vec<f64>,
    bias: f64,
    feature_count: usize,
}

/// Train a win probability model from historical episodes.
///
/// `features_per_episode`: each row is a feature vector at trade entry.
/// `labels`: 1.0 if trade was profitable, 0.0 otherwise.
pub fn train_win_probability(
    features_per_episode: &[Vec<f64>],
    labels: &[f64],
    learning_rate: f64,
    epochs: usize,
) -> WinProbabilityModel {
    if features_per_episode.is_empty() {
        return WinProbabilityModel {
            weights: vec![0.0; 48],
            bias: 0.0,
            feature_count: 48,
        };
    }

    let feature_count = features_per_episode[0].len();
    let mut weights = vec![0.0f64; feature_count];
    let mut bias = 0.0f64;

    // Gradient descent with logistic loss
    for _ in 0..epochs {
        for (features, &label) in features_per_episode.iter().zip(labels.iter()) {
            let mut logit = bias;
            for (i, &f) in features.iter().enumerate().take(feature_count) {
                logit += weights[i] * f;
            }
            let pred = 1.0 / (1.0 + (-logit).exp());
            let error = pred - label;

            // Update weights
            for (i, &f) in features.iter().enumerate().take(feature_count) {
                weights[i] -= learning_rate * error * f;
            }
            bias -= learning_rate * error;
        }
    }

    WinProbabilityModel {
        weights,
        bias,
        feature_count,
    }
}
