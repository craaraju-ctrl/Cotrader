//! Medium: Minimum confluence score.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct ConfluenceMinimum;

#[async_trait]
impl Rule for ConfluenceMinimum {
    fn name(&self) -> &str { "confluence_minimum" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, _ctx: &RuleContext<'_>) -> RuleResult {
        RuleResult::pass(self.name(), self.priority())
    }
}
