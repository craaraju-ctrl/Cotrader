//! ElliottWave Tools

pub enum ElliottWaveTool {
    Detector,
    Validator,
}

impl ElliottWaveTool {
    pub fn name(&self) -> &'static str {
        match self {
            ElliottWaveTool::Detector => "Detector",
            ElliottWaveTool::Validator => "Validator",
        }
    }
}
