-- Migration: Add trigger_price and time_in_force to orders table
-- These columns are needed for stop-loss/stop-limit orders and TimeInForce support

DO $$
BEGIN
    -- Add trigger_price column if it doesn't exist
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'orders' AND column_name = 'trigger_price'
    ) THEN
        ALTER TABLE orders ADD COLUMN trigger_price DOUBLE PRECISION;
        RAISE NOTICE 'Added trigger_price column';
    ELSE
        RAISE NOTICE 'trigger_price column already exists';
    END IF;

    -- Add time_in_force column if it doesn't exist
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'orders' AND column_name = 'time_in_force'
    ) THEN
        ALTER TABLE orders ADD COLUMN time_in_force VARCHAR(16) NOT NULL DEFAULT 'Gtc';
        RAISE NOTICE 'Added time_in_force column';
    ELSE
        RAISE NOTICE 'time_in_force column already exists';
    END IF;
END $$;

-- Enlarge existing varchar columns to match the migration schema
ALTER TABLE orders ALTER COLUMN side TYPE VARCHAR(8);
ALTER TABLE orders ALTER COLUMN order_type TYPE VARCHAR(16);
ALTER TABLE orders ALTER COLUMN status TYPE VARCHAR(32);
