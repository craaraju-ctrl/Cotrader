//! Executor Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct StrategyDecisionProcessor;
pub struct PortfolioManagerProcessor;
pub struct ExecutionCoordinatorProcessor;

#[async_trait]
impl AgentProcessor for StrategyDecisionProcessor {
    fn name(&self) -> &str { "StrategyDecision" }
    fn role(&self) -> &str { "Generate trade signals" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Signal { action, confidence, .. } => AgentOutput { action, confidence, reasoning: "Strategy signal generated".to_string(), data: None },
            _ => AgentOutput { action: "HOLD".to_string(), confidence: 0.0, reasoning: "No signal".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for PortfolioManagerProcessor {
    fn name(&self) -> &str { "PortfolioManager" }
    fn role(&self) -> &str { "Position sizing and risk allocation" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let heat = portfolio_state.positions.iter().map(|p| p.unrealized_pnl.abs()).sum::<f64>() / portfolio_state.equity.max(1.0);
                AgentOutput { action: "SIZE".to_string(), confidence: 1.0 - heat.min(1.0), reasoning: format!("Portfolio heat: {:.2}", heat), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for ExecutionCoordinatorProcessor {
    fn name(&self) -> &str { "ExecutionCoordinator" }
    fn role(&self) -> &str { "Order routing and settlement" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Execution { symbol, action, size, price } => AgentOutput { action: action.clone(), confidence: 1.0, reasoning: format!("Execute {} {} {:.6} @ ${:.2}", action, symbol, size, price), data: None },
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No execution".to_string(), data: None }
        }
    }
}
