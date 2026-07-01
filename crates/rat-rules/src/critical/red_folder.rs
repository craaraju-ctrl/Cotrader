//! Critical: No high-impact news events today.

use async_trait::async_trait;
use crate::context::{RuleContext, EventImpact};
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct RedFolder;

#[async_trait]
impl Rule for RedFolder {
    fn name(&self) -> &str { "red_folder" }
    fn priority(&self) -> RulePriority { RulePriority::Critical }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let discipline_on = ctx.rules.red_folder_discipline;
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let count = ctx.calendar.iter().filter(|e| {
            e.impact == EventImpact::High && e.date == today
        }).count();
        if !discipline_on || count == 0 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("{} events", count), count as f64, 0.0)
        }
    }
}
