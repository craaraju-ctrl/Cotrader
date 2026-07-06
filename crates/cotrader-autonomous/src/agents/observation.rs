//! Observation Agent — Trade outcomes → performance tracking → simple rule learning.

use super::reasoning::ReasoningChain;
use crate::state::SharedState;

#[derive(Clone)]
pub struct ObservationAgent {
    pub state: SharedState,
}

#[derive(Debug, Clone)]
pub struct ObservationSummary {
    pub total_trades: usize,
    pub win_rate: f64,
    pub avg_regret: f64,
    pub recent_outcome: Option<String>,
    pub rules_discovered: usize,
}

impl ObservationAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Log a completed trade and update performance metrics.
    pub async fn observe_trade(
        &self,
        symbol: &str,
        direction: &str,
        entry_price: f64,
        exit_price: f64,
        pnl: f64,
        exit_reason: &str,
    ) {
        let outcome = if pnl > 0.0 { "WIN" } else if pnl < 0.0 { "LOSS" } else { "BREAKEVEN" };
        let pnl_pct = if entry_price > 0.0 { pnl / entry_price } else { 0.0 };

        println!(
            "[Observation] {} {} {} — PnL: {:.2} ({:.2}%) — {}",
            outcome, direction, symbol, pnl, pnl_pct * 100.0, exit_reason
        );

        // Store in episode store
        let portfolio = self.state.portfolio_store.portfolio.read().await;
        let regime = self.state.market_data.market_regime.read().await;
        let regime_str = regime.map(|r| format!("{:?}", r)).unwrap_or_else(|| "Unknown".to_string());

        let regret_score = if pnl > 0.0 { 0.0 } else { (pnl.abs() / entry_price * 10.0).min(1.0) };

        let _ = self.state.agent_memory.episode_store.insert_closed_trade(
            &crate::episode_store::ClosedEpisode {
                id: uuid::Uuid::new_v4().to_string(),
                symbol: symbol.to_string(),
                direction: direction.to_string(),
                entry_price,
                exit_price,
                stop_loss: 0.0,
                take_profit: 0.0,
                position_size: 0.0,
                pnl,
                pnl_pct,
                outcome: outcome.to_string(),
                exit_reason: exit_reason.to_string(),
                regret_score,
                lesson: String::new(),
                confluence_score: 0.0,
                portfolio_heat: 0.0,
                market_regime: regime_str,
                session: String::new(),
                agent_reasoning: String::new(),
                consecutive_losses_at_entry: portfolio.consecutive_losses,
                entry_time: chrono::Utc::now().to_rfc3339(),
                exit_time: chrono::Utc::now().to_rfc3339(),
                rule_version: 0,
                was_correct: pnl > 0.0,
            },
        );

        // Simple pattern learning: log high-regret trades
        if regret_score > 0.5 {
            println!(
                "[Observation] High regret trade: {} {} — PnL: {:.2} — Consider review",
                direction, symbol, pnl
            );
        }
    }

    /// Get recent performance summary.
    pub async fn get_summary(&self) -> ObservationSummary {
        let stats = self.state.agent_memory.episode_store.kelly_trade_stats(100);
        ObservationSummary {
            total_trades: stats.trade_count as usize,
            win_rate: stats.win_probability,
            avg_regret: 0.0,
            recent_outcome: None,
            rules_discovered: 0,
        }
    }

    /// Produce reasoning chain.
    pub fn reason(&self, summary: &ObservationSummary) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Observation", "ALL");

        chain.add_step(
            &format!("Performance: {} trades, {:.1}% win rate", summary.total_trades, summary.win_rate * 100.0),
            "Tracked trade outcomes and computed win rate from episode store",
            vec![format!("trades={}", summary.total_trades), format!("wr={:.1}%", summary.win_rate * 100.0)],
            0.9,
        );

        chain.finalize(&format!(
            "Observed {} trades with {:.1}% win rate",
            summary.total_trades, summary.win_rate * 100.0
        ));
        chain
    }
}
