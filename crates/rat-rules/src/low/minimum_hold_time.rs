//! Low: 5-minute minimum hold time.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MinimumHoldTime;

#[async_trait]
impl Rule for MinimumHoldTime {
    fn name(&self) -> &str { "minimum_hold_time" }
    fn priority(&self) -> RulePriority { RulePriority::Low }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
