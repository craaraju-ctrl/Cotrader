//! Critical: Market must be open (crypto bypasses).

use async_trait::async_trait;
use crate::context::RuleContext;
use crate::rule::{Rule, RulePriority, RuleResult};

pub struct SessionTiming;

#[async_trait]
impl Rule for SessionTiming {
    fn name(&self) -> &str { "session_timing" }
    fn priority(&self) -> RulePriority { RulePriority::Critical }

    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult {
        let is_crypto = rat_core::is_crypto_symbol(ctx.symbol);
        if is_crypto {
            return RuleResult::pass(self.name(), self.priority());
        }
        if ctx.rules.respect_session_timing {
            let now = chrono::Utc::now();
            let open = rat_core::is_in_trading_session(now, ctx.rules);
            if open {
                RuleResult::pass(self.name(), self.priority())
            } else {
                RuleResult::fail(self.name(), self.priority(), "Market closed", 0.0, 1.0)
            }
        } else {
            RuleResult::pass(self.name(), self.priority())
        }
    }
}
