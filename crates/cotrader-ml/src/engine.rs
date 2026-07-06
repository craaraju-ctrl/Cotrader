//! MLEngine — Central orchestration for all ML model inference.
//!
//! Models are loaded lazily on first prediction. If a model file is missing
//! or inference fails, every method falls back to the existing deterministic logic.

use crate::feature_store::FeatureStore;
use crate::models::regime_classifier::RegimeClassifierModel;
use crate::models::signal_scorer::SignalScorerModel;
use crate::models::win_probability::WinProbabilityModel;
use crate::models::pattern_detector::PatternDetectorModel;
use crate::models::strategy_selector::StrategySelectorModel;
use crate::persistence::model_store::ModelStore;
use cotrader_core::MarketRegime;
use std::path::Path;
use tokio::sync::RwLock;

/// Central ML engine that manages all models and provides inference API.
pub struct MLEngine {
    model_store: ModelStore,
    feature_store: FeatureStore,
    regime_classifier: RwLock<Option<RegimeClassifierModel>>,
    signal_scorer: RwLock<Option<SignalScorerModel>>,
    win_probability: RwLock<Option<WinProbabilityModel>>,
    pattern_detector: RwLock<Option<PatternDetectorModel>>,
    strategy_selector: RwLock<Option<StrategySelectorModel>>,
}

impl std::fmt::Debug for MLEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MLEngine")
            .field("models_dir", &"data/models")
            .finish()
    }
}

impl MLEngine {
    /// Create a new ML engine. Models are loaded lazily on first use.
    pub fn new(models_dir: impl AsRef<Path>) -> Self {
        let models_dir = models_dir.as_ref().to_path_buf();
        Self {
            model_store: ModelStore::new(&models_dir),
            feature_store: FeatureStore::new(),
            regime_classifier: RwLock::new(None),
            signal_scorer: RwLock::new(None),
            win_probability: RwLock::new(None),
            pattern_detector: RwLock::new(None),
            strategy_selector: RwLock::new(None),
        }
    }

    /// Predict market regime. Returns (regime, confidence, source).
    /// source is "ml" or "fallback" to indicate which path was used.
    pub async fn predict_regime(
        &self,
        features: &[f64],
        fallback: MarketRegime,
    ) -> (MarketRegime, f64, &'static str) {
        let mut guard = self.regime_classifier.write().await;
        if guard.is_none() {
            *guard = self.model_store.load_regime_classifier().ok();
        }
        if let Some(model) = guard.as_ref() {
            match model.predict(features) {
                Ok((regime, confidence)) => (regime, confidence, "ml"),
                Err(e) => {
                    tracing::warn!("[ML] Regime classifier inference failed: {}, using fallback", e);
                    (fallback, 0.5, "fallback")
                }
            }
        } else {
            (fallback, 0.5, "fallback")
        }
    }

    /// Score signal quality. Returns (profitability_probability, source).
    pub async fn score_signal(
        &self,
        features: &[f64],
        fallback_conviction: f64,
    ) -> (f64, &'static str) {
        let mut guard = self.signal_scorer.write().await;
        if guard.is_none() {
            *guard = self.model_store.load_signal_scorer().ok();
        }
        if let Some(model) = guard.as_ref() {
            match model.predict(features) {
                Ok(prob) => (prob, "ml"),
                Err(e) => {
                    tracing::warn!("[ML] Signal scorer inference failed: {}, using fallback", e);
                    (fallback_conviction, "fallback")
                }
            }
        } else {
            (fallback_conviction, "fallback")
        }
    }

    /// Predict win probability for Kelly sizing. Returns (win_prob, source).
    pub async fn predict_win_probability(
        &self,
        features: &[f64],
        fallback_win_prob: f64,
    ) -> (f64, &'static str) {
        let mut guard = self.win_probability.write().await;
        if guard.is_none() {
            *guard = self.model_store.load_win_probability().ok();
        }
        if let Some(model) = guard.as_ref() {
            match model.predict(features) {
                Ok(prob) => (prob, "ml"),
                Err(e) => {
                    tracing::warn!("[ML] Win probability inference failed: {}, using fallback", e);
                    (fallback_win_prob, "fallback")
                }
            }
        } else {
            (fallback_win_prob, "fallback")
        }
    }

    /// Detect ML patterns from OHLCV data. Returns (direction, confidence, source).
    pub async fn detect_patterns(
        &self,
        ohlcv_features: &[f64],
    ) -> (String, f64, &'static str) {
        let mut guard = self.pattern_detector.write().await;
        if guard.is_none() {
            *guard = self.model_store.load_pattern_detector().ok();
        }
        if let Some(model) = guard.as_ref() {
            match model.predict(ohlcv_features) {
                Ok((direction, confidence)) => (direction, confidence, "ml"),
                Err(e) => {
                    tracing::warn!("[ML] Pattern detector inference failed: {}", e);
                    ("Neutral".to_string(), 0.0, "fallback")
                }
            }
        } else {
            ("Neutral".to_string(), 0.0, "fallback")
        }
    }

    /// Select best strategy. Returns (strategy_index, confidence, source).
    pub async fn select_strategy(
        &self,
        features: &[f64],
        fallback_index: usize,
    ) -> (usize, f64, &'static str) {
        let mut guard = self.strategy_selector.write().await;
        if guard.is_none() {
            *guard = self.model_store.load_strategy_selector().ok();
        }
        if let Some(model) = guard.as_ref() {
            match model.predict(features) {
                Ok((idx, conf)) => (idx, conf, "ml"),
                Err(e) => {
                    tracing::warn!("[ML] Strategy selector inference failed: {}, using fallback", e);
                    (fallback_index, 0.5, "fallback")
                }
            }
        } else {
            (fallback_index, 0.5, "fallback")
        }
    }

    /// Check if any ML models are loaded.
    pub async fn has_models(&self) -> bool {
        let rc = self.regime_classifier.read().await;
        let ss = self.signal_scorer.read().await;
        let wp = self.win_probability.read().await;
        let pd = self.pattern_detector.read().await;
        let st = self.strategy_selector.read().await;
        rc.is_some() || ss.is_some() || wp.is_some() || pd.is_some() || st.is_some()
    }

    /// Get the feature store reference.
    pub fn feature_store(&self) -> &FeatureStore {
        &self.feature_store
    }
}
