pub struct OnChainFeed;

impl OnChainFeed {
    pub fn name() -> &'static str { "OnChainFeed" }

    pub fn fetch(&self, symbol: &str) -> Vec<String> {
        vec![
            format!("On-chain data for {}:", symbol),
            "Exchange reserves: Decreasing (-2.3% this week) — supply squeeze".into(),
            "Whale wallets (1000+ BTC): Accumulating (+42 wallets in 30d)".into(),
            "Active addresses: 1.2M daily (↑8% MoM)".into(),
            "Hash rate: 620 EH/s (ATH) — network security at peak".into(),
            "MVRV Z-Score: 2.1 (above average, but not overheated zone >3.5)".into(),
            "Funding rate: 0.008% per 8h — neutral".into(),
            "Signal: MILDLY BULLISH — accumulation pattern with declining exchange supply".into(),
        ]
    }
}
