//! Medium: 5+ consecutive wins warning.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct WinStreakGreed;

#[async_trait]
impl Rule for WinStreakGreed {
    fn name(&self) -> &str { "win_streak_greed" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let wins = ctx.portfolio.winning_trades_today;
        if wins < 5 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} wins", wins), wins as f64, 5.0)
        }
    }
}
