# Task: Position & Order & PnL Tracking System

## Goal
Build a comprehensive real-time position, order, and PnL tracking system with unified data structures across exchanges (Binance Futures, Bybit), stored in ClickHouse, visualized via Grafana dashboards integrated into the existing frontend.

## Current State Analysis

### Existing Infrastructure
- **ClickHouse Tables**: orders, fills, positions, strategy_status, daily_pnl (materialized view)
- **order-recorder**: Standalone service that writes IPC events to ClickHouse
- **Dashboard**: React frontend with pages: Dashboard, Bots, Infrastructure, Config, Data, Logs
- **Connectors**: Binance Futures, Bybit (REST + WS)

### Gaps to Address
1. Position snapshots not being recorded periodically
2. No Income/Realized PnL tracking from exchange APIs
3. No Grafana integration
4. No unified cross-exchange data normalization
5. Dashboard lacks PnL/position visualization

## Phases

- [x] **Phase 1: Data Collection Enhancement** ✅ COMPLETED
  - [x] 1.1 Add periodic position sync via REST API to connectors
    - Added `get_position_information()` to Binance Futures REST client
    - Added `get_all_positions()` to Bybit REST client
    - Created `connector/src/sync.rs` module with `SyncTask`, `PositionProvider`, `IncomeProvider` traits
    - Implemented `BinanceFuturesProvider` and `BybitProvider`
  - [x] 1.2 Implement Income API integration
    - Added `get_income()` to Binance Futures REST client (`/fapi/v1/income`)
    - Added `get_transaction_log()`, `get_closed_pnl()` to Bybit REST client
  - [x] 1.3 User Data Stream position updates (already working via existing WS handlers)
  - [x] 1.4 Unified data structures for cross-exchange normalization
    - Created `PositionData`, `IncomeData` in sync module
    - Unified income type mapping: REALIZED_PNL, COMMISSION, FUNDING_FEE

- [x] **Phase 2: ClickHouse Schema Enhancement** ✅ COMPLETED
  - [x] 2.1 Created `income` table in `deploy/docker/scripts/init-clickhouse.sql`
  - [x] 2.2 Enhanced `positions` table with `connector`, `entry_price`, `mark_price` fields
  - [x] 2.3 Added `daily_pnl` materialized view (already existed)
  - [ ] 2.4 Create hourly_pnl materialized view (optional, can be done later)

- [x] **Phase 3: Recording Service Enhancement** ✅ COMPLETED
  - [x] 3.1 Added `IncomeEvent` to `order-recorder/src/events.rs`
  - [x] 3.2 Added `Income` variant to `RecordingEvent` enum
  - [x] 3.3 Updated `ClickHouseWriter` with `write_income()`, `flush_incomes()`
  - [x] 3.4 Updated `RecordingService` to handle Income events
  - [x] 3.5 Added `publish_income()` to `RecordingPublisher`
  - [x] 3.6 Fixed iceoryx2 0.6 migration (ZeroCopySend derive)
  - [x] 3.7 Enhanced `PositionSnapshot` with `connector`, `entry_price`, `mark_price`

- [x] **Phase 4: Grafana Setup** ✅ COMPLETED
  - [x] 4.1 Deploy Grafana container in docker-compose (port 13001)
  - [x] 4.2 Configure ClickHouse data source with provisioning
  - [x] 4.3 Created Trading Overview dashboard with:
    - Orders (1h) stat panel
    - Fills (1h) stat panel
    - Avg Latency stat panel
    - Active Symbols stat panel
    - Orders by Symbol time series
    - Fills by Symbol time series
    - Order Latency Distribution histogram
    - Latency Percentiles Over Time
    - Recent Orders table
  - [x] 4.4 Added Symbol and Strategy template variables

- [x] **Phase 5: Frontend Integration** ✅ COMPLETED
  - [x] 5.1 Created Analytics page component (`server/dashboard/src/pages/Analytics.tsx`)
  - [x] 5.2 Added Grafana iframe embedding with kiosk mode
  - [x] 5.3 Added Analytics route to App.tsx and navigation to Sidebar.tsx
  - [x] 5.4 Configured Grafana for iframe embedding (GF_SECURITY_ALLOW_EMBEDDING)
  - [x] 5.5 Built frontend successfully

