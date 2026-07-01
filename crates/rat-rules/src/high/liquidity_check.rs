//! High: Bar range must be <0.5% (thin book detection).

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct LiquidityCheck;

#[async_trait]
impl Rule for LiquidityCheck {
    fn name(&self) -> &str { "liquidity_check" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let range = ctx.market.bar_range();
        if range < 0.005 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("Range {:.2}%", range * 100.0), range, 0.005)
        }
    }
}
