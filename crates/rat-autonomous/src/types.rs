use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rat_core::TradeDirection;

// ═══════════════════════════════════════════════════════════════════════════════
// Agent Decision — Unified inter-agent communication protocol.
//
// Every agent produces an `AgentDecision` when it evaluates a trade proposal.
// This creates a transparent "virtual communication" layer where agents
// explicitly state their verdict (block/hold/skip/pass/buy/sell/veto) with
// structured reasons, evidence, and addressed-to fields.
//
// The `CommunicationLog` aggregates all decisions for a pipeline run,
// making the full agent-to-agent conversation visible and auditable.
// ═══════════════════════════════════════════════════════════════════════════════

/// Verdict an agent returns after evaluating a trade proposal.
/// Each variant carries a structured reason explaining the decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionVerdict {
    /// Agent blocks the trade — hard rule violation or critical risk.
    Block { reason: String },
    /// Agent holds — insufficient evidence or mixed signals.
    Hold { reason: String },
    /// Agent skips — not applicable (e.g., crypto bypasses session check).
    Skip { reason: String },
    /// Agent passes — all checks clear, no concerns.
    Pass { reason: String },
    /// Agent recommends BUY.
    Buy { reason: String, confidence: f64 },
    /// Agent recommends SELL.
    Sell { reason: String, confidence: f64 },
    /// Judge vetoes the debate consensus.
    Veto { reason: String },
    /// Agent warns — non-blocking concern for higher-priority agents.
    Warn { reason: String },
}

impl DecisionVerdict {
    /// Human-readable label for the verdict.
    pub fn label(&self) -> &str {
        match self {
            DecisionVerdict::Block { .. } => "BLOCK",
            DecisionVerdict::Hold { .. } => "HOLD",
            DecisionVerdict::Skip { .. } => "SKIP",
            DecisionVerdict::Pass { .. } => "PASS",
            DecisionVerdict::Buy { .. } => "BUY",
            DecisionVerdict::Sell { .. } => "SELL",
            DecisionVerdict::Veto { .. } => "VETO",
            DecisionVerdict::Warn { .. } => "WARN",
        }
    }

    /// The reason string regardless of variant.
    pub fn reason(&self) -> &str {
        match self {
            DecisionVerdict::Block { reason } => reason,
            DecisionVerdict::Hold { reason } => reason,
            DecisionVerdict::Skip { reason } => reason,
            DecisionVerdict::Pass { reason } => reason,
            DecisionVerdict::Buy { reason, .. } => reason,
            DecisionVerdict::Sell { reason, .. } => reason,
            DecisionVerdict::Veto { reason } => reason,
            DecisionVerdict::Warn { reason } => reason,
        }
    }

    /// Confidence score (0.0–1.0) for Buy/Sell, 1.0 for block/veto, 0.5 otherwise.
    pub fn confidence(&self) -> f64 {
        match self {
            DecisionVerdict::Buy { confidence, .. } | DecisionVerdict::Sell { confidence, .. } => {
                *confidence
            }
            DecisionVerdict::Block { .. } | DecisionVerdict::Veto { .. } => 1.0,
            _ => 0.5,
        }
    }

    /// Whether this verdict is blocking (prevents trade execution).
    pub fn is_blocking(&self) -> bool {
        matches!(
            self,
            DecisionVerdict::Block { .. } | DecisionVerdict::Veto { .. }
        )
    }
}

/// A single agent's decision — the atomic unit of inter-agent communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDecision {
    /// Which agent produced this decision.
    pub agent: String,
    /// Symbol being evaluated.
    pub symbol: String,
    /// The verdict (block/hold/skip/pass/buy/sell/veto/warn).
    pub verdict: DecisionVerdict,
    /// Structured evidence supporting the verdict.
    pub evidence: Vec<String>,
    /// Which agent this decision is addressed to (None = broadcast to all).
    pub addressed_to: Option<String>,
    /// ISO-8601 timestamp.
    pub timestamp: String,
}

impl AgentDecision {
    pub fn new(agent: &str, symbol: &str, verdict: DecisionVerdict) -> Self {
        Self {
            agent: agent.to_string(),
            symbol: symbol.to_string(),
            verdict,
            evidence: vec![],
            addressed_to: None,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Builder: add evidence items.
    pub fn with_evidence(mut self, evidence: Vec<String>) -> Self {
        self.evidence = evidence;
        self
    }

    /// Builder: set addressed_to.
    pub fn addressed_to(mut self, to: &str) -> Self {
        self.addressed_to = Some(to.to_string());
        self
    }

    /// Format as a human-readable communication line.
    pub fn to_communication_line(&self) -> String {
        let target = self.addressed_to.as_deref().unwrap_or("Pipeline");
        format!(
            "[{}] → [{}]: {} on {} — {}",
            self.agent,
            target,
            self.verdict.label(),
            self.symbol,
            self.verdict.reason()
        )
    }
}

/// Full communication log for a single pipeline run — the complete
/// agent-to-agent conversation with verdicts, reasons, and evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationLog {
    /// Unique pipeline run identifier.
    pub run_id: u64,
    /// Symbol being evaluated.
    pub symbol: String,
    /// All agent decisions in chronological order.
    pub decisions: Vec<AgentDecision>,
    /// Final pipeline verdict.
    pub final_verdict: String,
    /// Summary of the full conversation.
    pub summary: String,
}

impl CommunicationLog {
    pub fn new(run_id: u64, symbol: &str) -> Self {
        Self {
            run_id,
            symbol: symbol.to_string(),
            decisions: vec![],
            final_verdict: "PENDING".to_string(),
            summary: String::new(),
        }
    }

