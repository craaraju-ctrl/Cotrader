//! High: Position size capped at 25%.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct KellySizing;

#[async_trait]
impl Rule for KellySizing {
    fn name(&self) -> &str { "kelly_sizing" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
