use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::time;
use uuid::Uuid;

use crate::orchestra::types::ExecutionDecision;
use crate::rat::types::ConnectionMode;

use super::types::*;

/// Client for communicating with an external Memory Agent service.
/// Auto-detects the active interface (WebSocket or REST) on construction.
#[derive(Clone)]
pub struct MemoryAgentClient {
    /// Base URL of the memory agent (e.g., "http://localhost:9090" or "ws://localhost:9090")
    base_url: String,
    /// Detected mode of communication
    mode: ConnectionMode,
    /// Agent identity for routing
    agent_id: String,
    /// Whether the last health check succeeded (interior mutability via AtomicBool)
    healthy: Arc<AtomicBool>,
    /// HTTP client (shared across requests, reqwest::Client is Clone)
    http_client: reqwest::Client,
}

impl MemoryAgentClient {
    /// Create a new MemoryAgentClient and auto-detect the interface.
    ///
    /// Auto-detection logic:
    /// 1. Try WebSocket connection to `base_url/api/v1/memory/ws`
    /// 2. If WS fails, try REST health check on `base_url/api/v1/memory/health`
    /// 3. If neither works, mark as `Unknown` — orchestra will run without memory
    pub async fn auto_detect(base_url: &str, agent_id: &str) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client");

        let mut mode = ConnectionMode::Unknown;
        let mut is_healthy = false;

        let base_url = base_url.trim_end_matches('/').to_string();

        // Step 1: Try WebSocket (preferred for real-time sync)
        let ws_url = format!("{}/api/v1/memory/ws", base_url);
        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((_ws_stream, _response)) => {
                mode = ConnectionMode::WebSocket;
                is_healthy = true;
                tracing::info!(
                    "[MemoryAgent] Connected via WebSocket: {}",
                    ws_url
                );
                // Note: In production, the WS stream would be stored in an
                // Arc<Mutex<Option<...>>> for persistent bidirectional communication.
            }
            Err(e) => {
                tracing::debug!(
                    "[MemoryAgent] WebSocket connect failed ({}), falling back to REST",
                    e
                );
            }
        }

        // Step 2: Try REST health check
        if !is_healthy {
            let health_url = format!("{}{}", base_url, MEMORY_HEALTH_PATH);
            match http_client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    mode = ConnectionMode::Rest;
                    is_healthy = true;
                    tracing::info!(
                        "[MemoryAgent] Connected via REST: {}",
                        base_url
                    );
                }
                Ok(resp) => {
                    tracing::debug!(
                        "[MemoryAgent] REST health check returned {}",
                        resp.status()
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        "[MemoryAgent] REST health check failed: {}",
                        e
                    );
                }
            }
        }

        if !is_healthy {
            tracing::warn!(
                "[MemoryAgent] Could not connect to {} via WS or REST. Running without memory agent.",
                base_url
            );
        }

        Self {
            base_url,
            mode,
            agent_id: agent_id.to_string(),
            healthy: Arc::new(AtomicBool::new(is_healthy)),
            http_client,
        }
    }

    /// Create a client with an explicit connection mode (skip auto-detect).
    pub fn with_mode(base_url: &str, agent_id: &str, mode: ConnectionMode) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            mode,
            agent_id: agent_id.to_string(),
            healthy: Arc::new(AtomicBool::new(mode != ConnectionMode::Unknown)),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Return the current connection mode.
    pub fn connection_mode(&self) -> ConnectionMode {
        self.mode
    }

    /// Return whether the memory agent is reachable.
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    // ── State Sync ─────────────────────────────────────────

    /// Push the current trading state to the memory agent.
    pub async fn sync_state(
        &self,
        state: &MemoryState,
    ) -> Result<(), String> {
        if self.mode == ConnectionMode::Unknown {
            return Ok(());
        }

        let url = format!("{}{}", self.base_url, MEMORY_SYNC_PATH);
        match self.http_client.post(&url)
            .json(state)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => Err(format!("Memory sync returned {}", resp.status())),
            Err(e) => Err(format!("Memory sync request failed: {}", e)),
        }
    }

    /// Sync a completed trade execution to memory.
    pub async fn sync_trade_execution(
        &self,
        symbol: &str,
        action: &str,
        quantity: f64,
        price: Option<f64>,
        _trade_ids: &[Uuid],
    ) -> Result<(), String> {
        let decision = MemoryDecision {
            decision_id: Uuid::new_v4(),
            action: action.to_string(),
            symbol: symbol.to_string(),
            quantity,
            price,
            outcome: "executed".into(),
            timestamp: Utc::now(),
        };

        self.sync_state(&MemoryState {
            cycle_id: Uuid::new_v4(),
            agent_id: self.agent_id.clone(),
            timestamp: Utc::now(),
            positions: vec![],
            recent_decisions: vec![decision],
            balance_snapshot: vec![],
        })
        .await
    }

    // ── Cross-Reference ────────────────────────────────────

    /// Cross-reference a proposed trade decision against memory.
    /// Returns Ok(true) if the trade should proceed.
    pub async fn cross_reference_trade(
        &self,
        decision: &ExecutionDecision,
    ) -> Result<bool, String> {
        if self.mode == ConnectionMode::Unknown || !self.is_healthy() {
            tracing::debug!(
                "[MemoryAgent] No memory connection — allowing {} {}",
                decision.action,
                decision.symbol
            );
            return Ok(true);
        }

        let query = serde_json::json!({
            "agent_id": self.agent_id,
            "decision": {
                "action": decision.action.to_string(),
                "symbol": decision.symbol,
                "quantity": decision.quantity,
                "confidence": decision.confidence,
            }
        });

        let url = format!("{}{}", self.base_url, MEMORY_CROSS_REF_PATH);        let response = self.http_client.post(&url)
            .json(&query)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<CrossReferenceResult>().await {
                        Ok(result) => {
                            if !result.allowed {
                                tracing::warn!(
                                    "[MemoryAgent] Cross-ref blocked: {}",
                                    result.reason
                                );
                            }
                            return Ok(result.allowed);
                        }
                        Err(_) => {
                            tracing::warn!(
                                "[MemoryAgent] Cross-ref response parse failed, allowing trade"
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "[MemoryAgent] Cross-ref returned {}, allowing trade",
                        status
                    );
                }
                Ok(true)
            }
            Err(e) => {
                tracing::warn!(
                    "[MemoryAgent] Cross-ref request failed: {}. Allowing trade.",
                    e
                );
                Ok(true)
            }
        }
    }

    // ── Health Check ───────────────────────────────────────

    /// Periodically check memory agent health. Run in a background task.
    /// Uses AtomicBool for interior mutability so &self is sufficient.
    pub async fn run_health_checks(self: Arc<Self>) {
        let mut interval = time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if self.mode == ConnectionMode::Unknown {
                continue;
            }
            let url = format!("{}{}", self.base_url, MEMORY_HEALTH_PATH);
            match self.http_client.get(&url).timeout(Duration::from_secs(5)).send().await {
                Ok(resp) => {
                    let was_healthy = self.is_healthy();
                    let is_ok = resp.status().is_success();
                    self.healthy.store(is_ok, Ordering::Relaxed);
                    if is_ok && !was_healthy {
                        tracing::info!("[MemoryAgent] Connection restored");
                    } else if !is_ok && was_healthy {
                        tracing::warn!("[MemoryAgent] Connection lost");
                    }
                }
                Err(_) => {
                    if self.is_healthy() {
                        tracing::warn!("[MemoryAgent] Health check failed — entering degraded mode");
                        self.healthy.store(false, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}
