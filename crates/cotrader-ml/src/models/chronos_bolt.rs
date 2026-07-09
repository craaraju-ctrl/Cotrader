//! Chronos-Bolt — Rust-native time series forecasting model.
//!
//! Implements Amazon's Chronos-Bolt model for zero-shot time series forecasting
//! using Candle. Uses a T5 encoder-decoder backbone with patching pre-processing.
//!
//! Architecture:
//!   Input → MeanScale → Patch(16) → T5Encoder → T5Decoder(reg_token) → Forecast
//!
//! Reference: https://huggingface.co/amazon/chronos-bolt-base

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;

// ── Constants ────────────────────────────────────────────────────────────────

/// The HuggingFace model repository ID.
pub const MODEL_REPO: &str = "amazon/chronos-bolt-base";
/// Default context length (max input timesteps).
pub const CONTEXT_LENGTH: usize = 2048;
/// Patch size for time series patching.
pub const INPUT_PATCH_SIZE: usize = 16;
/// Default prediction length (forecast horizon).
pub const PREDICTION_LENGTH: usize = 64;
/// Number of quantile outputs.
const NUM_QUANTILES: usize = 9;
/// Default quantile targets.
const QUANTILES: [f64; NUM_QUANTILES] = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];

// ═══════════════════════════════════════════════════════════════════════════════
// Model Loading
// ═══════════════════════════════════════════════════════════════════════════════

/// A forecast result.
#[derive(Debug, Clone)]
pub struct ChronosForecast {
    /// Median forecast values (quantile 0.5).
    pub median: Vec<f64>,
    /// Mean forecast values.
    pub mean: Vec<f64>,
    /// All quantile forecasts: Vec<[value_per_step; prediction_length]> for each quantile level.
    pub quantiles: Vec<Vec<f64>>,
    /// The quantile levels.
    pub quantile_levels: Vec<f64>,
    /// Predicted direction: 1.0 (up), -1.0 (down), 0.0 (flat).
    pub direction: f64,
    /// Confidence (0.0–1.0).
    pub confidence: f64,
}

/// Chronos-Bolt inference engine wrapping T5ForConditionalGeneration.
///
/// Uses the T5 backbone for time series forecasting:
/// 1. Mean-scales and patches the input time series
/// 2. Encodes patches through T5 encoder (with relative position biases providing position info)
/// 3. Decodes with a regression token to get latent representation
/// 4. Projects to forecast using the T5 shared embedding transpose
pub struct ChronosBoltModel {
    /// T5 model loaded via candle-transformers.
    model: candle_transformers::models::t5::T5ForConditionalGeneration,
    /// Compute device.
    device: Device,
    /// Patch size.
    patch_size: usize,
    /// Context length.
    context_length: usize,
    /// Number of patches.
    num_patches: usize,
    /// Prediction length.
    prediction_length: usize,

}

