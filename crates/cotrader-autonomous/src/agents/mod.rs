//! 8 Core Agents — channel-based concurrent pipeline.
//!
//! Agents communicate via typed `tokio::sync::mpsc` channels instead of
//! sequential `.await` chains. Each agent consumes an immutable `CacheFrame`
//! and emits events through output channels.
//!
//! ```text
//! CacheFrame (broadcast)
//!   ├── Analysis → Plan → Risk + Decision → Implementation
//!   ├── Psychology (parallel)
//!   ├── Evolution (parallel)
//!   └── Observation (parallel)
//! ```
//!
//! Side effects (COT events, signed intents) flow through dedicated channels
//! back to the orchestrator, which applies them to SharedState.

pub mod analysis;
pub mod planning;
pub mod decision;
pub mod implementation;
pub mod observation;
pub mod risk;
pub mod psychology;
pub mod evolution;
pub mod reasoning;
pub mod demo;

#[cfg(test)]
mod integration_test;

pub use analysis::AnalysisAgent;
pub use planning::PlanningAgent;
pub use decision::DecisionAgent;
pub use implementation::ImplementationAgent;
pub use observation::ObservationAgent;
pub use risk::RiskAgent;
pub use psychology::PsychologyAgent;
pub use evolution::EvolutionAgent;
pub use reasoning::{ReasoningChain, ReasoningStep};

