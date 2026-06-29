use chrono::{DateTime, Utc};
use dashmap::DashMap;

use crate::types::{
    err_invalid_order, ExchangeResult, MarginMode, PositionMode,
    PositionSide, FundingRateInfo, PositionInfo, Side,
};

/// Helper to build a position map key that includes the side.
/// OneWay mode: only one side gets a key per symbol.
/// Hedge mode: both "symbol:L" and "symbol:S" can coexist.
fn pos_key(symbol: &str, side: PositionSide) -> String {
    let tag = match side { PositionSide::Long => "L", PositionSide::Short => "S" };
    format!("{}:{}", symbol, tag)
}

/// Tracks position-level accounting for futures.
/// For OneWay mode: one entry per symbol.
/// For Hedge mode: separate entries for Long and Short per symbol.
#[derive(Debug, Clone)]
struct Position {
    side: PositionSide,
    size: f64,
    entry_price: f64,
    leverage: u32,
    margin_mode: MarginMode,
    /// Isolated margin allocated to this position
    isolated_margin: f64,
}

#[derive(Debug, Clone)]
struct FundingRateState {
    rate: f64,
    last_funding_time: DateTime<Utc>,
    next_funding_time: DateTime<Utc>,
}

/// FuturesEngine manages leverage, margin modes, position modes, liquidation,
/// and funding rates for futures-style trading.
pub struct FuturesEngine {
    /// Per-user, per-symbol leverage (1-125)
    leverage: DashMap<String, DashMap<String, u32>>,
    /// Per-user, per-symbol margin mode
    margin_modes: DashMap<String, DashMap<String, MarginMode>>,
    /// Per-user position mode (OneWay or Hedge)
    position_modes: DashMap<String, PositionMode>,
    /// Open futures positions: user -> position_key -> Position
    /// position_key = "symbol:L" or "symbol:S" (see pos_key())
    positions: DashMap<String, DashMap<String, Position>>,
    /// Funding rate state per symbol
    funding: DashMap<String, FundingRateState>,
    /// Last mark prices (updated from trades)
    mark_prices: DashMap<String, f64>,
}

impl FuturesEngine {
    pub fn new() -> Self {
        Self {
            leverage: DashMap::new(),
            margin_modes: DashMap::new(),
            position_modes: DashMap::new(),
            positions: DashMap::new(),
            funding: DashMap::new(),
            mark_prices: DashMap::new(),
        }
    }

    // ── Leverage ────────────────────────────────────────────

    pub fn set_leverage(&self, uid: &str, symbol: &str, lev: u32) -> ExchangeResult<u32> {
        let lev = lev.clamp(1, 125);
        let sym_map = self.leverage.entry(uid.to_string()).or_insert_with(DashMap::new);
        sym_map.insert(symbol.to_string(), lev);
        Ok(lev)
    }

    pub fn get_leverage(&self, uid: &str, symbol: &str) -> u32 {
        self.leverage
            .get(uid)
            .and_then(|m| m.get(symbol).map(|r| *r))
            .unwrap_or(1)
    }

    // ── Margin Mode ─────────────────────────────────────────

    pub fn set_margin_mode(&self, uid: &str, symbol: &str, mode: MarginMode) {
        let sym_map = self.margin_modes.entry(uid.to_string()).or_insert_with(DashMap::new);
        sym_map.insert(symbol.to_string(), mode);
    }

    pub fn get_margin_mode(&self, uid: &str, symbol: &str) -> MarginMode {
        self.margin_modes
            .get(uid)
            .and_then(|m| m.get(symbol).map(|r| *r))
            .unwrap_or(MarginMode::Cross)
    }

    // ── Position Mode ───────────────────────────────────────

    pub fn set_position_mode(&self, uid: &str, mode: PositionMode) {
        self.position_modes.insert(uid.to_string(), mode);
    }

    pub fn get_position_mode(&self, uid: &str) -> PositionMode {
        self.position_modes.get(uid).map(|r| *r).unwrap_or(PositionMode::OneWay)
    }

    // ── Positions ───────────────────────────────────────────

