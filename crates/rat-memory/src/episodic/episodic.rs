//! Episodic Memory

pub struct EpisodicMemory;

impl EpisodicMemory {
    pub fn name() -> &'static str { "EpisodicMemory" }
    pub fn store(&self, _key: &str, _value: &str) {}
    pub fn retrieve(&self, _key: &str) -> Option<String> { None }
}
