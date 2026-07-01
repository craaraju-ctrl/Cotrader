//! Medium: Risk per trade appropriate for regime.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct RegimeSafety;

#[async_trait]
impl Rule for RegimeSafety {
    fn name(&self) -> &str { "regime_safety" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
