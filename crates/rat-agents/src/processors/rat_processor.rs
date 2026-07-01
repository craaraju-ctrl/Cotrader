//! Rat (CIO) — Top-level orchestrator processor.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct RatProcessor;

#[async_trait]
impl AgentProcessor for RatProcessor {
    fn name(&self) -> &str { "Rat" }
    fn role(&self) -> &str { "Chief Investment Officer" }
    
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                // Coordinate all managers for this symbol
                let avg_indicator = indicators.iter().map(|(_, v)| v).sum::<f64>() / indicators.len().max(1) as f64;
                let action = if avg_indicator > 0.6 { "BUY" } else if avg_indicator < 0.4 { "SELL" } else { "HOLD" };
                
                AgentOutput {
                    action: action.to_string(),
                    confidence: avg_indicator,
                    reasoning: format!("CIO decision for {}: avg indicator {:.2}", symbol, avg_indicator),
                    data: None,
                }
            }
            _ => AgentOutput {
                action: "HOLD".to_string(),
                confidence: 0.0,
                reasoning: "CIO delegates to managers".to_string(),
                data: None,
            }
        }
    }
}
