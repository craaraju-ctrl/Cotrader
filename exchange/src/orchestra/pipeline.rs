use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use tokio::sync::broadcast;

use crate::engine::ExchangeEngine;
use crate::rat::stream;
use crate::rat::types::RatEvent;

use super::error::{execute_fallback, FallbackStrategy, OrchestraError, OrchestraResult};
use super::types::*;

/// Shared state that persists across pipeline cycles.
pub struct PipelineContext {
    /// Last known good data points per symbol
    pub last_good: HashMap<String, DataPoint>,
    /// Cooldown tracker: symbol -> last trade timestamp
    pub cooldowns: HashMap<String, chrono::DateTime<Utc>>,
    /// Accumulated error count
    pub error_count: u64,
    /// Fallback activation count
    pub fallback_count: u64,
    /// Pipeline metrics
    pub metrics: PipelineMetrics,
    /// Whether agent is in safe-hold mode
    pub safe_hold: bool,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            last_good: HashMap::new(),
            cooldowns: HashMap::new(),
            error_count: 0,
            fallback_count: 0,
            metrics: PipelineMetrics {
                total_data_points: 0,
                signals_generated: 0,
                decisions_executed: 0,
                errors: 0,
                fallbacks_activated: 0,
                avg_processing_ms: 0.0,
                last_cycle: None,
            },
            safe_hold: false,
        }
    }
}

// ── Pipeline Stages ───────────────────────────────────────

/// Stage 1: Ingest — accept raw RatEvent from the broadcast channel
/// and convert to a normalized DataPoint.
pub fn stage_ingest(event: &RatEvent) -> OrchestraResult<DataPoint> {
    match event {
        RatEvent::OrderbookSnapshot(snap) => {
            let best_bid = snap.bids.first().map(|b| b[0]);
            let best_ask = snap.asks.first().map(|a| a[0]);
            let mid = match (best_bid, best_ask) {
                (Some(b), Some(a)) => Some((b + a) / 2.0),
                _ => None,
            };
            let bid_depth: f64 = snap.bids.iter().map(|b| b[0] * b[1]).sum();
            let ask_depth: f64 = snap.asks.iter().map(|a| a[0] * a[1]).sum();

            Ok(DataPoint {
                symbol: snap.symbol.clone(),
                timestamp: snap.timestamp,
                bid: best_bid,
                ask: best_ask,
                mid,
                spread: match (best_ask, best_bid) {
                    (Some(a), Some(b)) => Some(a - b),
                    _ => None,
                },
                bid_depth,
                ask_depth,
                last_price: mid,
                funding_rate: None,
            })
        }
        RatEvent::FundingTick(tick) => {
            let mid = tick.mark_price;
            Ok(DataPoint {
                symbol: tick.symbol.clone(),
                timestamp: tick.timestamp,
                bid: None,
                ask: None,
                mid: Some(mid),
                spread: None,
                bid_depth: 0.0,
                ask_depth: 0.0,
                last_price: Some(mid),
                funding_rate: Some(tick.funding_rate),
            })
        }
        RatEvent::TradeExecution(trade) => {
            Ok(DataPoint {
                symbol: trade.symbol.clone(),
                timestamp: trade.timestamp,
                bid: None,
                ask: None,
                mid: Some(trade.price),
                spread: None,
                bid_depth: 0.0,
                ask_depth: 0.0,
                last_price: Some(trade.price),
                funding_rate: None,
            })
        }
        RatEvent::Diagnostic(d) => {
            return Err(OrchestraError::MalformedData {
                symbol: "diagnostic".into(),
                detail: d.message.clone(),
            });
        }
        RatEvent::Heartbeat(_)
        | RatEvent::BalanceUpdate(_)
        | RatEvent::PositionChange(_)
        | RatEvent::AgentDecision(_) => {
            return Err(OrchestraError::MalformedData {
                symbol: "non-market".into(),
                detail: format!("{:?} is not a market data event", event),
            });
        }
    }
}

/// Stage 2: Normalize — fill in missing fields and validate boundaries.
pub fn stage_normalize(dp: DataPoint, ctx: &PipelineContext) -> OrchestraResult<DataPoint> {
    // Validate price bounds (sanity check)
    if let Some(mid) = dp.mid {
        if mid <= 0.0 || mid > 1_000_000.0 {
            return Err(OrchestraError::MalformedData {
                symbol: dp.symbol.clone(),
                detail: format!("Price out of bounds: {}", mid),
            });
        }
    }

    // Fill missing bid/ask from last known good if available
    let mut normalized = dp.clone();
    if normalized.bid.is_none() || normalized.ask.is_none() {
        if let Some(last) = ctx.last_good.get(&dp.symbol) {
            if normalized.bid.is_none() {
                normalized.bid = last.bid;
            }
            if normalized.ask.is_none() {
                normalized.ask = last.ask;
            }
            if normalized.mid.is_none() {
                normalized.mid = last.mid;
            }
        }
    }

    Ok(normalized)
}

