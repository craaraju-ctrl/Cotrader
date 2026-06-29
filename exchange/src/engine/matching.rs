use std::collections::HashMap;
use uuid::Uuid;

use crate::types::{
    err_invalid_order, err_invalid_price, err_invalid_quantity, err_order_not_found,
    ExchangeResult, Order, OrderStatus, OrderType, PlaceOrderResponse, Side, Trade,
};

use super::orderbook::OrderBook;

/// The matching engine processes incoming orders against the order book
/// and generates trades when orders cross.
#[derive(Debug)]
pub struct MatchingEngine {
    /// Order books keyed by symbol
    order_books: Vec<OrderBook>,
    /// Stop/trigger orders keyed by symbol, checked after each trade
    trigger_orders: HashMap<String, Vec<Order>>,
    /// OCO pairs: oco_id -> (leg1_order_id, leg2_order_id)
    oco_pairs: HashMap<Uuid, (Uuid, Uuid)>,
    /// Reverse lookup: order_id -> oco_id
    oco_lookup: HashMap<Uuid, Uuid>,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            order_books: Vec::new(),
            trigger_orders: HashMap::new(),
            oco_pairs: HashMap::new(),
            oco_lookup: HashMap::new(),
        }
    }

    /// Get or create an order book for a symbol - returns index
    fn get_or_create_book_index(&mut self, symbol: &str) -> usize {
        for (i, book) in self.order_books.iter().enumerate() {
            if book.symbol == symbol {
                return i;
            }
        }
        self.order_books.push(OrderBook::new(symbol));
        self.order_books.len() - 1
    }

    /// Validate an order before processing
    fn validate_order(&self, order: &Order) -> ExchangeResult<()> {
        if order.quantity <= 0.0 {
            return Err(err_invalid_quantity("Quantity must be positive"));
        }

        if order.order_type == OrderType::Limit {
            match order.price {
                Some(p) if p <= 0.0 => {
                    return Err(err_invalid_price("Price must be positive for limit orders"));
                }
                None => {
                    return Err(err_invalid_price("Limit orders require a price"));
                }
                _ => {}
            }
        }

        if order.symbol.is_empty() {
            return Err(err_invalid_order("Symbol cannot be empty"));
        }

        Ok(())
    }

    /// Process an incoming order against the order book
    pub fn process_order(&mut self, mut order: Order) -> ExchangeResult<PlaceOrderResponse> {
        self.validate_order(&order)?;

        let book_index = self.get_or_create_book_index(&order.symbol);
        let mut trades = Vec::new();

        match order.order_type {
            OrderType::Market => {
                if order.side == Side::Buy {
                    self.match_market_buy(book_index, &mut order, &mut trades)?;
                } else {
                    self.match_market_sell(book_index, &mut order, &mut trades)?;
                }
            }
            OrderType::StopLoss | OrderType::StopLimit | OrderType::TakeProfit | OrderType::TakeProfitLimit | OrderType::TrailingStop => {
                // Stop orders are stored in trigger_orders and checked after each trade
                order.status = OrderStatus::Open;
                let entry = self.trigger_orders.entry(order.symbol.clone()).or_default();
                entry.push(order.clone());
            }
            OrderType::Limit => {
                let order_price = order.price.unwrap_or(0.0);
                let tif = order.time_in_force;

                // PostOnly: reject if order would immediately match
                if tif == crate::types::TimeInForce::PostOnly {
                    let book = &self.order_books[book_index];
                    let would_match = match order.side {
                        Side::Buy => book.peek_best_ask().map_or(false, |a| a.price.unwrap_or(f64::MAX) <= order_price),
                        Side::Sell => book.peek_best_bid().map_or(false, |b| b.price.unwrap_or(0.0) >= order_price),
                    };
                    if would_match {
                        return Err(err_invalid_order("PostOnly order would match immediately"));
                    }
                }

                // FOK: check liquidity BEFORE matching to avoid partial fill state corruption
                if tif == crate::types::TimeInForce::Fok {
                    let book = &self.order_books[book_index];
                    let available = book.available_liquidity(order.side, order_price);
                    if available < order.quantity {
                        return Err(err_invalid_order("FOK insufficient liquidity"));
                    }
                }

                if order.side == Side::Buy {
                    self.match_limit_buy(book_index, &mut order, &mut trades)?;
                } else {
                    self.match_limit_sell(book_index, &mut order, &mut trades)?;
                }

                if !order.is_fully_filled() {
                    order.status = if order.filled_quantity > 0.0 {
                        OrderStatus::PartiallyFilled
                    } else {
                        OrderStatus::Open
                    };
                    // IOC: don't add remaining to orderbook
                    if tif != crate::types::TimeInForce::Ioc {
                        self.order_books[book_index].add_order(order.clone());
                    } else if order.filled_quantity > 0.0 {
                        order.status = OrderStatus::PartiallyFilled;
                    } else {
                        order.status = OrderStatus::Cancelled;
                    }
                }
            }
        }

        let status = if order.is_fully_filled() {
            OrderStatus::Filled
        } else if trades.is_empty() && order.order_type == OrderType::Market {
            OrderStatus::Cancelled
        } else if order.filled_quantity > 0.0 {
            OrderStatus::PartiallyFilled
        } else {
            order.status
        };

        let message = match status {
            OrderStatus::Filled => format!("Order filled via {} trade(s)", trades.len()),
            OrderStatus::PartiallyFilled => {
                format!("Partially filled: {}/{} executed", order.filled_quantity, order.quantity)
            }
            OrderStatus::Open => "Order added to open orders".into(),
            OrderStatus::Cancelled => "Order could not be filled (no liquidity)".into(),
            _ => "Order processed".into(),
        };

        Ok(PlaceOrderResponse {
            order_id: order.id,
            status,
            trades,
            message,
            filled_quantity: order.filled_quantity,
            remaining_quantity: order.remaining_quantity(),
        })
    }

    /// Match market buy against asks
    fn match_market_buy(
        &mut self,
        book_index: usize,
        order: &mut Order,
        trades: &mut Vec<Trade>,
    ) -> ExchangeResult<()> {
        let mut remaining = order.remaining_quantity();

        while remaining > 0.0 {
            let best_ask_price;
            let best_ask_id;
            let best_ask_user_id;
            let best_ask_remaining;

            {
                let book = &mut self.order_books[book_index];
                match book.peek_best_ask() {
                    Some(ask) => {
                        best_ask_price = ask.price;
                        best_ask_id = ask.id;
                        best_ask_user_id = ask.user_id.clone();
                        best_ask_remaining = ask.remaining_quantity();
                    }
                    None => {
                        if trades.is_empty() {
                            return Err(err_invalid_order("No liquidity available"));
                        }
                        break;
                    }
                }
            }

            let trade_qty = remaining.min(best_ask_remaining);
            let trade_price = best_ask_price.unwrap_or(0.0);

            let trade = Trade::new(
                order.symbol.clone(),
                order.id,
                best_ask_id,
                order.user_id.clone(),
                best_ask_user_id,
                trade_price,
                trade_qty,
                Side::Buy, // market buy is taker
            );
            trades.push(trade);

            let book = &mut self.order_books[book_index];
            if let Some(maker) = book.get_order_mut(best_ask_id) {
                maker.filled_quantity += trade_qty;
                if maker.is_fully_filled() {
                    maker.status = OrderStatus::Filled;
                    book.pop_best_ask();
                }
            }

            order.filled_quantity += trade_qty;
            remaining -= trade_qty;
        }

        Ok(())
    }

    /// Match market sell against bids
    fn match_market_sell(
        &mut self,
        book_index: usize,
        order: &mut Order,
        trades: &mut Vec<Trade>,
    ) -> ExchangeResult<()> {
        let mut remaining = order.remaining_quantity();

        while remaining > 0.0 {
            let best_bid_price;
            let best_bid_id;
            let best_bid_user_id;
            let best_bid_remaining;

            {
                let book = &mut self.order_books[book_index];
                match book.peek_best_bid() {
                    Some(bid) => {
                        best_bid_price = bid.price;
                        best_bid_id = bid.id;
                        best_bid_user_id = bid.user_id.clone();
                        best_bid_remaining = bid.remaining_quantity();
                    }
                    None => {
                        if trades.is_empty() {
                            return Err(err_invalid_order("No liquidity available"));
                        }
                        break;
                    }
                }
            }

            let trade_qty = remaining.min(best_bid_remaining);
            let trade_price = best_bid_price.unwrap_or(0.0);

            let trade = Trade::new(
                order.symbol.clone(),
                best_bid_id,
                order.id,
                best_bid_user_id,
                order.user_id.clone(),
                trade_price,
                trade_qty,
                Side::Sell, // market sell is taker
            );
            trades.push(trade);

            let book = &mut self.order_books[book_index];
            if let Some(maker) = book.get_order_mut(best_bid_id) {
                maker.filled_quantity += trade_qty;
                if maker.is_fully_filled() {
                    maker.status = OrderStatus::Filled;
                    book.pop_best_bid();
                }
            }

            order.filled_quantity += trade_qty;
            remaining -= trade_qty;
        }

        Ok(())
    }

    /// Match limit buy against asks
    fn match_limit_buy(
        &mut self,
        book_index: usize,
        order: &mut Order,
        trades: &mut Vec<Trade>,
    ) -> ExchangeResult<()> {
        let order_price = order.price.unwrap_or(0.0);
        let mut remaining = order.remaining_quantity();

        loop {
            let ask_info = {
                let book = &self.order_books[book_index];
                match book.peek_best_ask() {
                    Some(ask) if ask.price.unwrap_or(f64::MAX) <= order_price => {
                        Some((ask.id, ask.price, ask.user_id.clone(), ask.remaining_quantity()))
                    }
                    _ => None,
                }
            };

            match ask_info {
                Some((ask_id, ask_price, ask_user, ask_remaining)) => {
                    let trade_qty = remaining.min(ask_remaining);
                    let trade_price = ask_price.unwrap_or(0.0);

                    let trade = Trade::new(
                        order.symbol.clone(),
                        order.id,
                        ask_id,
                        order.user_id.clone(),
                        ask_user,
                        trade_price,
                        trade_qty,
                        Side::Buy, // limit buy is taker
                    );
                    trades.push(trade);

                    let book = &mut self.order_books[book_index];
                    if let Some(maker) = book.get_order_mut(ask_id) {
                        maker.filled_quantity += trade_qty;
                        if maker.is_fully_filled() {
                            maker.status = OrderStatus::Filled;
                            book.pop_best_ask();
                        }
                    }

                    order.filled_quantity += trade_qty;
                    remaining -= trade_qty;
                }
                None => break,
            }
        }

        Ok(())
    }

    /// Match limit sell against bids
    fn match_limit_sell(
        &mut self,
        book_index: usize,
        order: &mut Order,
        trades: &mut Vec<Trade>,
    ) -> ExchangeResult<()> {
        let order_price = order.price.unwrap_or(0.0);
        let mut remaining = order.remaining_quantity();

        loop {
            let bid_info = {
                let book = &self.order_books[book_index];
                match book.peek_best_bid() {
                    Some(bid) if bid.price.unwrap_or(0.0) >= order_price => {
                        Some((bid.id, bid.price, bid.user_id.clone(), bid.remaining_quantity()))
                    }
                    _ => None,
                }
            };

            match bid_info {
                Some((bid_id, bid_price, bid_user, bid_remaining)) => {
                    let trade_qty = remaining.min(bid_remaining);
                    let trade_price = bid_price.unwrap_or(0.0);

                    let trade = Trade::new(
                        order.symbol.clone(),
                        bid_id,
                        order.id,
                        bid_user,
                        order.user_id.clone(),
                        trade_price,
                        trade_qty,
                        Side::Sell, // limit sell is taker
                    );
                    trades.push(trade);

                    let book = &mut self.order_books[book_index];
                    if let Some(maker) = book.get_order_mut(bid_id) {
                        maker.filled_quantity += trade_qty;
                        if maker.is_fully_filled() {
                            maker.status = OrderStatus::Filled;
                            book.pop_best_bid();
                        }
                    }

                    order.filled_quantity += trade_qty;
                    remaining -= trade_qty;
                }
                None => break,
            }
        }

        Ok(())
    }

    /// Check and return triggered stop orders after a trade
    /// Converts StopLoss -> Market, StopLimit -> Limit for execution
    pub fn check_trigger_orders(&mut self, symbol: &str, last_price: f64) -> Vec<Order> {
        let mut triggered = Vec::new();
        if let Some(orders) = self.trigger_orders.get_mut(symbol) {
            let mut remaining = Vec::new();
            for order in orders.drain(..) {
                if order.is_triggered_by(last_price) {
                    let mut t = order.clone();
                    t.status = crate::types::OrderStatus::Pending;
                    t.filled_quantity = 0.0;
                    // Convert to executable order type
                    // Update trailing stop price before checking trigger
                    if t.order_type == OrderType::TrailingStop {
                        t.update_trail(last_price);
                        // Re-check if still triggered after trail update
                        if !t.is_triggered_by(last_price) {
                            remaining.push(t);
                            continue;
                        }
                    }
                    match t.order_type {
                        OrderType::StopLoss => {
                            t.order_type = OrderType::Market;
                            t.price = None;
                            t.trigger_price = None;
                        }
                        OrderType::StopLimit => {
                            t.order_type = OrderType::Limit;
                            t.trigger_price = None;
                        }
                        OrderType::TakeProfit => {
                            t.order_type = OrderType::Market;
                            t.price = None;
                            t.trigger_price = None;
                        }
                        OrderType::TakeProfitLimit => {
                            t.order_type = OrderType::Limit;
                            t.trigger_price = None;
                        }
                        OrderType::TrailingStop => {
                            t.order_type = OrderType::Market;
                            t.price = None;
                            t.trigger_price = None;
                        }
                        _ => {}
                    }
                    triggered.push(t);
                } else {
                    remaining.push(order);
                }
            }
            *orders = remaining;
        }
        triggered
    }

    /// Cancel an active order (including trigger orders)
    pub fn cancel_order(&mut self, symbol: &str, order_id: Uuid) -> ExchangeResult<Order> {
        let book_index = self.get_or_create_book_index(symbol);

        // Check orderbook first
        if let Some(order) = self.order_books[book_index].remove_order(order_id) {
            let mut o = order;
            o.status = OrderStatus::Cancelled;
            return Ok(o);
        }

        // Check trigger orders
        if let Some(orders) = self.trigger_orders.get_mut(symbol) {
            if let Some(pos) = orders.iter().position(|o| o.id == order_id) {
                let mut order = orders.remove(pos);
                order.status = OrderStatus::Cancelled;
                return Ok(order);
            }
        }

        Err(err_order_not_found(format!("Order not found: {}", order_id)))
    }

    /// Get order status
    pub fn get_order_status(&self, symbol: &str, order_id: Uuid) -> ExchangeResult<&Order> {
        for book in &self.order_books {
            if book.symbol == symbol {
                return book.get_order(order_id).ok_or_else(|| {
                    err_order_not_found(format!("Order not found: {}", order_id))
                });
            }
        }
        Err(err_order_not_found(format!("Symbol not found: {}", symbol)))
    }

    /// Get a reference to an order book
    pub fn get_book(&self, symbol: &str) -> Option<&OrderBook> {
        self.order_books.iter().find(|b| b.symbol == symbol)
    }

    /// Get or create an order book for a symbol (public version)
    pub fn get_or_create_book(&mut self, symbol: &str) -> &mut OrderBook {
        let index = self.get_or_create_book_index(symbol);
        &mut self.order_books[index]
    }

    /// Get trigger orders for a user
    pub fn get_trigger_orders(&self, uid: &str) -> ExchangeResult<Vec<Order>> {
        let mut orders = Vec::new();
        for (_sym, list) in &self.trigger_orders {
            for order in list {
                if order.user_id == uid && order.is_active() {
                    orders.push(order.clone());
                }
            }
        }
        Ok(orders)
    }

    /// Add a trigger (stop) order to the watch list
    pub fn add_trigger_order(&mut self, order: Order) {
        self.trigger_orders.entry(order.symbol.clone()).or_default().push(order);
    }

    /// Get a mutable reference to an order book
    pub fn get_book_mut(&mut self, symbol: &str) -> Option<&mut OrderBook> {
        self.order_books.iter_mut().find(|b| b.symbol == symbol)
    }

    /// Get all order books
    pub fn order_books(&self) -> &[OrderBook] {
        &self.order_books
    }

    /// Find an order by ID across ALL order books and trigger orders
    pub fn find_order_by_id(&self, order_id: Uuid) -> Option<(String, &Order)> {
        for book in &self.order_books {
            if let Some(order) = book.get_order(order_id) {
                return Some((book.symbol.clone(), order));
            }
        }
        for (symbol, orders) in &self.trigger_orders {
            if let Some(order) = orders.iter().find(|o| o.id == order_id) {
                return Some((symbol.clone(), order));
            }
        }
        None
    }

    /// Register an OCO pair: when one leg fills, the other is auto-cancelled.
    pub fn register_oco_pair(&mut self, oco_id: Uuid, leg1: Uuid, leg2: Uuid) {
        self.oco_pairs.insert(oco_id, (leg1, leg2));
        self.oco_lookup.insert(leg1, oco_id);
        self.oco_lookup.insert(leg2, oco_id);
    }

    /// Cancel the sibling order in an OCO pair.
    /// Returns the cancelled sibling if found.
    pub fn cancel_oco_sibling(&mut self, filled_order_id: Uuid) -> Option<Order> {
        let (sibling_id, oco_id) = {
            let oco_id = self.oco_lookup.get(&filled_order_id).copied()?;
            let (leg1, leg2) = self.oco_pairs.get(&oco_id)?;
            let sibling_id = if *leg1 == filled_order_id { *leg2 } else { *leg1 };
            (sibling_id, oco_id)
        };
        let result = self.cancel_order_by_id(sibling_id).ok()?;
        self.oco_pairs.remove(&oco_id);
        self.oco_lookup.remove(&filled_order_id);
        self.oco_lookup.remove(&sibling_id);
        Some(result)
    }

    /// Cancel an order by ID across ALL order books and trigger orders
    pub fn cancel_order_by_id(&mut self, order_id: Uuid) -> ExchangeResult<Order> {
        for i in 0..self.order_books.len() {
            if let Some(_order) = self.order_books[i].get_order(order_id) {
                let symbol = self.order_books[i].symbol.clone();
                return self.cancel_order(&symbol, order_id);
            }
        }
        // Check trigger orders across all symbols
        for (_symbol, orders) in &mut self.trigger_orders {
            if let Some(pos) = orders.iter().position(|o| o.id == order_id) {
                let mut order = orders.remove(pos);
                order.status = OrderStatus::Cancelled;
                return Ok(order);
            }
        }
        Err(err_order_not_found(format!("Order not found: {}", order_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_limit_match() {
        let mut engine = MatchingEngine::new();

        let sell = Order::new_limit("seller".into(), "BTC/USD".into(), Side::Sell, 50000.0, 1.0);
        let sell_result = engine.process_order(sell).unwrap();
        assert_eq!(sell_result.status, OrderStatus::Open);

        let buy = Order::new_limit("buyer".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);
        let buy_result = engine.process_order(buy).unwrap();
        assert_eq!(buy_result.status, OrderStatus::Filled);
        assert_eq!(buy_result.trades.len(), 1);
        assert_eq!(buy_result.trades[0].price, 50000.0);
        assert_eq!(buy_result.trades[0].quantity, 1.0);
    }

    #[test]
    fn test_partial_fill() {
        let mut engine = MatchingEngine::new();

        engine.process_order(
            Order::new_limit("seller".into(), "BTC/USD".into(), Side::Sell, 50000.0, 0.5)
        ).unwrap();

        let result = engine.process_order(
            Order::new_limit("buyer".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0)
        ).unwrap();
        assert_eq!(result.status, OrderStatus::PartiallyFilled);
        assert_eq!(result.trades.len(), 1);
    }

    #[test]
    fn test_no_match() {
        let mut engine = MatchingEngine::new();

        engine.process_order(
            Order::new_limit("seller".into(), "BTC/USD".into(), Side::Sell, 51000.0, 1.0)
        ).unwrap();

        let result = engine.process_order(
            Order::new_limit("buyer".into(), "BTC/USD".into(), Side::Buy, 49000.0, 1.0)
        ).unwrap();
        assert_eq!(result.status, OrderStatus::Open);
        assert_eq!(result.trades.len(), 0);
    }

    #[test]
    fn test_market_order() {
        let mut engine = MatchingEngine::new();

        engine.process_order(
            Order::new_limit("seller".into(), "BTC/USD".into(), Side::Sell, 50000.0, 1.0)
        ).unwrap();

        let result = engine.process_order(
            Order::new_market("buyer".into(), "BTC/USD".into(), Side::Buy, 1.0)
        ).unwrap();
        assert_eq!(result.status, OrderStatus::Filled);
        assert_eq!(result.trades.len(), 1);
        assert_eq!(result.trades[0].price, 50000.0);
    }

    #[test]
    fn test_multiple_price_levels() {
        let mut engine = MatchingEngine::new();

        engine.process_order(
            Order::new_limit("s1".into(), "BTC/USD".into(), Side::Sell, 51000.0, 1.0)
        ).unwrap();
        engine.process_order(
            Order::new_limit("s2".into(), "BTC/USD".into(), Side::Sell, 50000.0, 1.0)
        ).unwrap();
        engine.process_order(
            Order::new_limit("s3".into(), "BTC/USD".into(), Side::Sell, 52000.0, 1.0)
        ).unwrap();

        // Buy 2.5 BTC market order - consumes from all 3 levels (1.0 + 1.0 + 0.5)
        let result = engine.process_order(
            Order::new_market("buyer".into(), "BTC/USD".into(), Side::Buy, 2.5)
        ).unwrap();
        assert_eq!(result.trades.len(), 3);
        assert_eq!(result.trades[0].price, 50000.0);
        assert_eq!(result.trades[1].price, 51000.0);
        assert_eq!(result.trades[2].price, 52000.0);
    }

    #[test]
    fn test_cancel_order() {
        let mut engine = MatchingEngine::new();

        let order = Order::new_limit("user".into(), "BTC/USD".into(), Side::Buy, 50000.0, 1.0);
        let order_id = order.id;
        engine.process_order(order).unwrap();

        let cancelled = engine.cancel_order("BTC/USD", order_id).unwrap();
        assert_eq!(cancelled.status, OrderStatus::Cancelled);
    }

    #[test]
    fn test_invalid_quantity() {
        let mut engine = MatchingEngine::new();
        let bad = Order::new_limit("u1".into(), "BTC/USD".into(), Side::Buy, 50000.0, 0.0);
        assert!(engine.process_order(bad).is_err());
    }

    #[test]
    fn test_time_priority() {
        let mut engine = MatchingEngine::new();

        engine.process_order(
            Order::new_limit("seller1".into(), "BTC/USD".into(), Side::Sell, 50000.0, 1.0)
        ).unwrap();
        engine.process_order(
            Order::new_limit("seller2".into(), "BTC/USD".into(), Side::Sell, 50000.0, 1.0)
        ).unwrap();

        let result = engine.process_order(
            Order::new_market("buyer".into(), "BTC/USD".into(), Side::Buy, 1.0)
        ).unwrap();
        assert_eq!(result.trades[0].seller_id, "seller1");
    }
}
