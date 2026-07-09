//! # ReconciliationEngine — Broker vs Local Portfolio Reconciliation
//!
//! Periodically compares the broker's actual positions (from `BrokerAdapter::get_positions`)
//! against the local `PortfolioState` and reports discrepancies.
//!
//! ## Scenarios Detected
//! - **Phantom Position**: Position exists locally but not on broker (likely filled or cancelled
//!   before RAT recorded it) → auto-close local position with a warning.
//! - **Ghost Position**: Position exists on broker but not locally (e.g., placed from another app)
//!   → import into local portfolio.
//! - **Size Mismatch**: Different quantities for the same symbol → alert, use broker's count.
//! - **Price Staleness**: Local price significantly different from broker's mark price → update local.
//!
//! ## Alert Flow
//! All discrepancies are logged via COT for real-time TUI display and through
//! the `cotrader_core::notifier` for push alerts.

use crate::state::SharedState;
use chrono::Utc;
use cotrader_core::paper_engine::{BrokerAdapter, Position, PositionStatus};
use cotrader_core::TradeDirection;
use std::sync::Arc;

// ── Discrepancy Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Discrepancy {
    /// Position exists locally but not on broker
    PhantomPosition {
        symbol: String,
        local_qty: f64,
        local_entry: f64,
    },
    /// Position exists on broker but not locally
    GhostPosition {
        symbol: String,
        broker_qty: f64,
        broker_entry: f64,
        broker_current: f64,
    },
    /// Quantity differs between broker and local
    SizeMismatch {
        symbol: String,
        local_qty: f64,
        broker_qty: f64,
        local_entry: f64,
        broker_entry: f64,
    },
    /// Price is significantly stale
    PriceStaleness {
        symbol: String,
        local_price: f64,
        broker_price: f64,
        diff_pct: f64,
    },
}

impl std::fmt::Display for Discrepancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Discrepancy::PhantomPosition {
                symbol,
                local_qty,
                local_entry,
            } => {
                write!(
                    f,
                    "PHANTOM: {} {}@{} — not on broker, closing local",
                    symbol, local_qty, local_entry
                )
            }
            Discrepancy::GhostPosition {
                symbol,
                broker_qty,
                broker_entry,
                broker_current,
            } => {
                write!(
                    f,
                    "GHOST: {} {}@{} (cur={}) — not in local, importing",
                    symbol, broker_qty, broker_entry, broker_current
                )
            }
            Discrepancy::SizeMismatch {
                symbol,
                local_qty,
                broker_qty,
                local_entry,
                broker_entry,
            } => {
                write!(
                    f,
                    "SIZE: {} local={}@{} vs broker={}@{}",
                    symbol, local_qty, local_entry, broker_qty, broker_entry
                )
            }
            Discrepancy::PriceStaleness {
                symbol,
                local_price,
                broker_price,
                diff_pct,
            } => {
                write!(
                    f,
                    "PRICE: {} local={:.2} vs broker={:.2} ({:+.2}%)",
                    symbol, local_price, broker_price, diff_pct
                )
            }
        }
    }
}

// ── ReconciliationReport ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ReconciliationReport {
    pub discrepancies: Vec<Discrepancy>,
    pub actions_taken: Vec<String>,
    pub auto_closed: Vec<String>,
    pub auto_imported: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ReconciliationReport {
    pub fn has_issues(&self) -> bool {
        !self.discrepancies.is_empty()
    }

    pub fn summary(&self) -> String {
        if !self.has_issues() {
            return "✅ Reconciliation OK — no discrepancies".to_string();
        }
        let mut lines = vec![format!(
            "⚠ Reconciliation: {} discrepancies, {} auto-closed, {} auto-imported",
            self.discrepancies.len(),
            self.auto_closed.len(),
            self.auto_imported.len()
        )];
        for d in &self.discrepancies {
            lines.push(format!("  • {}", d));
        }
        lines.join("\n")
    }
}

// ── ReconciliationEngine ──────────────────────────────────────────────────────

pub struct ReconciliationEngine {
    state: SharedState,
    /// Price staleness threshold (as percentage difference)
    price_staleness_threshold_pct: f64,
}

