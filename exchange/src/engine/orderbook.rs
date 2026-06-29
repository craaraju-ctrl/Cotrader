use std::collections::{BTreeMap, HashMap, VecDeque};
use uuid::Uuid;

use crate::types::{Order, OrderBookLevel, Side};

/// Represents a single price level in the order book with FIFO queue
#[derive(Debug, Clone)]
pub struct PriceLevel {
    /// Price of this level
    pub price: f64,
    /// Orders at this price level, in FIFO order (time priority)
    pub orders: VecDeque<Order>,
}

impl PriceLevel {
    pub fn new(price: f64) -> Self {
        Self {
            price,
            orders: VecDeque::new(),
        }
    }

    /// Total quantity at this price level
    pub fn total_quantity(&self) -> f64 {
        self.orders.iter().map(|o| o.remaining_quantity()).sum()
    }

    /// Number of orders at this price level
    pub fn order_count(&self) -> u64 {
        self.orders.len() as u64
    }

    /// Add an order to the back of the queue (time priority)
    pub fn add_order(&mut self, order: Order) {
        self.orders.push_back(order);
    }

    /// Remove the front order (best time priority)
    pub fn pop_front(&mut self) -> Option<Order> {
        self.orders.pop_front()
    }

    /// Peek at the front order without removing
    pub fn front(&self) -> Option<&Order> {
        self.orders.front()
    }

    /// Check if this level is empty
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

/// Price-time priority order book for a single trading symbol
///
/// - Bids are sorted in DESCENDING price order (highest bid first)
/// - Asks are sorted in ASCENDING price order (lowest ask first)
/// - Within the same price level, orders are matched in FIFO order
#[derive(Debug)]
pub struct OrderBook {
    /// Trading symbol (e.g., "BTC/USD", "AAPL")
    pub symbol: String,
    /// Buy orders: BTreeMap<price, PriceLevel>
    /// Using iteration from the end for highest price
    bids: BTreeMap<u64, PriceLevel>,
    /// Sell orders: BTreeMap<price, PriceLevel>
    /// Using iteration from the start for lowest price
    asks: BTreeMap<u64, PriceLevel>,
    /// Quick lookup from order ID to order info (for cancellations and status)
    order_map: HashMap<Uuid, OrderInfo>,
    /// Counter for generating sequence numbers
    #[allow(dead_code)]
    sequence: u64,
}

/// Information about an order's location in the book
#[derive(Debug, Clone)]
struct OrderInfo {
    order: Order,
    #[allow(dead_code)]
    side: Side,
    #[allow(dead_code)]
    price_key: u64,
}

/// Price multiplier to convert f64 prices to integer keys
const PRICE_MULTIPLIER: f64 = 10_000.0;

impl OrderBook {
    pub fn new(symbol: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_map: HashMap::new(),
            sequence: 0,
        }
    }

    /// Convert price to integer key for the BTreeMap
    fn price_to_key(price: f64) -> u64 {
        (price * PRICE_MULTIPLIER).round() as u64
    }

    /// Add an order to the order book
    pub fn add_order(&mut self, order: Order) {
        let side = order.side;
        let price_key = match order.price {
            Some(p) => Self::price_to_key(p),
            None => return,
        };

        let book = match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        let entry = book.entry(price_key).or_insert_with(|| PriceLevel::new(price_key as f64 / PRICE_MULTIPLIER));
        entry.add_order(order.clone());

        self.order_map.insert(order.id, OrderInfo {
            order,
            side,
            price_key,
        });
    }

    /// Remove an order from the order book
    pub fn remove_order(&mut self, order_id: Uuid) -> Option<Order> {
        let info = self.order_map.remove(&order_id)?;

        let book = match info.side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        if let Some(level) = book.get_mut(&info.price_key) {
            let position = level.orders.iter().position(|o| o.id == order_id);
            if let Some(pos) = position {
                let order = level.orders.remove(pos).unwrap();
                if level.is_empty() {
                    book.remove(&info.price_key);
                }
                return Some(order);
            }
        }

        None
    }

