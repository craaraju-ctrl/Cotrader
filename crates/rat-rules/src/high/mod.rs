//! High rules — always block when triggered.

pub mod portfolio_heat;
pub mod loss_circuit_breaker;
pub mod max_daily_trades;
pub mod cooldown;
pub mod kelly_sizing;
pub mod vol_adjusted_stops;
pub mod liquidity_check;
pub mod exposure_concentration;
pub mod order_size_limits;
pub mod margin_utilization;

use crate::rule::Rule;

pub fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(portfolio_heat::PortfolioHeat),
        Box::new(loss_circuit_breaker::LossCircuitBreaker),
        Box::new(max_daily_trades::MaxDailyTrades),
        Box::new(cooldown::Cooldown),
        Box::new(kelly_sizing::KellySizing),
        Box::new(vol_adjusted_stops::VolAdjustedStops),
        Box::new(liquidity_check::LiquidityCheck),
        Box::new(exposure_concentration::ExposureConcentration),
        Box::new(order_size_limits::OrderSizeLimits),
        Box::new(margin_utilization::MarginUtilization),
    ]
}
