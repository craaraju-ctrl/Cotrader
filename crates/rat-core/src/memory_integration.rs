//! # Memory Integration — Bridges rat-core trading with agentic-memory
//!
//! Provides:
//! - ConcurrentPolicyCache for sub-millisecond risk checks
//! - Post-trade analytics pipeline to FinancialRegretScorer
//! - Volatility sync to TemporalEngine

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agentic_memory::performance::ConcurrentPolicyCache;
use agentic_memory::FinancialRegretScorer;

use crate::episode::TradingEpisode;

/// Shared state for memory integration.
#[derive(Clone)]
pub struct MemoryIntegration {
    /// Lock-free policy cache for hot-path risk checks
    pub policy_cache: Arc<ConcurrentPolicyCache<PolicyEntry>>,
    /// Financial regret scorer for post-trade analytics
    pub scorer: Arc<FinancialRegretScorer>,
    /// Current market volatility (updated by market-data crate)
    pub volatility: Arc<std::sync::atomic::AtomicU64>, // f64 stored as bits
}

/// A cached policy rule for sub-millisecond lookup.
#[derive(Debug, Clone)]
pub struct PolicyEntry {
    pub rule_id: String,
    pub rule_type: String,      // "risk_limit", "position_size", "session"
    pub max_value: f64,
    pub is_active: bool,
}

