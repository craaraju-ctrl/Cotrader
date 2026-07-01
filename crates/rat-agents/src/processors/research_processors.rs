//! Research Sub-Agent Processors.

use async_trait::async_trait;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct QuantResearcherProcessor;
pub struct TechnicalAnalystProcessor;
pub struct FundamentalAnalystProcessor;

#[async_trait]
impl AgentProcessor for QuantResearcherProcessor {
    fn name(&self) -> &str { "QuantResearcher" }
    fn role(&self) -> &str { "Statistical models" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.5);
                let volatility = indicators.iter().find(|(n, _)| n == "volatility").map(|(_, v)| *v).unwrap_or(0.5);
                let mean_reversion = indicators.iter().find(|(n, _)| n == "mean_reversion").map(|(_, v)| *v).unwrap_or(0.5);
                let skewness = indicators.iter().find(|(n, _)| n == "skewness").map(|(_, v)| *v).unwrap_or(0.0);
                let kurtosis = indicators.iter().find(|(n, _)| n == "kurtosis").map(|(_, v)| *v).unwrap_or(3.0);

                // Composite quant score: momentum trend + mean reversion + tail risk
                let momentum_score = momentum;
                let reversion_score = 1.0 - (mean_reversion - 0.5).abs() * 2.0; // high when near mean
                let tail_risk = if kurtosis > 5.0 { 0.8 } else if kurtosis > 4.0 { 0.5 } else { 0.2 };

                let quant_composite = (momentum_score * 0.4 + reversion_score * 0.3 + (1.0 - volatility) * 0.2 + (1.0 - tail_risk) * 0.1).clamp(0.0, 1.0);

                // Z-score interpretation
                let z_score = (momentum - 0.5) * 2.0; // normalize to roughly -1..1
                let signal_strength = z_score.abs();

                let action = if quant_composite > 0.7 && signal_strength > 0.3 {
                    "STRONG_QUANT_SIGNAL".to_string()
                } else if quant_composite > 0.5 {
                    "MODERATE_SIGNAL".to_string()
                } else {
                    "WEAK_SIGNAL".to_string()
                };

