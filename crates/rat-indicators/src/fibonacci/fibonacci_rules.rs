//! Fibonacci Rules

pub enum FibonacciRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl FibonacciRule {
    pub fn name(&self) -> &'static str {
        match self {
            FibonacciRule::OverboughtThreshold(_) => "OverboughtThreshold",
            FibonacciRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
