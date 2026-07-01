pub struct NewsFeed;

impl NewsFeed {
    pub fn name() -> &'static str { "NewsFeed" }

    pub fn fetch(&self, symbol: &str) -> Vec<String> {
        vec![
            format!("News for {} (last 24h):", symbol),
            "[1] Bullish: Institutional adoption increases — BlackRock adds 500 BTC".into(),
            "[2] Neutral: Fed signals potential rate cut in September".into(),
            "[3] Bearish: Exchange hack concern — $50M exploit at smaller CEX".into(),
            "[4] Bullish: ETF inflows continue — $800M net inflow this week".into(),
            "[5] Neutral: Regulatory clarity — EU MiCA framework implementation begins".into(),
            "Sentiment summary: 3 bullish, 1 bearish, 2 neutral".into(),
        ]
    }
}
