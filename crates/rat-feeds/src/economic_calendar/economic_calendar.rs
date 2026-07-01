pub struct EconomicCalendarFeed;

impl EconomicCalendarFeed {
    pub fn name() -> &'static str { "EconomicCalendarFeed" }

    pub fn fetch(&self) -> Vec<String> {
        vec![
            "Economic Calendar — Next 7 Days:".into(),
            "[Jul 5] Non-Farm Payrolls (US) — Consensus: 185K | Impact: HIGH".into(),
            "[Jul 11] CPI (US) — Consensus: 3.1% YoY | Impact: HIGH".into(),
            "[Jul 11] ECB Rate Decision — Consensus: Hold at 4.25% | Impact: MEDIUM".into(),
            "[Jul 17] Retail Sales (US) — Consensus: +0.3% MoM | Impact: MEDIUM".into(),
            "[Jul 30] FOMC Decision — Consensus: 25bp cut (90% prob) | Impact: HIGH".into(),
            "Risk window: Jul 5 NFP + Jul 11 CPI = high volatility period".into(),
            "Recommendation: Reduce position size by 30% before NFP".into(),
        ]
    }
}
