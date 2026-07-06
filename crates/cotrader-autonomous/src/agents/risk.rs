//! Risk Agent — Position sizing → drawdown → circuit breaker → limits → overtrading prevention.
//!
//! Merges: RiskCalculator, HardRulesGate, DrawdownMonitor, OvertradingPreventer

use super::reasoning::ReasoningChain;
use crate::state::SharedState;
use crate::types::TradeSignal;

#[derive(Clone)]
pub struct RiskAgent {
    pub state: SharedState,
}

#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    pub passed: bool,
    pub blocking_reason: Option<String>,
    pub warnings: Vec<String>,
    pub risk_score: f64,
    pub position_size_allowed: f64,
    pub adjustments: RiskAdjustments,
}

#[derive(Debug, Clone, Default)]
pub struct RiskAdjustments {
    pub size_multiplier: f64, // 1.0 = no change, 0.5 = halve size
    pub sl_widening: f64,     // 1.0 = no change, 1.3 = widen SL 30%
}

impl RiskAgent {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Run all risk checks on a proposed trade.
    pub async fn check(&self, signal: &TradeSignal) -> RiskCheckResult {
        let rules = self.state.rule_engine.rules.read().await;
        let portfolio = self.state.portfolio_store.portfolio.read().await;
        let mut warnings = Vec::new();
        let mut adjustments = RiskAdjustments::default();

        // 1. Portfolio heat check
        let total_risk: f64 = portfolio.open_positions.iter().map(|p| p.risk_amount).sum();
        let heat = if portfolio.total_equity > 0.0 {
            total_risk / portfolio.total_equity
        } else {
            0.0
        };

        if heat > rules.max_risk_per_trade * 5.0 {
            return RiskCheckResult {
                passed: false,
                blocking_reason: Some(format!("Portfolio heat critical: {:.1}% (max 50%)", heat * 100.0)),
                warnings,
                risk_score: heat,
                position_size_allowed: 0.0,
                adjustments,
            };
        }

        if heat > rules.max_risk_per_trade * 3.0 {
            warnings.push(format!("Portfolio heat elevated: {:.1}%", heat * 100.0));
            adjustments.size_multiplier *= 0.5; // Halve position size
        }

        // 2. Consecutive losses check
        if portfolio.consecutive_losses >= rules.max_consecutive_losses {
            return RiskCheckResult {
                passed: false,
                blocking_reason: Some(format!(
                    "Consecutive losses: {} (max {})",
                    portfolio.consecutive_losses, rules.max_consecutive_losses
                )),
                warnings,
                risk_score: 0.8,
                position_size_allowed: 0.0,
                adjustments,
            };
        }

        if portfolio.consecutive_losses >= 3 {
            warnings.push(format!("{} consecutive losses — reduce exposure", portfolio.consecutive_losses));
            adjustments.size_multiplier *= 0.75;
        }

        // 3. Daily drawdown check
        if portfolio.max_drawdown_today > rules.max_daily_drawdown {
            return RiskCheckResult {
                passed: false,
                blocking_reason: Some(format!(
                    "Daily drawdown exceeded: {:.1}% (max {:.1}%)",
                    portfolio.max_drawdown_today * 100.0,
                    rules.max_daily_drawdown * 100.0
                )),
                warnings,
                risk_score: 0.9,
                position_size_allowed: 0.0,
                adjustments,
            };
        }

        if portfolio.max_drawdown_today > rules.max_daily_drawdown * 0.7 {
            warnings.push(format!("Daily drawdown approaching limit: {:.1}%", portfolio.max_drawdown_today * 100.0));
            adjustments.size_multiplier *= 0.8;
        }

        // 4. Overtrading check
        if portfolio.total_trades_today >= 20 {
            return RiskCheckResult {
                passed: false,
                blocking_reason: Some(format!(
                    "Overtrading: {} trades today (max 20)",
                    portfolio.total_trades_today
                )),
                warnings,
                risk_score: 0.7,
                position_size_allowed: 0.0,
                adjustments,
            };
        }

        if portfolio.total_trades_today >= 10 {
            warnings.push(format!("High trade count today: {}", portfolio.total_trades_today));
            adjustments.size_multiplier *= 0.8;
        }

        // 5. Duplicate symbol check
        if portfolio.open_positions.iter().any(|p| p.symbol == signal.symbol) {
            warnings.push(format!("Already have open position for {}", signal.symbol));
            adjustments.size_multiplier *= 0.5;
        }

        // 6. Position size limit
        let max_position = portfolio.total_equity * rules.max_risk_per_trade;
        let actual_size = signal.position_size * signal.entry_price;
        let size_ok = actual_size <= max_position;

        if !size_ok {
            return RiskCheckResult {
                passed: false,
                blocking_reason: Some(format!(
                    "Position too large: ${:.0} > ${:.0} max",
                    actual_size, max_position
                )),
                warnings,
                risk_score: heat,
                position_size_allowed: 0.0,
                adjustments,
            };
        }

        // 7. R:R check
        let risk = (signal.entry_price - signal.stop_loss).abs();
        let reward = (signal.take_profit - signal.entry_price).abs();
        if risk > 0.0 && reward / risk < 1.5 {
            warnings.push(format!(
                "Low R:R ratio: {:.1}:1 (minimum 1.5:1)",
                reward / risk
            ));
            adjustments.size_multiplier *= 0.7;
        }

        RiskCheckResult {
            passed: true,
            blocking_reason: None,
            warnings,
            risk_score: heat,
            position_size_allowed: max_position / signal.entry_price,
            adjustments,
        }
    }

    /// Produce reasoning chain.
    pub fn reason(&self, result: &RiskCheckResult) -> ReasoningChain {
        let mut chain = ReasoningChain::new("Risk", "ALL");

        chain.add_step(
            &format!("Portfolio heat: {:.1}%", result.risk_score * 100.0),
            "Checked total risk exposure against limits",
            vec![format!("heat={:.1}%", result.risk_score * 100.0)],
            0.9,
        );

        if !result.warnings.is_empty() {
            chain.add_step(
                &format!("{} warnings issued", result.warnings.len()),
                &result.warnings.join("; "),
                result.warnings.iter().cloned().collect(),
                0.7,
            );
        }

        if result.adjustments.size_multiplier < 1.0 {
            chain.add_step(
                &format!("Position size reduced to {:.0}%", result.adjustments.size_multiplier * 100.0),
                "Risk adjustments applied based on current conditions",
                vec![format!("multiplier={:.2}", result.adjustments.size_multiplier)],
                0.75,
            );
        }

        if result.passed {
            chain.finalize("Risk check PASSED — trade allowed");
        } else {
            chain.finalize(&format!(
                "Risk check BLOCKED: {}",
                result.blocking_reason.as_deref().unwrap_or("unknown")
            ));
        }

        chain
    }
}
