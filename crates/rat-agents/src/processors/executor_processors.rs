//! Executor Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct StrategyDecisionProcessor;
pub struct PortfolioManagerProcessor;
pub struct ExecutionCoordinatorProcessor;

#[async_trait]
impl AgentProcessor for StrategyDecisionProcessor {
    fn name(&self) -> &str { "StrategyDecision" }
    fn role(&self) -> &str { "Generate trade signals" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Signal { symbol, action, confidence, indicators } => {
                let indicator_count = indicators.len() as f64;

                // Analyze indicator names for bullish/bearish bias
                let bullish_keywords = ["rsi_oversold", "macd_bullish", "ema_cross_up", "volume_surge", "breakout", "trend_up", "support_bounce"];
                let bearish_keywords = ["rsi_overbought", "macd_bearish", "ema_cross_down", "breakdown", "trend_down", "resistance_reject"];

                let mut bullish_score = 0.0;
                let mut bearish_score = 0.0;
                for ind in &indicators {
                    let lower = ind.to_lowercase();
                    if bullish_keywords.iter().any(|k| lower.contains(k)) { bullish_score += 1.0; }
                    if bearish_keywords.iter().any(|k| lower.contains(k)) { bearish_score += 1.0; }
                }

                let indicator_bias = if bullish_score + bearish_score > 0.0 {
                    (bullish_score - bearish_score) / (bullish_score + bearish_score)
                } else {
                    0.0
                };

                // Strategy decision: combine signal confidence with indicator bias
                let combined = confidence * 0.6 + ((indicator_bias + 1.0) / 2.0) * 0.4;
                let adjusted_confidence = combined.clamp(0.0, 1.0);

                // Threshold for action
                let final_action = if adjusted_confidence > 0.7 && indicator_bias > 0.2 {
                    action.clone() // pass through BUY/SELL
                } else if adjusted_confidence > 0.5 && indicator_bias > 0.1 {
                    action.clone()
                } else if adjusted_confidence < 0.3 || indicator_bias < -0.3 {
                    "HOLD".to_string()
                } else {
                    action.clone()
                };

                AgentOutput {
                    action: final_action,
                    confidence: adjusted_confidence,
                    reasoning: format!("{}: signal {} ({:.2}) | {} indicators | bullish {:.0} bearish {:.0} bias={:.2} | combined={:.2}",
                        symbol, action, confidence, indicator_count, bullish_score, bearish_score, indicator_bias, adjusted_confidence),
                    data: None,
                }
            }
            AgentInput::MarketData { symbol, price, indicators } => {
                let avg = indicators.iter().map(|(_, v)| v).sum::<f64>() / indicators.len().max(1) as f64;
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.5);

                let action = if avg > 0.65 && momentum > 0.6 { "BUY".to_string() }
                    else if avg < 0.35 && momentum < 0.4 { "SELL".to_string() }
                    else { "HOLD".to_string() };

                AgentOutput {
                    action,
                    confidence: avg,
                    reasoning: format!("{}: strategy from market data avg={:.2} mom={:.2}", symbol, avg, momentum),
                    data: None,
                }
            }
            _ => AgentOutput { action: "HOLD".to_string(), confidence: 0.0, reasoning: "No signal data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for PortfolioManagerProcessor {
    fn name(&self) -> &str { "PortfolioManager" }
    fn role(&self) -> &str { "Position sizing and risk allocation" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { symbol, portfolio_state, .. } => {
                let equity = portfolio_state.equity;
                let drawdown = portfolio_state.drawdown;
                let position_count = portfolio_state.positions.len() as f64;

                // Portfolio heat: total unrealized risk relative to equity
                let heat: f64 = portfolio_state.positions.iter()
                    .map(|p| p.unrealized_pnl.abs())
                    .sum::<f64>() / equity.max(1.0);

                // Exposure breakdown
                let long_exposure: f64 = portfolio_state.positions.iter()
                    .filter(|p| p.side == "LONG")
                    .map(|p| p.size)
                    .sum();
                let short_exposure: f64 = portfolio_state.positions.iter()
                    .filter(|p| p.side == "SHORT")
                    .map(|p| p.size)
                    .sum();

                // Position limits based on drawdown
                let max_positions = if drawdown > 0.10 { 3 }
                    else if drawdown > 0.05 { 6 }
                    else { 10 };

                // Risk budget per position
                let risk_budget = equity * 0.02 * (1.0 - drawdown).clamp(0.2, 1.0);

                // Can we add to this symbol?
                let existing = portfolio_state.positions.iter().find(|p| p.symbol == symbol);
                let can_add = existing.is_none() || position_count < max_positions as f64;

                let sizing_confidence = if heat > 0.15 { 0.2 }
                    else if heat > 0.10 { 0.4 }
                    else if position_count > max_positions as f64 { 0.3 }
                    else if drawdown > 0.10 { 0.4 }
                    else { 0.85 };

                let action = if !can_add {
                    "BLOCK_NEW_POSITION".to_string()
                } else if heat > 0.15 {
                    "REDUCE_ALL".to_string()
                } else {
                    "SIZE".to_string()
                };

                AgentOutput {
                    action,
                    confidence: sizing_confidence,
                    reasoning: format!("{}: heat={:.2} | equity=${:.2} | {} positions (max {}) | long=${:.2} short=${:.2} | risk_budget=${:.2} | DD={:.1}%",
                        symbol, heat, equity, position_count as u32, max_positions,
                        long_exposure, short_exposure, risk_budget, drawdown * 100.0),
                    data: None,
                }
            }
            AgentInput::Execution { symbol, action, size, price } => {
                let order_value = size * price;
                let max_order = 25000.0;
                let ratio = order_value / max_order;

                let action_ok = ratio <= 1.0;
                AgentOutput {
                    action: if action_ok { "APPROVE_SIZE".to_string() } else { "REDUCE_SIZE".to_string() },
                    confidence: if action_ok { 0.9 } else { (1.0 / ratio).clamp(0.2, 0.5) },
                    reasoning: format!("Size check {}: {} {:.6} @ ${:.2} = ${:.2} ({:.0}% of max)", symbol, action, size, price, order_value, ratio * 100.0),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for PM sizing".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for ExecutionCoordinatorProcessor {
    fn name(&self) -> &str { "ExecutionCoordinator" }
    fn role(&self) -> &str { "Order routing and settlement" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Execution { symbol, action, size, price } => {
                let order_value = size * price;

                // Execution quality checks
                let size_ok = order_value < 50000.0 && size > 0.0;
                let price_ok = price > 0.0;

                // Slippage estimation: assume 0.05% for small orders, more for large
                let slippage_bps = if order_value > 25000.0 { 15.0 }
                    else if order_value > 10000.0 { 8.0 }
                    else { 3.0 };

                let estimated_slippage = price * slippage_bps / 10000.0;
                let fill_price = if action == "BUY" {
                    price + estimated_slippage
                } else {
                    price - estimated_slippage
                };

                // Execution confidence based on order quality
                let exec_confidence = if size_ok && price_ok { 0.9 }
                    else if !size_ok { 0.3 }
                    else { 0.5 };

                let route = if order_value > 25000.0 { "ICEBERG" }
                    else if order_value > 10000.0 { "TWAP" }
                    else { "MARKET" };

                AgentOutput {
                    action: if size_ok && price_ok { action.clone() } else { "REJECT".to_string() },
                    confidence: exec_confidence,
                    reasoning: format!("Exec {}: {} {:.6} @ ${:.2} = ${:.2} | route={} | slip {:.1}bps (~${:.4}) | fill ~${:.2}",
                        symbol, action, size, price, order_value, route, slippage_bps, estimated_slippage, fill_price),
                    data: None,
                }
            }
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let return_pct = if entry_price > 0.0 { (exit_price - entry_price) / entry_price * 100.0 } else { 0.0 };

                AgentOutput {
                    action: "SETTLED".to_string(),
                    confidence: 0.95,
                    reasoning: format!("{}: settled | P&L ${:.2} ({:.1}%) | entry=${:.2} exit=${:.2}",
                        symbol, pnl, return_pct, entry_price, exit_price),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No execution data".to_string(), data: None }
        }
    }
}
