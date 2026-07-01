//! Guardian Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct DrawdownMonitorProcessor;
pub struct OvertradingPreventerProcessor;
pub struct OutcomeLoggerProcessor;

#[async_trait]
impl AgentProcessor for DrawdownMonitorProcessor {
    fn name(&self) -> &str { "DrawdownMonitor" }
    fn role(&self) -> &str { "Track and limit drawdown" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let dd = portfolio_state.drawdown;
                let level = if dd > 0.15 { "CRITICAL" } else if dd > 0.10 { "WARNING" } else if dd > 0.05 { "ELEVATED" } else { "NORMAL" };
                AgentOutput { action: "MONITOR".to_string(), confidence: 1.0 - dd, reasoning: format!("Drawdown: {:.1}% ({})", dd * 100.0, level), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for OvertradingPreventerProcessor {
    fn name(&self) -> &str { "OvertradingPreventer" }
    fn role(&self) -> &str { "Limit trade frequency" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let trades = portfolio_state.positions.len() as u32;
                let should_throttle = trades >= 15;
                AgentOutput { action: "CHECK".to_string(), confidence: if should_throttle { 0.3 } else { 0.9 }, reasoning: format!("{} positions", trades), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for OutcomeLoggerProcessor {
    fn name(&self) -> &str { "OutcomeLogger" }
    fn role(&self) -> &str { "Log trade outcomes" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Outcome { symbol, pnl, .. } => AgentOutput { action: "LOG".to_string(), confidence: 1.0, reasoning: format!("{} P&L: ${:.2}", symbol, pnl), data: None },
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No outcome".to_string(), data: None }
        }
    }
}
