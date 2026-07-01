//! Rule trait and result types.

use async_trait::async_trait;
use crate::context::RuleContext;

/// Priority levels for rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RulePriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

/// Result of evaluating a single rule.
pub struct RuleResult {
    pub passed: bool,
    pub rule_name: String,
    pub priority: RulePriority,
    pub reason: String,
    pub observed: f64,
    pub threshold: f64,
}

impl RuleResult {
    pub fn pass(name: &str, priority: RulePriority) -> Self {
        Self {
            passed: true,
            rule_name: name.to_string(),
            priority,
            reason: String::new(),
            observed: 0.0,
            threshold: 0.0,
        }
    }

    pub fn fail(name: &str, priority: RulePriority, reason: &str, observed: f64, threshold: f64) -> Self {
        Self {
            passed: false,
            rule_name: name.to_string(),
            priority,
            reason: reason.to_string(),
            observed,
            threshold,
        }
    }
}

/// Trait that every trading rule implements.
#[async_trait]
pub trait Rule: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> RulePriority;
    async fn evaluate(&self, ctx: &RuleContext<'_>) -> RuleResult;
}
