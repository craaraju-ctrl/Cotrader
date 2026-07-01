//! High: Margin utilization must not exceed 80%.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MarginUtilization;

#[async_trait]
impl Rule for MarginUtilization {
    fn name(&self) -> &str { "margin_utilization" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let utilization = if ctx.portfolio.total_equity > 0.0 {
            (ctx.portfolio.total_equity - ctx.portfolio.cash_balance) / ctx.portfolio.total_equity
        } else {
            0.0
        };
        if utilization <= 0.80 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("Margin {:.1}%", utilization * 100.0), utilization, 0.80)
        }
    }
}
