use crate::error::ReasoningError;
use crate::chain::State;

/// Tree-of-Thought: explore multiple reasoning branches, evaluate, prune.
///
/// At each depth level:
/// 1. Generate N candidate states from each frontier node
/// 2. Score each candidate with a heuristic
/// 3. Keep top K (branch factor)
/// 4. Repeat until max_depth
pub struct TreeOfThought {
    pub max_depth: usize,
    pub branch_factor: usize,
    pub generate_fn: Box<dyn Fn(&State) -> Vec<State> + Send + Sync>,
    pub evaluate_fn: Box<dyn Fn(&State) -> f64 + Send + Sync>,
}

#[derive(Debug, Clone)]
pub struct ThoughtNode {
    pub state: State,
    pub score: f64,
    pub depth: usize,
    pub path: Vec<usize>,
}

impl TreeOfThought {
    pub fn new(
        max_depth: usize,
        branch_factor: usize,
        generate: impl Fn(&State) -> Vec<State> + Send + Sync + 'static,
        evaluate: impl Fn(&State) -> f64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            max_depth,
            branch_factor,
            generate_fn: Box::new(generate),
            evaluate_fn: Box::new(evaluate),
        }
    }

    /// BFS expansion — solve returns the best reasoning paths.
    pub fn solve(&self, root: State) -> Result<Vec<ThoughtNode>, ReasoningError> {
        let root_score = (self.evaluate_fn)(&root);
        let mut frontier = vec![ThoughtNode {
            state: root,
            score: root_score,
            depth: 0,
            path: Vec::new(),
        }];

        for depth in 0..self.max_depth {
            let mut candidates: Vec<ThoughtNode> = Vec::new();

            for (idx, node) in frontier.iter().enumerate() {
                let children = (self.generate_fn)(&node.state);
                for (child_idx, child_state) in children.into_iter().enumerate() {
                    let score = (self.evaluate_fn)(&child_state);
                    let mut path = node.path.clone();
                    path.push(idx * 100 + child_idx);
                    candidates.push(ThoughtNode {
                        state: child_state,
                        score,
                        depth: depth + 1,
                        path,
                    });
                }
            }

            if candidates.is_empty() {
                return Err(ReasoningError::PruningExhausted { depth });
            }

            // Sort by score descending, keep top branch_factor
            candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(self.branch_factor);
            frontier = candidates;
        }

        Ok(frontier)
    }

    /// Get the single best reasoning path.
    pub fn best(&self, root: State) -> Result<ThoughtNode, ReasoningError> {
        let results = self.solve(root)?;
        results.into_iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or(ReasoningError::PruningExhausted { depth: 0 })
    }
}

/// Trading-specific: evaluate portfolio allocation candidates.
pub fn evaluate_portfolio(state: &State) -> f64 {
    let risk = state.get("risk").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let return_est = state.get("expected_return").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let diversification = state.get("diversification").and_then(|v| v.as_f64()).unwrap_or(0.5);

    // Simple Sharpe-like heuristic
    let sharpe_proxy = return_est / risk.max(0.01);
    sharpe_proxy * 0.6 + diversification * 0.4
}

/// Generate candidate portfolio allocations by perturbing current weights.
pub fn generate_allocations(current: &State) -> Vec<State> {
    let weights = current.get("weights").cloned().unwrap_or(serde_json::json!({}));
    let mut candidates = Vec::new();

    if let Some(obj) = weights.as_object() {
        for (asset, weight) in obj {
            if let Some(w) = weight.as_f64() {
                // Create variation: +5%, -5%, and current
                for delta in [-0.05, 0.05, 0.0] {
                    let new_w = (w + delta).max(0.0).min(1.0);
                    let mut new_state = current.clone();
                    if let Some(map) = new_state.get_mut("weights").and_then(|v| v.as_object_mut()) {
                        map.insert(asset.clone(), serde_json::json!(new_w));
                    }
                    let risk_delta = delta.abs() * 0.3;
                    new_state.insert("risk".into(), serde_json::json!(
                        current.get("risk").and_then(|v| v.as_f64()).unwrap_or(0.5) + risk_delta
                    ));
                    candidates.push(new_state);
                }
            }
        }
    }

    if candidates.is_empty() {
        candidates.push(current.clone());
    }
    candidates
}
