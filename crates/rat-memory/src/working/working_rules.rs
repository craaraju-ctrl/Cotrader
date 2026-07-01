//! Working Rules

pub enum WorkingRule {
    MaxEntries(usize),
    RetentionDays(u64),
}

impl WorkingRule {
    pub fn name(&self) -> &'static str {
        match self {
            WorkingRule::MaxEntries(_) => "MaxEntries",
            WorkingRule::RetentionDays(_) => "RetentionDays",
        }
    }
}
