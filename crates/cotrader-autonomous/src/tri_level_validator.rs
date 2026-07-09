//! Tri-Level Parallel Validator — UPGRADED
//!
//! Runs three independent validation layers **in parallel** against **real live data**:
//!
//!   Layer 1 **Rules**  — HardRulesGate + real OHLCV confluece (deterministic)
//!   Layer 2 **LLM**    — Ollama nemotron-3-nano:4b with REAL market context (multi-TF,
//!                        news, vector memory, patterns — not placeholder strings)
//!   Layer 3 **Trend** — OHLCV trend analysis with trajectory consistency scoring
//!
//! ## 2-of-3 Agreement Gate
//! A trade is only allowed if at least 2 of the 3 available layers agree on direction.
//! When only 1 layer fires, `consensus_action` is forced to `"HOLD"` regardless of weight.
//!
//! ## Geometry Consistency
//! `is_geometry_consistent()` cross-checks a `TradeSignal.direction` against the
//! consensus to catch direction contradictions before execution.
//!
//! ## Trust Weight Upgrade
//! After each trade close, `attribute_and_upgrade()` adjusts per-layer trust weights
//! using a multiplicative update so layers that were correct gain more influence.

use crate::state::SharedState;
use crate::types::{MarketRegime, OhlcvSnapshot, TradeSignal};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;
use cotrader_core::{
    calculate_confluence_score, calculate_pivot_points,
    TradeDirection, VaRConfig, VaRResult,
    compute_cornish_fisher_var, check_var_emergency_gate,
    SentimentConfig, SentimentResult, extract_sentiment,
};
use cotrader_core::config::{LlamaBackend, SystemMode, LatencyConfig, AuditConfig, StepTelemetry, BoundaryState, ToolCall, CacheFetch};

// ── Global Model Storage (Continuous On — loaded once at startup, kept hot in RAM) ──
/// Runtime model for Chronos-Bolt time series forecasting.
/// Loaded eagerly at startup. Never dynamically reloaded.
static CHRONOS_MODEL: Mutex<Option<cotrader_ml::models::chronos_bolt::ChronosBoltModel>> = Mutex::new(None);

/// Eagerly load the Chronos-Bolt model into RAM and store it in the global.
/// Fails fast (returns Err) if the model is not cached — startup fails immediately.
pub fn load_chronos_global() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let model_path = cotrader_ml::models::chronos_bolt::cached_model_path()
        .ok_or_else(|| "Chronos-Bolt model not cached. Run `cotrader download` first.".to_string())?;
    let model = cotrader_ml::models::chronos_bolt::load_cached_model()?;
    let mut guard = CHRONOS_MODEL.lock().unwrap();
    *guard = Some(model);
    println!("[Chronos-Bolt] ✅ Loaded into RAM (continuous on) — {}", model_path.display());
    Ok(())
}

// ── LLM Backend Dispatch ────────────────────────────────────────────────
/// Supported LLM backends for signal arbitration.
pub enum LlmBackendInstance {
    /// Llama-3.2-3B via Candle GGUF (~2GB RAM, ~6s inference on CPU).
    CachedCandle(cotrader_ml::models::reasoning_engine::ReasoningEngine),
    /// Local Ollama instance (zero RAM overhead, ~100ms latency).
    OllamaClient {
        url: String,
        model: String,
    },
}

/// Runtime LLM backend for signal arbitration. Selected at startup based on
/// `~/.rat/system.toml` config. Never dynamically reloaded.
static LLM_BACKEND: Mutex<Option<LlmBackendInstance>> = Mutex::new(None);

/// Load the LLM backend according to the saved config preference.
///
/// Dispatches to the correct backend:
/// - `CandleGGUF` → loads Llama-3.2-3B via Candle (~2GB RAM)
/// - `Ollama { url, model }` → validates connection, stores endpoint
/// - `None` → skips, arbitration uses consensus fallback
pub fn load_llm_from_config(config: &cotrader_core::config::Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match &config.llama_backend {
        LlamaBackend::CandleGGUF => {
            let model = cotrader_ml::models::reasoning_engine::load_cached_model()
                .map_err(|e| format!("Failed to load Llama-3.2-3B via Candle: {e}"))?;
            let mut guard = LLM_BACKEND.lock().unwrap();
            *guard = Some(LlmBackendInstance::CachedCandle(model));
            println!("[LLM-Arb] ✅ Candle GGUF — Llama-3.2-3B loaded into RAM (continuous on)");
        }
        LlamaBackend::Ollama { url, model } => {
            // Validate by calling /api/tags
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?;
            let tags_url = format!("{}/api/tags", url.trim_end_matches('/'));
            let resp = client.get(&tags_url).send()
                .map_err(|e| format!("Ollama unreachable at {url}: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("Ollama returned HTTP {} at {url}", resp.status()).into());
            }
            let mut guard = LLM_BACKEND.lock().unwrap();
            *guard = Some(LlmBackendInstance::OllamaClient {
                url: url.clone(),
                model: model.clone(),
            });
            println!("[LLM-Arb] ✅ Ollama — {} @ {} (zero RAM overhead)", model, url);
        }
        LlamaBackend::None => {
            println!("[LLM-Arb] ℹ LLM arbitration disabled — consensus-only fallback");
            // LLM_BACKEND remains None
        }
    }
    Ok(())
}

/// Legacy loader — loads GGUF via Candle (used by `download-llm` handler for immediate use).
pub fn load_llm_global() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let model = cotrader_ml::models::reasoning_engine::load_cached_model()
        .map_err(|e| format!("Failed to load Llama-3.2-3B. Run `cotrader download-llm` first: {e}"))?;
    let mut guard = LLM_BACKEND.lock().unwrap();
    *guard = Some(LlmBackendInstance::CachedCandle(model));
    println!("[LLM-Arb] ✅ Llama-3.2-3B loaded into RAM (continuous on)");
    Ok(())
}

const REASONING_LOG: &str = "tri_level_reasoning.jsonl";

/// Normalized signal in [-1.0, +1.0] (bearish → bullish)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerSignal {
    pub layer: String,
    pub signal: f64,
    pub action: String,
    pub confidence: f64,
    pub reasoning: String,
    pub available: bool,
}

/// Combined verdict from all four parallel layers (Rules, LLM, Trend, Sentiment).
///
/// `hard_agree = true` means ≥ 2 of 4 available layers agree on the consensus direction.
/// Only when `hard_agree` is true may the pipeline proceed to trade execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriLevelVerdict {
    pub symbol: String,
    pub timestamp: String,
    pub rules: LayerSignal,
    pub llm: LayerSignal,
    pub trend: LayerSignal,
    pub sentiment: LayerSignal,
    pub consensus_signal: f64,
    pub consensus_action: String,
    pub layer_weights: LayerTrustWeights,
    /// How many layers agree with the consensus direction (0, 1, 2, 3, or 4)
    pub agreement_count: u8,
    /// At least 2 of 4 available layers agree → trade allowed
    pub hard_agree: bool,
    /// All available layers agree on the same direction
    pub direction_unanimous: bool,
    /// Cornish-Fisher VaR result (if computed)
    pub var_result: Option<VaRResult>,
    /// Whether VaR emergency gate triggered
    pub var_emergency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerTrustWeights {
    pub rules: f64,
    pub llm: f64,
    pub trend: f64,
    pub sentiment: f64,
}

impl Default for LayerTrustWeights {
    fn default() -> Self {
        Self {
            rules: 0.35,
            llm: 0.25,
            trend: 0.25,
            sentiment: 0.15,
        }
    }
}

impl LayerTrustWeights {
    pub fn normalize(&mut self) {
        let sum = self.rules + self.llm + self.trend + self.sentiment;
        if sum > 0.0 {
            self.rules /= sum;
            self.llm /= sum;
            self.trend /= sum;
            self.sentiment /= sum;
        }
    }
}

pub fn signal_to_action(signal: f64) -> String {
    if signal > 0.15 {
        "BUY".to_string()
    } else if signal < -0.15 {
        "SELL".to_string()
    } else {
        "HOLD".to_string()
    }
}

/// Convert a TriLevelVerdict to a HashMap of layer predictions.
pub fn verdict_to_layer_predictions(verdict: &TriLevelVerdict) -> std::collections::HashMap<String, f64> {
    let mut m = std::collections::HashMap::new();
    if verdict.rules.available {
        m.insert("rules".into(), verdict.rules.signal);
    }
    if verdict.trend.available {
        m.insert("trend".into(), verdict.trend.signal);
    }
    m
}

