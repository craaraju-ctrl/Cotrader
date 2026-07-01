//! Arbitrage Rules

pub enum ArbitrageRule {
    MinConfluence(f64),
    MaxDrawdown(f64),
}

impl ArbitrageRule {
    pub fn name(&self) -> &'static str {
        match self {
            ArbitrageRule::MinConfluence(_) => "MinConfluence",
            ArbitrageRule::MaxDrawdown(_) => "MaxDrawdown",
        }
    }
}
