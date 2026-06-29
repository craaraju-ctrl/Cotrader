use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, RwLock};
use tokio::time;

use crate::engine::ExchangeEngine;
use crate::rat::types::RatEvent;
use crate::memory::MemoryAgentClient;

use super::error::{OrchestraError, OrchestraResult};
use super::pipeline::{run_pipeline, PipelineContext};
use super::types::{AgentConfig, ExecutionDecision, SignalAction};

/// The Multi-Agent Orchestra — a background processing loop that:
/// 1. Subscribes to RatEvent broadcasts from the exchange
/// 2. Runs the data pipeline (ingest → normalize → analyze → decide)
/// 3. Cross-references decisions with the Memory Agent
/// 4. Executes trades on the exchange engine
/// 5. Logs all decisions to the RAT stream
pub struct Orchestra {
    config: AgentConfig,
    ctx: Arc<RwLock<PipelineContext>>,
    memory_client: Option<Arc<MemoryAgentClient>>,
    engine: ExchangeEngine,
    rat_tx: broadcast::Sender<RatEvent>,
}

impl Orchestra {
    pub fn new(
        config: AgentConfig,
        engine: ExchangeEngine,
        rat_tx: broadcast::Sender<RatEvent>,
        memory_client: Option<Arc<MemoryAgentClient>>,
    ) -> Self {
        Self {
            config,
            ctx: Arc::new(RwLock::new(PipelineContext::new())),
            memory_client,
            engine,
            rat_tx,
        }
    }

