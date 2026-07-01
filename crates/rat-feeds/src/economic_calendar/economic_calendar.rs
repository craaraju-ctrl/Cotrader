//! EconomicCalendar Feed

pub struct EconomicCalendarFeed;

impl EconomicCalendarFeed {
    pub fn name() -> &'static str { "EconomicCalendarFeed" }
    pub fn fetch(&self) -> Vec<String> { vec![] }
}
