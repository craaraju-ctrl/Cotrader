//! Critical: 15% total account drawdown = permanent halt.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MaxAbsoluteDrawdown;

#[async_trait]
impl Rule for MaxAbsoluteDrawdown {
    fn name(&self) -> &str { "max_absolute_drawdown" }
    fn priority(&self) -> RulePriority { RulePriority::Critical }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let dd = ctx.portfolio.max_drawdown_today;
        let limit = 0.15;
        if dd < limit {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("DD {:.1}%", dd * 100.0), dd, limit)
        }
    }
}
