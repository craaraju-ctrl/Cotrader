//! Staleness Management System
//! Prevents outdated memories from dominating retrieval results through temporal decay.

use crate::types::TieredRecord;
use chrono::{DateTime, Utc};

/// Configuration for staleness behavior
#[derive(Debug, Clone)]
pub struct StalenessConfig {
    pub decay_rate_per_day: f64,
    pub max_age_days: u32,
    pub validation_threshold: f64,
    pub confidence_decay_per_day: f64,
    /// Volatility sensitivity multiplier (default: 2.0)
    pub volatility_sensitivity: f64,
    /// Permanent floor for structural rules (default: 0.3)
    pub structural_floor: f64,
}

impl Default for StalenessConfig {
    fn default() -> Self {
        Self {
            decay_rate_per_day: 0.028,
            max_age_days: 120,
            validation_threshold: 0.35,
            confidence_decay_per_day: 0.012,
            volatility_sensitivity: 2.0,
            structural_floor: 0.3,
        }
    }
}

/// Manages staleness calculations with volatility-aware decay
pub struct StalenessManager {
    config: StalenessConfig,
}

impl StalenessManager {
    pub fn new(config: StalenessConfig) -> Self {
        Self { config }
    }

    /// Calculate effective score after applying temporal decay (default, no volatility)
    pub fn effective_score(&self, record: &TieredRecord) -> f64 {
        self.effective_score_with_volatility(record, 0.0)
    }

    /// Calculate effective score with explicit volatility parameter.
    /// sigma: market volatility (0.0 = calm, 1.0 = extreme)
    pub fn effective_score_with_volatility(&self, record: &TieredRecord, sigma: f64) -> f64 {
        let age_days = self.calculate_age_days(&record.record.timestamp);

        // Volatility-adjusted decay rate
        let alpha = self.config.volatility_sensitivity;
        let effective_rate = self.config.decay_rate_per_day * (1.0 + alpha * sigma);

        let time_decay = (1.0 - effective_rate).powf(age_days);
        let confidence_decay = (1.0 - self.config.confidence_decay_per_day * age_days).max(0.15);

        let mut base = record.importance * time_decay * confidence_decay;

        // Max age penalty
        if age_days > self.config.max_age_days as f64 {
            base *= 0.22;
        }

        // Structural floor: procedures and rules never fully decay
        let is_structural = matches!(
            record.record.content_type.as_str(),
            "procedure" | "rule" | "workflow" | "behavioral"
        );
        if is_structural {
            base = base.max(self.config.structural_floor * record.importance);
        }

        base
    }

    fn calculate_age_days(&self, timestamp: &str) -> f64 {
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
            let now = Utc::now();
            (now - dt.with_timezone(&Utc)).num_days() as f64
        } else {
            60.0
        }
    }

    pub fn needs_validation(&self, record: &TieredRecord) -> bool {
        self.effective_score(record) < self.config.validation_threshold
    }

    pub fn apply_update(
        &self,
        record: &mut TieredRecord,
        new_importance: f64,
        new_timestamp: Option<String>,
    ) {
        record.importance = (record.importance * 0.55 + new_importance * 0.45).clamp(0.1, 1.0);
        if let Some(ts) = new_timestamp {
            record.record.timestamp = ts;
        }
    }
}

impl Default for StalenessManager {
    fn default() -> Self {
        Self::new(StalenessConfig::default())
    }
}
