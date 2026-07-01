//! Operations Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct PortfolioAdministratorProcessor;
pub struct JournalKeeperProcessor;

#[async_trait]
impl AgentProcessor for PortfolioAdministratorProcessor {
    fn name(&self) -> &str { "PortfolioAdministrator" }
    fn role(&self) -> &str { "Reconciliation" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let return_pct = if entry_price > 0.0 { (exit_price - entry_price) / entry_price * 100.0 } else { 0.0 };
                let _is_win = pnl > 0.0;

                // Reconciliation checks
                let _price_diff = (exit_price - entry_price).abs();
                let expected_pnl_direction = exit_price > entry_price;
                let pnl_consistent = (pnl > 0.0) == expected_pnl_direction;

                let checks_passed = if pnl_consistent { 1 } else { 0 };
                let total_checks = 1;
                let recon_confidence = checks_passed as f64 / total_checks as f64;

                let action = if !pnl_consistent {
                    "FLAG_INCONSISTENCY".to_string()
                } else if pnl.abs() > 100.0 {
                    "LARGE_TRADE_REVIEW".to_string()
                } else {
                    "RECONCILED".to_string()
                };

                AgentOutput {
                    action,
                    confidence: recon_confidence,
                    reasoning: format!("{}: P&L ${:.2} ({:.1}%) | entry=${:.2} exit=${:.2} | consistent={} | checks {}/{}",
                        symbol, pnl, return_pct, entry_price, exit_price, pnl_consistent, checks_passed, total_checks),
                    data: None,
                }
            }
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let equity = portfolio_state.equity;
                let total_unrealized: f64 = portfolio_state.positions.iter().map(|p| p.unrealized_pnl).sum();
                let net_exposure: f64 = portfolio_state.positions.iter().map(|p| {
                    if p.side == "LONG" { p.size } else { -p.size }
                }).sum();

                // Portfolio health metrics
                let utilization = if equity > 0.0 { total_unrealized.abs() / equity } else { 0.0 };
                let exposure_ratio = if equity > 0.0 { net_exposure.abs() / equity } else { 0.0 };

                let health = if utilization < 0.05 && exposure_ratio < 1.0 { "HEALTHY" }
                    else if utilization < 0.10 { "STRESSED" }
                    else { "AT_RISK" };

                let confidence = if health == "HEALTHY" { 0.9 } else if health == "STRESSED" { 0.6 } else { 0.3 };

                AgentOutput {
                    action: "RECONCILE_PORTFOLIO".to_string(),
                    confidence,
                    reasoning: format!("Portfolio: equity=${:.2} | unrealized=${:.2} ({:.1}%) | net_exposure={:.1}x | {}",
                        equity, total_unrealized, utilization * 100.0, exposure_ratio, health),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for reconciliation".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for JournalKeeperProcessor {
    fn name(&self) -> &str { "JournalKeeper" }
    fn role(&self) -> &str { "Trade journal" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Review { trade_id, pnl, lessons } => {
                let _is_win = pnl > 0.0;
                let lesson_count = lessons.len();
                let has_actionable = lessons.iter().any(|l|
                    l.to_lowercase().contains("should") ||
                    l.to_lowercase().contains("next time") ||
                    l.to_lowercase().contains("avoid") ||
                    l.to_lowercase().contains("improve")
                );

                // Journal quality: completeness of lessons + actionability
                let completeness = if lesson_count >= 3 { 1.0 } else { lesson_count as f64 / 3.0 };
                let actionability = if has_actionable { 0.8 } else { 0.3 };
                let journal_quality = (completeness * 0.5 + actionability * 0.5).clamp(0.0, 1.0);

                let action = if lesson_count == 0 {
                    "JOURNAL_INCOMPLETE".to_string()
                } else if !has_actionable {
                    "JOURNAL_NEEDS_ACTIONABLE".to_string()
                } else {
                    "JOURNAL_COMPLETE".to_string()
                };

                AgentOutput {
                    action,
                    confidence: journal_quality,
                    reasoning: format!("{}: P&L ${:.2} | {} lessons (actionable={}) | quality={:.2}",
                        trade_id, pnl, lesson_count, has_actionable, journal_quality),
                    data: None,
                }
            }
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let return_pct = if entry_price > 0.0 { (exit_price - entry_price) / entry_price * 100.0 } else { 0.0 };
                let severity = if pnl < -50.0 { "HIGH" } else if pnl < -20.0 { "MEDIUM" } else { "LOW" };

                AgentOutput {
                    action: "LOG_OUTCOME".to_string(),
                    confidence: 0.95,
                    reasoning: format!("{}: {} P&L ${:.2} ({:.1}%) | severity={} | entry=${:.2} exit=${:.2}",
                        symbol, if pnl > 0.0 { "WIN" } else { "LOSS" }, pnl, return_pct, severity, entry_price, exit_price),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No review data".to_string(), data: None }
        }
    }
}
