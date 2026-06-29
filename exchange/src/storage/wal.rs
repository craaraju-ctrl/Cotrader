use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool, Row};
use uuid::Uuid;

use crate::types::{err_internal, ExchangeResult, Order, OrderStatus, OrderType, Side, Trade};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum WalEvent { OrderPlaced(Order), TradeExecuted(Trade), OrderCancelled { order_id: Uuid, symbol: String } }

#[derive(Clone)]
pub struct WalStore { pool: PgPool }

impl WalStore {
    pub async fn new(database_url: &str) -> ExchangeResult<Self> {
        let pool = PgPoolOptions::new().max_connections(10).connect(database_url).await
            .map_err(|e| err_internal(format!("DB connect: {}", e)))?;
        let store = Self { pool };
        store.auto_migrate().await?;
        Ok(store)
    }

    /// Automatically run schema migrations on startup.
    /// Embeds migration SQL at compile time via include_str!.
    /// Uses raw_sql to handle multi-statement migration files including PL/pgSQL DO blocks.
    async fn auto_migrate(&self) -> ExchangeResult<()> {
        let migration_001 = include_str!("../../migrations/001_initial_schema.sql");
        let migration_002 = include_str!("../../migrations/002_add_trigger_price_time_in_force.sql");

        self.pool.execute(sqlx::raw_sql(migration_001)).await
            .map_err(|e| err_internal(format!("Migration 001: {}", e)))?;
        self.pool.execute(sqlx::raw_sql(migration_002)).await
            .map_err(|e| err_internal(format!("Migration 002: {}", e)))?;

        tracing::info!("Schema migrations applied successfully");
        Ok(())
    }

