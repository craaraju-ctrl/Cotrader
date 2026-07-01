//! Low: Max 2 positions per symbol.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct MaxPositionsPerSymbol;

#[async_trait]
impl Rule for MaxPositionsPerSymbol {
    fn name(&self) -> &str { "max_positions_per_symbol" }
    fn priority(&self) -> RulePriority { RulePriority::Low }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let count = ctx.portfolio.positions.iter().filter(|p| p.symbol == ctx.symbol).count();
        if count < 2 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} in {}", count, ctx.symbol), count as f64, 2.0)
        }
    }
}
