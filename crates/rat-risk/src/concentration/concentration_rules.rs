//! Concentration Rules

pub enum ConcentrationRule {
    MaxThreshold(f64),
    WarningLevel(f64),
}

impl ConcentrationRule {
    pub fn name(&self) -> &'static str {
        match self {
            ConcentrationRule::MaxThreshold(_) => "MaxThreshold",
            ConcentrationRule::WarningLevel(_) => "WarningLevel",
        }
    }
}
