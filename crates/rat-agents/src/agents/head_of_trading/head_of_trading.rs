pub struct HeadOfTrading;

impl HeadOfTrading {
    pub fn name() -> &'static str { "HeadOfTrading" }
    pub fn role() -> &'static str { "Head of Trading" }

    pub fn set_daily_targets(&self, market_conditions: &str) -> String {
        let (max_loss, target, exposure) = if market_conditions.contains("volatile") {
            (500.0, 1500.0, 0.3)
        } else if market_conditions.contains("trending") {
            (800.0, 3000.0, 0.6)
        } else {
            (600.0, 2000.0, 0.4)
        };
        format!(
            "Daily targets | Max loss: ${:.0} | Target: ${:.0} | Max exposure: {:.0}% | Conditions: {}",
            max_loss, target, exposure * 100.0, market_conditions
        )
    }

    pub fn prioritize_desk(&self, signal: &str) -> String {
        if signal.contains("high conviction") || signal.contains("strong") {
            "Priority: CRYPTO DESK — high conviction signal detected, allocate 60% of risk budget"
        } else if signal.contains("medium") || signal.contains("moderate") {
            "Priority: EQUITY DESK — moderate signal, standard position sizing"
        } else {
            "Priority: RESEARCH — weak signal, no execution, continue monitoring"
        }.to_string()
    }

    pub fn escalate(&self, issue: &str) -> String {
        if issue.contains("drawdown") && issue.contains(">10") {
            "ESCALATE TO CIO: Drawdown exceeds 10% — recommend reducing all positions by 50%".to_string()
        } else if issue.contains("breach") || issue.contains("limit") {
            "ESCALATE TO CIO: Risk limit breach detected — immediate review required".to_string()
        } else if issue.contains("system") || issue.contains("error") {
            "ESCALATE TO TECHNOLOGY: System issue — switch to manual execution".to_string()
        } else {
            format!("Logged for review: {}", issue)
        }
    }
}
