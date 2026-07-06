use crate::types::{AgentDecision, DecisionVerdict, OhlcvSnapshot, PipelineSummary, TradeSignal};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use cotrader_core::{Agent, TradeDirection};

/// Minimum deliberation time per pipeline layer (ms).
/// Each agent MUST take at least this long — no rushing decisions.
fn min_deliberation_ms() -> u64 {
    std::env::var("RAT_MIN_DELIBERATION_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15_000) // 15 seconds minimum per layer
}

/// Enforce minimum deliberation: agent must explain reasoning for at least this long.
async fn enforce_min_time(start: std::time::Instant, layer: &str) {
    let min_ms = min_deliberation_ms();
    let elapsed = start.elapsed().as_millis() as u64;
    if elapsed < min_ms {
        let remaining = min_ms - elapsed;
        println!("  [{}] deliberating for {}ms more (min {}ms total)", layer, remaining, min_ms);
        tokio::time::sleep(std::time::Duration::from_millis(remaining)).await;
    }
}

/// Track whether the metrics service has been confirmed reachable.
/// Once set to true, errors are silently suppressed. If never reachable,
/// only the first error per sender function is printed.
static METRICS_HEALTHY: AtomicBool = AtomicBool::new(false);

/// Suppress repeated "connection refused" noise by only logging the first
/// error from each sender function. Returns true if this is the first failure.
fn report_metrics_error_once() -> bool {
    !METRICS_HEALTHY.swap(true, Ordering::Relaxed)
}

/// Probe the metrics endpoint. If it responds, reset the healthy flag so
/// subsequent errors are reported again (one-time re-notification).
async fn check_metrics_health(endpoint: &str) {
    let url = format!("{}/health", endpoint.trim_end_matches('/'));
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let was_silenced = METRICS_HEALTHY.swap(false, Ordering::Relaxed);
            if was_silenced {
                eprintln!("[Metrics] Health check OK on {} — re-enabling error reporting", url);
            }
        }
        _ => {} // still down, keep flag as-is
    }
}

/// Run periodic metrics health checks in the background (every 60s).
pub fn start_metrics_health_check_loop() {
    let endpoint = std::env::var("METRICS_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9730".to_string());
    tokio::spawn(async move {
        // Initial delay: let the system settle before first probe
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            check_metrics_health(&endpoint).await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// Send a pipeline run event to the rat-metrics service.
/// Non-blocking: sends via HTTP POST in a background tokio task.
/// Failures are logged but do not affect pipeline execution.
pub async fn send_pipeline_event_to_metrics(
    symbol: &str,
    action: &str,
    total_duration_ms: f64,
    layers: Vec<(&str, f64, &str)>,
) {
    let metrics_url =
        std::env::var("METRICS_URL").unwrap_or_else(|_| "http://127.0.0.1:9730".to_string());
    let event = serde_json::json!({
        "event_type": "pipeline_run",
        "symbol": symbol,
        "action": action,
        "total_duration_ms": total_duration_ms,
        "layers": layers.into_iter().map(|(name, dur, result)| serde_json::json!({
            "name": name,
            "duration_ms": dur,
            "result": result,
        })).collect::<Vec<_>>(),
        "timestamp_micros": chrono::Utc::now().timestamp_micros(),
    });

    tokio::spawn(async move {
        let url = format!("{}/event", metrics_url.trim_end_matches('/'));
        match reqwest::Client::new()
            .post(&url)
            .json(&event)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(_) => {}
            Err(_) if report_metrics_error_once() => {
                eprintln!(
                    "[Metrics] Pipeline event send failed (metrics may not be running on localhost:9730) — suppressing further errors"
                );
            }
            Err(_) => {} // silenced
        }
    });
}

/// Send a trade outcome event to rat-metrics.
/// NOTE: This is wired into the OutcomeProcessor at trade CLOSE time.
/// Currently not called from the pipeline (trade outcomes sent at close, not open).
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub async fn send_trade_outcome_to_metrics(
    symbol: &str,
    direction: &str,
    entry_price: f64,
    exit_price: f64,
    pnl: f64,
    pnl_pct: f64,
    confidence: f64,
    win: bool,
    holding_time_secs: u64,
) {
    let metrics_url =
        std::env::var("METRICS_URL").unwrap_or_else(|_| "http://127.0.0.1:9730".to_string());
    let event = serde_json::json!({
        "event_type": "trade_outcome",
        "symbol": symbol,
        "direction": direction,
        "entry_price": entry_price,
        "exit_price": exit_price,
        "pnl": pnl,
        "pnl_pct": pnl_pct,
        "confidence": confidence,
        "win": win,
        "holding_time_secs": holding_time_secs,
        "timestamp_micros": chrono::Utc::now().timestamp_micros(),
    });

    tokio::spawn(async move {
        let url = format!("{}/event", metrics_url.trim_end_matches('/'));
        match reqwest::Client::new()
            .post(&url)
            .json(&event)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(_) => {}
            Err(_) if report_metrics_error_once() => {
                eprintln!("[Metrics] Trade outcome send failed — metrics unavailable on localhost:9730 (suppressing further errors)");
            }
            Err(_) => {} // silenced
        }
    });
}

/// Send a latency sample event to rat-metrics.
pub async fn send_latency_to_metrics(component: &str, duration_ms: f64, symbol: Option<&str>) {
    let metrics_url =
        std::env::var("METRICS_URL").unwrap_or_else(|_| "http://127.0.0.1:9730".to_string());
    let event = serde_json::json!({
        "event_type": "latency_sample",
        "component": component,
        "duration_ms": duration_ms,
        "symbol": symbol,
        "timestamp_micros": chrono::Utc::now().timestamp_micros(),
    });

    tokio::spawn(async move {
        let url = format!("{}/event", metrics_url.trim_end_matches('/'));
        match reqwest::Client::new()
            .post(&url)
            .json(&event)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(_) => {}
            Err(_) if report_metrics_error_once() => {
                eprintln!("[Metrics] Latency sample send failed — metrics unavailable on localhost:9730 (suppressing further errors)");
            }
            Err(_) => {} // silenced
        }
    });
}

