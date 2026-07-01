//! Graph Memory

pub struct GraphMemory;

impl GraphMemory {
    pub fn name() -> &'static str { "GraphMemory" }
    pub fn store(&self, _key: &str, _value: &str) {}
    pub fn retrieve(&self, _key: &str) -> Option<String> { None }
}
