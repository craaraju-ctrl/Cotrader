//! Research Workflow — Multi-agent research workflow.

use crate::runner::agents::ResearchDesk;

pub struct ResearchWorkflow {
    research_desk: ResearchDesk,
}

impl ResearchWorkflow {
    pub fn new() -> Self {
        Self {
            research_desk: ResearchDesk::new(),
        }
    }

    /// Run comprehensive research on a symbol.
    pub async fn research(&self, symbol: &str) -> ResearchWorkflowResult {
        println!("[Research] Starting research for {}", symbol);

        // Run all research agents
        let output = self.research_desk.research(symbol, 0.0).await;

        // Determine conviction
        let conviction = if output.combined_score > 0.7 {
            "HIGH".to_string()
        } else if output.combined_score > 0.5 {
            "MEDIUM".to_string()
        } else {
            "LOW".to_string()
        };

        ResearchWorkflowResult {
            symbol: output.symbol,
            action: output.action,
            conviction,
            quant_score: output.quant_score,
            technical_score: output.technical_score,
            fundamental_score: output.fundamental_score,
            combined_score: output.combined_score,
        }
    }
}

pub struct ResearchWorkflowResult {
    pub symbol: String,
    pub action: String,
    pub conviction: String,
    pub quant_score: f64,
    pub technical_score: f64,
    pub fundamental_score: f64,
    pub combined_score: f64,
}
