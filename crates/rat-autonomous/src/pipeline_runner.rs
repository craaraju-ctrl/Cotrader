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
use rat_eventbus::EventBus;
use rat_core::{OhlcvBar, TradeDirection};

/// Global pipeline lock — allow up to 3 concurrent pipeline runs (was 1, caused 26-minute backlog).
/// Each symbol runs independently; the semaphore prevents LLM/provider overload.
static PIPELINE_SEM: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(3)));

/// Publish a pipeline lifecycle event to the EventBus.
/// Called from `orchestrator_pipeline.rs` when the pipeline completes for a symbol.
pub async fn publish_pipeline_event(
    event_bus: &dyn EventBus,
    symbol: &str,
    action: &str,
    duration_ms: f64,
    executed: bool,
) {
    let event = rat_eventbus::RatEvent::Signal(rat_eventbus::SignalEvent {
        symbol: symbol.to_string(),
        action: action.to_string(),
        entry_price: 0.0,
        stop_loss: 0.0,
        take_profit: 0.0,
        confidence: if executed { 0.8 } else { 0.0 },
        reasoning: format!("Pipeline completed: {} ({}ms)", action, duration_ms as u64),
        source: "pipeline".to_string(),
        timestamp_micros: chrono::Utc::now().timestamp_micros(),
    });
    let _ = event_bus
        .publish(&rat_eventbus::subjects::signal(symbol), &event)
        .await;
}

/// Per-symbol cycle dedup: tracks which symbols have an in-flight pipeline run.
/// If a symbol is already running, skip it instead of queuing (prevents backlog growth).
/// Uses std::sync::Mutex (not tokio) because Drop is sync and the critical section is tiny.
static IN_FLIGHT: Lazy<std::sync::Mutex<HashSet<String>>> =
    Lazy::new(|| std::sync::Mutex::new(HashSet::new()));

/// RAII guard that removes a symbol from the IN_FLIGHT set when dropped.
/// Uses std::sync::Mutex::lock() which blocks briefly but never silently leaks.
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
    rat_core::normalize_base_symbol(symbol)
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

async fn fetch_yahoo_ohlcv(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Vec<OhlcvBar>, Box<dyn std::error::Error + Send + Sync>> {
    let yahoo_symbol = rat_core::yahoo_symbol(symbol);
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{yahoo_symbol}?interval=1m&range=1d"
    );
    let resp: serde_json::Value = client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        )
        .timeout(Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;

    let result = &resp["chart"]["result"][0];
    let timestamps: Vec<i64> = result["timestamp"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();
    let quote = &result["indicators"]["quote"][0];
    let parse_arr = |key: &str| -> Vec<f64> {
        quote[key]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_default()
    };
    let opens = parse_arr("open");
    let highs = parse_arr("high");
    let lows = parse_arr("low");
    let closes = parse_arr("close");
    let volumes = parse_arr("volume");

    let n = timestamps
        .len()
        .min(opens.len())
        .min(highs.len())
        .min(lows.len())
        .min(closes.len())
        .min(volumes.len());

    let mut bars = Vec::with_capacity(n);
    for i in 0..n {
        let dt = chrono::DateTime::from_timestamp(timestamps[i], 0).unwrap_or_else(Utc::now);
        bars.push(OhlcvBar {
            timestamp: dt.to_rfc3339(),
            open: opens[i],
            high: highs[i],
            low: lows[i],
            close: closes[i],
            volume: volumes[i],
        });
    }
    Ok(bars)
}

/// Ensure OHLCV history + live price exist before running the agentic pipeline.
pub async fn ensure_market_data(
    symbol: &str,
    client: &reqwest::Client,
    state: &SharedState,
) -> Result<f64, String> {
    let sym = normalize_symbol(symbol);
    let is_crypto = rat_core::is_crypto_symbol(&sym);

    let bar_count = {
        let history = state.market_data.ohlcv_history.read().await;
        history.get(&sym).map(|b| b.len()).unwrap_or(0)
    };

    if bar_count < 20 {
        let bars = if is_crypto {
            rat_core::fetch_klines(client, &sym, "1m", 100)
                .await
                .map_err(|e| format!("Binance klines: {e}"))?
        } else {
            fetch_yahoo_ohlcv(client, &sym)
                .await
                .map_err(|e| format!("Yahoo OHLCV: {e}"))?
        };
        if bars.is_empty() {
            return Err(format!("No OHLCV bars returned for {sym}"));
        }
        let n = bars.len();
        state.market_data.ohlcv_history.write().await.insert(sym.clone(), bars);
        println!("[PipelineRunner] Loaded {n} OHLCV bars for {sym}");
    }

    let live_price = if is_crypto {
        rat_core::fetch_binance_price(client, &sym)
            .await
            .map_err(|e| format!("Binance price: {e}"))?
    } else {
        let yahoo_symbol = rat_core::yahoo_symbol(&sym);
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{yahoo_symbol}?interval=1m&range=1d"
        );
        let resp: serde_json::Value = client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Yahoo price: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Yahoo parse: {e}"))?;
        let meta = &resp["chart"]["result"][0]["meta"];
        // Try regularMarketPrice first; fall back to chartPreviousClose when markets are closed
        meta["regularMarketPrice"]
            .as_f64()
            .or_else(|| meta["chartPreviousClose"].as_f64())
            .ok_or_else(|| {
                format!("Yahoo: no price for {sym} (no regularMarketPrice or chartPreviousClose)")
            })?
    };

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
    // RAII guard: remove from in-flight set when done (even on panic)
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
        // Brief pause between symbols to avoid rate limits
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
    /// Symbols to scan (e.g., ["BTC", "ETH", "SOL"]). Inner loop iterates these.
    pub symbols: Vec<String>,
    /// Minimum cooldown between runs of the SAME symbol (seconds).
    /// E.g., 1800 = 30 minutes between BTC → BTC runs.
    pub cooldown_secs: u64,
    /// Pause between different symbols (seconds). Prevents rate-limit hammering.
    pub inter_symbol_delay_ms: u64,
    /// If true, skip symbols currently in cooldown (don't block, just move on).
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
    /// Map of symbol → last run timestamp.
    last_run: HashMap<String, DateTime<Utc>>,
    /// Default cooldown duration.
    cooldown_secs: u64,
}

