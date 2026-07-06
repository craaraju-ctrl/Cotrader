//! FeatureStore — Centralized feature computation for all ML models.
//!
//! Takes raw market data (MetricsSnapshot + OhlcvBar history) and produces
//! a normalized 48-feature vector that all ML models consume.
//! Avoids redundant indicator calculation across models.

use cotrader_core::MarketRegime;

/// Total number of features in the unified feature vector.
pub const FEATURE_COUNT: usize = 48;

/// Feature indices for each section (for interpretability).
pub mod indices {
    pub const RSI_14: usize = 0;
    pub const MACD_HIST: usize = 1;
    pub const ATR_PCT: usize = 2;
    pub const BB_WIDTH: usize = 3;
    pub const STOCH_K: usize = 4;
    pub const ADX: usize = 5;
    pub const CCI: usize = 6;
    pub const WILLIAMS_R: usize = 7;
    pub const VWAP_DEV: usize = 8;
    pub const MFI: usize = 9;
    pub const CMF: usize = 10;
    pub const OBV_DIR: usize = 11;
    pub const PLUS_DI: usize = 12;
    pub const MINUS_DI: usize = 13;
    pub const SAR_TREND: usize = 14;
    pub const KELTNER_WIDTH: usize = 15;
    pub const MOMENTUM: usize = 16;
    pub const ROC: usize = 17;
    pub const TRIX: usize = 18;
    pub const HMA_SLOPE: usize = 19;
    pub const AROON_UP: usize = 20;
    pub const AROON_DOWN: usize = 21;
    pub const ELDER_RAY_BULL: usize = 22;
    pub const ELDER_RAY_BEAR: usize = 23;
    pub const REL_VOLUME: usize = 24;
    pub const VOLATILITY_20: usize = 25;

    // 5-bar momentum features (5)
    pub const MOM_1: usize = 26;
    pub const MOM_2: usize = 27;
    pub const MOM_3: usize = 28;
    pub const MOM_4: usize = 29;
    pub const MOM_5: usize = 30;

    // 5-bar volume changes (5)
    pub const VOL_CHG_1: usize = 31;
    pub const VOL_CHG_2: usize = 32;
    pub const VOL_CHG_3: usize = 33;
    pub const VOL_CHG_4: usize = 34;
    pub const VOL_CHG_5: usize = 35;

    // Regime one-hot (5)
    pub const REGIME_BULL: usize = 36;
    pub const REGIME_BEAR: usize = 37;
    pub const REGIME_RANGE: usize = 38;
    pub const REGIME_VOLATILE: usize = 39;
    pub const REGIME_LOW_LIQ: usize = 40;

    // Context features (7)
    pub const TIME_OF_DAY: usize = 41;
    pub const DAY_OF_WEEK: usize = 42;
    pub const PORTFOLIO_HEAT: usize = 43;
    pub const CONSEC_LOSSES: usize = 44;
    pub const DAILY_PNL_PCT: usize = 45;
    pub const VOL_TREND: usize = 46;
    pub const PRICE_RANGE_PCT: usize = 47;
}

pub struct FeatureStore;

impl FeatureStore {
    pub fn new() -> Self {
        Self
    }

