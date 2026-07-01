//! Semantic Memory

pub struct SemanticMemory;

impl SemanticMemory {
    pub fn name() -> &'static str { "SemanticMemory" }
    pub fn store(&self, _key: &str, _value: &str) {}
    pub fn retrieve(&self, _key: &str) -> Option<String> { None }
}
