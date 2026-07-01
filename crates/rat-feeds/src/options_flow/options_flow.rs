//! OptionsFlow Feed

pub struct OptionsFlowFeed;

impl OptionsFlowFeed {
    pub fn name() -> &'static str { "OptionsFlowFeed" }
    pub fn fetch(&self) -> Vec<String> { vec![] }
}
