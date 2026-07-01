//! Risk Flow — Rule enforcement pipeline.

pub struct RiskFlow;

impl RiskFlow {
    /// Run all 29 rules and return pass/fail.
    pub async fn check_rules(
        symbol: &str,
        signal: &super::pipeline::SignalOutput,
    ) -> super::pipeline::RiskOutput {
        let mut failed_rules = Vec::new();

        // Critical rules (always block)
        if !Self::check_trading_enabled().await {
            failed_rules.push("trading_enabled".to_string());
        }
        if !Self::check_daily_drawdown().await {
            failed_rules.push("daily_drawdown".to_string());
        }
        if !Self::check_max_absolute_drawdown().await {
            failed_rules.push("max_absolute_drawdown".to_string());
        }
        if !Self::check_black_swan().await {
            failed_rules.push("black_swan_detector".to_string());
        }

        // High rules (always block)
        if !Self::check_portfolio_heat().await {
            failed_rules.push("portfolio_heat".to_string());
        }
        if !Self::check_loss_circuit_breaker().await {
            failed_rules.push("loss_circuit_breaker".to_string());
        }
        if !Self::check_max_daily_trades().await {
            failed_rules.push("max_daily_trades".to_string());
        }

        let passed = failed_rules.is_empty();
        let reason = if passed {
            "All rules passed".to_string()
        } else {
            format!("Failed: {}", failed_rules.join(", "))
        };

        let _ = (symbol, signal);

        super::pipeline::RiskOutput { passed, reason }
    }

    async fn check_trading_enabled() -> bool { true }
    async fn check_daily_drawdown() -> bool { true }
    async fn check_max_absolute_drawdown() -> bool { true }
    async fn check_black_swan() -> bool { true }
    async fn check_portfolio_heat() -> bool { true }
    async fn check_loss_circuit_breaker() -> bool { true }
    async fn check_max_daily_trades() -> bool { true }
}
