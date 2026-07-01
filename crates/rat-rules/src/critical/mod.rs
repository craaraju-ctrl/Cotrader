//! Critical rules — never overridden, always block.

pub mod trading_enabled;
pub mod daily_drawdown;
pub mod red_folder;
pub mod session_timing;
pub mod max_absolute_drawdown;
pub mod black_swan_detector;

use crate::rule::Rule;

pub fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(trading_enabled::TradingEnabled),
        Box::new(daily_drawdown::DailyDrawdown),
        Box::new(red_folder::RedFolder),
        Box::new(session_timing::SessionTiming),
        Box::new(max_absolute_drawdown::MaxAbsoluteDrawdown),
        Box::new(black_swan_detector::BlackSwanDetector),
    ]
}
