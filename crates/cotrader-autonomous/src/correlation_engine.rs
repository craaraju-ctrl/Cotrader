//! # Correlation Engine
//!
//! Computes rolling Pearson correlations between symbol pairs and adjusts
//! position sizes to prevent over-concentration in correlated assets.
//!
//! - High correlation (>0.7): reduce position size for new trades
//! - Moderate correlation (0.4-0.7): slight reduction
//! - Low correlation (<0.4): no adjustment

use std::collections::HashMap;

/// Pairwise correlation result
#[derive(Debug, Clone)]
pub struct CorrelationPair {
    pub symbol_a: String,
    pub symbol_b: String,
    pub correlation: f64, // -1.0 to 1.0
}

/// Portfolio concentration risk
#[derive(Debug, Clone)]
pub struct ConcentrationRisk {
    pub symbol: String,
    pub correlation_penalty: f64, // 0.0 to 1.0 (how much to reduce position)
    pub correlated_with: Vec<(String, f64)>, // (symbol, correlation)
    pub effective_exposure: f64,
}

/// Per-symbol price/return state
struct SymbolPriceState {
    last_price: f64,
    returns: Vec<f64>,
}

/// Correlation Engine
pub struct CorrelationEngine {
    /// Raw price state per symbol (for computing returns)
    price_states: HashMap<String, SymbolPriceState>,
    /// Rolling window size
    window: usize,
    /// Correlation thresholds
    high_threshold: f64,   // 0.7
    moderate_threshold: f64, // 0.4
}

impl Default for CorrelationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CorrelationEngine {
    pub fn new() -> Self {
        Self {
            price_states: HashMap::new(),
            window: 50,
            high_threshold: 0.7,
            moderate_threshold: 0.4,
        }
    }

    /// Update price for a symbol (computes log returns internally)
    pub fn update_price(&mut self, symbol: &str, price: f64) {
        let state = self.price_states.entry(symbol.to_string()).or_insert_with(|| SymbolPriceState {
            last_price: price,
            returns: Vec::new(),
        });

        if state.last_price > 0.0 && price > 0.0 {
            let return_val = (price / state.last_price).ln();
            state.returns.push(return_val);
        }

        state.last_price = price;

        if state.returns.len() > self.window * 2 {
            state.returns.drain(..state.returns.len() - self.window * 2);
        }
    }

    /// Compute Pearson correlation between two symbols over the rolling window
    pub fn correlation(&self, symbol_a: &str, symbol_b: &str) -> Option<f64> {
        let state_a = self.price_states.get(symbol_a)?;
        let state_b = self.price_states.get(symbol_b)?;

        let len = state_a.returns.len().min(state_b.returns.len()).min(self.window);
        if len < 10 {
            return None;
        }

        let a = &state_a.returns[state_a.returns.len() - len..];
        let b = &state_b.returns[state_b.returns.len() - len..];

        // Compute means
        let mean_a: f64 = a.iter().sum::<f64>() / len as f64;
        let mean_b: f64 = b.iter().sum::<f64>() / len as f64;

        // Compute covariance and standard deviations
        let mut cov = 0.0;
        let mut var_a = 0.0;
        let mut var_b = 0.0;
        for i in 0..len {
            let da = a[i] - mean_a;
            let db = b[i] - mean_b;
            cov += da * db;
            var_a += da * da;
            var_b += db * db;
        }

        let denom = (var_a * var_b).sqrt();
        if denom > 0.0 {
            Some(cov / denom)
        } else {
            Some(0.0)
        }
    }

    /// Compute all pairwise correlations for a set of symbols
    pub fn compute_all_pairs(&self, symbols: &[String]) -> Vec<CorrelationPair> {
        let mut pairs = Vec::new();
        for i in 0..symbols.len() {
            for j in (i + 1)..symbols.len() {
                if let Some(corr) = self.correlation(&symbols[i], &symbols[j]) {
                    pairs.push(CorrelationPair {
                        symbol_a: symbols[i].clone(),
                        symbol_b: symbols[j].clone(),
                        correlation: corr,
                    });
                }
            }
        }
        pairs
    }

