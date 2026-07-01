//! Bollinger Rules

pub enum BollingerRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl BollingerRule {
    pub fn name(&self) -> &'static str {
        match self {
            BollingerRule::OverboughtThreshold(_) => "OverboughtThreshold",
            BollingerRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
