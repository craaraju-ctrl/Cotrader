//! OnChain Skills

pub enum OnChainSkill {
    Fetch,
    Parse,
}

impl OnChainSkill {
    pub fn name(&self) -> &'static str {
        match self {
            OnChainSkill::Fetch => "Fetch",
            OnChainSkill::Parse => "Parse",
        }
    }
}
