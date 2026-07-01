//! Fibonacci Skills

pub enum FibonacciSkill {
    Calculation,
    SignalGeneration,
}

impl FibonacciSkill {
    pub fn name(&self) -> &'static str {
        match self {
            FibonacciSkill::Calculation => "Calculation",
            FibonacciSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
