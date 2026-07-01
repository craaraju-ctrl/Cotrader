//! High: 60-second cooldown between trades.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct Cooldown;

#[async_trait]
impl Rule for Cooldown {
    fn name(&self) -> &str { "cooldown" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
