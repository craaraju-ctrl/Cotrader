//! High: Portfolio heat (total risk) must not exceed 10%.

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct PortfolioHeat;

#[async_trait]
impl Rule for PortfolioHeat {
    fn name(&self) -> &str { "portfolio_heat" }
    fn priority(&self) -> RulePriority { RulePriority::High }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let heat = if ctx.portfolio.total_equity > 0.0 {
            ctx.portfolio.total_risk / ctx.portfolio.total_equity
        } else {
            0.0
        };
        let limit = if ctx.sigma > 0.03 { 0.10 * (1.0 - ctx.sigma * 0.5) } else { 0.10 };
        if heat <= limit {
            RuleResult::pass(self.name(), self.priority())
        } else {
            RuleResult::fail(self.name(), self.priority(), &format!("Heat {:.1}%", heat * 100.0), heat, limit)
        }
    }
}
