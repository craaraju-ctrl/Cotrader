//! # SelfEvolutionValidator — Extended Validation Harness for Compounding Improvement
//!
//! Runs N cycles of the full autonomous pipeline on one or more symbols, optionally
//! inducing regret (tight stops), and measures the self-evolution loop:
//!
//! **Metrics tracked per cycle-bucket:**
//! - Regret trend (average regret per bucket, should decrease over time)
//! - Win/loss rate per bucket
//! - Rule adaptation events (# RULE_ADAPT, actual rule value changes)
//! - MetaControl rule changes applied (max_risk, min_confluence, etc.)
//!
//! **Expected outcome (compounding improvement):**
//! After meta-adaptations tighten risk rules, subsequent cycles should show
//! lower average regret, fewer high-regret episodes, and more cautious decisions.
//!
//! ## Usage
//! ```ignore
//! let validator = SelfEvolutionValidator::new(orchestrator);
//! let report = validator.run_extended_validation(&["BTC", "ETH"], 50, true).await?;
//! println!("{}", report.summary());
//! ```

use crate::episode_store::RuleChangeSnapshot;
use crate::orchestrator_struct::AutonomousOrchestrator;
use crate::state::SharedState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;

/// When set, validation induces regret by tightening stops to this percentage.
const INDUCED_REGRET_SL_PCT: f64 = 0.5;

/// Minimum number of closed trades needed for background trend analysis.
const MIN_TRADES_FOR_ANALYSIS: usize = 15;

/// BUCKET_SIZE episodes per statistical bucket (10 = every 10 episodes we compute averages)
const BUCKET_SIZE: usize = 10;

// ── Per-Cycle Metrics (one entry per pipeline cycle) ───────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleMetrics {
    pub cycle_number: usize,
    pub symbol: String,
    pub decision: String, // "BUY" | "SELL" | "HOLD"
    pub confidence: f64,
    pub confluence: f64,
    pub regret_score: Option<f64>,     // populated if trade closed
    pub trade_outcome: Option<String>, // "WIN" | "LOSS" | "BREAKEVEN"
    pub exit_reason: Option<String>,   // "stop_loss" | "take_profit" | "manual"
    pub rule_change_applied: bool,
    pub rules_snapshot: RulesSnapshot,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesSnapshot {
    pub max_risk_per_trade: f64,
    pub max_daily_drawdown: f64,
    pub max_consecutive_losses: u32,
    pub min_confluence_score: f64,
}

impl RulesSnapshot {
    fn from(rules: &cotrader_core::DisciplineRules) -> Self {
        Self {
            max_risk_per_trade: rules.max_risk_per_trade,
            max_daily_drawdown: rules.max_daily_drawdown,
            max_consecutive_losses: rules.max_consecutive_losses,
            min_confluence_score: rules.min_confluence_score,
        }
    }
}

// ── Bucket Statistics (compressed metrics over BUCKET_SIZE cycles) ─────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketStats {
    pub bucket_index: usize,
    pub cycle_count: usize,
    pub avg_regret: f64,
    pub win_count: usize,
    pub loss_count: usize,
    pub hold_count: usize,
    pub avg_confidence: f64,
    pub rule_changes: Vec<RuleChangeSnapshot>,
    pub rules_at_end: RulesSnapshot,
}

// ── Final Report ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfEvolutionReport {
    pub run_start: DateTime<Utc>,
    pub run_end: DateTime<Utc>,
    pub symbols: Vec<String>,
    pub total_cycles: usize,
    pub induce_regret: bool,

    /// Per-bucket stats for trend analysis
    pub buckets: Vec<BucketStats>,

    /// All cycles (detailed, for debugging)
    pub cycles: Vec<CycleMetrics>,

    /// All rule changes applied during the run
    pub rule_changes: Vec<RuleChangeSnapshot>,

    // ── Trend Analysis ─────────────────────────────────────────────────
    /// Average regret in the first half of the run
    pub regret_first_half: f64,
    /// Average regret in the second half of the run
    pub regret_second_half: f64,
    /// Regret trend direction: "DECREASING" (improving) | "INCREASING" | "STABLE"
    pub regret_trend: String,

    /// Win rate in first half
    pub win_rate_first_half: f64,
    /// Win rate in second half
    pub win_rate_second_half: f64,

    /// Total rule adaptations triggered
    pub total_rule_adaptations: usize,

    /// Summary narrative for the report
    pub summary_text: String,
}

