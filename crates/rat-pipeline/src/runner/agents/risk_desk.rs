//! Risk Desk — Coordinates risk management and compliance.

pub struct RiskDesk {
    pub market_risk_manager: MarketRiskManagerAgent,
    pub compliance_officer: ComplianceOfficerAgent,
}

pub struct MarketRiskManagerAgent;
pub struct ComplianceOfficerAgent;

impl RiskDesk {
    pub fn new() -> Self {
        Self {
            market_risk_manager: MarketRiskManagerAgent,
            compliance_officer: ComplianceOfficerAgent,
        }
    }

    /// Run all risk checks and return verdict.
    pub async fn check_risk(&self, symbol: &str, signal: &str) -> RiskVerdict {
        let market_risk = self.market_risk_manager.check(symbol, signal).await;
        let compliance = self.compliance_officer.check(symbol, signal).await;

        let passed = market_risk.passed && compliance.passed;
        let reason = if !market_risk.passed {
            market_risk.reason
        } else if !compliance.passed {
            compliance.reason
        } else {
            "All checks passed".to_string()
        };

        RiskVerdict { passed, reason }
    }
}

pub struct RiskVerdict {
    pub passed: bool,
    pub reason: String,
}

impl MarketRiskManagerAgent {
    pub async fn check(&self, symbol: &str, signal: &str) -> CheckResult {
        let _ = (symbol, signal);
        CheckResult { passed: true, reason: "OK".to_string() }
    }
}

impl ComplianceOfficerAgent {
    pub async fn check(&self, symbol: &str, signal: &str) -> CheckResult {
        let _ = (symbol, signal);
        CheckResult { passed: true, reason: "OK".to_string() }
    }
}

pub struct CheckResult {
    pub passed: bool,
    pub reason: String,
}
