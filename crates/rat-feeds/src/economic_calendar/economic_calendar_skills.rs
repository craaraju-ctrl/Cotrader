//! EconomicCalendar Skills

pub enum EconomicCalendarSkill {
    Fetch,
    Parse,
}

impl EconomicCalendarSkill {
    pub fn name(&self) -> &'static str {
        match self {
            EconomicCalendarSkill::Fetch => "Fetch",
            EconomicCalendarSkill::Parse => "Parse",
        }
    }
}
