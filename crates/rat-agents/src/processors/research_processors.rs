//! Research Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct QuantResearcherProcessor;
pub struct TechnicalAnalystProcessor;
pub struct FundamentalAnalystProcessor;

#[async_trait]
impl AgentProcessor for QuantResearcherProcessor {
    fn name(&self) -> &str { "QuantResearcher" }
    fn role(&self) -> &str { "Statistical models" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { indicators, .. } => {
                let avg = indicators.iter().map(|(_, v)| v).sum::<f64>() / indicators.len().max(1) as f64;
                AgentOutput { action: "ANALYZE".to_string(), confidence: avg, reasoning: format!("Quant score: {:.2}", avg), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for TechnicalAnalystProcessor {
    fn name(&self) -> &str { "TechnicalAnalyst" }
    fn role(&self) -> &str { "Charts and patterns" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, .. } => AgentOutput { action: "ANALYZE".to_string(), confidence: 0.6, reasoning: format!("Technical analysis for {} @ ${:.2}", symbol, price), data: None },
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for FundamentalAnalystProcessor {
    fn name(&self) -> &str { "FundamentalAnalyst" }
    fn role(&self) -> &str { "Valuation and earnings" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, .. } => AgentOutput { action: "ANALYZE".to_string(), confidence: 0.5, reasoning: format!("Fundamental analysis for {}", symbol), data: None },
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}
