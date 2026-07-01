//! REST Polling Feed — Periodic price fetching via REST API.

pub struct RestPollingFeed {
    symbols: Vec<String>,
    interval_secs: u64,
}

impl RestPollingFeed {
    pub fn new(symbols: Vec<String>, interval_secs: u64) -> Self {
        Self { symbols, interval_secs }
    }

    pub async fn poll(&self) -> Vec<(String, f64)> {
        let mut results = Vec::new();
        for symbol in &self.symbols {
            let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}USDT", symbol);
            if let Ok(resp) = reqwest::Client::new()
                .get(&url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(price) = json.get("price").and_then(|p| p.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                        results.push((symbol.clone(), price));
                    }
                }
            }
        }
        results
    }

    pub fn interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.interval_secs)
    }
}
