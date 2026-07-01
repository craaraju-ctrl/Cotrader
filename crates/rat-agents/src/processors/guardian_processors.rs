//! Guardian Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct DrawdownMonitorProcessor;
pub struct OvertradingPreventerProcessor;
pub struct OutcomeLoggerProcessor;

#[async_trait]
impl AgentProcessor for DrawdownMonitorProcessor {
    fn name(&self) -> &str { "DrawdownMonitor" }
    fn role(&self) -> &str { "Track and limit drawdown" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let dd = portfolio_state.drawdown;
                let daily_pnl = portfolio_state.daily_pnl;
                let equity = portfolio_state.equity;
                let positions = &portfolio_state.positions;

                // Count losers and calculate total unrealized loss
                let losers = positions.iter().filter(|p| p.unrealized_pnl < 0.0).count();
                let total_loss: f64 = positions.iter().filter(|p| p.unrealized_pnl < 0.0).map(|p| p.unrealized_pnl).sum();
                let max_single_loss = positions.iter().map(|p| p.unrealized_pnl).fold(0.0_f64, f64::min);

                // Drawdown severity assessment
                let level = if dd > 0.15 { "CRITICAL" }
                    else if dd > 0.10 { "SEVERE" }
                    else if dd > 0.05 { "WARNING" }
                    else if dd > 0.02 { "ELEVATED" }
                    else { "NORMAL" };

                // Recovery potential: how much room before hard limit
                let hard_limit = 0.20;
                let headroom = (hard_limit - dd).max(0.0);

                // Daily loss tracking
                let daily_loss_pct = if equity > 0.0 { daily_pnl.abs() / equity } else { 0.0 };
                let daily_breach = daily_loss_pct > 0.03;

                // Confidence: high when healthy, low when stressed
                let confidence = (1.0 - dd * 5.0).clamp(0.0, 1.0);

                let action = if dd > 0.15 {
                    "HALT_NEW_TRADES".to_string()
                } else if dd > 0.10 || daily_breach {
                    "FORCE_REDUCE".to_string()
                } else if dd > 0.05 {
                    "REDUCE_SIZE".to_string()
                } else if losers > positions.len() / 2 && positions.len() > 3 {
                    "MONITOR_LOSERS".to_string()
                } else {
                    "CLEAR".to_string()
                };

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("Drawdown: {:.1}% ({}) | headroom {:.1}% | daily P&L ${:.2} ({:.1}%) | {} losers (${:.2} total) | worst ${:.2}",
                        dd * 100.0, level, headroom * 100.0, daily_pnl, daily_loss_pct * 100.0,
                        losers, total_loss, max_single_loss),
                    data: None,
                }
            }
            AgentInput::Outcome { pnl, .. } => {
                let is_large_loss = pnl < -30.0;
                let severity = if pnl < -100.0 { "CRITICAL" } else if pnl < -50.0 { "HIGH" } else if pnl < -20.0 { "MEDIUM" } else { "LOW" };

                AgentOutput {
                    action: if is_large_loss { "DRAWDOWN_ALERT".to_string() } else { "LOG".to_string() },
                    confidence: if is_large_loss { 0.3 } else { 0.9 },
                    reasoning: format!("Outcome P&L ${:.2} | severity={}", pnl, severity),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for drawdown monitor".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for OvertradingPreventerProcessor {
    fn name(&self) -> &str { "OvertradingPreventer" }
    fn role(&self) -> &str { "Limit trade frequency" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let trade_count = portfolio_state.positions.len() as f64;
                let equity = portfolio_state.equity;
                let drawdown = portfolio_state.drawdown;
                let daily_pnl = portfolio_state.daily_pnl;

                // Position concentration check
                let total_exposure: f64 = portfolio_state.positions.iter().map(|p| p.size.abs()).sum();
                let avg_position_size = if trade_count > 0.0 { total_exposure / trade_count } else { 0.0 };
                let concentration = if equity > 0.0 { avg_position_size / equity } else { 0.0 };

                // Overtrading score components
                let count_pressure = (trade_count / 15.0).min(1.0); // 15 positions = max
                let dd_pressure = if drawdown > 0.05 { (drawdown - 0.05) * 10.0 } else { 0.0 };
                let loss_pressure = if daily_pnl < 0.0 { (daily_pnl.abs() / equity * 20.0).min(1.0) } else { 0.0 };
                let concentration_pressure = (concentration * 2.0).min(1.0);

                let overtrading_score = (count_pressure * 0.35 + dd_pressure * 0.25 + loss_pressure * 0.25 + concentration_pressure * 0.15).clamp(0.0, 1.0);

                let action = if overtrading_score > 0.8 {
                    "BLOCK_TRADES".to_string()
                } else if overtrading_score > 0.6 {
                    "THROTTLE".to_string()
                } else if overtrading_score > 0.4 {
                    "CAUTION".to_string()
                } else {
                    "CLEAR".to_string()
                };

                let confidence = (1.0 - overtrading_score).clamp(0.0, 1.0);

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("Overtrading: score={:.2} | {} positions (max 15) | DD={:.1}% | daily P&L ${:.2} | concentration={:.1}% | count={} dd={} loss={} conc={:.2}",
                        overtrading_score, trade_count as u32, drawdown * 100.0, daily_pnl,
                        concentration * 100.0, count_pressure, dd_pressure, loss_pressure, concentration_pressure),
                    data: None,
                }
            }
            AgentInput::Execution { symbol, action, size, price } => {
                // Validate against overtrading: position size vs equity
                let order_value = size * price;
                let position_ratio = order_value / 50000.0; // assume 50k base
                let ok = position_ratio < 0.1; // max 10% per trade

                AgentOutput {
                    action: if ok { "ALLOW".to_string() } else { "REDUCE".to_string() },
                    confidence: if ok { 0.9 } else { 0.3 },
                    reasoning: format!("Order check {}: {} {:.6} @ ${:.2} = ${:.2} ({:.1}% of base)", symbol, action, size, price, order_value, position_ratio * 100.0),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for overtrading check".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for OutcomeLoggerProcessor {
    fn name(&self) -> &str { "OutcomeLogger" }
    fn role(&self) -> &str { "Log trade outcomes" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let return_pct = if entry_price > 0.0 { (exit_price - entry_price) / entry_price * 100.0 } else { 0.0 };
                let is_win = pnl > 0.0;
                let severity = if pnl < -100.0 { "CRITICAL" } else if pnl < -50.0 { "HIGH" } else if pnl < -20.0 { "MEDIUM" } else if pnl > 50.0 { "EXCELLENT" } else { "NORMAL" };

                // Log quality: completeness of data
                let has_prices = entry_price > 0.0 && exit_price > 0.0;
                let log_quality = if has_prices { 1.0 } else { 0.5 };

                AgentOutput {
                    action: "LOG".to_string(),
                    confidence: log_quality,
                    reasoning: format!("{}: {} | P&L ${:.2} ({:.1}%) | entry=${:.2} exit=${:.2} | severity={}",
                        symbol, if is_win { "WIN" } else { "LOSS" }, pnl, return_pct, entry_price, exit_price, severity),
                    data: None,
                }
            }
            AgentInput::Review { trade_id, pnl, lessons } => {
                let lesson_quality = if lessons.is_empty() { 0.3 }
                    else { (lessons.iter().map(|l| l.len() as f64).sum::<f64>() / lessons.len() as f64 / 50.0).min(1.0) };

                AgentOutput {
                    action: "LOG_REVIEW".to_string(),
                    confidence: lesson_quality,
                    reasoning: format!("{}: P&L ${:.2} | {} lessons (avg len {:.0}) | quality {:.2}",
                        trade_id, pnl, lessons.len(),
                        if lessons.is_empty() { 0.0 } else { lessons.iter().map(|l| l.len() as f64).sum::<f64>() / lessons.len() as f64 },
                        lesson_quality),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No outcome to log".to_string(), data: None }
        }
    }
}
