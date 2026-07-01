//! Verifier Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct RiskPsychologyProcessor;
pub struct RiskCalculatorProcessor;
pub struct ReflectorProcessor;

#[async_trait]
impl AgentProcessor for RiskPsychologyProcessor {
    fn name(&self) -> &str { "RiskPsychology" }
    fn role(&self) -> &str { "Evaluate emotional state" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let risk_score = portfolio_state.drawdown * 0.5 + (portfolio_state.positions.len() as f64 / 10.0);
                AgentOutput { action: "EVALUATE".to_string(), confidence: 1.0 - risk_score, reasoning: format!("Psychology risk: {:.2}", risk_score), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for RiskCalculatorProcessor {
    fn name(&self) -> &str { "RiskCalculator" }
    fn role(&self) -> &str { "Calculate position sizing" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let equity = portfolio_state.equity;
                let max_position = equity * 0.02; // 2% risk per trade
                AgentOutput { action: "CALCULATE".to_string(), confidence: 0.9, reasoning: format!("Max position: ${:.2}", max_position), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for ReflectorProcessor {
    fn name(&self) -> &str { "Reflector" }
    fn role(&self) -> &str { "Post-trade reflection" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Outcome { pnl, .. } => {
                let lesson = if pnl > 0.0 { "Profitable trade" } else { "Loss - review setup" };
                AgentOutput { action: "REFLECT".to_string(), confidence: 0.8, reasoning: lesson.to_string(), data: None }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No outcome".to_string(), data: None }
        }
    }
}