                AgentOutput {
                    action,
                    confidence: quant_composite,
                    reasoning: format!("{}: quant={:.2} | mom={:.2} revert={:.2} vol={:.2} skew={:.2} kurt={:.1} tail_risk={:.2}",
                        symbol, quant_composite, momentum_score, reversion_score, volatility, skewness, kurtosis, tail_risk),
                    data: None,
                }
            }
            AgentInput::Signal { symbol, confidence, indicators, .. } => {
                // Evaluate signal quality from quant perspective
                let indicator_count = indicators.len() as f64;
                let coverage = (indicator_count / 6.0).min(1.0);
                let adjusted = confidence * coverage;
                AgentOutput {
                    action: "VALIDATE".to_string(),
                    confidence: adjusted.clamp(0.0, 1.0),
                    reasoning: format!("{}: quant validation {:.2} (coverage {:.0}/{:.0} indicators)", symbol, adjusted, indicator_count, 6.0),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for quant analysis".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for TechnicalAnalystProcessor {
    fn name(&self) -> &str { "TechnicalAnalyst" }
    fn role(&self) -> &str { "Charts and patterns" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let rsi = indicators.iter().find(|(n, _)| n == "rsi").map(|(_, v)| *v).unwrap_or(0.5);
                let macd = indicators.iter().find(|(n, _)| n == "macd").map(|(_, v)| *v).unwrap_or(0.0);
                let ema_cross = indicators.iter().find(|(n, _)| n == "ema_cross").map(|(_, v)| *v).unwrap_or(0.5);
                let adx = indicators.iter().find(|(n, _)| n == "adx").map(|(_, v)| *v).unwrap_or(0.0);
                let bollinger = indicators.iter().find(|(n, _)| n == "bollinger").map(|(_, v)| *v).unwrap_or(0.5);
                let atr = indicators.iter().find(|(n, _)| n == "atr").map(|(_, v)| *v).unwrap_or(0.0);

                // Technical score components
                let mut bullish = 0.0;
                let mut bearish = 0.0;
                let mut total_weight = 0.0;

                // RSI (weight: 0.25)
                if rsi > 0.7 { bearish += 0.25; } else if rsi < 0.3 { bullish += 0.25; } else { bullish += (0.5 - rsi).abs() * 0.25; }
                total_weight += 0.25;

                // MACD (weight: 0.2)
                if macd > 0.0 { bullish += 0.2; } else { bearish += 0.2; }
                total_weight += 0.2;

                // EMA cross (weight: 0.2)
                if ema_cross > 0.6 { bullish += 0.2; } else if ema_cross < 0.4 { bearish += 0.2; }
                total_weight += 0.2;

                // ADX (weight: 0.15) — measures trend strength
                let trend_strength = adx;

                // Bollinger position (weight: 0.15)
                if bollinger < 0.2 { bullish += 0.15; } else if bollinger > 0.8 { bearish += 0.15; }
                total_weight += 0.15;

                let net_score = (bullish - bearish) / total_weight;
                let confidence = ((net_score.abs() * 0.5 + trend_strength * 0.3 + (1.0 - (rsi - 0.5).abs() * 2.0) * 0.2)).clamp(0.0, 1.0);

                let action = if net_score > 0.3 && trend_strength > 0.5 {
                    "STRONG_BUY".to_string()
                } else if net_score < -0.3 && trend_strength > 0.5 {
                    "STRONG_SELL".to_string()
                } else if net_score > 0.15 {
                    "LEAN_BUY".to_string()
                } else if net_score < -0.15 {
                    "LEAN_SELL".to_string()
                } else {
                    "NEUTRAL".to_string()
                };

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("{}: tech={:.2} (bull={:.2} bear={:.2}) | RSI={:.2} MACD={} EMA={} ADX={:.1} BB={:.2} ATR={:.2}",
                        symbol, net_score, bullish, bearish, rsi, if macd > 0.0 { "+" } else { "-" },
                        if ema_cross > 0.5 { "UP" } else { "DN" }, adx, bollinger, atr),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No market data for TA".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for FundamentalAnalystProcessor {
    fn name(&self) -> &str { "FundamentalAnalyst" }
    fn role(&self) -> &str { "Valuation and earnings" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let pe_ratio = indicators.iter().find(|(n, _)| n == "pe_ratio").map(|(_, v)| *v).unwrap_or(0.5);
                let earnings_growth = indicators.iter().find(|(n, _)| n == "earnings_growth").map(|(_, v)| *v).unwrap_or(0.5);
                let debt_equity = indicators.iter().find(|(n, _)| n == "debt_equity").map(|(_, v)| *v).unwrap_or(0.5);
                let dividend_yield = indicators.iter().find(|(n, _)| n == "dividend_yield").map(|(_, v)| *v).unwrap_or(0.0);
                let book_value = indicators.iter().find(|(n, _)| n == "book_value").map(|(_, v)| *v).unwrap_or(0.0);

                // Value score: low PE + high earnings growth + low debt = good
                let pe_score = 1.0 - pe_ratio.min(1.0); // low PE is good
                let growth_score = earnings_growth;
                let debt_score = 1.0 - debt_equity.min(1.0); // low debt is good
                let value_composite = (pe_score * 0.3 + growth_score * 0.3 + debt_score * 0.25 + dividend_yield * 0.15).clamp(0.0, 1.0);

                let action = if value_composite > 0.7 { "UNDERVALUED".to_string() }
                    else if value_composite > 0.55 { "FAIR_VALUE".to_string() }
                    else if value_composite < 0.3 { "OVERVALUED".to_string() }
                    else { "SLIGHTLY_OVER".to_string() };

                let confidence = value_composite;

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("{}: value={:.2} | PE={:.2} growth={:.2} debt={:.2} div={:.2} BV={:.2}",
                        symbol, value_composite, pe_ratio, earnings_growth, debt_equity, dividend_yield, book_value),
                    data: None,
                }
            }
            AgentInput::Review { lessons, .. } => {
                let fundamental_lessons: Vec<_> = lessons.iter()
                    .filter(|l| l.to_lowercase().contains("fundamental") || l.to_lowercase().contains("earnings") || l.to_lowercase().contains("valuation"))
                    .collect();
                let relevance = fundamental_lessons.len() as f64 / lessons.len().max(1) as f64;
                AgentOutput {
                    action: "LEARN".to_string(),
                    confidence: relevance,
                    reasoning: format!("Fundamental review: {:.0}% relevant lessons ({}/{})", relevance * 100.0, fundamental_lessons.len(), lessons.len()),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No data for fundamental analysis".to_string(), data: None }
        }
    }
}
