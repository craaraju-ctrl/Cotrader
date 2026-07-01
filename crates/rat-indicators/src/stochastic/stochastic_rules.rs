//! Stochastic Rules

pub enum StochasticRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl StochasticRule {
    pub fn name(&self) -> &'static str {
        match self {
            StochasticRule::OverboughtThreshold(_) => "OverboughtThreshold",
            StochasticRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
