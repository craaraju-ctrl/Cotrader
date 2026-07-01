pub struct OptionsFlowFeed;

impl OptionsFlowFeed {
    pub fn name() -> &'static str { "OptionsFlowFeed" }

    pub fn fetch(&self, symbol: &str) -> Vec<String> {
        vec![
            format!("Options flow for {}:", symbol),
            "Put/Call ratio: 0.72 (bullish — below 1.0)".into(),
            "Unusual activity: 3x volume on $60K calls (Jan 2027 expiry)".into(),
            "Implied volatility: 45% (below 30d avg of 52%)".into(),
            "Max pain: $57,000 — current price above max pain = bullish".into(),
            "Net premium flow: +$45M calls (bullish)".into(),
            "Dealer gamma: Positive above $58K — pin risk at current level".into(),
            "Signal: MODERATELY BULLISH — options market pricing upside".into(),
        ]
    }
}
