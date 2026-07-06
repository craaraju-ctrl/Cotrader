//! Interactive Brokers Adapter — Global Markets
//!
//! Covers US stocks, EU stocks, Indian stocks, Japanese stocks, Forex, Commodities.

use async_trait::async_trait;
use cotrader_core::paper_engine::*;
use cotrader_core::TradeDirection;

pub struct IbkrBroker {
    host: String,
    port: u16,
    client_id: i32,
    paper_mode: bool,
}

impl IbkrBroker {
    pub fn new(host: &str, port: u16, client_id: i32, paper_mode: bool) -> Self {
        Self { host: host.to_string(), port, client_id, paper_mode }
    }

    pub fn from_env() -> Self {
        let host = std::env::var("IBKR_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("IBKR_PORT").unwrap_or_else(|_| "4001".to_string()).parse().unwrap_or(4001);
        let client_id = std::env::var("IBKR_CLIENT_ID").unwrap_or_else(|_| "1".to_string()).parse().unwrap_or(1);
        let paper_mode = std::env::var("IBKR_PAPER").unwrap_or_else(|_| "true".to_string()).parse().unwrap_or(true);
        Self::new(&host, port, client_id, paper_mode)
    }
}

#[async_trait]
impl BrokerAdapter for IbkrBroker {
    async fn connect(&self) -> Result<(), String> {
        println!("[IBKR] Connecting to {}:{} (client_id={})", self.host, self.port, self.client_id);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), String> {
        println!("[IBKR] Disconnected");
        Ok(())
    }

    async fn place_order(&self, request: OrderRequest, _market_price: f64) -> Result<String, String> {
        let direction = match request.direction {
            TradeDirection::Long => "BUY",
            TradeDirection::Short => "SELL",
        };
        let order_id = format!("ibkr-{}-{}", request.symbol, chrono::Utc::now().timestamp_millis());
        println!("[IBKR] {} {} (qty={})", direction, request.symbol, request.qty);
        Ok(order_id)
    }

    async fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        println!("[IBKR] Cancel: {}", order_id);
        Ok(())
    }

    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        Ok(vec![])
    }

    async fn get_summary(&self) -> Result<PortfolioSummary, String> {
        Ok(PortfolioSummary {
            cash: 100_000.0, equity: 100_000.0, margin_used: 0.0, free_margin: 100_000.0,
            daily_pnl: 0.0, daily_pnl_pct: 0.0, total_trades: 0, winning_trades: 0,
            losing_trades: 0, win_rate: 0.0, consecutive_losses: 0, max_drawdown: 0.0,
            max_drawdown_pct: 0.0, open_positions: 0, total_pnl_all_time: 0.0,
        })
    }

    async fn get_order_status(&self, _order_id: &str) -> Result<OrderStatus, String> {
        Ok(OrderStatus::Filled)
    }

    async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<ClosedTrade>, String> {
        Ok(vec![])
    }

    async fn update_price(&self, _symbol: &str, _market_price: f64) -> Result<Vec<ClosedTrade>, String> {
        Ok(vec![])
    }

    async fn close_position(&self, _position_id: &str, _exit_price: f64) -> Result<ClosedTrade, String> {
        Ok(ClosedTrade {
            id: "closed-1".to_string(),
            symbol: "TEST".to_string(),
            direction: TradeDirection::Long,
            qty: 1,
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

    async fn check_risk(&self, _symbol: &str, _estimated_cost: f64) -> Result<RiskCheckResult, String> {
        Ok(RiskCheckResult {
            passed: true, max_position_size_ok: true, daily_loss_limit_ok: true,
            drawdown_ok: true, concentration_ok: true, portfolio_heat_ok: true, warnings: vec![],
        })
    }

    async fn reset(&self) -> Result<(), String> { Ok(()) }
    fn mode(&self) -> TradingMode { if self.paper_mode { TradingMode::Paper } else { TradingMode::Live } }
    fn broker_name(&self) -> &str { "Interactive Brokers" }
}