    /// Add a decision to the log.
    pub fn push(&mut self, decision: AgentDecision) {
        self.decisions.push(decision);
    }

    /// Format the full conversation as a readable transcript.
    pub fn transcript(&self) -> String {
        let mut lines = vec![format!(
            "═══ Communication Log for {} (run #{}) ═══",
            self.symbol, self.run_id
        )];
        for d in &self.decisions {
            lines.push(d.to_communication_line());
            if !d.evidence.is_empty() {
                for e in &d.evidence {
                    lines.push(format!("  └─ evidence: {}", e));
                }
            }
        }
        lines.push(format!("─── Final: {} ───", self.final_verdict));
        lines.join("\n")
    }

    /// Count how many agents blocked/vetoed.
    pub fn blocking_count(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| d.verdict.is_blocking())
            .count()
    }

    /// Get all blocking reasons.
    pub fn blocking_reasons(&self) -> Vec<String> {
        self.decisions
            .iter()
            .filter(|d| d.verdict.is_blocking())
            .map(|d| format!("[{}] {}", d.agent, d.verdict.reason()))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CotEntry {
    pub id: u64,
    pub chain_id: u64,
    pub parent_id: Option<u64>,
    pub agent: String,
    pub input: String,
    pub action: String,
    pub reason: String,
    pub confidence: f64,
    pub timestamp: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub symbol: String,
    pub direction: TradeDirection,
    pub entry_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub position_size: f64,
    pub confidence_score: f64,
    pub confluence_score: f64,
    pub risk_reward_ratio: f64,
    pub reasoning: String,
    pub timestamp: DateTime<Utc>,
    pub session_valid: bool,
    pub risk_check_passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketRegime {
    TrendingBull,
    TrendingBear,
    Ranging,
    Volatile,
    LowLiquidity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioState {
    pub cash_balance: f64,
    pub total_equity: f64,
    pub daily_pnl: f64,
    pub daily_pnl_pct: f64,
    pub open_positions: Vec<OpenPosition>,
    pub total_trades_today: u32,
    pub winning_trades_today: u32,
    pub losing_trades_today: u32,
    pub consecutive_losses: u32,
    pub max_drawdown_today: f64,
    pub last_trade_time: Option<DateTime<Utc>>,
    /// Symbol of the most recent trade — cooldown applies per-symbol, not globally.
    #[serde(default)]
    pub last_trade_symbol: Option<String>,
    /// Per-symbol last trade timestamps for cooldown enforcement.
    #[serde(default)]
    pub last_trade_by_symbol: HashMap<String, DateTime<Utc>>,
    pub trading_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPosition {
    pub symbol: String,
    pub direction: TradeDirection,
    pub entry_price: f64,
    pub current_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub quantity: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
    pub entry_time: DateTime<Utc>,
    pub risk_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAnalysis {
    pub max_position_size: f64,
    pub risk_per_trade_pct: f64,
    pub risk_reward_ratio: f64,
    pub portfolio_heat: f64,
    pub daily_drawdown_pct: f64,
    pub var_95: f64,
    pub recommendation: RiskRecommendation,
    pub psychology_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskRecommendation {
    Proceed,
    Caution,
    ReduceSize,
    Halt,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Hard Rules Gate — Priority-based rule hierarchy
// Research shows: "The upper layer always overrides the lower layers."
// When rules conflict, highest priority wins. Equal priority → most conservative.
// ═══════════════════════════════════════════════════════════════════════════════

/// Priority levels for rule conflict resolution.
/// Critical rules can NEVER be overridden by lower layers.
/// Variants are ordered LOW → CRITICAL so that derive(Ord) gives
/// Critical > High > Medium > Low (higher index = higher priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RulePriority {
    Low,      // 0 — warnings only, never block
    Medium,   // 1 — block only if no Higher rule overrides
    High,     // 2 — always block (risk limits, circuit breakers)
    Critical, // 3 — never overridden (drawdown halt, session, red folder)
}

/// Result of a single hard rule check.
#[derive(Debug, Clone)]
pub struct RuleCheck {
    pub passed: bool,
    pub rule_name: String,
    pub priority: RulePriority,
    pub reason: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chain-of-Reasoning (CoR) — Structured audit trail for every rule decision.
//
// Each rule produces a `RuleTrace` with ordered `ReasoningStep`s showing:
//   1. What data was examined
//   2. What threshold was applied
//   3. What the actual value was
//   4. How the verdict was reached
//
// The full `ChainOfReasoning` aggregates all traces into a single
// human-readable + machine-parseable audit log for the gate evaluation.
// ═══════════════════════════════════════════════════════════════════════════════

/// A single reasoning step inside a rule trace — the atomic unit of explainability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Short identifier: "check_drawdown", "compare_heat", etc.
    pub step: String,
    /// Human-readable description of what this step checks.
    pub description: String,
    /// The actual observed value (e.g., current_drawdown_pct).
    pub observed: f64,
    /// The threshold it was compared against.
    pub threshold: f64,
    /// Whether this individual step passed.
    pub passed: bool,
}

/// Complete reasoning trace for a single rule — explains *how* the verdict was reached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleTrace {
    /// Rule identifier (matches `RuleCheck.rule_name`).
    pub rule_name: String,
    /// Priority level of this rule.
    pub priority: RulePriority,
    /// Ordered reasoning steps (each step builds on the previous).
    pub steps: Vec<ReasoningStep>,
    /// Final verdict: "PASS", "BLOCK", "WARN", "OVERRIDE".
    pub verdict: String,
    /// Confidence in this verdict (0.0–1.0).
    pub confidence: f64,
    /// Plain-text conclusion summarising the reasoning chain.
    pub conclusion: String,
}

/// Full chain-of-reasoning for an entire HardRulesGate evaluation.
/// Contains one `RuleTrace` per rule evaluated, plus an overall summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainOfReasoning {
    /// Symbol being evaluated.
    pub symbol: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Total number of rules evaluated.
    pub rules_evaluated: usize,
    /// One trace per rule (in evaluation order).
    pub traces: Vec<RuleTrace>,
    /// Overall gate verdict: "PASS", "BLOCK", "WARN_ONLY".
    pub overall_verdict: String,
    /// The rule that caused the block (if any).
    pub blocking_rule: Option<String>,
    /// One-line human-readable summary.
    pub summary: String,
}

/// Complete result of the Hard Rules Gate evaluation.
#[derive(Debug, Clone)]
pub struct HardRulesGateResult {
    pub passed: bool,
    pub failed_rules: Vec<RuleCheck>,
    pub highest_failed_priority: Option<RulePriority>,
    pub total_rules_checked: usize,
    /// Full chain-of-reasoning traces (one per rule evaluated).
    pub reasoning_chain: ChainOfReasoning,
}

impl HardRulesGateResult {
    pub fn passed() -> Self {
        Self {
            passed: true,
            failed_rules: vec![],
            highest_failed_priority: None,
            total_rules_checked: 0,
            reasoning_chain: ChainOfReasoning {
                symbol: String::new(),
                timestamp: String::new(),
                rules_evaluated: 0,
                traces: vec![],
                overall_verdict: "PASS".to_string(),
                blocking_rule: None,
                summary: "No rules evaluated".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub market_open: bool,
    pub session_name: String,
    /// Minutes until close (or open). Stored as i64 to avoid chrono Duration serde issues
    /// in test builds / episode_store contexts (chrono "serde" feature enables DateTime but
    /// Duration requires explicit handling in some derives).
    pub time_to_close: Option<i64>,
    pub time_to_open: Option<i64>,
    pub is_pre_open: bool,
    pub is_post_close: bool,
    pub minutes_since_open: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMatch {
    pub pattern_key: String,
    pub match_score: f64,
    pub historical_outcome: String,
    pub avg_return: f64,
    pub win_rate: f64,
    pub total_occurrences: usize,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub phase: String,
    pub passed: bool,
    pub details: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PipelineSummary {
    pub executed: bool,
    pub phase_results: Vec<PipelineResult>,
    pub total_duration_ms: u64,
    pub final_signal: Option<TradeSignal>,
    pub reason: String,
}

/// Snapshot of OHLCV data taken at pipeline start so all 3 verification layers
/// (HardRulesGate, LLM, Kronos) see the exact same data — no race conditions.
#[derive(Debug, Clone)]
pub struct OhlcvSnapshot {
    pub symbol: String,
    pub bars: Vec<rat_core::OhlcvBar>,
    pub capture_time: chrono::DateTime<chrono::Utc>,
}

impl OhlcvSnapshot {
    /// Capture a snapshot from shared state at the current instant.
    pub async fn capture(symbol: &str, state: &crate::state::SharedState) -> Self {
        let bars = {
            let hist = state.ohlcv_history.read().await;
            hist.get(symbol).cloned().unwrap_or_default()
        };
        Self {
            symbol: symbol.to_string(),
            bars,
            capture_time: chrono::Utc::now(),
        }
    }

    /// Number of bars in this snapshot.
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// Last close price, or 0.0 if empty.
    pub fn last_close(&self) -> f64 {
        self.bars.last().map(|b| b.close).unwrap_or(0.0)
    }

    /// Iterator over the bars (delegates to Vec).
    pub fn bars(&self) -> &[rat_core::OhlcvBar] {
        &self.bars
    }
}
