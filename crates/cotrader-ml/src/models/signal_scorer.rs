//! Signal Scorer — MLP model for predicting trade profitability.
//!
//! Architecture: Input(34) → Hidden(128, ReLU) → Hidden(64, ReLU) → Hidden(32, ReLU) → Output(1, Sigmoid)

use candle_core::{Device, Tensor};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct ModelWeights {
    w1: Vec<Vec<f32>>,
    b1: Vec<f32>,
    w2: Vec<Vec<f32>>,
    b2: Vec<f32>,
    w3: Vec<Vec<f32>>,
    b3: Vec<f32>,
}

pub struct SignalScorerModel {
    weights: ModelWeights,
    device: Device,
}

impl SignalScorerModel {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data = std::fs::read_to_string(path)?;
        let weights: ModelWeights = serde_json::from_str(&data)?;
        Ok(Self { weights, device: Device::Cpu })
    }

    pub fn predict(&self, features: &[f64]) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let input: Vec<f32> = features.iter().take(34).map(|&x| x as f32).collect();
        let x = Tensor::new(input.as_slice(), &self.device)?.unsqueeze(0)?;

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
        let prob = candle_nn::ops::sigmoid(&logits)?;

        let prob_vec: Vec<f32> = prob.to_vec1()?;
        Ok(prob_vec[0] as f64)
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = serde_json::to_string_pretty(&self.weights)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}
