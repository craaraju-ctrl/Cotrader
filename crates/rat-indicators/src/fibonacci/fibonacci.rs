//! Fibonacci Indicator

pub struct FibonacciIndicator;

impl FibonacciIndicator {
    pub fn name() -> &'static str { "FibonacciIndicator" }
    pub fn calculate(&self, _data: &[f64]) -> f64 { 0.0 }
}
