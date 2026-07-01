//! Identifier Sub-Agent Processors.

use async_trait::async_trait;
use chrono::Timelike;
use crate::traits::{AgentProcessor, AgentInput, AgentOutput};

pub struct WatchlistScannerProcessor;
pub struct MarketIntelligenceProcessor;
pub struct PivotCalculatorProcessor;
pub struct ConfluenceScorerProcessor;
pub struct PatternRetrieverProcessor;
pub struct SessionTimerProcessor;
pub struct RedFolderCheckerProcessor;

#[async_trait]
impl AgentProcessor for WatchlistScannerProcessor {
    fn name(&self) -> &str { "WatchlistScanner" }
    fn role(&self) -> &str { "Scan watchlist for opportunities" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.5);
                let volume = indicators.iter().find(|(n, _)| n == "volume").map(|(_, v)| *v).unwrap_or(0.5);
                let volatility = indicators.iter().find(|(n, _)| n == "volatility").map(|(_, v)| *v).unwrap_or(0.5);
                let trend = indicators.iter().find(|(n, _)| n == "trend").map(|(_, v)| *v).unwrap_or(0.5);

                let opportunity_score = (momentum * 0.35 + volume * 0.25 + trend * 0.25 + (1.0 - volatility) * 0.15).clamp(0.0, 1.0);

                let action = if opportunity_score > 0.7 {
                    "STRONG_OPPORTUNITY".to_string()
                } else if opportunity_score > 0.5 {
                    "OPPORTUNITY".to_string()
                } else if opportunity_score > 0.3 {
                    "WATCH".to_string()
                } else {
                    "PASS".to_string()
                };

