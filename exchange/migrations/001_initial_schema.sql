-- Tredo Exchange — Initial Database Schema
-- Run: psql -d tredo_exchange -f migrations/001_initial_schema.sql

-- Orders table: stores all placed orders (active + historical)
CREATE TABLE IF NOT EXISTS orders (
    id              UUID PRIMARY KEY,
    user_id         VARCHAR(64) NOT NULL,
    symbol          VARCHAR(32) NOT NULL,
    side            VARCHAR(8) NOT NULL CHECK (side IN ('Buy', 'Sell')),
    order_type      VARCHAR(16) NOT NULL CHECK (order_type IN ('Limit', 'Market', 'StopLoss', 'StopLimit')),
    price           DOUBLE PRECISION,
    trigger_price   DOUBLE PRECISION,
    quantity        DOUBLE PRECISION NOT NULL,
    filled_quantity DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    status          VARCHAR(32) NOT NULL DEFAULT 'Pending' CHECK (status IN ('Pending','Open','PartiallyFilled','Filled','Cancelled','Rejected','Expired')),
    time_in_force   VARCHAR(16) NOT NULL DEFAULT 'Gtc' CHECK (time_in_force IN ('Gtc','Ioc','Fok','PostOnly')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for order queries
CREATE INDEX IF NOT EXISTS idx_orders_user_id ON orders (user_id);
CREATE INDEX IF NOT EXISTS idx_orders_symbol ON orders (symbol);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders (status);
CREATE INDEX IF NOT EXISTS idx_orders_created_at ON orders (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_orders_user_status ON orders (user_id, status);

-- Trades table: stores all executed trades
CREATE TABLE IF NOT EXISTS trades (
    id              UUID PRIMARY KEY,
    symbol          VARCHAR(32) NOT NULL,
    buy_order_id    UUID NOT NULL,
    sell_order_id   UUID NOT NULL,
    buyer_id        VARCHAR(64) NOT NULL,
    seller_id       VARCHAR(64) NOT NULL,
    price           DOUBLE PRECISION NOT NULL,
    quantity        DOUBLE PRECISION NOT NULL,
    total           DOUBLE PRECISION NOT NULL,
    timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for trade queries
CREATE INDEX IF NOT EXISTS idx_trades_symbol ON trades (symbol);
CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades (timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_trades_symbol_ts ON trades (symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_trades_buy_order ON trades (buy_order_id);
CREATE INDEX IF NOT EXISTS idx_trades_sell_order ON trades (sell_order_id);

-- WAL sequence: append-only event log for crash recovery
CREATE TABLE IF NOT EXISTS wal_sequence (
    id          BIGSERIAL PRIMARY KEY,
    event_type  VARCHAR(32) NOT NULL,
    order_id    UUID,
    payload     JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for WAL replay
CREATE INDEX IF NOT EXISTS idx_wal_created_at ON wal_sequence (created_at ASC);
CREATE INDEX IF NOT EXISTS idx_wal_event_type ON wal_sequence (event_type);
