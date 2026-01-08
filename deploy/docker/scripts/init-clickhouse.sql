-- HFT System ClickHouse Schema Initialization
-- This script is run automatically when the ClickHouse container starts

-- Create the HFT database
CREATE DATABASE IF NOT EXISTS hft;

-- Orders table - tracks all order lifecycle events
CREATE TABLE IF NOT EXISTS hft.orders
(
    order_id UInt64,
    symbol LowCardinality(String),
    side Int8,                          -- 1=Buy, -1=Sell
    price Float64,
    qty Float64,
    status UInt8,                       -- 0=New, 1=PartiallyFilled, 2=Filled, 3=Cancelled, etc.
    create_time DateTime64(9, 'UTC'),
    update_time DateTime64(9, 'UTC'),
    exch_time DateTime64(9, 'UTC'),
    leaves_qty Float64,
    exec_qty Float64,
    exec_price Float64,
    latency_ns Int64,
    strategy_id LowCardinality(String)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(create_time)
ORDER BY (strategy_id, symbol, create_time, order_id)
TTL create_time + INTERVAL 90 DAY;

-- Fills table - individual trade executions
CREATE TABLE IF NOT EXISTS hft.fills
(
    fill_id UInt64,
    order_id UInt64,
    symbol LowCardinality(String),
    side Int8,
    price Float64,
    qty Float64,
    fee Float64,
    is_maker UInt8,
    exch_time DateTime64(9, 'UTC'),
    local_time DateTime64(9, 'UTC'),
    slippage_bps Float64,
    strategy_id LowCardinality(String)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(exch_time)
ORDER BY (strategy_id, symbol, exch_time, fill_id)
TTL exch_time + INTERVAL 90 DAY;

-- Positions table - position snapshots over time
CREATE TABLE IF NOT EXISTS hft.positions
(
    timestamp DateTime64(9, 'UTC'),
    symbol LowCardinality(String),
    position Float64,
    avg_price Float64,
    unrealized_pnl Float64,
    realized_pnl Float64,
    strategy_id LowCardinality(String)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (strategy_id, symbol, timestamp)
TTL timestamp + INTERVAL 30 DAY;

-- Strategy status table - strategy state changes
CREATE TABLE IF NOT EXISTS hft.strategy_status
(
    timestamp DateTime64(9, 'UTC'),
    strategy_id LowCardinality(String),
    status UInt8,
    message String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (strategy_id, timestamp)
TTL timestamp + INTERVAL 30 DAY;

-- Materialized view for daily PnL aggregation
CREATE MATERIALIZED VIEW IF NOT EXISTS hft.daily_pnl
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(date)
ORDER BY (strategy_id, symbol, date)
AS
SELECT
    toDate(exch_time) AS date,
    strategy_id,
    symbol,
    sum(qty * price * side) AS trading_volume,
    sum(fee) AS total_fees,
    count() AS trade_count
FROM hft.fills
GROUP BY date, strategy_id, symbol;

-- Create user for HFT application (optional, uses default user)
-- CREATE USER IF NOT EXISTS hft IDENTIFIED BY 'hft_password';
-- GRANT ALL ON hft.* TO hft;
