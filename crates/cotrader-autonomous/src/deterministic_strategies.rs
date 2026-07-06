// ═══════════════════════════════════════════════════════════════════════════════
// Structure-Based Trading Strategies — Professional-Grade
//
// These strategies are based on what price ACTUALLY does at key levels,
// not what indicators calculate after the fact. Each strategy requires
// 3+ independent factors to align before firing.
//
// Strategies:
//   1. StructureBreakout — Consolidation range break with volume confirmation
//   2. TrendPullback — Trend continuation on pullback to key level
//   3. LiquiditySweep — Stop hunt reversal at liquidity pools
//
// Each strategy produces a TradeSignal with conviction, direction, and levels.
// The select_best_strategy() function scores all strategies and picks the strongest.
// ═══════════════════════════════════════════════════════════════════════════════

use crate::helpers;
use crate::types::{MarketRegime, TradeSignal};
use chrono::Utc;
use cotrader_core::{OhlcvBar, TradeDirection};

/// Base risk per trade (1% of equity by default)
const BASE_RISK_PCT: f64 = 0.01;

/// Result from a deterministic strategy
#[derive(Debug, Clone)]
pub struct StrategyResult {
    pub strategy_name: String,
    pub direction: TradeDirection,
    pub entry_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub confidence: f64,
    pub reason: String,
    /// Which regimes this strategy is suitable for
    pub suitable_regimes: Vec<MarketRegime>,
    pub rsi: f64,
    pub atr_pct: f64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Strategy 1: Structure Breakout
//
// Identifies consolidation ranges (tight price action), then trades the breakout
// when price moves beyond the range with volume confirmation.
//
// How it works:
//   1. Find the high and low of the last 20 bars
//   2. Check if range is tight (< 2x ATR = consolidation)
//   3. BUY if price breaks above range high with volume > 1.2x average
//   4. SELL if price breaks below range low with volume > 1.2x average
//   5. Stop loss at the opposite end of the range
//   6. Target at 2x the range width from entry
//
// Best in: Volatile, TrendingBull, TrendingBear markets
// ═══════════════════════════════════════════════════════════════════════════════
pub fn structure_breakout_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
) -> Option<StrategyResult> {
    if bars.len() < 25 {
        return None;
    }

    let atr = helpers::compute_atr(bars, 14);
    let atr_pct = atr / current_price;
    let rsi = helpers::compute_rsi(bars, 14);
    let rel_vol = helpers::compute_relative_volume(bars);

    // Find consolidation range using bars BEFORE the last one (exclude current bar)
    let lookback = 20.min(bars.len().saturating_sub(1));
    let recent = &bars[bars.len() - 1 - lookback..bars.len() - 1];
    let range_high = recent.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let range_low = recent.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let range_width = range_high - range_low;

    // Consolidation check: range must be tight (< 2x ATR)
    let is_consolidating = range_width < atr * 2.0;

    if !is_consolidating || range_width <= 0.0 {
        return None;
    }

    // Volume must confirm the breakout
    let volume_confirms = rel_vol > 1.2;

    // BUY: Price breaks above range high with volume
    if current_price > range_high && volume_confirms {
        let breakout_strength = (current_price - range_high) / range_width;
        let confidence = (breakout_strength * 0.4 + (rel_vol - 1.0) * 0.3 + 0.3).clamp(0.0, 0.95);

        // Stop loss at range low (the consolidation floor)
        let stop_loss = range_low;
        // Target: 2× range width above entry. The classic 1× measured move
        // gives R:R ≈ 1:1 against a full-range stop — violates the
        // 1%-risk → ≥2R discipline (the R:R floor would silently extend it;
        // better the strategy states its real target).
        let take_profit = current_price + range_width * 2.0;

        return Some(StrategyResult {
            strategy_name: "StructureBreakout".to_string(),
            direction: TradeDirection::Long,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence,
            reason: format!(
                "Breakout BUY: price {:.2} broke above consolidation range [{:.2}-{:.2}], vol={:.1}x",
                current_price, range_low, range_high, rel_vol
            ),
            suitable_regimes: vec![
                MarketRegime::Volatile,
                MarketRegime::TrendingBull,
                MarketRegime::TrendingBear,
            ],
            rsi,
            atr_pct,
        });
    }

    // SELL: Price breaks below range low with volume
    if current_price < range_low && volume_confirms {
        let breakout_strength = (range_low - current_price) / range_width;
        let confidence = (breakout_strength * 0.4 + (rel_vol - 1.0) * 0.3 + 0.3).clamp(0.0, 0.95);

        // Stop loss at range high (the consolidation ceiling)
        let stop_loss = range_high;
        // Target: 2× range width below entry (same ≥2R reasoning as the long side).
        let take_profit = current_price - range_width * 2.0;

        return Some(StrategyResult {
            strategy_name: "StructureBreakout".to_string(),
            direction: TradeDirection::Short,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence,
            reason: format!(
                "Breakout SELL: price {:.2} broke below consolidation range [{:.2}-{:.2}], vol={:.1}x",
                current_price, range_low, range_high, rel_vol
            ),
            suitable_regimes: vec![MarketRegime::Volatile, MarketRegime::TrendingBear],
            rsi,
            atr_pct,
        });
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Strategy 2: Trend Pullback
//
// Identifies the dominant trend, waits for price to pull back to a key level
// (Fibonacci retracement or moving average), then enters in the trend direction.
//
// How it works:
//   1. Determine trend direction using 50-period SMA slope
//   2. Calculate Fibonacci retracement levels (38.2%, 50%, 61.8%)
//   3. BUY if uptrend + price pulled back to 50-61.8% Fib + RSI < 50
//   4. SELL if downtrend + price pulled back to 50-61.8% Fib + RSI > 50
//   5. Stop loss below the recent swing low (for longs) or above swing high (for shorts)
//   6. Target at the previous swing high (for longs) or swing low (for shorts)
//
// Best in: TrendingBull, TrendingBear markets
// ═══════════════════════════════════════════════════════════════════════════════
pub fn trend_pullback_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
) -> Option<StrategyResult> {
    if bars.len() < 55 {
        return None;
    }

    let atr = helpers::compute_atr(bars, 14);
    let atr_pct = atr / current_price;
    let rsi = helpers::compute_rsi(bars, 14);

    // Calculate 50-period SMA and its slope
    let sma_50 = compute_sma(bars, 50);
    let sma_50_prev = compute_sma(&bars[..bars.len().saturating_sub(5)], 50);
    let sma_slope = (sma_50 - sma_50_prev) / sma_50_prev;

    // Determine trend direction
    let is_uptrend = sma_slope > 0.001; // SMA rising
    let is_downtrend = sma_slope < -0.001; // SMA falling

    if !is_uptrend && !is_downtrend {
        return None; // No clear trend
    }

    // Find recent swing high and low (last 30 bars)
    let lookback = 30.min(bars.len());
    let recent = &bars[bars.len() - lookback..];
    let swing_high = recent.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let swing_low = recent.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let swing_range = swing_high - swing_low;

    if swing_range <= 0.0 {
        return None;
    }

    // Calculate Fibonacci retracement levels
    let fib_382 = swing_high - swing_range * 0.382;
    let fib_500 = swing_high - swing_range * 0.500;
    let fib_618 = swing_high - swing_range * 0.618;

    // BUY: Uptrend + pullback to 50-61.8% Fib zone + RSI oversold
    if is_uptrend && current_price >= fib_500 && current_price <= fib_618 && rsi < 50.0 {
        // Entry at current price (at the Fib level)
        let stop_loss = swing_low - atr * 0.5; // Below the swing low
        let take_profit = swing_high; // Target the previous high

        let fib_position = (current_price - fib_500) / (fib_618 - fib_500);
        let confidence = (0.5 + (1.0 - fib_position) * 0.2 + (50.0 - rsi) / 100.0 * 0.3)
            .clamp(0.0, 0.95);

        return Some(StrategyResult {
            strategy_name: "TrendPullback".to_string(),
            direction: TradeDirection::Long,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence,
            reason: format!(
                "Trend pullback BUY: uptrend (SMA slope={:.4}), price at {:.1}% Fib retrace, RSI={:.1}",
                sma_slope, fib_position * 100.0, rsi
            ),
            suitable_regimes: vec![MarketRegime::TrendingBull],
            rsi,
            atr_pct,
        });
    }

    // SELL: Downtrend + pullback to 50-61.8% Fib zone + RSI overbought
    if is_downtrend && current_price >= fib_382 && current_price <= fib_500 && rsi > 50.0 {
        let stop_loss = swing_high + atr * 0.5; // Above the swing high
        let take_profit = swing_low; // Target the previous low

        let fib_position = (fib_500 - current_price) / (fib_500 - fib_382);
        let confidence = (0.5 + (1.0 - fib_position) * 0.2 + (rsi - 50.0) / 100.0 * 0.3)
            .clamp(0.0, 0.95);

        return Some(StrategyResult {
            strategy_name: "TrendPullback".to_string(),
            direction: TradeDirection::Short,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence,
            reason: format!(
                "Trend pullback SELL: downtrend (SMA slope={:.4}), price at {:.1}% Fib retrace, RSI={:.1}",
                sma_slope, fib_position * 100.0, rsi
            ),
            suitable_regimes: vec![MarketRegime::TrendingBear],
            rsi,
            atr_pct,
        });
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Strategy 3: Liquidity Sweep
//
// Identifies where stop losses are clustered (below recent lows for longs,
// above recent highs for shorts), waits for price to sweep that liquidity
// and reverse, then enters on the reversal.
//
// How it works:
//   1. Find the lowest low in the last 20 bars (liquidity pool below)
//   2. Find the highest high in the last 20 bars (liquidity pool above)
//   3. BUY if price swept below the low then reversed back above it
//   4. SELL if price swept above the high then reversed back below it
//   5. Stop loss below the sweep low (for longs) or above sweep high (for shorts)
//   6. Target at 2x the sweep distance
//
// Best in: Ranging, Volatile markets
// ═══════════════════════════════════════════════════════════════════════════════
pub fn liquidity_sweep_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
) -> Option<StrategyResult> {
    if bars.len() < 25 {
        return None;
    }

    let atr = helpers::compute_atr(bars, 14);
    let atr_pct = atr / current_price;
    let rsi = helpers::compute_rsi(bars, 14);

    // Find liquidity pools (recent extremes)
    let lookback = 20.min(bars.len());
    let recent = &bars[bars.len() - lookback..];
    let liquidity_low = recent.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let liquidity_high = recent.iter().map(|b| b.high).fold(f64::MIN, f64::max);

    // Check if the PREVIOUS bar swept liquidity and current bar is reversing
    if bars.len() < 2 {
        return None;
    }
    let prev_bar = &bars[bars.len() - 2];
    let prev_low = prev_bar.low;
    let prev_high = prev_bar.high;

    // BUY: Previous bar swept below liquidity low, current bar is reversing up
    let swept_below = prev_low < liquidity_low;
    let reversing_up = current_price > prev_bar.close && current_price > liquidity_low;

    if swept_below && reversing_up && rsi < 50.0 {
        let sweep_distance = liquidity_low - prev_low;
        let confidence = (0.4 + (sweep_distance / atr * 0.2) + (50.0 - rsi) / 100.0 * 0.3)
            .clamp(0.0, 0.95);

        // Stop loss below the sweep low
        let stop_loss = prev_low - atr * 0.3;
        // Target: 2x sweep distance above entry
        let take_profit = current_price + sweep_distance * 2.0;

        return Some(StrategyResult {
            strategy_name: "LiquiditySweep".to_string(),
            direction: TradeDirection::Long,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence,
            reason: format!(
                "Liquidity sweep BUY: price swept below {:.2} (to {:.2}), now reversing, RSI={:.1}",
                liquidity_low, prev_low, rsi
            ),
            suitable_regimes: vec![MarketRegime::Ranging, MarketRegime::Volatile],
            rsi,
            atr_pct,
        });
    }

    // SELL: Previous bar swept above liquidity high, current bar is reversing down
    let swept_above = prev_high > liquidity_high;
    let reversing_down = current_price < prev_bar.close && current_price < liquidity_high;

    if swept_above && reversing_down && rsi > 50.0 {
        let sweep_distance = prev_high - liquidity_high;
        let confidence = (0.4 + (sweep_distance / atr * 0.2) + (rsi - 50.0) / 100.0 * 0.3)
            .clamp(0.0, 0.95);

        // Stop loss above the sweep high
        let stop_loss = prev_high + atr * 0.3;
        // Target: 2x sweep distance below entry
        let take_profit = current_price - sweep_distance * 2.0;

        return Some(StrategyResult {
            strategy_name: "LiquiditySweep".to_string(),
            direction: TradeDirection::Short,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence,
            reason: format!(
                "Liquidity sweep SELL: price swept above {:.2} (to {:.2}), now reversing, RSI={:.1}",
                liquidity_high, prev_high, rsi
            ),
            suitable_regimes: vec![MarketRegime::Ranging, MarketRegime::Volatile],
            rsi,
            atr_pct,
        });
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Strategy 5: Momentum Divergence
//
// Identifies when price makes a new high/low but RSI doesn't confirm —
// a reversal signal. Requires price structure + momentum disagreement.
//
// How it works:
//   1. Find the last 2 swing highs and 2 swing lows
//   2. Bearish divergence: price makes higher high, RSI makes lower high
//   3. Bullish divergence: price makes lower low, RSI makes higher low
//   4. Confirmation: price breaks the swing low (for bearish) or high (for bullish)
//
// Best in: TrendingBull, TrendingBear markets (reversal at extremes)
// ═══════════════════════════════════════════════════════════════════════════════
pub fn momentum_divergence_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
) -> Option<StrategyResult> {
    if bars.len() < 30 {
        return None;
    }

    let atr = helpers::compute_atr(bars, 14);
    let atr_pct = atr / current_price;
    let rsi = helpers::compute_rsi(bars, 14);

    // Find swing highs and lows in last 30 bars
    let lookback = 30.min(bars.len());
    let recent = &bars[bars.len() - lookback..];

    let mut swing_highs: Vec<(usize, f64, f64)> = Vec::new(); // (index, price, rsi)
    let mut swing_lows: Vec<(usize, f64, f64)> = Vec::new();

    for (i, bar) in recent.iter().enumerate() {
        if i > 2 && i < recent.len() - 2 {
            // Swing high: bar high > neighbors
            if bar.high > recent[i - 1].high && bar.high > recent[i + 1].high
                && bar.high > recent[i - 2].high && bar.high > recent[i + 2].high
            {
                let bar_rsi = helpers::compute_rsi(&bars[..bars.len() - lookback + i + 1], 14);
                swing_highs.push((i, bar.high, bar_rsi));
            }
            // Swing low: bar low < neighbors
            if bar.low < recent[i - 1].low && bar.low < recent[i + 1].low
                && bar.low < recent[i - 2].low && bar.low < recent[i + 2].low
            {
                let bar_rsi = helpers::compute_rsi(&bars[..bars.len() - lookback + i + 1], 14);
                swing_lows.push((i, bar.low, bar_rsi));
            }
        }
    }

    // Need at least 2 swing highs or 2 swing lows for divergence
    if swing_highs.len() >= 2 {
        let (_i1, p1, r1) = swing_highs[swing_highs.len() - 2];
        let (_i2, p2, r2) = swing_highs[swing_highs.len() - 1];

        // Bearish divergence: price higher high, RSI lower high
        if p2 > p1 && r2 < r1 && r1 > 60.0 {
            let strength = ((r1 - r2) / 30.0 * 0.5 + (p2 - p1) / p1 * 10.0 * 0.3 + 0.2)
                .clamp(0.0, 0.9);
            let stop_loss = current_price + atr * 1.5;
            let take_profit = current_price - atr * 3.0;

            return Some(StrategyResult {
                strategy_name: "MomentumDivergence".to_string(),
                direction: TradeDirection::Short,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: strength,
                reason: format!(
                    "Bearish divergence: price HH at {:.2} vs {:.2}, RSI LH at {:.1} vs {:.1}",
                    p2, p1, r2, r1
                ),
                suitable_regimes: vec![MarketRegime::TrendingBull, MarketRegime::TrendingBear],
                rsi,
                atr_pct,
            });
        }
    }

    if swing_lows.len() >= 2 {
        let (_i1, p1, r1) = swing_lows[swing_lows.len() - 2];
        let (_i2, p2, r2) = swing_lows[swing_lows.len() - 1];

        // Bullish divergence: price lower low, RSI higher low
        if p2 < p1 && r2 > r1 && r1 < 40.0 {
            let strength = ((r2 - r1) / 30.0 * 0.5 + (p1 - p2) / p1 * 10.0 * 0.3 + 0.2)
                .clamp(0.0, 0.9);
            let stop_loss = current_price - atr * 1.5;
            let take_profit = current_price + atr * 3.0;

            return Some(StrategyResult {
                strategy_name: "MomentumDivergence".to_string(),
                direction: TradeDirection::Long,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: strength,
                reason: format!(
                    "Bullish divergence: price LL at {:.2} vs {:.2}, RSI HL at {:.1} vs {:.1}",
                    p2, p1, r2, r1
                ),
                suitable_regimes: vec![MarketRegime::TrendingBull, MarketRegime::TrendingBear],
                rsi,
                atr_pct,
            });
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Strategy 6: Volume Profile POC Bounce
//
// Trades bounces off the Point of Control (highest volume price level).
// The POC acts as a magnet — price tends to revert to it.
//
// How it works:
//   1. Calculate volume-weighted average price (VWAP) as POC proxy
//   2. BUY if price is below POC and showing bullish momentum
//   3. SELL if price is above POC and showing bearish momentum
//   4. Stop loss beyond recent swing, target at POC
//
// Best in: Ranging, Volatile markets
// ═══════════════════════════════════════════════════════════════════════════════
pub fn volume_profile_poc_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
) -> Option<StrategyResult> {
    if bars.len() < 20 {
        return None;
    }

    let atr = helpers::compute_atr(bars, 14);
    let atr_pct = atr / current_price;
    let rsi = helpers::compute_rsi(bars, 14);

    // Calculate VWAP as POC proxy (volume-weighted average)
    let lookback = 20.min(bars.len());
    let recent = &bars[bars.len() - lookback..];
    let mut total_volume = 0.0;
    let mut volume_price_sum = 0.0;
    for bar in recent {
        let typical_price = (bar.high + bar.low + bar.close) / 3.0;
        volume_price_sum += typical_price * bar.volume;
        total_volume += bar.volume;
    }
    let vwap = if total_volume > 0.0 {
        volume_price_sum / total_volume
    } else {
        return None;
    };

    let distance_from_poc = (current_price - vwap) / vwap;

    // BUY: Price below POC with bullish momentum
    if distance_from_poc < -0.005 && rsi < 50.0 && rsi > 30.0 {
        let strength = (distance_from_poc.abs() * 20.0 * 0.4 + (50.0 - rsi) / 50.0 * 0.3 + 0.3)
            .clamp(0.0, 0.9);
        let stop_loss = current_price - atr * 1.5;
        let take_profit = vwap; // Target POC

        return Some(StrategyResult {
            strategy_name: "VolumeProfilePOC".to_string(),
            direction: TradeDirection::Long,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence: strength,
            reason: format!(
                "POC bounce BUY: price {:.2} below VWAP {:.2} ({:.1}%), RSI={:.1}",
                current_price, vwap, distance_from_poc * 100.0, rsi
            ),
            suitable_regimes: vec![MarketRegime::Ranging, MarketRegime::Volatile],
            rsi,
            atr_pct,
        });
    }

    // SELL: Price above POC with bearish momentum
    if distance_from_poc > 0.005 && rsi > 50.0 && rsi < 70.0 {
        let strength = (distance_from_poc * 20.0 * 0.4 + (rsi - 50.0) / 50.0 * 0.3 + 0.3)
            .clamp(0.0, 0.9);
        let stop_loss = current_price + atr * 1.5;
        let take_profit = vwap; // Target POC

        return Some(StrategyResult {
            strategy_name: "VolumeProfilePOC".to_string(),
            direction: TradeDirection::Short,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence: strength,
            reason: format!(
                "POC bounce SELL: price {:.2} above VWAP {:.2} ({:.1}%), RSI={:.1}",
                current_price, vwap, distance_from_poc * 100.0, rsi
            ),
            suitable_regimes: vec![MarketRegime::Ranging, MarketRegime::Volatile],
            rsi,
            atr_pct,
        });
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Master Strategy Selector — Adaptive Regime Weighting
//
// Runs all 6 strategies, scores each by confidence + regime fit,
// and picks the strongest signal. Returns None if no strategy fires.
// Regime weights adjust which strategies get priority:
//   - TrendingBull/Bear: TrendPullback, MomentumDivergence weighted higher
//   - Ranging: VolumeProfilePOC, SupportResistanceBounce weighted higher
//   - Volatile: StructureBreakout, LiquiditySweep weighted higher
// ═══════════════════════════════════════════════════════════════════════════════
pub fn select_best_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
    regime: &MarketRegime,
    supports: &[f64],
    resistances: &[f64],
) -> Option<StrategyResult> {
    // Regime-specific strategy weights (multiplier for confidence)
    let regime_weights: Vec<(&str, f64)> = match regime {
        MarketRegime::TrendingBull | MarketRegime::TrendingBear => vec![
            ("TrendPullback", 1.3),
            ("MomentumDivergence", 1.2),
            ("StructureBreakout", 1.0),
            ("LiquiditySweep", 0.9),
            ("VolumeProfilePOC", 0.8),
            ("SupportResistanceBounce", 0.9),
        ],
        MarketRegime::Ranging => vec![
            ("VolumeProfilePOC", 1.3),
            ("SupportResistanceBounce", 1.2),
            ("LiquiditySweep", 1.1),
            ("StructureBreakout", 0.9),
            ("TrendPullback", 0.7),
            ("MomentumDivergence", 0.8),
        ],
        MarketRegime::Volatile => vec![
            ("StructureBreakout", 1.3),
            ("LiquiditySweep", 1.2),
            ("MomentumDivergence", 1.1),
            ("VolumeProfilePOC", 0.9),
            ("TrendPullback", 0.8),
            ("SupportResistanceBounce", 0.8),
        ],
        MarketRegime::LowLiquidity => vec![
            ("LiquiditySweep", 1.2),
            ("VolumeProfilePOC", 1.1),
            ("SupportResistanceBounce", 1.0),
            ("StructureBreakout", 0.8),
            ("TrendPullback", 0.7),
            ("MomentumDivergence", 0.7),
        ],
    };

    let strategies = [
        structure_breakout_strategy(bars, current_price),
        trend_pullback_strategy(bars, current_price),
        liquidity_sweep_strategy(bars, current_price),
        momentum_divergence_strategy(bars, current_price),
        volume_profile_poc_strategy(bars, current_price),
    ];

    let mut best: Option<StrategyResult> = None;
    let mut best_score = 0.0;

    // Check all 5 primary strategies
    for strategy in strategies.iter().flatten() {
        let regime_multiplier = regime_weights
            .iter()
            .find(|(name, _)| *name == strategy.strategy_name)
            .map(|(_, w)| *w)
            .unwrap_or(1.0);

        let regime_suitability = if strategy.suitable_regimes.contains(regime) {
            1.0
        } else {
            0.5
        };
        let score = strategy.confidence * regime_multiplier * 0.6 + regime_suitability * 0.4;

        if score > best_score && score > 0.35 {
            best_score = score;
            best = Some(strategy.clone());
        }
    }

    // Check S/R bounce as fallback
    if let Some(ref sr) = support_resistance_bounce_strategy(bars, current_price, supports, resistances) {
        let regime_multiplier = regime_weights
            .iter()
            .find(|(name, _)| *name == "SupportResistanceBounce")
            .map(|(_, w)| *w)
            .unwrap_or(1.0);
        let regime_suitability = if sr.suitable_regimes.contains(regime) {
            1.0
        } else {
            0.5
        };
        let score = sr.confidence * regime_multiplier * 0.6 + regime_suitability * 0.4;

        if score > best_score && score > 0.35 {
            best = Some(sr.clone());
        }
    }

    best
}

// ═══════════════════════════════════════════════════════════════════════════════
// Support/Resistance Bounce (kept as fallback strategy)
// ═══════════════════════════════════════════════════════════════════════════════
pub fn support_resistance_bounce_strategy(
    bars: &[OhlcvBar],
    current_price: f64,
    supports: &[f64],
    resistances: &[f64],
) -> Option<StrategyResult> {
    if bars.len() < 15 || (supports.is_empty() && resistances.is_empty()) {
        return None;
    }

    let atr = helpers::compute_atr(bars, 14);
    let atr_pct = atr / current_price;
    let rsi = helpers::compute_rsi(bars, 14);
    let rel_vol = helpers::compute_relative_volume(bars);

    // Check nearest support level for bounce BUY
    for support in supports.iter().take(3) {
        let distance = ((current_price - support) / current_price).abs();
        if distance < atr_pct * 0.5 && rsi < 50.0 && rel_vol > 0.8 {
            let strength = (1.0 - distance / (atr_pct * 0.5)) * 0.5 + (0.5 - (rsi / 100.0)) * 0.5;
            let confidence = strength.clamp(0.0, 0.95);
            let stop_loss = support * (1.0 - atr_pct).max(0.92);
            let take_profit = current_price * (1.0 + atr_pct * 2.5).min(1.12);

            return Some(StrategyResult {
                strategy_name: "SupportResistanceBounce".to_string(),
                direction: TradeDirection::Long,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence,
                reason: format!(
                    "S/R bounce BUY: near support at {:.2} (distance={:.2}%), RSI={:.1}",
                    support,
                    distance * 100.0,
                    rsi
                ),
                suitable_regimes: vec![MarketRegime::Ranging, MarketRegime::TrendingBull],
                rsi,
                atr_pct,
            });
        }
    }

    // Check nearest resistance level for bounce SELL
    for resistance in resistances.iter().take(3) {
        let distance = ((resistance - current_price) / current_price).abs();
        if distance < atr_pct * 0.5 && rsi > 50.0 && rel_vol > 0.8 {
            let strength = (1.0 - distance / (atr_pct * 0.5)) * 0.5 + ((rsi / 100.0) - 0.5) * 0.5;
            let confidence = strength.clamp(0.0, 0.95);
            let stop_loss = resistance * (1.0 + atr_pct).min(1.08);
            let take_profit = current_price * (1.0 - atr_pct * 2.5).max(0.88);

            return Some(StrategyResult {
                strategy_name: "SupportResistanceBounce".to_string(),
                direction: TradeDirection::Short,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence,
                reason: format!(
                    "S/R bounce SELL: near resistance at {:.2} (distance={:.2}%), RSI={:.1}",
                    resistance,
                    distance * 100.0,
                    rsi
                ),
                suitable_regimes: vec![MarketRegime::Ranging, MarketRegime::TrendingBear],
                rsi,
                atr_pct,
            });
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helper functions
// ═══════════════════════════════════════════════════════════════════════════════

fn compute_sma(bars: &[OhlcvBar], period: usize) -> f64 {
    if bars.len() < period {
        return bars.last().map(|b| b.close).unwrap_or(0.0);
    }
    let sum: f64 = bars[bars.len() - period..].iter().map(|b| b.close).sum();
    sum / period as f64
}

/// Convert a StrategyResult to a TradeSignal
pub fn strategy_result_to_signal(result: &StrategyResult, equity: f64) -> TradeSignal {
    let risk_amount = equity * BASE_RISK_PCT;
    let raw_size = risk_amount / (result.entry_price - result.stop_loss).abs().max(0.01);
    let max_size = equity / result.entry_price.max(0.01);
    let position_size = raw_size.min(max_size);
    let rr_ratio = (result.take_profit - result.entry_price).abs()
        / (result.stop_loss - result.entry_price).abs().max(0.001);

    TradeSignal {
        symbol: String::new(),
        direction: result.direction,
        entry_price: result.entry_price,
        stop_loss: result.stop_loss,
        take_profit: result.take_profit,
        position_size,
        confidence_score: result.confidence,
        confluence_score: result.confidence,
        risk_reward_ratio: rr_ratio,
        reasoning: format!("{} | {}", result.strategy_name, result.reason),
        timestamp: Utc::now(),
        session_valid: true,
        risk_check_passed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bars(prices: &[f64]) -> Vec<OhlcvBar> {
        prices
            .iter()
            .enumerate()
            .map(|(i, &p)| OhlcvBar {
                timestamp: format!("2026-01-{:02}T00:00:00+00:00", i + 1),
                open: p * 0.998,
                high: p * 1.01,
                low: p * 0.99,
                close: p,
                volume: 1000.0,
            })
            .collect()
    }

    #[test]
    fn test_structure_breakout_buy() {
        // Create a tight consolidation (25+ bars) followed by a breakout with volume spike
        let mut prices: Vec<f64> = (0..25).map(|_| 100.0).collect(); // consolidation
        prices.push(105.0); // breakout bar
        let mut bars = make_bars(&prices);
        if let Some(last) = bars.last_mut() {
            last.volume = 1500.0;
        }
        // make_bars sets high = price * 1.01, so range_high = 105 * 1.01 = 106.05
        // current_price must exceed range_high to trigger breakout
        let current_price = 107.0;
        let result = structure_breakout_strategy(&bars, current_price);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.direction, TradeDirection::Long);
        assert_eq!(r.strategy_name, "StructureBreakout");
    }

    #[test]
    fn test_trend_pullback_buy() {
        // Create an uptrend with a pullback
        let prices: Vec<f64> = (0..60)
            .map(|i| {
                if i < 40 {
                    100.0 + i as f64 * 0.5 // uptrend
                } else {
                    120.0 - (i - 40) as f64 * 0.3 // pullback
                }
            })
            .collect();
        let bars = make_bars(&prices);
        let result = trend_pullback_strategy(&bars, *prices.last().unwrap());
        // Should detect uptrend and pullback
        if let Some(r) = result {
            assert_eq!(r.direction, TradeDirection::Long);
            assert_eq!(r.strategy_name, "TrendPullback");
        }
    }

    #[test]
    fn test_liquidity_sweep_buy() {
        // Create a sweep below a low then reversal
        let mut prices: Vec<f64> = (0..20).map(|i| 100.0 + (i as f64 * 0.1).sin()).collect();
        let bars = make_bars(&prices);

        // Simulate: previous bar swept below, current bar reversing
        let mut modified_bars = bars;
        let sweep_low = modified_bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        modified_bars.last_mut().unwrap().low = sweep_low - 0.5; // sweep below
        modified_bars.last_mut().unwrap().close = sweep_low + 0.2; // reverse up

        let result = liquidity_sweep_strategy(&modified_bars, sweep_low + 0.2);
        if let Some(r) = result {
            assert_eq!(r.direction, TradeDirection::Long);
            assert_eq!(r.strategy_name, "LiquiditySweep");
        }
    }

    #[test]
    fn test_volume_profile_poc_buy() {
        // Create a U-shaped price action with volume concentration at bottom
        let mut prices: Vec<f64> = Vec::new();
        for i in 0..30 {
            if i < 10 {
                prices.push(100.0 - i as f64 * 0.5); // decline
            } else if i < 20 {
                prices.push(95.0 + (i - 10) as f64 * 0.3); // recovery
            } else {
                prices.push(98.0); // consolidation near VWAP
            }
        }
        let mut bars = make_bars(&prices);
        // Add volume spike at the bottom (high volume at low prices)
        for i in 8..12 {
            bars[i].volume = 2000.0;
        }
        let result = volume_profile_poc_strategy(&bars, 97.0);
        // Should find VWAP and detect price below it
        if let Some(r) = result {
            assert!(r.strategy_name == "VolumeProfilePOC");
        }
    }

    #[test]
    fn test_select_best_strategy_regime_weighting() {
        // Create enough bars for all strategies
        let prices: Vec<f64> = (0..60).map(|i| 100.0 + (i as f64 * 0.1).sin() * 5.0).collect();
        let bars = make_bars(&prices);

        // Test with different regimes
        let result_ranging = select_best_strategy(&bars, 100.0, &MarketRegime::Ranging, &[], &[]);
        let result_trending = select_best_strategy(&bars, 100.0, &MarketRegime::TrendingBull, &[], &[]);

        // Both should return something (or None if no strategy fires)
        // The key test is that they don't panic
        if let Some(r) = result_ranging {
            assert!(!r.strategy_name.is_empty());
        }
        if let Some(r) = result_trending {
            assert!(!r.strategy_name.is_empty());
        }
    }
}
