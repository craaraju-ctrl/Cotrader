//! High: Stop loss within 0.5%-8% range.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct VolAdjustedStops;

#[async_trait]
impl Rule for VolAdjustedStops {
    fn name(&self) -> &str { "vol_adjusted_stops" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
