//! # Symbol Ranker
//!
//! Prioritizes symbols by opportunity score so the system focuses on the best
//! setups first. Replaces equal-priority scanning with opportunity-based ranking.
//!
//! Opportunity factors:
//! - Confluence strength (40%): higher alignment = more opportunity
//! - Regime fit (25%): trending regimes score higher than choppy
//! - Volume activity (20%): higher relative volume = more liquidity
//! - Momentum (15%): stronger recent momentum = directional conviction

use std::collections::HashMap;

/// Ranked symbol with opportunity score
#[derive(Debug, Clone)]
pub struct RankedSymbol {
    pub symbol: String,
    pub opportunity_score: f64, // 0.0 to 1.0
    pub confluence_score: f64,
    pub regime_score: f64,
    pub volume_score: f64,
    pub momentum_score: f64,
    pub rank: usize,
}

/// Symbol Ranker
pub struct SymbolRanker {
    /// Volume history per symbol (for relative volume calculation)
    volume_history: HashMap<String, Vec<f64>>,
    /// Price history per symbol (for momentum calculation)
    price_history: HashMap<String, Vec<f64>>,
    /// How many top symbols to rank
    top_n: usize,
    /// Minimum opportunity score to include
    min_opportunity: f64,
}

impl Default for SymbolRanker {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolRanker {
    pub fn new() -> Self {
        Self {
            volume_history: HashMap::new(),
            price_history: HashMap::new(),
            top_n: 10,
            min_opportunity: 0.2,
        }
    }

    /// Feed volume data for a symbol
    pub fn update_volume(&mut self, symbol: &str, volume: f64) {
        let history = self.volume_history.entry(symbol.to_string()).or_default();
        history.push(volume);
        if history.len() > 100 {
            history.remove(0);
        }
    }

    /// Feed price data for a symbol
    pub fn update_price(&mut self, symbol: &str, price: f64) {
        let history = self.price_history.entry(symbol.to_string()).or_default();
        history.push(price);
        if history.len() > 100 {
            history.remove(0);
        }
    }

    /// Rank symbols by opportunity score
    pub fn rank(
        &self,
        confluence_scores: &HashMap<String, f64>,   // symbol → alignment_score
        regime_scores: &HashMap<String, f64>,         // symbol → regime fitness (0-1)
    ) -> Vec<RankedSymbol> {
        let mut ranked = Vec::new();

        for symbol in confluence_scores.keys() {
            let confluence = confluence_scores.get(symbol).copied().unwrap_or(0.0).abs();
            let regime = regime_scores.get(symbol).copied().unwrap_or(0.5);
            let volume = self.compute_volume_score(symbol);
            let momentum = self.compute_momentum_score(symbol);

            let opportunity = confluence * 0.40 + regime * 0.25 + volume * 0.20 + momentum * 0.15;

            if opportunity >= self.min_opportunity {
                ranked.push(RankedSymbol {
                    symbol: symbol.clone(),
                    opportunity_score: opportunity,
                    confluence_score: confluence,
                    regime_score: regime,
                    volume_score: volume,
                    momentum_score: momentum,
                    rank: 0,
                });
            }
        }

        ranked.sort_by(|a, b| b.opportunity_score.partial_cmp(&a.opportunity_score).unwrap_or(std::cmp::Ordering::Equal));
        for (i, item) in ranked.iter_mut().enumerate() {
            item.rank = i + 1;
        }

        ranked.into_iter().take(self.top_n).collect()
    }

    /// Get top N symbols for immediate analysis
    pub fn top_symbols(&self, ranked: &[RankedSymbol]) -> Vec<String> {
        ranked.iter().map(|r| r.symbol.clone()).collect()
    }

    fn compute_volume_score(&self, symbol: &str) -> f64 {
        let history = match self.volume_history.get(symbol) {
            Some(h) if h.len() >= 5 => h,
            _ => return 0.5, // default neutral
        };
        let recent_avg: f64 = history[history.len().saturating_sub(5)..].iter().sum::<f64>() / 5.0;
        let overall_avg: f64 = history.iter().sum::<f64>() / history.len() as f64;
        if overall_avg > 0.0 {
            (recent_avg / overall_avg).clamp(0.0, 2.0) / 2.0
        } else {
            0.5
        }
    }

    fn compute_momentum_score(&self, symbol: &str) -> f64 {
        let history = match self.price_history.get(symbol) {
            Some(h) if h.len() >= 10 => h,
            _ => return 0.5,
        };
        let recent = &history[history.len() - 10..];
        let first = recent[0];
        let last = recent[recent.len() - 1];
        if first > 0.0 {
            let pct_change = ((last - first) / first * 100.0).abs();
            // Normalize: 5% move = score 1.0
            (pct_change / 5.0).clamp(0.0, 1.0)
        } else {
            0.5
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ranking_by_confluence() {
        let ranker = SymbolRanker::new();
        let mut confluence = HashMap::new();
        confluence.insert("BTC".to_string(), 0.8);
        confluence.insert("ETH".to_string(), 0.3);
        confluence.insert("SOL".to_string(), 0.6);

        let regimes = HashMap::new();
        let ranked = ranker.rank(&confluence, &regimes);
        assert!(!ranked.is_empty());
        // BTC should be ranked first (highest confluence)
        assert_eq!(ranked[0].symbol, "BTC");
        assert_eq!(ranked[0].rank, 1);
    }

    #[test]
    fn test_volume_affects_ranking() {
        let mut ranker = SymbolRanker::new();
        // ETH: low historical volume, then spike in recent ticks (recent_avg > overall_avg)
        for _ in 0..18 {
            ranker.update_volume("ETH", 100.0);
        }
        ranker.update_volume("ETH", 500.0);
        ranker.update_volume("ETH", 500.0);

        // BTC: constant volume (recent_avg == overall_avg)
        for _ in 0..20 {
            ranker.update_volume("BTC", 100.0);
        }

        let confluence = HashMap::from([
            ("BTC".to_string(), 0.5),
            ("ETH".to_string(), 0.5),
        ]);
        let regimes = HashMap::new();
        let ranked = ranker.rank(&confluence, &regimes);
        // ETH should rank higher due to recent volume spike
        assert_eq!(ranked[0].symbol, "ETH");
    }

    #[test]
    fn test_top_n_limit() {
        let ranker = SymbolRanker::new();
        let confluence: HashMap<String, f64> = (0..20)
            .map(|i| (format!("SYM{}", i), 0.5))
            .collect();
        let regimes = HashMap::new();
        let ranked = ranker.rank(&confluence, &regimes);
        assert!(ranked.len() <= 10, "Should limit to top_n");
    }
}
