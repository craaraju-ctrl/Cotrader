pub struct HeadOfRisk;

impl HeadOfRisk {
    pub fn name() -> &'static str { "HeadOfRisk" }
    pub fn role() -> &'static str { "Head of Risk Management" }

    pub fn set_risk_limits(&self, portfolio_value: f64) -> String {
        format!(
            "Risk limits set for ${:.0} portfolio:\n\
             - Max single position: {:.0}% (${:.0})\n\
             - Max daily drawdown: 2% ($ {:.0})\n\
             - Max portfolio heat: 6%\n\
             - Max leverage: 3x\n\
             - Max correlated exposure: 40%",
            portfolio_value,
            5.0, portfolio_value * 0.05,
            portfolio_value * 0.02,
        )
    }

    pub fn monitor_drawdown(&self) -> String {
        let current_dd = 3.2;
        if current_dd > 5.0 {
            "CRITICAL: Drawdown > 5% — reduce all positions by 50%".to_string()
        } else if current_dd > 3.0 {
            format!("WARNING: Current drawdown {:.1}% — monitoring closely, no new positions", current_dd)
        } else {
            format!("Drawdown OK: {:.1}% — within normal range", current_dd)
        }
    }

    pub fn emergency_halt(&self, breach_type: &str) -> String {
        format!(
            "EMERGENCY HALT ACTIVATED\n\
             Breach: {}\n\
             Actions taken:\n\
             1. All pending orders cancelled\n\
             2. All open positions flagged for review\n\
             3. Trading suspended for 1 hour\n\
             4. Alert sent to CIO and compliance\n\
             5. Circuit breaker engaged",
            breach_type
        )
    }

    pub fn approve_risk(&self, proposal: &str) -> String {
        if proposal.contains("leverage") && proposal.contains("10x") {
            "REJECTED — Leverage exceeds 3x limit".to_string()
        } else if proposal.contains("concentration") && proposal.contains(">20") {
            "REJECTED — Single asset concentration exceeds 20%".to_string()
        } else {
            format!("APPROVED — Risk assessment passed | {}", proposal)
        }
    }
}
