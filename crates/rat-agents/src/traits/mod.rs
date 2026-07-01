//! Agent Traits — Lightweight functional processors for all agents.

use async_trait::async_trait;
use crate::thinking::reasoning::ReasoningChain;

/// Core trait that all agents implement.
#[async_trait]
pub trait AgentProcessor: Send + Sync {
    /// Agent name for logging.
    fn name(&self) -> &str;
    
    /// Agent role in the hierarchy.
    fn role(&self) -> &str;
    
    /// Process input data and produce output.
    async fn process(&self, input: AgentInput) -> AgentOutput;
    
    /// Generate reasoning chain for the decision.
    fn reason(&self, input: &AgentInput, output: &AgentOutput) -> ReasoningChain {
        ReasoningChain {
            decision: output.action.clone(),
            steps: vec![],
            overall_confidence: output.confidence,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Input data for agent processing.
#[derive(Debug, Clone)]
pub enum AgentInput {
    MarketData {
        symbol: String,
        price: f64,
        indicators: Vec<(String, f64)>,
    },
    Signal {
        symbol: String,
        action: String,
        confidence: f64,
        indicators: Vec<String>,
    },
    RiskCheck {
        symbol: String,
        signal: String,
        portfolio_state: PortfolioSnapshot,
    },
    Execution {
        symbol: String,
        action: String,
        size: f64,
        price: f64,
    },
    Outcome {
        symbol: String,
        pnl: f64,
        entry_price: f64,
        exit_price: f64,
    },
    Review {
        trade_id: String,
        pnl: f64,
        lessons: Vec<String>,
    },
}

/// Output from agent processing.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub action: String,
    pub confidence: f64,
    pub reasoning: String,
    pub data: Option<serde_json::Value>,
}

/// Portfolio snapshot for risk checks.
#[derive(Debug, Clone)]
pub struct PortfolioSnapshot {
    pub equity: f64,
    pub positions: Vec<PositionSnapshot>,
    pub drawdown: f64,
    pub daily_pnl: f64,
}

#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub unrealized_pnl: f64,
}
