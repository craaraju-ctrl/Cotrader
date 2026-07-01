use std::fmt;

#[derive(Debug, Clone)]
pub enum ReasoningError {
    StepFailed { step: String, message: String },
    MaxAttemptsExceeded { max: usize },
    Timeout { chain: String, elapsed_ms: u64 },
    CycleDetected,
    ConsensusFailed { votes: Vec<(String, usize)>, threshold: usize },
    PruningExhausted { depth: usize },
    NodeNotFound { id: usize },
    EmptyGraph,
    EvaluationFailed { node_id: usize },
}

impl fmt::Display for ReasoningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StepFailed { step, message } => write!(f, "Step '{}' failed: {}", step, message),
            Self::MaxAttemptsExceeded { max } => write!(f, "Max attempts ({}) exceeded", max),
            Self::Timeout { chain, elapsed_ms } => write!(f, "Chain '{}' timed out after {}ms", chain, elapsed_ms),
            Self::CycleDetected => write!(f, "Cycle detected in thought graph"),
            Self::ConsensusFailed { votes, threshold } => write!(f, "No consensus: votes={:?}, threshold={}", votes, threshold),
            Self::PruningExhausted { depth } => write!(f, "All branches pruned at depth {}", depth),
            Self::NodeNotFound { id } => write!(f, "Node {} not found in graph", id),
            Self::EmptyGraph => write!(f, "Graph has no nodes"),
            Self::EvaluationFailed { node_id } => write!(f, "Evaluation failed for node {}", node_id),
        }
    }
}

impl std::error::Error for ReasoningError {}
