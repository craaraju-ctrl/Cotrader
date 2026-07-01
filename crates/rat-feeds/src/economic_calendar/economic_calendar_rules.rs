//! EconomicCalendar Rules

pub enum EconomicCalendarRule {
    MaxAge(u64),
    MinRelevance(f64),
}

impl EconomicCalendarRule {
    pub fn name(&self) -> &'static str {
        match self {
            EconomicCalendarRule::MaxAge(_) => "MaxAge",
            EconomicCalendarRule::MinRelevance(_) => "MinRelevance",
        }
    }
}
