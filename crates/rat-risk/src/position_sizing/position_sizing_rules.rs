//! PositionSizing Rules

pub enum PositionSizingRule {
    MaxThreshold(f64),
    WarningLevel(f64),
}

impl PositionSizingRule {
    pub fn name(&self) -> &'static str {
        match self {
            PositionSizingRule::MaxThreshold(_) => "MaxThreshold",
            PositionSizingRule::WarningLevel(_) => "WarningLevel",
        }
    }
}
