//! Medium: Session risk budget.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct SessionRiskBudget;

#[async_trait]
impl Rule for SessionRiskBudget {
    fn name(&self) -> &str { "session_risk_budget" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
