//! Medium: Avoid first/last 15 min of session.

use async_trait::async_trait;
use chrono::Timelike;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct TimeOfDayFilter;

#[async_trait]
impl Rule for TimeOfDayFilter {
    fn name(&self) -> &str { "time_of_day_filter" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let is_crypto = rat_core::is_crypto_symbol(ctx.symbol);
        if is_crypto {
            return RuleResult::pass(self.name(), self.priority());
        }
        let now = chrono::Utc::now();
        let time_val = now.hour() as f64 + now.minute() as f64 / 60.0;
        let in_buffer = time_val < 9.25 || time_val > 15.5;
        if !in_buffer {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), "Buffer zone", time_val, 9.25)
        }
    }
}
