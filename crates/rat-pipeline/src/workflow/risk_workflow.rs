//! Risk Workflow — Comprehensive risk assessment workflow.

use crate::runner::agents::RiskDesk;

pub struct RiskWorkflow {
    risk_desk: RiskDesk,
}

impl RiskWorkflow {
    pub fn new() -> Self {
        Self {
            risk_desk: RiskDesk::new(),
        }
    }

    /// Run comprehensive risk assessment.
    pub async fn assess(&self, symbol: &str) -> RiskWorkflowResult {
        println!("[Risk] Assessing risk for {}", symbol);

        let verdict = self.risk_desk.check_risk(symbol, "").await;

        RiskWorkflowResult {
            symbol: symbol.to_string(),
            passed: verdict.passed,
            reason: verdict.reason,
            risk_level: if verdict.passed { "LOW" } else { "HIGH" }.to_string(),
        }
    }
}

pub struct RiskWorkflowResult {
    pub symbol: String,
    pub passed: bool,
    pub reason: String,
    pub risk_level: String,
}
