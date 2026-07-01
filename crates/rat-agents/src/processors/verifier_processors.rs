//! Verifier Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct RiskPsychologyProcessor;
pub struct RiskCalculatorProcessor;
pub struct ReflectorProcessor;

#[async_trait]
impl AgentProcessor for RiskPsychologyProcessor {
    fn name(&self) -> &str { "RiskPsychology" }
    fn role(&self) -> &str { "Evaluate emotional state" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, signal, .. } => {
                let drawdown = portfolio_state.drawdown;
                let position_count = portfolio_state.positions.len() as f64;
                let unrealized_pnl: f64 = portfolio_state.positions.iter().map(|p| p.unrealized_pnl).sum();
                let equity = portfolio_state.equity;
                let pnl_ratio = if equity > 0.0 { unrealized_pnl / equity } else { 0.0 };

                // Tilt detection: many losers or large drawdown
                let losers = portfolio_state.positions.iter().filter(|p| p.unrealized_pnl < 0.0).count() as f64;
                let tilt_score = if position_count > 0.0 {
                    (losers / position_count * 0.5 + drawdown * 0.5).clamp(0.0, 1.0)
                } else {
                    drawdown
                };

                // Overconfidence: large winning streak + increasing position sizes
                let overconfidence = if unrealized_pnl > 0.0 && pnl_ratio > 0.05 {
                    (pnl_ratio * 3.0).min(0.5)
                } else {
                    0.0
                };

                // Revenge trading signals: high drawdown + many positions
                let revenge = if drawdown > 0.08 && position_count > 8.0 {
                    ((drawdown - 0.08) * 10.0).min(0.5)
                } else {
                    0.0
                };

                let composite_risk = (tilt_score * 0.4 + overconfidence * 0.3 + revenge * 0.3).clamp(0.0, 1.0);
                let mental_health = (1.0 - composite_risk).clamp(0.0, 1.0);

                let action = if mental_health < 0.3 {
                    "HALT_TRADING".to_string()
                } else if mental_health < 0.5 {
                    "COOLING_OFF".to_string()
                } else if mental_health < 0.7 {
                    "REDUCE_SIZE".to_string()
                } else {
                    "CLEAR".to_string()
                };

                AgentOutput {
                    action,
                    confidence: mental_health,
                    reasoning: format!("Psych: mental={:.2} | tilt={:.2} (losers {}/{}) | overconfidence={:.2} | revenge={:.2} | DD={:.1}%",
                        mental_health, tilt_score, losers as u32, position_count as u32, overconfidence, revenge, drawdown * 100.0),
                    data: None,
                }
            }
            AgentInput::Review { pnl, lessons, .. } => {
                let is_big_loss = pnl < -50.0;
                let has_risk_lessons = lessons.iter().any(|l| l.to_lowercase().contains("risk") || l.to_lowercase().contains("size") || l.to_lowercase().contains("revenge"));
                let risk_flag = if is_big_loss && has_risk_lessons { 0.8 }
                    else if is_big_loss { 0.6 }
                    else if has_risk_lessons { 0.4 }
                    else { 0.1 };

                AgentOutput {
                    action: if risk_flag > 0.6 { "FLAG_RISK_PATTERN".to_string() } else { "REVIEW".to_string() },
                    confidence: 1.0 - risk_flag,
                    reasoning: format!("Review P&L ${:.2} | risk_flag={:.2} | lessons: {}", pnl, risk_flag, lessons.join(", ")),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for psychology check".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for RiskCalculatorProcessor {
    fn name(&self) -> &str { "RiskCalculator" }
    fn role(&self) -> &str { "Calculate position sizing" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let equity = portfolio_state.equity;
                let drawdown = portfolio_state.drawdown;
                let daily_pnl = portfolio_state.daily_pnl;
                let position_count = portfolio_state.positions.len() as f64;

                // Risk per trade: 2% base, reduced by drawdown
                let base_risk_pct = 0.02;
                let dd_multiplier = (1.0 - drawdown * 2.0).clamp(0.2, 1.0);
                let risk_per_trade = base_risk_pct * dd_multiplier;
                let max_risk_amount = equity * risk_per_trade;

                // Position limits based on account state
                let max_positions = if drawdown > 0.10 { 3 } else if drawdown > 0.05 { 6 } else { 10 };

                // Daily loss limit
                let daily_loss_limit = equity * 0.03;
                let daily_used = if daily_pnl < 0.0 { daily_pnl.abs() } else { 0.0 };
                let daily_remaining = (daily_loss_limit - daily_used).max(0.0);

                // Kelly fraction (simplified): assumes 55% win rate, 1.5 reward/risk
                let kelly = 0.55 - (0.45 / 1.5);
                let half_kelly = kelly * 0.5;
                let kelly_size = equity * half_kelly;

                let sizing_confidence = if drawdown > 0.15 { 0.3 }
                    else if drawdown > 0.10 { 0.5 }
                    else if position_count > max_positions as f64 { 0.4 }
                    else if daily_used > daily_loss_limit * 0.8 { 0.35 }
                    else { 0.85 };

                AgentOutput {
                    action: "CALCULATE".to_string(),
                    confidence: sizing_confidence,
                    reasoning: format!("Risk: ${:.2} ({:.2}% equity) | Max {} positions (have {:.0}) | Daily limit ${:.2} (${:.2} used) | Kelly ${:.2} | DD={:.1}%",
                        max_risk_amount, risk_per_trade * 100.0, max_positions, position_count,
                        daily_loss_limit, daily_used, kelly_size, drawdown * 100.0),
                    data: None,
                }
            }
            AgentInput::Execution { symbol, action, size, price } => {
                let order_value = size * price;
                // Validate order size
                let max_single_order = 25000.0;
                let size_ratio = order_value / max_single_order;
                let action_ok = size_ratio <= 1.0;

                AgentOutput {
                    action: if action_ok { "APPROVE".to_string() } else { "REDUCE_SIZE".to_string() },
                    confidence: if action_ok { 0.9 } else { (1.0 / size_ratio).clamp(0.2, 0.5) },
                    reasoning: format!("Order check {}: {} {:.6} @ ${:.2} = ${:.2} ({:.0}% of limit)",
                        symbol, action, size, price, order_value, size_ratio * 100.0),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for risk calc".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for ReflectorProcessor {
    fn name(&self) -> &str { "Reflector" }
    fn role(&self) -> &str { "Post-trade reflection" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let return_pct = if entry_price > 0.0 { (exit_price - entry_price) / entry_price * 100.0 } else { 0.0 };
                let is_win = pnl > 0.0;
                let abs_return = return_pct.abs();
                let risk_reward = if pnl < 0.0 && entry_price > 0.0 {
                    let risk = (entry_price - exit_price).abs();
                    let reward = (exit_price - entry_price).abs();
                    if risk > 0.0 { reward / risk } else { 0.0 }
                } else {
                    abs_return / 2.0 // assumed 2% target
                };

                // Quality score based on R:R and return
                let quality = if is_win {
                    (risk_reward * 0.4 + (abs_return / 5.0).min(1.0) * 0.6).clamp(0.0, 1.0)
                } else {
                    ((1.0 - (abs_return / 5.0).min(1.0)) * 0.6 + (1.0 / (risk_reward + 0.1)).min(1.0) * 0.4).clamp(0.0, 1.0)
                };

                let lesson = if is_win && risk_reward > 1.5 {
                    "Good R:R win — maintain discipline".to_string()
                } else if is_win && risk_reward < 1.0 {
                    "Win but poor R:R — taking profits too early".to_string()
                } else if !is_win && abs_return < 1.0 {
                    "Small loss — acceptable stop".to_string()
                } else if !is_win && abs_return > 3.0 {
                    "Large loss — stop may be too wide or slipped".to_string()
                } else {
                    "Average trade outcome".to_string()
                };

                let confidence = quality.clamp(0.1, 0.95);

                AgentOutput {
                    action: "REFLECT".to_string(),
                    confidence,
                    reasoning: format!("{}: {} | P&L ${:.2} ({:.1}%) | R:R {:.2} | Quality {:.2} | {}",
                        symbol, if is_win { "WIN" } else { "LOSS" }, pnl, return_pct, risk_reward, quality, lesson),
                    data: None,
                }
            }
            AgentInput::Review { trade_id, pnl, lessons } => {
                let avg_lesson_quality = if lessons.is_empty() { 0.5 }
                    else { lessons.iter().map(|l| if l.len() > 20 { 0.8 } else { 0.4 }).sum::<f64>() / lessons.len() as f64 };

                let actionability = lessons.iter()
                    .filter(|l| l.to_lowercase().contains("should") || l.to_lowercase().contains("next time") || l.to_lowercase().contains("avoid"))
                    .count() as f64 / lessons.len().max(1) as f64;

                let reflection_score = (avg_lesson_quality * 0.5 + actionability * 0.5).clamp(0.0, 1.0);

                AgentOutput {
                    action: "REFLECT".to_string(),
                    confidence: reflection_score,
                    reasoning: format!("Review {}: P&L ${:.2} | {} lessons (actionability={:.2}) | score={:.2}",
                        trade_id, pnl, lessons.len(), actionability, reflection_score),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No outcome to reflect on".to_string(), data: None }
        }
    }
}