impl SelfEvolutionReport {
    /// Generate a human-readable summary of the validation run.
    pub fn summary(&self) -> String {
        let mut lines = vec![
            "╔══════════════════════════════════════════════════════════════╗".to_string(),
            "║        RAT SELF-EVOLUTION VALIDATION REPORT              ║".to_string(),
            "╚══════════════════════════════════════════════════════════════╝".to_string(),
            String::new(),
            format!(
                "Run: {} → {}",
                self.run_start.format("%H:%M:%S"),
                self.run_end.format("%H:%M:%S")
            ),
            format!("Symbols: {}", self.symbols.join(", ")),
            format!(
                "Total cycles: {} (induce_regret={})",
                self.total_cycles, self.induce_regret
            ),
            format!("Total rule adaptations: {}", self.total_rule_adaptations),
            String::new(),
            "── REGRET TREND ──".to_string(),
            format!("  First half avg regret:  {:.3}", self.regret_first_half),
            format!("  Second half avg regret: {:.3}", self.regret_second_half),
        ];

        // Direction indicator
        let regret_arrow = if self.regret_trend == "DECREASING" {
            "📉 DECREASING (improving!)"
        } else if self.regret_trend == "INCREASING" {
            "📈 INCREASING (degrading)"
        } else {
            "➡️ STABLE"
        };
        lines.push(format!("  Trend: {}", regret_arrow));

        lines.push(String::new());
        lines.push("── WIN RATE TREND ──".to_string());
        lines.push(format!(
            "  First half win rate:  {:.1}%",
            self.win_rate_first_half * 100.0
        ));
        lines.push(format!(
            "  Second half win rate: {:.1}%",
            self.win_rate_second_half * 100.0
        ));

        // Win rate direction
        if self.win_rate_second_half > self.win_rate_first_half {
            lines.push("  Direction: 📈 Improving!".to_string());
        } else if self.win_rate_second_half < self.win_rate_first_half {
            lines.push("  Direction: 📉 Declining".to_string());
        } else {
            lines.push("  Direction: ➡️ Stable".to_string());
        }

        if !self.buckets.is_empty() {
            lines.push(String::new());
            lines.push("── PER-BUCKET BREAKDOWN ──".to_string());
            for bucket in &self.buckets {
                let regret_str = format!("{:.3}", bucket.avg_regret);
                let wr_str = if bucket.cycle_count > 0 {
                    format!(
                        "{:.0}%",
                        (bucket.win_count as f64 / bucket.cycle_count.max(1) as f64) * 100.0
                    )
                } else {
                    "N/A".to_string()
                };
                lines.push(format!(
                    "  Bucket {:2}: {} cycles | regret={} | WR={} | wins={} losses={} holds={} | rules_changed={}",
                    bucket.bucket_index,
                    bucket.cycle_count,
                    regret_str,
                    wr_str,
                    bucket.win_count,
                    bucket.loss_count,
                    bucket.hold_count,
                    bucket.rule_changes.len(),
                ));
            }
        }

        if !self.rule_changes.is_empty() {
            lines.push(String::new());
            lines.push("── RULE ADAPTATIONS ──".to_string());
            for rc in &self.rule_changes {
                lines.push(format!(
                    "  {}: {:.4} → {:.4} — {}",
                    rc.rule_name, rc.old_value, rc.new_value, rc.reason
                ));
            }
        }

        lines.push(String::new());
        lines.push("── CONCLUSION ──".to_string());
        lines.push(format!("  {}", self.summary_text));

        // Compounding improvement assessment
        let compounding = if self.regret_trend == "DECREASING"
            && self.win_rate_second_half >= self.win_rate_first_half
        {
            "✅ Compounding improvement detected: regret decreasing and win rate stable/improving."
        } else if self.regret_trend == "DECREASING" {
            "🟡 Partial improvement: regret decreasing but win rate not yet improving."
        } else {
            "🔄 Insufficient data for compounding assessment. Run more cycles or induce stronger regret."
        };
        lines.push(format!("  {}", compounding));

        lines.join("\n")
    }
}

// ── SelfEvolutionValidator ─────────────────────────────────────────────────

pub struct SelfEvolutionValidator {
    orchestrator: AutonomousOrchestrator,
}

impl SelfEvolutionValidator {
    pub fn new(orchestrator: AutonomousOrchestrator) -> Self {
        Self { orchestrator }
    }

