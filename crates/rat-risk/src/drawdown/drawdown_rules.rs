//! Drawdown Rules

pub enum DrawdownRule {
    MaxThreshold(f64),
    WarningLevel(f64),
}

impl DrawdownRule {
    pub fn name(&self) -> &'static str {
        match self {
            DrawdownRule::MaxThreshold(_) => "MaxThreshold",
            DrawdownRule::WarningLevel(_) => "WarningLevel",
        }
    }
}
