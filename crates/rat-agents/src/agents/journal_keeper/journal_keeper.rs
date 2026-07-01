pub struct JournalKeeper;

impl JournalKeeper {
    pub fn name() -> &'static str { "JournalKeeper" }
    pub fn role() -> &'static str { "Trading Journal Keeper" }

    pub fn record_trade(&self, trade: &str, reasoning: &str) -> String {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        format!(
            "[{}] TRADE RECORDED | Trade: {} | Reasoning: {} | Status: Logged",
            timestamp, trade, reasoning
        )
    }

    pub fn extract_lessons(&self, period: &str) -> String {
        format!(
            "Lessons from {}: 1) Avoid entries during low-volume hours (before 9:30 ET) \
             2) Respect 2% risk per trade rule — violated 3 times \
             3) Winning trades held avg 2.3 days, losing trades avg 0.4 days — cut losers faster \
             4) RSI >75 entries had 30% win rate — too aggressive at extremes \
             5) Best setups occurred at key support/resistance with volume confirmation",
            period
        )
    }

    pub fn performance_stats(&self, period: &str) -> String {
        format!(
            "Performance {}: Win Rate: 52.3% | Avg Win: +2.1% | Avg Loss: -1.3% | \
             Profit Factor: 1.85 | Max Drawdown: -8.2% | Sharpe: 1.42 | \
             Total Trades: 47 | Profitable: 25 | Losing: 22 | \
             Best Trade: +6.8% | Worst Trade: -3.1% | Avg Hold: 1.8 days",
            period
        )
    }

    pub fn review_mistakes(&self) -> String {
        "Recurring mistakes: 1) Overtrading after losses (revenge trading detected in 4 trades) \
         2) Moving stop-loss further from entry (violated risk rules 2 times) \
         3) Averaging down into losing positions (3 occurrences) \
         4) Ignoring regime change signals (entered long during bear regime) \
         Corrective actions: Implement 24h cooldown after 2 consecutive losses, \
         hard stop-loss that cannot be moved, no averaging down policy"
            .to_string()
    }
}