    /// Start the orchestration loop in a background tokio task.
    /// Returns a JoinHandle that can be awaited or aborted.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!(
                "[Orchestra] Agent '{}' started for symbols: {:?}",
                self.config.agent_id,
                self.config.symbols
            );

            if let Some(ref mc) = self.memory_client {
                tracing::info!(
                    "[Orchestra] Memory agent connected via {:?}",
                    mc.connection_mode()
                );
            } else {
                tracing::warn!("[Orchestra] No memory agent connected — running without cross-reference");
            }

            // Subscribe to the RAT broadcast channel
            let mut rx = self.rat_tx.subscribe();

            // Main event loop
            loop {
                tokio::select! {
                    Ok(event) = rx.recv() => {
                        if !self.config.enabled {
                            continue;
                        }

                        // Filter: only process events for configured symbols
                        let symbol = match &event {
                            RatEvent::OrderbookSnapshot(s) => Some(&s.symbol),
                            RatEvent::FundingTick(f) => Some(&f.symbol),
                            RatEvent::TradeExecution(t) => Some(&t.symbol),
                            _ => None,
                        };

                        if let Some(sym) = symbol {
                            if !self.config.symbols.contains(sym) {
                                continue;
                            }
                        }

                        // Run the pipeline
                        let decisions = run_pipeline(
                            &event,
                            &self.ctx,
                            &self.config,
                            &self.engine,
                            Some(&self.rat_tx),
                        ).await;

                        // Process each decision
                        for decision in decisions {
                            self.handle_decision(decision).await;
                        }
                    }
                    _ = time::sleep(Duration::from_secs(60)) => {
                        // Periodic diagnostics every 60s even without events
                        let ctx = self.ctx.read().await;
                        tracing::debug!(
                            "[Orchestra] Pipeline stats: {} data points, {} signals, {} decisions, {} errors",
                            ctx.metrics.total_data_points,
                            ctx.metrics.signals_generated,
                            ctx.metrics.decisions_executed,
                            ctx.metrics.errors,
                        );
                    }
                }
            }
        })
    }

    /// Handle a single execution decision:
    /// 1. Cross-reference with Memory Agent
    /// 2. Execute via ExchangeEngine
    /// 3. Log to RAT stream
    async fn handle_decision(&self, decision: ExecutionDecision) {
        // ── Step 1: Cross-reference with memory (if available) ──
        let should_execute = if let Some(ref memory) = self.memory_client {
            if self.config.use_memory_cross_reference {
                match memory.cross_reference_trade(&decision).await {
                    Ok(allowed) => allowed,
                    Err(e) => {
                        tracing::warn!(
                            "[Orchestra] Memory cross-ref failed for {}: {}. Proceeding anyway.",
                            decision.symbol, e
                        );
                        true
                    }
                }
            } else {
                true
            }
        } else {
            true
        };

        if !should_execute {
            tracing::info!(
                "[Orchestra] Memory agent blocked decision: {:?} {} (reason: historical conflict)",
                decision.action,
                decision.symbol
            );
            crate::rat::stream::broadcast_rat_diagnostic(
                &self.rat_tx,
                "warn",
                &format!(
                    "Memory blocked: {:?} {} — historical context conflict",
                    decision.action, decision.symbol
                ),
                "orchestra::engine",
            );
            return;
        }

        // ── Step 2: Execute the trade ──
        let result = self.execute_decision(&decision).await;

        // ── Step 3: Log to RAT stream —-
        match result {
            Ok(trade_ids) => {
                tracing::info!(
                    "[Orchestra] Executed: {:?} {} qty={:.4} — trades: {:?}",
                    decision.action,
                    decision.symbol,
                    decision.quantity,
                    trade_ids
                );

                // Log agent decision to RAT stream
                let decision_event = crate::rat::types::RatAgentDecision {
                    decision_id: decision.signal_id,
                    agent_id: self.config.agent_id.clone(),
                    symbol: decision.symbol.clone(),
                    action: decision.action.to_string(),
                    reason: decision.reason.clone(),
                    confidence: decision.confidence,
                    market_snapshot: None,
                    timestamp: chrono::Utc::now(),
                };
                crate::rat::stream::broadcast_rat_decision(&self.rat_tx, decision_event);

                // Sync state to memory agent
                if let Some(ref memory) = self.memory_client {
                    let _ = memory.sync_trade_execution(
                        &decision.symbol,
                        &decision.action.to_string(),
                        decision.quantity,
                        decision.price,
                        &trade_ids,
                    ).await;
                }
            }
            Err(e) => {
                tracing::error!(
                    "[Orchestra] Execution failed for {:?} {}: {}",
                    decision.action,
                    decision.symbol,
                    e
                );
                crate::rat::stream::broadcast_rat_diagnostic(
                    &self.rat_tx,
                    "error",
                    &format!("Execution failed: {} — {}", decision.symbol, e),
                    "orchestra::engine",
                );
            }
        }
    }

    /// Execute a decision on the exchange engine.
    async fn execute_decision(&self, decision: &ExecutionDecision) -> OrchestraResult<Vec<uuid::Uuid>> {
        let side = match decision.action {
            SignalAction::EnterLong | SignalAction::ExitShort => crate::types::Side::Buy,
            SignalAction::EnterShort | SignalAction::ExitLong => crate::types::Side::Sell,
            SignalAction::Hold => {
                return Err(OrchestraError::ExecutionBlocked {
                    symbol: decision.symbol.clone(),
                    detail: "Hold action has no execution".into(),
                });
            }
        };

        let order = crate::types::Order::new_market(
            "orchestra".into(),
            decision.symbol.clone(),
            side,
            decision.quantity,
        );

        match self.engine.place_order(order).await {
            Ok(resp) => {
                let ids: Vec<uuid::Uuid> = resp.trades.iter().map(|t| t.id).collect();
                Ok(ids)
            }
            Err(e) => Err(OrchestraError::ExecutionBlocked {
                symbol: decision.symbol.clone(),
                detail: e.to_string(),
            }),
        }
    }

    // ── Accessors ──

    /// Get a snapshot of current pipeline metrics.
    pub async fn metrics(&self) -> super::types::PipelineMetrics {
        self.ctx.read().await.metrics.clone()
    }

    /// Check whether the orchestra is in safe-hold mode.
    pub async fn is_safe_hold(&self) -> bool {
        self.ctx.read().await.safe_hold
    }

    /// Manually reset the pipeline context (e.g., after a config change).
    pub async fn reset(&self) {
        let mut ctx = self.ctx.write().await;
        *ctx = PipelineContext::new();
        tracing::info!("[Orchestra] Pipeline context reset");
    }
}
