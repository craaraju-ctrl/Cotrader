//! Technology Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct SystemArchitectProcessor;
pub struct DataEngineerProcessor;
pub struct BacktestEngineProcessor;
pub struct SentimentAnalystProcessor;
pub struct RegimeDetectorProcessor;
pub struct MoneyManagerProcessor;

#[async_trait]
impl AgentProcessor for SystemArchitectProcessor {
    fn name(&self) -> &str { "SystemArchitect" }
    fn role(&self) -> &str { "System health" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { indicators, .. } => {
                // System health = data completeness
                let expected_indicators = 8;
                let completeness = (indicators.len() as f64 / expected_indicators as f64).min(1.0);
                let has_all = indicators.len() >= expected_indicators;

                let staleness = indicators.iter().any(|(n, _)| n.contains("stale"));
                let health = if has_all && !staleness { 0.95 }
                    else if completeness > 0.5 && !staleness { 0.8 }
                    else if staleness { 0.3 }
                    else { 0.5 };

                AgentOutput {
                    action: if health > 0.8 { "HEALTHY" } else if health > 0.5 { "DEGRADED" } else { "CRITICAL" }.to_string(),
                    confidence: health,
                    reasoning: format!("System health: {:.2} | {}/{} indicators | stale={}", health, indicators.len(), expected_indicators, staleness),
                    data: None,
                }
            }
            AgentInput::RiskCheck { portfolio_state, .. } => {
                let equity = portfolio_state.equity;
                let position_count = portfolio_state.positions.len();
                let health = if equity > 0.0 && position_count < 20 { 0.9 } else { 0.5 };

                AgentOutput {
                    action: if health > 0.8 { "HEALTHY" } else { "CHECK" }.to_string(),
                    confidence: health,
                    reasoning: format!("System: equity=${:.2} | {} positions | health={:.2}", equity, position_count, health),
                    data: None,
                }
            }
            _ => AgentOutput { action: "HEALTH".to_string(), confidence: 0.7, reasoning: "Default health check".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for DataEngineerProcessor {
    fn name(&self) -> &str { "DataEngineer" }
    fn role(&self) -> &str { "Data quality" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                // Data quality checks
                let price_valid = price > 0.0;
                let indicator_count = indicators.len();
                let has_zero_values = indicators.iter().any(|(_, v)| *v == 0.0);
                let has_nan = indicators.iter().any(|(_, v)| v.is_nan());
                let has_negatives = indicators.iter().any(|(n, v)| *v < 0.0 && !n.contains("pnl") && !n.contains("drawdown"));

                let mut issues = Vec::new();
                if !price_valid { issues.push("invalid_price".to_string()); }
                if has_nan { issues.push("nan_values".to_string()); }
                if has_zero_values { issues.push("zero_indicators".to_string()); }
                if has_negatives { issues.push("unexpected_negatives".to_string()); }
                if indicator_count < 3 { issues.push("low_indicator_count".to_string()); }

                let quality = if issues.is_empty() { 0.95 }
                    else if issues.len() == 1 { 0.75 }
                    else if issues.len() == 2 { 0.5 }
                    else { 0.2 };

                let action = if quality > 0.8 { "VALID" }
                    else if quality > 0.5 { "PARTIAL" }
                    else { "INVALID" };

                AgentOutput {
                    action: action.to_string(),
                    confidence: quality,
                    reasoning: format!("{}: quality={:.2} | {} indicators | price=${:.2} | issues: {}",
                        symbol, quality, indicator_count, price, if issues.is_empty() { "none".to_string() } else { issues.join(", ") }),
                    data: None,
                }
            }
            AgentInput::Outcome { symbol, pnl, entry_price, exit_price } => {
                let price_valid = entry_price > 0.0 && exit_price > 0.0;
                let pnl_consistent = if entry_price > 0.0 {
                    let expected = exit_price - entry_price;
                    (pnl > 0.0) == (expected > 0.0)
                } else {
                    false
                };

                let quality = if price_valid && pnl_consistent { 0.9 } else { 0.4 };
                AgentOutput {
                    action: if quality > 0.8 { "VALID" } else { "CHECK" }.to_string(),
                    confidence: quality,
                    reasoning: format!("{}: outcome data quality={:.2} | prices_valid={} pnl_consistent={}", symbol, quality, price_valid, pnl_consistent),
                    data: None,
                }
            }
            _ => AgentOutput { action: "VALIDATE".to_string(), confidence: 0.7, reasoning: "Default data check".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for BacktestEngineProcessor {
    fn name(&self) -> &str { "BacktestEngine" }
    fn role(&self) -> &str { "Strategy testing" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, indicators, .. } => {
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.5);
                let volatility = indicators.iter().find(|(n, _)| n == "volatility").map(|(_, v)| *v).unwrap_or(0.5);

                // Simulated backtest metrics
                let sharpe = if volatility > 0.0 { (momentum - 0.5) / volatility } else { 0.0 };
                let win_rate = 0.5 + (momentum - 0.5) * 0.3;
                let profit_factor = if (1.0 - win_rate) > 0.0 { win_rate / (1.0 - win_rate) } else { 2.0 };

                let strategy_quality = (sharpe.abs() * 0.3 + win_rate * 0.3 + (profit_factor.min(3.0) / 3.0) * 0.4).clamp(0.0, 1.0);

                let action = if strategy_quality > 0.7 { "STRONG_STRATEGY".to_string() }
                    else if strategy_quality > 0.5 { "VIABLE_STRATEGY".to_string() }
                    else { "WEAK_STRATEGY".to_string() };

                AgentOutput {
                    action,
                    confidence: strategy_quality,
                    reasoning: format!("{}: backtest quality={:.2} | sharpe={:.2} WR={:.1}% PF={:.2}",
                        symbol, strategy_quality, sharpe, win_rate * 100.0, profit_factor),
                    data: None,
                }
            }
            AgentInput::Outcome { pnl, entry_price, .. } => {
                let return_pct = if entry_price > 0.0 { pnl / entry_price * 100.0 } else { 0.0 };
                let expectancy = return_pct;

                AgentOutput {
                    action: "EVALUATE".to_string(),
                    confidence: (0.5 + (expectancy / 10.0).clamp(-0.3, 0.4)).clamp(0.0, 1.0),
                    reasoning: format!("Backtest eval: P&L ${:.2} ({:.1}%) | expectancy={:.2}%", pnl, return_pct, expectancy),
                    data: None,
                }
            }
            _ => AgentOutput { action: "BACKTEST".to_string(), confidence: 0.5, reasoning: "Default backtest".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for SentimentAnalystProcessor {
    fn name(&self) -> &str { "SentimentAnalyst" }
    fn role(&self) -> &str { "News and social sentiment" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let sentiment_score = indicators.iter().find(|(n, _)| n == "sentiment").map(|(_, v)| *v).unwrap_or(0.5);
                let social_volume = indicators.iter().find(|(n, _)| n == "social_volume").map(|(_, v)| *v).unwrap_or(0.5);
                let fear_greed = indicators.iter().find(|(n, _)| n == "fear_greed").map(|(_, v)| *v).unwrap_or(0.5);
                let news_count = indicators.iter().find(|(n, _)| n == "news_count").map(|(_, v)| *v).unwrap_or(0.0);

                // Sentiment strength: high conviction + high volume = strong signal
                let conviction = (sentiment_score - 0.5).abs() * 2.0; // 0-1
                let volume_factor = social_volume;
                let fear_greed_extreme = (fear_greed - 0.5).abs() * 2.0; // contrarian at extremes

                let sentiment_confidence = (conviction * 0.4 + volume_factor * 0.3 + fear_greed_extreme * 0.2 + (news_count / 20.0).min(1.0) * 0.1).clamp(0.0, 1.0);

                let action = if sentiment_score > 0.7 && fear_greed > 0.7 {
                    "BULLISH_EXTREME".to_string() // potential contrarian warning
                } else if sentiment_score > 0.6 {
                    "BULLISH".to_string()
                } else if sentiment_score < 0.3 && fear_greed < 0.3 {
                    "BEARISH_EXTREME".to_string() // potential contrarian opportunity
                } else if sentiment_score < 0.4 {
                    "BEARISH".to_string()
                } else {
                    "NEUTRAL".to_string()
                };

                AgentOutput {
                    action,
                    confidence: sentiment_confidence,
                    reasoning: format!("{}: sentiment={:.2} | social_vol={:.2} | fear_greed={:.2} | news={:.0} | conviction={:.2}",
                        symbol, sentiment_score, social_volume, fear_greed, news_count, conviction),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for sentiment".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for RegimeDetectorProcessor {
    fn name(&self) -> &str { "RegimeDetector" }
    fn role(&self) -> &str { "Market regime classification" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, indicators, .. } => {
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.5);
                let volatility = indicators.iter().find(|(n, _)| n == "volatility").map(|(_, v)| *v).unwrap_or(0.5);
                let trend = indicators.iter().find(|(n, _)| n == "trend").map(|(_, v)| *v).unwrap_or(0.5);
                let volume = indicators.iter().find(|(n, _)| n == "volume").map(|(_, v)| *v).unwrap_or(0.5);
                let mean_rev = indicators.iter().find(|(n, _)| n == "mean_reversion").map(|(_, v)| *v).unwrap_or(0.5);

                // Regime classification
                let regime;
                let confidence;

                if volatility > 0.7 && (momentum - 0.5).abs() < 0.15 {
                    regime = "HIGH_VOLATILITY_RANGING";
                    confidence = volatility * 0.8;
                } else if trend > 0.7 && volatility < 0.5 && momentum > 0.6 {
                    regime = "TRENDING_BULL";
                    confidence = (trend * 0.4 + (1.0 - volatility) * 0.3 + momentum * 0.3).clamp(0.0, 1.0);
                } else if trend > 0.7 && volatility < 0.5 && momentum < 0.4 {
                    regime = "TRENDING_BEAR";
                    confidence = (trend * 0.4 + (1.0 - volatility) * 0.3 + (1.0 - momentum) * 0.3).clamp(0.0, 1.0);
                } else if mean_rev > 0.6 && volatility < 0.4 {
                    regime = "MEAN_REVERTING";
                    confidence = (mean_rev * 0.5 + (1.0 - volatility) * 0.3 + volume * 0.2).clamp(0.0, 1.0);
                } else if volatility > 0.6 && trend < 0.4 {
                    regime = "CHOPPY";
                    confidence = volatility * 0.6 + (1.0 - trend) * 0.4;
                } else {
                    regime = "TRANSITIONAL";
                    confidence = 0.4;
                }

                let strategy_fit = match regime {
                    "TRENDING_BULL" | "TRENDING_BEAR" => "TREND_FOLLOWING",
                    "MEAN_REVERTING" => "MEAN_REVERSION",
                    "HIGH_VOLATILITY_RANGING" => "BREAKOUT",
                    "CHOPPY" => "SCALPING",
                    _ => "MIXED",
                };

                AgentOutput {
                    action: "DETECT".to_string(),
                    confidence: confidence.clamp(0.0, 1.0),
                    reasoning: format!("{}: regime={} ({:.2}) | mom={:.2} vol={:.2} trend={:.2} | strategy={}",
                        symbol, regime, confidence, momentum, volatility, trend, strategy_fit),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for regime detection".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for MoneyManagerProcessor {
    fn name(&self) -> &str { "MoneyManager" }
    fn role(&self) -> &str { "Position sizing" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::RiskCheck { symbol, portfolio_state, .. } => {
                let equity = portfolio_state.equity;
                let drawdown = portfolio_state.drawdown;
                let position_count = portfolio_state.positions.len() as f64;

                // Kelly criterion (simplified): assumes known win rate and payoff ratio
                let win_rate = 0.55;
                let payoff_ratio = 1.5;
                let kelly = win_rate - ((1.0 - win_rate) / payoff_ratio);

                // Conservative sizing: quarter Kelly
                let quarter_kelly = kelly * 0.25;

                // Adjust for drawdown
                let dd_adjustment = (1.0 - drawdown * 3.0).clamp(0.1, 1.0);
                let adjusted_kelly = quarter_kelly * dd_adjustment;

                // Position sizing
                let risk_per_trade = equity * adjusted_kelly;
                let max_position = equity * 0.10; // 10% hard cap
                let position_size = risk_per_trade.min(max_position);

                // Confidence based on edge quality
                let edge_quality = (kelly * payoff_ratio * (1.0 - drawdown)).clamp(0.0, 1.0);

                AgentOutput {
                    action: "SIZE".to_string(),
                    confidence: edge_quality,
                    reasoning: format!("{}: kelly={:.3} | quarter={:.4} | dd_adj={:.2} | adj_kelly={:.4} | size=${:.2} (max ${:.2}) | edge={:.2}",
                        symbol, kelly, quarter_kelly, dd_adjustment, adjusted_kelly, position_size, max_position, edge_quality),
                    data: None,
                }
            }
            AgentInput::Execution { symbol, action, size, price } => {
                let order_value = size * price;
                let max_single = 25000.0;
                let ratio = order_value / max_single;

                AgentOutput {
                    action: if ratio <= 1.0 { "APPROVE".to_string() } else { "REDUCE".to_string() },
                    confidence: if ratio <= 1.0 { 0.9 } else { (1.0 / ratio).clamp(0.2, 0.5) },
                    reasoning: format!("Money mgmt check {}: {} {:.6} @ ${:.2} = ${:.2} ({:.0}% of max)",
                        symbol, action, size, price, order_value, ratio * 100.0),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for money management".to_string(), data: None }
        }
    }
}
