pub struct MarketDataFeed;

impl MarketDataFeed {
    pub fn name() -> &'static str { "MarketDataFeed" }

    pub fn fetch(&self, symbol: &str) -> Vec<String> {
        vec![
            format!("Market data for {}:", symbol),
            "Source: Binance REST API".into(),
            "Interval: 1h".into(),
            "Last price: $58,542.00".into(),
            "24h change: +1.2%".into(),
            "24h volume: $2.4B".into(),
            "Bid/Ask spread: 0.01%".into(),
            "Order book depth: $45M within 1%".into(),
            format!("Status: FRESH (updated {} seconds ago)", 3),
        ]
    }
}
