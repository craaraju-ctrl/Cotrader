//! Real REST API clients for live price data.
//! All data is fetched through Tredo Exchange — the single price gateway.
//!
//! Tredo Exchange internally aggregates from BinanceFeed, FinnhubFeed, and
//! other upstream sources. CoTrader never calls external APIs directly.

use crate::data_feed::Bar;

// ── Symbol classification ───────────────────────────────────────────────────

/// Returns true if the symbol is a cryptocurrency.
pub fn is_crypto_symbol(symbol: &str) -> bool {
    cotrader_core::is_crypto_symbol(symbol)
}

// ── Price fetching (single price tick) ──────────────────────────────────────

/// Fetch the latest price for a symbol from Tredo Exchange.
pub async fn fetch_price(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    cotrader_core::fetch_tredo_price(client, symbol).await
}

/// Fetch latest price + 24h stats as a Bar-like structure.
/// Returns (price, high_24h, low_24h, volume_24h, change_pct).
async fn fetch_price_stats(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<(f64, f64, f64, f64, f64), Box<dyn std::error::Error + Send + Sync>> {
    let ticker = cotrader_core::fetch_tredo_ticker_24hr(client, symbol).await?;
    let price = ticker["lastPrice"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| ticker["lastPrice"].as_f64())
        .unwrap_or(0.0);
    let high = ticker["highPrice"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| ticker["highPrice"].as_f64())
        .unwrap_or(price);
    let low = ticker["lowPrice"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| ticker["lowPrice"].as_f64())
        .unwrap_or(price);
    let volume = ticker["volume"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| ticker["volume"].as_f64())
        .unwrap_or(0.0);
    let change_pct = ticker["priceChangePercent"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| ticker["priceChangePercent"].as_f64())
        .unwrap_or(0.0);
    Ok((price, high, low, volume, change_pct))
}

/// Fetch a full OHLCV bar (uses 24h stats + latest price from Tredo).
pub async fn fetch_live_bar(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Bar, Box<dyn std::error::Error + Send + Sync>> {
    let now = chrono::Utc::now();
    let price = fetch_price(client, symbol).await?;
    let (_, high, low, volume, _) = fetch_price_stats(client, symbol).await.unwrap_or((
        price,
        price * 1.02,
        price * 0.98,
        0.0,
        0.0,
    ));

    Ok(Bar {
        timestamp: now,
        open: price,
        high,
        low,
        close: price,
        volume,
    })
}

/// Fetch latest price from Tredo Exchange.
pub async fn fetch_tredo_price(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    cotrader_core::fetch_tredo_price(client, symbol).await
}

/// Fetch klines (historical OHLCV bars) from Tredo Exchange.
pub async fn fetch_tredo_klines(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> Result<Vec<Bar>, Box<dyn std::error::Error + Send + Sync>> {
    let ohlcv = cotrader_core::fetch_tredo_candles(client, symbol, interval, limit).await?;
    Ok(ohlcv
        .into_iter()
        .map(|b| {
            let dt = chrono::DateTime::parse_from_rfc3339(&b.timestamp)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            Bar {
                timestamp: dt,
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                volume: b.volume,
            }
        })
        .collect())
}
