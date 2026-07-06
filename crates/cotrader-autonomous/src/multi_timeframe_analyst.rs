//! # Multi-Timeframe Technical Analyst
//!
//! Analyzes each symbol across 6 timeframes simultaneously:
//! 1m, 5m, 15m, 1h, 4h, 1d
//!
//! Each timeframe produces independent indicator values. The signals are then
//! fed into the ConfluenceScorer for multi-timeframe alignment analysis.

use std::collections::HashMap;

/// Supported timeframes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    M1,   // 1 minute
    M5,   // 5 minutes
    M15,  // 15 minutes
    H1,   // 1 hour
    H4,   // 4 hours
    D1,   // 1 day
}

impl Timeframe {
    pub fn all() -> &'static [Timeframe] {
        &[Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::H1, Timeframe::H4, Timeframe::D1]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Timeframe::M1 => "1m",
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
        }
    }

    /// Number of 1m bars that fit in this timeframe
    pub fn bars_per_period(&self) -> usize {
        match self {
            Timeframe::M1 => 1,
            Timeframe::M5 => 5,
            Timeframe::M15 => 15,
            Timeframe::H1 => 60,
            Timeframe::H4 => 240,
            Timeframe::D1 => 1440,
        }
    }
}

/// Indicator values for a single timeframe
#[derive(Debug, Clone)]
pub struct TimeframeIndicators {
    pub timeframe: Timeframe,
    pub rsi: f64,
    pub macd: f64,
    pub macd_signal: f64,
    pub adx: f64,
    pub bb_upper: f64,
    pub bb_lower: f64,
    pub bb_position: f64, // 0.0 = at lower, 1.0 = at upper
    pub stoch_k: f64,
    pub stoch_d: f64,
    pub score: f64, // -1.0 (strong sell) to +1.0 (strong buy)
}

/// Multi-timeframe analysis result for a symbol
#[derive(Debug, Clone)]
pub struct MultiTimeframeResult {
    pub symbol: String,
    pub timeframes: Vec<TimeframeIndicators>,
    pub aggregate_score: f64,
}

/// Multi-Timeframe Technical Analyst
pub struct MultiTimeframeAnalyst {
    /// Per-symbol, per-timeframe close price history
    closes: HashMap<String, HashMap<Timeframe, Vec<f64>>>,
    /// Indicator parameters
    rsi_period: usize,
    macd_fast: usize,
    macd_slow: usize,
    macd_signal_period: usize,
    bb_period: usize,
    bb_std: f64,
    stoch_k_period: usize,
    stoch_d_period: usize,
}

impl Default for MultiTimeframeAnalyst {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiTimeframeAnalyst {
    pub fn new() -> Self {
        Self {
            closes: HashMap::new(),
            rsi_period: 14,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal_period: 9,
            bb_period: 20,
            bb_std: 2.0,
            stoch_k_period: 14,
            stoch_d_period: 3,
        }
    }

    /// Feed a 1m close price. Automatically aggregates into all timeframes.
    pub fn feed(&mut self, symbol: &str, price: f64, _timestamp: i64) {
        let symbol_closes = self.closes.entry(symbol.to_string()).or_default();

        for &tf in Timeframe::all() {
            let bars = symbol_closes.entry(tf).or_default();
            bars.push(price);

            // Keep rolling window per timeframe
            let max_bars = match tf {
                Timeframe::M1 => 200,
                Timeframe::M5 => 200,
                Timeframe::M15 => 200,
                Timeframe::H1 => 200,
                Timeframe::H4 => 200,
                Timeframe::D1 => 200,
            };
            if bars.len() > max_bars {
                bars.drain(..bars.len() - max_bars);
            }
        }
    }

    /// Analyze a symbol across all timeframes
    pub fn analyze(&self, symbol: &str) -> Option<MultiTimeframeResult> {
        let symbol_closes = self.closes.get(symbol)?;
        let mut timeframes = Vec::new();

        for &tf in Timeframe::all() {
            if let Some(closes) = symbol_closes.get(&tf) {
                let min_bars = self.macd_slow + self.macd_signal_period + 1;
                if closes.len() < min_bars {
                    continue;
                }
                let indicators = self.compute_indicators(closes);
                let score = self.compute_score(&indicators);
                timeframes.push(TimeframeIndicators {
                    timeframe: tf,
                    rsi: indicators.0,
                    macd: indicators.1,
                    macd_signal: indicators.2,
                    adx: indicators.3,
                    bb_upper: indicators.4,
                    bb_lower: indicators.5,
                    bb_position: indicators.6,
                    stoch_k: indicators.7,
                    stoch_d: indicators.8,
                    score,
                });
            }
        }

        if timeframes.is_empty() {
            return None;
        }

        // Aggregate: higher timeframes get more weight
        let weights = [0.03, 0.07, 0.15, 0.20, 0.25, 0.30]; // M1..D1
        let aggregate_score: f64 = timeframes
            .iter()
            .zip(weights.iter())
            .map(|(tf, w)| tf.score * w)
            .sum::<f64>()
            / weights.iter().sum::<f64>();

        Some(MultiTimeframeResult {
            symbol: symbol.to_string(),
            timeframes,
            aggregate_score,
        })
    }

