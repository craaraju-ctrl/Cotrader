//! Implementation Agent — Order execution → position management → broker communication.
//!
//! Refactored to accept CacheFrame for read-only inputs and emit side effects
//! through channel senders. The orchestrator applies side effects to SharedState.

use super::decision::DecisionResult;
use super::reasoning::ReasoningChain;
use crate::types::{AgentOutputEvent, CacheFrame, OpenPosition, SignedTradeIntent, TradeSignal};
use chrono::Utc;
use std::sync::Arc;
use ed25519_dalek::Signer;
use uuid::Uuid;

#[derive(Clone)]
pub struct ImplementationAgent {
    pub cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
    pub intent_tx: tokio::sync::mpsc::Sender<SignedTradeIntent>,
    /// Ed25519 signing key for this agent (loaded from env).
    pub signing_key: Arc<ed25519_dalek::SigningKey>,
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
    pub fn new(
        cot_tx: tokio::sync::broadcast::Sender<AgentOutputEvent>,
        intent_tx: tokio::sync::mpsc::Sender<SignedTradeIntent>,
        signing_key: ed25519_dalek::SigningKey,
    ) -> Self {
        Self {
            cot_tx,
            intent_tx,
            signing_key: Arc::new(signing_key),
        }
    }

    /// Execute a trade signal — produces a signed intent sent through the channel.
    pub async fn execute(
        &self,
        frame: &CacheFrame,
        signal: &TradeSignal,
        decision: &DecisionResult,
    ) -> ExecutionResult {
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

        // Build and sign the trade intent
        let fill_price = signal.entry_price;
        let quantity = signal.position_size;

        let intent = SignedTradeIntent {
            intent_id: Uuid::new_v4().to_string(),
            agent_id: "Implementation".to_string(),
            epoch_id: frame.epoch_id,
            rule_version: frame.rule_version,
            signal: signal.clone(),
            created_at: Utc::now(),
            signature: Vec::new(), // filled below
            verifying_key: self.signing_key.verifying_key().to_bytes().to_vec(),
        };

        // Sign the intent
        let message = Self::intent_message(&intent);
        let signature = self.signing_key.sign(message.as_bytes());
        let signed_intent = SignedTradeIntent {
            signature: signature.to_bytes().to_vec(),
            ..intent
        };

        // Send through channel to execution pipeline
        let _ = self.intent_tx.try_send(signed_intent);

        // Emit COT event
        let order_id = format!("intent-{}-{}", signal.symbol, Utc::now().timestamp_millis());
        let _ = self
            .cot_tx
            .send(AgentOutputEvent::Cot {
                agent: "Implementation".to_string(),
                symbol: signal.symbol.clone(),
                action: decision.action.clone(),
                reason: format!(
                    "Signed intent sent: {} {} @ {:.2} qty={:.4} SL={:.2} TP={:.2}",
                    decision.action, signal.symbol, fill_price, quantity, signal.stop_loss, signal.take_profit
                ),
                confidence: decision.confidence,
            });

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

    /// Build the message bytes that are signed for a trade intent.
    fn intent_message(intent: &SignedTradeIntent) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            intent.agent_id,
            intent.epoch_id,
            intent.rule_version,
            intent.signal.symbol,
            intent.signal.entry_price,
            intent.signal.stop_loss,
            intent.signal.take_profit,
            intent.created_at.timestamp_micros()
        )
    }

    /// Produce reasoning chain.
    pub fn reason(&self, result: &ExecutionResult) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Implementation", &result.symbol);

        if result.executed {
            chain.add_step(
                &format!("Executed {} order", result.action),
                &format!(
                    "Filled at {:.2}, qty={:.4}",
                    result.fill_price.unwrap_or(0.0),
                    result.quantity
                ),
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
                &format!(
                    "Skipped {} — {}",
                    result.action,
                    result.error.as_deref().unwrap_or("blocked")
                ),
                "Decision was HOLD/BLOCK or execution failed",
                vec![],
                0.5,
            );
            chain.finalize("No trade executed");
        }

        chain
    }
}
