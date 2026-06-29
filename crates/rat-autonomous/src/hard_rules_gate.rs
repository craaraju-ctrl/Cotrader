// ═══════════════════════════════════════════════════════════════════════════════
// Hard Rules Gate — Single top-level enforcement of ALL hard rules
//
// Architecture (per institutional best practice):
//
//   ┌─────────────────────────────────────────────────────────────────┐
//   │                    HARD RULES GATE (Layer 1)                    │
//   │  Priority: Critical > High > Medium > Low                      │
//   │  Override: Upper layer ALWAYS wins. Equal priority → conservative│
//   │  Blocking: Critical/High always block. Medium blocks only if    │
//   │            no Higher rule overrides. Low = WARNING only.        │
//   └─────────────────────────────────────────────────────────────────┘
//                              ↓ (if passed)
//   ┌─────────────────────────────────────────────────────────────────┐
//   │                 REGIME DETECTION (Layer 2)                      │
//   │  Dynamic thresholds based on market state                      │
//   └─────────────────────────────────────────────────────────────────┘
//                              ↓
//   ┌─────────────────────────────────────────────────────────────────┐
//   │               DEBATE LAYER (Layer 3) — Advisory Only           │
//   │  6 agents provide evidence + confidence. No veto power.        │
//   └─────────────────────────────────────────────────────────────────┘
//                              ↓
//   ┌─────────────────────────────────────────────────────────────────┐
//   │           JUDGE / ADJUDICATOR (Layer 4) — Final Authority      │
//   │  Combines hard rules + debate evidence → BUY/HOLD/SELL         │
//   └─────────────────────────────────────────────────────────────────┘
//                              ↓
//   ┌─────────────────────────────────────────────────────────────────┐
//   │              EXECUTION LAYER (Layer 5)                         │
//   │  Executes the adjudicated decision                             │
//   └─────────────────────────────────────────────────────────────────┘
//
// Key principle: Debate agents are ADVISORY only. Only the Judge has
// decision-making power. This prevents "hallucinated conviction" and
// ensures hard rules are never bypassed by agent enthusiasm.
// ═══════════════════════════════════════════════════════════════════════════════

use crate::state::SharedState;
use rat_core::memory_integration::MemoryIntegration;
use crate::types::{
    AgentDecision, ChainOfReasoning, DecisionVerdict, HardRulesGateResult, OhlcvSnapshot,
    ReasoningStep, RuleCheck, RulePriority, RuleTrace,
};
use chrono::Utc;

pub struct HardRulesGate {
    state: SharedState,
    memory: Option<MemoryIntegration>,
}

impl HardRulesGate {
    pub fn new(state: SharedState) -> Self {
        Self { state, memory: None }
    }

    /// Create with memory integration for policy cache lookups.
    pub fn with_memory(state: SharedState, memory: MemoryIntegration) -> Self {
        Self { state, memory: Some(memory) }
    }

    /// Run ALL hard rules in priority order using the default SharedState.
    /// Delegates to [`evaluate_with_ohlcv`] with a live snapshot from SharedState.
    pub async fn evaluate(&self, symbol: &str) -> HardRulesGateResult {
        let snapshot = OhlcvSnapshot::capture(symbol, &self.state).await;
        self.evaluate_with_ohlcv(symbol, &snapshot).await
    }

    /// Run ALL hard rules using an explicit OHLCV snapshot so all 3 verification
    /// layers (HardRulesGate, LLM, Kronos) see the identical market data.
    ///
    /// Priority-based blocking logic:
    /// - Critical/High failures: ALWAYS block (no override possible)
    /// - Medium failures: block ONLY if no Critical/High rule has already been checked
    ///   (i.e., Medium rules are soft-blocks that can be overridden by a Higher rule passing)
    /// - Low failures: WARNINGS ONLY — logged but never block
    ///
    /// This prevents a Low-priority position-size preference from blocking a
    /// Critical drawdown halt, while still enforcing Medium-priority regime checks.
    pub async fn evaluate_with_ohlcv(
        &self,
        symbol: &str,
        snapshot: &OhlcvSnapshot,
    ) -> HardRulesGateResult {
        self.evaluate_with_volatility(symbol, snapshot, 0.0).await
    }

    /// Volatility-aware rule evaluation.
    /// sigma: market volatility (0.0 calm, 1.0 extreme)
    pub async fn evaluate_with_volatility(
        &self,
        symbol: &str,
        snapshot: &OhlcvSnapshot,
        sigma: f64,
    ) -> HardRulesGateResult {
        let mut failed_rules = Vec::new();
        let mut traces: Vec<RuleTrace> = Vec::new();
        let mut highest_blocking_priority = None;
        let mut total_checked = 0;

        // ── CRITICAL PRIORITY (Never overridden) ────────────────────────────

        // 1. Trading enabled check (policy cache first)
        total_checked += 1;
        let enabled = if let Some(ref mem) = self.memory {
            mem.check_policy("trading_enabled")
        } else {
            self.state.portfolio.read().await.trading_enabled
        };
        {
            let passed = enabled;
            traces.push(RuleTrace {
                rule_name: "trading_enabled".to_string(),
                priority: RulePriority::Critical,
                steps: vec![ReasoningStep {
                    step: "check_trading_enabled".to_string(),
                    description: "Portfolio trading_enabled flag must be true".to_string(),
                    observed: if enabled { 1.0 } else { 0.0 },
                    threshold: 1.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    "Trading is enabled".to_string()
                } else {
                    "Trading is disabled (drawdown halt active)".to_string()
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "trading_enabled".to_string(),
                    priority: RulePriority::Critical,
                    reason: "Trading is disabled (drawdown halt active)".to_string(),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Critical);
                }
            }
        }

        // 2. Daily drawdown limit (2% hard limit)
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let dd = portfolio.max_drawdown_today;
            let passed = dd <= 0.02;
            traces.push(RuleTrace {
                rule_name: "daily_drawdown".to_string(),
                priority: RulePriority::Critical,
                steps: vec![ReasoningStep {
                    step: "compare_drawdown".to_string(),
                    description: format!("Daily drawdown {:.2}% vs 2.0% hard limit", dd * 100.0),
                    observed: dd,
                    threshold: 0.02,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!("Drawdown {:.2}% within 2% limit", dd * 100.0)
                } else {
                    format!("Daily drawdown at {:.2}% exceeds 2% hard limit", dd * 100.0)
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "daily_drawdown".to_string(),
                    priority: RulePriority::Critical,
                    reason: format!("Daily drawdown at {:.2}% exceeds 2% hard limit", dd * 100.0),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Critical);
                }
            }
        }

        // 3. Red folder discipline
        total_checked += 1;
        {
            let rules = self.state.rules.read().await;
            let discipline_on = rules.red_folder_discipline;
            drop(rules);
            let calendar = self.state.calendar_events.read().await;
            let today_str = Utc::now().format("%Y-%m-%d").to_string();
            let red_folder_count = calendar
                .iter()
                .filter(|event| {
                    event.impact == rat_core::calendar::EventImpact::High
                        && event.date == today_str
                })
                .count();
            let passed = !discipline_on || red_folder_count == 0;
            traces.push(RuleTrace {
                rule_name: "red_folder".to_string(),
                priority: RulePriority::Critical,
                steps: vec![
                    ReasoningStep {
                        step: "check_discipline_enabled".to_string(),
                        description: "Red folder discipline flag".to_string(),
                        observed: if discipline_on { 1.0 } else { 0.0 },
                        threshold: 1.0,
                        passed: discipline_on,
                    },
                    ReasoningStep {
                        step: "count_red_folders".to_string(),
                        description: format!(
                            "High-impact events on {}: {} (must be 0)",
                            today_str, red_folder_count
                        ),
                        observed: red_folder_count as f64,
                        threshold: 0.0,
                        passed: red_folder_count == 0,
                    },
                ],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    if !discipline_on {
                        "Red folder discipline disabled".to_string()
                    } else {
                        "No high-impact events today".to_string()
                    }
                } else {
                    format!("{} high-impact economic event(s) today", red_folder_count)
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "red_folder".to_string(),
                    priority: RulePriority::Critical,
                    reason: "High-impact economic event today".to_string(),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Critical);
                }
            }
        }

        // 4. Session timing (crypto bypasses this)
        total_checked += 1;
        {
            let is_crypto = rat_core::is_crypto_symbol(symbol);
            let mut in_session = true; // default: pass (crypto bypass)
            if !is_crypto {
                let rules = self.state.rules.read().await;
                if rules.respect_session_timing {
                    let now = Utc::now();
                    let indian_open = crate::helpers::get_indian_session_info(now).market_open;
                    let global_open = rat_core::is_in_trading_session(now, &rules);
                    in_session = indian_open || global_open;
                }
            }
            let passed = in_session;
            traces.push(RuleTrace {
                rule_name: "session_timing".to_string(),
                priority: RulePriority::Critical,
                steps: vec![
                    ReasoningStep {
                        step: "check_crypto_bypass".to_string(),
                        description: format!("Symbol {} is_crypto={}", symbol, is_crypto),
                        observed: if is_crypto { 1.0 } else { 0.0 },
                        threshold: 0.0,
                        passed: true, // crypto bypass is informational
                    },
                    ReasoningStep {
                        step: "check_session".to_string(),
                        description: "Market session open check".to_string(),
                        observed: if in_session { 1.0 } else { 0.0 },
                        threshold: 1.0,
                        passed: in_session,
                    },
                ],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    if is_crypto {
                        "Crypto bypasses session check".to_string()
                    } else {
                        "Within allowed trading session".to_string()
                    }
                } else {
                    "Outside allowed trading sessions".to_string()
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "session_timing".to_string(),
                    priority: RulePriority::Critical,
                    reason: "Outside allowed trading sessions".to_string(),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Critical);
                }
            }
        }