impl SymbolCooldownTracker {
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            last_run: HashMap::new(),
            cooldown_secs,
        }
    }

    /// Returns true if the symbol is allowed to run (not in cooldown).
    pub fn is_allowed(&self, symbol: &str) -> bool {
        match self.last_run.get(symbol) {
            Some(last) => {
                let elapsed = (Utc::now() - *last).num_seconds() as u64;
                elapsed >= self.cooldown_secs
            }
            None => true, // never run before
        }
    }

    /// Record that a symbol has just been processed.
    pub fn record_run(&mut self, symbol: &str) {
        self.last_run.insert(symbol.to_string(), Utc::now());
    }

    /// Time remaining until the symbol can run again (seconds). 0 if allowed.
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

/// Run the whitelist sequential loop: for each symbol in the whitelist, capture
/// a single OHLCV snapshot and run it through HardRulesGate + LLM + Kronos
/// (all 3 layers see identical market data), then execute if passed.
///
/// Cooldown is per-symbol: BTC runs once, then ETH runs once, then SOL runs once,
/// then back to BTC (if cooldown has elapsed).
///
/// ## Output
/// Returns a `BatchPipelineReport` with one result per whitelist symbol per loop.
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

        // Check cooldown before running
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

        // Record the run timestamp for cooldown
        cooldown.record_run(&sym);
        results.push(outcome.report);

        // Brief pause between symbols to avoid rate limits
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

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── EventBus: publish_pipeline_event ───────────────────────────────────

    #[tokio::test]
    async fn test_publish_pipeline_event_sends_signal_event() {
        let bus = rat_eventbus::InMemoryEventBus::new();
        let mut sub = bus.subscribe("rat.signal.*").await.unwrap();

        publish_pipeline_event(&bus, "BTC", "BUY", 150.0, true).await;

        let (subject, event) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            sub.recv(),
        )
        .await
        .expect("Should receive event within timeout")
        .expect("Should receive Some event");

        assert_eq!(subject, "rat.signal.btc");
        match event {
            rat_eventbus::RatEvent::Signal(sig) => {
                assert_eq!(sig.symbol, "BTC");
                assert_eq!(sig.action, "BUY");
                assert_eq!(sig.source, "pipeline");
                assert!((sig.confidence - 0.8).abs() < 0.01, "executed=true → confidence=0.8");
                assert!(sig.reasoning.contains("BUY"), "reasoning contains action");
                assert!(sig.reasoning.contains("150"), "reasoning contains duration");
            }
            _ => panic!("Expected SignalEvent"),
        }
    }

    #[tokio::test]
    async fn test_publish_pipeline_event_subject_per_symbol() {
        let bus = rat_eventbus::InMemoryEventBus::new();
        let mut sub_btc = bus.subscribe("rat.signal.btc").await.unwrap();
        let mut sub_eth = bus.subscribe("rat.signal.eth").await.unwrap();

        publish_pipeline_event(&bus, "BTC", "HOLD", 80.0, false).await;
        publish_pipeline_event(&bus, "ETH", "SELL", 200.0, true).await;

        // BTC event goes to rat.signal.btc
        let (btc_subj, btc_ev) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            sub_btc.recv(),
        )
        .await
        .expect("BTC event should arrive")
        .expect("BTC event should be Some");
        assert_eq!(btc_subj, "rat.signal.btc");
        match btc_ev {
            rat_eventbus::RatEvent::Signal(s) => {
                assert_eq!(s.symbol, "BTC");
                assert_eq!(s.action, "HOLD");
            }
            _ => panic!("Expected SignalEvent"),
        }

        // ETH event goes to rat.signal.eth
        let (eth_subj, eth_ev) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            sub_eth.recv(),
        )
        .await
        .expect("ETH event should arrive")
        .expect("ETH event should be Some");
        assert_eq!(eth_subj, "rat.signal.eth");
        match eth_ev {
            rat_eventbus::RatEvent::Signal(s) => {
                assert_eq!(s.symbol, "ETH");
                assert_eq!(s.action, "SELL");
            }
            _ => panic!("Expected SignalEvent"),
        }
    }

    #[tokio::test]
    async fn test_publish_pipeline_event_confidence_reflects_execution() {
        let bus = rat_eventbus::InMemoryEventBus::new();
        let mut sub = bus.subscribe("rat.signal.*").await.unwrap();

        // Not executed → confidence 0.0
        publish_pipeline_event(&bus, "BTC", "HOLD", 100.0, false).await;
        let (_, ev1) = sub.recv().await.expect("Should receive event");
        match ev1 {
            rat_eventbus::RatEvent::Signal(s) => {
                assert!(
                    (s.confidence - 0.0).abs() < 0.01,
                    "Not executed should have 0.0 confidence"
                );
            }
            _ => panic!("Expected SignalEvent"),
        }

        // Executed → confidence 0.8
        publish_pipeline_event(&bus, "BTC", "BUY", 100.0, true).await;
        let (_, ev2) = sub.recv().await.expect("Should receive event");
        match ev2 {
            rat_eventbus::RatEvent::Signal(s) => {
                assert!(
                    (s.confidence - 0.8).abs() < 0.01,
                    "Executed should have 0.8 confidence"
                );
            }
            _ => panic!("Expected SignalEvent"),
        }
    }

    #[tokio::test]
    async fn test_publish_pipeline_event_reasoning_includes_action_and_duration() {
        let bus = rat_eventbus::InMemoryEventBus::new();
        let mut sub = bus.subscribe("rat.signal.*").await.unwrap();

        publish_pipeline_event(&bus, "ETH", "SELL", 512.0, true).await;

        let (_, event) = sub
            .recv()
            .await
            .expect("Should receive event");
        match event {
            rat_eventbus::RatEvent::Signal(s) => {
                assert!(
                    s.reasoning.contains("SELL"),
                    "Reasoning should mention action: '{}'",
                    s.reasoning
                );
                assert!(
                    s.reasoning.contains("512ms"),
                    "Reasoning should mention duration: '{}'",
                    s.reasoning
                );
            }
            _ => panic!("Expected SignalEvent"),
        }
    }

    // ── SymbolCooldownTracker ──────────────────────────────────────────────

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
        // ETH should still be allowed
        assert!(tracker.is_allowed("ETH"));
        // BTC should be blocked
        assert!(!tracker.is_allowed("BTC"));
    }

    #[test]
    fn test_cooldown_remaining_secs() {
        // Use a large cooldown (3600s) so timing is robust even under load.
        let mut tracker = SymbolCooldownTracker::new(3600);
        // Never run → remaining = 0
        assert_eq!(tracker.remaining_secs("BTC"), 0);

        tracker.record_run("BTC");
        // Just ran → remaining ≈ cooldown (close to 3600)
        let rem = tracker.remaining_secs("BTC");
        assert!(
            rem > 3590,
            "Should have nearly full cooldown remaining, got {}",
            rem
        );
        assert!(
            rem <= 3600,
            "Should not exceed cooldown, got {}",
            rem
        );
    }

    #[test]
    fn test_cooldown_resets_after_zero_second_cooldown() {
        let mut tracker = SymbolCooldownTracker::new(0);
        // Zero cooldown → always allowed
        assert!(tracker.is_allowed("BTC"));
        tracker.record_run("BTC");
        // Zero cooldown → still allowed
        assert!(tracker.is_allowed("BTC"));
    }

    #[test]
    fn test_empty_cooldown_tracker_returns_zero_remaining() {
        let tracker = SymbolCooldownTracker::new(300);
        // No symbols recorded at all
        assert_eq!(tracker.remaining_secs("NONEXISTENT"), 0);
        assert!(tracker.is_allowed("NONEXISTENT"));
    }
}
