//! Signal Flow — Indicator → Signal generation pipeline.
//!
//! Runs real indicators and combines into actionable signals.

pub struct SignalFlow;

impl SignalFlow {
    /// Run all indicators and combine into a single signal.
    pub async fn generate_signal(
        symbol: &str,
        prices: &[f64],
        highs: &[f64],
        lows: &[f64],
        volumes: &[f64],
    ) -> super::pipeline::SignalOutput {
        let mut scores = Vec::new();

        // RSI (14-period)
        if prices.len() >= 15 {
            let rsi = rat_indicators::rsi::RsiIndicator::new(14);
            let rsi_values = rsi.calculate(prices);
            if let Some(&last_rsi) = rsi_values.last() {
                let rsi_score = (100.0 - last_rsi) / 100.0; // Invert: low RSI = bullish
                scores.push(("RSI", rsi_score, 0.25));
            }
        }

        // MACD (12, 26, 9)
        if prices.len() >= 35 {
            let macd = rat_indicators::macd::MacdIndicator::new(12, 26, 9);
            let result = macd.calculate(prices);
            if let (Some(&macd_val), Some(&signal_val)) = (result.macd_line.last(), result.signal_line.last()) {
                let macd_score = if macd_val > signal_val { 0.7 } else { 0.3 };
                scores.push(("MACD", macd_score, 0.20));
            }
        }

        // ATR (14-period)
        if highs.len() >= 15 {
            let atr = rat_indicators::atr::AtrIndicator::new(14);
            let atr_values = atr.calculate(highs, lows, prices);
            if let Some(&last_atr) = atr_values.last() {
                let atr_pct = atr.atr_pct(last_atr, *prices.last().unwrap_or(&1.0));
                let atr_score = if atr_pct > 3.0 { 0.3 } else { 0.5 }; // High vol = cautious
                scores.push(("ATR", atr_score, 0.15));
            }
        }

        // Bollinger Bands (20, 2.0)
        if prices.len() >= 20 {
            let bollinger = rat_indicators::bollinger::BollingerIndicator::new(20, 2.0);
            let bands = bollinger.calculate(prices);
            if let (Some(&upper), Some(&lower), Some(&middle)) = 
                (bands.upper.last(), bands.lower.last(), bands.middle.last()) {
                let price = *prices.last().unwrap_or(&1.0);
                let bb_score = if price < lower { 0.8 } else if price > upper { 0.2 } else { 0.5 };
                scores.push(("Bollinger", bb_score, 0.20));
            }
        }

        // Stochastic (14, 3, 3)
        if highs.len() >= 14 {
            let stoch = rat_indicators::stochastic::StochasticIndicator::new(14, 3, 3);
            let result = stoch.calculate(highs, lows, prices);
            if let (Some(&k), Some(&d)) = (result.k.last(), result.d.last()) {
                let stoch_score = if k < 20.0 && d < 20.0 { 0.8 } else if k > 80.0 && d > 80.0 { 0.2 } else { 0.5 };
                scores.push(("Stochastic", stoch_score, 0.20));
            }
        }

        // Combine weighted scores
        let total_weight: f64 = scores.iter().map(|(_, _, w)| w).sum();
        let weighted_score: f64 = scores.iter().map(|(_, s, w)| s * w).sum();
        let avg_score = if total_weight > 0.0 { weighted_score / total_weight } else { 0.5 };

        // Generate action based on combined score
        let action = if avg_score > 0.65 {
            "BUY".to_string()
        } else if avg_score < 0.35 {
            "SELL".to_string()
        } else {
            "HOLD".to_string()
        };

        let reasoning = scores.iter()
            .map(|(name, score, _)| format!("{}: {:.2}", name, score))
            .collect::<Vec<_>>()
            .join(", ");

        super::pipeline::SignalOutput {
            action,
            confidence: avg_score,
            reasoning: format!("Combined: {} (final: {:.2})", reasoning, avg_score),
        }
    }
}
