//! # Autonomous Execution Engine — Dynamic Price Discovery & 24/7 Monitoring
//!
//! The system autonomously discovers trigger prices based on market structure
//! and volatility. No hardcoded prices — all entry points are computed by the
//! system's code knowledge from historical data and live market conditions.

use crate::hard_rules_gate::HardRulesGate;
use crate::state::SharedState;
use crate::strategy_decision::StrategyDecisionAgent;
use crate::types::TradeSignal;
use rat_core::HardRulesVerdict;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub struct AutonomousExecutionEngine {
    state: SharedState,
    /// System-discovered signals with computed entry prices
    active_system_signals: Arc<RwLock<Vec<TradeSignal>>>,
}

impl AutonomousExecutionEngine {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            active_system_signals: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 24/7 background daemon — continuously matches live market prices
    /// against system-discovered trigger points and executes on rule approval.
    pub async fn start_observability_loop(
        &self,
        mut market_stream: mpsc::Receiver<(String, f64)>,
    ) {
        println!("[ExecutionEngine] Launched 24/7 Dynamic Price Discovery & Rule Monitor.");

        let strategy_agent = StrategyDecisionAgent::new(self.state.clone());

        while let Some((symbol, current_market_price)) = market_stream.recv().await {
            // 1. System autonomously updates trigger price based on code knowledge
            if let Ok(Some(new_signal)) = strategy_agent
                .evaluate_market_and_discover_price(&symbol)
                .await
            {
                let mut signals = self.active_system_signals.write().await;
                // Replace old price locations with fresh structured signal
                signals.clear();
                signals.push(new_signal);
            }

            let mut signals = self.active_system_signals.write().await;
            for signal in signals.iter_mut() {
                if !signal.session_valid || signal.symbol != symbol {
                    continue;
                }

                // 2. Price check: has market touched the system-discovered entry?
                let price_condition_met = match signal.direction {
                    rat_core::TradeDirection::Long => {
                        current_market_price <= signal.entry_price
                    }
                    rat_core::TradeDirection::Short => {
                        current_market_price >= signal.entry_price
                    }
                };

                if price_condition_met {
                    signal.session_valid = false; // Prevent double-trigger

                    let state_handle = self.state.clone();
                    let executed_signal = signal.clone();

                    // 3. Rules check: verify HardRulesGate permission on price hit
                    tokio::spawn(async move {
                        let gate = HardRulesGate::new(state_handle.clone());
                        let verdict = gate.evaluate(&executed_signal.symbol).await;

                        match verdict {
                            HardRulesVerdict::Passed { .. } => {
                                let mut portfolio = state_handle.portfolio.write().await;
                                portfolio.cash_balance -=
                                    executed_signal.position_size * current_market_price;

                                println!(
                                    "[ORDER FILLED] Autonomous trade executed at discovered price: ${:.2}. Reason: {}",
                                    current_market_price, executed_signal.reasoning
                                );
                            }
                            HardRulesVerdict::Blocked { chain } => {
                                eprintln!(
                                    "[Veto] Price hit but rules blocked trade: {:?}",
                                    chain
                                );
                            }
                        }
                    });
                }
            }

            // Clean up triggered signals
            signals.retain(|s| s.session_valid);
        }
    }
}