                let confidence = if price > 0.0 && !indicators.is_empty() { opportunity_score } else { 0.0 };

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("{}: opp={:.2} (mom={:.2}, vol={:.2}, trend={:.2}, price=${:.2})",
                        symbol, opportunity_score, momentum, volume, trend, price),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No market data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for MarketIntelligenceProcessor {
    fn name(&self) -> &str { "MarketIntelligence" }
    fn role(&self) -> &str { "Aggregate market signals" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let bullish_count = indicators.iter().filter(|(n, v)| *v > 0.6 && (n.contains("rsi") || n.contains("macd") || n.contains("trend") || n.contains("adx"))).count();
                let bearish_count = indicators.iter().filter(|(n, v)| *v < 0.4 && (n.contains("rsi") || n.contains("macd") || n.contains("trend") || n.contains("adx"))).count();
                let total = indicators.len().max(1) as f64;
                let avg = indicators.iter().map(|(_, v)| v).sum::<f64>() / total;

                // Direction confidence: how many indicators agree with the direction
                let direction_agreement = if avg > 0.5 {
                    bullish_count as f64 / total
                } else {
                    bearish_count as f64 / total
                };
                let confidence = (avg * 0.5 + direction_agreement * 0.5).clamp(0.0, 1.0);

                let action = if avg > 0.65 && direction_agreement > 0.6 {
                    "STRONG_BUY".to_string()
                } else if avg < 0.35 && direction_agreement > 0.6 {
                    "STRONG_SELL".to_string()
                } else if avg > 0.55 {
                    "LEAN_BUY".to_string()
                } else if avg < 0.45 {
                    "LEAN_SELL".to_string()
                } else {
                    "NEUTRAL".to_string()
                };

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("{}: avg={:.2} | {} bullish, {} bearish, agreement={:.2}",
                        symbol, avg, bullish_count, bearish_count, direction_agreement),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No market data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for PivotCalculatorProcessor {
    fn name(&self) -> &str { "PivotCalculator" }
    fn role(&self) -> &str { "Compute support/resistance" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let atr = indicators.iter().find(|(n, _)| n == "atr").map(|(_, v)| *v).unwrap_or(0.0);
                let high = indicators.iter().find(|(n, _)| n == "high").map(|(_, v)| *v).unwrap_or(price);
                let low = indicators.iter().find(|(n, _)| n == "low").map(|(_, v)| *v).unwrap_or(price);
                let atr_val = if atr > 0.0 { atr } else { (high - low).max(0.01) };

                let pivot = (high + low + price) / 3.0;
                let support1 = 2.0 * pivot - high;
                let support2 = pivot - (high - low);
                let resistance1 = 2.0 * pivot - low;
                let resistance2 = pivot + (high - low);

                let dist_to_support = if support1 > 0.0 { ((price - support1) / price * 100.0).abs() } else { 10.0 };
                let dist_to_resistance = if resistance1 > 0.0 { ((resistance1 - price) / price * 100.0).abs() } else { 10.0 };
                let zone_proximity = 1.0 - (dist_to_support.min(dist_to_resistance) / 5.0).min(1.0);
                let confidence = (0.5 + zone_proximity * 0.4).clamp(0.0, 1.0);

                let action = if price <= support1 * 1.005 {
                    "AT_SUPPORT".to_string()
                } else if price >= resistance1 * 0.995 {
                    "AT_RESISTANCE".to_string()
                } else if price > pivot {
                    "ABOVE_PIVOT".to_string()
                } else {
                    "BELOW_PIVOT".to_string()
                };

                AgentOutput {
                    action,
                    confidence,
                    reasoning: format!("{}: pivot=${:.2} | S1=${:.2} S2=${:.2} | R1=${:.2} R2=${:.2} | ATR=${:.2}",
                        symbol, pivot, support1, support2, resistance1, resistance2, atr_val),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No market data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for ConfluenceScorerProcessor {
    fn name(&self) -> &str { "ConfluenceScorer" }
    fn role(&self) -> &str { "Score multi-factor confluence" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::Signal { symbol, action, confidence: signal_confidence, indicators } => {
                if indicators.is_empty() {
                    return AgentOutput {
                        action: "SCORE".to_string(),
                        confidence: 0.0,
                        reasoning: format!("{}: no indicators, cannot score", symbol),
                        data: None,
                    };
                }

                // Score based on indicator count, agreement, and signal confidence
                let count_score = (indicators.len() as f64 / 6.0).min(1.0);

                // Analyze indicator names for bullish/bearish bias
                let bullish_names = ["rsi", "macd", "ema", "adx", "trend", "volume", "momentum", "ichimoku", "supertrend"];
                let mut bullish = 0;
                let mut bearish = 0;
                for ind in &indicators {
                    let lower = ind.to_lowercase();
                    if bullish_names.iter().any(|n| lower.contains(n)) {
                        // Assume higher-indicator-name means bullish context
                        bullish += 1;
                    } else {
                        bearish += 1;
                    }
                }
                let agreement = if bullish + bearish > 0 {
                    (bullish.max(bearish) as f64) / (bullish + bearish) as f64
                } else {
                    0.5
                };

                let confluence = (signal_confidence * 0.4 + count_score * 0.3 + agreement * 0.3).clamp(0.0, 1.0);

                let strength = if confluence > 0.8 { "VERY_STRONG" }
                    else if confluence > 0.65 { "STRONG" }
                    else if confluence > 0.45 { "MODERATE" }
                    else { "WEAK" };

                AgentOutput {
                    action: "SCORE".to_string(),
                    confidence: confluence,
                    reasoning: format!("{}: {} ({:.2}) | {} indicators | agreement {:.2} | signal_conf {:.2}",
                        symbol, strength, confluence, indicators.len(), agreement, signal_confidence),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No signal to score".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for PatternRetrieverProcessor {
    fn name(&self) -> &str { "PatternRetriever" }
    fn role(&self) -> &str { "Match historical patterns" }
    async fn process(&self, input: AgentInput) -> AgentOutput {
        match input {
            AgentInput::MarketData { symbol, price, indicators } => {
                let momentum = indicators.iter().find(|(n, _)| n == "momentum").map(|(_, v)| *v).unwrap_or(0.5);
                let volatility = indicators.iter().find(|(n, _)| n == "volatility").map(|(_, v)| *v).unwrap_or(0.5);
                let trend = indicators.iter().find(|(n, _)| n == "trend").map(|(_, v)| *v).unwrap_or(0.5);
                let volume = indicators.iter().find(|(n, _)| n == "volume").map(|(_, v)| *v).unwrap_or(0.5);

                // Pattern classification based on indicator fingerprint
                let pattern_name;
                let pattern_confidence;

                if momentum > 0.7 && trend > 0.65 && volatility < 0.5 {
                    pattern_name = "BULLISH_BREAKOUT".to_string();
                    pattern_confidence = (momentum * 0.4 + trend * 0.3 + (1.0 - volatility) * 0.3).clamp(0.0, 1.0);
                } else if momentum < 0.3 && trend < 0.35 && volatility < 0.5 {
                    pattern_name = "BEARISH_BREAKDOWN".to_string();
                    pattern_confidence = ((1.0 - momentum) * 0.4 + (1.0 - trend) * 0.3 + (1.0 - volatility) * 0.3).clamp(0.0, 1.0);
                } else if volatility > 0.7 && (momentum - 0.5).abs() < 0.15 {
                    pattern_name = "CONSOLIDATION".to_string();
                    pattern_confidence = volatility * 0.6 + (1.0 - (momentum - 0.5).abs() * 2.0) * 0.4;
                } else if volume > 0.7 && (momentum > 0.6 || momentum < 0.4) {
                    pattern_name = "VOLUME_SPIKE".to_string();
                    pattern_confidence = volume * 0.5 + (momentum * 0.3 + trend * 0.2);
                } else if trend > 0.6 && volatility < 0.4 {
                    pattern_name = "TRENDING".to_string();
                    pattern_confidence = trend * 0.5 + (1.0 - volatility) * 0.3 + momentum * 0.2;
                } else {
                    pattern_name = "UNCLASSIFIED".to_string();
                    pattern_confidence = 0.2;
                }

                let confidence = (pattern_confidence * 0.7 + 0.3).clamp(0.0, 1.0);

                AgentOutput {
                    action: "MATCH".to_string(),
                    confidence,
                    reasoning: format!("{}: pattern={} ({:.2}) | mom={:.2} vol={:.2} trend={:.2} vol_spd={:.2}",
                        symbol, pattern_name, pattern_confidence, momentum, volatility, trend, volume),
                    data: None,
                }
            }
            _ => AgentOutput { action: "PASS".to_string(), confidence: 0.0, reasoning: "No market data".to_string(), data: None }
        }
    }
}

#[async_trait]
impl AgentProcessor for SessionTimerProcessor {
    fn name(&self) -> &str { "SessionTimer" }
    fn role(&self) -> &str { "Track market sessions" }
    async fn process(&self, _input: AgentInput) -> AgentOutput {
        let hour = chrono::Utc::now().hour();
        let minute = chrono::Utc::now().minute();
        let time_decimal = hour as f64 + minute as f64 / 60.0;

        // London: 07:00-16:00 UTC, NY: 12:00-21:00 UTC, Asian: 23:00-07:00 UTC
        let london_active = time_decimal >= 7.0 && time_decimal < 16.0;
        let ny_active = time_decimal >= 12.0 && time_decimal < 21.0;
        let asian_active = time_decimal >= 23.0 || time_decimal < 7.0;
        let overlap = london_active && ny_active;

        // Session quality score
        let mut session_quality = 0.0;
        let mut session_name = "CLOSED".to_string();
        if overlap {
            session_quality = 1.0;
            session_name = "LONDON_NY_OVERLAP".to_string();
        } else if ny_active {
            session_quality = 0.85;
            session_name = "NEW_YORK".to_string();
        } else if london_active {
            session_quality = 0.75;
            session_name = "LONDON".to_string();
        } else if asian_active {
            session_quality = 0.45;
            session_name = "ASIAN".to_string();
        }

        // Adjust for session proximity (ramp-up/ramp-down)
        let proximity_bonus: f64 = if london_active && time_decimal < 8.0 { 0.7 } else if ny_active && time_decimal < 13.0 { 0.8 } else if london_active && time_decimal > 15.0 { 0.6 } else { 1.0 };
        let final_quality: f64 = (session_quality * proximity_bonus).clamp(0.0, 1.0);

        AgentOutput {
            action: "CHECK".to_string(),
            confidence: final_quality,
            reasoning: format!("Session: {} | Quality: {:.2} | UTC {:02}:{:02} | London={} NY={} Asian={}",
                session_name, final_quality, hour, minute, london_active, ny_active, asian_active),
            data: None,
        }
    }
}

#[async_trait]
impl AgentProcessor for RedFolderCheckerProcessor {
    fn name(&self) -> &str { "RedFolderChecker" }
    fn role(&self) -> &str { "Check high-impact events" }
    async fn process(&self, _input: AgentInput) -> AgentOutput {
        let hour = chrono::Utc::now().hour();
        let minute = chrono::Utc::now().minute();
        let time_decimal = hour as f64 + minute as f64 / 60.0;

        // Known high-impact news windows (UTC) — US CPI, NFP, FOMC typically at 12:30 or 14:00
        let high_impact_windows = [(12.0, 13.5, "US_DATA"), (14.0, 15.0, "FED_SPEECH"), (7.0, 8.5, "EU_DATA"), (23.0, 24.0, "ASIA_DATA")];

        let mut approaching = false;
        let mut event_name = "NONE".to_string();
        let mut minutes_until = 999.0;

        for (start, end, name) in high_impact_windows {
            if time_decimal >= start - 0.5 && time_decimal < end {
                let dist = if time_decimal < start { (start - time_decimal) * 60.0 } else { 0.0 };
                if dist < minutes_until || (time_decimal >= start && time_decimal < end) {
                    minutes_until = dist;
                    event_name = name.to_string();
                    approaching = time_decimal < start;
                }
            }
        }

        let danger_level = if approaching && minutes_until < 30.0 {
            0.9
        } else if approaching && minutes_until < 60.0 {
            0.7
        } else if !approaching && event_name != "NONE" {
            0.5
        } else {
            0.1
        };

        let action = if danger_level > 0.8 { "PAUSE_TRADING".to_string() }
            else if danger_level > 0.6 { "REDUCE_SIZE".to_string() }
            else if danger_level > 0.3 { "CAUTION".to_string() }
            else { "CLEAR".to_string() };

        AgentOutput {
            action,
            confidence: 1.0 - danger_level,
            reasoning: format!("Red folder: event={} | danger={:.2} | approaching={} | {:.0}min away | UTC {:02}:{:02}",
                event_name, danger_level, approaching, minutes_until, hour, minute),
            data: None,
        }
    }
}
