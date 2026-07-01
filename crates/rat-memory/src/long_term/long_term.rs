//! LongTerm Memory

pub struct LongTermMemory;

impl LongTermMemory {
    pub fn name() -> &'static str { "LongTermMemory" }
    pub fn store(&self, _key: &str, _value: &str) {}
    pub fn retrieve(&self, _key: &str) -> Option<String> { None }
}