// NOTE: These phase methods are preserved for backward API compatibility.
// The pipeline now routes through Rat groups (see rat.rs) instead.
#[allow(dead_code)]
impl crate::orchestrator_struct::AutonomousOrchestrator {
    /// Phase 5: **Agentic** strategy decision (no pre-supplied price points).
    /// The agent autonomously identifies entry, SL, TP, direction using its full stack (indicators, debate, memory, rules).
    /// This is what makes it agentic AI rather than a scripted bot.
    /// Phase 6: Execute the paper trade and update the portfolio.
    pub async fn phase6_portfolio_and_execution(
        &self,
        signal: &TradeSignal,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        println!("\n[PHASE 6] Portfolio & Execution");
        let result = self.execution.execute_paper_trade(signal).await?;
        println!("[PHASE 6] {}", result);
        Ok(true)
    }

    /// Full autonomous pipeline with proper 5-layer hierarchy:
    ///
    /// ```text
    /// Phase 0: Position check
    /// Layer 1: HardRulesGate (ALL hard rules — NEVER overridden)
    /// Layer 2: Identifier (data gathering, COT entries, confluence/pivots)
    ///         Verifier (risk analysis, position sizing — advisory)
    /// Layer 3: DebateLayer (advisory — 6 agents, no veto power)
    /// Layer 4: Judge (final adjudication — only evaluates debate quality)
    /// Layer 5: Execute
    /// ```
    ///
    /// Key principle: HardRulesGate runs FIRST. If it blocks, no agents run.
    /// Identifier/Verifier gather data but never block — the gate already handled hard rules.
    /// Debate agents are ADVISORY only — only the Judge has decision-making power.
    ///
    /// Pipeline timing: MIN 5 min, MAX 10 min per symbol.
    /// Every agent MUST produce reasoning. No rushed decisions.
    /// No forced micro-management — agents use their tools at their own pace.
    pub async fn run_full_pipeline_quiet(
        &self,
        symbol: &str,
        quiet: bool,
    ) -> Result<PipelineSummary, Box<dyn Error + Send + Sync>> {
        let start = std::time::Instant::now();

        // ═══ 10-MINUTE MAX ══════════════════════════════════════════════
        // Trading decisions take time. LLM calls, memory queries, tool
        // usage — none of this should be rushed. 10 min is generous.
        let max_secs: u64 = std::env::var("RAT_PIPELINE_MAX_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(max_secs),
            self.run_full_pipeline_inner_quiet(symbol, quiet),
        )
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                println!("⏱ Pipeline for {} exceeded {}s — completing with available reasoning", symbol, max_secs);
                Ok(PipelineSummary {
                    executed: false,
                    phase_results: vec![],
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    final_signal: None,
                    reason: format!("Pipeline exceeded {}s — agents were deliberating", max_secs),
                })
            }
        }
    }

    /// Legacy wrapper — calls run_full_pipeline_quiet with quiet=false.
    pub async fn run_full_pipeline(
        &self,
        symbol: &str,
    ) -> Result<PipelineSummary, Box<dyn Error + Send + Sync>> {
        self.run_full_pipeline_quiet(symbol, false).await
    }

    /// Inner body of `run_full_pipeline_quiet` — extracted so the 60s timeout
    /// wrapper above can wrap the entire pipeline in one shot.
    /// When `quiet=true`, per-agent COT steps are skipped (only the summary
    /// COT at the end is emitted). This eliminates ~17 write-lock acquisitions
    /// on `cot_store` per pipeline run, significantly reducing contention.
    async fn run_full_pipeline_inner_quiet(
        &self,
        symbol: &str,
        quiet: bool,
    ) -> Result<PipelineSummary, Box<dyn Error + Send + Sync>> {
        let start = std::time::Instant::now();
        println!(
            "\n=== rat AUTONOMOUS (AGENTIC) PIPELINE for {} ===",
            symbol
        );

        // Preflight: ensure live price + OHLCV bars exist (fetch if missing)
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let observed_price =
            match crate::pipeline_runner::ensure_market_data(symbol, &client, &self.state).await {
                Ok(p) => p,
                Err(e) => {
                    self.state
                        .finalize_pipeline_run(
                            "NO_DATA",
                            &format!("No market data for {}: {}", symbol, e),
                        )
                        .await;
                    return Ok(PipelineSummary {
                        executed: false,
                        phase_results: vec![],
                        total_duration_ms: start.elapsed().as_millis() as u64,
                        final_signal: None,
                        reason: format!("No observable market data for {symbol}: {e}"),
                    });
                }
            };

        // ═══ CAPTURE UNIFIED OHLCV SNAPSHOT ═══════════════════════════════
        // Takes a single snapshot at pipeline start so all 3 verification layers
        // (HardRulesGate, LLM, Kronos) see the exact same market data.
        // No layer can see stale or differently-timed data.
        let ohlcv_snapshot = OhlcvSnapshot::capture(symbol, &self.state).await;
        println!(
            "[Pipeline] 📊 Captured OHLCV snapshot for {} — {} bars at {}",
            symbol,
            ohlcv_snapshot.len(),
            ohlcv_snapshot.capture_time
        );

        // ═══ START COMMUNICATION LOG ═════════════════════════════════════════
        // Begin recording all agent decisions for transparent virtual communication.
        self.state.start_pipeline_run(symbol).await;

        // Start a COT chain for this pipeline run
        let chain_id = self
            .state
            .start_cot_chain(
                "Orchestrator",
                &format!(
                    "Running full agentic pipeline for {} (observed market price {:.2})",
                    symbol, observed_price
                ),
                "PIPELINE_START",
                &format!("Starting fully autonomous agentic pipeline for {}", symbol),
                1.0,
            )
            .await;

        // ── Phase 0: Check if there is already an open position on this symbol ──
        {
            let portfolio = self.state.portfolio_store.portfolio.read().await;
            if portfolio
                .open_positions
                .iter()
                .any(|pos| pos.symbol == symbol)
            {
                self.state
                    .add_cot_step_quiet(
                        chain_id,
                        "Phase0",
                        "Checking existing positions",
                        "SKIP",
                        &format!("Already have an open position for {}", symbol),
                        1.0,
                        Some(symbol.to_string()),
                        quiet,
                    )
                    .await;
                self.state
                    .finalize_pipeline_run(
                        "SKIP",
                        &format!("Already have an open position for {}", symbol),
                    )
                    .await;
                return Ok(PipelineSummary {
                    executed: false,
                    phase_results: vec![],
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    final_signal: None,
                    reason: format!("Already have an open position for {}", symbol),
                });
            }
        }
        self.state
            .add_cot_step_quiet(
                chain_id,
                "Phase0",
                "Checking existing positions",
                "PASS",
                "No existing position — proceeding",
                1.0,
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ═══ LAYER 1: HARD RULES GATE (runs FIRST — no agents run if this fails) ═══
        // Single top-level enforcement of ALL hard rules with priority-based conflict resolution.
        // Critical/High rules block trading. Medium rules block if no Higher rule overrides.
        // Low-priority failures are WARNINGS only — they log but don't block.
        let t1_start = std::time::Instant::now();
        let hard_rules = crate::hard_rules_gate::HardRulesGate::new(self.state.clone());
        // ═══ HARD RULES TIMEOUT ════════════════════════════════════════
        // Default 60s — rules need time to evaluate all 29 conditions properly.
        let rules_timeout: u64 = std::env::var("RAT_RULES_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let gate_result = tokio::time::timeout(
            std::time::Duration::from_secs(rules_timeout),
            hard_rules.evaluate_with_ohlcv(symbol, &ohlcv_snapshot),
        )
        .await
        .unwrap_or_else(|_| {
            println!(
                "⏱ HardRulesGate timed out after 10s for {} — blocking",
                symbol
            );
            crate::types::HardRulesGateResult {
                passed: false,
                failed_rules: vec![],
                highest_failed_priority: Some(crate::types::RulePriority::Critical),
                total_rules_checked: 0,
                reasoning_chain: crate::types::ChainOfReasoning {
                    symbol: symbol.to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    rules_evaluated: 0,
                    traces: vec![],
                    overall_verdict: "BLOCK".to_string(),
                    blocking_rule: Some("TIMEOUT".to_string()),
                    summary: "HardRulesGate timed out".to_string(),
                },
            }
        });
        let t1_dur = t1_start.elapsed().as_millis() as f64;
        enforce_min_time(t1_start, "HardRulesGate").await;

        if !gate_result.passed {
            // Push BLOCK decision to communication log
            self.state
                .push_agent_decision(AgentDecision::new(
                    "HardRulesGate",
                    symbol,
                    DecisionVerdict::Block {
                        reason: format!(
                            "Highest priority: {:?}. Failed: {}",
                            gate_result
                                .highest_failed_priority
                                .unwrap_or(crate::types::RulePriority::Low),
                            gate_result
                                .failed_rules
                                .iter()
                                .map(|r| r.rule_name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    },
                ))
                .await;
            // Finalize the communication log with BLOCK verdict
            self.state
                .finalize_pipeline_run(
                    "BLOCKED",
                    &format!(
                        "Pipeline blocked by HardRulesGate: {}",
                        gate_result
                            .failed_rules
                            .first()
                            .map(|r| r.reason.as_str())
                            .unwrap_or("unknown")
                    ),
                )
                .await;

            self.state
                .add_cot_step_quiet(
                    chain_id,
                    "HardRulesGate",
                    &format!(
                        "Hard rules check for {} ({} rules checked)",
                        symbol, gate_result.total_rules_checked
                    ),
                    "BLOCKED",
                    &format!(
                        "Highest priority: {:?}. Failed rules: {}",
                        gate_result
                            .highest_failed_priority
                            .unwrap_or(crate::types::RulePriority::Low),
                        gate_result
                            .failed_rules
                            .iter()
                            .map(|r| r.rule_name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    0.0,
                    Some(symbol.to_string()),
                    quiet,
                )
                .await;
            return Ok(PipelineSummary {
                executed: false,
                phase_results: vec![],
                total_duration_ms: start.elapsed().as_millis() as u64,
                final_signal: None,
                reason: format!(
                    "Hard Rules Gate blocked: {} (priority {:?})",
                    gate_result
                        .failed_rules
                        .first()
                        .map(|r| r.reason.as_str())
                        .unwrap_or("unknown"),
                    gate_result.highest_failed_priority
                ),
            });
        }

        self.state
            .add_cot_step_quiet(
                chain_id,
                "HardRulesGate",
                &format!(
                    "Hard rules check for {} ({} rules checked)",
                    symbol, gate_result.total_rules_checked
                ),
                "PASSED",
                &format!("All {} hard rules passed", gate_result.total_rules_checked),
                1.0,
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ═══ LAYER 2: IDENTIFIER (data gathering — advisory only, never blocks) ═══════
        // Runs all 7 sub-agents to gather market intelligence.
        // Session/red_folder checks are now informational COT entries only —
        // the HardRulesGate already enforced these as Critical rules.
        let t2_start = std::time::Instant::now();
        let rat = self.rat();
        let (discipline_ok, confluence, pivots) = rat
            .run_identifier(symbol, observed_price, chain_id)
            .await?;
        let t2_dur = t2_start.elapsed().as_millis() as f64;
        enforce_min_time(t2_start, "Identifier").await;

        // Log discipline status as informational (gate already handled blocking)
        if !discipline_ok {
            self.state
                .add_cot_step_quiet(
                    chain_id,
                    "Identifier",
                    &format!("Discipline checks for {} (informational)", symbol),
                    "INFO",
                    "Session/red_folder check flagged — already enforced by HardRulesGate",
                    0.8,
                    Some(symbol.to_string()),
                    quiet,
                )
                .await;
        }

        self.state
            .add_cot_step_quiet(
                chain_id,
                "Identifier",
                &format!("Market analysis for {} @ {:.2}", symbol, observed_price),
                "ANALYZED",
                &format!(
                    "Confluence: {:.1}%, Pivot: {:.2}, R1: {:.2}, S1: {:.2}",
                    confluence * 100.0,
                    pivots.pivot,
                    pivots.r1,
                    pivots.s1
                ),
                confluence,
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ═══ LAYER 2: VERIFIER (risk analysis — advisory only, never blocks) ═════════
        // Runs risk psychology, risk calculator, reflector.
        // Drawdown/overtrading checks are informational — the HardRulesGate enforced these.
        let t3_start = std::time::Instant::now();
        let equity = {
            let portfolio = self.state.portfolio_store.portfolio.read().await;
            portfolio.total_equity
        };
        let risk = rat
            .run_verifier(symbol, observed_price, equity, chain_id)
            .await?;
        let t3_dur = t3_start.elapsed().as_millis() as f64;
        enforce_min_time(t3_start, "Verifier").await;

        // Log risk status as informational (gate already handled blocking)
        self.state
            .add_cot_step_quiet(
                chain_id,
                "Verifier",
                &format!(
                    "Risk assessment for {} (observed price {:.2})",
                    symbol, observed_price
                ),
                "ANALYZED",
                &format!(
                    "Heat: {:.1}%, DD: {:.1}%, Recommendation: {:?}",
                    risk.portfolio_heat * 100.0,
                    risk.daily_drawdown_pct * 100.0,
                    risk.recommendation
                ),
                (1.0 - risk.portfolio_heat).max(0.0),
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ═══ PRECISION GATE: Regime Consistency Check ═══════════════════════════
        // Lightweight spot-check on first trade of day: verify recent price action
        // is consistent with the declared regime.
        {
            let portfolio = self.state.portfolio_store.portfolio.read().await;
            let total_trades = portfolio.total_trades_today;
            drop(portfolio);

            if total_trades == 0 {
                let bars = {
                    let hist = self.state.market_data.ohlcv_history.read().await;
                    hist.get(symbol).cloned().unwrap_or_default()
                };

                if bars.len() >= 100 {
                    let recent_closes: Vec<f64> =
                        bars.iter().rev().take(20).map(|b| b.close).collect();
                    let recent_trend = if recent_closes.len() >= 2 {
                        (recent_closes[0] - recent_closes[recent_closes.len() - 1])
                            / recent_closes[recent_closes.len() - 1]
                    } else {
                        0.0
                    };
                    let regime = *self.state.market_data.market_regime.read().await;

                    let regime_consistent = match &regime {
                        Some(crate::types::MarketRegime::TrendingBull) => recent_trend > -0.005,
                        Some(crate::types::MarketRegime::TrendingBear) => recent_trend < 0.005,
                        _ => true,
                    };

                    if !regime_consistent {
                        self.state
                            .add_cot_step_quiet(
                                chain_id, "WFA_Gate",
                                &format!("Regime consistency check for {}", symbol),
                                "REJECT",
                                &format!("Regime {:?} inconsistent with recent trend ({:.3}%). WFA gate blocking.", regime, recent_trend * 100.0),
                                0.1, Some(symbol.to_string()), quiet,
                            )
                            .await;
                        self.state
                            .finalize_pipeline_run(
                                "WFA_REJECT",
                                &format!(
                                    "WFA gate: Regime {:?} inconsistent with recent price action",
                                    regime
                                ),
                            )
                            .await;
                        return Ok(PipelineSummary {
                            executed: false,
                            phase_results: vec![],
                            total_duration_ms: start.elapsed().as_millis() as u64,
                            final_signal: None,
                            reason: format!(
                                "WFA gate: Regime {:?} inconsistent with recent price action",
                                regime
                            ),
                        });
                    }

                    self.state
                        .add_cot_step_quiet(
                            chain_id,
                            "WFA_Gate",
                            &format!("Regime consistency check for {}", symbol),
                            "PASS",
                            &format!(
                                "Regime {:?} consistent with recent trend ({:.3}%)",
                                regime,
                                recent_trend * 100.0
                            ),
                            0.9,
                            Some(symbol.to_string()),
                            quiet,
                        )
                        .await;
                }
            }
        }

        // ═══ DELIBERATION PERIOD ════════════════════════════════════════════
        // Agents need time to absorb Layer 1 results before debating.
        // No forced speed — allow agents to load context, fetch additional data.
        let deliberation_ms: u64 = std::env::var("RAT_DELIBERATION_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);
        if deliberation_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(deliberation_ms)).await;
        }

        // ═══ LAYER 3: DEBATE LAYER (Advisory Only) ════════════════════════════
        // Multi-round adversarial decision: Bull Team vs Bear Team → Synthesizer → Judge
        // NOTE: Debate agents are ADVISORY only. They provide evidence + confidence.
        // Only the Judge (Layer 4) has decision-making power.
        let t4_start = std::time::Instant::now();
        let debate_layer = crate::debate_layer::DebateLayer::new(self.state.clone());
        let (verdict, signal_opt) = debate_layer
            .run_debate_with_confluence(symbol, observed_price, Some(confluence))
            .await;
        let t4_dur = t4_start.elapsed().as_millis() as f64;
        enforce_min_time(t4_start, "DebateLayer").await;

        self.state
            .add_cot_step_quiet(
                chain_id,
                "DebateLayer",
                &format!(
                    "Adversarial debate for {} ({} rounds)",
                    symbol, verdict.rounds_played
                ),
                &verdict.action,
                &format!(
                    "Confidence: {:.1}%, Judge veto: {}, Rounds: {}",
                    verdict.confidence * 100.0,
                    verdict.judge_veto,
                    verdict.rounds_played
                ),
                verdict.confidence,
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ═══ LAYER 3.5: SUPERINTELLIGENCE — cross-validate the debate verdict ═══
        // Runs AFTER the debate (L3) and BEFORE the Judge (L4)/execution (L5).
        // Conviction stacking + cross-validation can DOWNGRADE a weak debate
        // signal to HOLD, or UPGRADE a debate HOLD into a properly-leveled trade
        // via the StrategyDecisionAgent (which computes entry/SL/TP and runs SI
        // itself). Without this, debate HOLDs (e.g. on LLM timeouts) never reach
        // execution. Emits a COT step so the layer is visible in the flow.
        let mut signal_opt = signal_opt;
        if !verdict.judge_veto {
            let agg = self.state.agent_memory.last_aggregated_signal.read().await.clone();
            if let Some(agg_signal) = agg {
                let regime_opt = *self.state.market_data.market_regime.read().await;
                let regime_label: &str = match regime_opt {
                    Some(crate::types::MarketRegime::TrendingBull) => "trending_bull",
                    Some(crate::types::MarketRegime::TrendingBear) => "trending_bear",
                    Some(crate::types::MarketRegime::Volatile) => "volatile",
                    Some(crate::types::MarketRegime::LowLiquidity) => "low_liquidity",
                    Some(crate::types::MarketRegime::Ranging) => "ranging",
                    None => "unknown",
                };
                let mut si_evidence = crate::debate::EvidenceBuilder::new(regime_label);
                si_evidence.add("confluence", (confluence - 0.5) * 2.0, 0.30);
                let debate_score = match verdict.action.as_str() {
                    "BUY" => verdict.confidence,
                    "SELL" => -verdict.confidence,
                    _ => 0.0,
                };
                si_evidence.add("debate", debate_score, 0.40);

                let si = crate::super_intelligence::SuperIntelligence::analyze(
                    &self.state,
                    symbol,
                    observed_price,
                    &agg_signal,
                    &si_evidence,
                    &verdict.action,
                    verdict.confidence,
                )
                .await;

                self.state
                    .add_cot_step_quiet(
                        chain_id,
                        "SuperIntelligence",
                        &format!("Layer 3.5 cross-validation for {}", symbol),
                        &si.recommended_action,
                        &format!(
                            "conviction={:.1}% validation={:.1}% proceed={} (debate said {})",
                            si.conviction.final_conviction * 100.0,
                            si.validation.overall_validation_score * 100.0,
                            si.should_proceed,
                            verdict.action,
                        ),
                        si.recommended_confidence,
                        Some(symbol.to_string()),
                        quiet,
                    )
                    .await;

                // ═══ LAYER 3.6: LLM + KRONOS — always run StrategyDecisionAgent ═══
                // The StrategyDecisionAgent runs the TriLevelValidator
                // (Rules + LLM + Kronos) to produce a consensus trade signal.
                // This is ALWAYS called — not just when debate HOLDs — so the
                // LLM and Kronos are always "in the loop" with the pipeline.
                // Default 120s — LLM + Kronos need real time for complex decisions.
                let strategy_timeout: u64 = std::env::var("RAT_STRATEGY_TIMEOUT_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(120);
                let strategy_signal = tokio::time::timeout(
                    std::time::Duration::from_secs(strategy_timeout),
                    self.strategy.generate_signal_with_debate(symbol, observed_price, &verdict),
                )
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten();

                if let Some(ref strat_sig) = strategy_signal {
                    let dir_label = if strat_sig.direction == TradeDirection::Long {
                        "BUY"
                    } else {
                        "SELL"
                    };
                    println!(
                        "[Pipeline] 🧠 StrategyDecisionAgent (LLM+Kronos): {} {} @ entry={:.2} SL={:.2} TP={:.2} (conf={:.1}%)",
                        dir_label, symbol,
                        strat_sig.entry_price, strat_sig.stop_loss, strat_sig.take_profit,
                        strat_sig.confidence_score * 100.0
                    );
                    self.state
                        .add_cot_step_quiet(
                            chain_id,
                            "StrategyDecision",
                            &format!(
                                "LLM+Kronos evaluation for {} (TriLevel: Rules+LLM+Kronos)",
                                symbol
                            ),
                            dir_label,
                            &format!(
                                "entry={:.2} SL={:.2} TP={:.2} conf={:.1}% | TriLevel consensus",
                                strat_sig.entry_price,
                                strat_sig.stop_loss,
                                strat_sig.take_profit,
                                strat_sig.confidence_score * 100.0
                            ),
                            strat_sig.confidence_score,
                            Some(symbol.to_string()),
                            quiet,
                        )
                        .await;

                    // Compare debate signal vs strategy signal — pick the best
                    if signal_opt.is_none() {
                        // No debate signal → UPGRADE with strategy signal
                        println!(
                            "[Pipeline] 🧠 StrategyDecisionAgent UPGRADE: debate HOLD → {} for {} (LLM+Kronos confirmed)",
                            dir_label, symbol
                        );
                        signal_opt = Some(strat_sig.clone());
                    } else {
                        // Both debate and strategy have signals → use higher confidence
                        let debate_conf = signal_opt
                            .as_ref()
                            .map(|s| s.confidence_score)
                            .unwrap_or(0.0);
                        if strat_sig.confidence_score > debate_conf + 0.10 {
                            println!(
                                "[Pipeline] 🧠 StrategyDecisionAgent OVERRIDE: debate conf={:.1}% < strategy conf={:.1}% — using LLM+Kronos signal",
                                debate_conf * 100.0, strat_sig.confidence_score * 100.0
                            );
                            signal_opt = Some(strat_sig.clone());
                        } else if (debate_conf - strat_sig.confidence_score).abs() < 0.10 {
                            println!(
                                "[Pipeline] ✅ LLM+Kronos CONFIRMS debate: both ~{:.1}% conf — keeping debate signal",
                                debate_conf * 100.0
                            );
                        }
                    }
                } else {
                    println!(
                        "[Pipeline] 🧠 StrategyDecisionAgent (LLM+Kronos): HOLD for {} — clearing debate signal",
                        symbol
                    );
                    self.state
                        .add_cot_step_quiet(
                            chain_id,
                            "StrategyDecision",
                            &format!("LLM+Kronos evaluation for {}", symbol),
                            "HOLD",
                            "StrategyDecisionAgent returned HOLD — clearing debate signal to prevent stale execution",
                            0.5,
                            Some(symbol.to_string()),
                            quiet,
                        )
                        .await;
                    // CRITICAL FIX: When strategy returns HOLD, clear the debate signal
                    // to prevent executing a stale BUY/SELL from the debate layer.
                    signal_opt = None;
                }

                // SI DOWNGRADE still applies: suppress if SI says no
                if !si.should_proceed && signal_opt.is_some() {
                    println!(
                        "[Pipeline] 🧠 SuperIntelligence DOWNGRADE: {} → HOLD for {} (SI lacks conviction)",
                        verdict.action, symbol
                    );
                    signal_opt = None;
                }
            }
        }

        // Separate Judge COT entry so the TUI pipeline flow can track L4 status.
        // The TUI looks for agent="Judge" with action="APPROVE" or "VETO".
        self.state
            .add_cot_step_quiet(
                chain_id,
                "Judge",
                &format!("Final adjudication for {}", symbol),
                if verdict.judge_veto {
                    "VETO"
                } else {
                    "APPROVE"
                },
                &format!(
                    "{} | confidence={:.1}% | veto={} | synthesis_action={}",
                    verdict.reasoning,
                    verdict.confidence * 100.0,
                    verdict.judge_veto,
                    verdict.action,
                ),
                verdict.confidence,
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ── Push debate verdict to communication log ───────────────────
        if verdict.judge_veto {
            self.state
                .push_agent_decision(
                    AgentDecision::new(
                        "Judge",
                        symbol,
                        DecisionVerdict::Veto {
                            reason: verdict.reasoning.clone(),
                        },
                    )
                    .with_evidence(vec![
                        format!("confidence: {:.1}%", verdict.confidence * 100.0),
                        format!("synthesis_action: {}", verdict.action),
                        format!("rounds: {}", verdict.rounds_played),
                    ])
                    .addressed_to("ExecutionCoordinator"),
                )
                .await;
        } else if let Some(ref sig) = signal_opt {
            let dir_label = if sig.direction == TradeDirection::Long {
                "BUY"
            } else {
                "SELL"
            };
            self.state
                .push_agent_decision(
                    AgentDecision::new(
                        "Judge",
                        symbol,
                        match sig.direction {
                            TradeDirection::Long => DecisionVerdict::Buy {
                                reason: verdict.reasoning.clone(),
                                confidence: verdict.confidence,
                            },
                            _ => DecisionVerdict::Sell {
                                reason: verdict.reasoning.clone(),
                                confidence: verdict.confidence,
                            },
                        },
                    )
                    .with_evidence(vec![
                        format!("confidence: {:.1}%", verdict.confidence * 100.0),
                        format!("direction: {}", dir_label),
                        format!(
                            "entry: {:.2} SL: {:.2} TP: {:.2}",
                            sig.entry_price, sig.stop_loss, sig.take_profit
                        ),
                    ])
                    .addressed_to("ExecutionCoordinator"),
                )
                .await;
        } else {
            self.state
                .push_agent_decision(
                    AgentDecision::new(
                        "Judge",
                        symbol,
                        DecisionVerdict::Hold {
                            reason: "Debate synthesis returned HOLD — insufficient conviction"
                                .to_string(),
                        },
                    )
                    .with_evidence(vec![format!(
                        "confidence: {:.1}%",
                        verdict.confidence * 100.0
                    )]),
                )
                .await;
        }

        // ═══ LAYER 5: EXECUTE TRADE (Fix #2: fresh price at execution) ══
        // Re-fetch the latest price right before execution instead of using
        // observed_price from pipeline start (which may be 10-60 seconds stale).
        // The latest close from OHLCV history is used for the entry price.
        let _t5_start = std::time::Instant::now();
        let execution_price = {
            let history = self.state.market_data.ohlcv_history.read().await;
            history
                .get(symbol)
                .and_then(|bars| bars.last().map(|b| b.close))
                .unwrap_or(observed_price)
        };
        // Update the signal's entry price to the fresh execution price
        // (signal_opt is already mutable from the Layer 3.5 step above).
        if let Some(ref mut sig) = signal_opt {
            // Adjust SL/TP proportionally to account for price shift
            let price_shift = execution_price - sig.entry_price;
            sig.entry_price = execution_price;
            sig.stop_loss += price_shift;
            sig.take_profit += price_shift;
            println!(
                "[Pipeline] Execution price updated: {:.2} (was {:.2}, shifted SL/TP by {:.4})",
                execution_price, observed_price, price_shift
            );
        }
        let mut execution_failure: Option<String> = None;
        let executed = if verdict.judge_veto {
            self.state
                .add_cot_step_quiet(
                    chain_id,
                    "ExecutionCoordinator",
                    &format!("Judge veto for {} — execution skipped", symbol),
                    "SKIPPED",
                    "Judge vetoed debate verdict — no trade placed",
                    0.0,
                    Some(symbol.to_string()),
                    quiet,
                )
                .await;
            false
        } else if let Some(ref sig) = signal_opt {
            match self
                .execution
                .execute_paper_trade_with_chain(sig, Some(chain_id))
                .await
            {
                Ok(result) => {
                    println!("[Pipeline] ✅ Trade executed: {}", result);

                    let _ = self.outcome_logger.run(None).await;
                    self.state
                        .add_cot_step_quiet(
                            chain_id,
                            "OutcomeLogger",
                            "Logging trade outcome",
                            "LOGGED",
                            &format!("Trade logged for {} {:?}", symbol, sig.direction),
                            0.8,
                            Some(symbol.to_string()),
                            quiet,
                        )
                        .await;

                    true
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    println!("[Pipeline] ❌ Trade execution failed: {}", err_msg);
                    execution_failure = Some(err_msg);
                    false
                }
            }
        } else {
            self.state
                .add_cot_step_quiet(
                    chain_id,
                    "ExecutionCoordinator",
                    &format!("AGENTIC HOLD for {}", symbol),
                    "HOLD",
                    "Debate layer decided HOLD — no trade placed",
                    0.0,
                    Some(symbol.to_string()),
                    quiet,
                )
                .await;
            false
        };

        // ── Pipeline completion ──────────────────────────────────────────────
        let total_ms = start.elapsed().as_millis() as u64;
        let (final_action, exec_reason) = if executed {
            if let Some(sig) = signal_opt.as_ref() {
                (
                    "TRADE_EXECUTED",
                    format!(
                        "Pipeline complete: {} {} @ entry {:.2} (SL {:.2} TP {:.2}) in {}ms",
                        symbol,
                        if sig.direction == TradeDirection::Long {
                            "BUY"
                        } else {
                        "SELL"
                    },
                    sig.entry_price,
                    sig.stop_loss,
                    sig.take_profit,
                    total_ms
                ),
                )
            } else {
                (
                    "TRADE_EXECUTED",
                    format!("Pipeline complete: {} trade executed in {}ms (signal details unavailable)", symbol, total_ms),
                )
            }
        } else if verdict.judge_veto {
            (
                "JUDGE_VETO",
                format!(
                    "Pipeline complete: JUDGE VETO for {} in {}ms — {}",
                    symbol, total_ms, verdict.reasoning
                ),
            )
        } else if let Some(err) = &execution_failure {
            (
                "EXECUTION_FAILED",
                format!(
                    "Pipeline complete: EXECUTION FAILED for {} in {}ms — {}",
                    symbol, total_ms, err
                ),
            )
        } else {
            (
                "HOLD",
                format!("Pipeline complete: HOLD for {} in {}ms", symbol, total_ms),
            )
        };

        self.state
            .add_cot_step_quiet(
                chain_id,
                "Decision",
                "Pipeline final decision",
                final_action,
                &exec_reason,
                if executed { 0.9 } else { 0.5 },
                Some(symbol.to_string()),
                quiet,
            )
            .await;

        // ── Push execution decision to communication log ───────────────
        if executed {
            self.state
                .push_agent_decision(
                    AgentDecision::new(
                        "ExecutionCoordinator",
                        symbol,
                        DecisionVerdict::Pass {
                            reason: exec_reason.clone(),
                        },
                    )
                    .addressed_to("Pipeline"),
                )
                .await;
        } else if verdict.judge_veto {
            // Already pushed VETO above — skip duplicate
        } else {
            self.state
                .push_agent_decision(
                    AgentDecision::new(
                        "ExecutionCoordinator",
                        symbol,
                        DecisionVerdict::Skip {
                            reason: "No signal generated — HOLD".to_string(),
                        },
                    )
                    .addressed_to("Pipeline"),
                )
                .await;
        }

        // ── Finalize communication log ──────────────────────────────────
        let blocking_reasons = self.state.get_blocking_reasons().await;
        let total_decisions = {
            let log = self.state.io.communication_log.read().await;
            log.decisions.len()
        };
        let summary = format!(
            "{} decisions | final: {} | blocking: {}",
            total_decisions,
            final_action,
            blocking_reasons.len()
        );
        self.state
            .finalize_pipeline_run(final_action, &summary)
            .await;

        // ── Push summary COT entry ──────────────────────────────────────
        // Instead of 17 per-agent COT entries per run, push ONE summary entry
        // with all layer results embedded. Per-agent add_cot_step calls above
        // still broadcast to TUI in real-time but don't persist to SQLite.
        let exec_dur = if total_ms as f64 - t1_dur - t2_dur - t3_dur - t4_dur > 0.0 {
            total_ms as f64 - t1_dur - t2_dur - t3_dur - t4_dur
        } else {
            0.0
        };
        let hard_rules_reason = format!("{} rules checked", gate_result.total_rules_checked);
        let identifier_reason = format!("Confluence: {:.1}%", confluence * 100.0);
        let verifier_reason = format!(
            "Heat: {:.1}%, DD: {:.1}%",
            risk.portfolio_heat * 100.0,
            risk.daily_drawdown_pct * 100.0
        );
        let debate_reason = format!(
            "Action: {}, conf: {:.1}%",
            verdict.action,
            verdict.confidence * 100.0
        );
        let judge_reason = format!("Veto: {}, Action: {}", verdict.judge_veto, verdict.action);
        let summary_layers: Vec<(&str, &str, f64, &str)> = vec![
            (
                "HardRulesGate",
                if gate_result.passed { "PASS" } else { "FAIL" },
                t1_dur,
                &hard_rules_reason,
            ),
            ("Identifier", "PASS", t2_dur, &identifier_reason),
            ("Verifier", "PASS", t3_dur, &verifier_reason),
            (
                "DebateLayer",
                if verdict.judge_veto { "FAIL" } else { "PASS" },
                t4_dur,
                &debate_reason,
            ),
            (
                "Judge",
                if verdict.judge_veto {
                    "VETO"
                } else {
                    "APPROVE"
                },
                0.0,
                &judge_reason,
            ),
            (
                "Execution",
                if executed { "EXECUTED" } else { "FAILED" },
                exec_dur,
                &exec_reason,
            ),
        ];
        self.state
            .push_summary_cot(chain_id, symbol, summary_layers, final_action, &exec_reason)
            .await;

        // Log pipeline result
        println!(
            "[Pipeline] {} {} ({}ms, executed={})",
            symbol, final_action, total_ms, executed
        );

        // ── Send pipeline run event to metrics service (fire-and-forget) ──
        let layers = vec![
            (
                "hard_rules_gate",
                t1_dur,
                if gate_result.passed {
                    "PASS"
                } else {
                    "BLOCKED"
                },
            ),
            ("identifier", t2_dur, "ANALYZED"),
            ("verifier", t3_dur, "ANALYZED"),
            ("debate", t4_dur, &verdict.action),
            (
                "execution",
                {
                    // Compute execution duration from total minus layer times
                    let layer_sum = t1_dur + t2_dur + t3_dur + t4_dur;

                    if total_ms as f64 - layer_sum > 0.0 {
                        total_ms as f64 - layer_sum
                    } else {
                        0.0
                    }
                },
                if executed {
                    "EXECUTED"
                } else if execution_failure.is_some() {
                    "FAILED"
                } else if verdict.judge_veto {
                    "VETO"
                } else {
                    "HOLD"
                },
            ),
        ];
        send_pipeline_event_to_metrics(symbol, layers[4].2, total_ms as f64, layers.clone()).await;

        // Send latency samples for each layer
        send_latency_to_metrics("hard_rules_gate", t1_dur, Some(symbol)).await;
        send_latency_to_metrics("market_intel", t2_dur + t3_dur, Some(symbol)).await;
        send_latency_to_metrics("debate", t4_dur, Some(symbol)).await;
        send_latency_to_metrics("pipeline", total_ms as f64, Some(symbol)).await;

        // Trade outcome events are sent at trade CLOSE time by the OutcomeProcessor,
        // not at trade OPEN time. Sending at open time with placeholder values would
        // corrupt metrics (recording every trade as a losing trade).

        Ok(PipelineSummary {
            executed,
            phase_results: vec![],
            total_duration_ms: total_ms,
            final_signal: signal_opt,
            reason: exec_reason,
        })
    }
}
