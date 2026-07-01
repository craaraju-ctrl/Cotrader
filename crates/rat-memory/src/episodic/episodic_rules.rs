//! Episodic Rules

pub enum EpisodicRule {
    MaxEntries(usize),
    RetentionDays(u64),
}

impl EpisodicRule {
    pub fn name(&self) -> &'static str {
        match self {
            EpisodicRule::MaxEntries(_) => "MaxEntries",
            EpisodicRule::RetentionDays(_) => "RetentionDays",
        }
    }
}
