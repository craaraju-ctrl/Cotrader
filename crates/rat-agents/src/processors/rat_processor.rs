//! Rat (CIO) — Top-level orchestrator processor.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct RatProcessor;

#[async_trait]
impl AgentProcessor for RatProcessor {
    fn name(&self) -> &str { "Rat" }
    fn role(&self) -> &str { "Chief Investment Officer" }
    
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { symbol, signal, portfolio_state } => {
                let drawdown = portfolio_state.drawdown;
                let daily_pnl = portfolio_state.daily_pnl;
                let equity = portfolio_state.equity;
                let position_count = portfolio_state.positions.len();
                let total_exposure: f64 = portfolio_state.positions.iter().map(|p| p.size.abs()).sum();
                let exposure_ratio = total_exposure / equity.max(1.0);

                // Risk budget calculation
                let base_risk_budget = 0.02; // 2% per trade
                let drawdown_factor = (1.0 - drawdown).clamp(0.1, 1.0);
                let heat_penalty = (exposure_ratio / 2.0).min(0.5);
                let risk_budget = base_risk_budget * drawdown_factor * (1.0 - heat_penalty);
                let max_daily_loss = equity * 0.05;

                // Decision logic
                let mut confidence = 0.5;
                let mut action = "HOLD".to_string();
                let mut reasons = Vec::new();

                if drawdown > 0.15 {
                    reasons.push(format!("CRITICAL drawdown {:.1}%", drawdown * 100.0));
                    action = "REJECT".to_string();
                    confidence = 0.1;
                } else if drawdown > 0.10 {
                    reasons.push(format!("High drawdown {:.1}%", drawdown * 100.0));
                    if signal == "SELL" {
                        action = "APPROVE".to_string();
                        reasons.push("Allowing short in drawdown for hedging".to_string());
                        confidence = 0.7;
                    } else {
                        action = "REDUCE_SIZE".to_string();
                        confidence = 0.4;
                        reasons.push("Reducing position sizes".to_string());
                    }
                } else if daily_pnl < -max_daily_loss {
                    reasons.push(format!("Daily loss limit hit: ${:.2}", daily_pnl));
                    action = "REJECT".to_string();
                    confidence = 0.15;
                } else if exposure_ratio > 1.5 {
                    reasons.push(format!("Over-leveraged: {:.1}x exposure", exposure_ratio));
                    action = "REDUCE_SIZE".to_string();
                    confidence = 0.3;
                } else if position_count > 12 {
                    reasons.push(format!("Too many positions: {}", position_count));
                    action = "SELECTIVE".to_string();
                    confidence = 0.4;
                } else {
                    action = "APPROVE".to_string();
                    reasons.push("Risk budget within limits".to_string());
                    confidence = (drawdown_factor * (1.0 - heat_penalty)).clamp(0.3, 0.95);
                }

                let confidence = confidence.clamp(0.0, 1.0);
                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("CIO for {}: {}. Risk budget: {:.2}% equity | DD: {:.1}% | Positions: {} | Exposure: {:.1}x | Reasons: {}",
                        symbol, if confidence > 0.5 { "APPROVED" } else { "RESTRICTED" },
                        risk_budget * 100.0, drawdown * 100.0, position_count, exposure_ratio,
                        reasons.join("; ")),
                    data: None,
                }
            }
            AgentInput::MarketData { symbol, price, indicators } => {
                let avg = indicators.iter().map(|(_, v)| v).sum::<f64>() / indicators.len().max(1) as f64;
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.0);
                let volatility = indicators.iter().find(|(n, _)| n == "volatility").map(|(_, v)| *v).unwrap_or(0.5);

                let risk_adj_confidence = avg * (1.0 - volatility * 0.3);
                let action = if avg > 0.7 && volatility < 0.6 {
                    "BUY".to_string()
                } else if avg < 0.3 && volatility < 0.6 {
                    "SELL".to_string()
                } else if volatility > 0.8 {
                    "HOLD".to_string()
                } else {
                    "WATCH".to_string()
                };

                AgentOutput {
                    action,
                    confidence: risk_adj_confidence.clamp(0.0, 1.0),
                    reasoning: format!("CIO {}: momentum {:.2}, vol {:.2}, composite {:.2}", symbol, momentum, volatility, risk_adj_confidence),
                    data: None,
                }
            }
            AgentInput::Execution { symbol, action, size, price } => {
                let value = size * price;
                let size_ok = value < 50000.0;
                let action_str = if action == "BUY" || action == "SELL" {
                    if size_ok { "EXECUTE".to_string() } else { "REDUCE_SIZE".to_string() }
                } else {
                    action.clone()
                };

                AgentOutput {
                    action: action_str,
                    confidence: if size_ok { 0.85 } else { 0.4 },
                    reasoning: format!("CIO exec review {}: {} {:.6} @ ${:.2} = ${:.2}", symbol, action, size, price, value),
                    data: None,
                }
            }
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let return_pct = if entry_price > 0.0 { ((exit_price - entry_price) / entry_price * 100.0).abs() } else { 0.0 };
                let is_win = pnl > 0.0;
                AgentOutput {
                    action: "REVIEW".to_string(),
                    confidence: if is_win { 0.8 } else { 0.6 },
                    reasoning: format!("CIO outcome {}: {} P&L ${:.2} ({:.1}%)", symbol, if is_win { "WIN" } else { "LOSS" }, pnl, return_pct),
                    data: None,
                }
            }
            AgentInput::Review { trade_id, pnl, lessons } => {
                let severity = if pnl < -100.0 { "HIGH" } else if pnl < -20.0 { "MEDIUM" } else { "LOW" };
                AgentOutput {
                    action: "ACKNOWLEDGE".to_string(),
                    confidence: 0.9,
                    reasoning: format!("CIO review {}: {} P&L ${:.2}, {} priority. Lessons: {}",
                        trade_id, if pnl > 0.0 { "Profitable" } else { "Loss" }, pnl, severity, lessons.join(", ")),
                    data: None,
                }
            }
            AgentInput::Signal { symbol, action, confidence, indicators } => {
                let adjusted = confidence * 0.8;
                AgentOutput {
                    action: action.clone(),
                    confidence: adjusted.clamp(0.0, 1.0),
                    reasoning: format!("CIO signal {}: {} (adj {:.2}) from {} indicators", symbol, action, adjusted, indicators.len()),
                    data: None,
                }
            }
        }
    }
}