    pub async fn write_order(&self, order: &Order) -> ExchangeResult<()> {
        let payload = serde_json::to_value(WalEvent::OrderPlaced(order.clone())).map_err(|e| err_internal(e.to_string()))?;
        self.write_wal("OrderPlaced", Some(order.id), &payload).await?;
        sqlx::query(r#"INSERT INTO orders (id, user_id, symbol, side, order_type, price, trigger_price, quantity, filled_quantity, status, time_in_force, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (id) DO UPDATE SET filled_quantity=EXCLUDED.filled_quantity, status=EXCLUDED.status, updated_at=EXCLUDED.updated_at"#)
            .bind(order.id).bind(&order.user_id).bind(&order.symbol)
            .bind(format!("{:?}", order.side)).bind(format!("{:?}", order.order_type))
            .bind(order.price).bind(order.trigger_price).bind(order.quantity).bind(order.filled_quantity)
            .bind(format!("{:?}", order.status)).bind(format!("{:?}", order.time_in_force)).bind(order.created_at).bind(order.updated_at)
            .execute(&self.pool).await.map_err(|e| err_internal(format!("insert: {}", e)))?;
        Ok(())
    }

    pub async fn write_trade(&self, trade: &Trade) -> ExchangeResult<()> {
        let payload = serde_json::to_value(WalEvent::TradeExecuted(trade.clone())).map_err(|e| err_internal(e.to_string()))?;
        self.write_wal("TradeExecuted", Some(trade.buy_order_id), &payload).await?;
        sqlx::query(r#"INSERT INTO trades (id, symbol, buy_order_id, sell_order_id, buyer_id, seller_id, price, quantity, total, timestamp) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#)
            .bind(trade.id).bind(&trade.symbol).bind(trade.buy_order_id).bind(trade.sell_order_id)
            .bind(&trade.buyer_id).bind(&trade.seller_id).bind(trade.price).bind(trade.quantity)
            .bind(trade.total).bind(trade.timestamp)
            .execute(&self.pool).await.map_err(|e| err_internal(format!("insert: {}", e)))?;
        Ok(())
    }

    pub async fn update_order_status(&self, order_id: Uuid, filled_qty: f64, status: &OrderStatus) -> ExchangeResult<()> {
        let now = Utc::now();
        sqlx::query(r#"UPDATE orders SET filled_quantity=$1, status=$2, updated_at=$3 WHERE id=$4"#)
            .bind(filled_qty).bind(format!("{:?}", status)).bind(now).bind(order_id)
            .execute(&self.pool).await.map_err(|e| err_internal(format!("update: {}", e)))?;
        Ok(())
    }

    pub async fn record_cancellation(&self, order_id: Uuid, _symbol: &str) -> ExchangeResult<()> {
        let now = Utc::now();
        sqlx::query(r#"UPDATE orders SET status='Cancelled', updated_at=$1 WHERE id=$2"#)
            .bind(now).bind(order_id).execute(&self.pool).await.map_err(|e| err_internal(e.to_string()))?;
        Ok(())
    }

    async fn write_wal(&self, event_type: &str, order_id: Option<Uuid>, payload: &serde_json::Value) -> ExchangeResult<()> {
        sqlx::query(r#"INSERT INTO wal_sequence (event_type, order_id, payload) VALUES ($1,$2,$3)"#)
            .bind(event_type).bind(order_id).bind(payload).execute(&self.pool).await
            .map_err(|e| err_internal(format!("wal: {}", e)))?;
        Ok(())
    }

    pub async fn recover_state(&self) -> ExchangeResult<(Vec<Order>, Vec<Trade>)> {
        let rows = sqlx::query(r#"SELECT id, user_id, symbol, side, order_type, price, trigger_price, quantity, filled_quantity, status, time_in_force, created_at, updated_at FROM orders WHERE status IN ('Open','PartiallyFilled','Pending') ORDER BY created_at ASC"#)
            .fetch_all(&self.pool).await.map_err(|e| err_internal(e.to_string()))?;
        let orders: Vec<Order> = rows.into_iter().map(|r| Order {
            id: r.get("id"), user_id: r.get("user_id"), symbol: r.get("symbol"),
            side: match r.get::<String, _>("side").as_str() { "Buy" => Side::Buy, _ => Side::Sell },
            order_type: match r.get::<String, _>("order_type").as_str() { "Limit" => OrderType::Limit, _ => OrderType::Market },
            price: r.get("price"), trigger_price: r.get("trigger_price"), quantity: r.get("quantity"), filled_quantity: r.get("filled_quantity"),
            status: match r.get::<String, _>("status").as_str() { "Open" => OrderStatus::Open, "PartiallyFilled" => OrderStatus::PartiallyFilled, "Pending" => OrderStatus::Pending, _ => OrderStatus::Rejected },
            time_in_force: match r.get::<String, _>("time_in_force").as_str() { "PostOnly" => crate::types::TimeInForce::PostOnly, "Ioc" => crate::types::TimeInForce::Ioc, "Fok" => crate::types::TimeInForce::Fok, _ => crate::types::TimeInForce::Gtc },
            created_at: r.get("created_at"), updated_at: r.get("updated_at"),
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }).collect();
        let trade_rows = sqlx::query(r#"SELECT id, symbol, buy_order_id, sell_order_id, buyer_id, seller_id, price, quantity, total, timestamp FROM trades ORDER BY timestamp ASC"#)
            .fetch_all(&self.pool).await.map_err(|e| err_internal(e.to_string()))?;
        let trades: Vec<Trade> = trade_rows.into_iter().map(|r| Trade {
            id: r.get("id"), symbol: r.get("symbol"), buy_order_id: r.get("buy_order_id"),
            sell_order_id: r.get("sell_order_id"), buyer_id: r.get("buyer_id"), seller_id: r.get("seller_id"),
            price: r.get("price"), quantity: r.get("quantity"), total: r.get("total"), timestamp: r.get("timestamp"),
            taker_side: Side::Buy,
        }).collect();
        tracing::info!("Recovered {} orders, {} trades", orders.len(), trades.len());
        Ok((orders, trades))
    }

    pub async fn get_order_history(&self, uid: &str, limit: i64, offset: usize) -> ExchangeResult<Vec<Order>> {
        let rows = sqlx::query(r#"SELECT id, user_id, symbol, side, order_type, price, trigger_price, quantity, filled_quantity, status, time_in_force, created_at, updated_at FROM orders WHERE user_id=$1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"#)
            .bind(uid).bind(limit).bind(offset as i64)
            .fetch_all(&self.pool).await.map_err(|e| err_internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| Order {
            id: r.get("id"), user_id: r.get("user_id"), symbol: r.get("symbol"),
            side: match r.get::<String, _>("side").as_str() { "Buy" => Side::Buy, _ => Side::Sell },
            order_type: match r.get::<String, _>("order_type").as_str() { "Limit" => OrderType::Limit, _ => OrderType::Market },
            price: r.get("price"), trigger_price: r.get("trigger_price"), quantity: r.get("quantity"), filled_quantity: r.get("filled_quantity"),
            status: match r.get::<String, _>("status").as_str() { "Open" => OrderStatus::Open, "PartiallyFilled" => OrderStatus::PartiallyFilled, "Pending" => OrderStatus::Pending, "Filled" => OrderStatus::Filled, "Cancelled" => OrderStatus::Cancelled, _ => OrderStatus::Rejected },
            time_in_force: match r.get::<String, _>("time_in_force").as_str() { "PostOnly" => crate::types::TimeInForce::PostOnly, "Ioc" => crate::types::TimeInForce::Ioc, "Fok" => crate::types::TimeInForce::Fok, _ => crate::types::TimeInForce::Gtc },
            created_at: r.get("created_at"), updated_at: r.get("updated_at"),
            visible_quantity: None, trailing_delta: None, stop_price: None, oco_sibling_id: None,
        }).collect())
    }

    pub async fn get_recent_trades(&self, symbol: &str, limit: i64) -> ExchangeResult<Vec<Trade>> {
        let rows = sqlx::query(r#"SELECT id, symbol, buy_order_id, sell_order_id, buyer_id, seller_id, price, quantity, total, timestamp FROM trades WHERE symbol=$1 ORDER BY timestamp DESC LIMIT $2"#)
            .bind(symbol).bind(limit).fetch_all(&self.pool).await.map_err(|e| err_internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| Trade {
            id: r.get("id"), symbol: r.get("symbol"), buy_order_id: r.get("buy_order_id"),
            sell_order_id: r.get("sell_order_id"), buyer_id: r.get("buyer_id"), seller_id: r.get("seller_id"),
            price: r.get("price"), quantity: r.get("quantity"), total: r.get("total"), timestamp: r.get("timestamp"),
            taker_side: Side::Buy,
        }).collect())
    }

    pub async fn health_check(&self) -> bool { sqlx::query("SELECT 1").execute(&self.pool).await.is_ok() }
}
