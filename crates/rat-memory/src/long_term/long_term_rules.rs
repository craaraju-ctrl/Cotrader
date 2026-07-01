//! LongTerm Rules

pub enum LongTermRule {
    MaxEntries(usize),
    RetentionDays(u64),
}

impl LongTermRule {
    pub fn name(&self) -> &'static str {
        match self {
            LongTermRule::MaxEntries(_) => "MaxEntries",
            LongTermRule::RetentionDays(_) => "RetentionDays",
        }
    }
}
