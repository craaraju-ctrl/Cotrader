use dashmap::DashMap;
use std::time::{Duration, Instant};
use crate::types::{err_insufficient_balance, err_invalid_order, err_position_limit, err_rate_limit, ExchangeResult, Order, OrderType, Side};

struct UserRateLimit { ts: Vec<Instant> }
impl UserRateLimit {
    fn new() -> Self { Self { ts: Vec::new() } }
    fn check_and_record(&mut self, max: usize, window: Duration) -> bool {
        let cutoff = Instant::now() - window;
        self.ts.retain(|&t| t > cutoff);
        if self.ts.len() >= max { return false; }
        self.ts.push(Instant::now()); true
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pos { pub net: f64, pub open_buy: f64, pub open_sell: f64 }
#[derive(Debug, Clone)]
pub struct Bal { pub avail: f64, pub locked: f64 }
impl Bal {
    pub fn new(a: f64) -> Self { Self { avail: a, locked: 0.0 } }
    pub fn total(&self) -> f64 { self.avail + self.locked }
}

pub struct RiskEngine {
    bals: DashMap<String, DashMap<String, Bal>>,
    positions: DashMap<String, DashMap<String, Pos>>,
    rate: DashMap<String, UserRateLimit>,
    max_ord: usize, win_sec: u64, max_pos: f64,
    maker_fee: f64, taker_fee: f64,
}

impl RiskEngine {
    pub fn new() -> Self {
        Self {
            bals: DashMap::new(), positions: DashMap::new(), rate: DashMap::new(),
            max_ord: 100, win_sec: 60, max_pos: 1000.0,
            maker_fee: 0.001, taker_fee: 0.002, // 0.1% maker, 0.2% taker
        }
    }

    pub fn new_with_fees(maker: f64, taker: f64) -> Self {
        Self {
            bals: DashMap::new(), positions: DashMap::new(), rate: DashMap::new(),
            max_ord: 100, win_sec: 60, max_pos: 1000.0,
            maker_fee: maker, taker_fee: taker,
        }
    }

    pub fn get_fees(&self) -> (f64, f64) { (self.maker_fee, self.taker_fee) }

    pub fn deduct(&self, uid: &str, asset: &str, amt: f64) {
        if let Some(ub) = self.bals.get_mut(uid) {
            if let Some(mut e) = ub.get_mut(asset) {
                let d = amt.min(e.avail);
                e.avail -= d;
            }
        }
    }

    pub fn deposit(&self, uid: &str, asset: &str, amt: f64) {
        let ub = self.bals.entry(uid.to_string()).or_insert_with(DashMap::new);
        let mut e = ub.entry(asset.to_string()).or_insert_with(|| Bal::new(0.0));
        e.avail += amt;
    }

    pub fn get_balance(&self, uid: &str, asset: &str) -> Bal {
        match self.bals.get(uid) {
            Some(ref ub) => match ub.get(asset) {
                Some(e) => Bal { avail: e.avail, locked: e.locked },
                None => Bal::new(0.0),
            },
            None => Bal::new(0.0),
        }
    }

    pub fn get_all_balances(&self, uid: &str) -> Vec<(String, Bal)> {
        match self.bals.get(uid) {
            Some(ub) => ub.iter().map(|e| (e.key().clone(), Bal { avail: e.avail, locked: e.locked })).collect(),
            None => vec![],
        }
    }

    fn lock_funds(&self, uid: &str, asset: &str, amt: f64) -> ExchangeResult<()> {
        let ub = self.bals.entry(uid.to_string()).or_insert_with(DashMap::new);
        let mut e = ub.entry(asset.to_string()).or_insert_with(|| Bal::new(0.0));
        if e.avail < amt { return Err(err_insufficient_balance(format!("Need {:.8}, have {:.8}", amt, e.avail))); }
        e.avail -= amt; e.locked += amt; Ok(())
    }

    fn unlock(&self, uid: &str, asset: &str, amt: f64) {
        if let Some(ub) = self.bals.get_mut(uid) {
            if let Some(mut e) = ub.get_mut(asset) {
                let u = amt.min(e.locked); e.locked -= u; e.avail += u;
            }
        }
    }

    fn add_pos(&self, uid: &str, sym: &str, side: Side, qty: f64) {
        let up = self.positions.entry(uid.to_string()).or_insert_with(DashMap::new);
        let mut p = up.entry(sym.to_string()).or_insert_with(Pos::default);
        match side { Side::Buy => p.open_buy += qty, Side::Sell => p.open_sell += qty }
    }

    fn sub_pos(&self, uid: &str, sym: &str, side: Side, qty: f64) {
        if let Some(up) = self.positions.get_mut(uid) {
            if let Some(mut p) = up.get_mut(sym) {
                match side { Side::Buy => p.open_buy = (p.open_buy - qty).max(0.0), Side::Sell => p.open_sell = (p.open_sell - qty).max(0.0) }
            }
        }
    }

    fn net_pos(&self, uid: &str, sym: &str, side: Side, qty: f64) {
        let up = self.positions.entry(uid.to_string()).or_insert_with(DashMap::new);
        let mut p = up.entry(sym.to_string()).or_insert_with(Pos::default);
        match side { Side::Buy => p.net += qty, Side::Sell => p.net -= qty }
    }

    pub fn get_pos(&self, uid: &str, sym: &str) -> Pos {
        match self.positions.get(uid) {
            Some(up) => match up.get(sym) {
                Some(p) => Pos { net: p.net, open_buy: p.open_buy, open_sell: p.open_sell },
                None => Pos::default(),
            },
            None => Pos::default(),
        }
    }

    /// Check if an order can be placed. If `leverage > 1`, the required amount is
    /// scaled down to `notional / leverage` (margin-based checking instead of full notional).
    pub async fn check_order(&self, order: &Order, best_ask: Option<f64>, leverage: u32) -> ExchangeResult<()> {
        let mut lim = self.rate.entry(order.user_id.clone()).or_insert_with(UserRateLimit::new);
        if !lim.check_and_record(self.max_ord, Duration::from_secs(self.win_sec)) {
            return Err(err_rate_limit(format!("Rate limit: {} per {}s", self.max_ord, self.win_sec)));
        }
        let (base, quote) = parse_sym(&order.symbol)?;
        match order.side {
            Side::Buy => {
                let r = match order.order_type {
                    OrderType::Limit => order.price.unwrap_or(0.0) * order.quantity,
                    OrderType::Market => best_ask.map_or(order.quantity * 100_000.0, |p| p * order.quantity),
                    OrderType::StopLoss => order.trigger_price.unwrap_or(order.quantity * 100_000.0) * order.quantity,
                    OrderType::StopLimit => order.price.unwrap_or(order.price.unwrap_or(0.0)) * order.quantity,
                    OrderType::TakeProfit => order.trigger_price.unwrap_or(order.quantity * 100_000.0) * order.quantity,
                    OrderType::TakeProfitLimit => order.price.unwrap_or(order.trigger_price.unwrap_or(0.0)) * order.quantity,
                    OrderType::TrailingStop => order.stop_price.or(order.trigger_price).unwrap_or(order.quantity * 100_000.0) * order.quantity,
                };
                // Apply leverage: margin = full_notional / leverage
                let r = if leverage > 1 { r / leverage as f64 } else { r };
                if self.get_balance(&order.user_id, quote).avail < r { return Err(err_insufficient_balance(format!("Need {:.8} {}", r, quote))); }
                let pos = self.get_pos(&order.user_id, &order.symbol);
                if pos.net + pos.open_buy + order.quantity > self.max_pos { return Err(err_position_limit(format!("Max {:.2}", self.max_pos))); }
            }
            Side::Sell => {
                let required = if leverage > 1 { order.quantity / leverage as f64 } else { order.quantity };
                if self.get_balance(&order.user_id, base).avail < required { return Err(err_insufficient_balance(format!("Need {:.8} {}", required, base))); }
            }
        }
        // Trigger orders don't need balance check here - they're checked when triggered
        match order.order_type {
            OrderType::StopLoss | OrderType::StopLimit | OrderType::TakeProfit | OrderType::TakeProfitLimit | OrderType::TrailingStop => {}
            _ => {}
        }
        Ok(())
    }

    /// Lock funds for an order. If `leverage > 1`, only locks the margin amount (`notional / leverage`).
    pub async fn lock_for_order(&self, order: &Order, best_ask: Option<f64>, leverage: u32) -> ExchangeResult<()> {
        let (base, quote) = parse_sym(&order.symbol)?;
        match order.side {
            Side::Buy => {
                let amt = match order.order_type {
                    OrderType::Limit => order.price.unwrap_or(0.0) * order.quantity,
                    OrderType::Market => best_ask.map_or(order.quantity * 100_000.0, |p| p * order.quantity),
                    OrderType::StopLoss => order.trigger_price.unwrap_or(order.quantity * 100_000.0) * order.quantity,
                    OrderType::StopLimit => order.price.unwrap_or(0.0) * order.quantity,
                    // Trigger orders lock based on trigger price as estimate
                    OrderType::TakeProfit => order.trigger_price.unwrap_or(order.quantity * 100_000.0) * order.quantity,
                    OrderType::TakeProfitLimit => order.price.unwrap_or(0.0) * order.quantity,
                    OrderType::TrailingStop => order.stop_price.or(order.trigger_price).unwrap_or(order.quantity * 100_000.0) * order.quantity,
                };
                // Apply leverage: only lock the margin amount
                let amt = if leverage > 1 { (amt / leverage as f64).max(1.0) } else { amt };
                self.lock_funds(&order.user_id, quote, amt)?;
            }
            Side::Sell => {
                let amt = if leverage > 1 { (order.quantity / leverage as f64).max(0.0001) } else { order.quantity };
                self.lock_funds(&order.user_id, base, amt)?;
            }
        }
        self.add_pos(&order.user_id, &order.symbol, order.side, order.quantity);
        Ok(())
    }

    pub async fn release_for_cancellation(&self, order: &Order) {
        let (base, quote) = parse_sym(&order.symbol).unwrap_or(("", ""));
        let rem = order.quantity - order.filled_quantity;
        match order.side {
            Side::Buy => {
                // Use trigger_price as fallback for stop orders where price is None
                let price = order.price.or(order.trigger_price).unwrap_or(0.0);
                self.unlock(&order.user_id, quote, price * rem);
            }
            Side::Sell => { self.unlock(&order.user_id, base, rem); }
        }
        self.sub_pos(&order.user_id, &order.symbol, order.side, rem);
    }

    pub async fn settle_trade(&self, buyer: &str, seller: &str, sym: &str, price: f64, qty: f64, taker_side: Side) {
        let (base, quote) = parse_sym(sym).unwrap_or(("", ""));
        let total = price * qty;

        // Calculate fees
        // Taker pays taker_fee on the received asset, maker pays maker_fee
        let (buyer_fee, seller_fee) = if taker_side == Side::Buy {
            (self.taker_fee, self.maker_fee) // buyer is taker, seller is maker
        } else {
            (self.maker_fee, self.taker_fee) // buyer is maker, seller is taker
        };

        let buyer_base_fee = qty * buyer_fee;
        let seller_quote_fee = total * seller_fee;

        self.unlock(buyer, quote, total);
        self.deposit(buyer, base, qty - buyer_base_fee); // deduct buyer fee in base
        self.unlock(seller, base, qty);
        self.deposit(seller, quote, total - seller_quote_fee); // deduct seller fee in quote

        self.net_pos(buyer, sym, Side::Buy, qty - buyer_base_fee);
        self.net_pos(seller, sym, Side::Sell, qty);
        self.sub_pos(buyer, sym, Side::Buy, qty);
        self.sub_pos(seller, sym, Side::Sell, qty);
    }

    /// Release remaining locked funds for a specific user/asset.
    /// Used to release excess locked funds after market order execution.
    pub fn unlock_locked(&self, uid: &str, asset: &str, amount: f64) {
        self.unlock(uid, asset, amount);
    }

    pub fn recover_open_order(&self, order: &Order) {
        self.add_pos(&order.user_id, &order.symbol, order.side, order.quantity - order.filled_quantity);
        self.rate.entry(order.user_id.clone()).or_insert_with(UserRateLimit::new).ts.push(Instant::now());
    }
}

fn parse_sym(s: &str) -> ExchangeResult<(&str, &str)> {
    if let Some(i) = s.find('/') { Ok((&s[..i], &s[i+1..])) }
    else if let Some(i) = s.find('-') { Ok((&s[..i], &s[i+1..])) }
    else { Err(err_invalid_order(format!("Bad symbol: {}", s))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_basic() {
        let r = RiskEngine::new(); r.deposit("a", "USD", 100.0);
        assert_eq!(r.get_balance("a", "USD").avail, 100.0);
    }
    #[tokio::test]
    async fn test_insuff() {
        let r = RiskEngine::new(); r.deposit("a", "USD", 100.0);
        assert!(r.check_order(&Order::new_limit("a".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0), None, 1).await.is_err());
    }
    #[tokio::test]
    async fn test_suff() {
        let r = RiskEngine::new(); r.deposit("a", "USD", 100000.0);
        assert!(r.check_order(&Order::new_limit("a".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0), None, 1).await.is_ok());
    }
    #[tokio::test]
    async fn test_lock() {
        let r = RiskEngine::new(); r.deposit("a", "USD", 100000.0);
        let o = Order::new_limit("a".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);
        r.check_order(&o, None, 1).await.unwrap(); r.lock_for_order(&o, None, 1).await.unwrap();
        assert_eq!(r.get_balance("a", "USD").avail, 50000.0);
        assert_eq!(r.get_balance("a", "USD").locked, 50000.0);
        r.release_for_cancellation(&o).await;
        assert_eq!(r.get_balance("a", "USD").avail, 100000.0);
    }
}