    pub fn open_position(
        &self,
        uid: &str,
        symbol: &str,
        side: Side,
        fill_price: f64,
        fill_qty: f64,
    ) {
        let pos_side = match side {
            Side::Buy => PositionSide::Long,
            Side::Sell => PositionSide::Short,
        };
        let lev = self.get_leverage(uid, symbol);
        let mm = self.get_margin_mode(uid, symbol);
        let key = pos_key(symbol, pos_side);

        let sym_map = self.positions.entry(uid.to_string()).or_insert_with(DashMap::new);
        let mut entry = sym_map.entry(key.clone()).or_insert_with(|| Position {
            side: pos_side,
            size: 0.0,
            entry_price: 0.0,
            leverage: lev,
            margin_mode: mm,
            isolated_margin: 0.0,
        });

        let pos_mode = self.get_position_mode(uid);
        // In OneWay mode, if there's an existing position on the opposite side, reduce it first
        if pos_mode == PositionMode::OneWay && entry.size > 0.0 && entry.side != pos_side {
            let reduce_qty = fill_qty.min(entry.size);
            entry.size -= reduce_qty;
            if entry.size <= 0.0 {
                entry.size = 0.0;
                entry.entry_price = 0.0;
            }
            let remaining = fill_qty - reduce_qty;
            if remaining > 0.0 {
                entry.side = pos_side;
                entry.size = remaining;
                entry.entry_price = fill_price;
                entry.leverage = lev;
                entry.margin_mode = mm;
            }
            return;
        }

        // In Hedge mode (or OneWay with same side): increase position
        // For OneWay, the key always matches the current side since there's only one entry
        // For Hedge, each side has its own key, so this just adds to the correct side
        let total_cost = entry.entry_price * entry.size + fill_price * fill_qty;
        let total_qty = entry.size + fill_qty;
        entry.entry_price = if total_qty > 0.0 { total_cost / total_qty } else { fill_price };
        entry.size = total_qty;
        entry.side = pos_side;
        entry.leverage = lev;
        entry.margin_mode = mm;
    }

    pub fn reduce_position(
        &self,
        uid: &str,
        symbol: &str,
        side: Side,
        _fill_price: f64,
        fill_qty: f64,
    ) {
        // The trade side tells us which position to reduce:
        //   Sell trade → reduces Long position (opposite direction)
        //   Buy trade  → reduces Short position (opposite direction)
        let pos_side = match side {
            Side::Buy => PositionSide::Long,
            Side::Sell => PositionSide::Short,
        };
        // In OneWay mode: position side may differ from trade side (which is the opposite direction for closing)
        // In Hedge mode: each side tracked independently, so find the entry for the trade's side
        let opp_key = pos_key(symbol, match side {
            Side::Sell => PositionSide::Long,  // Selling reduces Long
            Side::Buy => PositionSide::Short,   // Buying reduces Short
        });

        if let Some(sym_map) = self.positions.get_mut(uid) {
            if let Some(mut entry) = sym_map.get_mut(&opp_key) {
                if entry.side == pos_side.opposite() {
                    entry.size = (entry.size - fill_qty).max(0.0);
                    if entry.size <= 0.0 {
                        entry.entry_price = 0.0;
                    }
                }
            }
        }
    }

    /// Get all positions for a user on a symbol (both Long and Short in Hedge mode).
    pub fn get_position(&self, uid: &str, symbol: &str) -> Option<PositionInfo> {
        let mark_price = self.mark_prices.get(symbol).map(|r| *r).unwrap_or(0.0);
        let sym_map = self.positions.get(uid)?;

        // Find the first entry with non-zero size for this symbol
        for entry in sym_map.iter() {
            if entry.size > 0.0 && entry.key().starts_with(&format!("{}:", symbol)) {
                let pnl = match entry.side {
                    PositionSide::Long => (mark_price - entry.entry_price) * entry.size,
                    PositionSide::Short => (entry.entry_price - mark_price) * entry.size,
                };
                let margin = if entry.isolated_margin > 0.0 {
                    entry.isolated_margin
                } else {
                    (entry.entry_price * entry.size) / entry.leverage as f64
                };
                let pnl_percent = if margin > 0.0 { (pnl / margin) * 100.0 } else { 0.0 };
                let liq_price = self.calc_liquidation_price(
                    entry.entry_price, entry.size, entry.leverage, entry.side, 0.05, margin,
                );
                return Some(PositionInfo {
                    symbol: symbol.to_string(),
                    side: entry.side,
                    size: entry.size,
                    entry_price: entry.entry_price,
                    mark_price,
                    unrealized_pnl: pnl,
                    pnl_percent,
                    liquidation_price: liq_price,
                    leverage: entry.leverage,
                    margin,
                    margin_mode: entry.margin_mode,
                });
            }
        }
        None
    }

