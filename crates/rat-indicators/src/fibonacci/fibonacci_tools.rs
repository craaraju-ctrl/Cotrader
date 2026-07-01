//! Fibonacci Tools

pub enum FibonacciTool {
    DataFetcher,
    Calculator,
}

impl FibonacciTool {
    pub fn name(&self) -> &'static str {
        match self {
            FibonacciTool::DataFetcher => "DataFetcher",
            FibonacciTool::Calculator => "Calculator",
        }
    }
}
