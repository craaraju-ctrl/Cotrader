use crate::error::ReasoningError;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

/// Task queue pattern (BabyAGI / AutoGPT style).
///
/// Create tasks → prioritize → execute → spawn new tasks from results → loop.

#[derive(Debug, Clone)]
pub struct Task {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub priority: f64,
    pub state: serde_json::Value,
}

impl Eq for Task {}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.partial_cmp(&other.priority).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: u64,
    pub output: serde_json::Value,
    pub follow_up_tasks: Vec<Task>,
    pub success: bool,
}

/// Agent loop — execute tasks, spawn follow-ups, repeat until done.
pub struct AgentLoop {
    pub max_iterations: usize,
    pub max_tasks: usize,
}

impl AgentLoop {
    pub fn new(max_iterations: usize, max_tasks: usize) -> Self {
        Self { max_iterations, max_tasks }
    }

    /// Run the agent loop with a task executor.
    pub fn run<F>(
        &self,
        initial_tasks: Vec<Task>,
        executor: F,
    ) -> Result<Vec<TaskResult>, ReasoningError>
    where
        F: Fn(&Task) -> Result<TaskResult, ReasoningError>,
    {
        let mut queue: BinaryHeap<Task> = initial_tasks.into();
        let mut results = Vec::new();
        let mut total_tasks = 0;

        for _ in 0..self.max_iterations {
            let task = match queue.pop() {
                Some(t) => t,
                None => break,
            };

            let result = executor(&task)?;
            total_tasks += 1;

            // Spawn follow-up tasks
            for mut follow_up in result.follow_up_tasks.clone() {
                if total_tasks + queue.len() < self.max_tasks {
                    follow_up.id = total_tasks as u64 + queue.len() as u64 + 1;
                    queue.push(follow_up);
                }
            }

            results.push(result);
        }

        Ok(results)
    }

    /// Run with state accumulation — each task's output feeds into a shared state.
    pub fn run_with_state<F>(
        &self,
        initial_tasks: Vec<Task>,
        mut state: serde_json::Value,
        executor: F,
    ) -> Result<(serde_json::Value, Vec<TaskResult>), ReasoningError>
    where
        F: Fn(&Task, &serde_json::Value) -> Result<TaskResult, ReasoningError>,
    {
        let mut queue: BinaryHeap<Task> = initial_tasks.into();
        let mut results = Vec::new();
        let mut total_tasks = 0;

        for _ in 0..self.max_iterations {
            let task = match queue.pop() {
                Some(t) => t,
                None => break,
            };

            let result = executor(&task, &state)?;

            // Merge output into state
            if let (Some(state_obj), Some(output)) = (state.as_object_mut(), result.output.as_object()) {
                for (k, v) in output {
                    state_obj.insert(k.clone(), v.clone());
                }
            }

            for mut follow_up in result.follow_up_tasks.clone() {
                if total_tasks + queue.len() < self.max_tasks {
                    follow_up.id = total_tasks as u64 + queue.len() as u64 + 1;
                    queue.push(follow_up);
                }
            }

            results.push(result);
            total_tasks += 1;
        }

        Ok((state, results))
    }
}

/// Trading: create initial research tasks for market analysis.
pub fn create_market_research_tasks(symbols: &[String]) -> Vec<Task> {
    symbols.iter().enumerate().map(|(i, symbol)| {
        Task {
            id: i as u64,
            name: format!("analyze_{}", symbol),
            description: format!("Complete technical and fundamental analysis for {}", symbol),
            priority: 1.0,
            state: serde_json::json!({ "symbol": symbol }),
        }
    }).collect()
}
