//! Semantic Rules

pub enum SemanticRule {
    MaxEntries(usize),
    RetentionDays(u64),
}

impl SemanticRule {
    pub fn name(&self) -> &'static str {
        match self {
            SemanticRule::MaxEntries(_) => "MaxEntries",
            SemanticRule::RetentionDays(_) => "RetentionDays",
        }
    }
}