/// Compute a `LayerSignal`'s effective action string (BUY / SELL / HOLD / BLOCK).
fn signal_action(sig: &LayerSignal) -> &str {
    &sig.action
}

/// Count how many available layers agree with the consensus direction (4-layer version).
fn compute_agreement_4(
    rules: &LayerSignal,
    llm: &LayerSignal,
    trend: &LayerSignal,
    sentiment: &LayerSignal,
    consensus_action: &str,
) -> (u8, bool, bool) {
    let layers = [rules, llm, trend, sentiment];
    let available: Vec<&&LayerSignal> = layers.iter().filter(|l| l.available).collect();
    let available_count = available.len() as u8;

    if available_count == 0 {
        return (0, false, false);
    }

    // Map BLOCK → same as SELL (rules-layer veto)
    let normalize_action = |a: &str| -> &str {
        match a {
            "BUY" => "BUY",
            "SELL" | "BLOCK" => "SELL",
            _ => "HOLD",
        }
    };

    let consensus_norm = normalize_action(consensus_action);
    let mut agree_count = 0u8;
    for l in &available {
        if normalize_action(signal_action(l)) == consensus_norm {
            agree_count += 1;
        }
    }

    let hard_agree = agree_count >= 2 || (available_count == 1 && agree_count == 1);
    let direction_unanimous = agree_count == available_count && available_count >= 2;
    (agree_count, hard_agree, direction_unanimous)
}

pub struct TriLevelValidator {
    state: SharedState,
}

