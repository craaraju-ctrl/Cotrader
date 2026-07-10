// CorrelationChecker (pairs skill/tool)
// For pairs trading / hedging awareness. Research: Enhances mean-reversion and risk in correlated assets (crypto focus).
// Now implements AgentSkill for pluggability. Computes real Pearson correlation coefficients
// using rolling 100-candle history windows.

use crate::state::SharedState;
use async_trait::async_trait;
use std::error::Error;
use cotrader_core::{skills::AgentSkill, AgentInput, AgentOutput};

pub struct CorrelationChecker {
    pub state: SharedState,
}

impl CorrelationChecker {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Compute Pearson product-moment correlation coefficient between two return series.
    /// Returns None if insufficient data (< 5 points) or zero variance.
    fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
        let n = x.len().min(y.len());
        if n < 5 {
            return None;
        }

        let mean_x = x.iter().sum::<f64>() / n as f64;
        let mean_y = y.iter().sum::<f64>() / n as f64;

        let mut cov_xy = 0.0;
        let mut var_x = 0.0;
        let mut var_y = 0.0;

        for i in 0..n {
            let dx = x[i] - mean_x;
            let dy = y[i] - mean_y;
            cov_xy += dx * dy;
            var_x += dx * dx;
            var_y += dy * dy;
        }

        let denominator = (var_x * var_y).sqrt();
        if denominator < 1e-12 {
            return None; // Zero variance — undefined correlation
        }

        Some((cov_xy / denominator).clamp(-1.0, 1.0))
    }

    /// Extract normalized returns from OHLCV bars (close-to-close percentage changes).
    fn extract_returns(bars: &[cotrader_core::OhlcvBar], lookback: usize) -> Vec<f64> {
        let start = bars.len().saturating_sub(lookback);
        let slice = &bars[start..];
        slice.windows(2)
            .map(|w| {
                if w[0].close > 0.0 {
                    (w[1].close - w[0].close) / w[0].close
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// Compute rolling correlation of `symbol` vs `reference` (default: BTC) using
    /// the last `lookback` candles (default: 100).
    async fn compute_rolling_correlation(
        &self,
        symbol: &str,
        reference: &str,
        lookback: usize,
    ) -> Option<f64> {
        let history = self.state.market_data.ohlcv_history.read().await;
        let sym_bars = history.get(symbol)?;
        let ref_bars = history.get(reference)?;

        let sym_returns = Self::extract_returns(sym_bars, lookback);
        let ref_returns = Self::extract_returns(ref_bars, lookback);

        Self::pearson_correlation(&sym_returns, &ref_returns)
    }

    pub async fn check_correlation(&self, symbol: &str) -> f64 {
        // BTC is the universal reference asset for crypto correlation
        let reference = if symbol == "BTC" { "ETH" } else { "BTC" };

        // Try rolling correlation with 100-candle window
        if let Some(corr) = self.compute_rolling_correlation(symbol, reference, 100).await {
            return corr;
        }

        // Fallback: try shorter window (50 candles)
        if let Some(corr) = self.compute_rolling_correlation(symbol, reference, 50).await {
            return corr;
        }

        // Fallback: default correlation based on asset class
        let majors = ["BTC", "ETH", "SOL", "BNB", "XRP"];
        if majors.contains(&symbol) {
            return 0.72; // High baseline corr among major cryptos
        }

        // Default moderate correlation for non-major assets
        0.45
    }
}

#[async_trait]
impl AgentSkill for CorrelationChecker {
    fn name(&self) -> &str {
        "CorrelationChecker"
    }
    fn description(&self) -> &str {
        "Estimates correlation to major assets (esp. BTC for crypto) using recent price history (how to detect pair risk / fakeouts for hedging or caution)."
    }

    async fn execute(
        &self,
        input: &AgentInput,
    ) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        if let AgentInput::ConfluenceRequest { context } = input {
            let corr = self.check_correlation(&context.symbol).await;
            println!(
                "[Skill] {} executed for {}: corr={:.2}",
                self.name(),
                context.symbol,
                corr
            );
            Ok(AgentOutput::SkillResult {
                name: self.name().to_string(),
                score: corr,
                note: "pair correlation proxy".to_string(),
                confidence: 0.65,
                direction: cotrader_core::agent::SkillDirection::Neutral,
                weight: 0.1,
            })
        } else {
            Ok(AgentOutput::Done)
        }
    }
}
