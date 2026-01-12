# Task: Fix Connector Errors (Excluding -5022)

## Goal
Fix parsing and network errors in Binance Futures and Bybit connectors identified from logs.

## Current Phase
Complete

## Errors to Fix

| # | Connector | Error | Priority |
|---|-----------|-------|----------|
| 1 | bf | `IncompleteMessage` - HTTP response incomplete | Low |
| 2 | bybit | `missing field orderId` - cancel order response parsing | Medium |
| 3 | bybit | `null instead of string` - income sync parsing | Medium |

## Phases

### Phase 1: Investigation
- [x] Investigate IncompleteMessage error (network issue, may not need code fix)
- [x] Find Bybit order cancellation response parsing code
- [x] Find Bybit income/transaction log parsing code
- **Status:** complete

**Findings:**
1. IncompleteMessage: Network issue, no code fix needed
2. `OrderResponseData` in msg.rs:521 - `order_id: String` is required but Bybit may not return it on cancel
3. `TransactionLogEntry` in msg.rs:608 - `symbol: String` lacks `#[serde(default)]`, Bybit returns null

### Phase 2: Fix Bybit orderId parsing
- [x] Identify the struct expecting orderId
- [x] Make orderId optional or handle missing field
- **Status:** complete (added `#[serde(default)]` to `OrderResponseData.order_id`)

### Phase 3: Fix Bybit income sync parsing
- [x] Identify the struct with string field receiving null
- [x] Make field optional or handle null
- **Status:** complete (added `#[serde(default)]` to `TransactionLogEntry.symbol`)

### Phase 4: Build & Deploy
- [x] Build connector
- [x] Deploy to Docker
- [x] Verify errors resolved
- **Status:** complete (commit 65e3c58)

## Decisions Made
| Decision | Rationale |
|----------|-----------|

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|

## Files to Examine
- connector/src/bybit/rest.rs - REST client for order operations
- connector/src/bybit/msg.rs - Message parsing structs
- connector/src/sync/recording.rs - Income sync logic
