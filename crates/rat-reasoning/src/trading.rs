use crate::chain::{ChainOfThought, State};
use crate::error::ReasoningError;
use crate::self_consistency::{SignalDirection, trading_vote};

/// Complete trading reasoning pipeline — composes all chain types.
pub struct TradingReasoningEngine;

impl TradingReasoningEngine {
    /// Build the full 5-layer reasoning pipeline for a single symbol.
    pub fn build_pipeline() -> ChainOfThought {
        ChainOfThought::new()
            .step("load_market_data", |state| {
                let symbol = state.get("symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("BTC")
                    .to_string();
                state.insert("symbol_loaded".into(), serde_json::json!(symbol));
                state.insert("data_ready".into(), serde_json::json!(true));
                Ok(())
            })
            .step("compute_indicators", |state| {
                let price = state.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let rsi = compute_rsi_from_price(price);
                let trend = if price > 0.0 { "up" } else { "down" };
                state.insert("rsi".into(), serde_json::json!(rsi));
                state.insert("trend".into(), serde_json::json!(trend));
                Ok(())
            })
            .step("generate_signal", |state| {
                let rsi = state.get("rsi").and_then(|v| v.as_f64()).unwrap_or(50.0);
                let trend = state.get("trend").and_then(|v| v.as_str()).unwrap_or("flat");

                let (signal, confidence) = if rsi > 70.0 && trend == "up" {
                    ("HOLD", 0.6)
                } else if rsi > 70.0 {
                    ("SELL", 0.7)
                } else if rsi < 30.0 && trend == "down" {
                    ("HOLD", 0.6)
                } else if rsi < 30.0 {
                    ("BUY", 0.7)
                } else {
                    ("HOLD", 0.5)
                };

                state.insert("signal".into(), serde_json::json!(signal));
                state.insert("confidence".into(), serde_json::json!(confidence));
                Ok(())
            })
            .step("self_consistency_check", |state| {
                let signal = state.get("signal").and_then(|v| v.as_str()).unwrap_or("HOLD");
                let confidence = state.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);

                // Simulate 5 analysis paths voting
                let dir = match signal {
                    "BUY" => SignalDirection::Buy,
                    "SELL" => SignalDirection::Sell,
                    _ => SignalDirection::Hold,
                };
                let result = trading_vote(&dir, &dir, &dir, &SignalDirection::Hold, &dir);
                let consensus = result.is_consensus();
                let vote_confidence = result.confidence();

                state.insert("consensus".into(), serde_json::json!(consensus));
                state.insert("vote_confidence".into(), serde_json::json!(vote_confidence));

                // Override signal if no consensus
                if !consensus && confidence < 0.6 {
                    state.insert("signal".into(), serde_json::json!("HOLD"));
                }
                Ok(())
            })
            .step("risk_check", |state| {
                let confidence = state.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
                let signal = state.get("signal").and_then(|v| v.as_str()).unwrap_or("HOLD");

                if signal != "HOLD" && confidence < 0.4 {
                    state.insert("signal".into(), serde_json::json!("HOLD"));
                    state.insert("blocked_reason".into(), serde_json::json!("confidence below threshold"));
                }
                Ok(())
            })
            .step("size_position", |state| {
                let signal = state.get("signal").and_then(|v| v.as_str()).unwrap_or("HOLD");
                let confidence = state.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);

                let size = if signal == "HOLD" {
                    0.0
                } else {
                    // Fractional Kelly — conservative
                    let kelly_fraction = confidence * 0.25;
                    kelly_fraction.min(0.1) // max 10% per trade
                };

                state.insert("position_size".into(), serde_json::json!(size));
                Ok(())
            })
            .step("generate_reasoning", |state| {
                let signal = state.get("signal").and_then(|v| v.as_str()).unwrap_or("HOLD");
                let confidence = state.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
                let rsi = state.get("rsi").and_then(|v| v.as_f64()).unwrap_or(50.0);
                let consensus = state.get("consensus").and_then(|v| v.as_bool()).unwrap_or(false);

                let reasoning = format!(
                    "Signal: {} ({:.0}% confidence) | RSI: {:.0} | Consensus: {}",
                    signal, confidence * 100.0, rsi, consensus
                );
                state.insert("reasoning".into(), serde_json::json!(reasoning));
                Ok(())
            })
    }

    /// Execute the full pipeline for a symbol.
    pub fn analyze(symbol: &str, price: f64) -> Result<State, ReasoningError> {
        let pipeline = Self::build_pipeline();
        let mut state = State::new();
        state.insert("symbol".into(), serde_json::json!(symbol));
        state.insert("price".into(), serde_json::json!(price));
        pipeline.run(state)
    }
}

fn compute_rsi_from_price(_price: f64) -> f64 {
    // Simplified RSI — in production, use rat_indicators
    50.0
}

/// Run reflexion on a trading strategy's backtest results.
pub fn strategy_reflexion(backtest_results: &State) -> Result<State, ReasoningError> {
    use crate::reflexion::{ReflexionLoop, trading_reflect, apply_reflections};

    let loop_engine = ReflexionLoop::new(5);
    let (improved, reflections) = loop_engine.run(
        backtest_results,
        |state, reflections| {
            let mut new_state = state.clone();
            apply_reflections(&mut new_state, reflections);
            let score = evaluate_strategy_state(&new_state);
            Ok((new_state, score))
        },
        |state, score| trading_reflect(state, score),
    )?;

    let mut result = improved;
    result.insert("reflections_applied".into(), serde_json::json!(reflections.len()));
    Ok(result)
}

fn evaluate_strategy_state(state: &State) -> f64 {
    let win_rate = state.get("win_rate").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let sharpe = state.get("sharpe").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_dd = state.get("max_drawdown").and_then(|v| v.as_f64()).unwrap_or(0.5);

    let wr_score = win_rate;
    let sharpe_score = (sharpe / 2.0).min(1.0);
    let dd_score = (1.0 - max_dd).max(0.0);

    (wr_score * 0.4 + sharpe_score * 0.3 + dd_score * 0.3).min(1.0)
}
