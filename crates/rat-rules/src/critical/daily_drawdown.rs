//! Critical: Daily drawdown must not exceed 2%.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct DailyDrawdown;

#[async_trait]
impl Rule for DailyDrawdown {
    fn name(&self) -> &str { "daily_drawdown" }
    fn priority(&self) -> RulePriority { RulePriority::Critical }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let dd = ctx.portfolio.max_drawdown_today;
        if dd <= 0.02 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("DD {:.1}%", dd * 100.0), dd, 0.02)
        }
    }
}
