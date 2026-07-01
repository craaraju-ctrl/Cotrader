//! High: Max 20 trades per day.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MaxDailyTrades;

#[async_trait]
impl Rule for MaxDailyTrades {
    fn name(&self) -> &str { "max_daily_trades" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let trades = ctx.portfolio.total_trades_today;
        if trades < 20 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} trades", trades), trades as f64, 20.0)
        }
    }
}