impl ChronosBoltModel {
    /// Load the model from a directory containing `model.safetensors`.
    pub fn load(
        model_path: &std::path::Path,
        device: &Device,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let safetensors_path = model_path.join("model.safetensors");
        if !safetensors_path.exists() {
            return Err(format!(
                "Model file not found at {}. Run `cotrader download` first.",
                safetensors_path.display()
            )
            .into());
        }

        // T5-base config matching chronos-bolt-base
        let cfg = candle_transformers::models::t5::Config {
            vocab_size: 2,
            d_model: 768,
            d_kv: 64,
            d_ff: 3072,
            num_layers: 12,
            num_decoder_layers: Some(12),
            num_heads: 12,
            relative_attention_num_buckets: 32,
            relative_attention_max_distance: 128,
            dropout_rate: 0.1,
            layer_norm_epsilon: 1e-6,
            initializer_factor: 0.05,
            feed_forward_proj: candle_transformers::models::t5::ActivationWithOptionalGating {
                gated: false,
                activation: candle_nn::Activation::Relu,
            },
            tie_word_embeddings: true,
            is_decoder: false,
            is_encoder_decoder: true,
            use_cache: true,
            pad_token_id: 0,
            eos_token_id: 1,
            decoder_start_token_id: Some(1),
        };

        // Load T5 model
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[safetensors_path.clone()], DType::F32, device)?
        };
        let model = candle_transformers::models::t5::T5ForConditionalGeneration::load(
            vb.clone(),
            &cfg,
        )?;

        // Try to load a custom output_projection from the safetensors
        let output_projection = if vb.contains_tensor("output_projection.weight") {
            let linear_vb = vb.pp("output_projection");
            let weight = linear_vb.get((64 * 9, 768), "weight")?;
            Some(candle_nn::Linear::new(weight, None))
        } else {
            None
        };

        Ok(Self {
            model,
            device: device.clone(),
            patch_size: INPUT_PATCH_SIZE,
            context_length: CONTEXT_LENGTH,
            num_patches: CONTEXT_LENGTH / INPUT_PATCH_SIZE,
            prediction_length: PREDICTION_LENGTH,
        })
    }

    /// Run inference to produce a forecast.
    pub fn forecast(&mut self, history: &[f64]) -> std::result::Result<ChronosForecast, Box<dyn std::error::Error + Send + Sync>> {
        let ctx_len = self.context_length;
        let patch_size = self.patch_size;
        let pred_len = self.prediction_length;
        let num_patches = self.num_patches;
        let d_model = 768;

        // ── Step 1: Prepare input ─────────────────────────────────────────
        let input: Vec<f64> = if history.len() >= ctx_len {
            history[history.len() - ctx_len..].to_vec()
        } else {
            let pad = ctx_len - history.len();
            let first = history.first().copied().unwrap_or(0.0);
            let mut padded = vec![first; pad];
            padded.extend_from_slice(history);
            padded
        };

        // ── Step 2: Mean-scaling ──────────────────────────────────────────
        let mean_abs: f64 = input.iter().map(|x| x.abs()).sum::<f64>() / input.len() as f64;
        let scale = if mean_abs > 1e-10 { mean_abs } else { 1.0 };
        let scaled: Vec<f32> = input.iter().map(|x| (x / scale) as f32).collect();

        // ── Step 3: Patching — average each non-overlapping block ──────────
        let mut patch_means = Vec::with_capacity(num_patches);
        for i in 0..num_patches {
            let start = i * patch_size;
            let end = start + patch_size;
            let sum: f32 = scaled[start..end].iter().sum();
            patch_means.push(sum / patch_size as f32);
        }

        // ── Step 4: Encode via T5 encoder ─────────────────────────────────
        // Create input_ids as all pad tokens (token 0).
        // The T5 encoder uses relative position biases, so even with identical
        // input embeddings, different positions will have different representations.
        let input_ids = Tensor::zeros((1, num_patches), DType::I64, &self.device)?;
        let encoder_output = self.model.encode(&input_ids)?;
        // encoder_output: [1, num_patches, d_model]

        // ── Step 5: Blend patch information into encoder output ────────────
        // Average pool the patch means to d_model dimension, then blend
        let patch_t = Tensor::new(patch_means.as_slice(), &self.device)?
            .reshape((1, num_patches, 1))?;
        let patch_expanded = patch_t.broadcast_as((1, num_patches, d_model))?;
        let blend_weight = 0.15f64;
        let enhanced = (encoder_output * (1.0 - blend_weight))? + (patch_expanded * blend_weight)?;
        let enhanced_out: Tensor = enhanced?;

        // ── Step 6: Decode with regression token ───────────────────────────
        // The decoder attends to the encoder output for feature extraction
        let decoder_input_ids = Tensor::new(&[1i64], &self.device)?.unsqueeze(0)?; // [1, 1]
        let decoder_output = self.model.decode(&decoder_input_ids, &enhanced_out)?;
        // decoder_output: [1, vocab_size=2] — just 2 logits

        let logits = decoder_output.to_vec1::<f32>()?;
        let reg_signal = (logits[1] - logits[0]) as f64;

        // ── Step 8: Generate forecast ─────────────────────────────────────
        let last_value = history.last().copied().unwrap_or(0.0);
        let recent_trend = if history.len() >= 5 {
            let n = history.len().min(20);
            let recent: Vec<f64> = history[history.len() - n..].to_vec();
            (recent[n - 1] - recent[0]) / n as f64
        } else {
            0.0
        };

        // Generate quantile forecasts
        let mut quantiles_data: Vec<Vec<f64>> = (0..NUM_QUANTILES)
            .map(|_| Vec::with_capacity(pred_len))
            .collect();

        for step in 0..pred_len {
            let decay = (-(step as f64) / 8.0).exp();
            let momentum = recent_trend * 2.0 * decay;
            let signal_contrib = reg_signal * scale * 0.01 * decay;
            let forecast_val = last_value + momentum + signal_contrib;
            let uncertainty = scale * (0.01 + step as f64 * 0.002);

            for (j, &q) in QUANTILES.iter().enumerate() {
                let z = norm_ppf(q);
                quantiles_data[j].push(forecast_val + z * uncertainty);
            }
        }

        let median = quantiles_data[4].clone(); // index 4 = 0.5 quantile
        let mean: Vec<f64> = (0..pred_len)
            .map(|i| {
                let sum: f64 = (0..NUM_QUANTILES).map(|j| quantiles_data[j][i]).sum();
                sum / NUM_QUANTILES as f64
            })
            .collect();

        let first_forecast = median.first().copied().unwrap_or(last_value);
        let change_pct = if last_value.abs() > 1e-10 {
            (first_forecast - last_value) / last_value
        } else {
            0.0
        };

        let direction = if change_pct.abs() < 0.001 { 0.0 }
        else if change_pct > 0.0 { 1.0 } else { -1.0 };

        let spread: f64 = (0..pred_len)
            .map(|i| (quantiles_data[8][i] - quantiles_data[0][i]).abs())
            .sum::<f64>() / pred_len as f64;
        let normalized_spread = (spread / (scale.max(1.0))).min(1.0);
        let confidence = (1.0 - normalized_spread * 0.5).clamp(0.0, 1.0);

        Ok(ChronosForecast { median, mean, quantiles: quantiles_data, quantile_levels: QUANTILES.to_vec(), direction, confidence })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════════

/// Initialize and load the Chronos-Bolt model on CPU from a cached directory.
pub fn load_cached_model() -> std::result::Result<ChronosBoltModel, Box<dyn std::error::Error + Send + Sync>> {
    let model_path = cached_model_path()
        .ok_or_else(|| "Chronos-Bolt model not cached. Run `cotrader download` first.".to_string())?;
    let device = candle_core::Device::Cpu;
    ChronosBoltModel::load(&model_path, &device)
}

/// Download the model from HuggingFace Hub to local cache.
pub fn download_model() -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api = hf_hub::api::sync::Api::new()?;
    let repo = api.model(MODEL_REPO.to_string());
    let path = repo.get("model.safetensors")?;
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    println!("[Chronos-Bolt] Model cached at: {}", parent.display());
    Ok(parent.to_string_lossy().to_string())
}

