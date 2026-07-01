//! OptionsFlow Rules

pub enum OptionsFlowRule {
    MaxAge(u64),
    MinRelevance(f64),
}

impl OptionsFlowRule {
    pub fn name(&self) -> &'static str {
        match self {
            OptionsFlowRule::MaxAge(_) => "MaxAge",
            OptionsFlowRule::MinRelevance(_) => "MinRelevance",
        }
    }
}
