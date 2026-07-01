//! Volatility Rules

pub enum VolatilityRule {
    MaxThreshold(f64),
    WarningLevel(f64),
}

impl VolatilityRule {
    pub fn name(&self) -> &'static str {
        match self {
            VolatilityRule::MaxThreshold(_) => "MaxThreshold",
            VolatilityRule::WarningLevel(_) => "WarningLevel",
        }
    }
}
