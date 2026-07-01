//! Low: Max 3 trades per symbol per day.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct SymbolFrequencyCap;

#[async_trait]
impl Rule for SymbolFrequencyCap {
    fn name(&self) -> &str { "symbol_frequency_cap" }
    fn priority(&self) -> RulePriority { RulePriority::Low }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let count = ctx.portfolio.positions.iter().filter(|p| p.symbol == ctx.symbol).count();
        if count < 3 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} in {}", count, ctx.symbol), count as f64, 3.0)
        }
    }
}