    pub fn get_all_positions(&self, uid: &str) -> Vec<PositionInfo> {
        match self.positions.get(uid) {
            Some(m) => m.iter().filter(|e| e.size > 0.0)
                .map(|e| {
                    // Extract symbol from the composite key (strip ":L" or ":S" suffix)
                    let symbol = e.key().rsplit_once(':')
                        .map(|(sym, _)| sym.to_string())
                        .unwrap_or_else(|| e.key().clone());
                    let mark_price = self.mark_prices.get(&symbol).map(|r| *r).unwrap_or(0.0);
                    let pnl = match e.side {
                        PositionSide::Long => (mark_price - e.entry_price) * e.size,
                        PositionSide::Short => (e.entry_price - mark_price) * e.size,
                    };
                    let margin = if e.isolated_margin > 0.0 {
                        e.isolated_margin
                    } else {
                        (e.entry_price * e.size) / e.leverage as f64
                    };
                    let pnl_percent = if margin > 0.0 { (pnl / margin) * 100.0 } else { 0.0 };
                    let liq_price = self.calc_liquidation_price(
                        e.entry_price, e.size, e.leverage, e.side, 0.05, margin,
                    );
                    PositionInfo {
                        symbol: symbol,
                        side: e.side,
                        size: e.size,
                        entry_price: e.entry_price,
                        mark_price,
                        unrealized_pnl: pnl,
                        pnl_percent,
                        liquidation_price: liq_price,
                        leverage: e.leverage,
                        margin,
                        margin_mode: e.margin_mode,
                    }
                })
                .collect(),
            None => vec![],
        }
    }

    // ── Mark Price ──────────────────────────────────────────

    pub fn update_mark_price(&self, symbol: &str, price: f64) {
        self.mark_prices.insert(symbol.to_string(), price);
    }

    pub fn get_mark_price(&self, symbol: &str) -> Option<f64> {
        self.mark_prices.get(symbol).map(|r| *r)
    }

    // ── Liquidation Price ───────────────────────────────────

    /// Calculate liquidation price for a position.
    pub fn calc_liquidation_price(
        &self,
        entry_price: f64,
        size: f64,
        leverage: u32,
        side: PositionSide,
        maintenance_pct: f64,
        _margin: f64,
    ) -> f64 {
        if size <= 0.0 || entry_price <= 0.0 { return 0.0; }
        let mm_pct = maintenance_pct;
        match side {
            PositionSide::Long => {
                entry_price * (1.0 - (1.0 - mm_pct) / leverage as f64)
            }
            PositionSide::Short => {
                entry_price * (1.0 + (1.0 - mm_pct) / leverage as f64)
            }
        }
    }

    /// Check if a position should be liquidated based on current mark price.
    /// Checks both Long and Short positions for the symbol.
    pub fn check_liquidation(&self, uid: &str, symbol: &str) -> Option<PositionInfo> {
        let mark_price = self.mark_prices.get(symbol).map(|r| *r).unwrap_or(0.0);
        let sym_map = self.positions.get(uid)?;
        let prefix = format!("{}:", symbol);

        for entry in sym_map.iter() {
            if !entry.key().starts_with(&prefix) || entry.size <= 0.0 {
                continue;
            }
            // Calculate liquidation price for this entry
            let liq_price = self.calc_liquidation_price(
                entry.entry_price, entry.size, entry.leverage, entry.side, 0.05,
                if entry.isolated_margin > 0.0 { entry.isolated_margin }
                else { (entry.entry_price * entry.size) / entry.leverage as f64 },
            );
            let needs_liq = match entry.side {
                PositionSide::Long => mark_price <= liq_price,
                PositionSide::Short => mark_price >= liq_price,
            };
            if needs_liq {
                let pnl = match entry.side {
                    PositionSide::Long => (mark_price - entry.entry_price) * entry.size,
                    PositionSide::Short => (entry.entry_price - mark_price) * entry.size,
                };
                let margin = if entry.isolated_margin > 0.0 {
                    entry.isolated_margin
                } else {
                    (entry.entry_price * entry.size) / entry.leverage as f64
                };
                return Some(PositionInfo {
                    symbol: symbol.to_string(),
                    side: entry.side,
                    size: entry.size,
                    entry_price: entry.entry_price,
                    mark_price,
                    unrealized_pnl: pnl,
                    pnl_percent: if margin > 0.0 { (pnl / margin) * 100.0 } else { 0.0 },
                    liquidation_price: liq_price,
                    leverage: entry.leverage,
                    margin,
                    margin_mode: entry.margin_mode,
                });
            }
        }
        None
    }

