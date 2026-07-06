//! Implementation Agent — Order execution → position management → broker communication.
//!
//! Merges: ExecutionCoordinator, PortfolioManager, LiveOrderManager
//! Handles: Paper trading, live order placement, position tracking, SL/TP monitoring

use super::decision::DecisionResult;
use super::reasoning::ReasoningChain;
use crate::state::SharedState;
use crate::types::{OpenPosition, TradeSignal};
use chrono::Utc;

#[derive(Clone)]
pub struct ImplementationAgent {
    pub state: SharedState,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub symbol: String,
    pub action: String,
    pub executed: bool,
    pub order_id: Option<String>,
    pub fill_price: Option<f64>,
    pub quantity: f64,
    pub sl: f64,
    pub tp: f64,
    pub error: Option<String>,
}

impl ImplementationAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Execute a trade signal through the broker.
    pub async fn execute(&self, signal: &TradeSignal, decision: &DecisionResult) -> ExecutionResult {
        // Block if decision is not actionable
        if decision.action == "HOLD" || decision.action == "BLOCK" {
            return ExecutionResult {
                symbol: signal.symbol.clone(),
                action: decision.action.clone(),
                executed: false,
                order_id: None,
                fill_price: None,
                quantity: 0.0,
                sl: 0.0,
                tp: 0.0,
                error: Some(format!("Decision was {}", decision.action)),
            };
        }

        if self.state.io.config.paper_mode {
            self.execute_paper(signal, decision).await
        } else {
            self.execute_live(signal, decision).await
        }
    }

    /// Paper trade execution — simulates fill and updates portfolio.
    async fn execute_paper(&self, signal: &TradeSignal, decision: &DecisionResult) -> ExecutionResult {
        let fill_price = signal.entry_price;
        let quantity = signal.position_size;
        let order_id = format!("paper-{}-{}", signal.symbol, Utc::now().timestamp_millis());

        println!(
            "[Implementation] PAPER {} {} @ {:.2} qty={:.4} SL={:.2} TP={:.2} (conf={:.0}%)",
            decision.action, signal.symbol, fill_price, quantity,
            signal.stop_loss, signal.take_profit, decision.confidence * 100.0
        );

        // Update portfolio with new position
        {
            let mut portfolio = self.state.portfolio_store.portfolio.write().await;
            let risk_amount = (fill_price - signal.stop_loss).abs() * quantity;

            portfolio.open_positions.push(OpenPosition {
                symbol: signal.symbol.clone(),
                direction: if decision.action == "BUY" {
                    cotrader_core::TradeDirection::Long
                } else {
                    cotrader_core::TradeDirection::Short
                },
                entry_price: fill_price,
                current_price: fill_price,
                quantity,
                stop_loss: signal.stop_loss,
                take_profit: signal.take_profit,
                unrealized_pnl: 0.0,
                unrealized_pnl_pct: 0.0,
                entry_time: Utc::now(),
                risk_amount,
            });

            portfolio.total_trades_today += 1;
            portfolio.last_trade_time = Some(Utc::now());
            portfolio.last_trade_symbol = Some(signal.symbol.clone());
        }

        println!(
            "[Implementation] Position opened: {} {} @ {:.2}",
            decision.action, signal.symbol, fill_price
        );

        ExecutionResult {
            symbol: signal.symbol.clone(),
            action: decision.action.clone(),
            executed: true,
            order_id: Some(order_id),
            fill_price: Some(fill_price),
            quantity,
            sl: signal.stop_loss,
            tp: signal.take_profit,
            error: None,
        }
    }

    /// Live trade execution — calls broker API.
    async fn execute_live(&self, signal: &TradeSignal, decision: &DecisionResult) -> ExecutionResult {
        let broker = self.state.portfolio_store.broker_registry.active_broker().await;

        // Build order request
        let direction = if decision.action == "BUY" {
            cotrader_core::TradeDirection::Long
        } else {
            cotrader_core::TradeDirection::Short
        };

        let order_req = cotrader_core::paper_engine::OrderRequest {
            symbol: signal.symbol.clone(),
            direction,
            order_type: cotrader_core::paper_engine::OrderType::Market,
            qty: (signal.position_size * 1000.0) as i32, // Convert to integer units
            price: None,
            stop_loss: Some(signal.stop_loss),
            take_profit: Some(signal.take_profit),
            strategy: Some("neurosymbolic".to_string()),
            client_order_id: None,
        };

        match broker.place_order(order_req, signal.entry_price).await {
            Ok(order_id) => {
                println!(
                    "[Implementation] LIVE {} {} — order_id={}",
                    decision.action, signal.symbol, order_id
                );

                // Update portfolio
                {
                    let mut portfolio = self.state.portfolio_store.portfolio.write().await;
                    let risk_amount = (signal.entry_price - signal.stop_loss).abs() * signal.position_size;

                    portfolio.open_positions.push(OpenPosition {
                        symbol: signal.symbol.clone(),
                        direction: if decision.action == "BUY" {
                            cotrader_core::TradeDirection::Long
                        } else {
                            cotrader_core::TradeDirection::Short
                        },
                        entry_price: signal.entry_price,
                        current_price: signal.entry_price,
                        quantity: signal.position_size,
                        stop_loss: signal.stop_loss,
                        take_profit: signal.take_profit,
                        unrealized_pnl: 0.0,
                        unrealized_pnl_pct: 0.0,
                        entry_time: Utc::now(),
                        risk_amount,
                    });

                    portfolio.total_trades_today += 1;
                    portfolio.last_trade_time = Some(Utc::now());
                    portfolio.last_trade_symbol = Some(signal.symbol.clone());
                }

                ExecutionResult {
                    symbol: signal.symbol.clone(),
                    action: decision.action.clone(),
                    executed: true,
                    order_id: Some(order_id),
                    fill_price: Some(signal.entry_price),
                    quantity: signal.position_size,
                    sl: signal.stop_loss,
                    tp: signal.take_profit,
                    error: None,
                }
            }
            Err(e) => {
                println!("[Implementation] LIVE order FAILED: {}", e);
                ExecutionResult {
                    symbol: signal.symbol.clone(),
                    action: decision.action.clone(),
                    executed: false,
                    order_id: None,
                    fill_price: None,
                    quantity: 0.0,
                    sl: 0.0,
                    tp: 0.0,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    /// Monitor open positions for SL/TP hits.
    pub async fn monitor_positions(&self) {
        let mut portfolio = self.state.portfolio_store.portfolio.write().await;
        let mut positions_to_close = Vec::new();

        for (i, pos) in portfolio.open_positions.iter_mut().enumerate() {
            // Get current price (simplified — would fetch from market data)
            let current_price = pos.current_price;

            // Check stop loss
            let hit_sl = match pos.direction {
                cotrader_core::TradeDirection::Long => current_price <= pos.stop_loss,
                cotrader_core::TradeDirection::Short => current_price >= pos.stop_loss,
            };

            // Check take profit
            let hit_tp = match pos.direction {
                cotrader_core::TradeDirection::Long => current_price >= pos.take_profit,
                cotrader_core::TradeDirection::Short => current_price <= pos.take_profit,
            };

            if hit_sl {
                println!(
                    "[Implementation] STOP LOSS HIT: {} @ {:.2} (SL={:.2})",
                    pos.symbol, current_price, pos.stop_loss
                );
                positions_to_close.push((i, "stop_loss".to_string(), current_price));
            } else if hit_tp {
                println!(
                    "[Implementation] TAKE PROFIT HIT: {} @ {:.2} (TP={:.2})",
                    pos.symbol, current_price, pos.take_profit
                );
                positions_to_close.push((i, "take_profit".to_string(), current_price));
            }
        }

        // Close positions (reverse order to maintain indices)
        for (i, reason, exit_price) in positions_to_close.into_iter().rev() {
            let pos = portfolio.open_positions.remove(i);
            let pnl = match pos.direction {
                cotrader_core::TradeDirection::Long => (exit_price - pos.entry_price) * pos.quantity,
                cotrader_core::TradeDirection::Short => (pos.entry_price - exit_price) * pos.quantity,
            };

            let outcome = if pnl > 0.0 { "WIN" } else { "LOSS" };
            println!(
                "[Implementation] CLOSED {} {} — PnL: {:.2} ({})",
                outcome, pos.symbol, pnl, reason
            );

            // Update daily P&L
            portfolio.daily_pnl += pnl;
            if pnl > 0.0 {
                portfolio.winning_trades_today += 1;
            } else {
                portfolio.losing_trades_today += 1;
                portfolio.consecutive_losses += 1;
            }
        }
    }

    /// Produce reasoning chain.
    pub fn reason(&self, result: &ExecutionResult) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Implementation", &result.symbol);

        if result.executed {
            chain.add_step(
                &format!("Executed {} order", result.action),
                &format!("Filled at {:.2}, qty={:.4}", result.fill_price.unwrap_or(0.0), result.quantity),
                vec![
                    format!("order_id={:?}", result.order_id),
                    format!("SL={:.2}", result.sl),
                    format!("TP={:.2}", result.tp),
                ],
                0.9,
            );
            chain.finalize(&format!(
                "Trade executed: {} @ {:.2} (qty={:.4})",
                result.action,
                result.fill_price.unwrap_or(0.0),
                result.quantity
            ));
        } else {
            chain.add_step(
                &format!("Skipped {} — {}", result.action, result.error.as_deref().unwrap_or("blocked")),
                "Decision was HOLD/BLOCK or execution failed",
                vec![],
                0.5,
            );
            chain.finalize("No trade executed");
        }

        chain
    }
}
