//! Live broker — safety wrapper that gates real money execution.
//! Requires explicit confirmation before placing orders.
//!
//! FIXED (fault-isolation audit, D8): this module imported the deleted
//! `crate::paper_broker` module and did not compile (`rat-runtime` lib was
//! broken at HEAD). Ported to the unified `cotrader_core::paper_engine::BrokerAdapter`
//! trait so it wraps ANY broker (paper or live) and added the global
//! BreakerNet gate so a halt from any subsystem blocks live orders.

use crate::risk_manager::RiskManager;
use async_trait::async_trait;
use cotrader_core::paper_engine::{
    BrokerAdapter, ClosedTrade, OrderRequest, OrderStatus, PortfolioSummary, Position,
    RiskCheckResult, TradingMode,
};
use std::sync::Arc;

/// Safety gate for live trading — wraps any `BrokerAdapter` with safety checks:
/// 1. RiskManager hard stop
/// 2. Global BreakerNet halt (trading breaker / memory breaker / risk manager)
/// 3. Optional per-trade interactive confirmation
pub struct LiveBrokerSafety {
    inner: Arc<dyn BrokerAdapter>,
    risk_manager: Arc<RiskManager>,
    require_per_trade_confirmation: bool,
}

impl LiveBrokerSafety {
    pub fn new(
        inner: Arc<dyn BrokerAdapter>,
        risk_manager: Arc<RiskManager>,
        require_per_trade_confirmation: bool,
    ) -> Self {
        Self {
            inner,
            risk_manager,
            require_per_trade_confirmation,
        }
    }

    fn pre_trade_gate(&self) -> Result<(), String> {
        // Safety check 1: RiskManager hard stop
        if self.risk_manager.is_hard_stop_engaged() {
            return Err("Hard stop engaged — all live trading suspended".to_string());
        }
        // Safety check 2: global breaker coordination (any subsystem halted)
        if cotrader_core::breaker_net::is_halted() {
            return Err(
                "Global halt active (BreakerNet) — live trading suspended".to_string(),
            );
        }
        Ok(())
    }
}

#[async_trait]
impl BrokerAdapter for LiveBrokerSafety {
    async fn connect(&self) -> Result<(), String> {
        self.inner.connect().await
    }

    async fn disconnect(&self) -> Result<(), String> {
        self.inner.disconnect().await
    }

    async fn place_order(
        &self,
        request: OrderRequest,
        market_price: f64,
    ) -> Result<String, String> {
        self.pre_trade_gate()?;

        // Safety check 3: per-trade confirmation
        if self.require_per_trade_confirmation {
            eprintln!(
                "\n⚠ LIVE TRADE CONFIRMATION REQUIRED ⚠\n\
                 Symbol: {}\nDirection: {:?}\nQty: {}\nMarket price: {}\n\
                 Type 'YES' to confirm: ",
                request.symbol, request.direction, request.qty, market_price
            );
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            if input.trim().to_uppercase() != "YES" {
                return Err("Trade cancelled by user".to_string());
            }
        }

        self.inner.place_order(request, market_price).await
    }

    async fn cancel_order(&self, order_id: &str) -> Result<(), String> {
        // Cancellations are always allowed — they reduce risk.
        self.inner.cancel_order(order_id).await
    }

    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        self.inner.get_positions().await
    }

    async fn get_summary(&self) -> Result<PortfolioSummary, String> {
        self.inner.get_summary().await
    }

    async fn get_order_status(&self, order_id: &str) -> Result<OrderStatus, String> {
        self.inner.get_order_status(order_id).await
    }

    async fn get_recent_trades(&self, limit: usize) -> Result<Vec<ClosedTrade>, String> {
        self.inner.get_recent_trades(limit).await
    }

    async fn update_price(
        &self,
        symbol: &str,
        market_price: f64,
    ) -> Result<Vec<ClosedTrade>, String> {
        self.inner.update_price(symbol, market_price).await
    }

    async fn close_position(
        &self,
        position_id: &str,
        exit_price: f64,
    ) -> Result<ClosedTrade, String> {
        // Closing positions is risk-reducing — allowed even under halt.
        self.inner.close_position(position_id, exit_price).await
    }

    async fn check_risk(
        &self,
        symbol: &str,
        estimated_cost: f64,
    ) -> Result<RiskCheckResult, String> {
        self.inner.check_risk(symbol, estimated_cost).await
    }

    async fn reset(&self) -> Result<(), String> {
        self.inner.reset().await
    }

    fn mode(&self) -> TradingMode {
        self.inner.mode()
    }

    fn broker_name(&self) -> &str {
        "LiveBroker (safety-gated)"
    }
}
