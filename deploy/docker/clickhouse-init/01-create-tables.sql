CREATE TABLE IF NOT EXISTS hft.orders
(
    `order_id` UInt64,
    `symbol` LowCardinality(String),
    `side` Int8,
    `price` Float64,
    `qty` Float64,
    `status` UInt8,
    `create_time` DateTime64(9, 'UTC'),
    `update_time` DateTime64(9, 'UTC'),
    `exch_time` DateTime64(9, 'UTC'),
    `leaves_qty` Float64,
    `exec_qty` Float64,
    `exec_price` Float64,
    `latency_ns` Int64,
    `strategy_id` LowCardinality(String)
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(create_time)
ORDER BY (strategy_id, symbol, create_time, order_id)
TTL toDateTime(create_time) + toIntervalDay(90)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS hft.fills
(
    `fill_id` UInt64,
    `order_id` UInt64,
    `symbol` LowCardinality(String),
    `side` Int8,
    `price` Float64,
    `qty` Float64,
    `fee` Float64,
    `is_maker` UInt8,
    `exch_time` DateTime64(9, 'UTC'),
    `local_time` DateTime64(9, 'UTC'),
    `slippage_bps` Float64,
    `strategy_id` LowCardinality(String)
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(exch_time)
ORDER BY (strategy_id, symbol, exch_time, fill_id)
TTL toDateTime(exch_time) + toIntervalDay(90)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS hft.positions
(
    `timestamp` DateTime64(9, 'UTC'),
    `connector` LowCardinality(String),
    `symbol` LowCardinality(String),
    `position` Float64,
    `avg_price` Float64,
    `unrealized_pnl` Float64,
    `realized_pnl` Float64,
    `strategy_id` LowCardinality(String),
    `entry_price` Float64,
    `mark_price` Float64
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (connector, strategy_id, symbol, timestamp)
TTL toDateTime(timestamp) + toIntervalDay(30)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS hft.account_equity
(
    `timestamp` DateTime64(9, 'UTC'),
    `connector` LowCardinality(String),
    `strategy_id` LowCardinality(String),
    `account_equity` Float64,
    `actual_equity` Float64,
    `uni_mmr` Float64,
    `account_initial_margin` Float64,
    `account_maint_margin` Float64
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (connector, strategy_id, timestamp)
TTL toDateTime(timestamp) + toIntervalDay(90)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS hft.strategy_status
(
    `timestamp` DateTime64(9, 'UTC'),
    `strategy_id` LowCardinality(String),
    `status` UInt8,
    `message` String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (strategy_id, timestamp)
TTL toDateTime(timestamp) + toIntervalDay(30)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS hft.income
(
    `timestamp` DateTime64(9, 'UTC'),
    `connector` LowCardinality(String),
    `strategy_id` LowCardinality(String),
    `symbol` LowCardinality(String),
    `income_type` LowCardinality(String),
    `income` Float64,
    `asset` LowCardinality(String),
    `info` String,
    `tran_id` Int64,
    `trade_id` String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (connector, strategy_id, symbol, timestamp, tran_id)
TTL toDateTime(timestamp) + toIntervalDay(90)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS hft.factor_snapshots
(
    `timestamp` DateTime64(9, 'UTC'),
    `connector` LowCardinality(String),
    `strategy_id` LowCardinality(String),
    `symbol` LowCardinality(String),
    `factor_name` LowCardinality(String),
    `factor_value` Float64
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (strategy_id, symbol, timestamp, factor_name)
TTL toDateTime(timestamp) + toIntervalDay(30)
SETTINGS index_granularity = 8192;

CREATE MATERIALIZED VIEW IF NOT EXISTS hft.daily_pnl
(
    `date` Date,
    `strategy_id` LowCardinality(String),
    `symbol` LowCardinality(String),
    `trading_volume` Float64,
    `total_fees` Float64,
    `trade_count` UInt64
)
ENGINE = SummingMergeTree
PARTITION BY toYYYYMM(date)
ORDER BY (strategy_id, symbol, date)
SETTINGS index_granularity = 8192
AS SELECT
    toDate(exch_time) AS date,
    strategy_id,
    symbol,
    sum((qty * price) * side) AS trading_volume,
    sum(fee) AS total_fees,
    count() AS trade_count
FROM hft.fills
GROUP BY
    date,
    strategy_id,
    symbol;
