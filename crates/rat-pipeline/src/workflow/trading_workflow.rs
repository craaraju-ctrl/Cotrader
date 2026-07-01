//! Trading Workflow — End-to-end trading workflow.

use crate::runner::agents::{TradingDesk, ResearchDesk, RiskDesk, Operations, Technology};
use crate::runner::event_bus::{EventBus, Event, EventType};

pub struct TradingWorkflow {
    trading_desk: TradingDesk,
    research_desk: ResearchDesk,
    risk_desk: RiskDesk,
    operations: Operations,
    technology: Technology,
    event_bus: EventBus,
}

impl TradingWorkflow {
    pub fn new() -> Self {
        Self {
            trading_desk: TradingDesk::new(),
            research_desk: ResearchDesk::new(),
            risk_desk: RiskDesk::new(),
            operations: Operations::new(),
            technology: Technology::new(),
            event_bus: EventBus::new(1024),
        }
    }

    /// Run the full trading workflow for a symbol.
    pub async fn run(&mut self, symbol: &str) -> WorkflowResult {
        println!("[Workflow] Starting for {}", symbol);

        // Step 1: Research
        let research = self.research_desk.research(symbol, 0.0).await;
        self.event_bus.publish(Event::new(
            "research_desk",
            EventType::AgentDecision,
            serde_json::json!({"action": "research_complete", "score": research.combined_score}),
        ));
        println!("[Workflow] Research: score={:.2}", research.combined_score);

        // Step 2: Risk check
        let risk = self.risk_desk.check_risk(symbol, &research.action).await;
        self.event_bus.publish(Event::new(
            "risk_desk",
            EventType::RiskChecked,
            serde_json::json!({"passed": risk.passed, "reason": risk.reason}),
        ));
        println!("[Workflow] Risk: passed={}, reason={}", risk.passed, risk.reason);

        // Step 3: Trading decision
        if !risk.passed {
            return WorkflowResult {
                action: "BLOCKED".to_string(),
                reason: risk.reason,
                pnl: 0.0,
            };
        }

        let trade = self.trading_desk.route_trade(symbol, &research.action).await;
        self.event_bus.publish(Event::new(
            "trading_desk",
            EventType::TradeExecuted,
            serde_json::json!({"action": trade}),
        ));
        println!("[Workflow] Trade: {}", trade);

        // Step 4: Log outcome
        self.operations.log_trade(symbol, &trade, 0.0).await;

        // Step 5: System health check
        let health = self.technology.monitor().await;
        println!("[Workflow] Health: system_ok={}, data_quality={:.2}", health.system_ok, health.data_quality);

        WorkflowResult {
            action: trade,
            reason: "Workflow completed".to_string(),
            pnl: 0.0,
        }
    }
}

pub struct WorkflowResult {
    pub action: String,
    pub reason: String,
    pub pnl: f64,
}