/// Stage 3: Analyze — generate trading signals from normalized data.
/// Uses simple spread/volume/price-action heuristics as a baseline.
/// This is extensible to plug in ML models or more sophisticated strategies.
pub fn stage_analyze(dp: &DataPoint, ctx: &PipelineContext) -> OrchestraResult<Vec<TradeSignal>> {
    let mut signals = Vec::new();
    let mut indicators = Vec::new();

    // ── Spread analysis ──
    if let (Some(bid), Some(ask)) = (dp.bid, dp.ask) {
        let spread_pct = if bid > 0.0 { ((ask - bid) / bid) * 100.0 } else { 0.0 };
        let spread_signal = if spread_pct > 0.5 {
            "bearish"
        } else if spread_pct < 0.05 {
            "bullish"
        } else {
            "neutral"
        };
        indicators.push(Indicator {
            name: "spread_pct".into(),
            value: spread_pct,
            signal: spread_signal.into(),
        });
    }

    // ── Depth imbalance analysis ──
    let total_depth = dp.bid_depth + dp.ask_depth;
    if total_depth > 0.0 {
        let bid_ratio = dp.bid_depth / total_depth;
        let depth_signal = if bid_ratio > 0.65 {
            "bullish"
        } else if bid_ratio < 0.35 {
            "bearish"
        } else {
            "neutral"
        };
        indicators.push(Indicator {
            name: "bid_depth_ratio".into(),
            value: bid_ratio,
            signal: depth_signal.into(),
        });
    }

    // ── Funding rate analysis ──
    if let Some(fr) = dp.funding_rate {
        let fr_signal = if fr > 0.001 {
            "bearish"  // longs paying high funding -> potential short squeeze incoming
        } else if fr < -0.001 {
            "bullish"  // shorts paying high funding -> potential long squeeze
        } else {
            "neutral"
        };
        indicators.push(Indicator {
            name: "funding_rate".into(),
            value: fr,
            signal: fr_signal.into(),
        });
    }

    // ── Price momentum (last known good comparison) ──
    if let Some(mid) = dp.mid {
        if let Some(last) = ctx.last_good.get(&dp.symbol) {
            if let Some(last_mid) = last.mid {
                let change_pct = ((mid - last_mid) / last_mid) * 100.0;
                let momentum_signal = if change_pct > 1.0 {
                    "bullish"
                } else if change_pct < -1.0 {
                    "bearish"
                } else {
                    "neutral"
                };
                indicators.push(Indicator {
                    name: "momentum_pct".into(),
                    value: change_pct,
                    signal: momentum_signal.into(),
                });
            }
        }
    }

    // ── Compute overall signal ──
    let bullish_count = indicators.iter().filter(|i| i.signal == "bullish").count();
    let bearish_count = indicators.iter().filter(|i| i.signal == "bearish").count();
    let total = indicators.len() as f64;
    let confidence = if total > 0.0 {
        (bullish_count.max(bearish_count) as f64) / total
    } else {
        0.0
    };

    // Only emit signals when confidence is meaningful
    if confidence >= 0.4 && !indicators.is_empty() {
        let action = if bullish_count > bearish_count {
            SignalAction::EnterLong
        } else if bearish_count > bullish_count {
            SignalAction::EnterShort
        } else {
            SignalAction::Hold
        };

        let reason = format!(
            "{} indicators: {} bullish, {} bearish. Confidence: {:.2}",
            indicators.len(),
            bullish_count,
            bearish_count,
            confidence
        );

        if action != SignalAction::Hold {
            signals.push(TradeSignal {
                id: Uuid::new_v4(),
                symbol: dp.symbol.clone(),
                action,
                confidence,
                reason,
                indicators: indicators.clone(),
                timestamp: Utc::now(),
                data_point: dp.clone(),
            });
        }
    }

    Ok(signals)
}

/// Stage 4: Decide — convert signals into execution decisions
/// (respecting cooldowns, position limits, min confidence).
pub fn stage_decide(
    signals: Vec<TradeSignal>,
    ctx: &PipelineContext,
    config: &AgentConfig,
) -> Vec<ExecutionDecision> {
    let mut decisions = Vec::new();
    let now = Utc::now();

    for signal in signals {
        // Check confidence threshold
        if signal.confidence < config.min_confidence {
            tracing::debug!(
                "[Orchestra] Signal {} below min confidence ({:.2} < {:.2})",
                signal.id, signal.confidence, config.min_confidence
            );
            continue;
        }

        // Check cooldown
        if let Some(last_trade) = ctx.cooldowns.get(&signal.symbol) {
            let elapsed = (now - *last_trade).num_seconds() as u64;
            if elapsed < config.cooldown_seconds {
                let remaining = config.cooldown_seconds - elapsed;
                tracing::debug!(
                    "[Orchestra] Cooldown active for {} ({}s remaining)",
                    signal.symbol, remaining
                );
                continue;
            }
        }

        let price = signal.data_point.mid;
        let quantity = config.max_position_size.min(signal.data_point.ask_depth * 0.1).max(0.001);

        decisions.push(ExecutionDecision {
            signal_id: signal.id,
            action: signal.action,
            symbol: signal.symbol.clone(),
            quantity,
            price,
            confidence: signal.confidence,
            reason: signal.reason.clone(),
            pre_checked: false, // will be set after memory cross-ref
            timestamp: Utc::now(),
        });
    }

    decisions
}

