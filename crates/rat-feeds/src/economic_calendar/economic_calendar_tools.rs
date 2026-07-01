//! EconomicCalendar Tools

pub enum EconomicCalendarTool {
    ApiClient,
    Parser,
}

impl EconomicCalendarTool {
    pub fn name(&self) -> &'static str {
        match self {
            EconomicCalendarTool::ApiClient => "ApiClient",
            EconomicCalendarTool::Parser => "Parser",
        }
    }
}
