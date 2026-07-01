//! Stochastic Skills

pub enum StochasticSkill {
    Calculation,
    SignalGeneration,
}

impl StochasticSkill {
    pub fn name(&self) -> &'static str {
        match self {
            StochasticSkill::Calculation => "Calculation",
            StochasticSkill::SignalGeneration => "SignalGeneration",
        }
    }
}
