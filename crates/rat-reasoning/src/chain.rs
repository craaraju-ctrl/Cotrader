use crate::error::ReasoningError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Shared state that flows through all reasoning chains.
pub type State = HashMap<String, serde_json::Value>;

/// A single reasoning step — transforms state.
pub struct Step {
    pub name: String,
    pub apply: Box<dyn Fn(&mut State) -> Result<(), ReasoningError>>,
}

impl Step {
    pub fn new(name: &str, apply: impl Fn(&mut State) -> Result<(), ReasoningError> + 'static) -> Self {
        Self { name: name.to_string(), apply: Box::new(apply) }
    }
}

/// Chain-of-Thought: sequential reasoning pipeline.
///
/// State flows through ordered steps, each transforming it.
/// Each step is independently testable and composable.
pub struct ChainOfThought {
    steps: Vec<Step>,
}

impl ChainOfThought {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn step(mut self, name: &str, f: impl Fn(&mut State) -> Result<(), ReasoningError> + 'static) -> Self {
        self.steps.push(Step::new(name, f));
        self
    }

    pub fn run(&self, mut state: State) -> Result<State, ReasoningError> {
        for step in &self.steps {
            (step.apply)(&mut state).map_err(|e| ReasoningError::StepFailed {
                step: step.name.clone(),
                message: format!("{}", e),
            })?;
        }
        Ok(state)
    }

    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    pub fn step_names(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.name.as_str()).collect()
    }
}

impl Default for ChainOfThought {
    fn default() -> Self { Self::new() }
}

/// Trait unifying all reasoning chain types for composition.
pub trait ReasoningChain: Send + Sync {
    type Input: Send;
    type Output: Send;

    fn execute(&self, input: Self::Input) -> Result<Self::Output, ReasoningError>;
    fn name(&self) -> &str;
}

/// A trace entry recording what happened at each reasoning step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub step: String,
    pub timestamp: String,
    pub state_snapshot: serde_json::Value,
    pub duration_ms: u64,
}

/// Orchestrator that runs a reasoning chain and records a trace.
pub struct ReasoningOrchestrator {
    pub traces: Vec<TraceEntry>,
}

impl ReasoningOrchestrator {
    pub fn new() -> Self {
        Self { traces: Vec::new() }
    }

    pub fn record(&mut self, step: &str, state: &State, duration_ms: u64) {
        self.traces.push(TraceEntry {
            step: step.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            state_snapshot: serde_json::to_value(state).unwrap_or_default(),
            duration_ms,
        });
    }

    pub fn summary(&self) -> String {
        let total_ms: u64 = self.traces.iter().map(|t| t.duration_ms).sum();
        format!("Reasoning trace: {} steps, {}ms total", self.traces.len(), total_ms)
    }
}

/// Builder pattern for constructing ChainOfThought pipelines.
pub struct ChainBuilder {
    chain: ChainOfThought,
}

impl ChainBuilder {
    pub fn new() -> Self {
        Self { chain: ChainOfThought::new() }
    }

    pub fn add_step(mut self, name: &str, f: impl Fn(&mut State) -> Result<(), ReasoningError> + 'static) -> Self {
        self.chain = self.chain.step(name, f);
        self
    }

    pub fn build(self) -> ChainOfThought {
        self.chain
    }
}

impl Default for ChainBuilder {
    fn default() -> Self { Self::new() }
}
