//! rat-ml — Machine Learning Engine for RAT Agent
//!
//! Provides ML-powered predictions across 5 subsystems:
//! 1. Regime Classification — MLP classifies market regimes
//! 2. Signal Quality Scoring — MLP predicts trade profitability
//! 3. Win Probability — GBT predicts per-setup win rate for Kelly sizing
//! 4. Pattern Detection — CNN detects complex multi-candle patterns
//! 5. Strategy Selection — RandomForest picks best strategy per regime
//!
//! All models have deterministic fallbacks. If no model is trained yet,
//! the system works exactly as before (threshold-based logic).

pub mod engine;
pub mod feature_store;
pub mod models;
pub mod training;
pub mod persistence;
pub mod integration;

pub use engine::MLEngine;
pub use feature_store::FeatureStore;
pub use training::background::start_background_ml_training;
