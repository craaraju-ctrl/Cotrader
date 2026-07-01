use crate::introspector::{AgentMode, Introspector};
use std::collections::HashMap;
use std::sync::Arc;
use rat_autonomous::state::SharedState;
use rat_core::TradeDirection;

pub struct ActiveLearner {
    state: SharedState,
    introspector: Option<Arc<Introspector>>,
    uncertainty_map: HashMap<(String, String), f64>,
    exploration_budget_pct: f64,
}

impl ActiveLearner {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            introspector: None,
            uncertainty_map: HashMap::new(),
            exploration_budget_pct: 0.02,
        }
    }

    pub fn with_introspector(mut self, introspector: Arc<Introspector>) -> Self {
        self.introspector = Some(introspector);
        self
    }

    pub async fn maybe_explore(&self, symbol: &str, _direction: TradeDirection) -> Option<f64> {
        if let Some(intro) = &self.introspector {
            let intro_state = intro.introspect().await;
            if !matches!(intro_state.mode, AgentMode::Explore) {
                return None;
            }
        }
        let unc = self.compute_symbol_uncertainty(symbol).await;
        if unc < 0.6 {
            return None;
        }
        let price = self.get_current_price(symbol).await;
        if price <= 0.0 {
            return None;
        }
        let equity = self.state.portfolio_store.portfolio.read().await.total_equity;
        let max_probe = equity * self.exploration_budget_pct;
        if max_probe <= 0.0 {
            return None;
        }
        Some((max_probe / price) * 0.95)
    }

    pub fn record_probe_outcome(&mut self, symbol: &str, profitable: bool, surprise: f64) {
        let key = (symbol.to_string(), "exploration".to_string());
        let cur = self.uncertainty_map.get(&key).copied().unwrap_or(0.7);
        let new_unc = if profitable {
            cur * 0.8
        } else {
            (cur * 1.2).min(1.0)
        };
        let final_unc = if surprise > 0.05 {
            new_unc * 1.1
        } else {
            new_unc
        };
        self.uncertainty_map.insert(key, final_unc);
    }

    async fn compute_symbol_uncertainty(&self, symbol: &str) -> f64 {
        // Check if we have historical uncertainty data for this symbol
        let key = (symbol.to_string(), "exploration".to_string());
        if let Some(&known_unc) = self.uncertainty_map.get(&key) {
            return known_unc;
        }
        // Compute from price volatility: high ATR% = high uncertainty
        let bars = self.state.market_data.ohlcv_history.read().await;
        if let Some(b) = bars.get(symbol) {
            if b.len() >= 14 {
                let highs: Vec<f64> = b.iter().map(|x| x.high).collect();
                let lows: Vec<f64> = b.iter().map(|x| x.low).collect();
                let closes: Vec<f64> = b.iter().map(|x| x.close).collect();
                // Simple ATR% as uncertainty proxy
                let mut tr_sum = 0.0;
                for i in 1..b.len() {
                    let tr = (highs[i] - lows[i])
                        .max((highs[i] - closes[i - 1]).abs())
                        .max((lows[i] - closes[i - 1]).abs());
                    tr_sum += tr;
                }
                let atr = tr_sum / (b.len() - 1) as f64;
                let last_price = closes.last().copied().unwrap_or(1.0);
                let atr_pct = atr / last_price.max(0.001);
                // Map ATR% to uncertainty: 0-2% = low (0.3), 2-5% = medium (0.6), 5%+ = high (0.9)
                return (atr_pct * 18.0).clamp(0.2, 0.95);
            }
        }
        // Default: moderate uncertainty for unknown symbols
        0.6
    }

    async fn get_current_price(&self, symbol: &str) -> f64 {
        self.state
            .market_data
            .ohlcv_history
            .read()
            .await
            .get(symbol)
            .and_then(|b| b.last())
            .map(|b| b.close)
            .unwrap_or(0.0)
    }
}