    /// Liquidate ALL positions for a user on a symbol (both Long and Short).
    pub fn liquidate_position(&self, uid: &str, symbol: &str) -> ExchangeResult<Vec<PositionInfo>> {
        let mut liquidated = Vec::new();
        let prefix = format!("{}:", symbol);

        if let Some(sym_map) = self.positions.get_mut(uid) {
            // Collect keys to remove while iterating
            let keys_to_clear: Vec<String> = sym_map.iter()
                .filter(|e| e.key().starts_with(&prefix) && e.size > 0.0)
                .map(|e| e.key().clone())
                .collect();

            for key in &keys_to_clear {
                if let Some(mut entry) = sym_map.get_mut(key) {
                    let pos_info = self.build_position_info(symbol, &entry);
                    entry.size = 0.0;
                    entry.entry_price = 0.0;
                    entry.isolated_margin = 0.0;
                    if let Some(info) = pos_info {
                        liquidated.push(info);
                    }
                }
            }
        }

        if liquidated.is_empty() {
            return Err(err_invalid_order("No position to liquidate"));
        }
        Ok(liquidated)
    }

    fn build_position_info(&self, symbol: &str, p: &Position) -> Option<PositionInfo> {
        if p.size <= 0.0 { return None; }
        let mark_price = self.mark_prices.get(symbol).map(|r| *r).unwrap_or(0.0);
        let pnl = match p.side {
            PositionSide::Long => (mark_price - p.entry_price) * p.size,
            PositionSide::Short => (p.entry_price - mark_price) * p.size,
        };
        let margin = if p.isolated_margin > 0.0 {
            p.isolated_margin
        } else {
            (p.entry_price * p.size) / p.leverage as f64
        };
        let pnl_percent = if margin > 0.0 { (pnl / margin) * 100.0 } else { 0.0 };
        let liq_price = self.calc_liquidation_price(
            p.entry_price, p.size, p.leverage, p.side, 0.05, margin,
        );
        Some(PositionInfo {
            symbol: symbol.to_string(),
            side: p.side,
            size: p.size,
            entry_price: p.entry_price,
            mark_price,
            unrealized_pnl: pnl,
            pnl_percent,
            liquidation_price: liq_price,
            leverage: p.leverage,
            margin,
            margin_mode: p.margin_mode,
        })
    }

    // ── Funding Rate ────────────────────────────────────────

    /// Initialize funding rate for a symbol.
    pub fn init_funding_rate(&self, symbol: &str) {
        let now = Utc::now();
        self.funding.insert(symbol.to_string(), FundingRateState {
            rate: 0.0001,
            last_funding_time: now,
            next_funding_time: now + chrono::Duration::hours(8),
        });
    }

    /// Update funding rate based on long/short imbalance.
    pub fn update_funding_rate(&self, symbol: &str, long_interest: f64, short_interest: f64) {
        if let Some(mut f) = self.funding.get_mut(symbol) {
            let diff = long_interest - short_interest;
            let total = long_interest + short_interest;
            let base_rate = 0.0001;
            let premium = if total > 0.0 { (diff / total).abs().min(0.005) * 0.1 } else { 0.0 };
            f.rate = if diff > 0.0 { base_rate + premium } else { -(base_rate + premium) };
        }
    }

    /// Get current funding rate info for a symbol.
    pub fn get_funding_rate(&self, symbol: &str) -> FundingRateInfo {
        let now = Utc::now();
        match self.funding.get(symbol) {
            Some(f) => FundingRateInfo {
                symbol: symbol.to_string(),
                funding_rate: f.rate,
                next_funding_time: f.next_funding_time,
                last_funding_time: f.last_funding_time,
            },
            None => FundingRateInfo {
                symbol: symbol.to_string(),
                funding_rate: 0.0,
                next_funding_time: now + chrono::Duration::hours(8),
                last_funding_time: now,
            },
        }
    }

