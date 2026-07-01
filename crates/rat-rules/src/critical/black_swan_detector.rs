//! Critical: >5% price move in single bar = flash crash protection.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct BlackSwanDetector;

#[async_trait]
impl Rule for BlackSwanDetector {
    fn name(&self) -> &str { "black_swan_detector" }
    fn priority(&self) -> RulePriority { RulePriority::Critical }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let bars = &ctx.market.bars;
        if bars.len() < 2 {
            return RuleResult::pass(self.name(), self.priority());
        }
        let current = bars.last().unwrap().close;
        let prev = bars[bars.len() - 2].close;
        let change = (current - prev).abs() / prev;
        if change <= 0.05 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{:.1}% move", change * 100.0), change, 0.05)
        }
    }
}
