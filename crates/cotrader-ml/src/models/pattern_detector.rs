//! Pattern Detector — 1D-CNN for complex multi-candle pattern recognition.
//!
//! Architecture: Conv1D(5→32, k=3) → ReLU → Conv1D(32→64, k=3) → ReLU → GlobalAvgPool → FC(64→32) → Output(4)

use candle_core::{Device, Tensor};
use candle_nn::{Conv1d, Conv1dConfig, Module};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const PATTERN_WINDOW: usize = 20;
pub const PATTERN_CLASSES: usize = 4;

#[derive(Serialize, Deserialize)]
struct ConvWeights {
    weight: Vec<Vec<Vec<f32>>>,
    bias: Vec<f32>,
}

#[derive(Serialize, Deserialize)]
struct LinearWeights {
    weight: Vec<Vec<f32>>,
    bias: Vec<f32>,
}

#[derive(Serialize, Deserialize)]
struct ModelData {
    conv1: ConvWeights,
    conv2: ConvWeights,
    fc1: LinearWeights,
    fc2: LinearWeights,
}

pub struct PatternDetectorModel {
    conv1: Conv1d,
    conv2: Conv1d,
    fc1_w: Tensor,
    fc1_b: Tensor,
    fc2_w: Tensor,
    fc2_b: Tensor,
    device: Device,
}

impl PatternDetectorModel {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data = std::fs::read_to_string(path)?;
        let model: ModelData = serde_json::from_str(&data)?;
        let device = Device::Cpu;

        // Flatten conv weights for tensor creation
        let c1w_flat: Vec<f32> = model.conv1.weight.iter().flatten().flatten().copied().collect();
        let c1w = Tensor::new(c1w_flat.as_slice(), &device)?
            .reshape((model.conv1.weight.len(), model.conv1.weight[0].len(), model.conv1.weight[0][0].len()))?;
        let c1b = Tensor::new(model.conv1.bias.as_slice(), &device)?;
        let config = Conv1dConfig { padding: 1, stride: 1, dilation: 1, groups: 1 };
        let conv1 = Conv1d::new(c1w, Some(c1b), config);

        let c2w_flat: Vec<f32> = model.conv2.weight.iter().flatten().flatten().copied().collect();
        let c2w = Tensor::new(c2w_flat.as_slice(), &device)?
            .reshape((model.conv2.weight.len(), model.conv2.weight[0].len(), model.conv2.weight[0][0].len()))?;
        let c2b = Tensor::new(model.conv2.bias.as_slice(), &device)?;
        let conv2 = Conv1d::new(c2w, Some(c2b), config);

        let fc1_w_flat: Vec<f32> = model.fc1.weight.iter().flatten().copied().collect();
        let fc1_w = Tensor::new(fc1_w_flat.as_slice(), &device)?
            .reshape((model.fc1.weight.len(), model.fc1.weight[0].len()))?;
        let fc1_b = Tensor::new(model.fc1.bias.as_slice(), &device)?;

        let fc2_w_flat: Vec<f32> = model.fc2.weight.iter().flatten().copied().collect();
        let fc2_w = Tensor::new(fc2_w_flat.as_slice(), &device)?
            .reshape((model.fc2.weight.len(), model.fc2.weight[0].len()))?;
        let fc2_b = Tensor::new(model.fc2.bias.as_slice(), &device)?;

        Ok(Self { conv1, conv2, fc1_w, fc1_b, fc2_w, fc2_b, device })
    }

    pub fn predict(&self, ohlcv_features: &[f64]) -> Result<(String, f64), Box<dyn std::error::Error + Send + Sync>> {
        let window = PATTERN_WINDOW;
        let input: Vec<f32> = ohlcv_features.iter().map(|&x| x as f32).collect();
        let x = Tensor::new(input.as_slice(), &self.device)?
            .reshape((1usize, 5usize, window))?;

        let h1 = self.conv1.forward(&x)?.relu()?;
        let h2 = self.conv2.forward(&h1)?.relu()?;

        // Global average pooling over time dimension
        let pooled = h2.mean(2)?;

        // FC layers
        let h3 = pooled.matmul(&self.fc1_w.t()?)?.broadcast_add(&self.fc1_b)?.relu()?;
        let logits = h3.matmul(&self.fc2_w.t()?)?.broadcast_add(&self.fc2_b)?;

        let probs = candle_nn::ops::softmax(&logits, 1)?;
        let probs_vec: Vec<f32> = probs.to_vec1()?;

        let (best_idx, &best_prob) = probs_vec.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));

        let direction = match best_idx {
            0 => "StrongBullish",
            1 => "WeakBullish",
            2 => "WeakBearish",
            3 => "StrongBearish",
            _ => "Neutral",
        };

        Ok((direction.to_string(), best_prob as f64))
    }

    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let c1w: Vec<Vec<Vec<f32>>> = self.conv1.weight().to_vec3()?;
        let c1b: Vec<f32> = self.conv1.bias().map(|b| b.to_vec1()).transpose()?.unwrap_or_default();
        let c2w: Vec<Vec<Vec<f32>>> = self.conv2.weight().to_vec3()?;
        let c2b: Vec<f32> = self.conv2.bias().map(|b| b.to_vec1()).transpose()?.unwrap_or_default();

        let model = ModelData {
            conv1: ConvWeights { weight: c1w, bias: c1b },
            conv2: ConvWeights { weight: c2w, bias: c2b },
            fc1: LinearWeights { weight: self.fc1_w.to_vec2()?, bias: self.fc1_b.to_vec1()? },
            fc2: LinearWeights { weight: self.fc2_w.to_vec2()?, bias: self.fc2_b.to_vec1()? },
        };

        let data = serde_json::to_string_pretty(&model)?;
        std::fs::write(path, data)?;
        Ok(())
    }
}
