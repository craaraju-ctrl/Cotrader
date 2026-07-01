//! Medium: Max 5 correlated positions.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct CorrelationHeat;

#[async_trait]
impl Rule for CorrelationHeat {
    fn name(&self) -> &str { "correlation_heat" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let count = ctx.portfolio.position_count;
        if count <= 5 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} positions", count), count as f64, 5.0)
        }
    }
}
