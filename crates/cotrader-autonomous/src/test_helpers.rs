//! Test helpers for cotrader-autonomous unit tests.
//! Provides a simple `DemoBroker` implementing `BrokerAdapter`.

use async_trait::async_trait;
use cotrader_core::paper_engine::{
    BrokerAdapter, CloseReason, ClosedTrade, OrderRequest, OrderStatus, PortfolioSummary,
    Position, RiskCheckResult, TradingMode,
};
use cotrader_core::TradeDirection;

/// A minimal demo broker for unit tests.
/// Provides default stub implementations for all `BrokerAdapter` methods.
pub struct DemoBroker;

#[async_trait]
impl BrokerAdapter for DemoBroker {
    async fn connect(&self) -> Result<(), String> {
        Ok(())
    }
    async fn disconnect(&self) -> Result<(), String> {
        Ok(())
    }
    async fn place_order(&self, _req: OrderRequest, _price: f64) -> Result<String, String> {
        Ok("demo-order".to_string())
    }
    async fn cancel_order(&self, _id: &str) -> Result<(), String> {
        Ok(())
    }
    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        Ok(vec![])
    }
    async fn get_summary(&self) -> Result<PortfolioSummary, String> {
        Ok(PortfolioSummary {
            cash: 100_000.0,
            equity: 100_000.0,
            margin_used: 0.0,
            free_margin: 100_000.0,
            daily_pnl: 0.0,
            daily_pnl_pct: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            win_rate: 0.0,
            consecutive_losses: 0,
            max_drawdown: 0.0,
            max_drawdown_pct: 0.0,
            open_positions: 0,
            total_pnl_all_time: 0.0,
        })
    }
    async fn get_order_status(&self, _id: &str) -> Result<OrderStatus, String> {
        Ok(OrderStatus::Filled)
    }
    async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<ClosedTrade>, String> {
        Ok(vec![])
    }
    async fn update_price(&self, _sym: &str, _price: f64) -> Result<Vec<ClosedTrade>, String> {
        Ok(vec![])
    }
    async fn close_position(
        &self,
        _id: &str,
        _price: f64,
    ) -> Result<ClosedTrade, String> {
        Ok(ClosedTrade {
            id: "closed-1".to_string(),
            symbol: "TEST".to_string(),
            direction: TradeDirection::Long,
            qty: 1.0,
            entry_price: 100.0,
            exit_price: 100.0,
            realized_pnl: 0.0,
            realized_pnl_pct: 0.0,
            close_reason: CloseReason::Manual,
            opened_at: chrono::Utc::now(),
            closed_at: chrono::Utc::now(),
            duration_secs: 0,
            strategy: None,
            order_id: "order-1".to_string(),
        })
    }
    async fn check_risk(&self, _sym: &str, _cost: f64) -> Result<RiskCheckResult, String> {
        Ok(RiskCheckResult {
            passed: true,
            max_position_size_ok: true,
            daily_loss_limit_ok: true,
            drawdown_ok: true,
            concentration_ok: true,
            portfolio_heat_ok: true,
            warnings: vec![],
        })
    }
    async fn reset(&self) -> Result<(), String> {
        Ok(())
    }
    fn mode(&self) -> TradingMode {
        TradingMode::Paper
    }
    fn broker_name(&self) -> &str {
        "Demo"
    }
}
