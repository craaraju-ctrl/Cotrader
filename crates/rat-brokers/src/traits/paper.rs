//! Paper Broker — Simulated trading for backtesting and paper trading.

use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct PaperBroker {
    inner: Arc<Mutex<PaperBrokerInner>>,
}

struct PaperBrokerInner {
    connected: bool,
    balance: Balance,
    orders: Vec<Order>,
    positions: Vec<Position>,
    order_counter: u64,
    market_data: HashMap<String, MarketData>,
}

impl PaperBroker {
    pub fn new(initial_balance: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PaperBrokerInner {
                connected: false,
                balance: Balance {
                    total: initial_balance,
                    available: initial_balance,
                    margin_used: 0.0,
                    unrealized_pnl: 0.0,
                },
                orders: Vec::new(),
                positions: Vec::new(),
                order_counter: 0,
                market_data: HashMap::new(),
            })),
        }
    }

    /// Update market data for simulation.
    pub fn update_price(&self, symbol: &str, price: f64) {
        let mut inner = self.inner.lock().unwrap();
        let data = inner.market_data.entry(symbol.to_string()).or_default();
        data.symbol = symbol.to_string();
        data.last = price;
        data.bid = price * 0.9999;
        data.ask = price * 1.0001;
        data.timestamp = chrono::Utc::now();

        for pos in &mut inner.positions {
            if pos.symbol == symbol {
                pos.current_price = price;
                pos.unrealized_pnl = match pos.side {
                    OrderSide::Buy => (price - pos.entry_price) * pos.quantity,
                    OrderSide::Sell => (pos.entry_price - price) * pos.quantity,
                };
            }
        }
    }
}

#[async_trait]
impl Broker for PaperBroker {
    fn name(&self) -> &str { "Paper" }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.inner.lock().unwrap().connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), BrokerError> {
        self.inner.lock().unwrap().connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool { self.inner.lock().unwrap().connected }

    async fn place_order(&self, order: NewOrder) -> Result<OrderId, BrokerError> {
        let mut inner = self.inner.lock().unwrap();
        inner.order_counter += 1;
        let id = format!("PAPER-{}", inner.order_counter);

        let filled = if order.order_type == OrderType::Market {
            let price = inner.market_data.get(&order.symbol).map(|d| d.ask).unwrap_or(order.price.unwrap_or(0.0));
            order.quantity
        } else {
            0.0
        };

        let new_order = Order {
            id: id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            quantity: order.quantity,
            filled_quantity: filled,
            price: order.price.unwrap_or(0.0),
            status: if filled > 0.0 { OrderStatus::Filled } else { OrderStatus::Pending },
            created_at: chrono::Utc::now(),
        };

        inner.orders.push(new_order);

        if filled > 0.0 && order.order_type == OrderType::Market {
            let price = inner.market_data.get(&order.symbol).map(|d| d.ask).unwrap_or(0.0);
            inner.positions.push(Position {
                symbol: order.symbol,
                side: order.side,
                quantity: filled,
                entry_price: price,
                current_price: price,
                unrealized_pnl: 0.0,
            });
        }

        Ok(id)
    }

    async fn cancel_order(&self, order_id: &OrderId) -> Result<(), BrokerError> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(order) = inner.orders.iter_mut().find(|o| &o.id == order_id) {
            order.status = OrderStatus::Cancelled;
            Ok(())
        } else {
            Err(BrokerError::OrderRejected("Order not found".to_string()))
        }
    }

    async fn get_open_orders(&self, _symbol: &str) -> Result<Vec<Order>, BrokerError> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.orders.iter().filter(|o| o.status == OrderStatus::Pending).cloned().collect())
    }

    async fn get_positions(&self) -> Result<Vec<Position>, BrokerError> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.positions.clone())
    }

    async fn get_balance(&self) -> Result<Balance, BrokerError> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.balance.clone())
    }

    async fn get_market_data(&self, symbol: &str) -> Result<MarketData, BrokerError> {
        let inner = self.inner.lock().unwrap();
        inner.market_data.get(symbol).cloned().ok_or(BrokerError::ApiError("No data".to_string()))
    }

    async fn subscribe(&self, _symbols: Vec<String>) -> Result<(), BrokerError> {
        Ok(())
    }
}