use crate::episode_store::EpisodeStore;
use crate::resilience::{
    CircuitBreakerHierarchy,
    acquire_broker_permit, acquire_agent_permit,
};
use crate::types::{
    AgentOutputEvent, CacheFrame, EpochId, SignedTradeIntent,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Global epoch counter for CacheFrame versioning.
static EPOCH_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_epoch() -> EpochId {
    EPOCH_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Load or generate an Ed25519 signing key for an agent.
/// Checks `AGENT_{NAME}_SK` env var first, then generates ephemeral.
fn load_signing_key(agent_name: &str) -> ed25519_dalek::SigningKey {
    let env_var = format!("AGENT_{}_SK", agent_name.to_uppercase());
    let key_hex = std::env::var(&env_var).ok();
    if let Some(hex) = key_hex {
        if hex.len() == 64 {
            if let Ok(bytes) = hex::decode(&hex) {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    return ed25519_dalek::SigningKey::from_bytes(&arr);
                }
            }
        }
    }
    // Generate ephemeral key
    let mut csprng = rand::rngs::OsRng;
    ed25519_dalek::SigningKey::generate(&mut csprng)
}

/// The 8 agents, each with channel-based output.
#[derive(Clone)]
pub struct RatAgents {
    pub analysis: AnalysisAgent,
    pub planning: PlanningAgent,
    pub decision: DecisionAgent,
    pub implementation: ImplementationAgent,
    pub observation: ObservationAgent,
    pub risk: RiskAgent,
    pub psychology: PsychologyAgent,
    pub evolution: EvolutionAgent,
    /// Receiver for all COT events emitted by agents (broadcast channel, behind Mutex for &self access).
    pub cot_rx: Arc<tokio::sync::Mutex<tokio::sync::broadcast::Receiver<AgentOutputEvent>>>,
    /// Receiver for all signed trade intents.
    pub intent_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<SignedTradeIntent>>>,
    /// Circuit breaker hierarchy (3-tier).
    pub circuit_breakers: Arc<CircuitBreakerHierarchy>,
}

impl RatAgents {
    /// Create all 8 agents with shared ML engine and channel infrastructure.
    pub fn new(
        ml_engine: Arc<cotrader_ml::MLEngine>,
        episode_store: Arc<EpisodeStore>,
        circuit_breakers: Arc<CircuitBreakerHierarchy>,
    ) -> Self {
        // COT channel: broadcast channel so all agents can send without locking
        let (cot_tx, cot_rx) = tokio::sync::broadcast::channel::<AgentOutputEvent>(256);

        // Intent channel: signed trade intents go here
        let (intent_tx, intent_rx) = mpsc::channel::<SignedTradeIntent>(64);

        // Build all agents with shared resources
        Self {
            analysis: AnalysisAgent::new(
                ml_engine.clone(),
                cot_tx.clone(),
            ),
            planning: PlanningAgent::new(
                ml_engine.clone(),
                cot_tx.clone(),
            ),
            decision: DecisionAgent::new(
                ml_engine,
                cot_tx.clone(),
            ),
            implementation: ImplementationAgent::new(
                cot_tx.clone(),
                intent_tx,
                load_signing_key("IMPLEMENTATION"),
            ),
            risk: RiskAgent::new(cot_tx.clone()),
            psychology: PsychologyAgent::new(cot_tx.clone()),
            evolution: EvolutionAgent::new(cot_tx.clone(), episode_store),
            observation: ObservationAgent::new(cot_tx.clone()),
            cot_rx: Arc::new(tokio::sync::Mutex::new(cot_rx)),
            intent_rx: Arc::new(tokio::sync::Mutex::new(intent_rx)),
            circuit_breakers,
        }
    }

    /// Run the full 8-agent pipeline concurrently using channels.
    ///
    /// Returns all reasoning chains, the final signed trade intent (if any),
    /// and all COT events emitted during the run.
    pub async fn run_pipeline(
        &self,
        frame: CacheFrame,
    ) -> (
        Vec<ReasoningChain>,
        Option<SignedTradeIntent>,
        Vec<AgentOutputEvent>,
    ) {
        let symbol = frame.symbol.clone();
        let current_price = frame.current_price;

        println!(
            "\n═══ 8-AGENT PIPELINE (CONCURRENT): {} @ {:.2} ═══",
            symbol, current_price
        );

        // ── 3-tier circuit breaker check ──
        // Check if trading is allowed for this symbol before proceeding
        if !self.circuit_breakers.is_trading_allowed(&symbol, "tredo").await {
            println!("  [Pipeline] ⛔ Circuit breaker open for {} — aborting", symbol);
            return (Vec::new(), None, Vec::new());
        }

        // ── Channel setup for agent-to-agent communication ──
        // Channels have capacity 1 since each produces at most one output.
        let (analysis_tx, mut analysis_rx) = mpsc::channel::<analysis::AnalysisResult>(1);
        let (plan_tx, mut plan_rx) = mpsc::channel::<planning::PlanResult>(1);
        let (risk_tx, mut risk_rx) = mpsc::channel::<risk::RiskCheckResult>(1);
        let (decision_tx, mut decision_rx) = mpsc::channel::<decision::DecisionResult>(1);
        let (exec_tx, mut exec_rx) = mpsc::channel::<implementation::ExecutionResult>(1);

        // Collectors for parallel agents
        let (psych_tx, _psych_rx) = mpsc::channel::<psychology::PsychologyState>(1);
        let (evo_tx, _evo_rx) = mpsc::channel::<evolution::EvolutionStatus>(1);

        let frame = Arc::new(frame);
        let chains = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        // ── Spawn Analysis Agent ──
        let analysis_agent = self.analysis.clone();
        let f = frame.clone();
        let chains_clone = chains.clone();
        let analysis_handle = tokio::spawn(async move {
            let result = analysis_agent.analyze(&f).await;
            let chain = analysis_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
            println!(
                "  [Analysis] Regime: {:?}, conf: {:.0}%",
                result.regime,
                result.confidence * 100.0
            );
            let _ = analysis_tx.send(result).await;
        });

        // ── Spawn Psychology Agent (parallel) ──
        let psych_agent = self.psychology.clone();
        let f = frame.clone();
        let chains_clone = chains.clone();
        let psych_handle = tokio::spawn(async move {
            let result = psych_agent.assess(&f).await;
            let chain = psych_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
            println!(
                "  [Psychology] State: {:?}, biases: {}",
                result.emotional_state,
                result.biases_detected.len()
            );
            let _ = psych_tx.send(result).await;
        });

        // ── Spawn Evolution Agent (parallel) ──
        let evo_agent = self.evolution.clone();
        let f = frame.clone();
        let chains_clone = chains.clone();
        let evo_handle = tokio::spawn(async move {
            let result = evo_agent.evolve(&f).await;
            let chain = evo_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
            println!(
                "  [Evolution] Episodes: {}, models: {}",
                result.episodes_collected,
                result.models_deployed.len()
            );
            let _ = evo_tx.send(result).await;
        });

        // Await analysis (needed for planning)
        let _ = analysis_handle.await;

        // Acquire agent evaluation bulkhead permit
        let _agent_permit = acquire_agent_permit().await.ok();

        let analysis_result = analysis_rx
            .recv()
            .await
            .expect("Analysis agent should produce a result");
        // Clone for use in subsequent closures (avoid moved value error)
        let analysis_result_for_decision = analysis_result.clone();

        // ── Spawn Planning Agent ──
        let plan_agent = self.planning.clone();
        let f = frame.clone();
        let chains_clone = chains.clone();
        let plan_handle = tokio::spawn(async move {
            let result = plan_agent.plan(&f, &analysis_result).await;
            let chain = plan_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
            println!(
                "  [Planning] Strategy: {}, signal: {}",
                result.strategy_used,
                if result.signal.is_some() { "YES" } else { "NO" }
            );
            let _ = plan_tx.send(result).await;
        });

        let _ = plan_handle.await;
        let plan_result = plan_rx
            .recv()
            .await
            .expect("Planning agent should produce a result");
        // Extract signal before plan_result is moved into closures
        let plan_signal = plan_result.signal.clone();
        let plan_result_for_decision = plan_result.clone();

        // ── Spawn Risk and Decision concurrently (they're independent) ──
        let risk_agent = self.risk.clone();
        let f = frame.clone();
        let chains_clone = chains.clone();
        let risk_handle = tokio::spawn(async move {
            let mut result = risk::RiskCheckResult {
                passed: true,
                blocking_reason: None,
                warnings: vec![],
                risk_score: 0.0,
                position_size_allowed: 0.0,
                adjustments: risk::RiskAdjustments::default(),
            };

            if let Some(ref signal) = plan_result.signal {
                result = risk_agent.check(&f, signal).await;
            }
            let chain = risk_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
            println!("  [Risk] Passed: {}, warnings: {}", result.passed, result.warnings.len());
            let _ = risk_tx.send(result).await;
        });

        let dec_agent = self.decision.clone();
        let f = frame.clone();
        let chains_clone = chains.clone();
        let decision_handle = tokio::spawn(async move {
            let result = dec_agent.decide(&f, &analysis_result_for_decision, &plan_result_for_decision).await;
            let chain = dec_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
            println!("  [Decision] Action: {}, conf: {:.0}%", result.action, result.confidence * 100.0);
            let _ = decision_tx.send(result).await;
        });

        let _ = risk_handle.await;
        let risk_result = risk_rx.recv().await;
        let _ = decision_handle.await;
        let decision_result = decision_rx
            .recv()
            .await
            .expect("Decision agent should produce a result");

        // ── Spawn Implementation Agent (if risk passed + decision is actionable) ──
        let mut final_intent: Option<SignedTradeIntent> = None;

        let should_execute = risk_result
            .as_ref()
            .map(|r| r.passed)
            .unwrap_or(false)
            && (decision_result.action == "BUY" || decision_result.action == "SELL");

        if should_execute {
            if let Some(signal) = plan_signal.clone() {
                // Acquire broker bulkhead permit before execution
                let broker_permit = acquire_broker_permit().await;
                if broker_permit.is_err() {
                    println!("  [Pipeline] ⛔ Broker bulkhead full — skipping execution");
                } else {
                    let exec_agent = self.implementation.clone();
                    let f = frame.clone();
                    let chains_clone = chains.clone();
                    let exec_handle = tokio::spawn(async move {
                        let result = exec_agent.execute(&f, &signal, &decision_result).await;
                        let chain = exec_agent.reason(&result);
                        {
                            let mut chains = chains_clone.lock().await;
                            chains.push(chain);
                        }
                        println!("  [Implementation] Executed: {}, action: {}", result.executed, result.action);
                        let _ = exec_tx.send(result).await;
                    });

                    let _ = exec_handle.await;
                    if let Some(exec_result) = exec_rx.recv().await {
                        if exec_result.executed {
                            // Collect the signed intent from the intent_rx channel
                            if let Ok(mut rx) = self.intent_rx.try_lock() {
                                while let Ok(intent) = rx.try_recv() {
                                    // Verify the intent signature before accepting
                                    let key_bytes: [u8; 32] = match intent.verifying_key.clone().try_into() {
                                        Ok(b) => b,
                                        Err(_) => continue,
                                    };
                                    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&key_bytes) {
                                        Ok(key) => key,
                                        Err(_) => continue,
                                    };
                                    let message = format!(
                                        "{}|{}|{}|{}|{}|{}|{}|{}",
                                        intent.agent_id,
                                        intent.epoch_id,
                                        intent.rule_version,
                                        intent.signal.symbol,
                                        intent.signal.entry_price,
                                        intent.signal.stop_loss,
                                        intent.signal.take_profit,
                                        intent.created_at.timestamp_micros()
                                    );
                                    let sig_bytes: [u8; 64] = match intent.signature.clone().try_into() {
                                        Ok(b) => b,
                                        Err(_) => continue,
                                    };
                                    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                                    if verifying_key.verify_strict(message.as_bytes(), &signature).is_ok() {
                                        final_intent = Some(intent);
                                    } else {
                                        println!("  [Pipeline] ⛔ Intent signature verification FAILED — rejecting");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Await parallel agents (psychology, evolution)
        let _ = psych_handle.await;
        let _ = evo_handle.await;

        // ── Run Observation (parallel, after everything) ──
        let obs_agent = self.observation.clone();
        let f = frame;
        let chains_clone = chains.clone();
        let obs_handle = tokio::spawn(async move {
            let result = obs_agent.get_summary(&f).await;
            let chain = obs_agent.reason(&result);
            {
                let mut chains = chains_clone.lock().await;
                chains.push(chain);
            }
        });
        let _ = obs_handle.await;

        // ── Collect all COT events ──
        let mut cot_events = Vec::new();
        if let Ok(mut rx) = self.cot_rx.try_lock() {
            loop {
                match rx.try_recv() {
                    Ok(event) => cot_events.push(event),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                }
            }
        }

        let chains = Arc::try_unwrap(chains)
            .unwrap_or_else(|_| tokio::sync::Mutex::new(Vec::new()))
            .into_inner();

        println!("═══ PIPELINE COMPLETE: {} chains, {} COT events ═══\n", chains.len(), cot_events.len());

        (chains, final_intent, cot_events)
    }

    /// Run pipeline with WebSocket broadcast for TUI.
    pub async fn run_pipeline_with_broadcast(
        &self,
        frame: CacheFrame,
        update_tx: &tokio::sync::broadcast::Sender<String>,
    ) -> (
        Vec<ReasoningChain>,
        Option<SignedTradeIntent>,
        Vec<AgentOutputEvent>,
    ) {
        let (chains, intent, events) = self.run_pipeline(frame).await;

        // Broadcast all reasoning chains to TUI
        for chain in &chains {
            let msg = serde_json::json!({
                "type": "agent_reasoning",
                "agent": chain.agent,
                "symbol": chain.symbol,
                "steps": chain.steps.len(),
                "conclusion": chain.conclusion,
                "confidence": chain.confidence,
                "timestamp": chain.timestamp.to_rfc3339(),
            });
            let _ = update_tx.send(msg.to_string());
        }

        (chains, intent, events)
    }

    /// Print the full 8-agent tree.
    pub fn print_tree() {
        println!(
            "\nRAT (8 Core Agents)\n\
             ├── Analysis        — Market data, indicators, patterns, regime\n\
             ├── Planning        — Strategy, signals, trade setup\n\
             ├── Decision        — Cross-validation, conviction, debate\n\
             ├── Implementation  — Order execution, position management\n\
             ├── Observation     — Trade outcomes, performance tracking\n\
             ├── Risk            — Sizing, drawdown, circuit breaker\n\
             ├── Psychology      — Behavioral bias, discipline\n\
             └── Evolution       — Self-improvement, ML training"
        );
    }
}
