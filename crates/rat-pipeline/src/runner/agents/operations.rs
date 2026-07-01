//! Operations — Coordinates portfolio admin and journal keeping.

pub struct Operations {
    pub portfolio_admin: PortfolioAdminAgent,
    pub journal_keeper: JournalKeeperAgent,
}

pub struct PortfolioAdminAgent;
pub struct JournalKeeperAgent;

impl Operations {
    pub fn new() -> Self {
        Self {
            portfolio_admin: PortfolioAdminAgent,
            journal_keeper: JournalKeeperAgent,
        }
    }

    /// Log a completed trade.
    pub async fn log_trade(&self, symbol: &str, action: &str, pnl: f64) {
        self.journal_keeper.record(symbol, action, pnl).await;
        self.portfolio_admin.reconcile().await;
    }

    /// Generate end-of-day report.
    pub async fn eod_report(&self) -> String {
        self.portfolio_admin.report().await
    }
}

impl PortfolioAdminAgent {
    pub async fn reconcile(&self) {
        // TODO: Reconcile with broker
    }

    pub async fn report(&self) -> String {
        "EOD report generated".to_string()
    }
}

impl JournalKeeperAgent {
    pub async fn record(&self, symbol: &str, action: &str, pnl: f64) {
        let _ = (symbol, action, pnl);
        // TODO: Store in memory
    }
}
