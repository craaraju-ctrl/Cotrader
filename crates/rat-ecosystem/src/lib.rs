pub mod registry;
pub mod reasoning;
pub mod thinking;
pub mod skills;
pub mod indicators;
pub mod decision;
pub mod data_apis;
pub mod web_tools;
pub mod sentiment;
pub mod onchain;
pub mod news;
pub mod extra;

/// Central registry of all 1000+ tools, skills, and capabilities.
pub struct Ecosystem {
    pub registry: registry::ToolRegistry,
}
