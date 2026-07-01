//! Medium: Don't enter 30 min before high-impact news.

use async_trait::async_trait;
use crate::context::{RuleContext, EventImpact};
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct NewsEventProximity;

#[async_trait]
impl Rule for NewsEventProximity {
    fn name(&self) -> &str { "news_event_proximity" }
    fn priority(&self) -> RulePriority { RulePriority::Medium }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let is_crypto = rat_core::is_crypto_symbol(ctx.symbol);
        if is_crypto {
            return RuleResult::pass(self.name(), self.priority());
        }
        let now = chrono::Utc::now();
        let near = ctx.calendar.iter().any(|e| {
            if e.impact != EventImpact::High { return false; }
            if let Some(ref t) = e.time {
                if let Ok(event_time) = chrono::NaiveDateTime::parse_from_str(
                    &format!("{} {}", e.date, t), "%Y-%m-%d %H:%M"
                ) {
                    (event_time - now.naive_utc()).num_minutes().abs() < 30
                } else { false }
            } else { false }
        });
        if !near {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), "News within 30 min", 1.0, 0.0)
        }
    }
}
