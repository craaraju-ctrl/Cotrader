//! Volume Tools

pub enum VolumeTool {
    DataFetcher,
    Calculator,
}

impl VolumeTool {
    pub fn name(&self) -> &'static str {
        match self {
            VolumeTool::DataFetcher => "DataFetcher",
            VolumeTool::Calculator => "Calculator",
        }
    }
}