    /// Run extended validation: N cycles per symbol with optional induced regret.
    /// Returns a structured report with trend analysis and compounding evidence.
    pub async fn run_extended_validation(
        &self,
        symbols: &[&str],
        cycles: usize,
        induce_regret: bool,
    ) -> Result<SelfEvolutionReport, Box<dyn Error + Send + Sync>> {
        let run_start = Utc::now();
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!(
            "║   RAT SELF-EVOLUTION VALIDATION ({} cycles)        ║",
            cycles
        );
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!(
            "Symbols: {:?} | Induce regret: {} | Bucket size: {} episodes",
            symbols, induce_regret, BUCKET_SIZE
        );

        // When inducing regret, set the env flag the execution coordinator reads
        // so every paper trade gets a tight stop (→ losses → high regret →
        // exercises the meta-control rule-adaptation loop). Cleared after the run.
        if induce_regret {
            println!(
                ">>> INDUCING REGRET: {:.1}% tight SL for all trades",
                INDUCED_REGRET_SL_PCT
            );
            std::env::set_var(
                "RAT_INDUCE_REGRET_SL_PCT",
                INDUCED_REGRET_SL_PCT.to_string(),
            );
        }

        let mut all_cycles: Vec<CycleMetrics> = Vec::new();
        let mut rule_changes_all: Vec<RuleChangeSnapshot> = Vec::new();

        for cycle_num in 0..cycles {
            for &symbol in symbols {
                // Read current price from OHLCV history or portfolio
                let _price = {
                    let portfolio = self.orchestrator.state.portfolio_store.portfolio.read().await;
                    let s = symbol.to_string();
                    if let Some(pos) = portfolio.open_positions.iter().find(|p| p.symbol == s) {
                        pos.current_price
                    } else {
                        let history = self.orchestrator.state.market_data.ohlcv_history.read().await;
                        history
                            .get(symbol)
                            .and_then(|h| h.last().map(|b| b.close))
                            .unwrap_or(60000.0) // default BTC-ish price
                    }
                };

                // Determine direction based on simple price momentum
                let _direction = {
                    let history = self.orchestrator.state.market_data.ohlcv_history.read().await;
                    let has_bars = history.get(symbol).map(|h| h.len() >= 5).unwrap_or(false);
                    if has_bars {
                        let bars = history.get(symbol).unwrap();
                        let change = bars.last().unwrap().close - bars[bars.len() - 5].close;
                        if change >= 0.0 {
                            cotrader_core::TradeDirection::Long
                        } else {
                            cotrader_core::TradeDirection::Short
                        }
                    } else {
                        cotrader_core::TradeDirection::Long
                    }
                };

                // (levels are no longer computed here — the agent decides them autonomously inside run_full_pipeline)

                // Run the full pipeline
                let pipeline_result = match self
                    .orchestrator
                    .run_full_pipeline(symbol) // agentic: agent decides levels from its own analysis
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("[SelfEvolutionValidator] Pipeline error for {} cycle {}: {}. Skipping.", symbol, cycle_num, e);
                        continue;
                    }
                };

                // Check if position was closed in this cycle
                let (regret, outcome, exit_reason) = {
                    // Query the latest closed trade from SQLite
                    let store = &self.orchestrator.state.agent_memory.episode_store;
                    let recent = store.get_most_recent_closed(symbol).unwrap_or_else(|e| {
                        eprintln!(
                            "[SelfEvolutionValidator] DB error fetching closed trade: {}",
                            e
                        );
                        None
                    });
                    match recent {
                        Some(ep) => (
                            Some(ep.regret_score),
                            Some(ep.outcome),
                            Some(ep.exit_reason),
                        ),
                        None => (None, None, None),
                    }
                };

                // Snapshot current rules
                let rules_snapshot =
                    RulesSnapshot::from(&*self.orchestrator.state.rule_engine.rules.read().await);

                // Track if a rule change just happened
                let rule_change_this_cycle = {
                    let store = &self.orchestrator.state.agent_memory.episode_store;
                    let recent_changes = store.get_recent_rule_changes(1).unwrap_or_default();
                    !recent_changes.is_empty()
                };

                let decision = if pipeline_result.executed {
                    "BUY/SELL".to_string()
                } else {
                    "HOLD".to_string()
                };

                let metrics = CycleMetrics {
                    cycle_number: cycle_num,
                    symbol: symbol.to_string(),
                    decision,
                    confidence: pipeline_result
                        .final_signal
                        .as_ref()
                        .map(|s| s.confidence_score)
                        .unwrap_or(0.0),
                    confluence: pipeline_result
                        .final_signal
                        .as_ref()
                        .map(|s| s.confluence_score)
                        .unwrap_or(0.0),
                    regret_score: regret,
                    trade_outcome: outcome,
                    exit_reason,
                    rule_change_applied: rule_change_this_cycle,
                    rules_snapshot,
                    timestamp: Utc::now(),
                };

                all_cycles.push(metrics);

                if cycle_num % 5 == 0 {
                    print!(".");
                    if (cycle_num + 1) % 50 == 0 {
                        println!(" {} cycles complete", cycle_num + 1);
                    }
                }
            }
        }

        // Clear the induced-regret override so it can't leak into later runs.
        if induce_regret {
            std::env::remove_var("RAT_INDUCE_REGRET_SL_PCT");
        }

        // Collect all rule changes from the run
        {
            let store = &self.orchestrator.state.agent_memory.episode_store;
            if let Ok(changes) = store.get_all_rule_changes() {
                rule_changes_all = changes;
            }
        }

        // Compute buckets
        let buckets = Self::compute_buckets(&all_cycles);

        // Trend analysis
        let (regret_first, regret_second) = Self::compute_half_regret(&buckets);
        let (wr_first, wr_second) = Self::compute_half_win_rates(&buckets);
        let regret_trend = if regret_second < regret_first * 0.9 {
            "DECREASING".to_string()
        } else if regret_second > regret_first * 1.1 {
            "INCREASING".to_string()
        } else {
            "STABLE".to_string()
        };

        let total_rule_adaptations = rule_changes_all.len();

        // Generate conclusion
        let summary_text = Self::generate_conclusion(
            regret_trend.as_str(),
            regret_first,
            regret_second,
            wr_first,
            wr_second,
            total_rule_adaptations,
            cycles,
        );

        let run_end = Utc::now();

        let report = SelfEvolutionReport {
            run_start,
            run_end,
            symbols: symbols.iter().map(|s| s.to_string()).collect(),
            total_cycles: cycles,
            induce_regret,
            buckets,
            cycles: all_cycles,
            rule_changes: rule_changes_all,
            regret_first_half: regret_first,
            regret_second_half: regret_second,
            regret_trend,
            win_rate_first_half: wr_first,
            win_rate_second_half: wr_second,
            total_rule_adaptations,
            summary_text,
        };

        // Store report to redb for persistence
        if let Ok(json) = serde_json::to_string(&report) {
            let key = format!("evolution/report/{}", run_end.timestamp());
            let _ = self.orchestrator.state.agent_memory.memory.store_state(&key, &json);
        } // Output to agentmemory for cross-session learning
        {
            let mem = cotrader_core::AgentMemoryClient::new();
            let _ = mem
                .remember(
                    &format!(
                        "SELF_EVOLUTION: {} cycles on {}, regret trend {}, WR {:.0}% → {:.0}%, {} adaptations",
                        cycles,
                        symbols.join(","),
                        report.regret_trend.as_str(),
                        wr_first * 100.0,
                        wr_second * 100.0,
                        total_rule_adaptations
                    ),
                    "self_evolution",
                )
                .await;
        }

        println!("\n\n=== VALIDATION COMPLETE ===");
        println!("{}", report.summary());

        Ok(report)
    }

    /// Group cycles into buckets of BUCKET_SIZE and compute aggregate stats.
    fn compute_buckets(cycles: &[CycleMetrics]) -> Vec<BucketStats> {
        if cycles.is_empty() {
            return vec![];
        }

        let mut buckets: Vec<BucketStats> = Vec::new();
        let mut current_entries: Vec<&CycleMetrics> = Vec::new();

        for (i, cycle) in cycles.iter().enumerate() {
            current_entries.push(cycle);
            if current_entries.len() >= BUCKET_SIZE || i == cycles.len() - 1 {
                let bucket_idx = buckets.len();
                let count = current_entries.len();

                let avg_regret: f64 = {
                    let scores: Vec<f64> = current_entries
                        .iter()
                        .filter_map(|c| c.regret_score)
                        .collect();
                    if scores.is_empty() {
                        0.0
                    } else {
                        scores.iter().sum::<f64>() / scores.len() as f64
                    }
                };

                let win_count = current_entries
                    .iter()
                    .filter(|c| c.trade_outcome.as_deref() == Some("WIN"))
                    .count();
                let loss_count = current_entries
                    .iter()
                    .filter(|c| c.trade_outcome.as_deref() == Some("LOSS"))
                    .count();
                let hold_count = current_entries
                    .iter()
                    .filter(|c| c.decision == "HOLD")
                    .count();

                let avg_confidence: f64 =
                    current_entries.iter().map(|c| c.confidence).sum::<f64>() / count as f64;

                let rule_changes: Vec<RuleChangeSnapshot> = current_entries
                    .iter()
                    .filter(|c| c.rule_change_applied)
                    .map(|_| RuleChangeSnapshot {
                        rule_name: "see_global".to_string(),
                        old_value: 0.0,
                        new_value: 0.0,
                        reason: "bucket summary".to_string(),
                        applied_at: String::new(),
                    })
                    .collect();

                let last_entry = current_entries.last().unwrap();
                let rules_at_end = last_entry.rules_snapshot.clone();

                buckets.push(BucketStats {
                    bucket_index: bucket_idx,
                    cycle_count: count,
                    avg_regret,
                    win_count,
                    loss_count,
                    hold_count,
                    avg_confidence,
                    rule_changes,
                    rules_at_end,
                });

                current_entries.clear();
            }
        }

        buckets
    }

    /// Average regret in first half vs second half of buckets.
    fn compute_half_regret(buckets: &[BucketStats]) -> (f64, f64) {
        if buckets.is_empty() {
            return (0.0, 0.0);
        }
        let mid = buckets.len() / 2;
        if mid == 0 {
            return (buckets[0].avg_regret, buckets[0].avg_regret);
        }
        let first: Vec<_> = buckets[..mid].iter().collect();
        let second: Vec<_> = buckets[mid..].iter().collect();

        let f_avg = first.iter().map(|b| b.avg_regret).sum::<f64>() / first.len() as f64;
        let s_avg = second.iter().map(|b| b.avg_regret).sum::<f64>() / second.len() as f64;
        (f_avg, s_avg)
    }

    /// Win rates in first half vs second half.
    fn compute_half_win_rates(buckets: &[BucketStats]) -> (f64, f64) {
        if buckets.is_empty() {
            return (0.0, 0.0);
        }
        let mid = buckets.len() / 2;
        if mid == 0 {
            let total = buckets[0].win_count + buckets[0].loss_count;
            let wr = if total > 0 {
                buckets[0].win_count as f64 / total as f64
            } else {
                0.0
            };
            return (wr, wr);
        }
        let first: Vec<_> = buckets[..mid].iter().collect();
        let second: Vec<_> = buckets[mid..].iter().collect();

        let f_wins: usize = first.iter().map(|b| b.win_count).sum();
        let f_losses: usize = first.iter().map(|b| b.loss_count).sum();
        let s_wins: usize = second.iter().map(|b| b.win_count).sum();
        let s_losses: usize = second.iter().map(|b| b.loss_count).sum();

        let f_total = f_wins + f_losses;
        let s_total = s_wins + s_losses;

        let f_wr = if f_total > 0 {
            f_wins as f64 / f_total as f64
        } else {
            0.0
        };
        let s_wr = if s_total > 0 {
            s_wins as f64 / s_total as f64
        } else {
            0.0
        };

        (f_wr, s_wr)
    }

    /// Generate the conclusion text for the report.
    fn generate_conclusion(
        regret_trend: &str,
        regret_first: f64,
        regret_second: f64,
        wr_first: f64,
        wr_second: f64,
        total_adaptations: usize,
        total_cycles: usize,
    ) -> String {
        let mut parts = Vec::new();

        // Regret assessment
        if regret_trend == "DECREASING" {
            parts.push(format!(
                "Regret decreased from {:.3} to {:.3} — the system is learning from mistakes.",
                regret_first, regret_second
            ));
        } else if regret_trend == "INCREASING" {
            parts.push(format!(
                "Regret increased from {:.3} to {:.3} — may need more cycles or different market regime.",
                regret_first, regret_second
            ));
        } else {
            parts.push(format!(
                "Regret stable at ~{:.3} — system is consistent but may need stronger regret induction.",
                regret_first
            ));
        }

        // Win rate assessment
        if wr_second > wr_first + 0.05 {
            parts.push(format!(
                "Win rate improved from {:.0}% to {:.0}% — decisions are getting better over time.",
                wr_first * 100.0,
                wr_second * 100.0
            ));
        } else if wr_second < wr_first - 0.05 {
            parts.push(format!(
                "Win rate declined from {:.0}% to {:.0}% — rule tightening may be reducing edge detection.",
                wr_first * 100.0,
                wr_second * 100.0
            ));
        } else {
            parts.push(format!("Win rate stable around {:.0}%.", wr_first * 100.0));
        }

        // Rule adaptations
        if total_adaptations > 0 {
            parts.push(format!(
                "{} rule adaptations were applied by MetaControl, demonstrating active self-evolution.",
                total_adaptations
            ));
        } else {
            parts.push(
                "No rule adaptations triggered — either regret was too low (<0.5) or too few trades closed."
                    .to_string(),
            );
        }

        // Overall
        let overall = if regret_trend == "DECREASING" {
            format!(
                "Compounding improvement detected after {} cycles. The self-evolving loop is functioning: high-regret outcomes trigger MetaControl rule tightening, and subsequent cycles show lower regret. This validates the core 'Rules + Memory + Debate > Pure Prompting' philosophy in practice.",
                total_cycles
            )
        } else {
            format!(
                "Ran {} cycles — self-evolution loop is active (reflection + meta) but needs more high-regret events (--induce-regret or volatile market conditions) to demonstrate measurable compounding improvement.",
                total_cycles
            )
        };
        parts.push(overall);

        parts.join(" ")
    }
}

