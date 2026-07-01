//! MeanReversion Skills

pub enum MeanReversionSkill {
    SignalGeneration,
    Backtesting,
}

impl MeanReversionSkill {
    pub fn name(&self) -> &'static str {
        match self {
            MeanReversionSkill::SignalGeneration => "SignalGeneration",
            MeanReversionSkill::Backtesting => "Backtesting",
        }
    }
}
