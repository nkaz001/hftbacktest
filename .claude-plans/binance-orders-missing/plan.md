# Task: Investigate Missing Binance Orders After 10 AM

## Goal
Identify why Binance orders stopped after 10 AM and fix the issue.

## Current Phase
Complete

## Phases

### Phase 1: Log Investigation
- [x] Check bot logs for errors around 10 AM
- [x] Check connector logs for connection issues
- [x] Check order recorder logs
- **Status:** complete

### Phase 2: Root Cause Analysis
- [x] Identify the specific error or issue
- [x] Understand why orders stopped
- **Status:** complete

### Phase 3: Fix Implementation
- [x] Restart the connector to restore WebSocket connection
- [x] Verify order book sync
- [x] Fix root cause: add symbol re-subscription on reconnect
- **Status:** complete

### Phase 4: Verification
- [x] Confirm orders are flowing again
- [x] Prices updating (90712.40 → 90716.15)
- **Status:** complete

### Phase 5: Fix Other Connectors
- [x] binancespot - connect_market_data_stream
- [x] binancepapi - connect_market_data_stream
- [x] bybit - connect_public_stream
- [x] bybit - connect_private_stream
- **Status:** complete (commit 6c85760)

## Root Cause Analysis

**Problem:** The market data WebSocket stream was interrupted at `2026-01-11T01:34:41 UTC` (09:34 Beijing time), and the connector failed to properly re-synchronize the order book after reconnecting. As a result, the order book data is stale (price stuck at 90508.40).

**Key Log Events:**
1. `2026-01-11T01:30:02` - User data stream connection interrupted
2. `2026-01-11T01:34:41` - Market data stream connection interrupted
3. `2026-01-11T02:30:06` - `listenKeyExpired` error for user data stream

### Bug Found: Symbol Re-subscription Missing on Reconnect

When the WebSocket reconnects, the code creates a NEW `MarketDataStream` object (line 136-140 in mod.rs):
```rust
.retry(|| async {
    let mut stream = market_data_stream::MarketDataStream::new(
        client.clone(),
        ev_tx.clone(),
        symbol_tx.subscribe(),  // NEW receiver
    );
    ...
})
```

**The Issue:**
1. A new `MarketDataStream` with a fresh (empty) `OrderBookManager` is created
2. The `symbol_rx` channel only receives **new** subscription requests via `symbol_tx.subscribe()`
3. Existing symbols (already in `self.symbols` HashSet) are never re-broadcast after reconnection
4. The WebSocket connects but never subscribes to any symbols -> no depth data flows

**Solution Required:**
Modify `connect_market_data_stream` to re-send all registered symbols to the new receiver after reconnection. This can be done by:
1. Passing `self.symbols` (SharedSymbolSet) to the retry closure
2. After creating the new MarketDataStream, send all existing symbols to `symbol_tx`

**Location:** `connector/src/binancefutures/mod.rs:115-148`

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Restart connector | WebSocket connection needs to be re-established to sync order book |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| ConnectionInterrupted | WebSocket auto-reconnect failed to re-sync order book | Need to restart connector |

## Files Examined
- /var/lib/hft/logs/bots/obi_mm_btcusdt_bf.log
- /var/lib/hft/logs/infra/connector_bf.log