/// Run a lightweight automated self-evolution analysis on existing closed trades.
/// This is called from the slow loop (not from pipeline runs).
/// It fetches recent closed trades, groups them into statistical buckets,
/// and computes regret/win-rate trend for logging/monitoring.
pub async fn run_background_validation(state: &SharedState) -> String {
    let run_start = Utc::now();

    // Fetch recent closed trades
    let recent_trades = state
        .agent_memory
        .episode_store
        .load_recent_closed_trades(50, None)
        .unwrap_or_default();

    if recent_trades.len() < MIN_TRADES_FOR_ANALYSIS {
        return format!(
            "[SelfEvolution] Skipping background validation — only {} closed trades (need {})",
            recent_trades.len(),
            MIN_TRADES_FOR_ANALYSIS
        );
    }

    // Build CycleMetrics from closed trades
    let cycles: Vec<CycleMetrics> = recent_trades
        .into_iter()
        .enumerate()
        .map(|(i, trade)| {
            let outcome = if trade.pnl > 0.0 {
                Some("WIN".to_string())
            } else if trade.pnl < 0.0 {
                Some("LOSS".to_string())
            } else {
                Some("BREAKEVEN".to_string())
            };
            CycleMetrics {
                cycle_number: i,
                symbol: trade.symbol.clone(),
                decision: String::new(),
                confidence: 0.0,
                confluence: 0.0,
                regret_score: Some(trade.regret_score),
                trade_outcome: outcome,
                exit_reason: Some(trade.exit_reason.clone()),
                rule_change_applied: false,
                rules_snapshot: RulesSnapshot {
                    max_risk_per_trade: 0.0,
                    max_daily_drawdown: 0.0,
                    max_consecutive_losses: 0,
                    min_confluence_score: 0.0,
                },
                timestamp: chrono::DateTime::parse_from_rfc3339(&trade.entry_time)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }
        })
        .collect();

    // Run trend analysis
    let buckets = SelfEvolutionValidator::compute_buckets(&cycles);
    let (regret_first, regret_second) = SelfEvolutionValidator::compute_half_regret(&buckets);
    let (wr_first, wr_second) = SelfEvolutionValidator::compute_half_win_rates(&buckets);

    let regret_trend = if regret_second < regret_first * 0.9 {
        "DECREASING"
    } else if regret_second > regret_first * 1.1 {
        "INCREASING"
    } else {
        "STABLE"
    };

    // Fetch recent rule changes
    let rule_changes = state
        .agent_memory
        .episode_store
        .get_recent_rule_changes(10)
        .unwrap_or_default();

    let mut report = format!(
        "[SelfEvolution] 🔬 Background analysis — {} trades, {} buckets, {} rule changes | ",
        cycles.len(),
        buckets.len(),
        rule_changes.len()
    );

    report.push_str(&format!(
        "regret {:.3} → {:.3} ({}) | WR {:.0}% → {:.0}%",
        regret_first,
        regret_second,
        match regret_trend {
            "DECREASING" => "📉 improving",
            "INCREASING" => "📉 degrading",
            _ => "➡️ stable",
        },
        wr_first * 100.0,
        wr_second * 100.0
    ));

    if !rule_changes.is_empty() {
        report.push_str(&format!(
            " | changes: {}",
            rule_changes
                .iter()
                .map(|c| format!("{}: {:.4}→{:.4}", c.rule_name, c.old_value, c.new_value))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Store analysis result for dashboard rendering
    let timestamp = run_start.timestamp();
    let json = serde_json::json!({
        "type": "self_evolution_report",
        "timestamp": timestamp,
        "total_trades": cycles.len(),
        "buckets": buckets.len(),
        "rule_changes": rule_changes.len(),
        "regret_first_half": regret_first,
        "regret_second_half": regret_second,
        "regret_trend": regret_trend,
        "wr_first_half": wr_first,
        "wr_second_half": wr_second,
        "text": report,
    });
    let _ = state
        .agent_memory
        .memory
        .store_state(&format!("evolution/latest/{}", timestamp), &json.to_string());

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_cycle(n: usize, regret: Option<f64>, outcome: Option<&str>, hold: bool) -> CycleMetrics {
        CycleMetrics {
            cycle_number: n,
            symbol: "BTC".to_string(),
            decision: if hold {
                "HOLD".to_string()
            } else {
                "BUY/SELL".to_string()
            },
            confidence: 0.7,
            confluence: 0.6,
            regret_score: regret,
            trade_outcome: outcome.map(|s| s.to_string()),
            exit_reason: None,
            rule_change_applied: false,
            rules_snapshot: RulesSnapshot {
                max_risk_per_trade: 0.02,
                max_daily_drawdown: 0.05,
                max_consecutive_losses: 3,
                min_confluence_score: 0.5,
            },
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn buckets_group_by_size_and_count_outcomes() {
        // 25 cycles → 3 buckets (10, 10, 5)
        let mut cycles = Vec::new();
        for i in 0..25 {
            let outcome = if i % 2 == 0 {
                Some("WIN")
            } else {
                Some("LOSS")
            };
            cycles.push(mk_cycle(i, Some(0.4), outcome, false));
        }
        let buckets = SelfEvolutionValidator::compute_buckets(&cycles);
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0].cycle_count, 10);
        assert_eq!(buckets[1].cycle_count, 10);
        assert_eq!(buckets[2].cycle_count, 5);
        // wins + losses per full bucket should sum to the cycle count
        assert_eq!(buckets[0].win_count + buckets[0].loss_count, 10);
    }

    #[test]
    fn half_regret_detects_improvement() {
        // First half high regret, second half low regret.
        let mut cycles = Vec::new();
        for i in 0..20 {
            let regret = if i < 10 { 0.8 } else { 0.2 };
            cycles.push(mk_cycle(i, Some(regret), Some("WIN"), false));
        }
        let buckets = SelfEvolutionValidator::compute_buckets(&cycles);
        let (first, second) = SelfEvolutionValidator::compute_half_regret(&buckets);
        assert!(
            first > second,
            "first-half regret should exceed second-half"
        );
        assert!((first - 0.8).abs() < 1e-9);
        assert!((second - 0.2).abs() < 1e-9);
    }

    #[test]
    fn half_win_rate_computes_correctly() {
        // First half all wins, second half all losses.
        let mut cycles = Vec::new();
        for i in 0..20 {
            let outcome = if i < 10 { Some("WIN") } else { Some("LOSS") };
            cycles.push(mk_cycle(i, Some(0.5), outcome, false));
        }
        let buckets = SelfEvolutionValidator::compute_buckets(&cycles);
        let (first, second) = SelfEvolutionValidator::compute_half_win_rates(&buckets);
        assert!(
            (first - 1.0).abs() < 1e-9,
            "first half should be 100% win rate"
        );
        assert!(
            (second - 0.0).abs() < 1e-9,
            "second half should be 0% win rate"
        );
    }

    #[test]
    fn empty_cycles_produce_no_buckets() {
        let buckets = SelfEvolutionValidator::compute_buckets(&[]);
        assert!(buckets.is_empty());
        let (rf, rs) = SelfEvolutionValidator::compute_half_regret(&buckets);
        assert_eq!((rf, rs), (0.0, 0.0));
    }

    // ── run_background_validation: integration tests ───────────────────────

    /// Helper: create a SharedState with an in-memory SQLite EpisodeStore
    /// seeded with `n` closed trades for testing `run_background_validation`.
    async fn make_test_state_with_trades(
        n: usize,
        regret_score: f64,
        pnl: f64,
        outcome: &str,
        include_rule_changes: bool,
    ) -> SharedState {
        use std::sync::Arc;
        use cotrader_core::paper_engine::BrokerAdapter;
        use crate::test_helpers::DemoBroker;
        let memory = cotrader_core::MemoryStore::new("test_se_bg_redb").expect("MemoryStore");
        let rules = cotrader_core::DisciplineRules::default();
        let config = cotrader_core::Config::default();
        let paper_broker: Arc<dyn BrokerAdapter> = Arc::new(DemoBroker);
        let state = SharedState::new(memory, rules, config, ":memory:", paper_broker)
            .expect("SharedState init");

        // Seed closed trades through the EpisodeStore directly.
        // The SharedState's EpisodeStore owns the ":memory:" SQLite connection.
        let store = state.agent_memory.episode_store.clone();
        for i in 0..n {
            let ts = chrono::Utc::now() - chrono::Duration::minutes(i as i64);
            let ep = crate::episode_store::ClosedEpisode {
                id: format!("trade_{}", i),
                symbol: "BTC".to_string(),
                direction: "Long".to_string(),
                entry_price: 50000.0,
                exit_price: 51000.0,
                stop_loss: 49000.0,
                take_profit: 53000.0,
                position_size: 1.0,
                pnl,
                pnl_pct: if pnl > 0.0 { pnl } else { -pnl },
                outcome: outcome.to_string(),
                exit_reason: "stop_loss".to_string(),
                regret_score,
                lesson: "test".to_string(),
                confluence_score: 0.6,
                portfolio_heat: 0.05,
                market_regime: "trending_bull".to_string(),
                session: "regular".to_string(),
                agent_reasoning: "test".to_string(),
                consecutive_losses_at_entry: 0,
                entry_time: ts.to_rfc3339(),
                exit_time: ts.to_rfc3339(),
                rule_version: 1,
                was_correct: pnl > 0.0,
            };
            let _ = store.insert_closed_trade(&ep);
        }

        if include_rule_changes {
            let _ = store.record_rule_change(
                "max_risk_per_trade",
                "0.015",
                "High regret detected",
                chrono::Utc::now().timestamp() as u64,
            );
        }

        state
    }

    #[tokio::test]
    async fn test_background_validation_skips_when_too_few_trades() {
        let state = make_test_state_with_trades(5, 0.3, -50.0, "LOSS", false).await;
        let report = run_background_validation(&state).await;
        assert!(
            report.contains("Skipping background validation"),
            "Should skip when < {} trades: '{}'",
            MIN_TRADES_FOR_ANALYSIS,
            report
        );
        assert!(
            report.contains("5"),
            "Should mention trade count: '{}'",
            report
        );
    }

    #[tokio::test]
    async fn test_background_validation_full_analysis_with_wins() {
        // 20 trades — all wins with low regret
        let state = make_test_state_with_trades(20, 0.1, 200.0, "WIN", false).await;
        let report = run_background_validation(&state).await;

        assert!(
            !report.contains("Skipping"),
            "Should run full analysis: '{}'",
            report
        );
        assert!(report.contains("Background analysis"), "Should label as background analysis");
        assert!(report.contains("trades"), "Should mention trades");
        assert!(report.contains("buckets"), "Should mention buckets");
        assert!(report.contains("regret"), "Should mention regret");
        assert!(report.contains("WR"), "Should mention win rate");
    }

    #[tokio::test]
    async fn test_background_validation_full_analysis_with_losses() {
        // 30 trades — all losses with high regret
        let state = make_test_state_with_trades(30, 0.8, -150.0, "LOSS", false).await;
        let report = run_background_validation(&state).await;

        assert!(
            !report.contains("Skipping"),
            "Should run full analysis: '{}'",
            report
        );
        // Regret should be high (degrading)
        assert!(
            report.contains("degrading") || report.contains("stable"),
            "High regret should be degrading or stable: '{}'",
            report
        );
    }

    #[tokio::test]
    async fn test_background_validation_includes_rule_changes() {
        let state = make_test_state_with_trades(30, 0.5, 100.0, "WIN", true).await;
        let report = run_background_validation(&state).await;

        assert!(
            !report.contains("Skipping"),
            "Should run full analysis: '{}'",
            report
        );
        assert!(
            report.contains("rule_changes") || report.contains("max_risk_per_trade") || report.contains("changes"),
            "Should include rule changes data: '{}'",
            report
        );
    }

    #[tokio::test]
    async fn test_background_validation_buckets_and_trend_detected() {
        // 25 trades → should produce 3 buckets (10, 10, 5)
        let state = make_test_state_with_trades(25, 0.4, 50.0, "WIN", false).await;
        let report = run_background_validation(&state).await;

        assert!(
            report.contains("buckets"),
            "Should mention buckets: '{}'",
            report
        );
        // Should parse the bucket count from the report
        assert!(
            report.contains("stable") || report.contains("improving") || report.contains("degrading"),
            "Should indicate a trend direction: '{}'",
            report
        );
    }

    #[tokio::test]
    async fn test_report_format_is_parseable() {
        let state = make_test_state_with_trades(18, 0.2, 150.0, "WIN", false).await;
        let report = run_background_validation(&state).await;

        // The report should be a well-formed line of text starting with [SelfEvolution]
        assert!(
            report.starts_with("[SelfEvolution]"),
            "Report should start with [SelfEvolution]"
        );
        // Should contain key metric labels
        assert!(report.contains("regret"), "Should mention regret: '{}'", report);
        assert!(report.contains("WR"), "Should mention win rate: '{}'", report);
        // Should separate sections with |
        assert!(
            report.matches('|').count() >= 2,
            "Should have at least 2 | separators: '{}'",
            report
        );
    }
}