// ── Pipeline Orchestrator ─────────────────────────────────

/// Run the full pipeline on a single event. Returns execution decisions.
/// Each stage is wrapped in error handling with the appropriate fallback.
pub async fn run_pipeline(
    event: &RatEvent,
    ctx: &Arc<RwLock<PipelineContext>>,
    config: &AgentConfig,
    _engine: &ExchangeEngine,
    rat_tx: Option<&broadcast::Sender<RatEvent>>,
) -> Vec<ExecutionDecision> {
    let start = Instant::now();
    let mut ctx_lock = ctx.write().await;

    // ── Stage 1: Ingest ──
    let dp = match stage_ingest(event) {
        Ok(dp) => {
            ctx_lock.metrics.total_data_points += 1;
            dp
        }
        Err(e) => {
            let fb = e.fallback_strategy();
            ctx_lock.error_count += 1;
            ctx_lock.metrics.errors += 1;
            ctx_lock.metrics.fallbacks_activated += 1;
            execute_fallback(fb, &e);
            if fb == FallbackStrategy::ShutdownAgent {
                ctx_lock.safe_hold = true;
            }
            return Vec::new();
        }
    };

    // ── Stage 2: Normalize ──
    let dp = match stage_normalize(dp, &ctx_lock) {
        Ok(dp) => dp,
        Err(e) => {
            let fb = e.fallback_strategy();
            ctx_lock.error_count += 1;
            ctx_lock.metrics.errors += 1;
            ctx_lock.fallback_count += 1;
            ctx_lock.metrics.fallbacks_activated += 1;
            execute_fallback(fb, &e);
            return Vec::new();
        }
    };

    // Store as last known good (populate even if analysis fails later)
    ctx_lock.last_good.insert(dp.symbol.clone(), dp.clone());

    // Check safe-hold — no analysis in safe-hold mode
    if ctx_lock.safe_hold {
        tracing::warn!("[Orchestra] Safe-hold active for {}", dp.symbol);
        return Vec::new();
    }

    // ── Stage 3: Analyze ──
    let signals = match stage_analyze(&dp, &ctx_lock) {
        Ok(s) => {
            ctx_lock.metrics.signals_generated += s.len() as u64;
            s
        }
        Err(e) => {
            let fb = e.fallback_strategy();
            ctx_lock.error_count += 1;
            ctx_lock.metrics.errors += 1;
            ctx_lock.fallback_count += 1;
            ctx_lock.metrics.fallbacks_activated += 1;
            execute_fallback(fb, &e);
            return Vec::new();
        }
    };

    if signals.is_empty() {
        ctx_lock.metrics.last_cycle = Some(Utc::now());
        return Vec::new();
    }

    // ── Stage 4: Decide ──
    let decisions = stage_decide(signals, &ctx_lock, config);
    ctx_lock.metrics.decisions_executed += decisions.len() as u64;

    // Update cooldowns for symbols with decisions
    for d in &decisions {
        ctx_lock.cooldowns.insert(d.symbol.clone(), Utc::now());
    }

    // Update metrics
    let elapsed = start.elapsed().as_millis() as f64;
    ctx_lock.metrics.avg_processing_ms = if ctx_lock.metrics.total_data_points > 0 {
        (ctx_lock.metrics.avg_processing_ms * (ctx_lock.metrics.total_data_points as f64 - 1.0)
            + elapsed)
            / ctx_lock.metrics.total_data_points as f64
    } else {
        0.0
    };
    ctx_lock.metrics.last_cycle = Some(Utc::now());

    // Broadcast via diagnostics for any generated decisions
    if !decisions.is_empty() {
        for d in &decisions {
            tracing::info!(
                "[Orchestra] Decision: {:?} {} @ {:.2} (conf={:.2})",
                d.action, d.symbol, d.price.unwrap_or(0.0), d.confidence
            );
            if let Some(tx) = rat_tx {
                stream::broadcast_rat_diagnostic(
                    tx,
                    "info",
                    &format!("Decision: {} {} qty={:.4} conf={:.2}", d.action, d.symbol, d.quantity, d.confidence),
                    "orchestra::pipeline",
                );
            }
        }
    }

    decisions
}
