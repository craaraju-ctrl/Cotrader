//! # Memory Integration — Simplified version without agentic-memory dependency.
//!
//! Provides:
//! - Policy cache for risk checks
//! - Volatility tracking

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;



/// A cached policy rule for sub-millisecond lookup.
#[derive(Debug, Clone)]
pub struct PolicyEntry {
    pub rule_id: String,
    pub rule_type: String,
    pub max_value: f64,
    pub is_active: bool,
}

/// Shared state for memory integration.
#[derive(Debug, Clone)]
pub struct MemoryIntegration {
    /// Current market volatility (updated by market-data crate)
    pub volatility: Arc<std::sync::atomic::AtomicU64>,
    /// Policy cache for risk checks
    pub policy_cache: Arc<RwLock<HashMap<String, PolicyEntry>>>,
}

impl MemoryIntegration {
    /// Create new integration with default configuration.
    pub fn new() -> Self {
        Self {
            volatility: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            policy_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get current volatility as f64.
    pub fn get_volatility(&self) -> f64 {
        f64::from_bits(self.volatility.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Set current volatility from f64.
    pub fn set_volatility(&self, vol: f64) {
        self.volatility.store(vol.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// Check if a policy is active.
    pub async fn check_policy(&self, rule_id: &str) -> bool {
        let cache = self.policy_cache.read().await;
        cache.get(rule_id).map(|p| p.is_active).unwrap_or(true)
    }

    /// Set a policy entry.
    pub async fn set_policy(&self, entry: PolicyEntry) {
        let mut cache = self.policy_cache.write().await;
        cache.insert(entry.rule_id.clone(), entry);
    }
}

impl Default for MemoryIntegration {
    fn default() -> Self {
        Self::new()
    }
}
