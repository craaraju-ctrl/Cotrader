//! DataLoader — Load training data from EpisodeStore and OHLCV history.

use rusqlite::Connection;
use std::path::Path;

/// A single training sample: features at trade entry + label.
pub struct TrainingSample {
    pub features: Vec<f64>,
    pub label: f64,        // 1.0 = profitable, 0.0 = loss
    pub pnl_pct: f64,      // actual P&L percentage
    pub strategy_index: usize, // which strategy was used
    pub regime: String,    // market regime at entry
}

/// Load training data from the SQLite episode store.
pub fn load_episode_training_data(
    db_path: &Path,
    limit: usize,
) -> Result<Vec<TrainingSample>, Box<dyn std::error::Error + Send + Sync>> {
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare(
        "SELECT id, symbol, direction, entry_price, exit_price, stop_loss, take_profit,
                position_size, pnl, pnl_pct, outcome, regret_score, confluence_score,
                portfolio_heat, market_regime, consecutive_losses_at_entry, was_correct
         FROM closed_episodes
         ORDER BY exit_time DESC
         LIMIT ?1"
    )?;

    let samples = stmt.query_map([limit as i64], |row| {
        let pnl_pct: f64 = row.get(9)?;
        let _outcome: String = row.get(10)?;
        let confluence: f64 = row.get(12)?;
        let portfolio_heat: f64 = row.get(13)?;
        let regime: String = row.get(14)?;
        let consec_losses: u32 = row.get(15)?;
        let was_correct: bool = row.get(16)?;

        // Build a minimal feature vector from available data
        // In production, we'd recompute full indicators from stored OHLCV
        let features = vec![
            confluence,
            portfolio_heat,
            (consec_losses as f64) / 10.0,
            pnl_pct.abs(),
            // Regime as numeric
            match regime.as_str() {
                "TrendingBull" => 1.0,
                "TrendingBear" => 2.0,
                "Ranging" => 3.0,
                "Volatile" => 4.0,
                "LowLiquidity" => 5.0,
                _ => 3.0,
            } / 5.0,
        ];

        Ok(TrainingSample {
            features,
            label: if was_correct { 1.0 } else { 0.0 },
            pnl_pct,
            strategy_index: 0,
            regime,
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(samples)
}

/// Load OHLCV data from SQLite for pattern training.
pub fn load_ohlcv_data(
    db_path: &Path,
    symbol: &str,
    limit: usize,
) -> Result<Vec<(Vec<f64>, f64)>, Box<dyn std::error::Error + Send + Sync>> {
    // Returns (ohlcv_features, label) pairs for CNN training
    // Label: 1.0 if price went up after the window, 0.0 if down
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare(
        "SELECT open, high, low, close, volume FROM ohlcv_bars
         WHERE symbol = ?1
         ORDER BY timestamp DESC
         LIMIT ?2"
    )?;

    let bars: Vec<(f64, f64, f64, f64, f64)> = stmt.query_map(rusqlite::params![symbol, limit as i64], |row| {
        Ok((
            row.get::<_, f64>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, f64>(4)?,
        ))
    })?.collect::<Result<Vec<_>, _>>()?;

    let mut samples = Vec::new();
    let window = 20;

    if bars.len() >= window + 1 {
        for i in 0..bars.len() - window {
            let window_bars = &bars[i..i + window];
            let next_close = bars[i + window].3; // close of next bar
            let current_close = window_bars[window - 1].3;

            // Normalize per-bar
            let mut features = Vec::with_capacity(window * 5);
            for &(open, high, low, close, volume) in window_bars {
                let range = if high > low { high - low } else { 1.0 };
                features.push((close - low) / range);
                features.push((high - low) / if high > 0.0 { high } else { 1.0 });
                features.push((open - low) / range);
                features.push(volume / if high > 0.0 { high } else { 1.0 });
                features.push(close / if open > 0.0 { open } else { 1.0 } - 1.0);
            }

            let label = if next_close > current_close { 1.0 } else { 0.0 };
            samples.push((features, label));
        }
    }

    Ok(samples)
}
