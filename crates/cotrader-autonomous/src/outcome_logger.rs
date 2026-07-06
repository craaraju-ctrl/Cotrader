// outcome_logger.rs
// SUB AGENT 8 — OutcomeLoggerAgent (Deterministic)
// Full content from the rat autonomous module (rebranded)

use crate::state::SharedState;
use async_trait::async_trait;
use chrono::Utc;
use std::error::Error;
use cotrader_core::{Agent, AgentInput, AgentOutput, AgentTier};

pub struct OutcomeLoggerAgent {
    pub state: SharedState,
}

impl OutcomeLoggerAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Agent for OutcomeLoggerAgent {
    fn name(&self) -> &str {
        "OutcomeLoggerAgent"
    }
    fn tier(&self) -> AgentTier {
        AgentTier::Sub
    }

    async fn run(
        &self,
        input: Option<AgentInput>,
    ) -> Result<AgentOutput, Box<dyn Error + Send + Sync>> {
        match input {
            Some(AgentInput::LogOutcome { key, value }) => {
                let _ = self.state.agent_memory.memory.store_decision(&key, &value);
                println!("[OutcomeLogger] Logged: {}", key);
                Ok(AgentOutput::Done)
            }
            _ => {
                // Default: log current portfolio summary with realized + unrealized P&L
                let portfolio = self.state.portfolio_store.portfolio.read().await;
                let unrealized_pnl: f64 = portfolio.open_positions.iter().map(|p| p.unrealized_pnl).sum();
                let total_pnl = portfolio.daily_pnl + unrealized_pnl;
                let summary = format!(
                    "Total P&L: ${:.2} (Realized: ${:.2} + Unrealized: ${:.2}) | Trades: {} | Wins: {} | Losses: {} | Open: {} | Equity: ${:.2}",
                    total_pnl,
                    portfolio.daily_pnl,
                    unrealized_pnl,
                    portfolio.total_trades_today,
                    portfolio.winning_trades_today,
                    portfolio.losing_trades_today,
                    portfolio.open_positions.len(),
                    portfolio.total_equity
                );
                let key = format!("summary/{}", Utc::now().timestamp());
                let _ = self.state.agent_memory.memory.store_decision(&key, &summary);
                println!("[OutcomeLogger] {}", summary);
                Ok(AgentOutput::Done)
            }
        }
    }
}