    /// Get the best bid price (highest buy order)
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.last_key_value().map(|(k, _)| *k as f64 / PRICE_MULTIPLIER)
    }

    /// Get the best ask price (lowest sell order)
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first_key_value().map(|(k, _)| *k as f64 / PRICE_MULTIPLIER)
    }

    /// Get the spread (difference between best ask and best bid)
    pub fn spread(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(ask), Some(bid)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Peek at the best bid order (highest price, earliest time)
    pub fn peek_best_bid(&self) -> Option<&Order> {
        self.bids.last_key_value().and_then(|(_, level)| level.front())
    }

    /// Peek at the best ask order (lowest price, earliest time)
    pub fn peek_best_ask(&self) -> Option<&Order> {
        self.asks.first_key_value().and_then(|(_, level)| level.front())
    }

    /// Pop the best bid order
    pub fn pop_best_bid(&mut self) -> Option<Order> {
        let key = *self.bids.last_key_value()?.0;
        let order = {
            let level = self.bids.get_mut(&key)?;
            let order = level.pop_front()?;
            if level.is_empty() {
                self.bids.remove(&key);
            }
            order
        };
        self.order_map.remove(&order.id);
        Some(order)
    }

    /// Pop the best ask order
    pub fn pop_best_ask(&mut self) -> Option<Order> {
        let key = *self.asks.first_key_value()?.0;
        let order = {
            let level = self.asks.get_mut(&key)?;
            let order = level.pop_front()?;
            if level.is_empty() {
                self.asks.remove(&key);
            }
            order
        };
        self.order_map.remove(&order.id);
        Some(order)
    }

    /// Get all orders in the book
    pub fn all_orders(&self) -> Vec<(Uuid, &Order)> {
        self.order_map.iter().map(|(id, info)| (*id, &info.order)).collect()
    }

    /// Get an order by ID
    pub fn get_order(&self, order_id: Uuid) -> Option<&Order> {
        self.order_map.get(&order_id).map(|info| &info.order)
    }

    /// Get a mutable reference to an order by ID
    pub fn get_order_mut(&mut self, order_id: Uuid) -> Option<&mut Order> {
        self.order_map.get_mut(&order_id).map(|info| &mut info.order)
    }

    /// Get order book snapshot (top N levels)
    pub fn snapshot(&self, levels: usize) -> (Vec<OrderBookLevel>, Vec<OrderBookLevel>) {
        let bids: Vec<OrderBookLevel> = self.bids
            .iter()
            .rev()
            .take(levels)
            .map(|(_, level)| OrderBookLevel {
                price: level.price,
                quantity: level.total_quantity(),
                order_count: level.order_count(),
            })
            .collect();

        let asks: Vec<OrderBookLevel> = self.asks
            .iter()
            .take(levels)
            .map(|(_, level)| OrderBookLevel {
                price: level.price,
                quantity: level.total_quantity(),
                order_count: level.order_count(),
            })
            .collect();

        (bids, asks)
    }

    /// Calculate total available quantity at or better than a given price for FOK checks
    /// For buy side: sum of all ask quantities with price <= target_price
    /// For sell side: sum of all bid quantities with price >= target_price
    pub fn available_liquidity(&self, side: Side, target_price: f64) -> f64 {
        match side {
            Side::Buy => self.asks.iter()
                .take_while(|(_, l)| l.price <= target_price)
                .map(|(_, l)| l.total_quantity())
                .sum(),
            Side::Sell => self.bids.iter().rev()
                .take_while(|(_, l)| l.price >= target_price)
                .map(|(_, l)| l.total_quantity())
                .sum(),
        }
    }

    /// Total number of active orders in the book
    pub fn order_count(&self) -> usize {
        self.order_map.len()
    }

    /// Check if the order book has any orders
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_orderbook() {
        let ob = OrderBook::new("BTC/USD");
        assert_eq!(ob.symbol, "BTC/USD");
        assert!(ob.is_empty());
    }

    #[test]
    fn test_add_and_get_order() {
        let mut ob = OrderBook::new("BTC/USD");
        let order = Order::new_limit("user1".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        ob.add_order(order);

        assert_eq!(ob.order_count(), 1);
        assert!(ob.get_order(order_id).is_some());
    }

    #[test]
    fn test_remove_order() {
        let mut ob = OrderBook::new("BTC/USD");
        let order = Order::new_limit("user1".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        ob.add_order(order);

        let removed = ob.remove_order(order_id);
        assert!(removed.is_some());
        assert_eq!(ob.order_count(), 0);
        assert!(ob.is_empty());
    }

    #[test]
    fn test_price_time_priority() {
        let mut ob = OrderBook::new("BTC/USD");

        let order1 = Order::new_limit("user1".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);
        let order2 = Order::new_limit("user2".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);

        ob.add_order(order1);
        ob.add_order(order2);

        let best = ob.peek_best_bid();
        assert!(best.is_some());
        assert_eq!(best.unwrap().user_id, "user1");
    }

    #[test]
    fn test_best_bid_ask() {
        let mut ob = OrderBook::new("BTC/USD");

        ob.add_order(Order::new_limit("u1".into(), "BTC/USD".into(), Side::Buy, 49000.0, 1.0));
        ob.add_order(Order::new_limit("u2".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0));
        ob.add_order(Order::new_limit("u3".into(), "BTC/USD".into(), Side::Sell, 51000.0, 1.0));
        ob.add_order(Order::new_limit("u4".into(), "BTC/USD".into(), Side::Sell, 52000.0, 1.0));

        assert!(ob.best_bid().unwrap() - 50000.0 < 0.01);
        assert!(ob.best_ask().unwrap() - 51000.0 < 0.01);
        assert!(ob.spread().unwrap() - 1000.0 < 0.01);
    }
}