impl ReconciliationEngine {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            price_staleness_threshold_pct: 1.0, // 1% difference triggers alert
        }
    }

    /// Read local portfolio positions into the `Position` format used by reconciliation.
    async fn get_local_positions(&self) -> Vec<Position> {
        let portfolio = self.state.portfolio_store.portfolio.read().await;
        let mut positions = Vec::new();
        for pos in &portfolio.open_positions {
            positions.push(Position {
                id: format!("local-{}", pos.symbol),
                symbol: pos.symbol.clone(),
                direction: pos.direction,
                qty: pos.quantity,
                entry_price: pos.entry_price,
                current_price: pos.current_price,
                stop_loss: pos.stop_loss,
                take_profit: pos.take_profit,
                unrealized_pnl: pos.unrealized_pnl,
                unrealized_pnl_pct: pos.unrealized_pnl_pct,
                status: PositionStatus::Open,
                opened_at: pos.entry_time,
                closed_at: None,
                strategy: Some("rat-auto".to_string()),
                order_id: String::new(),
            });
        }
        positions
    }

    /// Get the preferred broker for reconciliation (live broker if available, else active).
    async fn get_recon_broker(&self) -> Arc<dyn BrokerAdapter> {
        match self.state.portfolio_store.broker_registry.live_broker().await {
            Some(live) => {
                println!("[Reconciliation] Using live broker ({})", live.broker_name());
                live
            }
            None => {
                let b = self.state.portfolio_store.broker_registry.active_broker().await;
                println!("[Reconciliation] Using active broker ({})", b.broker_name());
                b
            }
        }
    }

    /// Run a full reconciliation cycle: compare broker positions vs local portfolio.
    /// Returns a report of all discrepancies found and any auto-reconciliation actions taken.
    pub async fn reconcile(&self) -> ReconciliationReport {
        let mut report = ReconciliationReport {
            timestamp: Utc::now(),
            ..Default::default()
        };

        // 1. Get broker positions
        let broker = self.get_recon_broker().await;
        let broker_positions = match broker.get_positions().await {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("[Reconciliation] ⚠ Failed to fetch broker positions: {}", e);
                eprintln!("{}", msg);
                report.actions_taken.push(msg);
                return report;
            }
        };

        // 2. Get local positions
        let local_positions = self.get_local_positions().await;

        // 3. Compare: Find phantom positions (local but not on broker)
        for local_pos in &local_positions {
            let on_broker = broker_positions
                .iter()
                .any(|bp| bp.symbol == local_pos.symbol);

            if !on_broker {
                report.discrepancies.push(Discrepancy::PhantomPosition {
                    symbol: local_pos.symbol.clone(),
                    local_qty: local_pos.qty,
                    local_entry: local_pos.entry_price,
                });

                // Auto-close phantom positions — they were likely filled externally
                // or the position was closed on the exchange without RAT's knowledge
                let pm = crate::portfolio_manager::PortfolioManagerAgent::new(self.state.clone());
                match pm
                    .close_position(&local_pos.symbol, local_pos.current_price)
                    .await
                {
                    Ok(pnl) => {
                        let msg = format!(
                            "Auto-closed phantom {} @ {:.2} P&L=${:.2}",
                            local_pos.symbol, local_pos.current_price, pnl
                        );
                        report.auto_closed.push(msg.clone());
                        report.actions_taken.push(msg);
                    }
                    Err(e) => {
                        let msg =
                            format!("Failed to auto-close phantom {}: {}", local_pos.symbol, e);
                        report.actions_taken.push(msg);
                    }
                }
            }
        }

        // 4. Compare: Find ghost positions (on broker but not local)
        //    and size mismatches / price staleness
        for broker_pos in &broker_positions {
            let local_match = local_positions
                .iter()
                .find(|lp| lp.symbol == broker_pos.symbol);

            match local_match {
                None => {
                    // Ghost position — exists on broker but not in local portfolio
                    report.discrepancies.push(Discrepancy::GhostPosition {
                        symbol: broker_pos.symbol.clone(),
                        broker_qty: broker_pos.qty,
                        broker_entry: broker_pos.entry_price,
                        broker_current: broker_pos.current_price,
                    });

                    // Auto-import ghost positions (conservative: add to local portfolio)
                    let signal = crate::types::TradeSignal {
                        symbol: broker_pos.symbol.clone(),
                        direction: broker_pos.direction,
                        entry_price: broker_pos.entry_price,
                        stop_loss: 0.0,
                        take_profit: 0.0,
                        position_size: broker_pos.qty as f64,
                        confidence_score: 0.5,
                        confluence_score: 0.5,
                        risk_reward_ratio: 0.0,
                        reasoning: format!(
                            "Auto-imported from broker reconciliation (qty={}, entry={})",
                            broker_pos.qty, broker_pos.entry_price
                        ),
                        timestamp: Utc::now(),
                        session_valid: true,
                        risk_check_passed: true,
                    };

                    let pm =
                        crate::portfolio_manager::PortfolioManagerAgent::new(self.state.clone());
                    match pm.add_position(&signal).await {
                        Ok(()) => {
                            let msg = format!(
                                "Auto-imported ghost {} {}@{}",
                                broker_pos.symbol, broker_pos.qty, broker_pos.entry_price
                            );
                            report.auto_imported.push(msg.clone());
                            report.actions_taken.push(msg);
                        }
                        Err(e) => {
                            let msg =
                                format!("Failed to import ghost {}: {}", broker_pos.symbol, e);
                            report.actions_taken.push(msg);
                        }
                    }
                }
                Some(local_pos) => {
                    // Check size mismatch
                    if local_pos.qty != broker_pos.qty {
                        report.discrepancies.push(Discrepancy::SizeMismatch {
                            symbol: broker_pos.symbol.clone(),
                            local_qty: local_pos.qty,
                            broker_qty: broker_pos.qty,
                            local_entry: local_pos.entry_price,
                            broker_entry: broker_pos.entry_price,
                        });

                        // Update local qty to match broker (broker is source of truth)
                        let mut portfolio = self.state.portfolio_store.portfolio.write().await;
                        if let Some(lp) = portfolio
                            .open_positions
                            .iter_mut()
                            .find(|p| p.symbol == broker_pos.symbol)
                        {
                            lp.quantity = broker_pos.qty as f64;
                            report.actions_taken.push(format!(
                                "Updated {} qty from {} to {} (broker source of truth)",
                                broker_pos.symbol, local_pos.qty, broker_pos.qty
                            ));
                        }
                        drop(portfolio);
                    }

                    // Check price staleness
                    if local_pos.current_price > 0.0 && broker_pos.current_price > 0.0 {
                        let diff_pct = ((broker_pos.current_price - local_pos.current_price)
                            / local_pos.current_price)
                            .abs()
                            * 100.0;
                        if diff_pct > self.price_staleness_threshold_pct {
                            report.discrepancies.push(Discrepancy::PriceStaleness {
                                symbol: broker_pos.symbol.clone(),
                                local_price: local_pos.current_price,
                                broker_price: broker_pos.current_price,
                                diff_pct,
                            });

                            // Update local price to match broker
                            if let Some(pnl) = self                        .state
                        .portfolio_store.portfolio
                        .write()
                        .await
                        .open_positions
                        .iter_mut()
                        .find(|p| p.symbol == broker_pos.symbol)
                            {
                                pnl.current_price = broker_pos.current_price;
                                pnl.unrealized_pnl = match pnl.direction {
                                    TradeDirection::Long => {
                                        (broker_pos.current_price - pnl.entry_price) * pnl.quantity
                                    }
                                    TradeDirection::Short => {
                                        (pnl.entry_price - broker_pos.current_price) * pnl.quantity
                                    }
                                };
                                pnl.unrealized_pnl_pct = if pnl.entry_price > 0.0 {
                                    (pnl.unrealized_pnl / (pnl.entry_price * pnl.quantity)) * 100.0
                                } else {
                                    0.0
                                };

                                report.actions_taken.push(format!(
                                    "Updated {} price from {:.2} to {:.2} (broker source of truth)",
                                    broker_pos.symbol,
                                    local_pos.current_price,
                                    broker_pos.current_price
                                ));
                            }
                        }
                    }
                }
            }
        }

        // 5. Log discrepancies via COT
        if report.has_issues() {
            let summary = report.summary();
            let _ = self
                .state
                .push_cot(
                    "ReconciliationEngine",
                    "Broker reconciliation cycle",
                    if !report.auto_closed.is_empty() || !report.auto_imported.is_empty() {
                        "AUTO_RECONCILED"
                    } else {
                        "DISCREPANCIES"
                    },
                    &summary,
                    0.5,
                    0,
                    None,
                    None,
                )
                .await;

            // Send push notification for critical issues
            if !report.auto_closed.is_empty() || !report.discrepancies.is_empty() {
                cotrader_core::notifier::alert(
                    "Live Broker Reconciliation — Discrepancies Found",
                    &summary,
                )
                .await;
            }
        } else {
            let _ = self
                .state
                .push_cot(
                    "ReconciliationEngine",
                    "Broker reconciliation cycle",
                    "OK",
                    "No discrepancies — local portfolio matches broker",
                    0.95,
                    0,
                    None,
                    None,
                )
                .await;
        }

        report
    }

    /// Force-sync local portfolio positions to the live broker (e.g. Tredo Exchange).
    ///
    /// This is the **push-direction** counterpart to `reconcile()`. While `reconcile()`
    /// detects discrepancies and adjusts the local portfolio to match the broker,
    /// `force_sync_positions()` pushes local state **to** the broker so Tredo
    /// always mirrors what the orchestrator thinks it should hold.
    ///
    /// ## Sync Logic
    /// 1. Positions that exist locally but not on the broker → `place_order()` on broker
    /// 2. Positions that exist on the broker but not locally → `close_position()` on broker
    /// 3. Positions that exist on both but have different quantities → close + re-place with correct qty
    ///
    /// Returns a report with all actions taken.
    pub async fn force_sync_positions(&self) -> ReconciliationReport {
        let mut report = ReconciliationReport {
            timestamp: Utc::now(),
            ..Default::default()
        };

        // 1. Get the live broker — force sync only makes sense against an external exchange
        let broker = match self.state.portfolio_store.broker_registry.live_broker().await {
            Some(live) => live,
            None => {
                let msg = "[ForceSync] No live broker registered — skipping position sync".to_string();
                println!("{}", msg);
                report.actions_taken.push(msg);
                return report;
            }
        };

        println!("[ForceSync] Using live broker ({}) for position sync", broker.broker_name());

        // 2. Get broker positions
        let broker_positions = match broker.get_positions().await {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("[ForceSync] ⚠ Failed to fetch broker positions: {}", e);
                eprintln!("{}", msg);
                report.actions_taken.push(msg);
                return report;
            }
        };

        // 3. Get local positions
        let local_positions = self.get_local_positions().await;

        // 4. For each local position: ensure it exists on the broker
        for local_pos in &local_positions {
            let broker_match = broker_positions
                .iter()
                .find(|bp| bp.symbol == local_pos.symbol);

            match broker_match {
                None => {
                    // Position exists locally but NOT on broker → place order to open it
                    let order_req = cotrader_core::paper_engine::OrderRequest {
                        symbol: local_pos.symbol.clone(),
                        direction: local_pos.direction,
                        order_type: cotrader_core::paper_engine::OrderType::Market,
                        qty: local_pos.qty,
                        price: Some(local_pos.entry_price),
                        stop_loss: Some(local_pos.stop_loss),
                        take_profit: Some(local_pos.take_profit),
                        strategy: Some("position-sync".to_string()),
                        client_order_id: None,
                    };

                    match broker.place_order(order_req, local_pos.current_price).await {
                        Ok(order_id) => {
                            let msg = format!(
                                "Synced {} {}@{} to broker (order={})",
                                local_pos.symbol, local_pos.qty, local_pos.entry_price, order_id
                            );
                            println!("[ForceSync] {}", msg);
                            report.actions_taken.push(msg);
                        }
                        Err(e) => {
                            let msg = format!(
                                "Failed to sync {} to broker: {}",
                                local_pos.symbol, e
                            );
                            eprintln!("[ForceSync] ⚠ {}", msg);
                            report.actions_taken.push(msg);
                        }
                    }
                }
                Some(bp) => {
                    // Position exists on both — check if quantities match
                    if (local_pos.qty - bp.qty).abs() > 0.0001 {
                        // Quantity mismatch: close existing position on broker,
                        // then re-place with the correct quantity
                        let close_msg = format!(
                            "Size mismatch for {}: local={} broker={} — re-syncing",
                            local_pos.symbol, local_pos.qty, bp.qty
                        );
                        println!("[ForceSync] {}", close_msg);
                        report.actions_taken.push(close_msg);

                        // Close the existing position on broker
                        if let Err(e) = broker.close_position(&bp.id, bp.current_price).await {
                            let msg = format!(
                                "Failed to close {} on broker for re-sync: {}",
                                local_pos.symbol, e
                            );
                            eprintln!("[ForceSync] ⚠ {}", msg);
                            report.actions_taken.push(msg);
                            continue;
                        }

                        // Place new position with correct qty
                        let order_req = cotrader_core::paper_engine::OrderRequest {
                            symbol: local_pos.symbol.clone(),
                            direction: local_pos.direction,
                            order_type: cotrader_core::paper_engine::OrderType::Market,
                            qty: local_pos.qty,
                            price: Some(local_pos.entry_price),
                            stop_loss: Some(local_pos.stop_loss),
                            take_profit: Some(local_pos.take_profit),
                            strategy: Some("position-sync".to_string()),
                            client_order_id: None,
                        };

                        match broker.place_order(order_req, local_pos.current_price).await {
                            Ok(order_id) => {
                                let msg = format!(
                                    "Re-synced {} {}@{} to broker (order={})",
                                    local_pos.symbol, local_pos.qty, local_pos.entry_price, order_id
                                );
                                println!("[ForceSync] {}", msg);
                                report.actions_taken.push(msg);
                            }
                            Err(e) => {
                                let msg = format!(
                                    "Failed to re-sync {} to broker: {}",
                                    local_pos.symbol, e
                                );
                                eprintln!("[ForceSync] ⚠ {}", msg);
                                report.actions_taken.push(msg);
                            }
                        }
                    } else {
                        // Quantities match — position is already in sync
                        println!(
                            "[ForceSync] {} already in sync on broker (qty={})",
                            local_pos.symbol, local_pos.qty
                        );
                    }
                }
            }
        }

        // 5. Close any broker positions that don't exist locally (ghost positions)
        for bp in &broker_positions {
            let in_local = local_positions
                .iter()
                .any(|lp| lp.symbol == bp.symbol);

            if !in_local {
                match broker.close_position(&bp.id, bp.current_price).await {
                    Ok(_) => {
                        let msg = format!(
                            "Closed ghost position {} on broker (qty={}, entry={})",
                            bp.symbol, bp.qty, bp.entry_price
                        );
                        println!("[ForceSync] {}", msg);
                        report.actions_taken.push(msg);
                    }
                    Err(e) => {
                        let msg = format!("Failed to close ghost {} on broker: {}", bp.symbol, e);
                        eprintln!("[ForceSync] ⚠ {}", msg);
                        report.actions_taken.push(msg);
                    }
                }
            }
        }

        // 6. Summary log
        if report.actions_taken.is_empty() {
            println!("[ForceSync] All positions already in sync with broker");
            report.actions_taken.push("All positions already in sync with broker".to_string());
        } else {
            println!(
                "[ForceSync] Sync complete — {} action(s) taken",
                report.actions_taken.len()
            );
        }

        report
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SharedState;
    use async_trait::async_trait;
    use cotrader_core::paper_engine::{
        BrokerAdapter, CloseReason, ClosedTrade, OrderRequest, OrderStatus, PortfolioSummary,
        Position, PositionStatus, RiskCheckResult, TradingMode,
    };
    use cotrader_core::{Config, DisciplineRules, MemoryStore, TradeDirection};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // ── RecordingBroker: test mock that records whether get_positions() was called ──

    struct RecordingBroker {
        name: String,
        positions_called: Arc<AtomicBool>,
        order_placed: Arc<AtomicBool>,
        position_closed: Arc<AtomicBool>,
        return_error: bool,
        place_order_error: bool,
        mode: TradingMode,
        returned_positions: Vec<Position>,
    }

    impl RecordingBroker {
        fn new(name: &str, mode: TradingMode) -> (Self, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
            let called = Arc::new(AtomicBool::new(false));
            let placed = Arc::new(AtomicBool::new(false));
            let closed = Arc::new(AtomicBool::new(false));
            (
                Self {
                    name: name.to_string(),
                    positions_called: called.clone(),
                    order_placed: placed.clone(),
                    position_closed: closed.clone(),
                    return_error: false,
                    place_order_error: false,
                    mode,
                    returned_positions: Vec::new(),
                },
                called,
                placed,
                closed,
            )
        }

        fn with_error(mut self) -> Self {
            self.return_error = true;
            self
        }

        fn with_place_order_error(mut self) -> Self {
            self.place_order_error = true;
            self
        }

        fn with_positions(mut self, positions: Vec<Position>) -> Self {
            self.returned_positions = positions;
            self
        }
    }

    #[async_trait]
    impl BrokerAdapter for RecordingBroker {
        async fn connect(&self) -> Result<(), String> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), String> {
            Ok(())
        }
        async fn place_order(&self, _req: OrderRequest, _price: f64) -> Result<String, String> {
            self.order_placed.store(true, Ordering::SeqCst);
            if self.place_order_error {
                Err("Simulated place_order failure".to_string())
            } else {
                Ok("test-order".to_string())
            }
        }
        async fn cancel_order(&self, _id: &str) -> Result<(), String> {
            Ok(())
        }
        async fn get_positions(&self) -> Result<Vec<Position>, String> {
            self.positions_called.store(true, Ordering::SeqCst);
            if self.return_error {
                Err("Test error from broker".to_string())
            } else if !self.returned_positions.is_empty() {
                Ok(self.returned_positions.clone())
            } else {
                Ok(vec![])
            }
        }
        async fn get_summary(&self) -> Result<PortfolioSummary, String> {
            Ok(PortfolioSummary::default())
        }
        async fn get_order_status(&self, _id: &str) -> Result<OrderStatus, String> {
            Ok(OrderStatus::Filled)
        }
        async fn get_recent_trades(&self, _limit: usize) -> Result<Vec<ClosedTrade>, String> {
            Ok(vec![])
        }
        async fn update_price(
            &self,
            _sym: &str,
            _price: f64,
        ) -> Result<Vec<ClosedTrade>, String> {
            Ok(vec![])
        }
        async fn close_position(
            &self,
            _id: &str,
            _price: f64,
        ) -> Result<ClosedTrade, String> {
            self.position_closed.store(true, Ordering::SeqCst);
            Ok(ClosedTrade {
                id: "closed".to_string(),
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
        async fn check_risk(
            &self,
            _sym: &str,
            _cost: f64,
        ) -> Result<RiskCheckResult, String> {
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
            self.mode
        }
        fn broker_name(&self) -> &str {
            &self.name
        }
    }

    // ── Test helpers ────────────────────────────────────────────────────────

    /// Create a minimal SharedState for testing reconciliation broker selection.
    /// Uses temp files for the redb and episode store.
    async fn create_test_state(
        paper_broker: Arc<dyn BrokerAdapter>,
        live_broker: Option<Arc<dyn BrokerAdapter>>,
    ) -> SharedState {
        let tmp_dir = std::env::temp_dir().join(format!(
            "rat_recon_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let redb_path = tmp_dir.join("test.redb");
        let ep_db_path = tmp_dir.join("test.db");

        let memory = MemoryStore::new(
            redb_path.to_str().expect("valid redb path"),
        )
        .expect("MemoryStore creation");
        let rules = DisciplineRules::default();
        let config = Config::default();

        let mut state = SharedState::new(
            memory,
            rules,
            config,
            ep_db_path.to_str().expect("valid db path"),
            paper_broker,
        )
        .expect("SharedState creation");

        if let Some(live) = live_broker {
            state
                .portfolio_store
                .broker_registry
                .register_live_broker(live)
                .await;
        }

        state
    }

    // ── Broker Selection Tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_reconcile_prefers_live_broker() {
        // When a live broker is registered, reconcile() should use it
        // (not the active/paper broker)
        let (paper_broker, paper_called, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);
        let (live_broker, live_called, _, _) =
            RecordingBroker::new("LiveExchange", TradingMode::Live);

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        let engine = ReconciliationEngine::new(state);
        let report = engine.reconcile().await;

        // Live broker's get_positions should have been called
        assert!(
            live_called.load(Ordering::SeqCst),
            "Reconcile should use live_broker() when a live broker is registered"
        );
        // Paper broker should NOT have been called
        assert!(
            !paper_called.load(Ordering::SeqCst),
            "Reconcile should NOT fall back to active_broker() when live_broker is available"
        );
        // No discrepancies expected
        assert!(!report.has_issues(), "No discrepancies with empty positions");
    }

    #[tokio::test]
    async fn test_reconcile_falls_back_to_active_broker() {
        // When NO live broker is registered, reconcile() should use
        // active_broker() (which is the paper broker in paper mode)
        let (paper_broker, paper_called, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);

        let state = create_test_state(Arc::new(paper_broker), None).await;

        let engine = ReconciliationEngine::new(state);
        let report = engine.reconcile().await;

        // Paper broker's get_positions should have been called
        assert!(
            paper_called.load(Ordering::SeqCst),
            "Reconcile should fall back to active_broker() when no live broker is registered"
        );
        // No discrepancies expected
        assert!(!report.has_issues(), "No discrepancies with empty positions");
    }

    #[tokio::test]
    async fn test_reconcile_handles_broker_error_gracefully() {
        // When get_positions() returns an error, reconcile() should
        // catch it, log it, and return an empty report (not panic)
        let (paper_broker, _paper_called, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);
        let (live_broker, _live_called, _, _) = RecordingBroker::new("FailingExchange", TradingMode::Live);
        let live_broker = live_broker.with_error();

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        let engine = ReconciliationEngine::new(state);
        let report = engine.reconcile().await;

        // Report should have no discrepancies (we returned early)
        assert!(!report.has_issues(), "Report should have no discrepancies after error");
        // But it should have recorded the error in actions_taken
        assert!(
            !report.actions_taken.is_empty(),
            "Report should record the broker error in actions_taken"
        );
        let error_msg = &report.actions_taken[0];
        assert!(
            error_msg.contains("Failed to fetch broker positions"),
            "Error message should mention fetch failure, got: {}",
            error_msg
        );
        assert!(
            error_msg.contains("Test error from broker"),
            "Error message should include the broker's error, got: {}",
            error_msg
        );
    }

    // ── Force Sync Tests ─────────────────────────────────────────────────

    fn make_test_position(symbol: &str, direction: TradeDirection, qty: f64, entry: f64) -> Position {
        Position {
            id: format!("test-{}", symbol),
            symbol: symbol.to_string(),
            direction,
            qty,
            entry_price: entry,
            current_price: entry,
            stop_loss: entry * 0.95,
            take_profit: entry * 1.05,
            unrealized_pnl: 0.0,
            unrealized_pnl_pct: 0.0,
            status: PositionStatus::Open,
            opened_at: chrono::Utc::now(),
            closed_at: None,
            strategy: Some("test".to_string()),
            order_id: String::new(),
        }
    }

    #[tokio::test]
    async fn test_force_sync_no_live_broker_skips() {
        // When no live broker is registered, force_sync should skip gracefully
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);

        let state = create_test_state(Arc::new(paper_broker), None).await;
        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        // Should have recorded the "no live broker" message
        assert!(
            report.actions_taken.iter().any(|a| a.contains("No live broker registered")),
            "Should report no live broker: {:?}",
            report.actions_taken
        );
        assert!(!report.has_issues(), "No discrepancies expected");
    }

    #[tokio::test]
    async fn test_force_sync_empty_state_does_nothing() {
        // With empty local and empty broker, no sync actions needed
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);
        let (live_broker, _, placed, closed) =
            RecordingBroker::new("LiveExchange", TradingMode::Live);

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        assert!(!placed.load(Ordering::SeqCst), "place_order should NOT be called");
        assert!(!closed.load(Ordering::SeqCst), "close_position should NOT be called");
        assert!(
            report.actions_taken.iter().any(|a| a.contains("already in sync")),
            "Should report already in sync: {:?}",
            report.actions_taken
        );
    }

    #[tokio::test]
    async fn test_force_sync_pushes_local_position_to_broker() {
        // Local has BTC position, broker has none -> place_order should be called
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);
        let (live_broker, _, placed, closed) =
            RecordingBroker::new("LiveExchange", TradingMode::Live);

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        // Add a local BTC position
        {
            let signal = crate::types::TradeSignal {
                symbol: "BTC".to_string(),
                direction: TradeDirection::Long,
                entry_price: 50000.0,
                stop_loss: 47500.0,
                take_profit: 52500.0,
                position_size: 1.0,
                confidence_score: 0.8,
                confluence_score: 0.7,
                risk_reward_ratio: 2.0,
                reasoning: "Force sync test".to_string(),
                timestamp: chrono::Utc::now(),
                session_valid: true,
                risk_check_passed: true,
            };
            let pm = crate::portfolio_manager::PortfolioManagerAgent::new(state.clone());
            pm.add_position(&signal).await.unwrap();
        }

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        assert!(
            placed.load(Ordering::SeqCst),
            "place_order should be called to sync BTC to broker"
        );
        assert!(!closed.load(Ordering::SeqCst), "close_position should NOT be called");
        assert!(
            report.actions_taken.iter().any(|a| a.contains("Synced BTC")),
            "Should report syncing BTC: {:?}",
            report.actions_taken
        );
    }

    #[tokio::test]
    async fn test_force_sync_closes_ghost_on_broker() {
        // Broker has BTC position, local has none -> close_position should be called
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);

        let btc_pos = make_test_position("BTC", TradeDirection::Long, 1.0, 50000.0);
        let (live_broker, _, placed, closed) = RecordingBroker::new("LiveExchange", TradingMode::Live);
        let live_broker = live_broker.with_positions(vec![btc_pos]);

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        assert!(!placed.load(Ordering::SeqCst), "place_order should NOT be called");
        assert!(
            closed.load(Ordering::SeqCst),
            "close_position should be called to close ghost BTC on broker"
        );
        assert!(
            report.actions_taken.iter().any(|a| a.contains("Closed ghost") && a.contains("BTC")),
            "Should report closing ghost BTC: {:?}",
            report.actions_taken
        );
    }

    #[tokio::test]
    async fn test_force_sync_does_nothing_when_in_sync() {
        // Both local and broker have matching BTC position -> no actions
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);

        let btc_pos = make_test_position("BTC", TradeDirection::Long, 1.0, 50000.0);
        let (live_broker, _, placed, closed) = RecordingBroker::new("LiveExchange", TradingMode::Live);
        let live_broker = live_broker.with_positions(vec![btc_pos]);

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        // Add a matching local BTC position
        {
            let signal = crate::types::TradeSignal {
                symbol: "BTC".to_string(),
                direction: TradeDirection::Long,
                entry_price: 50000.0,
                stop_loss: 47500.0,
                take_profit: 52500.0,
                position_size: 1.0,
                confidence_score: 0.8,
                confluence_score: 0.7,
                risk_reward_ratio: 2.0,
                reasoning: "Force sync test".to_string(),
                timestamp: chrono::Utc::now(),
                session_valid: true,
                risk_check_passed: true,
            };
            let pm = crate::portfolio_manager::PortfolioManagerAgent::new(state.clone());
            pm.add_position(&signal).await.unwrap();
        }

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        assert!(!placed.load(Ordering::SeqCst), "place_order should NOT be called");
        assert!(!closed.load(Ordering::SeqCst), "close_position should NOT be called");
        assert!(
            report.actions_taken.iter().any(|a| a.contains("already in sync")),
            "Should report already in sync: {:?}",
            report.actions_taken
        );
    }

    #[tokio::test]
    async fn test_force_sync_reconciles_size_mismatch() {
        // Local has BTC at qty 1.0, broker has BTC at qty 2.0 -> close + re-place
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);

        let btc_pos = make_test_position("BTC", TradeDirection::Long, 2.0, 50000.0);
        let (live_broker, _, placed, closed) = RecordingBroker::new("LiveExchange", TradingMode::Live);
        let live_broker = live_broker.with_positions(vec![btc_pos]);

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        // Add local BTC position with DIFFERENT qty (1.0 vs broker's 2.0)
        {
            let signal = crate::types::TradeSignal {
                symbol: "BTC".to_string(),
                direction: TradeDirection::Long,
                entry_price: 50000.0,
                stop_loss: 47500.0,
                take_profit: 52500.0,
                position_size: 1.0,
                confidence_score: 0.8,
                confluence_score: 0.7,
                risk_reward_ratio: 2.0,
                reasoning: "Size mismatch test".to_string(),
                timestamp: chrono::Utc::now(),
                session_valid: true,
                risk_check_passed: true,
            };
            let pm = crate::portfolio_manager::PortfolioManagerAgent::new(state.clone());
            pm.add_position(&signal).await.unwrap();
        }

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        // Both close and place should be called for re-sync
        assert!(closed.load(Ordering::SeqCst), "close_position should be called to close mismatched position");
        assert!(placed.load(Ordering::SeqCst), "place_order should be called to re-place with correct qty");
        assert!(
            report.actions_taken.iter().any(|a| a.contains("Size mismatch")),
            "Should report size mismatch: {:?}",
            report.actions_taken
        );
    }

    #[tokio::test]
    async fn test_force_sync_handles_broker_error_gracefully() {
        // When broker.get_positions() fails, force_sync should handle it gracefully
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);
        let (live_broker, _, _, _) = RecordingBroker::new("FailingExchange", TradingMode::Live);
        let live_broker = live_broker.with_error();

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        assert!(!report.has_issues(), "No discrepancies expected");
        assert!(
            report.actions_taken.iter().any(|a| a.contains("Failed to fetch broker positions")),
            "Should report broker error: {:?}",
            report.actions_taken
        );
    }

    #[tokio::test]
    async fn test_force_sync_partial_failure_close_succeeds_place_fails() {
        // Size mismatch: close_position succeeds, but place_order fails.
        // This simulates the case where Tredo accepts the close but then
        // rejects the re-place order (e.g. network blip, risk check failure).
        let (paper_broker, _, _, _) =
            RecordingBroker::new("PaperEngine", TradingMode::Paper);

        let btc_pos = make_test_position("BTC", TradeDirection::Long, 2.0, 50000.0);
        // Configure live broker: return positions (size mismatch will trigger re-sync),
        // and place_order will fail
        let (live_broker, _, placed, closed) = RecordingBroker::new("LiveExchange", TradingMode::Live);
        let live_broker = live_broker
            .with_positions(vec![btc_pos])
            .with_place_order_error();

        let state = create_test_state(
            Arc::new(paper_broker),
            Some(Arc::new(live_broker)),
        )
        .await;

        // Add local BTC position with DIFFERENT qty (1.0 vs broker's 2.0)
        {
            let signal = crate::types::TradeSignal {
                symbol: "BTC".to_string(),
                direction: TradeDirection::Long,
                entry_price: 50000.0,
                stop_loss: 47500.0,
                take_profit: 52500.0,
                position_size: 1.0,
                confidence_score: 0.8,
                confluence_score: 0.7,
                risk_reward_ratio: 2.0,
                reasoning: "Partial failure test".to_string(),
                timestamp: chrono::Utc::now(),
                session_valid: true,
                risk_check_passed: true,
            };
            let pm = crate::portfolio_manager::PortfolioManagerAgent::new(state.clone());
            pm.add_position(&signal).await.unwrap();
        }

        let engine = ReconciliationEngine::new(state);
        let report = engine.force_sync_positions().await;

        // close_position should have been called (the close succeeds)
        assert!(
            closed.load(Ordering::SeqCst),
            "close_position should be called to close mismatched position"
        );
        // place_order should also have been called (the re-place was attempted)
        assert!(
            placed.load(Ordering::SeqCst),
            "place_order should be called to re-place with correct qty (even though it will fail)"
        );
        // The report should contain both the "Size mismatch" message AND
        // the "Failed to re-sync" error (not "Re-synced" success)
        assert!(
            report.actions_taken.iter().any(|a| a.contains("Size mismatch")),
            "Should report size mismatch: {:?}",
            report.actions_taken
        );
        assert!(
            report.actions_taken.iter().any(|a| a.contains("Failed to re-sync")),
            "Should report the re-sync failure: {:?}",
            report.actions_taken
        );
        // Should NOT have a success message
        assert!(
            !report.actions_taken.iter().any(|a| a.contains("Re-synced")),
            "Should NOT report successful re-sync: {:?}",
            report.actions_taken
        );
    }

    // ── Existing Discrepancy Display Tests ─────────────────────────────────

    #[test]
    fn test_discrepancy_display_phantom() {
        let d = Discrepancy::PhantomPosition {
            symbol: "BTC".to_string(),
            local_qty: 1.0,
            local_entry: 50000.0,
        };
        let s = d.to_string();
        assert!(s.contains("PHANTOM"));
        assert!(s.contains("BTC"));
    }

    #[test]
    fn test_discrepancy_display_ghost() {
        let d = Discrepancy::GhostPosition {
            symbol: "ETH".to_string(),
            broker_qty: 2.0,
            broker_entry: 3000.0,
            broker_current: 3100.0,
        };
        let s = d.to_string();
        assert!(s.contains("GHOST"));
        assert!(s.contains("ETH"));
    }

    #[test]
    fn test_discrepancy_display_size() {
        let d = Discrepancy::SizeMismatch {
            symbol: "SOL".to_string(),
            local_qty: 5.0,
            broker_qty: 3.0,
            local_entry: 150.0,
            broker_entry: 155.0,
        };
        let s = d.to_string();
        assert!(s.contains("SIZE"));
        assert!(s.contains("SOL"));
    }

    #[test]
    fn test_report_has_issues() {
        let mut report = ReconciliationReport::default();
        assert!(!report.has_issues());

        report.discrepancies.push(Discrepancy::PhantomPosition {
            symbol: "BTC".to_string(),
            local_qty: 1.0,
            local_entry: 50000.0,
        });
        assert!(report.has_issues());
    }

    #[test]
    fn test_report_summary_ok() {
        let report = ReconciliationReport::default();
        assert!(report.summary().contains("OK"));
    }

    #[test]
    fn test_report_summary_issues() {
        let mut report = ReconciliationReport::default();
        report.discrepancies.push(Discrepancy::PhantomPosition {
            symbol: "BTC".to_string(),
            local_qty: 1.0,
            local_entry: 50000.0,
        });
        let summary = report.summary();
        assert!(summary.contains("PHANTOM"));
        assert!(summary.contains("discrepancies"));
    }
}