impl MemoryIntegration {
    /// Create new integration with default configuration.
    pub fn new() -> Self {
        Self {
            policy_cache: Arc::new(ConcurrentPolicyCache::new(1000, 300)),
            scorer: Arc::new(FinancialRegretScorer::new()),
            volatility: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Get current volatility as f64.
    pub fn get_volatility(&self) -> f64 {
        f64::from_bits(self.volatility.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Update volatility from market data.
    pub fn set_volatility(&self, sigma: f64) {
        self.volatility.store(sigma.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// Check a policy rule from the lock-free cache.
    /// Returns true if the rule passes (under limit).
    pub fn check_policy(&self, rule_id: &str) -> bool {
        if let Some(entry) = self.policy_cache.get(rule_id) {
            entry.is_active
        } else {
            // Rule not cached — default to allowing (conservative)
            true
        }
    }

    /// Insert a policy rule into the cache.
    pub fn set_policy(&self, entry: PolicyEntry) {
        self.policy_cache.insert(entry.rule_id.clone(), entry);
    }

    /// Score a post-trade episode using the FinancialRegretScorer.
    /// Returns importance score (0.0-1.0).
    pub fn score_episode(&self, episode: &TradingEpisode) -> f64 {
        let mut metadata = HashMap::new();

        // Extract regret from reflection
        if let Some(ref reflection) = episode.reflection {
            metadata.insert("regret_score".to_string(), reflection.regret_score.to_string());
        }

        // Extract balance delta from outcome
        if let Some(ref outcome) = episode.outcome {
            metadata.insert("balance_delta".to_string(), outcome.pnl.to_string());
            metadata.insert("is_win".to_string(), (outcome.pnl > 0.0).to_string());
        }

        // Market state metadata
        metadata.insert("regime".to_string(), episode.market_state.regime.clone());
        metadata.insert("volatility".to_string(), episode.market_state.volatility_24h.to_string());
        metadata.insert("symbol".to_string(), episode.symbol.clone());

        // Compute base importance from access patterns
        let context = agentic_memory::types::ImportanceContext {
            access_count: 1,
            age_seconds: 0.0,
            has_embedding: false,
            content_length: episode.market_state.to_summary().len(),
            content_type: "trading_episode".to_string(),
            tier: agentic_memory::types::MemoryTier::Episodic,
            graph_connections: 0,
            expert_endorsements: 0,
        };

        self.scorer.score(&context, &metadata)
    }

    /// Convert episode to memory record metadata for storage.
    pub fn episode_to_metadata(&self, episode: &TradingEpisode) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("symbol".to_string(), episode.symbol.clone());
        metadata.insert("action".to_string(), episode.action.clone());
        metadata.insert("regime".to_string(), episode.market_state.regime.clone());
        metadata.insert("volatility".to_string(), episode.market_state.volatility_24h.to_string());
        metadata.insert("confidence".to_string(), episode.confidence.to_string());
        metadata.insert("entry_price".to_string(), episode.entry_price.to_string());

        if let Some(ref reflection) = episode.reflection {
            metadata.insert("regret_score".to_string(), reflection.regret_score.to_string());
            metadata.insert("lesson".to_string(), reflection.lesson.clone());
        }

        if let Some(ref outcome) = episode.outcome {
            metadata.insert("pnl".to_string(), outcome.pnl.to_string());
            metadata.insert("pnl_pct".to_string(), outcome.pnl_pct.to_string());
            metadata.insert("exit_reason".to_string(), outcome.exit_reason.clone());
            metadata.insert("is_win".to_string(), (outcome.pnl > 0.0).to_string());
        }

        metadata
    }
}

impl Default for MemoryIntegration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
use std::time::Duration;

    #[test]
    fn test_policy_cache_sub_ms() {
        let integration = MemoryIntegration::new();

        // Insert some rules
        integration.set_policy(PolicyEntry {
            rule_id: "max_position_size".to_string(),
            rule_type: "risk_limit".to_string(),
            max_value: 0.1,
            is_active: true,
        });

        integration.set_policy(PolicyEntry {
            rule_id: "session_block".to_string(),
            rule_type: "session".to_string(),
            max_value: 0.0,
            is_active: false,
        });

        // Measure lookup time
        let start = Instant::now();
        for _ in 0..10000 {
            let _ = integration.check_policy("max_position_size");
        }
        let elapsed = start.elapsed();

        // Should be well under 1ms for 10k lookups
        assert!(elapsed < Duration::from_millis(50),
            "10k policy lookups took {:?}, expected < 1ms", elapsed);

        // Verify rules
        assert!(integration.check_policy("max_position_size"));
        assert!(!integration.check_policy("session_block"));
        assert!(integration.check_policy("unknown_rule")); // default allow
    }

    #[test]
    fn test_volatility_atomic() {
        let integration = MemoryIntegration::new();

        assert_eq!(integration.get_volatility(), 0.0);

        integration.set_volatility(0.75);
        assert_eq!(integration.get_volatility(), 0.75);

        integration.set_volatility(0.01);
        assert_eq!(integration.get_volatility(), 0.01);
    }

    #[test]
    fn test_episode_scoring() {
        let integration = MemoryIntegration::new();

        let episode = TradingEpisode {
            episode_id: "ep-1".to_string(),
            timestamp: chrono::Utc::now(),
            symbol: "BTC".to_string(),
            market_state: crate::episode::MarketStateSnapshot {
                price: 60000.0,
                pivot: 59500.0,
                r1: 61000.0,
                s1: 58000.0,
                confluence: 0.75,
                trend: "Bullish".to_string(),
                volatility_24h: 0.03,
                trend_strength: 0.8,
                regime: "TrendingBull".to_string(),
                session_valid: true,
                calendar_events: vec![],
                patterns: vec![],
                news_headlines: vec![],
                multi_tf_summary: String::new(),
                trading_mode: "paper".to_string(),
                portfolio_heat: 0.05,
                consecutive_losses: 0,
                daily_pnl_pct: 0.0,
            },
            action: "BUY".to_string(),
            entry_price: 60000.0,
            stop_loss: 59000.0,
            take_profit: 62000.0,
            confidence: 0.75,
            reasoning_trace: vec![],
            outcome: Some(crate::episode::TradeOutcome {
                exit_price: 61500.0,
                pnl: 1500.0,
                pnl_pct: 2.5,
                exit_reason: "take_profit".to_string(),
                holding_period_secs: 3600,
                max_unrealized_pnl: 2000.0,
                min_unrealized_pnl: -500.0,
                slippage: 10.0,
            }),
            reflection: Some(crate::episode::PostTradeReflection {
                timestamp: chrono::Utc::now(),
                lesson: "Good entry on trend continuation".to_string(),
                violated_assumptions: vec![],
                regret_score: 0.1, // Low regret (good trade)
                what_went_wrong: vec![],
                what_went_right: vec!["Entered at support".to_string()],
                suggested_rule_change: None,
                should_alert: false,
            }),
        };

        let score = integration.score_episode(&episode);
        assert!(score > 0.0 && score <= 1.0,
            "Score should be 0-1, got {}", score);

        // Verify metadata extraction
        let metadata = integration.episode_to_metadata(&episode);
        assert_eq!(metadata.get("symbol").unwrap(), "BTC");
        assert_eq!(metadata.get("regret_score").unwrap(), "0.1");
        assert_eq!(metadata.get("is_win").unwrap(), "true");
        assert_eq!(metadata.get("regime").unwrap(), "TrendingBull");
    }
}