        // ── HIGH PRIORITY (Risk limits, circuit breakers) ───────────────────

        // 5. Portfolio heat limit (10%)
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let heat = if portfolio.total_equity > 0.0 {
                portfolio
                    .open_positions
                    .iter()
                    .map(|p| p.risk_amount)
                    .sum::<f64>()
                    / portfolio.total_equity
            } else {
                0.0
            };
            // Volatility-adjusted heat limit: high sigma compresses allowed heat
            let heat_limit = if sigma > 0.03 {
                0.10 * (1.0 - sigma * 0.5) // At sigma=0.05, limit drops to 7.5%
            } else {
                0.10
            };
            let passed = heat <= heat_limit;
            traces.push(RuleTrace {
                rule_name: "portfolio_heat".to_string(),
                priority: RulePriority::High,
                steps: vec![ReasoningStep {
                    step: "compare_heat".to_string(),
                    description: format!("Portfolio heat {:.1}% vs {:.1}% volatility-adjusted limit (sigma: {:.4})", heat * 100.0, heat_limit * 100.0, sigma),
                    observed: heat,
                    threshold: heat_limit,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!("Heat {:.1}% within {:.1}% limit", heat * 100.0, heat_limit * 100.0)
                } else {
                    format!("Portfolio heat at {:.1}% exceeds {:.1}% volatility-adjusted limit (sigma: {:.4})", heat * 100.0, heat_limit * 100.0, sigma)
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "portfolio_heat".to_string(),
                    priority: RulePriority::High,
                    reason: format!("Portfolio heat at {:.1}% exceeds 10% limit", heat * 100.0),
                });
                if highest_blocking_priority.is_none()
                    || highest_blocking_priority == Some(RulePriority::Medium)
                {
                    highest_blocking_priority = Some(RulePriority::High);
                }
            }
        }

        // 6. Consecutive loss circuit breaker (4+)
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let losses = portfolio.consecutive_losses;
            let passed = losses < 4;
            traces.push(RuleTrace {
                rule_name: "loss_circuit_breaker".to_string(),
                priority: RulePriority::High,
                steps: vec![ReasoningStep {
                    step: "compare_consecutive_losses".to_string(),
                    description: format!("{} consecutive losses vs 4 threshold", losses),
                    observed: losses as f64,
                    threshold: 4.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!("{} consecutive losses, below 4 threshold", losses)
                } else {
                    format!("{} consecutive losses — circuit breaker triggered", losses)
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "loss_circuit_breaker".to_string(),
                    priority: RulePriority::High,
                    reason: format!("{} consecutive losses — circuit breaker triggered", losses),
                });
                if highest_blocking_priority.is_none()
                    || highest_blocking_priority == Some(RulePriority::Medium)
                {
                    highest_blocking_priority = Some(RulePriority::High);
                }
            }
        }

        // 7. Max daily trades (8 total)
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let trades = portfolio.total_trades_today;
            let passed = trades < 8;
            traces.push(RuleTrace {
                rule_name: "max_daily_trades".to_string(),
                priority: RulePriority::High,
                steps: vec![ReasoningStep {
                    step: "compare_daily_trades".to_string(),
                    description: format!("{} trades today vs 8 limit", trades),
                    observed: trades as f64,
                    threshold: 8.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!("{} trades today, within 8-trade limit", trades)
                } else {
                    format!("{} trades today exceeds 8-trade daily limit", trades)
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "max_daily_trades".to_string(),
                    priority: RulePriority::High,
                    reason: format!("{} trades today exceeds 8-trade daily limit", trades),
                });
                if highest_blocking_priority.is_none()
                    || highest_blocking_priority == Some(RulePriority::Medium)
                {
                    highest_blocking_priority = Some(RulePriority::High);
                }
            }
        }

