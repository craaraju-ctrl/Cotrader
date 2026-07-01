//! rat-strategies
pub mod arbitrage;
pub mod breakout;
pub mod grid;
pub mod market_making;
pub mod mean_reversion;
pub mod momentum;
pub mod pairs;
pub mod scalping;
pub mod swing;
pub mod trend_following;

/// Trading signal with action and confidence score
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub action: String,
    pub confidence: f64,
}

impl Signal {
    pub fn buy(confidence: f64) -> Self {
        Self { action: "BUY".to_string(), confidence: confidence.clamp(0.0, 1.0) }
    }
    pub fn sell(confidence: f64) -> Self {
        Self { action: "SELL".to_string(), confidence: confidence.clamp(0.0, 1.0) }
    }
    pub fn hold() -> Self {
        Self { action: "HOLD".to_string(), confidence: 0.0 }
    }
}
