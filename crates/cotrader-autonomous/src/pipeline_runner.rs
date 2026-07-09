//! Orchestrated pipeline execution — preflight market data, serialization, batch runs.

use crate::orchestrator_struct::AutonomousOrchestrator;
use crate::state::SharedState;
use crate::types::PipelineSummary;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use cotrader_core::{OhlcvBar, TradeDirection};

/// Global pipeline lock — allow up to 3 concurrent pipeline runs.
static PIPELINE_SEM: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(3)));

/// Publish a pipeline lifecycle event (simplified — no eventbus).
pub async fn publish_pipeline_event(
    _event_bus: &dyn std::any::Any,
    symbol: &str,
    action: &str,
    duration_ms: f64,
    executed: bool,
) {
    println!(
        "[Pipeline] {} {} ({}ms, executed={})",
        symbol, action, duration_ms as u64, executed
    );
}

/// Per-symbol cycle dedup: tracks which symbols have an in-flight pipeline run.
static IN_FLIGHT: Lazy<std::sync::Mutex<HashSet<String>>> =
    Lazy::new(|| std::sync::Mutex::new(HashSet::new()));

/// RAII guard that removes a symbol from the IN_FLIGHT set when dropped.
struct InFlightGuard(String);
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = IN_FLIGHT.lock() {
            set.remove(&self.0);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineRunReport {
    pub symbol: String,
    pub success: bool,
    pub executed: bool,
    pub action: String,
    pub reason: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchPipelineReport {
    pub success: bool,
    pub symbols_run: usize,
    pub trades_executed: usize,
    pub total_duration_ms: u64,
    pub results: Vec<PipelineRunReport>,
}

fn normalize_symbol(symbol: &str) -> String {
    cotrader_core::normalize_base_symbol(symbol)
}

fn summary_to_report(symbol: &str, summary: &PipelineSummary) -> PipelineRunReport {
    let action = summary
        .final_signal
        .as_ref()
        .map(|s| {
            if s.direction == TradeDirection::Long {
                "BUY".to_string()
            } else {
                "SELL".to_string()
            }
        })
        .unwrap_or_else(|| "HOLD".to_string());

    PipelineRunReport {
        symbol: symbol.to_string(),
        success: true,
        executed: summary.executed,
        action,
        reason: summary.reason.clone(),
        duration_ms: summary.total_duration_ms,
        error: None,
    }
}

/// Ensure OHLCV history + live price exist before running the agentic pipeline.
/// All data is fetched through Tredo Exchange — the single price gateway.
pub async fn ensure_market_data(
    symbol: &str,
    client: &reqwest::Client,
    state: &SharedState,
) -> Result<f64, String> {
    let sym = normalize_symbol(symbol);

    let bar_count = {
        let history = state.market_data.ohlcv_history.read().await;
        history.get(&sym).map(|b| b.len()).unwrap_or(0)
    };

    // Fetch OHLCV candles from Tredo Exchange
    if bar_count < 20 {
        let bars = cotrader_core::fetch_tredo_candles(client, &sym, "1m", 100)
            .await
            .unwrap_or_default();

        if bars.is_empty() {
            return Err(format!("Tredo Exchange: no OHLCV bars returned for {sym}"));
        }
        let n = bars.len();
        state.market_data.ohlcv_history.write().await.insert(sym.clone(), bars);
        println!("[PipelineRunner] Loaded {n} OHLCV bars from Tredo Exchange for {sym}");
    }

    // Fetch live price from Tredo Exchange
    // If Tredo is unreachable, fall back to the last OHLCV close price (enables offline testing)
    let live_price = match cotrader_core::fetch_tredo_price(client, &sym).await {
        Ok(price) => price,
        Err(_) => {
            let fallback = {
                let history = state.market_data.ohlcv_history.read().await;
                history.get(&sym).and_then(|b| b.last().map(|b| b.close)).unwrap_or(0.0)
            };
            if fallback > 0.0 {
                println!(
                    "[PipelineRunner] Tredo unreachable — using last OHLCV close {:.2} for {}",
                    fallback, sym
                );
                fallback
            } else {
                return Err(format!(
                    "No live price from Tredo and no OHLCV history for {sym}"
                ));
            }
        }
    };

    println!("[PipelineRunner] {} live price from Tredo: {:.2}", sym, live_price);

    // Update the latest bar with the live price
    {
        let mut history = state.market_data.ohlcv_history.write().await;
        let hist = history.entry(sym.clone()).or_default();
        let now = Utc::now();
        if hist.is_empty() {
            hist.push(OhlcvBar {
                timestamp: now.to_rfc3339(),
                open: live_price,
                high: live_price,
                low: live_price,
                close: live_price,
                volume: 0.0,
            });
        } else if let Some(last) = hist.last_mut() {
            last.close = live_price;
            if live_price > last.high {
                last.high = live_price;
            }
            if live_price < last.low {
                last.low = live_price;
            }
            last.timestamp = now.to_rfc3339();
        }
    }

    Ok(live_price)
}

/// Outcome of a single pipeline run (report + optional full summary for episode capture).
pub struct PipelineRunOutcome {
    pub report: PipelineRunReport,
    pub summary: Option<PipelineSummary>,
}

/// Run pipeline for a single symbol with preflight + global lock.
pub async fn run_single_quiet(
    orchestrator: &AutonomousOrchestrator,
    client: &reqwest::Client,
    symbol: &str,
    quiet: bool,
) -> PipelineRunOutcome {
    let sym = normalize_symbol(symbol);
    let started = Instant::now();

    // ── Cycle dedup: skip if this symbol already has a pipeline in-flight ──
    {
        let mut in_flight = IN_FLIGHT.lock().unwrap();
        if !in_flight.insert(sym.clone()) {
            println!(
                "[PipelineRunner] ⏭ {} already in-flight — skipping (dedup)",
                sym
            );
            return PipelineRunOutcome {
                report: PipelineRunReport {
                    symbol: sym,
                    success: true,
                    executed: false,
                    action: "SKIPPED".to_string(),
                    reason: "In-flight dedup".to_string(),
                    duration_ms: 0,
                    error: None,
                },
                summary: None,
            };
        }
    }
    let _guard = InFlightGuard(sym.clone());

    let _permit = match PIPELINE_SEM.acquire().await {
        Ok(p) => p,
        Err(e) => {
            return PipelineRunOutcome {
                report: PipelineRunReport {
                    symbol: sym,
                    success: false,
                    executed: false,
                    action: "ERROR".to_string(),
                    reason: "Pipeline lock unavailable".to_string(),
                    duration_ms: 0,
                    error: Some(e.to_string()),
                },
                summary: None,
            };
        }
    };

    if let Err(e) = ensure_market_data(&sym, client, &orchestrator.state).await {
        return PipelineRunOutcome {
            report: PipelineRunReport {
                symbol: sym,
                success: false,
                executed: false,
                action: "SKIPPED".to_string(),
                reason: format!("Market data not ready: {e}"),
                duration_ms: started.elapsed().as_millis() as u64,
                error: Some(e),
            },
            summary: None,
        };
    }

    match orchestrator.run_full_pipeline_quiet(&sym, quiet).await {
        Ok(summary) => PipelineRunOutcome {
            report: summary_to_report(&sym, &summary),
            summary: Some(summary),
        },
        Err(e) => PipelineRunOutcome {
            report: PipelineRunReport {
                symbol: sym,
                success: false,
                executed: false,
                action: "ERROR".to_string(),
                reason: format!("Pipeline error: {e}"),
                duration_ms: started.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            },
            summary: None,
        },
    }
}

/// Legacy wrapper — calls run_single_quiet with quiet=false.
pub async fn run_single(
    orchestrator: &AutonomousOrchestrator,
    client: &reqwest::Client,
    symbol: &str,
) -> PipelineRunOutcome {
    run_single_quiet(orchestrator, client, symbol, false).await
}

/// Run pipeline sequentially for many symbols (safe for LLM + portfolio).
pub async fn run_batch(
    orchestrator: &AutonomousOrchestrator,
    client: &reqwest::Client,
    symbols: &[String],
) -> BatchPipelineReport {
    let started = Instant::now();
    let mut results = Vec::with_capacity(symbols.len());
    let mut trades_executed = 0usize;

    for symbol in symbols {
        let sym = normalize_symbol(symbol);
        if sym.is_empty() {
            continue;
        }
        println!("[PipelineRunner] ▶ Running pipeline for {sym}...");
        let outcome = run_single(orchestrator, client, &sym).await;
        if outcome.report.executed {
            trades_executed += 1;
        }
        results.push(outcome.report);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    BatchPipelineReport {
        success: results.iter().all(|r| r.success),
        symbols_run: results.len(),
        trades_executed,
        total_duration_ms: started.elapsed().as_millis() as u64,
        results,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Whitelist Sequential Execution Loop — Per-Symbol Cooldown
// ═══════════════════════════════════════════════════════════════════════════════

/// Configuration for the whitelist run loop.
#[derive(Debug, Clone)]
pub struct WhitelistConfig {
    pub symbols: Vec<String>,
    pub cooldown_secs: u64,
    pub inter_symbol_delay_ms: u64,
    pub skip_in_cooldown: bool,
}

impl Default for WhitelistConfig {
    fn default() -> Self {
        Self {
            symbols: vec!["BTC".to_string(), "ETH".to_string(), "SOL".to_string()],
            cooldown_secs: 1800,
            inter_symbol_delay_ms: 500,
            skip_in_cooldown: true,
        }
    }
}

/// Per-symbol cooldown tracker.
#[derive(Debug, Clone)]
pub struct SymbolCooldownTracker {
    last_run: HashMap<String, DateTime<Utc>>,
    cooldown_secs: u64,
}

impl SymbolCooldownTracker {
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            last_run: HashMap::new(),
            cooldown_secs,
        }
    }

    pub fn is_allowed(&self, symbol: &str) -> bool {
        match self.last_run.get(symbol) {
            Some(last) => {
                let elapsed = (Utc::now() - *last).num_seconds() as u64;
                elapsed >= self.cooldown_secs
            }
            None => true,
        }
    }

    pub fn record_run(&mut self, symbol: &str) {
        self.last_run.insert(symbol.to_string(), Utc::now());
    }

    pub fn remaining_secs(&self, symbol: &str) -> u64 {
        match self.last_run.get(symbol) {
            Some(last) => {
                let elapsed = (Utc::now() - *last).num_seconds() as u64;
                self.cooldown_secs.saturating_sub(elapsed)
            }
            None => 0,
        }
    }
}

/// Run the whitelist sequential loop.
pub async fn run_whitelist_loop(
    orchestrator: &AutonomousOrchestrator,
    client: &reqwest::Client,
    config: &WhitelistConfig,
    cooldown: &mut SymbolCooldownTracker,
) -> BatchPipelineReport {
    let started = Instant::now();
    let mut results = Vec::with_capacity(config.symbols.len());
    let mut trades_executed = 0usize;

    for symbol in &config.symbols {
        let sym = normalize_symbol(symbol);
        if sym.is_empty() {
            continue;
        }

        if config.skip_in_cooldown && !cooldown.is_allowed(&sym) {
            let remaining = cooldown.remaining_secs(&sym);
            println!(
                "[WhitelistLoop] ⏳ {} in cooldown ({}s remaining) — skipping",
                sym, remaining
            );
            results.push(PipelineRunReport {
                symbol: sym,
                success: true,
                executed: false,
                action: "SKIPPED".to_string(),
                reason: format!("In cooldown ({}s remaining)", remaining),
                duration_ms: 0,
                error: None,
            });
            continue;
        }

        println!(
            "[WhitelistLoop] ▶ Running pipeline for {} (whitelist loop)...",
            sym
        );

        let outcome = run_single(orchestrator, client, &sym).await;
        if outcome.report.executed {
            trades_executed += 1;
        }

        cooldown.record_run(&sym);
        results.push(outcome.report);

        if config.inter_symbol_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(config.inter_symbol_delay_ms)).await;
        }
    }

    BatchPipelineReport {
        success: results.iter().all(|r| r.success),
        symbols_run: results.len(),
        trades_executed,
        total_duration_ms: started.elapsed().as_millis() as u64,
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooldown_allowed_when_never_run() {
        let tracker = SymbolCooldownTracker::new(300);
        assert!(tracker.is_allowed("BTC"));
        assert!(tracker.is_allowed("ETH"));
    }

    #[test]
    fn test_cooldown_blocks_immediate_rerun() {
        let mut tracker = SymbolCooldownTracker::new(300);
        tracker.record_run("BTC");
        assert!(!tracker.is_allowed("BTC"), "Should not be allowed immediately after run");
    }

    #[test]
    fn test_cooldown_allows_different_symbols() {
        let mut tracker = SymbolCooldownTracker::new(300);
        tracker.record_run("BTC");
        assert!(tracker.is_allowed("ETH"));
        assert!(!tracker.is_allowed("BTC"));
    }

    #[test]
    fn test_cooldown_remaining_secs() {
        let mut tracker = SymbolCooldownTracker::new(3600);
        assert_eq!(tracker.remaining_secs("BTC"), 0);
        tracker.record_run("BTC");
        let rem = tracker.remaining_secs("BTC");
        assert!(rem > 3590, "Should have nearly full cooldown remaining, got {}", rem);
        assert!(rem <= 3600, "Should not exceed cooldown, got {}", rem);
    }

    #[test]
    fn test_cooldown_resets_after_zero_second_cooldown() {
        let mut tracker = SymbolCooldownTracker::new(0);
        assert!(tracker.is_allowed("BTC"));
        tracker.record_run("BTC");
        assert!(tracker.is_allowed("BTC"));
    }

    #[test]
    fn test_empty_cooldown_tracker_returns_zero_remaining() {
        let tracker = SymbolCooldownTracker::new(300);
        assert_eq!(tracker.remaining_secs("NONEXISTENT"), 0);
        assert!(tracker.is_allowed("NONEXISTENT"));
    }
}
