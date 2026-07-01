//! Medium rules — block only if no higher rule overrides.

pub mod regime_safety;
pub mod confluence_minimum;
pub mod correlation_heat;
pub mod mae_tracking;
pub mod session_risk_budget;
pub mod time_of_day_filter;
pub mod news_event_proximity;
pub mod win_streak_greed;
pub mod loss_streak_recovery;

use crate::rule::Rule;

pub fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(regime_safety::RegimeSafety),
        Box::new(confluence_minimum::ConfluenceMinimum),
        Box::new(correlation_heat::CorrelationHeat),
        Box::new(mae_tracking::MaeTracking),
        Box::new(session_risk_budget::SessionRiskBudget),
        Box::new(time_of_day_filter::TimeOfDayFilter),
        Box::new(news_event_proximity::NewsEventProximity),
        Box::new(win_streak_greed::WinStreakGreed),
        Box::new(loss_streak_recovery::LossStreakRecovery),
    ]
}
