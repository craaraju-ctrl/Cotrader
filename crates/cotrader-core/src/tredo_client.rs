//! Tredo Exchange REST client — the single gateway for all market data.
//!
//! All live data (prices, OHLCV, order book, news) is fetched through
//! Tredo Exchange, which internally aggregates from BinanceFeed, FinnhubFeed,
//! and other upstream sources. CoTrader never calls external APIs directly.

use crate::OhlcvBar;
use std::time::Duration;

/// Map CoTrader bare symbol to Tredo paired symbol.
/// "BTC" → "BTC/USD", "BTC/USD" → "BTC/USD"
pub fn to_tredo_symbol(symbol: &str) -> String {
    if symbol.contains('/') {
        symbol.to_string()
    } else {
        format!("{}/USD", symbol)
    }
}

/// Map Tredo paired symbol back to CoTrader bare symbol.
/// "BTC/USD" → "BTC"
pub fn from_tredo_symbol(symbol: &str) -> String {
    symbol.split('/').next().unwrap_or(symbol).to_string()
}

/// Get the Tredo Exchange base URL from environment.
/// Returns None if not configured.
pub fn tredo_base_url() -> Option<String> {
    let url = std::env::var("COTRADER_BASE_URL").ok()?;
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Fetch live price from Tredo Exchange REST API.
/// Returns None if Tredo is not configured or unreachable.
pub async fn fetch_tredo_price(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let base = tredo_base_url().ok_or("COTRADER_BASE_URL not set")?;
    let tredo_sym = to_tredo_symbol(symbol);
    let url = format!("{}/api/v1/ticker/24hr?symbol={}", base, tredo_sym);
    let resp: serde_json::Value = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?
        .json()
        .await?;
    // Try multiple possible field names
    if let Some(price) = resp["lastPrice"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
    {
        return Ok(price);
    }
    if let Some(price) = resp["lastPrice"].as_f64() {
        return Ok(price);
    }
    if let Some(price) = resp["price"].as_f64() {
        return Ok(price);
    }
    Err(format!("Tredo: no price for {}", symbol).into())
}

/// Fetch live price from Tredo, returning None on failure (non-fatal).
pub async fn try_fetch_tredo_price(
    client: &reqwest::Client,
    symbol: &str,
) -> Option<f64> {
    fetch_tredo_price(client, symbol).await.ok()
}

/// Fetch OHLCV candles from Tredo Exchange REST API.
pub async fn fetch_tredo_candles(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> Result<Vec<OhlcvBar>, Box<dyn std::error::Error + Send + Sync>> {
    let base = tredo_base_url().ok_or("COTRADER_BASE_URL not set")?;
    let tredo_sym = to_tredo_symbol(symbol);
    let url = format!(
        "{}/api/v1/candles?symbol={}&interval={}&limit={}",
        base, tredo_sym, interval, limit
    );
    let resp: serde_json::Value = client
        .get(&url)
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .json()
        .await?;

    let candles = resp["candles"]
        .as_array()
        .ok_or("Tredo: no 'candles' array in response")?;

    let mut bars = Vec::with_capacity(candles.len());
    for c in candles {
        let timestamp = c["timestamp"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let open = c["open"].as_f64().unwrap_or(0.0);
        let high = c["high"].as_f64().unwrap_or(0.0);
        let low = c["low"].as_f64().unwrap_or(0.0);
        let close = c["close"].as_f64().unwrap_or(0.0);
        let volume = c["volume"].as_f64().unwrap_or(0.0);
        if close > 0.0 {
            bars.push(OhlcvBar {
                timestamp,
                open,
                high,
                low,
                close,
                volume,
            });
        }
    }
    Ok(bars)
}

/// Fetch candles from Tredo, returning empty Vec on failure.
pub async fn try_fetch_tredo_candles(
    client: &reqwest::Client,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> Vec<OhlcvBar> {
    fetch_tredo_candles(client, symbol, interval, limit)
        .await
        .unwrap_or_default()
}

/// Fetch order book from Tredo Exchange REST API.
pub async fn fetch_tredo_orderbook(
    client: &reqwest::Client,
    symbol: &str,
    depth: usize,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let base = tredo_base_url().ok_or("COTRADER_BASE_URL not set")?;
    let tredo_sym = to_tredo_symbol(symbol);
    let url = format!(
        "{}/api/v1/orderbook?symbol={}&depth={}",
        base, tredo_sym, depth
    );
    let resp: serde_json::Value = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?
        .json()
        .await?;
    Ok(resp)
}

/// Fetch 24hr ticker stats from Tredo Exchange REST API.
pub async fn fetch_tredo_ticker_24hr(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let base = tredo_base_url().ok_or("COTRADER_BASE_URL not set")?;
    let tredo_sym = to_tredo_symbol(symbol);
    let url = format!("{}/api/v1/ticker/24hr?symbol={}", base, tredo_sym);
    let resp: serde_json::Value = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?
        .json()
        .await?;
    Ok(resp)
}

/// Fetch news from Tredo Exchange REST API (backed by FinnhubFeed internally).
pub async fn fetch_tredo_news(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let base = tredo_base_url().ok_or("COTRADER_BASE_URL not set")?;
    let tredo_sym = to_tredo_symbol(symbol);
    let url = format!("{}/api/v1/news/{}", base, tredo_sym);
    let resp: serde_json::Value = client
        .get(&url)
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .json()
        .await?;
    Ok(resp)
}

/// Check if Tredo Exchange is reachable and healthy.
pub async fn tredo_health_check(
    client: &reqwest::Client,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let base = tredo_base_url().ok_or("COTRADER_BASE_URL not set")?;
    let url = format!("{}/api/v1/health", base);
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(3))
        .send()
        .await?;
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_tredo_symbol() {
        assert_eq!(to_tredo_symbol("BTC"), "BTC/USD");
        assert_eq!(to_tredo_symbol("ETH"), "ETH/USD");
        assert_eq!(to_tredo_symbol("BTC/USD"), "BTC/USD");
        assert_eq!(to_tredo_symbol("AAPL/USD"), "AAPL/USD");
    }

    #[test]
    fn test_from_tredo_symbol() {
        assert_eq!(from_tredo_symbol("BTC/USD"), "BTC");
        assert_eq!(from_tredo_symbol("ETH/USD"), "ETH");
        assert_eq!(from_tredo_symbol("BTC"), "BTC");
    }
}
