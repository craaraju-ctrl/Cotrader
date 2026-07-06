//! PipelineHook — Integration points into the 5-layer pipeline.
//!
//! Provides helper functions that the existing pipeline components call
//! to get ML-enhanced predictions. Each function has a deterministic fallback.

use crate::engine::MLEngine;
use crate::feature_store::FeatureStore;
use cotrader_core::MarketRegime;

/// Hook into regime detection: ML-first, fallback to threshold.
pub async fn enhance_regime_detection(
    engine: &MLEngine,
    rsi: f64,
    macd_hist: f64,
    atr_pct: f64,
    bars: &[cotrader_core::OhlcvBar],
    current_regime: MarketRegime,
) -> (MarketRegime, f64, &'static str) {
    let features = build_indicator_features(rsi, macd_hist, atr_pct, bars);
    engine.predict_regime(&features, current_regime).await
}

/// Hook into signal scoring: ML-first, fallback to conviction stack.
pub async fn enhance_signal_scoring(
    engine: &MLEngine,
    conviction_factors: &[f64], // 8 factors from ConvictionStack
    indicator_features: &[f64], // 26 from MetricsSnapshot
    fallback_conviction: f64,
) -> (f64, &'static str) {
    let mut features = Vec::with_capacity(34);
    features.extend_from_slice(conviction_factors);
    features.extend_from_slice(indicator_features);
    engine.score_signal(&features, fallback_conviction).await
}

/// Hook into Kelly sizing: ML-first, fallback to 0.55.
pub async fn enhance_kelly_sizing(
    engine: &MLEngine,
    features: &[f64],
    fallback_win_prob: f64,
) -> (f64, &'static str) {
    engine.predict_win_probability(features, fallback_win_prob).await
}

/// Hook into pattern detection: ML-first, fallback to rule-based.
pub async fn enhance_pattern_detection(
    engine: &MLEngine,
    bars: &[cotrader_core::OhlcvBar],
) -> (String, f64, &'static str) {
    let feature_store = FeatureStore::new();
    let ohlcv_features = feature_store.build_ohlcv_matrix(bars, 20);
    engine.detect_patterns(&ohlcv_features).await
}

/// Hook into strategy selection: ML-first, fallback to rule-based.
pub async fn enhance_strategy_selection(
    engine: &MLEngine,
    features: &[f64],
    fallback_index: usize,
) -> (usize, f64, &'static str) {
    engine.select_strategy(features, fallback_index).await
}

/// Build a simplified feature vector from indicator values.
fn build_indicator_features(rsi: f64, macd_hist: f64, atr_pct: f64, bars: &[cotrader_core::OhlcvBar]) -> Vec<f64> {
    let mut features = vec![0.0f64; 30];

    // First 26: indicator values (normalized)
    features[0] = rsi / 100.0;
    features[1] = macd_hist.tanh();
    features[2] = (atr_pct * 100.0).min(1.0);

    // Fill remaining with price momentum
    if bars.len() >= 6 {
        for i in 0..5.min(bars.len() - 1) {
            let idx = bars.len() - 1 - i;
            let ret = (bars[idx].close - bars[idx - 1].close) / bars[idx - 1].close;
            features[26 + i] = (ret * 10.0).clamp(-1.0, 1.0);
        }
    }

    features
}
