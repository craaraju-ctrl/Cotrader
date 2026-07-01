//! Operations Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct PortfolioAdministratorProcessor;
pub struct JournalKeeperProcessor;

#[async_trait]
impl AgentProcessor for PortfolioAdministratorProcessor {
    fn name(&self) -> &str { "PortfolioAdministrator" }
    fn role(&self) -> &str { "Reconciliation" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Outcome { symbol, pnl, .. } => AgentOutput { action: "RECONCILE".to_string(), confidence: 1.0, reasoning: format!("{} P&L: ${:.2}", symbol, pnl), data: None },
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for JournalKeeperProcessor {
    fn name(&self) -> &str { "JournalKeeper" }
    fn role(&self) -> &str { "Trade journal" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Review { trade_id, pnl, lessons } => AgentOutput { action: "JOURNAL".to_string(), confidence: 1.0, reasoning: format!("{}: P&L ${:.2}, lessons: {}", trade_id, pnl, lessons.join(", ")), data: None },
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No review".to_string(), data: None }
        }
    }
}
