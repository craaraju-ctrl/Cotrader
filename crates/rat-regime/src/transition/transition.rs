//! Transition Regime

pub struct TransitionRegime;

impl TransitionRegime {
    pub fn name() -> &'static str { "TransitionRegime" }
    pub fn detect(&self) -> bool { false }
}