    /// Compute concentration risk for a symbol given current positions
    pub fn concentration_risk(
        &self,
        symbol: &str,
        current_positions: &HashMap<String, f64>, // symbol → exposure
    ) -> ConcentrationRisk {
        let mut correlated_with = Vec::new();
        let mut total_correlated_exposure = 0.0;

        for (other, &exposure) in current_positions {
            if other == symbol {
                continue;
            }
            if let Some(corr) = self.correlation(symbol, other) {
                if corr.abs() > self.moderate_threshold {
                    correlated_with.push((other.clone(), corr));
                    total_correlated_exposure += exposure * corr.abs();
                }
            }
        }

        let own_exposure = current_positions.get(symbol).copied().unwrap_or(0.0);
        let effective_exposure = own_exposure + total_correlated_exposure;

        // Penalty: 0 at low correlation, up to 0.5 at high correlation
        let max_corr = correlated_with.iter().map(|(_, c)| c.abs()).fold(0.0_f64, f64::max);
        let correlation_penalty = if max_corr > self.high_threshold {
            0.5 // High correlation — halve position
        } else if max_corr > self.moderate_threshold {
            0.25 // Moderate — reduce by 25%
        } else {
            0.0
        };

        ConcentrationRisk {
            symbol: symbol.to_string(),
            correlation_penalty,
            correlated_with,
            effective_exposure,
        }
    }

    /// Get position size adjustment factor (0.5 to 1.0)
    pub fn position_adjustment(&self, concentration: &ConcentrationRisk) -> f64 {
        (1.0 - concentration.correlation_penalty).max(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_identical_series() {
        let mut engine = CorrelationEngine::new();
        for i in 0..30 {
            let price = 100.0 + i as f64;
            engine.update_price("BTC", price);
            engine.update_price("ETH", price);
        }
        let corr = engine.correlation("BTC", "ETH").unwrap();
        assert!(corr > 0.99, "Identical series should have correlation near 1.0, got {}", corr);
    }

    #[test]
    fn test_correlation_inverse_series() {
        let mut engine = CorrelationEngine::new();
        // Use exponential growth/decline so log returns are constant → correlation = -1.0
        for i in 0..30 {
            let price_a = 100.0 * (1.01_f64).powi(i);   // constant positive log return
            let price_b = 200.0 * (0.99_f64).powi(i);    // constant negative log return
            engine.update_price("BTC", price_a);
            engine.update_price("ETH", price_b);
        }
        let corr = engine.correlation("BTC", "ETH").unwrap();
        assert!(corr < -0.99, "Inverse series should have correlation near -1.0, got {}", corr);
    }

    #[test]
    fn test_concentration_risk_high_correlation() {
        let mut engine = CorrelationEngine::new();
        for i in 0..30 {
            let price = 100.0 + i as f64;
            engine.update_price("BTC", price);
            engine.update_price("ETH", price);
            engine.update_price("SOL", price * 2.0);
        }
        let positions = HashMap::from([
            ("BTC".to_string(), 10000.0),
            ("ETH".to_string(), 5000.0),
            ("SOL".to_string(), 3000.0),
        ]);
        let risk = engine.concentration_risk("BTC", &positions);
        assert!(risk.correlation_penalty > 0.0, "Should detect concentration risk");
        let adjustment = engine.position_adjustment(&risk);
        assert!(adjustment < 1.0, "Should reduce position size");
    }

    #[test]
    fn test_low_correlation_no_penalty() {
        let mut engine = CorrelationEngine::new();
        // Use genuinely uncorrelated price series (sin vs cos at different frequencies)
        // sin(0.7*i) and cos(1.3*i) have different periods → low correlation in log returns
        for i in 0..50 {
            engine.update_price("BTC", 100.0 + (i as f64 * 0.7).sin() * 10.0);
            engine.update_price("ETH", 200.0 + (i as f64 * 1.3).cos() * 20.0);
        }
        let positions = HashMap::from([
            ("BTC".to_string(), 10000.0),
            ("ETH".to_string(), 5000.0),
        ]);
        let risk = engine.concentration_risk("BTC", &positions);
        // Low correlation should have zero penalty
        assert_eq!(risk.correlation_penalty, 0.0,
            "Low correlation should have zero penalty, got {}", risk.correlation_penalty);
    }
}
