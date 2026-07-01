//! Medium: 3+ consecutive losses warning.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct LossStreakRecovery;

#[async_trait]
impl Rule for LossStreakRecovery {
    fn name(&self) -> &str { "loss_streak_recovery" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let losses = ctx.portfolio.consecutive_losses;
        if losses < 3 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} losses", losses), losses as f64, 3.0)
        }
    }
}
