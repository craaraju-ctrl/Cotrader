use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool category in the ecosystem
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Category {
    ReasoningFramework,
    ThinkingStyle,
    Skill,
    Indicator,
    DecisionModel,
    DataSource,
    WebTool,
    SentimentTool,
    OnChainTool,
    NewsSource,
    RiskModel,
    ExecutionAlgo,
    PortfolioMethod,
    MLModel,
    BacktestMethod,
    OrderType,
    RiskMetric,
    RegimeType,
    TimeFrame,
    PatternType,
}

/// Individual tool/capability entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub description: String,
    pub subcategory: String,
    pub complexity: Complexity,
    pub tags: Vec<String>,
    pub implementation: Implementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Complexity {
    Basic,
    Intermediate,
    Advanced,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Implementation {
    BuiltIn,
    ExternalApi { base_url: String },
    CrateDependency { crate_name: String },
    HttpEndpoint { url: String },
    WebSocket { url: String },
    File { path: String },
}

/// Central registry holding all tools
pub struct ToolRegistry {
    pub tools: Vec<ToolEntry>,
    pub by_category: HashMap<Category, Vec<usize>>,
    pub by_tag: HashMap<String, Vec<usize>>,
    pub by_name: HashMap<String, usize>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            by_category: HashMap::new(),
            by_tag: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    pub fn register(&mut self, entry: ToolEntry) {
        let idx = self.tools.len();
        let name = entry.name.clone();
        let cat = entry.category.clone();
        let tags = entry.tags.clone();

        self.by_category.entry(cat).or_default().push(idx);
        for tag in &tags {
            self.by_tag.entry(tag.clone()).or_default().push(idx);
        }
        self.by_name.insert(name, idx);
        self.tools.push(entry);
    }

    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.by_name.get(name).map(|&i| &self.tools[i])
    }

    pub fn by_category(&self, cat: &Category) -> Vec<&ToolEntry> {
        self.by_category
            .get(cat)
            .map(|indices| indices.iter().map(|&i| &self.tools[i]).collect())
            .unwrap_or_default()
    }

    pub fn by_tag(&self, tag: &str) -> Vec<&ToolEntry> {
        self.by_tag
            .get(tag)
            .map(|indices| indices.iter().map(|&i| &self.tools[i]).collect())
            .unwrap_or_default()
    }

    pub fn search(&self, query: &str) -> Vec<&ToolEntry> {
        let q = query.to_lowercase();
        self.tools
            .iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
                    || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }

    pub fn category_counts(&self) -> HashMap<Category, usize> {
        self.by_category
            .iter()
            .map(|(cat, indices)| (cat.clone(), indices.len()))
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