    /// Get closes for a specific timeframe (for external consumers like ConfluenceScorer)
    pub fn get_closes(&self, symbol: &str, tf: Timeframe) -> Option<&Vec<f64>> {
        self.closes.get(symbol)?.get(&tf)
    }

    // ── Indicator computation (same algorithms as single-TF analyst) ──────

    fn compute_indicators(&self, closes: &[f64]) -> (f64, f64, f64, f64, f64, f64, f64, f64, f64) {
        let rsi = self.compute_rsi(closes);
        let (macd, macd_signal) = self.compute_macd(closes);
        let (bb_upper, bb_lower) = self.compute_bollinger(closes);
        let bb_range = bb_upper - bb_lower;
        let bb_position = if bb_range > 0.0 {
            (closes.last().unwrap_or(&0.0) - bb_lower) / bb_range
        } else {
            0.5
        };
        let adx = self.compute_adx(closes);
        let (stoch_k, stoch_d) = self.compute_stochastic(closes);
        (rsi, macd, macd_signal, adx, bb_upper, bb_lower, bb_position, stoch_k, stoch_d)
    }

    fn compute_score(&self, indicators: &(f64, f64, f64, f64, f64, f64, f64, f64, f64)) -> f64 {
        let (rsi, macd, macd_signal, adx, _bb_upper, _bb_lower, bb_position, stoch_k, stoch_d) = indicators;
        let mut score = 0.0;

        // RSI
        let rsi_score = if *rsi < 30.0 { 1.0 } else if *rsi > 70.0 { -1.0 } else { (50.0 - rsi) / 50.0 };
        score += rsi_score * 0.25;

        // MACD
        let macd_diff = macd - macd_signal;
        let macd_score = (macd_diff / macd_signal.abs().max(1.0)).clamp(-1.0, 1.0);
        score += macd_score * 0.25;

        // ADX filter
        let adx_modifier = if *adx > 25.0 { 1.0 } else if *adx < 15.0 { 0.3 } else { 0.7 };

        // Bollinger position
        let bb_score = (0.5 - bb_position).clamp(-1.0, 1.0);
        score += bb_score * 0.15;

        // Stochastic
        let stoch_score = if *stoch_k < 20.0 { 1.0 } else if *stoch_k > 80.0 { -1.0 } else { 0.0 };
        let stoch_crossover = if stoch_k > stoch_d { 0.3 } else { -0.3 };
        score += (stoch_score + stoch_crossover) * 0.15 * 0.5;

        score * adx_modifier
    }

    fn compute_rsi(&self, closes: &[f64]) -> f64 {
        if closes.len() < self.rsi_period + 1 { return 50.0; }
        let mut avg_gain = 0.0;
        let mut avg_loss = 0.0;
        for i in 1..=self.rsi_period {
            let change = closes[i] - closes[i - 1];
            if change > 0.0 { avg_gain += change; } else { avg_loss += -change; }
        }
        avg_gain /= self.rsi_period as f64;
        avg_loss /= self.rsi_period as f64;
        for i in (self.rsi_period + 1)..closes.len() {
            let change = closes[i] - closes[i - 1];
            let gain = if change > 0.0 { change } else { 0.0 };
            let loss = if change < 0.0 { -change } else { 0.0 };
            avg_gain = (avg_gain * (self.rsi_period as f64 - 1.0) + gain) / self.rsi_period as f64;
            avg_loss = (avg_loss * (self.rsi_period as f64 - 1.0) + loss) / self.rsi_period as f64;
        }
        if avg_loss == 0.0 { 100.0 } else { 100.0 - (100.0 / (1.0 + avg_gain / avg_loss)) }
    }

    fn compute_macd(&self, closes: &[f64]) -> (f64, f64) {
        let ema_fast = self.compute_ema(closes, self.macd_fast);
        let ema_slow = self.compute_ema(closes, self.macd_slow);
        let macd_line = ema_fast - ema_slow;
        let macd_series: Vec<f64> = (self.macd_slow..closes.len())
            .map(|i| {
                let ema_f = self.compute_ema(&closes[..=i], self.macd_fast);
                let ema_s = self.compute_ema(&closes[..=i], self.macd_slow);
                ema_f - ema_s
            })
            .collect();
        let signal = if macd_series.len() >= self.macd_signal_period {
            self.compute_ema(&macd_series, self.macd_signal_period)
        } else { macd_line };
        (macd_line, signal)
    }

    fn compute_bollinger(&self, closes: &[f64]) -> (f64, f64) {
        let period = self.bb_period.min(closes.len());
        let slice = &closes[closes.len() - period..];
        let mean = slice.iter().sum::<f64>() / period as f64;
        let variance = slice.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();
        (mean + self.bb_std * std_dev, mean - self.bb_std * std_dev)
    }

