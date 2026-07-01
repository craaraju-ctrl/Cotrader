//! OptionsSentiment Tools

pub enum OptionsSentimentTool {
    Analyzer,
    Scorer,
}

impl OptionsSentimentTool {
    pub fn name(&self) -> &'static str {
        match self {
            OptionsSentimentTool::Analyzer => "Analyzer",
            OptionsSentimentTool::Scorer => "Scorer",
        }
    }
}