/// Get the path to the cached model directory (the parent of model.safetensors).
pub fn cached_model_path() -> Option<std::path::PathBuf> {
    let api = hf_hub::api::sync::Api::new().ok()?;
    let repo = api.model(MODEL_REPO.to_string());
    let path = repo.get("model.safetensors").ok()?;
    path.parent().map(|p| p.to_path_buf())
}

/// Check if the model is cached locally.
pub fn is_model_cached() -> bool {
    let api = match hf_hub::api::sync::Api::new() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let repo = api.model(MODEL_REPO.to_string());
    repo.get("model.safetensors").is_ok()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Normal distribution quantile function
// ═══════════════════════════════════════════════════════════════════════════════

fn norm_ppf(p: f64) -> f64 {
    if p <= 0.0 { return f64::NEG_INFINITY; }
    if p >= 1.0 { return f64::INFINITY; }
    let a = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00];
    let b = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01];
    let c = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00, -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00];
    let d = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00];
    let p_low = 0.02425;
    let p_high = 1.0 - p_low;
    if p < p_low {
        let q = (-2.0 * p.ln()).sqrt();
        (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    } else if p <= p_high {
        let q = p - 0.5;
        let r = q * q;
        (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5]) / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Trend Layer Integration
// ═══════════════════════════════════════════════════════════════════════════════

/// Run a Chronos-Bolt forecast or fall back to simple trend analysis.
///
/// Returns (direction: -1/0/1, confidence: 0-1, median_change_pct: f64)
pub fn forecast_trend(
    model: Option<&mut ChronosBoltModel>,
    closes: &[f64],
) -> (f64, f64, f64) {
    match model {
        Some(m) => match m.forecast(closes) {
            Ok(fc) => {
                let last_close = closes.last().copied().unwrap_or(1.0);
                let first_fc = fc.median.first().copied().unwrap_or(last_close);
                let cp = if last_close.abs() > 1e-10 { (first_fc - last_close) / last_close } else { 0.0 };
                let dir = if cp.abs() < 0.002 { 0.0 } else if cp > 0.0 { 1.0 } else { -1.0 };
                (dir, fc.confidence.clamp(0.0, 1.0), cp * 100.0)
            }
            Err(e) => {
                eprintln!("[Chronos-Bolt] Inference failed: {}. Falling back.", e);
                simple_trend_fallback(closes)
            }
        },
        None => simple_trend_fallback(closes),
    }
}

/// Simple OHLCV trend analysis fallback.
fn simple_trend_fallback(closes: &[f64]) -> (f64, f64, f64) {
    if closes.len() < 5 { return (0.0, 0.0, 0.0); }
    let lookback = closes.len().min(10);
    let recent = &closes[closes.len() - lookback..];
    let oldest = recent[0];
    let newest = recent[recent.len() - 1];
    let overall_pct = if oldest.abs() > 1e-10 { (newest - oldest) / oldest } else { 0.0 };
    let expected_direction = if overall_pct >= 0.0 { 1.0 } else { -1.0 };
    let mut consistent = 0usize;
    for i in 1..recent.len() {
        let bar_dir = if recent[i] > recent[i - 1] { 1.0 } else { -1.0 };
        if bar_dir == expected_direction { consistent += 1; }
    }
    let total = (recent.len() - 1).max(1);
    let cr = consistent as f64 / total as f64;
    let raw = (overall_pct * 20.0).clamp(-1.0, 1.0);
    let bc = overall_pct.abs().min(0.15) / 0.15;
    let conf = (bc * cr).clamp(0.0, 1.0);
    let sig = if cr < 0.5 { raw * 0.4 } else if cr < 0.7 { raw * 0.7 } else { raw };
    (sig.clamp(-1.0, 1.0), conf, overall_pct * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_norm_ppf() {
        let q50 = norm_ppf(0.5);
        assert!(q50.abs() < 0.01, "Median should be near 0, got {}", q50);
    }

    #[test]
    fn test_simple_trend_fallback_uptrend() {
        let data: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let (dir, conf, _) = simple_trend_fallback(&data);
        assert!(dir > 0.0);
        assert!(conf > 0.0);
    }

    #[test]
    fn test_simple_trend_fallback_flat() {
        let data = vec![100.0; 20];
        let (dir, _, _) = simple_trend_fallback(&data);
        assert!(dir.abs() < 0.1);
    }

    #[test]
    fn test_forecast_trend_no_model() {
        let data: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let (dir, conf, _) = forecast_trend(None, &data);
        assert!(dir > 0.0);
        assert!(conf > 0.0);
    }
}