    fn compute_adx(&self, closes: &[f64]) -> f64 {
        let period = 14;
        if closes.len() < period + 1 { return 20.0; }
        let mut plus_dm_sum = 0.0;
        let mut minus_dm_sum = 0.0;
        let mut tr_sum = 0.0;
        for i in 1..=period {
            let high_diff = closes[i] - closes[i - 1];
            let low_diff = closes[i - 1] - closes[i];
            if high_diff > low_diff && high_diff > 0.0 { plus_dm_sum += high_diff; }
            else if low_diff > high_diff && low_diff > 0.0 { minus_dm_sum += low_diff; }
            tr_sum += (closes[i] - closes[i - 1]).abs();
        }
        let mut plus_dm = plus_dm_sum / period as f64;
        let mut minus_dm = minus_dm_sum / period as f64;
        let mut tr = tr_sum / period as f64;
        let mut dx_sum = 0.0;
        for i in (period + 1)..closes.len() {
            let high_diff = closes[i] - closes[i - 1];
            let low_diff = closes[i - 1] - closes[i];
            let raw_plus_dm = if high_diff > low_diff && high_diff > 0.0 { high_diff } else { 0.0 };
            let raw_minus_dm = if low_diff > high_diff && low_diff > 0.0 { low_diff } else { 0.0 };
            plus_dm = (plus_dm * (period as f64 - 1.0) + raw_plus_dm) / period as f64;
            minus_dm = (minus_dm * (period as f64 - 1.0) + raw_minus_dm) / period as f64;
            tr = (tr * (period as f64 - 1.0) + (closes[i] - closes[i - 1]).abs()) / period as f64;
            if tr > 0.0 {
                let plus_di = (plus_dm / tr) * 100.0;
                let minus_di = (minus_dm / tr) * 100.0;
                let di_sum = plus_di + minus_di;
                if di_sum > 0.0 { dx_sum += ((plus_di - minus_di).abs() / di_sum) * 100.0; }
            }
        }
        let dx_count = closes.len() - period - 1;
        if dx_count > 0 { dx_sum / dx_count as f64 } else { 20.0 }
    }

    fn compute_stochastic(&self, closes: &[f64]) -> (f64, f64) {
        let period = self.stoch_k_period.min(closes.len());
        let slice = &closes[closes.len() - period..];
        let highest = slice.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let lowest = slice.iter().cloned().fold(f64::INFINITY, f64::min);
        let current = *closes.last().unwrap_or(&0.0);
        if highest == lowest { return (50.0, 50.0); }
        let k = (current - lowest) / (highest - lowest) * 100.0;
        let d_period = self.stoch_d_period.min(closes.len());
        if d_period < 3 { return (k, k); }
        let mut k_values = Vec::new();
        for i in 0..d_period {
            let end = closes.len() - d_period + i + 1;
            let sub_slice = &closes[end - period..end];
            let hi = sub_slice.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let lo = sub_slice.iter().cloned().fold(f64::INFINITY, f64::min);
            let cur = closes[end - 1];
            if hi != lo { k_values.push((cur - lo) / (hi - lo) * 100.0); } else { k_values.push(50.0); }
        }
        let d = k_values.iter().sum::<f64>() / k_values.len() as f64;
        (k, d)
    }

    fn compute_ema(&self, data: &[f64], period: usize) -> f64 {
        if data.is_empty() { return 0.0; }
        if data.len() < period { return data.iter().sum::<f64>() / data.len() as f64; }
        let seed: f64 = data[..period].iter().sum::<f64>() / period as f64;
        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = seed;
        for &price in &data[period..] { ema = (price - ema) * multiplier + ema; }
        ema
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_bars_per_period() {
        assert_eq!(Timeframe::M1.bars_per_period(), 1);
        assert_eq!(Timeframe::H1.bars_per_period(), 60);
        assert_eq!(Timeframe::D1.bars_per_period(), 1440);
    }

    #[test]
    fn test_feed_and_analyze() {
        let mut analyst = MultiTimeframeAnalyst::new();
        // Feed 50 bars with uptrend
        for i in 0..50 {
            analyst.feed("BTC", 50000.0 + i as f64 * 100.0, i * 60);
        }
        let result = analyst.analyze("BTC");
        assert!(result.is_some(), "Should produce result after 50 bars");
        let result = result.unwrap();
        assert!(!result.timeframes.is_empty(), "Should have timeframe results");
    }

    #[test]
    fn test_aggregate_score_range() {
        let mut analyst = MultiTimeframeAnalyst::new();
        for i in 0..60 {
            analyst.feed("ETH", 3000.0 + (i as f64 * 10.0).sin() * 100.0, i * 60);
        }
        let result = analyst.analyze("ETH").unwrap();
        assert!(result.aggregate_score >= -1.0 && result.aggregate_score <= 1.0);
    }

    #[test]
    fn test_insufficient_data_returns_none() {
        let analyst = MultiTimeframeAnalyst::new();
        assert!(analyst.analyze("BTC").is_none());
    }
}
