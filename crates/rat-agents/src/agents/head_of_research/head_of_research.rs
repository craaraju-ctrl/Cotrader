pub struct HeadOfResearch;

impl HeadOfResearch {
    pub fn name() -> &'static str { "HeadOfResearch" }
    pub fn role() -> &'static str { "Head of Research" }

    pub fn synthesize(&self, reports: &[String]) -> String {
        let bullish = reports.iter().filter(|r| r.contains("bullish") || r.contains("BUY")).count();
        let bearish = reports.iter().filter(|r| r.contains("bearish") || r.contains("SELL")).count();
        let total = reports.len();
        format!(
            "Synthesis: {} reports analyzed | Bullish: {} | Bearish: {} | Neutral: {} | Consensus: {}",
            total, bullish, bearish, total - bullish - bearish,
            if bullish > bearish { "LEANING BULLISH" } else if bearish > bullish { "LEANING BEARISH" } else { "NEUTRAL" }
        )
    }

    pub fn prioritize_research(&self, opportunities: &[String]) -> String {
        let mut prioritized = opportunities.to_vec();
        prioritized.sort_by(|a, b| {
            let a_score = if a.contains("momentum") { 3 } else if a.contains("reversal") { 2 } else { 1 };
            let b_score = if b.contains("momentum") { 3 } else if b.contains("reversal") { 2 } else { 1 };
            b_score.cmp(&a_score)
        });
        format!("Research priorities: {}", prioritized.join(" > "))
    }

    pub fn quality_check(&self, report: &str) -> String {
        let mut issues = Vec::new();
        if !report.contains("data") && !report.contains("evidence") {
            issues.push("Missing data evidence");
        }
        if !report.contains("risk") {
            issues.push("No risk assessment");
        }
        if report.len() < 50 {
            issues.push("Insufficient detail");
        }
        if issues.is_empty() {
            "Quality: PASS — report meets all criteria".to_string()
        } else {
            format!("Quality: NEEDS IMPROVEMENT — {}", issues.join(", "))
        }
    }
}