- [x] **Phase 6: Additional Grafana Dashboards** ✅ COMPLETED
  - [x] 6.1 Created Position Monitor dashboard (`position-monitor.json`)
    - Total Unrealized/Realized PnL stats
    - Active Positions count
    - Position History by Symbol (time series)
    - Unrealized PnL by Symbol (time series)
    - Current Positions table
  - [x] 6.2 Created PnL Tracking dashboard (`pnl-tracking.json`)
    - Realized PnL, Commission, Funding Fees stats
    - Cumulative PnL Over Time (time series)
    - Realized PnL by Symbol (bar chart)
    - Fee Breakdown pie chart
    - PnL by Symbol pie chart
    - PnL Summary table
  - [x] 6.3 Created Trading Activity dashboard (`trading-activity.json`)
    - Total Fills, Trading Volume, Orders stats
    - Maker Rate, Avg Latency stats
    - Trading Volume by Symbol (bar chart)
    - Trade Count by Symbol (time series)
    - Maker vs Taker Rate (time series)
    - Buy vs Sell Distribution (time series)
    - Trading Summary table
  - [x] 6.4 Updated Analytics page with dashboard selector

## Architecture Design

### Data Flow
```
┌─────────────────────────────────────────────────────────────────┐
│                        Exchanges                                 │
│  ┌──────────────────┐  ┌──────────────────┐                     │
│  │ Binance Futures  │  │     Bybit        │                     │
│  │ - User Data WS   │  │ - Private WS     │                     │
│  │ - REST APIs      │  │ - REST APIs      │                     │
│  └────────┬─────────┘  └────────┬─────────┘                     │
└───────────┼─────────────────────┼───────────────────────────────┘
            │                     │
            ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Connector Layer                              │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Position Sync Task (every 30s)                          │   │
│  │  - GET /fapi/v2/positionRisk (Binance)                   │   │
│  │  - GET /v5/position/list (Bybit)                         │   │
│  └──────────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Income Sync Task (every 60s)                            │   │
│  │  - GET /fapi/v1/income (Binance)                         │   │
│  │  - GET /v5/account/transaction-log (Bybit)               │   │
│  └──────────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  User Data Stream (real-time)                            │   │
│  │  - Position updates                                      │   │
│  │  - Order updates                                         │   │
│  │  - Account updates                                       │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │
            │ IPC (iceoryx2)
            ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Order Recorder Service                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Event Types:                                            │   │
│  │  - OrderUpdate (existing)                                │   │
│  │  - FillEvent (existing)                                  │   │
│  │  - PositionSnapshot (existing)                           │   │
│  │  - StrategyStatus (existing)                             │   │
│  │  - IncomeEvent (NEW)                                     │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │
            │ HTTP Insert
            ▼
┌─────────────────────────────────────────────────────────────────┐
│                        ClickHouse                                │
│  ┌────────────────┐ ┌────────────────┐ ┌────────────────────┐   │
│  │     orders     │ │     fills      │ │    positions       │   │
│  └────────────────┘ └────────────────┘ └────────────────────┘   │
│  ┌────────────────┐ ┌────────────────┐ ┌────────────────────┐   │
│  │  strategy_stat │ │    income      │ │  daily_pnl (MV)    │   │
│  └────────────────┘ └────────────────┘ └────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │
            │ ClickHouse Data Source
            ▼
┌─────────────────────────────────────────────────────────────────┐
│                         Grafana                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Dashboards:                                             │   │
│  │  - Position Monitor                                      │   │
│  │  - PnL Tracking                                          │   │
│  │  - Trading Volume                                        │   │
│  │  - Order Latency                                         │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
            │
            │ iframe embed
            ▼
┌─────────────────────────────────────────────────────────────────┐
│                    React Dashboard                               │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  /analytics - Grafana embedded panels                    │   │
│  │  Filters: Account | Strategy | Symbol | Time Range       │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### New ClickHouse Schema

```sql
-- Income table (fees, realized PnL, funding)
CREATE TABLE IF NOT EXISTS hft.income
(
    timestamp DateTime64(9, 'UTC'),      -- Income time
    connector LowCardinality(String),    -- bf, bybit, etc.
    strategy_id LowCardinality(String),
    symbol LowCardinality(String),
    income_type LowCardinality(String),  -- REALIZED_PNL, COMMISSION, FUNDING_FEE, etc.
    income Float64,                       -- Amount (positive = profit, negative = loss)
    asset LowCardinality(String),        -- USDT, BTC, etc.
    info String,                         -- Additional info (trade ID, etc.)
    trade_id UInt64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (connector, strategy_id, symbol, timestamp)
TTL toDateTime(timestamp) + INTERVAL 90 DAY;

-- Enhanced positions table (add connector field)
-- ALTER TABLE hft.positions ADD COLUMN connector LowCardinality(String) AFTER timestamp;
-- ALTER TABLE hft.positions ADD COLUMN mark_price Float64 AFTER mid_price;
-- ALTER TABLE hft.positions ADD COLUMN entry_price Float64 AFTER avg_price;

-- Materialized view for hourly PnL
CREATE MATERIALIZED VIEW IF NOT EXISTS hft.hourly_pnl
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY (connector, strategy_id, symbol, hour)
AS
SELECT
    toStartOfHour(timestamp) AS hour,
    connector,
    strategy_id,
    symbol,
    sumIf(income, income_type = 'REALIZED_PNL') AS realized_pnl,
    sumIf(income, income_type = 'COMMISSION') AS commission,
    sumIf(income, income_type = 'FUNDING_FEE') AS funding_fee,
    sum(income) AS total_income
FROM hft.income
GROUP BY hour, connector, strategy_id, symbol;
```

### Unified Income Types Mapping

| Binance Income Type | Bybit Transaction Type | Unified Type |
|---------------------|------------------------|--------------|
| REALIZED_PNL | RealisedPnL | REALIZED_PNL |
| COMMISSION | Fee | COMMISSION |
| FUNDING_FEE | Funding | FUNDING_FEE |
| TRANSFER | Transfer | TRANSFER |
| INSURANCE_CLEAR | Settlement | SETTLEMENT |

### New Event Types

```rust
/// Income event - realized PnL, fees, funding
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct IncomeEvent {
    pub timestamp: i64,           // Event timestamp (nanoseconds)
    pub connector: String,        // Connector name (bf, bybit)
    pub strategy_id: String,
    pub symbol: String,
    pub income_type: String,      // REALIZED_PNL, COMMISSION, FUNDING_FEE
    pub income: f64,              // Amount
    pub asset: String,            // Currency
    pub info: String,             // Additional info
    pub trade_id: u64,
}
```

## Grafana Dashboard Specs

### 1. Position Monitor Dashboard
- **Variables**: `$connector`, `$strategy`, `$symbol`, `$timerange`
- **Panels**:
  - Position History (time series)
  - Current Positions (table)
  - Unrealized PnL (gauge)
  - Position Distribution (pie chart)

### 2. PnL Tracking Dashboard
- **Panels**:
  - Cumulative PnL (time series with area fill)
  - Realized PnL by Symbol (stacked bar)
  - Fee Breakdown (pie: commission, funding)
  - Daily PnL Heatmap
  - PnL Summary (stat panels)

### 3. Trading Activity Dashboard
- **Panels**:
  - Trading Volume (bar chart, hourly)
  - Trade Count (time series)
  - Fill Rate (maker vs taker %)
  - Average Slippage (time series)

### 4. Order Latency Dashboard
- **Panels**:
  - Latency Distribution (histogram)
  - Latency Percentiles (p50, p95, p99)
  - Latency by Symbol (heatmap)
  - Latency Over Time (time series)

## Key Questions

1. Should position sync be at connector level or order-recorder level?
   - **Answer**: Connector level - it has exchange API access

2. How to handle connector restarts for income sync (avoid duplicates)?
   - **Answer**: Track last synced timestamp in local state, use idempotent inserts

3. Should Grafana run inside HFT container or separate?
   - **Answer**: Separate container in docker-compose for isolation

4. How to pass strategy/connector context to Grafana from frontend?
   - **Answer**: URL parameters for dashboard variables

## Decisions Made

- Position sync interval: 30 seconds
- Income sync interval: 60 seconds (API rate limits)
- ClickHouse TTL: 90 days for income, 30 days for positions
- Grafana auth: Anonymous access within Docker network
- Use iframe embedding over Grafana panels API (simpler)

## Files to Create/Modify

### New Files
| File | Purpose |
|------|---------|
| `order-recorder/src/income.rs` | Income event handling |
| `connector/src/common/position_sync.rs` | Position sync task |
| `connector/src/common/income_sync.rs` | Income sync task |
| `deploy/docker/grafana/` | Grafana config and provisioning |
| `deploy/docker/grafana/dashboards/*.json` | Dashboard JSON definitions |
| `server/dashboard/src/pages/Analytics.tsx` | Analytics page with Grafana embed |

### Modified Files
| File | Change |
|------|--------|
| `deploy/docker/docker-compose.yml` | Add Grafana service |
| `deploy/docker/scripts/init-clickhouse.sql` | Add income table, enhance positions |
| `order-recorder/src/events.rs` | Add IncomeEvent |
| `order-recorder/src/writer.rs` | Handle income writes |
| `connector/src/binancefutures/mod.rs` | Add position/income sync tasks |
| `connector/src/bybit/mod.rs` | Add position/income sync tasks |
| `server/dashboard/src/App.tsx` | Add Analytics route |
| `server/dashboard/src/components/layout/Sidebar.tsx` | Add Analytics nav item |

## Errors Encountered
- iceoryx2 0.6 migration required `ZeroCopySend` derive on `RecordingMessage` struct
- Bybit Position struct fields are different from Binance (e.g., `size` is f64, not String)
- Needed to update `create_order_hook()` and `OrderHookContext` to accept `connector` parameter

## Files Created/Modified

### Created
- `connector/src/sync.rs` - Position/income sync module with provider traits and implementations
- `order-recorder/src/events.rs` - Added `IncomeEvent` struct

### Modified
- `connector/Cargo.toml` - Added `recording` feature flag and order-recorder dependency
- `connector/src/lib.rs` - Added sync module
- `connector/src/binancefutures/rest.rs` - Added `get_income()` method
- `connector/src/binancefutures/msg/rest.rs` - Added `IncomeEntry` struct
- `connector/src/bybit/rest.rs` - Added `get_transaction_log()`, `get_closed_pnl()`, `get_all_positions()`
- `connector/src/bybit/msg.rs` - Added `TransactionLogEntry`, `ClosedPnlEntry` structs
- `order-recorder/Cargo.toml` - Updated iceoryx2 to 0.6.1
- `order-recorder/src/events.rs` - Added `ZeroCopySend` derive, enhanced `PositionSnapshot`
- `order-recorder/src/writer.rs` - Added `IncomeRow`, `write_income()`, `flush_incomes()`
- `order-recorder/src/service.rs` - Added Income event handling
- `order-recorder/src/hook.rs` - Added `connector` field, `publish_position_full()`
- `deploy/docker/scripts/init-clickhouse.sql` - Added `income` table, enhanced `positions` table
- `strategies/obi_mm_rust/src/testnet.rs` - Updated `create_order_hook()` call
- `server/dashboard/src/pages/Analytics.tsx` - New Analytics page with Grafana iframe embedding
- `server/dashboard/src/App.tsx` - Added Analytics route
- `server/dashboard/src/components/layout/Sidebar.tsx` - Added Analytics navigation item
- `deploy/docker/docker-compose.yml` - Added GF_SECURITY_ALLOW_EMBEDDING for iframe support
- `deploy/docker/grafana/dashboards/position-monitor.json` - Position Monitor dashboard
- `deploy/docker/grafana/dashboards/pnl-tracking.json` - PnL Tracking dashboard
- `deploy/docker/grafana/dashboards/trading-activity.json` - Trading Activity dashboard

## Status
**PARTIALLY COMPLETE** - Position tracking working, Income sync pending connector integration.

### 2026-01-10 Fix: Position Publishing
- **Issue**: Position Monitor dashboard was empty because strategy wasn't calling `publish_position()`
- **Fix**: Added position publishing every 30 seconds in `strategies/obi_mm_rust/src/testnet.rs`
- **Result**: `positions` table now receiving data

### Remaining Work: Income Sync
- **Issue**: `income` table is empty - requires connector-level sync
- **Location**: `connector/src/sync.rs` has `SyncTask` ready but not integrated into connectors
- **To integrate**: Modify `connector/src/binancefutures/mod.rs` and `connector/src/bybit/mod.rs` to:
  1. Create `SyncTask` with REST client as provider
  2. Spawn `run_income_sync()` task in connector main loop

## Access URLs
- **HFT Dashboard**: http://localhost:13000
- **HFT Analytics Page**: http://localhost:13000/analytics
- **Grafana**: http://localhost:13001 (admin/admin)
- **ClickHouse HTTP**: http://localhost:18123

### Grafana Dashboards
- **Trading Overview**: http://localhost:13001/d/hft-trading-overview/hft-trading-overview
- **Position Monitor**: http://localhost:13001/d/hft-position-monitor/hft-position-monitor
- **PnL Tracking**: http://localhost:13001/d/hft-pnl-tracking/hft-pnl-tracking
- **Trading Activity**: http://localhost:13001/d/hft-trading-activity/hft-trading-activity