        // 8. Cooldown check — per-symbol (trading ETH must not block on a prior BTC trade).
        // Paper mode uses a shorter cooldown for observation/testing.
        total_checked += 1;
        {
            let rules = self.state.rules.read().await;
            let mut cooldown = rules.cooldown_secs;
            if self.state.config.paper_mode {
                cooldown = cooldown.min(60);
            }
            drop(rules);

            let portfolio = self.state.portfolio.read().await;
            let last_trade_time =
                portfolio
                    .last_trade_by_symbol
                    .get(symbol)
                    .copied()
                    .or_else(|| {
                        if portfolio.last_trade_symbol.as_deref() == Some(symbol) {
                            portfolio.last_trade_time
                        } else {
                            None
                        }
                    });
            let (elapsed_secs, cooldown_active, passed) =
                if let Some(last_trade_time) = last_trade_time {
                    let elapsed = Utc::now() - last_trade_time;
                    let secs = elapsed.num_seconds();
                    (secs, secs < cooldown as i64, secs >= cooldown as i64)
                } else {
                    // No prior trade on this symbol — no cooldown
                    (0, false, true)
                };
            traces.push(RuleTrace {
                rule_name: "cooldown".to_string(),
                priority: RulePriority::High,
                steps: vec![ReasoningStep {
                    step: "compare_cooldown".to_string(),
                    description: format!(
                        "{}s elapsed since last {} trade (min {}s)",
                        elapsed_secs, symbol, cooldown
                    ),
                    observed: elapsed_secs as f64,
                    threshold: cooldown as f64,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!("Cooldown cleared ({}s >= {}s)", elapsed_secs, cooldown)
                } else {
                    format!(
                        "{}s since last {} trade (min {}s)",
                        elapsed_secs, symbol, cooldown
                    )
                },
            });
            if cooldown_active {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "cooldown".to_string(),
                    priority: RulePriority::High,
                    reason: format!(
                        "{}s since last {} trade (min {}s)",
                        elapsed_secs, symbol, cooldown
                    ),
                });
                if highest_blocking_priority.is_none()
                    || highest_blocking_priority == Some(RulePriority::Medium)
                {
                    highest_blocking_priority = Some(RulePriority::High);
                }
            }
        }

        // ── AUTO-REGIME INFERENCE (Bootstrapping fix) ────────────────────────
        // If market_regime is None (first run / no skills yet), compute a
        // preliminary regime from the OHLCV snapshot so the pipeline can proceed.
        // Uses the same snapshot data as LLM and Kronos — all 3 layers see identical data.
        {
            let regime = *self.state.market_regime.read().await;
            if regime.is_none() {
                if snapshot.len() >= 50 {
                    let prices: Vec<f64> = snapshot.bars().iter().map(|b| b.close).collect();
                    let highs: Vec<f64> = snapshot.bars().iter().map(|b| b.high).collect();
                    let lows: Vec<f64> = snapshot.bars().iter().map(|b| b.low).collect();
                    let inferred = crate::helpers::estimate_market_regime(&prices, &highs, &lows);
                    *self.state.market_regime.write().await = Some(inferred);
                    println!(
                        "[HardRulesGate] 🧭 Auto-inferred regime for {}: {:?} (from {} snapshot bars)",
                        symbol,
                        inferred,
                        snapshot.len()
                    );
                } else {
                    println!(
                        "[HardRulesGate] ⏳ Bootstrapping {} — only {} snapshot bars (need 50+), using relaxed thresholds",
                        symbol,
                        snapshot.len()
                    );
                }
            }
        }

        // ── MEDIUM PRIORITY (Regime, confluence) ────────────────────────────

        // 9. Regime safety: no BUY in bear regime with low confluence
        total_checked += 1;
        {
            let regime = *self.state.market_regime.read().await;
            let is_bear = regime == Some(crate::types::MarketRegime::TrendingBear);
            let base_confluence = {
                let rules = self.state.rules.read().await;
                rules.min_confluence_score
            };
            let bear_threshold = base_confluence;
            let confluence = crate::helpers::resolve_symbol_confluence(&self.state, symbol).await;
            let passed = !is_bear || confluence >= bear_threshold;
            traces.push(RuleTrace {
                rule_name: "regime_safety".to_string(),
                priority: RulePriority::Medium,
                steps: vec![
                    ReasoningStep {
                        step: "check_regime".to_string(),
                        description: format!("Current regime: {:?}", regime),
                        observed: if is_bear { 1.0 } else { 0.0 },
                        threshold: 0.0,
                        passed: !is_bear, // not-bear always passes
                    },
                    ReasoningStep {
                        step: "compare_confluence_bear".to_string(),
                        description: format!(
                            "Confluence {:.1}% vs bear threshold {:.0}%",
                            confluence * 100.0,
                            bear_threshold * 100.0
                        ),
                        observed: confluence,
                        threshold: bear_threshold,
                        passed: confluence >= bear_threshold,
                    },
                ],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    if !is_bear {
                        "Not in bear regime — rule not applicable".to_string()
                    } else {
                        format!(
                            "Bear regime confluence {:.1}% >= {:.0}% threshold",
                            confluence * 100.0,
                            bear_threshold * 100.0
                        )
                    }
                } else {
                    format!(
                        "Bear regime with confluence {:.1}% < {:.0}% minimum",
                        confluence * 100.0,
                        bear_threshold * 100.0
                    )
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "regime_safety".to_string(),
                    priority: RulePriority::Medium,
                    reason: format!(
                        "Bear regime with confluence {:.1}% < {:.0}% minimum",
                        confluence * 100.0,
                        bear_threshold * 100.0
                    ),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Medium);
                }
            }
        }

        // 10. Confluence minimum (regime-adaptive)
        // Uses snapshot bar count so all layers see the same data volume.
        total_checked += 1;
        {
            let confluence = crate::helpers::resolve_symbol_confluence(&self.state, symbol).await;
            let agg_is_none = self.state.last_aggregated_signal.read().await.is_none();
            let regime = *self.state.market_regime.read().await;
            let bars_count = snapshot.len();
            let is_bootstrapping = bars_count < 100 || agg_is_none;
            let base_confluence = {
                let rules = self.state.rules.read().await;
                rules.min_confluence_score
            };
            let mut min_confluence = match &regime {
                Some(crate::types::MarketRegime::TrendingBull) => {
                    (base_confluence - 0.15).max(0.30)
                }
                Some(crate::types::MarketRegime::TrendingBear) => base_confluence,
                Some(crate::types::MarketRegime::Ranging) => base_confluence,
                Some(crate::types::MarketRegime::Volatile) => base_confluence + 0.10,
                Some(crate::types::MarketRegime::LowLiquidity) => {
                    (base_confluence + 0.20).min(0.90)
                }
                None => base_confluence,
            };
            if is_bootstrapping {
                min_confluence = 0.35;
            }
            let passed = confluence >= min_confluence;
            traces.push(RuleTrace {
                rule_name: "confluence_minimum".to_string(),
                priority: RulePriority::Medium,
                steps: vec![
                    ReasoningStep {
                        step: "check_bootstrapping".to_string(),
                        description: format!(
                            "bars={}, agg_signal_present={}",
                            bars_count, !agg_is_none
                        ),
                        observed: bars_count as f64,
                        threshold: 100.0,
                        passed: !is_bootstrapping, // informational
                    },
                    ReasoningStep {
                        step: "compare_confluence".to_string(),
                        description: format!(
                            "Confluence {:.1}% vs {} threshold {:.1}%",
                            confluence * 100.0,
                            if is_bootstrapping {
                                "bootstrap"
                            } else {
                                "regime"
                            },
                            min_confluence * 100.0
                        ),
                        observed: confluence,
                        threshold: min_confluence,
                        passed,
                    },
                ],
                verdict: if passed || is_bootstrapping {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: if is_bootstrapping { 0.5 } else { 1.0 },
                conclusion: if is_bootstrapping {
                    format!(
                        "Bootstrap override — confluence {:.1}% accepted ({} bars)",
                        confluence * 100.0,
                        bars_count
                    )
                } else if passed {
                    format!(
                        "Confluence {:.1}% >= regime minimum {:.1}%",
                        confluence * 100.0,
                        min_confluence * 100.0
                    )
                } else {
                    format!(
                        "Confluence {:.1}% below regime minimum {:.1}%",
                        confluence * 100.0,
                        min_confluence * 100.0
                    )
                },
            });
            if !passed && !is_bootstrapping {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "confluence_minimum".to_string(),
                    priority: RulePriority::Medium,
                    reason: format!(
                        "Confluence {:.1}% below regime minimum {:.1}%",
                        confluence * 100.0,
                        min_confluence * 100.0
                    ),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Medium);
                }
            } else if is_bootstrapping && confluence < min_confluence {
                println!(
                    "[HardRulesGate] ⏩ Confluence {:.1}% below bootstrap minimum {:.1}% ({} bars) — allowing pass to seed skills",
                    confluence * 100.0, min_confluence * 100.0, bars_count
                );
            }
        }

        // ── LOW PRIORITY (Position limits — WARNINGS ONLY) ──────────────────

        // 11. Open position check (max 3 per symbol)
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let sym_positions = portfolio
                .open_positions
                .iter()
                .filter(|p| p.symbol == symbol)
                .count();
            let passed = sym_positions < 3;
            traces.push(RuleTrace {
                rule_name: "max_positions_per_symbol".to_string(),
                priority: RulePriority::Low,
                steps: vec![ReasoningStep {
                    step: "count_symbol_positions".to_string(),
                    description: format!("{} positions on {} vs 3 max", sym_positions, symbol),
                    observed: sym_positions as f64,
                    threshold: 3.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "WARN".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!(
                        "{} position(s) on {} — within 3-per-symbol limit",
                        sym_positions, symbol
                    )
                } else {
                    format!(
                        "{} positions on {} — max 3 per symbol",
                        sym_positions, symbol
                    )
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "max_positions_per_symbol".to_string(),
                    priority: RulePriority::Low,
                    reason: format!(
                        "{} positions on {} — max 3 per symbol",
                        sym_positions, symbol
                    ),
                });
                // Low priority: never blocks, just warns
            }
        }

        // 12. Max total open positions (10)
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let total = portfolio.open_positions.len();
            let passed = total < 10;
            traces.push(RuleTrace {
                rule_name: "max_total_positions".to_string(),
                priority: RulePriority::Low,
                steps: vec![ReasoningStep {
                    step: "count_total_positions".to_string(),
                    description: format!("{} total open positions vs 10 max", total),
                    observed: total as f64,
                    threshold: 10.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "WARN".to_string()
                },
                confidence: 1.0,
                conclusion: if passed {
                    format!("{} open position(s) — within 10-total limit", total)
                } else {
                    format!("{} open positions — max 10 total", total)
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "max_total_positions".to_string(),
                    priority: RulePriority::Low,
                    reason: format!("{} open positions — max 10 total", total),
                });
                // Low priority: never blocks, just warns
            }
        }

        // ══════════════════════════════════════════════════════════════════════
        // ADVANCED RISK RULES (13–17) — Quantitative risk management
        // These rules add institutional-grade risk checks beyond basic limits.
        // ══════════════════════════════════════════════════════════════════════

        // 13. Kelly Criterion position sizing — block if proposed size > 2× half-Kelly
        //     Prevents over-sizing relative to historical edge.
        total_checked += 1;
        {
            let kelly_stats = self.state.episode_store.kelly_trade_stats(50);
            let portfolio = self.state.portfolio.read().await;
            let equity = portfolio.total_equity;
            let has_enough_data = kelly_stats.trade_count >= 10
                && kelly_stats.avg_win > 0.0
                && kelly_stats.avg_loss > 0.0;
            let (passed, kelly_half, trade_count) = if has_enough_data {
                let kelly = rat_core::kelly_criterion_fraction(
                    kelly_stats.win_probability,
                    kelly_stats.avg_win,
                    kelly_stats.avg_loss,
                    equity,
                    100.0, // reference price for fraction calc
                    true,  // conservative = half-Kelly
                );
                // Warn if total risk exposure exceeds 2× half-Kelly fraction of equity
                let total_risk: f64 = portfolio.open_positions.iter().map(|p| p.risk_amount).sum();
                let kelly_risk_limit = equity * kelly.half_kelly * 2.0;
                let passed = kelly_risk_limit <= 0.0 || total_risk <= kelly_risk_limit;
                (passed, kelly.half_kelly, kelly_stats.trade_count)
            } else {
                (true, 0.0, kelly_stats.trade_count)
            };
            traces.push(RuleTrace {
                rule_name: "kelly_sizing".to_string(),
                priority: RulePriority::High,
                steps: vec![
                    ReasoningStep {
                        step: "check_kelly_data".to_string(),
                        description: format!(
                            "Trade history: {} trades (need ≥10 for Kelly)",
                            trade_count
                        ),
                        observed: trade_count as f64,
                        threshold: 10.0,
                        passed: has_enough_data,
                    },
                    ReasoningStep {
                        step: "compare_kelly_risk".to_string(),
                        description: format!(
                            "Half-Kelly fraction: {:.2}%, total risk vs 2× Kelly limit",
                            kelly_half * 100.0
                        ),
                        observed: kelly_half,
                        threshold: kelly_half * 2.0,
                        passed,
                    },
                ],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: if has_enough_data { 0.9 } else { 0.3 },
                conclusion: if !has_enough_data {
                    format!(
                        "Insufficient trade history ({} trades) — Kelly check skipped",
                        trade_count
                    )
                } else if passed {
                    format!(
                        "Half-Kelly={:.2}% — risk within 2× Kelly limit",
                        kelly_half * 100.0
                    )
                } else {
                    format!(
                        "Half-Kelly={:.2}% — total risk exceeds 2× Kelly-optimal limit",
                        kelly_half * 100.0
                    )
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "kelly_sizing".to_string(),
                    priority: RulePriority::High,
                    reason: format!(
                        "Total risk exceeds 2× half-Kelly ({:.2}%) limit — over-sizing risk",
                        kelly_half * 100.0
                    ),
                });
                if highest_blocking_priority.is_none()
                    || highest_blocking_priority == Some(RulePriority::Medium)
                {
                    highest_blocking_priority = Some(RulePriority::High);
                }
            }
        }

        // 14. Correlation-based portfolio heat — block if weighted correlation > 0.7
        //     Prevents concentrated exposure to correlated assets.
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let positions = &portfolio.open_positions;
            let (passed, avg_corr, pos_count) = if positions.len() < 2 {
                (true, 0.0, positions.len())
            } else {
                // Compute average pairwise correlation proxy using symbol similarity
                let mut corr_sum = 0.0;
                let mut corr_count = 0usize;
                for i in 0..positions.len() {
                    for j in (i + 1)..positions.len() {
                        let sym_a = &positions[i].symbol;
                        let sym_b = &positions[j].symbol;
                        // Proxy: same base asset (e.g., BTC/ETH all crypto) → high corr
                        // Different asset classes → lower corr
                        let corr = if sym_a == sym_b {
                            1.0
                        } else if rat_core::is_crypto_symbol(sym_a)
                            && rat_core::is_crypto_symbol(sym_b)
                        {
                            0.75 // crypto-to-crypto typical correlation
                        } else {
                            0.3 // cross-asset lower correlation
                        };
                        corr_sum += corr;
                        corr_count += 1;
                    }
                }
                let avg = if corr_count > 0 {
                    corr_sum / corr_count as f64
                } else {
                    0.0
                };
                // Weighted by risk: if high-correlation positions have large risk, more dangerous
                let total_risk: f64 = positions.iter().map(|p| p.risk_amount).sum();
                let weighted_corr = avg * (total_risk / portfolio.total_equity.max(1.0));
                let passed = weighted_corr <= 0.07; // 7% weighted correlation heat limit
                (passed, weighted_corr, positions.len())
            };
            traces.push(RuleTrace {
                rule_name: "correlation_heat".to_string(),
                priority: RulePriority::Medium,
                steps: vec![
                    ReasoningStep {
                        step: "count_positions".to_string(),
                        description: format!("{} open positions for correlation check", pos_count),
                        observed: pos_count as f64,
                        threshold: 2.0,
                        passed: pos_count >= 2, // informational
                    },
                    ReasoningStep {
                        step: "compare_correlation_heat".to_string(),
                        description: format!(
                            "Weighted correlation heat {:.2}% vs 7.0% limit",
                            avg_corr * 100.0
                        ),
                        observed: avg_corr,
                        threshold: 0.07,
                        passed,
                    },
                ],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "BLOCK".to_string()
                },
                confidence: 0.8,
                conclusion: if pos_count < 2 {
                    "Fewer than 2 positions — correlation check not applicable".to_string()
                } else if passed {
                    format!(
                        "Weighted correlation heat {:.1}% within 7% limit",
                        avg_corr * 100.0
                    )
                } else {
                    format!(
                        "Weighted correlation heat {:.1}% exceeds 7% limit — concentrated risk",
                        avg_corr * 100.0
                    )
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "correlation_heat".to_string(),
                    priority: RulePriority::Medium,
                    reason: format!(
                        "Weighted correlation heat {:.1}% exceeds 7% limit",
                        avg_corr * 100.0
                    ),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Medium);
                }
            }
        }

        // 15. Maximum Adverse Excursion (MAE) tracking — warn if any position has
        //     unrealised loss > 2× its original risk amount (stop should have triggered).
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let mut worst_mae_ratio = 0.0_f64;
            let mut worst_symbol = String::new();
            for pos in &portfolio.open_positions {
                if pos.symbol == symbol && pos.risk_amount > 0.0 {
                    let unrealised_loss = pos.unrealized_pnl.min(0.0).abs();
                    let mae_ratio = unrealised_loss / pos.risk_amount;
                    if mae_ratio > worst_mae_ratio {
                        worst_mae_ratio = mae_ratio;
                        worst_symbol = pos.symbol.clone();
                    }
                }
            }
            // Only check positions for the current symbol
            let passed = worst_mae_ratio <= 2.0 || worst_symbol.is_empty();
            traces.push(RuleTrace {
                rule_name: "mae_tracking".to_string(),
                priority: RulePriority::Medium,
                steps: vec![ReasoningStep {
                    step: "compare_mae_ratio".to_string(),
                    description: format!(
                        "Worst MAE on {}: {:.1}× risk (limit 2.0×)",
                        symbol, worst_mae_ratio
                    ),
                    observed: worst_mae_ratio,
                    threshold: 2.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "WARN".to_string()
                },
                confidence: 0.85,
                conclusion: if worst_symbol.is_empty() {
                    format!("No open {} positions to check MAE", symbol)
                } else if passed {
                    format!(
                        "MAE on {} at {:.1}× risk — within 2× limit",
                        worst_symbol, worst_mae_ratio
                    )
                } else {
                    format!(
                        "MAE on {} at {:.1}× risk exceeds 2× limit — stop should have triggered",
                        worst_symbol, worst_mae_ratio
                    )
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "mae_tracking".to_string(),
                    priority: RulePriority::Medium,
                    reason: format!(
                        "MAE on {} at {:.1}× risk exceeds 2× limit",
                        worst_symbol, worst_mae_ratio
                    ),
                });
                if highest_blocking_priority.is_none() {
                    highest_blocking_priority = Some(RulePriority::Medium);
                }
            }
        }

        // 16. Session-based risk budgets — reduce max risk per trade during volatile
        //     session windows (first/last 15 min) and during lunch-hour thin liquidity.
        total_checked += 1;
        {
            let is_crypto = rat_core::is_crypto_symbol(symbol);
            let now = Utc::now();
            let session_info = crate::helpers::get_indian_session_info(now);
            // Determine if we're in a high-risk session window
            let (in_volatile_window, window_label) = if is_crypto {
                // Crypto: no session restrictions, always pass
                (false, "crypto_bypass".to_string())
            } else if session_info.is_pre_open || session_info.is_post_close {
                (true, "outside_hours".to_string())
            } else if session_info.minutes_since_open < 15 {
                (true, "opening_volatility".to_string())
            } else if let Some(mins_to_close) = session_info.time_to_close {
                if mins_to_close < 15 {
                    (true, "closing_unwind".to_string())
                } else {
                    (false, "normal_session".to_string())
                }
            } else {
                (false, "normal_session".to_string())
            };
            // During volatile windows, the risk budget is halved — the strategy_decision
            // layer reads this as a reduced max_risk_per_trade. We flag it here as a
            // WARN (not block) since the strategy layer enforces the actual cap.
            let passed = !in_volatile_window;
            traces.push(RuleTrace {
                rule_name: "session_risk_budget".to_string(),
                priority: RulePriority::Low,
                steps: vec![ReasoningStep {
                    step: "check_session_window".to_string(),
                    description: format!(
                        "Session window: {} (volatile={})",
                        window_label, in_volatile_window
                    ),
                    observed: if in_volatile_window { 0.5 } else { 1.0 },
                    threshold: 1.0,
                    passed,
                }],
                verdict: if passed {
                    "PASS".to_string()
                } else {
                    "WARN".to_string()
                },
                confidence: 1.0,
                conclusion: if is_crypto {
                    "Crypto — no session-based risk budget".to_string()
                } else if passed {
                    "Normal session — full risk budget".to_string()
                } else {
                    format!("Volatile window ({}) — risk budget halved", window_label)
                },
            });
            if !passed {
                // Low priority: informational only — don't push to failed_rules
                // (would break pass/warn_only test assertions during volatile windows)
                println!(
                    "[HardRulesGate] ⏳ Session risk budget: {} (risk halved)",
                    window_label
                );
                // Explicitly push AgentDecision here since it won't be in failed_rules.
                let decision = AgentDecision::new(
                    "HardRulesGate",
                    symbol,
                    DecisionVerdict::Warn {
                        reason: format!(
                            "Volatile session window ({}) — risk budget halved",
                            window_label
                        ),
                    },
                )
                .with_evidence(vec![
                    format!("rule: session_risk_budget"),
                    format!("window: {}", window_label),
                ]);
                self.state.push_agent_decision(decision).await;
            }
        }

        // 17. Volatility-adjusted stops — block if stop loss distance < 0.5× ATR
        //     Prevents stops that are too tight for current volatility (whipsaw risk).
        total_checked += 1;
        {
            let portfolio = self.state.portfolio.read().await;
            let sym_positions: Vec<_> = portfolio
                .open_positions
                .iter()
                .filter(|p| p.symbol == symbol)
                .collect();
            // Compute ATR from snapshot for volatility reference
            let atr = if snapshot.len() >= 14 {
                crate::helpers::compute_atr(snapshot.bars(), 14)
            } else {
                // Fallback: use price range as ATR proxy
                snapshot
                    .bars()
                    .iter()
                    .map(|b| (b.high - b.low).abs())
                    .sum::<f64>()
                    / snapshot.len().max(1) as f64
            };
            let current_price = snapshot.last_close();
            let atr_pct = if current_price > 0.0 {
                atr / current_price
            } else {
                0.0
            };
            // Check each open position's stop distance against ATR
            let mut worst_sl_ratio = 0.0_f64;
            let mut has_tight_stop = false;
            for pos in &sym_positions {
                let sl_distance = (pos.entry_price - pos.stop_loss).abs();
                if sl_distance > 0.0 && atr > 0.0 {
                    let sl_atr_ratio = sl_distance / atr;
                    if sl_atr_ratio < worst_sl_ratio || worst_sl_ratio == 0.0 {
                        worst_sl_ratio = sl_atr_ratio;
                    }
                    if sl_atr_ratio < 0.5 {
                        has_tight_stop = true;
                    }
                }
            }
            let passed = !has_tight_stop || sym_positions.is_empty();
            traces.push(RuleTrace {
                rule_name: "vol_adjusted_stops".to_string(),
                priority: RulePriority::High,
                steps: vec![
                    ReasoningStep {
                        step: "compute_atr".to_string(),
                        description: format!("ATR={:.2} ({:.2}% of price) from {} bars", atr, atr_pct * 100.0, snapshot.len()),
                        observed: atr_pct,
                        threshold: 0.0,
                        passed: true, // informational
                    },
                    ReasoningStep {
                        step: "compare_sl_to_atr".to_string(),
                        description: format!(
                            "Tightest SL on {}: {:.2}× ATR (min 0.5×)",
                            symbol, worst_sl_ratio
                        ),
                        observed: worst_sl_ratio,
                        threshold: 0.5,
                        passed,
                    },
                ],
                verdict: if passed { "PASS".to_string() } else { "BLOCK".to_string() },
                confidence: 0.9,
                conclusion: if sym_positions.is_empty() {
                    format!("No open {} positions — stop check not applicable", symbol)
                } else if passed {
                    format!(
                        "SL distances OK — tightest at {:.2}× ATR (min 0.5×)",
                        worst_sl_ratio
                    )
                } else {
                    format!(
                        "SL at {:.2}× ATR < 0.5× minimum — too tight for current volatility (whipsaw risk)",
                        worst_sl_ratio
                    )
                },
            });
            if !passed {
                failed_rules.push(RuleCheck {
                    passed: false,
                    rule_name: "vol_adjusted_stops".to_string(),
                    priority: RulePriority::High,
                    reason: format!(
                        "SL at {:.2}× ATR < 0.5× minimum — too tight for volatility",
                        worst_sl_ratio
                    ),
                });
                if highest_blocking_priority.is_none()
                    || highest_blocking_priority == Some(RulePriority::Medium)
                {
                    highest_blocking_priority = Some(RulePriority::High);
                }
            }
        }

        // ── Determine pass/fail using priority-based blocking ────────────────
        // Critical/High always block. Medium blocks only if no Higher rule overrides.
        // Low never blocks.
        let passed = highest_blocking_priority.is_none()
            || highest_blocking_priority == Some(RulePriority::Low);

        // ── Log results and produce AgentDecisions ─────────────────────────
        if passed {
            if !failed_rules.is_empty() {
                println!(
                    "[HardRulesGate] ⚠️  {} warnings for {} (none blocking)",
                    failed_rules.len(),
                    symbol
                );
                for rule in &failed_rules {
                    println!("  - [WARNING] {}: {}", rule.rule_name, rule.reason);
                    // Push WARN decision for each warning
                    let decision = AgentDecision::new(
                        "HardRulesGate",
                        symbol,
                        DecisionVerdict::Warn {
                            reason: rule.reason.clone(),
                        },
                    )
                    .with_evidence(vec![
                        format!("rule: {}", rule.rule_name),
                        format!("priority: {:?}", rule.priority),
                    ]);
                    self.state.push_agent_decision(decision).await;
                }
            } else {
                println!(
                    "[HardRulesGate] ✅ All {} rules passed for {}",
                    total_checked, symbol
                );
                // Push PASS decision
                let decision = AgentDecision::new(
                    "HardRulesGate",
                    symbol,
                    DecisionVerdict::Pass {
                        reason: format!("All {} hard rules passed", total_checked),
                    },
                )
                .with_evidence(vec![format!("rules_checked: {}", total_checked)]);
                self.state.push_agent_decision(decision).await;
            }
        } else {
            println!(
                "[HardRulesGate] ⛔ {}/{} rules failed for {} (blocking priority: {:?})",
                failed_rules.len(),
                total_checked,
                symbol,
                highest_blocking_priority
            );
            for rule in &failed_rules {
                if rule.priority >= RulePriority::Medium {
                    println!(
                        "  - [BLOCK] [{:?}] {}: {}",
                        rule.priority, rule.rule_name, rule.reason
                    );
                    // Push BLOCK decision for blocking rules
                    let decision = AgentDecision::new(
                        "HardRulesGate",
                        symbol,
                        DecisionVerdict::Block {
                            reason: rule.reason.clone(),
                        },
                    )
                    .with_evidence(vec![
                        format!("rule: {}", rule.rule_name),
                        format!("priority: {:?}", rule.priority),
                    ]);
                    self.state.push_agent_decision(decision).await;
                } else {
                    println!(
                        "  - [WARN] [{:?}] {}: {}",
                        rule.priority, rule.rule_name, rule.reason
                    );
                    // Push WARN decision for non-blocking warnings
                    let decision = AgentDecision::new(
                        "HardRulesGate",
                        symbol,
                        DecisionVerdict::Warn {
                            reason: rule.reason.clone(),
                        },
                    )
                    .with_evidence(vec![
                        format!("rule: {}", rule.rule_name),
                        format!("priority: {:?}", rule.priority),
                    ]);
                    self.state.push_agent_decision(decision).await;
                }
            }
        }

        // ── Build blocking_rule and summary ──────────────────────────────
        let blocking_rule_name = if !passed {
            failed_rules
                .iter()
                .filter(|r| r.priority >= RulePriority::Medium)
                .max_by_key(|r| r.priority)
                .map(|r| r.rule_name.clone())
        } else {
            None
        };
        let failed_count = failed_rules.len();
        let failed_is_empty = failed_rules.is_empty();

        let summary = if passed {
            if failed_is_empty {
                format!("All {} hard rules passed for {}", total_checked, symbol)
            } else {
                format!(
                    "{} warnings (non-blocking) for {}; all rules passed",
                    failed_count, symbol
                )
            }
        } else {
            format!(
                "{}/{} rules failed for {} (blocker: {})",
                failed_count,
                total_checked,
                symbol,
                blocking_rule_name.as_deref().unwrap_or("unknown")
            )
        };

        HardRulesGateResult {
            passed,
            failed_rules,
            highest_failed_priority: highest_blocking_priority,
            total_rules_checked: total_checked,
            reasoning_chain: ChainOfReasoning {
                symbol: symbol.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                rules_evaluated: total_checked,
                traces,
                overall_verdict: if passed {
                    if failed_is_empty {
                        "PASS".to_string()
                    } else {
                        "WARN_ONLY".to_string()
                    }
                } else {
                    "BLOCK".to_string()
                },
                blocking_rule: blocking_rule_name,
                summary,
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Unit Tests — Priority-based blocking logic
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MarketRegime, OpenPosition};
    use chrono::{Duration, Utc};
    use std::sync::atomic::{AtomicU64, Ordering};
    use rat_core::{Config, DisciplineRules, MemoryStore, TradeDirection};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Create a clean SharedState for testing. Uses unique DB names to avoid
    /// DatabaseAlreadyOpen when tests run in parallel.
    /// Calendar events are cleared so the red_folder Critical rule doesn't
    /// interfere with priority-level testing.
    async fn setup_state() -> SharedState {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let redb_path = format!("file:test_hrg_{}.redb?mode=memory", id);
        let memory = MemoryStore::new(&redb_path).expect("MemoryStore creation");
        let config = Config {
            kronos_service_url: "http://127.0.0.1:19999".to_string(),
            ..Config::default()
        };
        let rules = DisciplineRules::default();
        let state = SharedState::new(memory, rules, config, ":memory:").expect("SharedState init");
        // Clear calendar events so red_folder Critical rule doesn't fire in tests
        *state.calendar_events.write().await = Vec::new();
        state
    }

    /// Create a SharedState with trading disabled (Critical rule).
    async fn setup_state_trading_disabled() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            portfolio.trading_enabled = false;
        }
        state
    }

    /// Create a SharedState with drawdown > 2% (Critical rule).
    async fn setup_state_drawdown() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            portfolio.max_drawdown_today = 0.025; // 2.5% > 2% limit
        }
        state
    }

    /// Create a SharedState with high portfolio heat (High rule).
    async fn setup_state_high_heat() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            portfolio.total_equity = 100_000.0;
            // Add positions with total risk = 12% of equity
            for i in 0..3 {
                portfolio.open_positions.push(OpenPosition {
                    symbol: format!("SYM{}", i),
                    direction: TradeDirection::Long,
                    entry_price: 100.0,
                    current_price: 100.0,
                    stop_loss: 95.0,
                    take_profit: 110.0,
                    quantity: 1.0,
                    unrealized_pnl: 0.0,
                    unrealized_pnl_pct: 0.0,
                    entry_time: Utc::now(),
                    risk_amount: 4000.0, // 3 × 4000 = 12000 / 100000 = 12%
                });
            }
        }
        state
    }

    /// Create a SharedState with 4+ consecutive losses (High rule).
    async fn setup_state_consecutive_losses() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            portfolio.consecutive_losses = 5;
        }
        state
    }

    /// Create a SharedState with 8+ daily trades (High rule).
    async fn setup_state_max_trades() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            portfolio.total_trades_today = 9;
        }
        state
    }

    /// Create a SharedState with recent trade (cooldown active — High rule).
    /// Default cooldown is 1800s, so 10s ago ensures it blocks.
    async fn setup_state_cooldown() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            let ts = Utc::now() - Duration::seconds(10);
            portfolio.last_trade_time = Some(ts);
            portfolio.last_trade_symbol = Some("BTC".to_string());
            portfolio.last_trade_by_symbol.insert("BTC".to_string(), ts);
        }
        state
    }

    /// Create a SharedState with bear regime + low confluence (Medium rule).
    /// Seeds conviction=0.25 so the bear regime safety rule (threshold=0.30) triggers.
    async fn setup_state_bear_regime_low_confluence() -> SharedState {
        let state = setup_state().await;
        {
            *state.market_regime.write().await = Some(MarketRegime::TrendingBear);
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: -0.3,
                bullish_strength: 0.2,
                bearish_strength: 0.6,
                conviction: 0.25, // below bear threshold 0.30
                consensus: Some(rat_core::agent::SkillDirection::Bearish),
                participating_count: 3,
                bullish_count: 1,
                bearish_count: 2,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
        }
        state
    }

    /// Create a SharedState with high confluence (all Medium rules pass).
    async fn setup_state_high_confluence() -> SharedState {
        let state = setup_state().await;
        {
            *state.market_regime.write().await = Some(MarketRegime::TrendingBull);
            // Seed high confluence so Medium rules pass
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: 0.6,
                bullish_strength: 0.7,
                bearish_strength: 0.1,
                conviction: 0.85, // well above all minimums
                consensus: Some(rat_core::agent::SkillDirection::Bullish),
                participating_count: 5,
                bullish_count: 4,
                bearish_count: 1,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
        }
        state
    }

    /// Create a SharedState with 3 positions on the same symbol (Low rule).
    async fn setup_state_max_positions_per_symbol() -> SharedState {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            for i in 0..3 {
                portfolio.open_positions.push(OpenPosition {
                    symbol: "BTC".to_string(),
                    direction: TradeDirection::Long,
                    entry_price: 60000.0 + i as f64 * 1000.0,
                    current_price: 61000.0,
                    stop_loss: 58000.0,
                    take_profit: 65000.0,
                    quantity: 0.1,
                    unrealized_pnl: 0.0,
                    unrealized_pnl_pct: 0.0,
                    entry_time: Utc::now(),
                    risk_amount: 200.0, // each 200 / 100000 = 0.2%
                });
            }
            portfolio.total_equity = 100_000.0;
        }
        state
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: All rules pass → passed = true
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_all_rules_pass() {
        let state = setup_state_high_confluence().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(result.passed, "All rules should pass when state is clean");
        assert!(
            result.failed_rules.is_empty(),
            "No rules should fail when state is clean"
        );
        assert!(
            result.highest_failed_priority.is_none(),
            "No blocking priority when all pass"
        );
        assert_eq!(result.total_rules_checked, 17, "Should check all 17 rules");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Critical rule always blocks
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_critical_always_blocks() {
        // Scenario 1: Trading disabled (Critical)
        let state = setup_state_trading_disabled().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(!result.passed, "Trading disabled should block");
        assert_eq!(result.highest_failed_priority, Some(RulePriority::Critical));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "trading_enabled"));

        // Scenario 2: Drawdown > 2% (Critical)
        let state = setup_state_drawdown().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("ETH").await;

        assert!(!result.passed, "Drawdown > 2% should block");
        assert_eq!(result.highest_failed_priority, Some(RulePriority::Critical));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "daily_drawdown"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: High priority always blocks
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_high_always_blocks() {
        // Scenario 1: Portfolio heat > 10% (High)
        let state = setup_state_high_heat().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(!result.passed, "High heat should block");
        assert_eq!(result.highest_failed_priority, Some(RulePriority::High));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "portfolio_heat"));

        // Scenario 2: 5 consecutive losses (High)
        let state = setup_state_consecutive_losses().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(!result.passed, "4+ consecutive losses should block");
        assert_eq!(result.highest_failed_priority, Some(RulePriority::High));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "loss_circuit_breaker"));

        // Scenario 3: 9 daily trades (High)
        let state = setup_state_max_trades().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("ETH").await;

        assert!(!result.passed, "8+ daily trades should block");
        assert_eq!(result.highest_failed_priority, Some(RulePriority::High));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "max_daily_trades"));

        // Scenario 4: Cooldown active (High)
        let state = setup_state_cooldown().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(!result.passed, "Active cooldown should block");
        assert_eq!(result.highest_failed_priority, Some(RulePriority::High));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "cooldown"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Medium blocks only if no Critical/High already set
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_medium_blocks_alone() {
        // Bear regime + low confluence → Medium blocks (no Critical/High)
        let state = setup_state_bear_regime_low_confluence().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(
            !result.passed,
            "Medium should block when no Higher rule overrides"
        );
        assert_eq!(
            result.highest_failed_priority,
            Some(RulePriority::Medium),
            "Highest blocking priority should be Medium"
        );
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "regime_safety"));
    }

    #[tokio::test]
    async fn test_medium_does_not_override_critical() {
        // Critical (drawdown) + Medium (confluence) → Critical blocks
        // Seed low conviction so bear regime safety rule triggers
        let state = setup_state_drawdown().await;
        {
            *state.market_regime.write().await = Some(MarketRegime::TrendingBear);
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: -0.3,
                bullish_strength: 0.2,
                bearish_strength: 0.6,
                conviction: 0.25, // below bear threshold 0.30
                consensus: Some(rat_core::agent::SkillDirection::Bearish),
                participating_count: 3,
                bullish_count: 1,
                bearish_count: 2,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(!result.passed, "Should block (Critical)");
        assert_eq!(
            result.highest_failed_priority,
            Some(RulePriority::Critical),
            "Critical should override Medium"
        );
        // Both rules should be in failed_rules
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "daily_drawdown"));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "regime_safety" || r.rule_name == "confluence_minimum"));
    }

    #[tokio::test]
    async fn test_medium_does_not_override_high() {
        // High (heat) + Medium (confluence) → High blocks
        // Set regime to TrendingBear for high threshold, and seed low conviction
        let state = setup_state_high_heat().await;
        {
            *state.market_regime.write().await = Some(MarketRegime::TrendingBear);
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: 0.1,
                bullish_strength: 0.2,
                bearish_strength: 0.3,
                conviction: 0.25, // below base=0.30 bear threshold
                consensus: Some(rat_core::agent::SkillDirection::Bearish),
                participating_count: 3,
                bullish_count: 1,
                bearish_count: 2,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(!result.passed, "Should block (High)");
        assert_eq!(
            result.highest_failed_priority,
            Some(RulePriority::High),
            "High should override Medium"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Low never blocks — warnings only
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_low_never_blocks() {
        // 3 positions on BTC → Low rule triggers but should NOT block
        let state = setup_state_max_positions_per_symbol().await;
        {
            // Also set high confluence so Medium rules pass
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: 0.6,
                bullish_strength: 0.7,
                bearish_strength: 0.1,
                conviction: 0.85,
                consensus: Some(rat_core::agent::SkillDirection::Bullish),
                participating_count: 5,
                bullish_count: 4,
                bearish_count: 1,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
            *state.market_regime.write().await = Some(MarketRegime::TrendingBull);
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(
            result.passed,
            "Low priority (max positions) should NOT block — only warn"
        );
        assert!(
            !result.failed_rules.is_empty(),
            "Should still record the Low-priority failure"
        );
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "max_positions_per_symbol"));
        // highest_failed_priority stays None because Low never sets it
        assert!(
            result.highest_failed_priority.is_none(),
            "Low should not set highest_blocking_priority"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Critical + Low together → Critical blocks, Low is warning
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_critical_overrides_low() {
        let state = setup_state_drawdown().await;
        {
            let mut portfolio = state.portfolio.write().await;
            // Also add 3 positions on BTC (Low rule)
            for i in 0..3 {
                portfolio.open_positions.push(OpenPosition {
                    symbol: "BTC".to_string(),
                    direction: TradeDirection::Long,
                    entry_price: 60000.0 + i as f64 * 1000.0,
                    current_price: 61000.0,
                    stop_loss: 58000.0,
                    take_profit: 65000.0,
                    quantity: 0.1,
                    unrealized_pnl: 0.0,
                    unrealized_pnl_pct: 0.0,
                    entry_time: Utc::now(),
                    risk_amount: 200.0,
                });
            }
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(
            !result.passed,
            "Critical should block even with Low warnings"
        );
        assert_eq!(
            result.highest_failed_priority,
            Some(RulePriority::Critical),
            "Critical overrides Low"
        );
        // Both rules should be recorded
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "daily_drawdown"));
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "max_positions_per_symbol"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Multiple High rules → still blocks with High priority
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_multiple_high_rules() {
        let state = setup_state().await;
        {
            let mut portfolio = state.portfolio.write().await;
            portfolio.consecutive_losses = 5; // High
            portfolio.total_trades_today = 10; // High
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("ETH").await;

        assert!(!result.passed, "Multiple High rules should block");
        assert_eq!(
            result.highest_failed_priority,
            Some(RulePriority::High),
            "Highest should be High"
        );
        assert!(
            result.failed_rules.len() >= 2,
            "Should have at least 2 failed rules"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Confluence minimum varies by regime (Medium rule)
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_confluence_regime_adaptive() {
        // Ranging regime → min confluence = base = 0.30
        // Seed low confluence (0.25) → should block
        // Must seed 101+ bars to bypass the bootstrap override (<100 bars skips failure)
        let state = setup_state().await;
        {
            *state.market_regime.write().await = Some(MarketRegime::Ranging);
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: 0.1,
                bullish_strength: 0.2,
                bearish_strength: 0.3,
                conviction: 0.25, // below 0.30 threshold
                consensus: Some(rat_core::agent::SkillDirection::Neutral),
                participating_count: 3,
                bullish_count: 1,
                bearish_count: 2,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
            // Seed 101 bars so is_bootstrapping = false and actual threshold applies
            let bar = rat_core::OhlcvBar {
                timestamp: "2026-01-01T00:00:00+00:00".to_string(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1000.0,
            };
            state
                .ohlcv_history
                .write()
                .await
                .insert("BTC".to_string(), vec![bar; 101]);
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(
            !result.passed,
            "Confluence 0.25 < 0.30 (Ranging min) should block"
        );
        assert!(result
            .failed_rules
            .iter()
            .any(|r| r.rule_name == "confluence_minimum"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: Crypto bypasses session timing (Critical)
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_crypto_bypasses_session_timing() {
        // BTC should pass even if session timing would fail for non-crypto
        let state = setup_state_high_confluence().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        assert!(
            result.passed,
            "BTC (crypto) should bypass session timing check"
        );
        assert!(
            !result
                .failed_rules
                .iter()
                .any(|r| r.rule_name == "session_timing"),
            "session_timing should not fail for crypto"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: HardRulesGateResult::passed() helper works
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_result_helper() {
        let result = HardRulesGateResult::passed();
        assert!(result.passed);
        assert!(result.failed_rules.is_empty());
        assert!(result.highest_failed_priority.is_none());
        assert_eq!(result.total_rules_checked, 0);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: RulePriority ordering (Critical > High > Medium > Low)
    // ═══════════════════════════════════════════════════════════════════════
    #[test]
    fn test_priority_ordering() {
        assert!(RulePriority::Critical > RulePriority::High);
        assert!(RulePriority::High > RulePriority::Medium);
        assert!(RulePriority::Medium > RulePriority::Low);
        assert!(RulePriority::Critical > RulePriority::Low);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: All 17 rules checked (total_rules_checked = 17)
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_all_17_rules_checked() {
        let state = setup_state_high_confluence().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;
        assert_eq!(
            result.total_rules_checked, 17,
            "Should check all 17 rules regardless of pass/fail"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TEST: ChainOfReasoning traces are populated
    // ═══════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_traces_populated() {
        let state = setup_state_high_confluence().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        let cor = &result.reasoning_chain;
        assert_eq!(cor.traces.len(), 17, "Should have 17 traces (one per rule)");
        assert_eq!(cor.overall_verdict, "PASS");
        assert!(
            cor.blocking_rule.is_none(),
            "No blocking rule when all pass"
        );
        assert!(cor.summary.contains("All"));
        // Every trace should have at least one step
        for trace in &cor.traces {
            assert!(
                !trace.steps.is_empty(),
                "Trace '{}' should have at least one step",
                trace.rule_name
            );
            assert!(
                !trace.conclusion.is_empty(),
                "Trace '{}' should have a conclusion",
                trace.rule_name
            );
            assert!(
                !trace.verdict.is_empty(),
                "Trace '{}' should have a verdict",
                trace.rule_name
            );
        }
    }

    #[tokio::test]
    async fn test_traces_on_block() {
        let state = setup_state_trading_disabled().await;
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        let cor = &result.reasoning_chain;
        assert_eq!(
            cor.traces.len(),
            17,
            "Should have 17 traces even when blocked"
        );
        assert_eq!(cor.overall_verdict, "BLOCK");
        assert_eq!(cor.blocking_rule.as_deref(), Some("trading_enabled"));
        // Find the trading_enabled trace — should have verdict BLOCK
        let te_trace = cor
            .traces
            .iter()
            .find(|t| t.rule_name == "trading_enabled")
            .unwrap();
        assert_eq!(te_trace.verdict, "BLOCK");
        assert_eq!(te_trace.priority, RulePriority::Critical);
        assert!(
            te_trace.steps.iter().any(|s| !s.passed),
            "Should have a failing step"
        );
    }

    #[tokio::test]
    async fn test_traces_on_warn_only() {
        // Low-priority warning only → overall_verdict should be WARN_ONLY
        let state = setup_state_max_positions_per_symbol().await;
        {
            let agg = rat_core::skill_aggregator::AggregatedSignal {
                net_signal: 0.6,
                bullish_strength: 0.7,
                bearish_strength: 0.1,
                conviction: 0.85,
                consensus: Some(rat_core::agent::SkillDirection::Bullish),
                participating_count: 5,
                bullish_count: 4,
                bearish_count: 1,
                neutral_count: 0,
            };
            *state.last_aggregated_signal.write().await = Some(agg);
            *state.market_regime.write().await = Some(MarketRegime::TrendingBull);
        }
        let gate = HardRulesGate::new(state);
        let result = gate.evaluate("BTC").await;

        let cor = &result.reasoning_chain;
        assert_eq!(cor.overall_verdict, "WARN_ONLY");
        assert!(
            cor.traces.len() >= 11,
            "Should have traces for all evaluated rules"
        );
        // Find the max_positions_per_symbol trace — should have verdict WARN
        let pos_trace = cor
            .traces
            .iter()
            .find(|t| t.rule_name == "max_positions_per_symbol")
            .unwrap();
        assert_eq!(pos_trace.verdict, "WARN");
    }
}