    /// Build the full 48-feature vector from MetricsSnapshot fields and context.
    ///
    /// `metrics` fields are the 26 indicators from MarketMetricsMeter.
    /// `bars` is the OHLCV history (at least 20 bars for momentum).
    /// `regime` is the current market regime.
    /// `portfolio_heat` is total risk / total equity.
    /// `consecutive_losses` is current consecutive loss count.
    /// `daily_pnl_pct` is today's P&L as a fraction.
    pub fn build_features(
        &self,
        rsi_14: f64,
        macd_hist: f64,
        atr_pct: f64,
        bb_upper: f64,
        bb_mid: f64,
        bb_lower: f64,
        stoch_k: f64,
        adx: f64,
        cci: f64,
        williams_r: f64,
        vwap_deviation: f64,
        mfi: f64,
        cmf: f64,
        obv_direction: f64,
        plus_di: f64,
        minus_di: f64,
        parabolic_trend: &str,
        keltner_upper: f64,
        keltner_mid: f64,
        keltner_lower: f64,
        momentum: f64,
        roc: f64,
        trix: f64,
        hma_slope: f64,
        aroon_up: f64,
        aroon_down: f64,
        elder_ray_bull: f64,
        elder_ray_bear: f64,
        rel_volume: f64,
        volatility_20: f64,
        bars: &[cotrader_core::OhlcvBar],
        regime: Option<MarketRegime>,
        portfolio_heat: f64,
        consecutive_losses: u32,
        daily_pnl_pct: f64,
    ) -> Vec<f64> {
        let mut features = vec![0.0f64; FEATURE_COUNT];

        // 26 indicator features (normalized to roughly [0,1] or [-1,1])
        features[indices::RSI_14] = rsi_14 / 100.0;
        features[indices::MACD_HIST] = macd_hist.tanh(); // squash to [-1,1]
        features[indices::ATR_PCT] = (atr_pct * 100.0).min(1.0); // cap at 1%
        let bb_width = if bb_mid > 0.0 { (bb_upper - bb_lower) / bb_mid } else { 0.0 };
        features[indices::BB_WIDTH] = bb_width.min(1.0);
        features[indices::STOCH_K] = stoch_k / 100.0;
        features[indices::ADX] = adx / 100.0;
        features[indices::CCI] = (cci / 200.0).clamp(-1.0, 1.0);
        features[indices::WILLIAMS_R] = (williams_r + 50.0) / 50.0; // map [-100,0] to [-1,1]
        features[indices::VWAP_DEV] = (vwap_deviation * 10.0).clamp(-1.0, 1.0);
        features[indices::MFI] = mfi / 100.0;
        features[indices::CMF] = cmf; // already [-1,1]
        features[indices::OBV_DIR] = obv_direction.tanh();
        features[indices::PLUS_DI] = plus_di / 100.0;
        features[indices::MINUS_DI] = minus_di / 100.0;
        features[indices::SAR_TREND] = if parabolic_trend == "uptrend" { 1.0 } else { -1.0 };
        let keltner_width = if keltner_mid > 0.0 { (keltner_upper - keltner_lower) / keltner_mid } else { 0.0 };
        features[indices::KELTNER_WIDTH] = keltner_width.min(1.0);
        features[indices::MOMENTUM] = momentum.tanh();
        features[indices::ROC] = (roc / 10.0).clamp(-1.0, 1.0);
        features[indices::TRIX] = trix.tanh();
        features[indices::HMA_SLOPE] = (hma_slope * 100.0).clamp(-1.0, 1.0);
        features[indices::AROON_UP] = aroon_up / 100.0;
        features[indices::AROON_DOWN] = aroon_down / 100.0;
        features[indices::ELDER_RAY_BULL] = elder_ray_bull.tanh();
        features[indices::ELDER_RAY_BEAR] = elder_ray_bear.tanh();
        features[indices::REL_VOLUME] = (rel_volume / 3.0).min(1.0); // cap at 3x
        features[indices::VOLATILITY_20] = (volatility_20 * 100.0).min(1.0);

        // 5-bar price momentum
        if bars.len() >= 6 {
            for i in 0..5 {
                let idx = bars.len() - 1 - i;
                let prev_idx = idx - 1;
                let ret = (bars[idx].close - bars[prev_idx].close) / bars[prev_idx].close;
                features[indices::MOM_1 + i] = (ret * 10.0).clamp(-1.0, 1.0);
            }
        }

        // 5-bar volume changes
        if bars.len() >= 6 {
            for i in 0..5 {
                let idx = bars.len() - 1 - i;
                let prev_idx = idx - 1;
                if bars[prev_idx].volume > 0.0 {
                    let vol_chg = (bars[idx].volume - bars[prev_idx].volume) / bars[prev_idx].volume;
                    features[indices::VOL_CHG_1 + i] = (vol_chg / 2.0).clamp(-1.0, 1.0);
                }
            }
        }

        // Regime one-hot
        match regime {
            Some(MarketRegime::TrendingBull) => features[indices::REGIME_BULL] = 1.0,
            Some(MarketRegime::TrendingBear) => features[indices::REGIME_BEAR] = 1.0,
            Some(MarketRegime::Ranging) => features[indices::REGIME_RANGE] = 1.0,
            Some(MarketRegime::Volatile) => features[indices::REGIME_VOLATILE] = 1.0,
            Some(MarketRegime::LowLiquidity) => features[indices::REGIME_LOW_LIQ] = 1.0,
            None => features[indices::REGIME_RANGE] = 1.0, // default to ranging
        }

        // Context features
        let now = chrono::Utc::now();
        features[indices::TIME_OF_DAY] = (now.timestamp() % 86400) as f64 / 86400.0;
        features[indices::DAY_OF_WEEK] = now.format("%u").to_string().parse::<f64>().unwrap_or(3.0) / 7.0;
        features[indices::PORTFOLIO_HEAT] = portfolio_heat.min(1.0);
        features[indices::CONSEC_LOSSES] = (consecutive_losses as f64 / 10.0).min(1.0);
        features[indices::DAILY_PNL_PCT] = (daily_pnl_pct * 10.0).clamp(-1.0, 1.0);

        // Volatility trend: compare recent vol to older vol
        if bars.len() >= 20 {
            let recent_vol = self.compute_volatility(&bars[bars.len()-5..]);
            let older_vol = self.compute_volatility(&bars[bars.len()-10..bars.len()-5]);
            let vol_trend = if older_vol > 0.0 { (recent_vol - older_vol) / older_vol } else { 0.0 };
            features[indices::VOL_TREND] = (vol_trend * 5.0).clamp(-1.0, 1.0);
        }

        // Price range as % of price
        if let Some(last) = bars.last() {
            if last.close > 0.0 {
                let range = (last.high - last.low) / last.close;
                features[indices::PRICE_RANGE_PCT] = (range * 100.0).min(1.0);
            }
        }

        features
    }

