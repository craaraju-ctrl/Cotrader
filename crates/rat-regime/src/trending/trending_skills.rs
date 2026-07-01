//! Trending Skills

pub enum TrendingSkill {
    Detection,
    Transition,
}

impl TrendingSkill {
    pub fn name(&self) -> &'static str {
        match self {
            TrendingSkill::Detection => "Detection",
            TrendingSkill::Transition => "Transition",
        }
    }
}
