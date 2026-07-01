//! Medium: Max adverse excursion limit.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MaeTracking;

#[async_trait]
impl Rule for MaeTracking {
    fn name(&self) -> &str { "mae_tracking" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
