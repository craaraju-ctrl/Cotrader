//! Graph Rules

pub enum GraphRule {
    MaxEntries(usize),
    RetentionDays(u64),
}

impl GraphRule {
    pub fn name(&self) -> &'static str {
        match self {
            GraphRule::MaxEntries(_) => "MaxEntries",
            GraphRule::RetentionDays(_) => "RetentionDays",
        }
    }
}
