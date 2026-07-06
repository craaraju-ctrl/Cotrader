//! Regime Classifier — MLP model for market regime detection.
//!
//! Architecture: Input(30) → Hidden(64, ReLU) → Hidden(32, ReLU) → Output(5, Softmax)
//! Output: [P(TrendingBull), P(TrendingBear), P(Ranging), P(Volatile), P(LowLiquidity)]

use candle_core::{Device, Tensor};
use cotrader_core::MarketRegime;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct ModelWeights {
    w1: Vec<Vec<f32>>, // (64, 30)
    b1: Vec<f32>,      // (64)
    w2: Vec<Vec<f32>>, // (32, 64)
    b2: Vec<f32>,      // (32)
    w3: Vec<Vec<f32>>, // (5, 32)
    b3: Vec<f32>,      // (5)
}

pub struct RegimeClassifierModel {
    weights: ModelWeights,
    device: Device,
}

impl RegimeClassifierModel {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data = std::fs::read_to_string(path)?;
        let weights: ModelWeights = serde_json::from_str(&data)?;
        Ok(Self { weights, device: Device::Cpu })
    }

    /// Predict regime from feature vector (first 30 features).
    pub fn predict(&self, features: &[f64]) -> Result<(MarketRegime, f64), Box<dyn std::error::Error + Send + Sync>> {
        let input: Vec<f32> = features.iter().take(30).map(|&x| x as f32).collect();
        let x = Tensor::new(input.as_slice(), &self.device)?.unsqueeze(0)?;

        // Flatten weight matrices for Tensor::new (NdArray requires contiguous data)
        let w1_flat: Vec<f32> = self.weights.w1.iter().flatten().copied().collect();
        let w1 = Tensor::new(w1_flat.as_slice(), &self.device)?
            .reshape((self.weights.w1.len(), self.weights.w1[0].len()))?;
        let b1 = Tensor::new(self.weights.b1.as_slice(), &self.device)?;
        let h1 = x.matmul(&w1.t()?)?.broadcast_add(&b1)?.relu()?;

        let w2_flat: Vec<f32> = self.weights.w2.iter().flatten().copied().collect();
        let w2 = Tensor::new(w2_flat.as_slice(), &self.device)?
            .reshape((self.weights.w2.len(), self.weights.w2[0].len()))?;
        let b2 = Tensor::new(self.weights.b2.as_slice(), &self.device)?;
        let h2 = h1.matmul(&w2.t()?)?.broadcast_add(&b2)?.relu()?;

        let w3_flat: Vec<f32> = self.weights.w3.iter().flatten().copied().collect();
        let w3 = Tensor::new(w3_flat.as_slice(), &self.device)?
            .reshape((self.weights.w3.len(), self.weights.w3[0].len()))?;
        let b3 = Tensor::new(self.weights.b3.as_slice(), &self.device)?;
        let logits = h2.matmul(&w3.t()?)?.broadcast_add(&b3)?;
        let probs = candle_nn::ops::softmax(&logits, 1)?;

        let probs_vec: Vec<f32> = probs.to_vec1()?;
        let (best_idx, &best_prob) = probs_vec.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((2, &0.0));

        let regime = match best_idx {
            0 => MarketRegime::TrendingBull,
            1 => MarketRegime::TrendingBear,
            2 => MarketRegime::Ranging,
            3 => MarketRegime::Volatile,
            4 => MarketRegime::LowLiquidity,
            _ => MarketRegime::Ranging,
        };

        Ok((regime, best_prob as f64))
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = serde_json::to_string_pretty(&self.weights)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn new_random(input_dim: usize, hidden1: usize, hidden2: usize, output_dim: usize) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let w1: Vec<Vec<f32>> = (0..hidden1).map(|_| (0..input_dim).map(|_| rng.gen_range(-0.1..0.1)).collect()).collect();
        let b1: Vec<f32> = vec![0.0; hidden1];
        let w2: Vec<Vec<f32>> = (0..hidden2).map(|_| (0..hidden1).map(|_| rng.gen_range(-0.1..0.1)).collect()).collect();
        let b2: Vec<f32> = vec![0.0; hidden2];
        let w3: Vec<Vec<f32>> = (0..output_dim).map(|_| (0..hidden2).map(|_| rng.gen_range(-0.1..0.1)).collect()).collect();
        let b3: Vec<f32> = vec![0.0; output_dim];
        Self { weights: ModelWeights { w1, b1, w2, b2, w3, b3 }, device: Device::Cpu }
    }
}
