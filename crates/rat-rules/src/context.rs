//! Rule evaluation context — shared data passed to all rules.

use rat_core::DisciplineRules;

/// Snapshot of market data for rule evaluation.
pub struct MarketSnapshot {
    pub bars: Vec<rat_core::OhlcvBar>,
    pub last_close: f64,
}

impl MarketSnapshot {
    pub fn from_bars(bars: Vec<rat_core::OhlcvBar>) -> Self {
        let last_close = bars.last().map(|b| b.close).unwrap_or(0.0);
        Self { bars, last_close }
    }

    /// Bar range as fraction (high - low) / close.
    pub fn bar_range(&self) -> f64 {
        if let Some(bar) = self.bars.last() {
            if bar.close > 0.0 {
                (bar.high - bar.low) / bar.close
            } else {
                0.0
            }
        } else {
            0.0
        }
    }
}

/// Portfolio state snapshot for rule evaluation.
pub struct PortfolioSnapshot {
    pub total_equity: f64,
    pub cash_balance: f64,
    pub max_drawdown_today: f64,
    pub total_trades_today: u32,
    pub winning_trades_today: u32,
    pub consecutive_losses: u32,
    pub trading_enabled: bool,
    pub position_count: usize,
    pub total_risk: f64,
    pub positions: Vec<PositionInfo>,
}

pub struct PositionInfo {
    pub symbol: String,
    pub risk_amount: f64,
}

/// Calendar event for news proximity checks.
pub struct CalendarEvent {
    pub date: String,
    pub time: Option<String>,
    pub impact: EventImpact,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventImpact {
    High,
    Medium,
    Low,
}

/// Context passed to every rule during evaluation.
pub struct RuleContext<'a> {
    pub symbol: &'a str,
    pub sigma: f64,
    pub market: &'a MarketSnapshot,
    pub portfolio: &'a PortfolioSnapshot,
    pub rules: &'a DisciplineRules,
    pub calendar: &'a [CalendarEvent],
}
