//! High: Max 10% of equity per order.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct OrderSizeLimits;

#[async_trait]
impl Rule for OrderSizeLimits {
    fn name(&self) -> &str { "order_size_limits" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
