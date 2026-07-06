//! 8 Core Agents — compressed from 21, each with real reasoning.
//!
//! ```
//! RAT (Orchestrator)
//! ├── Analysis        Market data → indicators → patterns → regime
//! ├── Planning        Strategy selection → signal generation → trade setup
//! ├── Decision        Cross-validation → conviction → debate → verdict
//! ├── Implementation  Order execution → position management
//! ├── Observation     Trade outcomes → performance tracking
//! ├── Risk            Position sizing → drawdown → circuit breaker
//! ├── Psychology      Behavioral bias → emotional state
//! └── Evolution       Self-improvement → weight tuning → ML training
//! ```

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

use crate::state::SharedState;

/// The 8 compressed agents, held together for easy access.
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
}

impl RatAgents {
    pub fn new(state: SharedState) -> Self {
        Self {
            analysis: AnalysisAgent::new(state.clone()),
            planning: PlanningAgent::new(state.clone()),
            decision: DecisionAgent::new(state.clone()),
            implementation: ImplementationAgent::new(state.clone()),
            observation: ObservationAgent::new(state.clone()),
            risk: RiskAgent::new(state.clone()),
            psychology: PsychologyAgent::new(state.clone()),
            evolution: EvolutionAgent::new(state),
        }
    }

    /// Run the full 8-agent pipeline for a symbol.
    /// Returns all reasoning chains for logging/display.
    pub async fn run_pipeline(&self, symbol: &str, current_price: f64) -> Vec<ReasoningChain> {
        let mut chains = Vec::new();

        println!("\n═══ 8-AGENT PIPELINE: {} @ {:.2} ═══", symbol, current_price);

        // 1. Analysis
        let analysis = self.analysis.analyze(symbol, current_price).await;
        let chain = self.analysis.reason(&analysis);
        println!("{}", chain.format_for_log());
        chains.push(chain);

        // 2. Planning
        let plan = self.planning.plan(&analysis, current_price).await;
        let chain = self.planning.reason(&plan);
        println!("{}", chain.format_for_log());
        chains.push(chain);

        // 3. Risk check
        if let Some(ref signal) = plan.signal {
            let risk_result = self.risk.check(signal).await;
            let chain = self.risk.reason(&risk_result);
            println!("{}", chain.format_for_log());
            chains.push(chain);

            if risk_result.passed {
                // 4. Decision (includes neurosymbolic verification)
                let decision = self.decide(&analysis, &plan).await;
                let chain = self.decision.reason(&decision);
                println!("{}", chain.format_for_log());
                chains.push(chain);

                // 5. Implementation (if decision is actionable)
                if decision.action == "BUY" || decision.action == "SELL" {
                    if let Some(ref sig) = plan.signal {
                        let exec_result = self.implementation.execute(sig, &decision).await;
                        let chain = self.implementation.reason(&exec_result);
                        println!("{}", chain.format_for_log());
                        chains.push(chain);
                    }
                }
            }
        }

        // 6. Psychology check
        let psych_state = self.psychology.assess().await;
        let chain = self.psychology.reason(&psych_state);
        println!("{}", chain.format_for_log());
        chains.push(chain);

        // 7. Evolution check
        let evo_status = self.evolution.evolve().await;
        let chain = self.evolution.reason(&evo_status);
        println!("{}", chain.format_for_log());
        chains.push(chain);

        println!("═══ PIPELINE COMPLETE: {} chains ═══\n", chains.len());
        chains
    }

    /// Run pipeline and broadcast reasoning to TUI via WebSocket.
    pub async fn run_pipeline_with_broadcast(
        &self,
        symbol: &str,
        current_price: f64,
        update_tx: &tokio::sync::broadcast::Sender<String>,
    ) -> Vec<ReasoningChain> {
        let chains = self.run_pipeline(symbol, current_price).await;

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

        chains
    }

    /// Convenience: run decision step directly.
    pub async fn decide(&self, analysis: &analysis::AnalysisResult, plan: &planning::PlanResult) -> decision::DecisionResult {
        self.decision.decide(analysis, plan).await
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
