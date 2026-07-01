//! High: Total exposure must not exceed 30%.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct ExposureConcentration;

#[async_trait]
impl Rule for ExposureConcentration {
    fn name(&self) -> &str { "exposure_concentration" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let concentration = if ctx.portfolio.total_equity > 0.0 {
            ctx.portfolio.total_risk / ctx.portfolio.total_equity
        } else {
            0.0
        };
        if concentration <= 0.30 {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("Exposure {:.1}%", concentration * 100.0), concentration, 0.30)
        }
    }
}