    /// Build OHLCV feature matrix for CNN pattern detector.
    /// Returns (window_size, 5) matrix flattened to Vec<f64>.
    /// Each bar: [open, high, low, close, volume] normalized per-bar.
    pub fn build_ohlcv_matrix(&self, bars: &[cotrader_core::OhlcvBar], window: usize) -> Vec<f64> {
        if bars.len() < window {
            return vec![0.0; window * 5];
        }
        let start = bars.len() - window;
        let slice = &bars[start..];
        let mut features = Vec::with_capacity(window * 5);

        for bar in slice {
            let bar_range = if bar.high > bar.low { bar.high - bar.low } else { 1.0 };
            features.push((bar.close - bar.low) / bar_range); // close position in range
            features.push((bar.high - bar.low) / bar.high);   // range as % of high
            features.push((bar.open - bar.low) / bar_range);   // open position in range
            features.push(if bar.high > 0.0 { bar.volume / bar.high } else { 0.0 }); // volume normalized
            features.push(bar.close / bar.open - 1.0); // bar return
        }

        features
    }

    /// Normalize features using z-score (running mean/std) or min-max.
    /// For now, use simple min-max clipping to [0,1] range.
    pub fn normalize(&self, features: &mut [f64]) {
        for f in features.iter_mut() {
            *f = f.clamp(-3.0, 3.0); // clip outliers
            // Map [-3,3] to [0,1]
            *f = (*f + 3.0) / 6.0;
        }
    }

    fn compute_volatility(&self, bars: &[cotrader_core::OhlcvBar]) -> f64 {
        if bars.len() < 2 {
            return 0.0;
        }
        let returns: Vec<f64> = bars.windows(2)
            .map(|w| (w[1].close - w[0].close).abs() / w[0].close)
            .collect();
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        variance.sqrt()
    }
}
