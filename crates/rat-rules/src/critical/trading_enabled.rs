//! Critical: Trading must be enabled (global kill switch).

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct TradingEnabled;

#[async_trait]
impl Rule for TradingEnabled {
    fn name(&self) -> &str { "trading_enabled" }
    fn priority(&self) -> RulePriority { RulePriority::Critical }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        if ctx.portfolio.trading_enabled {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), "Trading disabled", 0.0, 1.0)
        }
    }
}
