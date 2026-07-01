//! Stochastic Tools

pub enum StochasticTool {
    DataFetcher,
    Calculator,
}

impl StochasticTool {
    pub fn name(&self) -> &'static str {
        match self {
            StochasticTool::DataFetcher => "DataFetcher",
            StochasticTool::Calculator => "Calculator",
        }
    }
}
