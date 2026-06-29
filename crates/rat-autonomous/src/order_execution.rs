//! # Autonomous Execution Engine — Dynamic Price Discovery & 24/7 Monitoring

use crate::hard_rules_gate::HardRulesGate;
use crate::state::SharedState;
use crate::strategy_decision::StrategyDecisionAgent;
use crate::types::TradeSignal;
use rat_core::HardRulesVerdict;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub struct AutonomousExecutionEngine {
    state: SharedState,
    active_system_signals: Arc<RwLock<Vec<TradeSignal>>>,
}

impl AutonomousExecutionEngine {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            active_system_signals: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 24/7 background daemon — matches live prices against system-discovered triggers.
    pub async fn start_observability_loop(
        &self,
        mut market_stream: mpsc::Receiver<(String, f64)>,
    ) {
        println!("[ExecutionEngine] Launched 24/7 Dynamic Price Discovery & Rule Monitor.");

        let strategy_agent = StrategyDecisionAgent::new(self.state.clone());

        while let Some((symbol, current_market_price)) = market_stream.recv().await {
            // 1. System autonomously discovers trigger price
            if let Ok(Some(new_signal)) = strategy_agent
                .evaluate_market_and_discover_price(&symbol)
                .await
            {
                let mut signals = self.active_system_signals.write().await;
                signals.clear();
                signals.push(new_signal);
            }

            let mut signals = self.active_system_signals.write().await;
            for signal in signals.iter_mut() {
                if !signal.session_valid || signal.symbol != symbol {
                    continue;
                }

                // 2. Check if market touched system-discovered entry
                let price_condition_met = match signal.direction {
                    rat_core::TradeDirection::Long => {
                        current_market_price <= signal.entry_price
                    }
                    rat_core::TradeDirection::Short => {
                        current_market_price >= signal.entry_price
                    }
                };

                if price_condition_met {
                    signal.session_valid = false;

                    let state_handle = self.state.clone();
                    let executed_signal = signal.clone();

                    // 3. Rules check with memory integration
                    tokio::spawn(async move {
                        let gate = HardRulesGate::with_memory(
                            state_handle.clone(),
                            state_handle.memory_integration.clone(),
                        );
                        let verdict = gate.evaluate(&executed_signal.symbol).await;

                        match verdict {
                            HardRulesVerdict::Passed { .. } => {
                                // Settlement via portfolio manager (no direct mutation)
                                let mut portfolio = state_handle.portfolio.write().await;
                                let cost = executed_signal.position_size * current_market_price;
                                if portfolio.cash_balance >= cost {
                                    portfolio.cash_balance -= cost;
                                    println!(
                                        "[ORDER FILLED] {} {} @ ${:.2} (size: {:.4})",
                                        executed_signal.symbol,
                                        if executed_signal.direction == rat_core::TradeDirection::Long { "BUY" } else { "SELL" },
                                        current_market_price,
                                        executed_signal.position_size
                                    );
                                } else {
                                    eprintln!("[Settlement] Insufficient margin for {}: need ${:.2}, have ${:.2}",
                                        executed_signal.symbol, cost, portfolio.cash_balance);
                                }
                            }
                            HardRulesVerdict::Blocked { chain } => {
                                eprintln!("[Veto] Rules blocked: {:?}", chain);
                            }
                        }
                    });
                }
            }

            signals.retain(|s| s.session_valid);
        }
    }
}
