//! Low: Max 10 total positions.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MaxTotalPositions;

#[async_trait]
impl Rule for MaxTotalPositions {
    fn name(&self) -> &str { "max_total_positions" }
    fn priority(&self) -> RulePriority { RulePriority::Low }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let count = ctx.portfolio.position_count;
        if count < 10 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} positions", count), count as f64, 10.0)
        }
    }
}
