//! Working Memory

pub struct WorkingMemory;

impl WorkingMemory {
    pub fn name() -> &'static str { "WorkingMemory" }
    pub fn store(&self, _key: &str, _value: &str) {}
    pub fn retrieve(&self, _key: &str) -> Option<String> { None }
}