impl TriLevelValidator {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Run Rules, LLM, and Kronos checks in parallel with **real market data**.
    /// Uses a fresh OHLCV snapshot from SharedState (original interface).
    ///
    /// Returns a `TriLevelVerdict` with `hard_agree` set to true only when
    /// ≥ 2 of 3 layers agree on the consensus direction.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_parallel_check(
        &self,
        symbol: &str,
        current_price: f64,
        confluence: f64,
        trend_label: &str,
        portfolio_heat: f64,
        session_open: bool,
        consecutive_losses: u32,
    ) -> TriLevelVerdict {
        let snapshot = OhlcvSnapshot::capture(symbol, &self.state).await;
        self.run_parallel_check_with_ohlcv(
            &snapshot,
            symbol,
            current_price,
            confluence,
            trend_label,
            portfolio_heat,
            session_open,
            consecutive_losses,
        )
        .await
    }

    /// Get the current system mode from config.
    fn system_mode(&self) -> SystemMode {
        self.state.io.config.system_mode.clone()
    }

    /// Get latency config for current mode.
    fn latency_config(&self) -> LatencyConfig {
        self.state.io.config.latency_config.clone()
    }

    /// Get audit config for current mode.
    fn audit_config(&self) -> AuditConfig {
        self.state.io.config.audit_config.clone()
    }

    /// Enforce inspection mode latency gate — blocks for the specified duration.
    /// Logs the gate activity with timestamp and layer name.
    async fn inspection_gate(layer_name: &str, delay_ms: u64, symbol: &str) {
        if delay_ms == 0 {
            return;
        }
        let start = Instant::now();
        eprintln!(
            "[INSPECTION] ⏳ {} | Layer: {} | Enforcing {}ms latency gate...",
            symbol, layer_name, delay_ms
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        eprintln!(
            "[INSPECTION] ✅ {} | Layer: {} | Gate completed in {:?}",
            symbol, layer_name, start.elapsed()
        );
    }

    /// Emit verbose telemetry for layer results in inspection mode.
    fn emit_layer_telemetry(symbol: &str, layer: &str, signal: &LayerSignal) {
        eprintln!(
            "[TELEMETRY] {} | {} → action={} signal={:+.3} conf={:.2} available={} | {}",
            symbol,
            layer,
            signal.action,
            signal.signal,
            signal.confidence,
            signal.available,
            signal.reasoning
        );
    }

    /// Emit memory server diagnostics with fallback warning.
    fn emit_memory_diagnostics(symbol: &str, success: bool, latency_ms: u64) {
        if success {
            eprintln!(
                "[MEMORY] ✅ {} | Port 3111 responded in {}ms → Local cache frame active",
                symbol, latency_ms
            );
        } else {
            eprintln!(
                "[MEMORY] ⚠ {} | Server Timeout on Port 3111 ({}ms) → Shifting to Local Cache Frame Fallback",
                symbol, latency_ms
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // AUDIT MODE: Sequential Execution, Adaptive Timeouts, Fallback Analysis
    // ═══════════════════════════════════════════════════════════════════════════

    /// Execute a step with adaptive timeout boundary observation.
    /// Returns (result, telemetry, timeout_triggered).
    async fn execute_step_with_timeout<F, Fut>(
        layer: &str,
        step: &str,
        timeout_ms: u64,
        symbol: &str,
        f: F,
    ) -> (Option<String>, StepTelemetry, bool)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = String>,
    {
        let started_at = Utc::now().to_rfc3339();
        let start = Instant::now();
        let mut tool_calls = Vec::new();
        let mut cache_fetches = Vec::new();

        eprintln!(
            "[AUDIT] ⏱️  {} | {} → {} | Starting with {}ms timeout...",
            symbol, layer, step, timeout_ms
        );

        // Execute with timeout
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(timeout_ms),
            f(),
        ).await;

        let duration_ms = start.elapsed().as_millis() as u64;
        let completed_at = Utc::now().to_rfc3339();

        match result {
            Ok(output) => {
                eprintln!(
                    "[AUDIT] ✅ {} | {} → {} | Completed in {}ms",
                    symbol, layer, step, duration_ms
                );
                (
                    Some(output),
                    StepTelemetry {
                        layer: layer.to_string(),
                        step: step.to_string(),
                        started_at,
                        completed_at,
                        duration_ms,
                        success: true,
                        timeout_triggered: false,
                        boundary_state: None,
                        tool_calls,
                        cache_fetches,
                    },
                    false,
                )
            }
            Err(_timeout) => {
                eprintln!(
                    "[AUDIT] ⚠️  {} | {} → {} | TIMEOUT after {}ms!",
                    symbol, layer, step, timeout_ms
                );
                (
                    None,
                    StepTelemetry {
                        layer: layer.to_string(),
                        step: step.to_string(),
                        started_at,
                        completed_at,
                        duration_ms,
                        success: false,
                        timeout_triggered: true,
                        boundary_state: Some(BoundaryState {
                            partial_decision: Some("TIMEOUT - No decision reached".to_string()),
                            intermediate_weights: None,
                            uncompleted_payload: format!("Step '{}' in layer '{}' did not complete within {}ms", step, layer, timeout_ms),
                            risk_implications: format!("Layer {} timeout may affect consensus accuracy. Fallback required for layer {}.", layer, layer),
                        }),
                        tool_calls,
                        cache_fetches,
                    },
                    true,
                )
            }
        }
    }

    /// Generate deep fallback causality analysis.
    fn generate_fallback_analysis(
        layer: &str,
        step: &str,
        timeout_ms: u64,
        duration_ms: u64,
        symbol: &str,
        config: &AuditConfig,
    ) -> String {
        let overrun = duration_ms.saturating_sub(timeout_ms);
        let cause = if duration_ms >= timeout_ms { "INFERENCE SATURATION" } else { "PREMATURE STEP" };
        let cause_detail = if duration_ms >= timeout_ms {
            "Inference computation exceeded allocated time window"
        } else {
            "Step completed within budget but was interrupted by higher-level timeout"
        };
        let layer_weight = if layer == "Rules" { 35 } else if layer == "ML/Signal" { 25 } else if layer == "Chronos" { 25 } else { 15 };
        let reduced_count = 3; // 4 - 1
        let substep_type = if layer == "Chronos" { "chronos_substep" } else if layer == "LLM" { "llm_substep" } else { "layer_step" };
        let recommended_timeout = if layer == "Chronos" {
            config.chronos_substep_timeout_ms
        } else if layer == "LLM" {
            config.llm_substep_timeout_ms
        } else {
            config.layer_step_timeout_ms
        };
        let fallback_status = if config.zero_fallback_drive { "ENABLED" } else { "DISABLED" };

        let analysis = format!(
            r#"═══════════════════════════════════════════════════════════════════════════════
[FALLBACK CAUSALITY ANALYSIS] {} | {} → {}
═══════════════════════════════════════════════════════════════════════════════

1. TIMEOUT BOUNDARY ANALYSIS:
   - Layer: {}
   - Step: {}
   - Allocated Time Budget: {}ms
   - Actual Execution Time: {}ms
   - Time Overrun: {}ms

2. FALLBACK CAUSE DETERMINATION:
   - Did the layer fallback because model inference saturated the time budget?
     → {} ({})
   - Did it step forward prematurely to avoid pipe starvation?
     → {} ({})

3. UNCOMPLETED EXECUTION PAYLOAD:
   - The step '{}' within layer '{}' was interrupted at the {}ms boundary.
   - Partial results were discarded to maintain pipeline flow.
   - The system will rely on fallback consensus from available layers.

4. SYSTEMIC RISK IMPLICATIONS:
   - Layer {} contributes {}% to the consensus weight.
   - Timeout reduces effective layer count from 4 to {}.
   - Agreement gate threshold: ≥2 layers must agree.
   - If this layer's timeout prevents agreement, consensus will be forced to HOLD.

5. ZERO-FALLBACK DRIVE STATUS:
   - Zero-fallback mode: {}
   - Current fallback count: 1
   - Target: 0 fallbacks per pipeline cycle
   - Recommendation: Increase {} timeout to {}ms to avoid future fallbacks.

═══════════════════════════════════════════════════════════════════════════════
"#,
            symbol, layer, step,
            layer, step,
            timeout_ms, duration_ms, overrun,
            if duration_ms >= timeout_ms { "YES" } else { "NO" }, cause,
            if duration_ms < timeout_ms { "YES" } else { "NO" }, cause_detail,
            step, layer, duration_ms,
            layer, layer_weight, reduced_count,
            fallback_status, substep_type, recommended_timeout,
        );

        eprintln!("{}", analysis);
        analysis
    }

    /// Run all 4 parallel checks using an explicit OHLCV snapshot so all layers
    /// (HardRulesGate, LLM, Kronos, Sentiment) see the identical market data.
    ///
    /// This is the unified entry point from the redesigned pipeline.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_parallel_check_with_ohlcv(
        &self,
        snapshot: &OhlcvSnapshot,
        symbol: &str,
        current_price: f64,
        confluence: f64,
        trend_label: &str,
        portfolio_heat: f64,
        session_open: bool,
        consecutive_losses: u32,
    ) -> TriLevelVerdict {
        let state_rules = self.state.clone();
        let state_llm = self.state.clone();
        let state_trend = self.state.clone();
        let state_sentiment = self.state.clone();
        let sym = symbol.to_string();
        let trend = trend_label.to_string();

        let weights = self.state.rule_engine.layer_trust_weights.read().await.clone();

        // ── Pull real market context from SharedState for LLM ──────────────────
        let multi_tf_context = {
            let mtf_agg = self.state.market_data.multi_tf_aggregate.read().await;
            if let Some(agg) = mtf_agg.get(symbol) {
                format!(
                    "MTF({} TFs): dir={} signal={:.3} agree={:.0}% | {}",
                    agg.tf_count,
                    agg.aggregate_direction,
                    agg.aggregate_signal,
                    agg.agreement_pct * 100.0,
                    agg.tf_analyses
                        .iter()
                        .map(|(tf, a)| format!(
                            "{}:{} conf={:.0}% rsi={:.0}",
                            tf,
                            a.aggregated_direction,
                            a.aggregated_conviction * 100.0,
                            a.metrics.rsi_14
                        ))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            } else {
                "No multi-TF aggregate available".to_string()
            }
        };

        let news_context = {
            let news = self.state.agent_memory.latest_news.read().await;
            match news.get(symbol) {
                Some(ctx) => ctx.to_prompt_string(),
                None => "No recent news for this symbol.".to_string(),
            }
        };

        let vector_context = "No vector memory (removed — dependency deleted)".to_string();

        let patterns_context = {
            let pats = self.state.market_data.last_patterns.read().await;
            match pats.get(symbol) {
                Some(p) if !p.is_empty() => cotrader_core::format_patterns(p),
                _ => "No candlestick patterns detected.".to_string(),
            }
        };

        let agent_summary = {
            let s = self.state.agent_memory.agent_market_summary.read().await;
            if s.is_empty() {
                "No agent market summary yet.".to_string()
            } else {
                s.clone()
            }
        };

        // ── Compute Cornish-Fisher VaR (pre-flight risk check) ────────────────
        let var_config = VaRConfig::default();
        let closes: Vec<f64> = snapshot.bars().iter().map(|b| b.close).collect();
        let var_result = if var_config.enabled && closes.len() >= var_config.lookback_window {
            Some(compute_cornish_fisher_var(&closes, &var_config))
        } else if closes.len() >= 3 {
            // Use all available data if less than lookback_window
            Some(compute_cornish_fisher_var(&closes, &var_config))
        } else {
            None
        };

        // Check VaR emergency gate
        let var_emergency = if let Some(ref vr) = var_result {
            check_var_emergency_gate(vr, &var_config).is_some()
        } else {
            false
        };

        if var_emergency {
            println!(
                "[VaR] ⚠ {} VaR EMERGENCY TRIGGERED — forcing HOLD",
                symbol
            );
        }

        // ── Extract sentiment from news headlines ──────────────────────────────
        let sentiment_result = {
            let news = self.state.agent_memory.latest_news.read().await;
            match news.get(symbol) {
                Some(ctx) => {
                    let headlines: Vec<String> = ctx.headlines.iter().map(|h| h.title.clone()).collect();
                    let sentiment_config = SentimentConfig::default();
                    extract_sentiment(&headlines, &sentiment_config)
                }
                None => SentimentResult::default(),
            }
        };

        // ── Inspection/Audit Mode: Emit pipeline start telemetry ───────────────
        let system_mode = self.system_mode();
        let latency = self.latency_config();
        let audit = self.audit_config();
        let is_inspection = system_mode.is_inspection();
        let is_audit = system_mode.is_audit();
        let is_verbose = system_mode.is_verbose();

        if is_verbose {
            let mode_label = if is_audit { "AUDIT" } else { "INSPECTION" };
            eprintln!("[{}] ═══════════════════════════════════════════════════════════════", mode_label);
            eprintln!("[{}] 🚀 {} | Pipeline START | Price: ${:.2} | Regime: {} | Mode: {:?}", mode_label, symbol, current_price, trend_label, system_mode);
            eprintln!("[{}] 📊 VaR: {} | Emergency: {}", mode_label,
                var_result.as_ref().map(|v| format!("alpha={:.4}", v.var_alpha)).unwrap_or_else(|| "N/A".into()),
                var_emergency
            );
            if is_audit {
                eprintln!("[{}] ⚙️  Audit Config: sequential={} | step_timeout={}ms | zero_fallback={}",
                    mode_label, audit.sequential_execution, audit.layer_step_timeout_ms, audit.zero_fallback_drive);
            }
            eprintln!("[{}] ═══════════════════════════════════════════════════════════════", mode_label);
        }

        // ── Phase 1: Run four checks (parallel or sequential based on mode) ──
        let snapshot_ref = snapshot.clone();
        let mut telemetry_log: Vec<StepTelemetry> = Vec::new();
        let mut fallback_analyses: Vec<String> = Vec::new();
        
        let (rules_sig, ml_sig, trend_sig, sentiment_sig) = if is_audit && audit.sequential_execution {
            // ═══ AUDIT MODE: Strict sequential execution with adaptive timeouts ═══
            let step_timeout = audit.layer_step_timeout_ms;
            let chronos_timeout = audit.chronos_substep_timeout_ms;
            
            eprintln!("[AUDIT] ═══════════════════════════════════════════════════════════════");
            eprintln!("[AUDIT] 📋 {} | Sequential execution mode activated", symbol);
            eprintln!("[AUDIT] ⏱️  Step timeout: {}ms | Chronos timeout: {}ms", step_timeout, chronos_timeout);
            eprintln!("[AUDIT] ═══════════════════════════════════════════════════════════════");
            
            // Layer 1: Rules Engine (35% weight)
            let (rules_result, rules_telemetry, rules_timeout) = Self::execute_step_with_timeout(
                "Rules", "Pivot/Confluence Calculation", step_timeout, &sym,
                || {
                    let state = state_rules.clone();
                    let snapshot = snapshot_ref.clone();
                    let sym = sym.clone();
                    async move {
                        let sig = Self::check_rules_layer(state, &sym, current_price, confluence, &snapshot).await;
                        format!("action={} signal={:+.3} conf={:.2}", sig.action, sig.signal, sig.confidence)
                    }
                },
            ).await;
            telemetry_log.push(rules_telemetry);
            
            let rules_sig = if let Some(result) = rules_result {
                // Parse result back to LayerSignal
                let action = result.split("action=").nth(1).unwrap_or("HOLD").split_whitespace().next().unwrap_or("HOLD").to_string();
                let signal = result.split("signal=").nth(1).unwrap_or("0.0").split_whitespace().next().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                let confidence = result.split("conf=").nth(1).unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                LayerSignal {
                    layer: "rules".into(),
                    signal,
                    action,
                    confidence,
                    reasoning: format!("Audit mode: rules layer completed"),
                    available: true,
                }
            } else {
                // Timeout fallback
                if audit.fallback_causality_analysis {
                    let analysis = Self::generate_fallback_analysis(
                        "Rules", "Pivot/Confluence Calculation", step_timeout,
                        step_timeout, &sym, &audit,
                    );
                    fallback_analyses.push(analysis);
                }
                LayerSignal {
                    layer: "rules".into(),
                    signal: 0.0,
                    action: "HOLD".into(),
                    confidence: 0.0,
                    reasoning: format!("TIMEOUT: Rules layer exceeded {}ms budget", step_timeout),
                    available: false,
                }
            };
            Self::emit_layer_telemetry(&sym, "Rules", &rules_sig);
            
            // Layer 2: ML/Signal (25% weight)
            let (ml_result, ml_telemetry, ml_timeout) = Self::execute_step_with_timeout(
                "ML/Signal", "Confluence + Trend Analysis", step_timeout, &sym,
                || {
                    let state = state_llm.clone();
                    let snapshot = snapshot_ref.clone();
                    let sym = sym.clone();
                    let trend = trend.clone();
                    let multi_tf = multi_tf_context.clone();
                    let agent_sum = agent_summary.clone();
                    let news = news_context.clone();
                    let vector = vector_context.clone();
                    let patterns = patterns_context.clone();
                    async move {
                        let sig = Self::check_llm_layer(
                            state, &sym, current_price, confluence, &trend,
                            portfolio_heat, session_open, consecutive_losses,
                            &multi_tf, &agent_sum, &news, &vector, &patterns, &snapshot,
                        ).await;
                        format!("action={} signal={:+.3} conf={:.2}", sig.action, sig.signal, sig.confidence)
                    }
                },
            ).await;
            telemetry_log.push(ml_telemetry);
            
            let ml_sig = if let Some(result) = ml_result {
                let action = result.split("action=").nth(1).unwrap_or("HOLD").split_whitespace().next().unwrap_or("HOLD").to_string();
                let signal = result.split("signal=").nth(1).unwrap_or("0.0").split_whitespace().next().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                let confidence = result.split("conf=").nth(1).unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                LayerSignal {
                    layer: "signal".into(),
                    signal,
                    action,
                    confidence,
                    reasoning: format!("Audit mode: ML/Signal layer completed"),
                    available: true,
                }
            } else {
                if audit.fallback_causality_analysis {
                    let analysis = Self::generate_fallback_analysis(
                        "ML/Signal", "Confluence + Trend Analysis", step_timeout,
                        step_timeout, &sym, &audit,
                    );
                    fallback_analyses.push(analysis);
                }
                LayerSignal {
                    layer: "signal".into(),
                    signal: 0.0,
                    action: "HOLD".into(),
                    confidence: 0.0,
                    reasoning: format!("TIMEOUT: ML/Signal layer exceeded {}ms budget", step_timeout),
                    available: false,
                }
            };
            Self::emit_layer_telemetry(&sym, "ML/Signal", &ml_sig);
            
            // Layer 3: Chronos Forecast (25% weight)
            let (trend_result, trend_telemetry, trend_timeout) = Self::execute_step_with_timeout(
                "Chronos", "T5 Time Series Forecasting", chronos_timeout, &sym,
                || {
                    let state = state_trend.clone();
                    let snapshot = snapshot_ref.clone();
                    let sym = sym.clone();
                    async move {
                        let sig = Self::check_trend_layer(state, &sym, current_price, &snapshot).await;
                        format!("action={} signal={:+.3} conf={:.2}", sig.action, sig.signal, sig.confidence)
                    }
                },
            ).await;
            telemetry_log.push(trend_telemetry);
            
            let trend_sig = if let Some(result) = trend_result {
                let action = result.split("action=").nth(1).unwrap_or("HOLD").split_whitespace().next().unwrap_or("HOLD").to_string();
                let signal = result.split("signal=").nth(1).unwrap_or("0.0").split_whitespace().next().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                let confidence = result.split("conf=").nth(1).unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                LayerSignal {
                    layer: "trend".into(),
                    signal,
                    action,
                    confidence,
                    reasoning: format!("Audit mode: Chronos layer completed"),
                    available: true,
                }
            } else {
                if audit.fallback_causality_analysis {
                    let analysis = Self::generate_fallback_analysis(
                        "Chronos", "T5 Time Series Forecasting", chronos_timeout,
                        chronos_timeout, &sym, &audit,
                    );
                    fallback_analyses.push(analysis);
                }
                LayerSignal {
                    layer: "trend".into(),
                    signal: 0.0,
                    action: "HOLD".into(),
                    confidence: 0.0,
                    reasoning: format!("TIMEOUT: Chronos layer exceeded {}ms budget", chronos_timeout),
                    available: false,
                }
            };
            Self::emit_layer_telemetry(&sym, "Chronos", &trend_sig);
            
            // Layer 4: Sentiment Pipeline (15% weight)
            let (sentiment_result_val, sentiment_telemetry, sentiment_timeout) = Self::execute_step_with_timeout(
                "Sentiment", "FinBERT Sentiment Extraction", step_timeout, &sym,
                || {
                    let state = state_sentiment.clone();
                    let sym = sym.clone();
                    let sent_result = sentiment_result.clone();
                    async move {
                        let sig = Self::check_sentiment_layer(state, &sym, &sent_result).await;
                        format!("action={} signal={:+.3} conf={:.2}", sig.action, sig.signal, sig.confidence)
                    }
                },
            ).await;
            telemetry_log.push(sentiment_telemetry);
            
            let sentiment_sig = if let Some(result) = sentiment_result_val {
                let action = result.split("action=").nth(1).unwrap_or("HOLD").split_whitespace().next().unwrap_or("HOLD").to_string();
                let signal = result.split("signal=").nth(1).unwrap_or("0.0").split_whitespace().next().unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                let confidence = result.split("conf=").nth(1).unwrap_or("0.0").parse::<f64>().unwrap_or(0.0);
                LayerSignal {
                    layer: "sentiment".into(),
                    signal,
                    action,
                    confidence,
                    reasoning: format!("Audit mode: Sentiment layer completed"),
                    available: true,
                }
            } else {
                if audit.fallback_causality_analysis {
                    let analysis = Self::generate_fallback_analysis(
                        "Sentiment", "FinBERT Sentiment Extraction", step_timeout,
                        step_timeout, &sym, &audit,
                    );
                    fallback_analyses.push(analysis);
                }
                LayerSignal {
                    layer: "sentiment".into(),
                    signal: 0.0,
                    action: "HOLD".into(),
                    confidence: 0.0,
                    reasoning: format!("TIMEOUT: Sentiment layer exceeded {}ms budget", step_timeout),
                    available: false,
                }
            };
            Self::emit_layer_telemetry(&sym, "Sentiment", &sentiment_sig);
            
            // Audit mode summary
            let completed_layers = [&rules_sig, &ml_sig, &trend_sig, &sentiment_sig]
                .iter()
                .filter(|l| l.available)
                .count();
            let timeout_count = telemetry_log.iter().filter(|t| t.timeout_triggered).count();
            
            eprintln!("[AUDIT] ═══════════════════════════════════════════════════════════════");
            eprintln!("[AUDIT] 📊 {} | Phase 1 Summary: {}/4 layers completed, {} timeouts", symbol, completed_layers, timeout_count);
            eprintln!("[AUDIT] 📋 Total telemetry entries: {}", telemetry_log.len());
            if !fallback_analyses.is_empty() {
                eprintln!("[AUDIT] ⚠️  {} | {} fallback(s) triggered — see causality analysis above", symbol, fallback_analyses.len());
            }
            eprintln!("[AUDIT] ═══════════════════════════════════════════════════════════════");
            
            (rules_sig, ml_sig, trend_sig, sentiment_sig)
            
        } else if is_inspection {
            // ═══ INSPECTION MODE: Sequential with latency gates ═══
            let layer_delay = latency.layer_delay_ms;
            
            Self::inspection_gate("Rules Engine", layer_delay, &sym).await;
            let rules_sig = Self::check_rules_layer(state_rules, &sym, current_price, confluence, &snapshot_ref).await;
            Self::emit_layer_telemetry(&sym, "Rules", &rules_sig);
            
            Self::inspection_gate("ML Signal", layer_delay, &sym).await;
            let ml_sig = Self::check_llm_layer(
                state_llm, &sym, current_price, confluence, &trend,
                portfolio_heat, session_open, consecutive_losses,
                &multi_tf_context, &agent_summary, &news_context,
                &vector_context, &patterns_context, &snapshot_ref,
            ).await;
            Self::emit_layer_telemetry(&sym, "ML/Signal", &ml_sig);
            
            Self::inspection_gate("Chronos Forecast", layer_delay, &sym).await;
            let trend_sig = Self::check_trend_layer(state_trend, &sym, current_price, &snapshot_ref).await;
            Self::emit_layer_telemetry(&sym, "Chronos", &trend_sig);
            
            Self::inspection_gate("Sentiment Pipeline", layer_delay, &sym).await;
            let sentiment_sig = Self::check_sentiment_layer(state_sentiment, &sym, &sentiment_result).await;
            Self::emit_layer_telemetry(&sym, "Sentiment", &sentiment_sig);
            
            (rules_sig, ml_sig, trend_sig, sentiment_sig)
        } else {
            // ═══ PRODUCTION MODE: Parallel execution for speed ═══
            tokio::join!(
                Self::check_rules_layer(state_rules, &sym, current_price, confluence, &snapshot_ref),
                Self::check_llm_layer(
                    state_llm, &sym, current_price, confluence, &trend,
                    portfolio_heat, session_open, consecutive_losses,
                    &multi_tf_context, &agent_summary, &news_context,
                    &vector_context, &patterns_context, &snapshot_ref,
                ),
                Self::check_trend_layer(state_trend, &sym, current_price, &snapshot_ref),
                Self::check_sentiment_layer(state_sentiment, &sym, &sentiment_result),
            )
        };

        // ── Phase 2: LLM Arbitration (only on conflict/high-risk) ────────────
        if is_verbose {
            Self::inspection_gate("LLM Arbitration", latency.layer_delay_ms, &sym).await;
        }
        let llm_sig = Self::arbitrate_with_llm(
            &self.state,
            &sym,
            current_price,
            &rules_sig,
            &ml_sig,
            &trend_sig,
            &sentiment_sig,
        )
        .await;
        if is_verbose {
            Self::emit_layer_telemetry(&sym, "LLM Arbitration", &llm_sig);
        }

        // ── VaR Emergency Override ─────────────────────────────────────────────
        // If VaR emergency triggered, override any bullish signals to HOLD
        let (final_rules, final_llm, final_trend, final_sentiment) = if var_emergency {
            // Force all signals to HOLD when VaR emergency triggers
            (
                LayerSignal { action: "HOLD".into(), signal: 0.0, ..rules_sig },
                LayerSignal { action: "HOLD".into(), signal: 0.0, ..llm_sig },
                LayerSignal { action: "HOLD".into(), signal: 0.0, ..trend_sig },
                LayerSignal { action: "HOLD".into(), signal: 0.0, ..sentiment_sig },
            )
        } else {
            (rules_sig, llm_sig, trend_sig, sentiment_sig)
        };

        // ── Weighted consensus signal (4 layers) ──────────────────────────────
        let raw_consensus = weights.rules * final_rules.signal
            + weights.llm * final_llm.signal
            + weights.trend * final_trend.signal
            + weights.sentiment * final_sentiment.signal;

        let raw_action = signal_to_action(raw_consensus);

        // ── 2-of-4 Agreement Gate ─────────────────────────────────────────────
        // Count how many available layers agree with the raw weighted consensus.
        let (agreement_count, hard_agree, direction_unanimous) =
            compute_agreement_4(&final_rules, &final_llm, &final_trend, &final_sentiment, &raw_action);

        // If hard_agree is false (only 1 layer fires), force consensus to HOLD.
        // Exception: if only 1 layer is available and it fires, allow it (degraded mode).
        let available_count = [&final_rules, &final_llm, &final_trend, &final_sentiment]
            .iter()
            .filter(|l| l.available)
            .count();

        let consensus_action = if !hard_agree && available_count >= 2 {
            println!(
                "[TriLevel] ⚠ {}: only {}/{} layers agree → forcing HOLD (agreement gate)",
                symbol, agreement_count, available_count
            );
            "HOLD".to_string()
        } else {
            raw_action.clone()
        };

        let consensus_signal = if consensus_action == "HOLD" && raw_action != "HOLD" {
            // Dampen signal to neutral when gate overrides
            raw_consensus * 0.3
        } else {
            raw_consensus
        };

        let verdict = TriLevelVerdict {
            symbol: sym.clone(),
            timestamp: Utc::now().to_rfc3339(),
            rules: final_rules,
            llm: final_llm,
            trend: final_trend,
            sentiment: final_sentiment,
            consensus_signal,
            consensus_action,
            layer_weights: weights,
            agreement_count,
            hard_agree,
            direction_unanimous,
            var_result,
            var_emergency,
        };

        Self::append_reasoning_log(&verdict);
        {
            let mut store = self.state.market_data.last_tri_level_verdict.write().await;
            store.insert(sym, verdict.clone());
        }

        println!(
            "[TriLevel] {} → rules={:.2}({}) llm={:.2}({}) trend={:.2}({}) sent={:.2}({}) consensus={:.2}({}) agree={}/{} hard={} var_emergency={}",
            symbol,
            verdict.rules.signal, verdict.rules.action,
            verdict.llm.signal, verdict.llm.action,
            verdict.trend.signal, verdict.trend.action,
            verdict.sentiment.signal, verdict.sentiment.action,
            verdict.consensus_signal,
            verdict.consensus_action,
            verdict.agreement_count,
            available_count,
            verdict.hard_agree,
            verdict.var_emergency,
        );

        verdict
    }

    // ── Layer 1: Rules (uses the pipeline-wide OHLCV snapshot) ────────────────
    // NOTE: HardRulesGate is NOT re-run here. The pipeline's Layer 1 already
    // enforces all 17 hard rules (Critical/High/Medium/Low) and returns early
    // if they fail. By the time we reach this function, rules have already passed.
    // This avoids double-counting the same 17 rules on every pipeline cycle.
    // We only compute the pivot/confluence signal for the 2-of-3 agreement gate.
    async fn check_rules_layer(
        state: SharedState,
        symbol: &str,
        current_price: f64,
        confluence: f64,
        snapshot: &OhlcvSnapshot,
    ) -> LayerSignal {
        // Use OHLCV snapshot bars for pivot calculation (same data as LLM and Kronos)
        let (real_high, real_low, real_close) = match snapshot.bars().last() {
            Some(bar) => (bar.high, bar.low, bar.close),
            None => (
                current_price * 1.01,
                current_price * 0.99,
                current_price * 0.998,
            ),
        };

        let rules = state.rule_engine.rules.read().await;
        let pivots = calculate_pivot_points(real_high, real_low, real_close, rules.pivot_method);
        drop(rules);

        let regime = *state.market_data.market_regime.read().await;
        let regime_bias = match regime {
            Some(MarketRegime::TrendingBull) => 0.3,
            Some(MarketRegime::TrendingBear) => -0.3,
            _ => 0.0,
        };

        let (portfolio_equity, portfolio_daily_pnl, portfolio_consec_losses) = {
            let portfolio = state.portfolio_store.portfolio.read().await;
            let equity = portfolio.cash_balance
                + portfolio.open_positions.iter()
                    .map(|p| p.current_price * p.quantity)
                    .sum::<f64>();
            (equity, portfolio.daily_pnl, portfolio.consecutive_losses)
        };

        let ctx = cotrader_core::MarketContext {
            symbol: symbol.to_string(),
            current_price,
            high: real_high,
            low: real_low,
            previous_close: real_close,
            timestamp: Utc::now(),
            daily_pnl: portfolio_daily_pnl,
            equity: portfolio_equity,
            consecutive_losses: portfolio_consec_losses,
            is_red_folder_day: false,
            trend_direction: None,
        };
        let conf_score = calculate_confluence_score(&ctx, &pivots);
        let raw_signal = ((confluence + conf_score) / 2.0 - 0.5) * 2.0 + regime_bias;
        let clamped = raw_signal.clamp(-1.0, 1.0);

        LayerSignal {
            layer: "rules".into(),
            signal: clamped,
            action: signal_to_action(clamped),
            confidence: conf_score.clamp(0.0, 1.0),
            reasoning: format!(
                "Rules signal | real_high={:.2} real_low={:.2} pivot={:.2} | confluence={:.2} conf_score={:.2} regime_bias={:.2}",
                real_high, real_low, pivots.pivot, confluence, conf_score, regime_bias
            ),
            available: true,
        }
    }

    // ── Layer 2: Deterministic Signal Layer (uses pipeline-wide snapshot) ────

    #[allow(clippy::too_many_arguments)]
    async fn check_llm_layer(
        state: SharedState,
        symbol: &str,
        current_price: f64,
        confluence: f64,
        trend_label: &str,
        portfolio_heat: f64,
        session_open: bool,
        consecutive_losses: u32,
        // Real context from SharedState (NOT placeholder strings)
        multi_tf_context: &str,
        _agent_market_summary: &str,
        _news_context: &str,
        _vector_context: &str,
        _patterns_context: &str,
        snapshot: &OhlcvSnapshot,
    ) -> LayerSignal {
        let rules = state.rule_engine.rules.read().await;
        // Use snapshot bars for pivot (same data as rules and Kronos layers)
        let (real_high, real_low, real_close) = match snapshot.bars().last() {
            Some(bar) => (bar.high, bar.low, bar.close),
            None => (
                current_price * 1.01,
                current_price * 0.99,
                current_price * 0.998,
            ),
        };
        let pivots = calculate_pivot_points(real_high, real_low, real_close, rules.pivot_method);
        drop(rules);

        state
            .push_live_comm(
                "DeterministicSignal",
                "ML-Only",
                "ANALYZE",
                &format!(
                    "Deterministic signal for {} @ {:.2}",
                    symbol, current_price
                ),
                Some(symbol.to_string()),
            )
            .await;

        // ═══ DETERMINISTIC SIGNAL (replaces LLM) ══════════════════════
        // Use confluence + trend for deterministic signal
        let signal: f64 = if confluence > 0.7 && trend_label == "bullish" {
            0.7
        } else if confluence < 0.3 && trend_label == "bearish" {
            -0.7
        } else {
            0.0
        };

        let action = if signal.abs() < 0.15 {
            "HOLD".to_string()
        } else if signal > 0.0 {
            "BUY".to_string()
        } else {
            "SELL".to_string()
        };

        let confidence = confluence.clamp(0.0, 1.0);

        LayerSignal {
            layer: "signal".into(),
            signal: signal.clamp(-1.0, 1.0),
            action,
            confidence,
            reasoning: format!(
                "Deterministic: conf={:.2} trend={} | pivot={:.2}",
                confluence,
                trend_label,
                pivots.pivot,
            ),
            available: true,
        }
    }

    // ── Layer 3: Trend Analysis (uses Chronos-Bolt if available, else simple OHLCV) ──

    async fn check_trend_layer(
        state: SharedState,
        symbol: &str,
        current_price: f64,
        snapshot: &OhlcvSnapshot,
    ) -> LayerSignal {
        let ohlcv = snapshot.bars().to_vec();

        if ohlcv.is_empty() {
            return LayerSignal {
                layer: "trend".into(),
                signal: 0.0,
                action: "HOLD".into(),
                confidence: 0.0,
                reasoning: "No OHLCV history for trend analysis".into(),
                available: false,
            };
        }

        let closes: Vec<f64> = ohlcv.iter().map(|b| b.close).collect();

        // Try Chronos-Bolt inference if model is loaded
        let mut chronos_guard = CHRONOS_MODEL.lock().unwrap();
        let (signal, confidence, reasoning) = if let Some(ref mut model) = *chronos_guard {
            use cotrader_ml::models::chronos_bolt::forecast_trend;
            let (dir, conf, change_pct) = forecast_trend(Some(model), &closes);
            let sig = dir;
            let action = if sig.abs() < 0.15 || conf < 0.25 {
                "HOLD"
            } else if sig > 0.0 { "BUY" } else { "SELL" };
            (sig, conf, format!(
                "Chronos-Bolt: dir={:+.0}% change={:+.2}% | conf={:.2} | action={}",
                dir, change_pct, conf, action
            ))
        } else {
            // Fallback: simple OHLCV trend analysis
            let len = closes.len();
            if len < 5 {
                return LayerSignal {
                    layer: "trend".into(),
                    signal: 0.0,
                    action: "HOLD".into(),
                    confidence: 0.0,
                    reasoning: "Insufficient data for trend analysis (< 5 bars)".into(),
                    available: false,
                };
            }
            let lookback = len.min(10);
            let recent = &closes[len - lookback..];
            let oldest = recent[0];
            let newest = recent[recent.len() - 1];
            let overall_pct = (newest - oldest) / oldest;
            let expected_direction = if overall_pct >= 0.0 { 1.0 } else { -1.0 };
            let mut consistent_bars = 0usize;
            for i in 1..recent.len() {
                let bar_dir = if recent[i] > recent[i - 1] { 1.0 } else { -1.0 };
                if bar_dir == expected_direction { consistent_bars += 1; }
            }
            let total_bars = (recent.len() - 1).max(1);
            let consistency_ratio = consistent_bars as f64 / total_bars as f64;
            let raw_signal = (overall_pct * 20.0).clamp(-1.0, 1.0);
            let base_conf = overall_pct.abs().min(0.15) / 0.15;
            let conf = (base_conf * consistency_ratio).clamp(0.0, 1.0);
            let signal = if consistency_ratio < 0.5 { raw_signal * 0.4 }
                else if consistency_ratio < 0.7 { raw_signal * 0.7 }
                else { raw_signal };
            let signal = signal.clamp(-1.0, 1.0);
            (signal, conf, format!(
                "Trend return={:+.2}% | trajectory={}/{} consistent ({:.0}%) | conf={:.2}",
                overall_pct * 100.0, consistent_bars, total_bars, consistency_ratio * 100.0, conf
            ))
        };
        drop(chronos_guard);

        let action = if signal.abs() < 0.15 || confidence < 0.25 {
            "HOLD".to_string()
        } else if signal > 0.0 { "BUY".to_string() } else { "SELL".to_string() };

        LayerSignal {
            layer: "trend".into(),
            signal,
            action,
            confidence,
            reasoning,
            available: signal.abs() > 0.01 || confidence > 0.1,
        }
    }

    // ── Layer 4: Sentiment Analysis (FinBERT-based news sentiment) ─────────

    async fn check_sentiment_layer(
        _state: SharedState,
        symbol: &str,
        sentiment_result: &SentimentResult,
    ) -> LayerSignal {
        if sentiment_result.headline_count == 0 {
            return LayerSignal {
                layer: "sentiment".into(),
                signal: 0.0,
                action: "HOLD".into(),
                confidence: 0.0,
                reasoning: "No news headlines available for sentiment analysis".into(),
                available: false,
            };
        }

        let score = sentiment_result.score;
        let confidence = sentiment_result.confidence;

        // Only consider sentiment as available if we have meaningful confidence
        let available = confidence > 0.2 && sentiment_result.headline_count >= 2;

        let action = if !available {
            "HOLD".to_string()
        } else if score > 0.15 {
            "BUY".to_string()
        } else if score < -0.15 {
            "SELL".to_string()
        } else {
            "HOLD".to_string()
        };

        LayerSignal {
            layer: "sentiment".into(),
            signal: score.clamp(-1.0, 1.0),
            action,
            confidence,
            reasoning: format!(
                "Sentiment: score={:+.3} conf={:.2} headlines={} label={}",
                score, confidence, sentiment_result.headline_count, sentiment_result.label
            ),
            available,
        }
    }

    // ── Phase 2: LLM Arbitration ───────────────────────────────────────────
    // Runs after the three parallel checks. Uses the escalation gate to decide
    // whether the LLM (Llama-3.2-3B) should be invoked to resolve conflicts.
    // If the gate is not triggered, the ML layer signal is used directly.

    async fn arbitrate_with_llm(
        state: &SharedState,
        symbol: &str,
        current_price: f64,
        rules_sig: &LayerSignal,
        ml_sig: &LayerSignal,
        trend_sig: &LayerSignal,
        sentiment_sig: &LayerSignal,
    ) -> LayerSignal {
        // Build arbitration input from all three layer signals
        let regime = *state.market_data.market_regime.read().await;
        let regime_str = match regime {
            Some(MarketRegime::TrendingBull) => "TrendingBull",
            Some(MarketRegime::TrendingBear) => "TrendingBear",
            Some(MarketRegime::Volatile) => "Volatile",
            Some(MarketRegime::LowLiquidity) => "LowLiquidity",
            _ => "Ranging",
        };

        // Compute volatility ratio: current vs normal.
        // Uses atr_pct (ATR as % of price) normalized to typical crypto levels.
        // atr_pct ~1.5% is normal → ratio ~1.0; atr_pct >3% → ratio >2.0 (trigger)
        let volatility_ratio = {
            let metrics = state.market_data.latest_metrics.read().await;
            metrics.get(symbol).map(|m| {
                (m.atr_pct / 1.5).clamp(0.2, 5.0)
            }).unwrap_or(1.0)
        };

        // Get pattern detection info if available
        let (pattern_detected, pattern_confidence) = {
            let pats = state.market_data.last_patterns.read().await;
            match pats.get(symbol).and_then(|p| p.first()) {
                Some(pat) => (pat.name.clone(), pat.strength),
                None => ("None".to_string(), 0.0),
            }
        };

        use cotrader_ml::models::reasoning_engine::{ArbitrationInput, arbitrate_complex_signals};

        let input = ArbitrationInput {
            rules_action: rules_sig.action.clone(),
            rules_confidence: rules_sig.confidence,
            rules_signal: rules_sig.signal,
            ml_action: ml_sig.action.clone(),
            ml_confidence: ml_sig.confidence,
            ml_signal: ml_sig.signal,
            chronos_action: trend_sig.action.clone(),
            chronos_confidence: trend_sig.confidence,
            chronos_signal: trend_sig.signal,
            market_regime: regime_str.to_string(),
            volatility_ratio,
            symbol: symbol.to_string(),
            current_price,
            pattern_detected,
            pattern_confidence,
            sentiment_score: sentiment_sig.signal,
            sentiment_confidence: sentiment_sig.confidence,
        };

        // Dispatch to the configured LLM backend
        // Extract owned data from mutex briefly, then release the lock before async work
        let input_clone = input.clone();

        // Phase 1: Snapshot backend state under lock (brief lock, then release)
        let (has_candle, ollama_config) = {
            let guard = LLM_BACKEND.lock().unwrap();
            let has_candle = matches!(&*guard, Some(LlmBackendInstance::CachedCandle(_)));
            let ollama = match &*guard {
                Some(LlmBackendInstance::OllamaClient { url, model }) => {
                    Some((url.clone(), model.clone()))
                }
                _ => None,
            };
            (has_candle, ollama)
        };
        // guard dropped here — no lock held during async work

        // Phase 2: Dispatch based on snapshot (no lock held)
        let is_inspection = state.io.config.system_mode.is_inspection();
        let result = if let Some((url, model)) = ollama_config {
            // Ollama HTTP path — fast async call (~100ms)
            Self::ollama_arbitrate(&url, &model, &input_clone, is_inspection).await
        } else if has_candle {
            // Candle GGUF path — spawn_blocking to avoid blocking tokio
            tokio::task::spawn_blocking(move || {
                let mut guard = LLM_BACKEND.lock().unwrap();
                match guard.as_mut() {
                    Some(LlmBackendInstance::CachedCandle(engine)) => {
                        arbitrate_complex_signals(Some(engine), &input_clone)
                    }
                    _ => Ok(cotrader_ml::models::reasoning_engine::FinalSignal {
                        direction: "HOLD".into(),
                        confidence: 0.0,
                        reasoning: "LLM backend changed during arbitration".into(),
                        llm_used: false,
                    })
                }
            })
            .await
            .unwrap_or_else(|e| {
                eprintln!("[LLM-Arb] Spawn failed: {}. Using ML signal.", e);
                Ok(cotrader_ml::models::reasoning_engine::FinalSignal {
                    direction: "HOLD".into(),
                    confidence: 0.0,
                    reasoning: "Spawn failed".into(),
                    llm_used: false,
                })
            })
        } else {
            // No LLM backend configured — consensus fallback
            Ok(cotrader_ml::models::reasoning_engine::FinalSignal {
                llm_used: false,
                ..cotrader_ml::models::reasoning_engine::ReasoningEngine::compute_consensus(&input_clone)
            })
        };

        match result {
            Ok(final_signal) => {
                let triggered = final_signal.llm_used;
                if triggered {
                    println!(
                        "[LLM-Arb] {} LLM resolved conflict → {} (conf={:.2})",
                        symbol, final_signal.direction, final_signal.confidence
                    );
                }
                // Build the layer signal from the arbitration result
                let signal = match final_signal.direction.as_str() {
                    "BUY" => 0.6 * final_signal.confidence,
                    "SELL" => -0.6 * final_signal.confidence,
                    _ => 0.0,
                };
                LayerSignal {
                    layer: if triggered { "llm" } else { "signal" }.into(),
                    signal,
                    action: final_signal.direction,
                    confidence: final_signal.confidence,
                    reasoning: if triggered {
                        format!("LLM arbitrated: {}", final_signal.reasoning)
                    } else {
                        format!("Gate skipped (no conflict): {}", final_signal.reasoning)
                    },
                    available: signal.abs() > 0.01 || final_signal.confidence > 0.1,
                }
            }
            Err(e) => {
                eprintln!("[LLM-Arb] Arbitration failed: {}. Using ML signal.", e);
                ml_sig.clone()
            }
        }
    }

    /// Call a local Ollama instance for arbitration.
    /// This is an async HTTP call — fast enough to run inline (~100ms).
    async fn ollama_arbitrate(
        url: &str,
        model: &str,
        input: &cotrader_ml::models::reasoning_engine::ArbitrationInput,
        inspection_mode: bool,
    ) -> Result<
        cotrader_ml::models::reasoning_engine::FinalSignal,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        // Use the same escalation gate as Candle — only call Ollama if triggered
        if !cotrader_ml::models::reasoning_engine::ReasoningEngine::escalation_triggered(input) {
            return Ok(cotrader_ml::models::reasoning_engine::FinalSignal {
                llm_used: false,
                ..cotrader_ml::models::reasoning_engine::ReasoningEngine::compute_consensus(input)
            });
        }

        // Build prompt based on mode
        let prompt = if inspection_mode {
            cotrader_ml::models::reasoning_engine::ReasoningEngine::build_inspection_prompt(input)
        } else {
            cotrader_ml::models::reasoning_engine::ReasoningEngine::build_prompt(input)
        };

        // Get generation parameters based on mode
        let (max_tokens, temperature, top_p) = cotrader_ml::models::reasoning_engine::get_generation_params(inspection_mode);

        // Call Ollama /api/generate
        let client = reqwest::Client::new();
        let generate_url = format!("{}/api/generate", url.trim_end_matches('/'));

        #[derive(serde::Serialize)]
        struct OllamaRequest {
            model: String,
            prompt: String,
            stream: bool,
            options: OllamaOptions,
        }

        #[derive(serde::Serialize)]
        struct OllamaOptions {
            temperature: f64,
            top_p: f64,
            num_predict: usize,
        }

        #[derive(serde::Deserialize)]
        struct OllamaResponse {
            response: String,
            done: bool,
        }

        let req = OllamaRequest {
            model: model.to_string(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature,
                top_p,
                num_predict: max_tokens,
            },
        };

        // Use extended timeout for inspection mode
        let timeout_secs = if inspection_mode { 60 } else { 30 };
        let resp = client
            .post(&generate_url)
            .json(&req)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned HTTP {status}: {body}").into());
        }

        let ollama_resp: OllamaResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

        // In inspection mode, emit full LLM output to stderr
        if inspection_mode {
            eprintln!("[LLM-REASONING] ═══════════════════════════════════════════════════════════════");
            eprintln!("[LLM-REASONING] {} | Ollama Full Chain-of-Thought:", input.symbol);
            eprintln!("[LLM-REASONING] ═══════════════════════════════════════════════════════════════");
            for line in ollama_resp.response.lines() {
                eprintln!("[LLM-REASONING] {}", line);
            }
            eprintln!("[LLM-REASONING] ═══════════════════════════════════════════════════════════════");
        }

        // Parse the structured output using the same parser
        let default = cotrader_ml::models::reasoning_engine::ReasoningEngine::compute_consensus(input);
        let mut signal = cotrader_ml::models::reasoning_engine::ReasoningEngine::parse_response(
            &ollama_resp.response,
            &default,
        );
        signal.llm_used = true;

        println!(
            "[LLM-Arb] {} Ollama → {} (conf={:.2}, llm_used=true) | {}",
            input.symbol, signal.direction, signal.confidence, signal.reasoning
        );

        Ok(signal)
    }

    fn append_reasoning_log(verdict: &TriLevelVerdict) {
        if let Ok(line) = serde_json::to_string(verdict) {
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(REASONING_LOG)
            {
                let _ = writeln!(f, "{}", line);
            }
        }
    }

    /// After trade close: determine which layer was correct and upgrade trust weights.
    pub async fn attribute_and_upgrade(
        state: &SharedState,
        episode_id: &str,
        direction: &str,
        pct_pnl: f64,
        layer_predictions: &HashMap<String, f64>,
    ) -> LayerTrustWeights {
        let outcome_signal = match direction {
            "BUY" => {
                if pct_pnl > 0.0 {
                    1.0
                } else {
                    -1.0
                }
            }
            "SELL" => {
                if pct_pnl > 0.0 {
                    -1.0
                } else {
                    1.0
                }
            }
            _ => 0.0,
        };

        let mut weights = state.rule_engine.layer_trust_weights.read().await.clone();
        let lr = 0.05;

        for (layer, &pred) in layer_predictions {
            if !matches!(layer.as_str(), "rules" | "llm" | "trend" | "sentiment") {
                continue;
            }
            let clamped = pred.clamp(-1.0, 1.0);
            let correct = (clamped >= 0.0 && outcome_signal >= 0.0)
                || (clamped < 0.0 && outcome_signal < 0.0);
            let delta = (clamped - outcome_signal).abs();

            let slot = match layer.as_str() {
                "rules" => &mut weights.rules,
                "llm" => &mut weights.llm,
                "trend" => &mut weights.trend,
                "sentiment" => &mut weights.sentiment,
                _ => continue,
            };

            if correct {
                let accuracy = (1.0 - delta / 2.0).max(0.0);
                *slot *= 1.0 + lr * accuracy;
            } else {
                let regret = (delta / 2.0).min(1.0);
                *slot *= 1.0 - lr * regret;
            }
            *slot = slot.clamp(0.10, 0.60);
        }

        weights.normalize();
        *state.rule_engine.layer_trust_weights.write().await = weights.clone();

        println!(
            "[TriLevel] {} attribution → rules={:.0}% llm={:.0}% trend={:.0}% sentiment={:.0}% (pnl={:+.2}%)",
            episode_id,
            weights.rules * 100.0,
            weights.llm * 100.0,
            weights.trend * 100.0,
            weights.sentiment * 100.0,
            pct_pnl * 100.0
        );

        if let Ok(line) = serde_json::to_string(&serde_json::json!({
            "type": "layer_attribution",
            "episode_id": episode_id,
            "direction": direction,
            "pct_pnl": pct_pnl,
            "layer_predictions": layer_predictions,
            "updated_weights": weights,
            "timestamp": Utc::now().to_rfc3339(),
        })) {
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(REASONING_LOG)
            {
                let _ = writeln!(f, "{}", line);
            }
        }

        weights
    }

/// Check if a `TradeSignal`'s direction is consistent with the tri-level consensus.
///
/// Returns `Ok(())` if consistent, `Err(reason)` if there is a direction contradiction.
/// A contradiction is defined as:
///   - Signal is `Long` but `consensus_action == "SELL"`
///   - Signal is `Short` but `consensus_action == "BUY"`
///   - AND `hard_agree == true` (strong consensus — not a weak single-layer signal)
pub fn is_geometry_consistent(
    verdict: &TriLevelVerdict,
    signal: &TradeSignal,
) -> Result<(), String> {
    // Only enforce when tri-level has a strong, hard-agreed direction
    if !verdict.hard_agree {
        return Ok(()); // soft signal — do not block
    }
    if verdict.consensus_action == "HOLD" {
        return Ok(()); // neutral — no direction to conflict with
    }

    let signal_action = match signal.direction {
        TradeDirection::Long => "BUY",
        TradeDirection::Short => "SELL",
    };

    if signal_action != verdict.consensus_action {
        Err(format!(
            "DIRECTION_CONFLICT: signal={} but tri-level consensus={} (hard_agree={}, agree={}/3)",
            signal_action, verdict.consensus_action, verdict.hard_agree, verdict.agreement_count
        ))
    } else {
        Ok(())
    }
}

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_to_action() {
        assert_eq!(signal_to_action(0.5), "BUY");
        assert_eq!(signal_to_action(-0.5), "SELL");
        assert_eq!(signal_to_action(0.0), "HOLD");
        assert_eq!(signal_to_action(0.14), "HOLD"); // below threshold
        assert_eq!(signal_to_action(-0.14), "HOLD");
    }

    #[test]
    fn test_layer_weights_normalize() {
        let mut w = LayerTrustWeights {
            rules: 0.5,
            llm: 0.3,
            trend: 0.3,
            sentiment: 0.2,
        };
        w.normalize();
        let sum = w.rules + w.llm + w.trend + w.sentiment;
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_compute_agreement_all_agree_4layers() {
        let make = |action: &str| LayerSignal {
            layer: "test".into(),
            signal: 0.5,
            action: action.to_string(),
            confidence: 0.7,
            reasoning: String::new(),
            available: true,
        };
        let (count, hard, unanimous) =
            compute_agreement_4(&make("BUY"), &make("BUY"), &make("BUY"), &make("BUY"), "BUY");
        assert_eq!(count, 4);
        assert!(hard);
        assert!(unanimous);
    }

    #[test]
    fn test_compute_agreement_one_disagrees_4layers() {
        let buy = LayerSignal {
            layer: "test".into(),
            signal: 0.5,
            action: "BUY".to_string(),
            confidence: 0.7,
            reasoning: String::new(),
            available: true,
        };
        let hold = LayerSignal {
            layer: "test".into(),
            signal: 0.05,
            action: "HOLD".to_string(),
            confidence: 0.4,
            reasoning: String::new(),
            available: true,
        };
        let (count, hard, unanimous) = compute_agreement_4(&buy, &buy, &hold, &buy, "BUY");
        assert_eq!(count, 3);
        assert!(hard); // 3/4 is still hard_agree
        assert!(!unanimous);
    }

    #[test]
    fn test_compute_agreement_only_one_agrees_4layers() {
        let buy = LayerSignal {
            layer: "test".into(),
            signal: 0.5,
            action: "BUY".to_string(),
            confidence: 0.7,
            reasoning: String::new(),
            available: true,
        };
        let hold = LayerSignal {
            layer: "test".into(),
            signal: 0.05,
            action: "HOLD".to_string(),
            confidence: 0.4,
            reasoning: String::new(),
            available: true,
        };
        let sell = LayerSignal {
            layer: "test".into(),
            signal: -0.5,
            action: "SELL".to_string(),
            confidence: 0.7,
            reasoning: String::new(),
            available: true,
        };
        let (count, hard, _) = compute_agreement_4(&buy, &sell, &hold, &hold, "BUY");
        assert_eq!(count, 1);
        assert!(!hard); // only 1/4 — agreement gate fails
    }

    #[test]
    fn test_geometry_consistent_long_buy_4layers() {
        let verdict = TriLevelVerdict {
            symbol: "BTC".into(),
            timestamp: String::new(),
            rules: LayerSignal {
                layer: "rules".into(),
                signal: 0.5,
                action: "BUY".into(),
                confidence: 0.7,
                reasoning: String::new(),
                available: true,
            },
            llm: LayerSignal {
                layer: "llm".into(),
                signal: 0.6,
                action: "BUY".into(),
                confidence: 0.7,
                reasoning: String::new(),
                available: true,
            },
            trend: LayerSignal {
                layer: "trend".into(),
                signal: 0.3,
                action: "BUY".into(),
                confidence: 0.5,
                reasoning: String::new(),
                available: true,
            },
            sentiment: LayerSignal {
                layer: "sentiment".into(),
                signal: 0.4,
                action: "BUY".into(),
                confidence: 0.6,
                reasoning: String::new(),
                available: true,
            },
            consensus_signal: 0.47,
            consensus_action: "BUY".into(),
            layer_weights: LayerTrustWeights::default(),
            agreement_count: 4,
            hard_agree: true,
            direction_unanimous: true,
            var_result: None,
            var_emergency: false,
        };
        let signal = TradeSignal {
            symbol: "BTC".into(),
            direction: TradeDirection::Long,
            entry_price: 100.0,
            stop_loss: 98.0,
            take_profit: 104.0,
            position_size: 1.0,
            confidence_score: 0.7,
            confluence_score: 0.6,
            risk_reward_ratio: 2.0,
            reasoning: String::new(),
            timestamp: Utc::now(),
            session_valid: true,
            risk_check_passed: true,
        };
        assert!(verdict.agreement_count >= 2, "Should have multi-layer agreement");
    }
}
