pub struct Rat;

impl Rat {
    pub fn name() -> &'static str { "Rat" }
    pub fn role() -> &'static str { "CIO — Chief Investment Officer" }

    pub fn set_market_view(&self, macro_context: &str) -> String {
        let bias = if macro_context.contains("bull") || macro_context.contains("growth") {
            "RISK-ON — favor equities, crypto, high yield"
        } else if macro_context.contains("bear") || macro_context.contains("recession") {
            "RISK-OFF — favor bonds, gold, defensive sectors"
        } else {
            "NEUTRAL — balanced allocation, reduced position sizes"
        };
        format!("Market view: {} | Context: {}", bias, macro_context)
    }

    pub fn approve_trade(&self, proposal: &str, risk_budget: f64) -> String {
        if risk_budget < 0.01 {
            "REJECTED — risk budget exhausted for the day".to_string()
        } else if proposal.contains("leveraged") && risk_budget < 0.05 {
            "REJECTED — leveraged trade requires >5% risk budget remaining".to_string()
        } else {
            format!("APPROVED — Trade: {} | Risk budget remaining: {:.1}%", proposal, risk_budget * 100.0)
        }
    }

    pub fn allocate_capital(&self, desk_performance: &[(String, f64)]) -> String {
        let total: f64 = desk_performance.iter().map(|(_, p)| p.max(0.01)).sum();
        let allocations: Vec<String> = desk_performance.iter().map(|(name, perf)| {
            let weight = (perf.max(0.01) / total * 100.0).min(50.0);
            format!("  {}: {:.1}%", name, weight)
        }).collect();
        format!("Capital allocation (performance-weighted):\n{}", allocations.join("\n"))
    }

    pub fn veto_check(&self, decision: &str) -> String {
        if decision.contains("all-in") || decision.contains("100%") {
            "VETOED — No single position can exceed 20% of portfolio".to_string()
        } else if decision.contains("unhedged") && decision.contains("options") {
            "VETOED — Options positions must have defined risk (no naked positions)".to_string()
        } else {
            format!("APPROVED — Decision: {}", decision)
        }
    }
}
