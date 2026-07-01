//! Volume Rules

pub enum VolumeRule {
    OverboughtThreshold(f64),
    OversoldThreshold(f64),
}

impl VolumeRule {
    pub fn name(&self) -> &'static str {
        match self {
            VolumeRule::OverboughtThreshold(_) => "OverboughtThreshold",
            VolumeRule::OversoldThreshold(_) => "OversoldThreshold",
        }
    }
}
