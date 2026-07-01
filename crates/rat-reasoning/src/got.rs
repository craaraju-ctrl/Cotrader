use crate::error::ReasoningError;
use crate::chain::State;

/// Graph-of-Thought: non-linear reasoning with feedback loops.
///
/// Thoughts are vertices in a DAG. Edges represent:
/// - Dependency: thought B needs thought A's output
/// - Aggregation: merge multiple thoughts into one
/// - Refinement: loop back to improve a thought

#[derive(Debug, Clone)]
pub enum EdgeType {
    Dependency,
    Aggregation,
    Refinement,
}

#[derive(Debug, Clone)]
pub struct GoTNode {
    pub id: usize,
    pub name: String,
    pub state: State,
    pub score: f64,
    pub resolved: bool,
}

#[derive(Debug, Clone)]
pub struct GoTEdge {
    pub from: usize,
    pub to: usize,
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone)]
pub struct GoTGraph {
    pub nodes: Vec<GoTNode>,
    pub edges: Vec<GoTEdge>,
    next_id: usize,
}

impl GoTGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new(), next_id: 0 }
    }

    pub fn add_node(&mut self, name: &str, state: State) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(GoTNode {
            id, name: name.to_string(), state, score: 0.0, resolved: false,
        });
        id
    }

    pub fn add_dependency(&mut self, from: usize, to: usize) {
        self.edges.push(GoTEdge { from, to, edge_type: EdgeType::Dependency });
    }

    pub fn aggregate(&mut self, sources: &[usize], name: &str, merged: State) -> usize {
        let new_id = self.add_node(name, merged);
        for &src in sources {
            self.edges.push(GoTEdge { from: src, to: new_id, edge_type: EdgeType::Aggregation });
        }
        new_id
    }

    pub fn refine(&mut self, source: usize, name: &str, improved: State) -> usize {
        let new_id = self.add_node(name, improved);
        self.edges.push(GoTEdge { from: source, to: new_id, edge_type: EdgeType::Refinement });
        new_id
    }

    /// Topological sort — returns nodes in dependency order.
    pub fn topological_order(&self) -> Result<Vec<usize>, ReasoningError> {
        let n = self.nodes.len();
        let mut in_degree = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for edge in &self.edges {
            if edge.from < n && edge.to < n {
                adj[edge.from].push(edge.to);
                in_degree[edge.to] += 1;
            }
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::new();

        while let Some(node) = queue.pop() {
            order.push(node);
            for &next in &adj[node] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push(next);
                }
            }
        }

        if order.len() != n {
            return Err(ReasoningError::CycleDetected);
        }
        Ok(order)
    }

    /// Resolve the graph: execute nodes in topological order.
    pub fn resolve(&mut self, evaluator: &dyn Fn(&GoTNode) -> f64) -> Result<Vec<usize>, ReasoningError> {
        let order = self.topological_order()?;

        for &node_id in &order {
            let score = evaluator(&self.nodes[node_id]);
            self.nodes[node_id].score = score;
            self.nodes[node_id].resolved = true;
        }

        Ok(order)
    }

    /// Get the best final node (highest score among leaf nodes).
    pub fn best_result(&self) -> Option<&GoTNode> {
        // Leaf nodes: no outgoing Dependency or Aggregation edges
        let leaves: Vec<usize> = self.nodes.iter()
            .filter(|n| {
                !self.edges.iter().any(|e| e.from == n.id &&
                    matches!(e.edge_type, EdgeType::Dependency | EdgeType::Aggregation))
            })
            .map(|n| n.id)
            .collect();

        leaves.iter()
            .filter_map(|&id| self.nodes.iter().find(|n| n.id == id))
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// Trading: build a GoT graph for multi-factor analysis.
pub fn build_trading_graph(
    macro_state: State,
    technical_state: State,
    sentiment_state: State,
) -> GoTGraph {
    let mut graph = GoTGraph::new();

    // Independent analysis branches
    let macro_id = graph.add_node("macro_analysis", macro_state);
    let tech_id = graph.add_node("technical_analysis", technical_state);
    let sent_id = graph.add_node("sentiment_analysis", sentiment_state);

    // Aggregate into unified view
    let mut combined = State::new();
    combined.insert("factors_analyzed".into(), serde_json::json!(3));
    let _unified_id = graph.aggregate(&[macro_id, tech_id, sent_id], "unified_view", combined);

    // Refinement loop: if unified view has low confidence, refine each branch
    let mut refined_macro = State::new();
    refined_macro.insert("refined".into(), serde_json::json!(true));
    let refined_id = graph.refine(macro_id, "refined_macro", refined_macro);

    graph.aggregate(&[refined_id, tech_id, sent_id], "final_view", State::new());

    graph
}
