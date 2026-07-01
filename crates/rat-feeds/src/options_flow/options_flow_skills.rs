//! OptionsFlow Skills

pub enum OptionsFlowSkill {
    Fetch,
    Parse,
}

impl OptionsFlowSkill {
    pub fn name(&self) -> &'static str {
        match self {
            OptionsFlowSkill::Fetch => "Fetch",
            OptionsFlowSkill::Parse => "Parse",
        }
    }
}