    // ── Background Task Helpers ─────────────────────────────

    /// Return all users who hold a non-zero position in the given symbol.
    /// Returns (user_id, PositionSnapshot) where PositionSnapshot has side, size, entry_price, leverage, margin.
    pub fn get_position_holders_for_symbol(&self, symbol: &str) -> Vec<(String, PositionSide, f64, f64, u32, f64)> {
        let prefix = format!("{}:", symbol);
        let mut holders = Vec::new();
        for user_entry in self.positions.iter() {
            let uid = user_entry.key().clone();
            for entry in user_entry.iter() {
                if !entry.key().starts_with(&prefix) || entry.size <= 0.0 {
                    continue;
                }
                let margin = if entry.isolated_margin > 0.0 {
                    entry.isolated_margin
                } else {
                    (entry.entry_price * entry.size) / entry.leverage as f64
                };
                holders.push((uid.clone(), entry.side, entry.size, entry.entry_price, entry.leverage, margin));
            }
        }
        holders
    }

    /// Calculate total long and short notional interest for a symbol.
    /// Used to adjust the funding rate based on skew.
    pub fn get_long_short_interest(&self, symbol: &str) -> (f64, f64) {
        let prefix = format!("{}:", symbol);
        let mut long = 0.0;
        let mut short = 0.0;
        let mark = self.mark_prices.get(symbol).map(|r| *r).unwrap_or(0.0);
        for user_entry in self.positions.iter() {
            for entry in user_entry.iter() {
                if !entry.key().starts_with(&prefix) || entry.size <= 0.0 {
                    continue;
                }
                let notional = entry.size * mark;
                match entry.side {
                    PositionSide::Long => long += notional,
                    PositionSide::Short => short += notional,
                }
            }
        }
        (long, short)
    }

    /// Advance the funding rate timestamp (after settlement is complete).
    pub fn advance_funding_timestamp(&self, symbol: &str) {
        let now = Utc::now();
        if let Some(mut f) = self.funding.get_mut(symbol) {
            f.last_funding_time = f.next_funding_time;
            f.next_funding_time = now + chrono::Duration::hours(8);
        }
    }

    /// Calculate required margin for an order with leverage.
    pub fn required_margin(&self, uid: &str, symbol: &str, _side: Side, price: f64, qty: f64) -> f64 {
        let lev = self.get_leverage(uid, symbol) as f64;
        let notional = price * qty;
        notional / lev
    }

    /// Check if user has sufficient margin for a leveraged order.
    /// `available_balance` is the user's available quote asset balance from RiskEngine.
    pub fn has_sufficient_margin(&self, uid: &str, symbol: &str, price: f64, qty: f64, available_balance: f64) -> bool {
        let required = self.required_margin(uid, symbol, Side::Buy, price, qty);
        available_balance >= required
    }

    /// Collect funding payments between long and short positions.
    /// Returns (long_pays_total, short_receives_total) per symbol.
    pub fn settle_funding(&self, symbol: &str, mark_price: f64) -> (f64, f64) {
        let rate = self.funding.get(symbol).map(|f| f.rate).unwrap_or(0.0);
        let now = Utc::now();
        let prefix = format!("{}:", symbol);

        // Update funding time
        if let Some(mut f) = self.funding.get_mut(symbol) {
            f.last_funding_time = f.next_funding_time;
            f.next_funding_time = now + chrono::Duration::hours(8);
        }

        let mut long_total = 0.0;
        let mut short_total = 0.0;

        // Iterate all users with positions in this symbol
        for user_entry in self.positions.iter() {
            for entry in user_entry.iter() {
                if !entry.key().starts_with(&prefix) { continue; }
                let payment = (rate * entry.size * mark_price).abs();
                match entry.side {
                    PositionSide::Long => long_total += payment,
                    PositionSide::Short => short_total += payment,
                }
            }
        }

        (long_total, short_total)
    }
}

/// Helper to get the opposite PositionSide
impl PositionSide {
    pub fn opposite(&self) -> Self {
        match self {
            PositionSide::Long => PositionSide::Short,
            PositionSide::Short => PositionSide::Long,
        }
    }
}
