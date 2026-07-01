use crate::registry::{Category, Complexity, Implementation, ToolEntry};

/// All reasoning frameworks available in the ecosystem
pub fn register_all(registry: &mut super::registry::ToolRegistry) {
    let frameworks = vec![
        // ── Chain-of-Thought Family ──────────────────────────────────
        ("cot", "Chain-of-Thought (CoT)", "Step-by-step sequential reasoning for multi-factor trade thesis", "standard",
         vec!["reasoning", "sequential", "multi-factor"], Complexity::Basic),
        ("zero_shot_cot", "Zero-Shot CoT", "Reasoning without examples — just prepend 'Let's think step by step'", "standard",
         vec!["reasoning", "zero-shot", "few-shot"], Complexity::Basic),
        ("auto_cot", "Auto-CoT", "Automatically generates reasoning chains from few-shot examples", "advanced",
         vec!["reasoning", "automatic", "few-shot"], Complexity::Intermediate),
        ("self_consistency", "Self-Consistency", "Samples multiple reasoning paths, picks majority answer — requires consensus for trade entry", "consensus",
         vec!["reasoning", "consensus", "ensemble"], Complexity::Intermediate),
        ("complexity_cot", "Complexity-Based CoT", "Selects reasoning chain length proportional to problem difficulty", "adaptive",
         vec!["reasoning", "adaptive", "complexity"], Complexity::Advanced),
        ("least_to_most", "Least-to-Most", "Decomposes complex problems into sub-problems solved sequentially", "decomposition",
         vec!["reasoning", "decomposition", "hierarchical"], Complexity::Intermediate),

        // ── Tree/Graph Family ────────────────────────────────────────
        ("tot", "Tree-of-Thought (ToT)", "Explores multiple reasoning branches, prunes weak paths — evaluate bull/bear/sideways scenarios simultaneously", "exploration",
         vec!["reasoning", "tree", "exploration", "branching"], Complexity::Advanced),
        ("got", "Graph-of-Thought (GoT)", "Non-linear reasoning with feedback loops — model interdependencies like rates→USD→commodities→equities", "graph",
         vec!["reasoning", "graph", "nonlinear", "feedback"], Complexity::Expert),
        ("monte_carlo_tree", "Monte Carlo Tree Search", "Simulate many market paths, select action with highest expected value", "simulation",
         vec!["reasoning", "simulation", "mcts", "tree-search"], Complexity::Expert),

        // ── Action-Oriented ──────────────────────────────────────────
        ("react", "ReAct (Reason+Act)", "Interleaves reasoning with real-time actions — analyze market state, execute orders, observe fills, re-reason", "action",
         vec!["reasoning", "action", "loop", "real-time"], Complexity::Intermediate),
        ("reflexion", "Reflexion", "Self-reflects on past failures to improve future decisions — after losing trade, analyze edge misidentification", "reflection",
         vec!["reasoning", "reflection", "self-improvement", "learning"], Complexity::Advanced),
        ("act_plan", "Act-Plan-Act", "Simple action-planning loop: observe → plan → act → observe", "action",
         vec!["reasoning", "action", "planning", "loop"], Complexity::Basic),

        // ── Decomposition ────────────────────────────────────────────
        ("skeleton_of_thought", "Skeleton-of-Thought", "Generates answer skeleton first, then fills details — outline trade plan then flesh each section", "skeleton",
         vec!["reasoning", "skeleton", "outline", "hierarchical"], Complexity::Intermediate),
        ("program_of_thought", "Program-of-Thought", "Generates executable code as reasoning output — write backtest inline, run it, reason over results", "code",
         vec!["reasoning", "code", "executable", "backtest"], Complexity::Advanced),
        ("divide_and_conquer", "Divide and Conquer", "Split problem into independent sub-problems, solve each separately, combine results", "decomposition",
         vec!["reasoning", "decomposition", "parallel"], Complexity::Intermediate),

        // ── Abstraction ──────────────────────────────────────────────
        ("step_back", "Step-Back Reasoning", "Asks high-level questions before diving in — 'What regime is the market in?' before analyzing positions", "abstraction",
         vec!["reasoning", "abstraction", "high-level", "regime"], Complexity::Intermediate),
        ("analogical", "Analogical Reasoning", "Maps patterns from known domains to current problems — '2024 AI rally resembles 1999 dot-com but with real revenue'", "mapping",
         vec!["reasoning", "analogical", "pattern-mapping", "historical"], Complexity::Advanced),
        ("abductive", "Abductive Reasoning", "Infers most likely explanation from incomplete data — volume spike + price dip + no news = large block trade", "inference",
         vec!["reasoning", "abductive", "inference", "incomplete-data"], Complexity::Advanced),

        // ── Dialectical ──────────────────────────────────────────────
        ("dialectical", "Dialectical Reasoning", "Thesis vs. antithesis → synthesis — bull case vs bear case explicitly debated, synthesis becomes trading thesis", "dialectic",
         vec!["reasoning", "dialectical", "debate", "synthesis"], Complexity::Advanced),
        ("adversarial", "Adversarial Reasoning", "Actively tries to disprove your own thesis — stress-test trade ideas before committing capital", "adversarial",
         vec!["reasoning", "adversarial", "devil-advocate", "stress-test"], Complexity::Advanced),

        // ── Probabilistic ────────────────────────────────────────────
        ("bayesian", "Bayesian Reasoning", "Updates beliefs probabilistically as new evidence arrives — prior: fairly valued, earnings beat → posterior: 10% upside", "probabilistic",
         vec!["reasoning", "bayesian", "probabilistic", "updating"], Complexity::Advanced),
        ("probabilistic", "Probabilistic Reasoning", "Reasons with uncertainty distributions — '60% chance breakout, 30% consolidation, 10% breakdown' → position accordingly", "probabilistic",
         vec!["reasoning", "probabilistic", "distribution", "uncertainty"], Complexity::Advanced),
        ("causal", "Causal Reasoning", "Identifies cause-effect chains — rate cut causes USD weakening causes emerging market inflows", "causal",
         vec!["reasoning", "causal", "cause-effect", "chain"], Complexity::Expert),

        // ── Metacognitive ────────────────────────────────────────────
        ("metacognitive", "Metacognitive Reasoning", "Monitors and regulates own thinking — detects overconfidence, scales down position size when model uncertainty is high", "meta",
         vec!["reasoning", "metacognitive", "self-awareness", "confidence"], Complexity::Expert),
        ("emergence", "Emergent Reasoning", "Complex patterns emerge from simple agent interactions — swarm intelligence for market analysis", "emergent",
         vec!["reasoning", "emergence", "swarm", "complex"], Complexity::Expert),
    ];

    for (id, name, desc, subcat, tags, complexity) in frameworks {
        registry.register(ToolEntry {
            id: format!("reasoning_{}", id),
            name: name.to_string(),
            category: Category::ReasoningFramework,
            description: desc.to_string(),
            subcategory: subcat.to_string(),
            complexity,
            tags: tags.into_iter().map(String::from).collect(),
            implementation: Implementation::BuiltIn,
        });
    }
}
