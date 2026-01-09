# Notes: reduceOnly Parameter Research

## Current Order Flow

### IPC Message Structure
- `hftbacktest/src/types.rs` - `Order` struct (lines 489-525)
- Fields: qty, leaves_qty, exec_qty, price_tick, tick_size, side, time_in_force, order_type, status, req
- **Missing: `reduce_only` field**

### Bot Submission
- `hftbacktest/src/live/bot.rs` - `submit_order` method (lines 360-420)
- Creates Order struct and sends via IPC
- Currently no `reduce_only` support

## Connector-Specific Details

### Binance Futures
- **REST Client**: `connector/src/binancefutures/rest.rs:161-201`
  - `submit_order()` constructs body with: newClientOrderId, symbol, side, price, quantity, type, timeInForce
  - **Missing: reduceOnly parameter**
- **Connector**: `connector/src/binancefutures/mod.rs:269`
  - Calls `client.submit_order()` with order params
- **API Support**: Binance Futures API supports `reduceOnly` boolean param

### Binance PAPI
- **REST Client**: `connector/src/binancepapi/rest.rs:159-194`
  - `submit_um_order()` - similar to Futures
  - **Missing: reduceOnly parameter**
- **Connector**: `connector/src/binancepapi/mod.rs:262-334`
- **API Support**: Binance Portfolio Margin API supports `reduceOnly`

### Bybit
- **Order Struct**: `connector/src/bybit/msg.rs:559-577`
  - `BybitOrder` struct - **Missing: reduce_only field**
- **Order Manager**: `connector/src/bybit/ordermanager.rs:95-159`
  - `new_order()` creates BybitOrder from IPC Order
- **REST Client**: `connector/src/bybit/rest.rs:131-145`
  - Serializes BybitOrder directly to JSON
- **Response Struct**: `connector/src/bybit/msg.rs:454-455`
  - `PrivateOrder` **already has reduce_only field** (receives from API)
  - Shows Bybit API expects/processes reduceOnly
- **API Support**: Bybit supports `reduceOnly` boolean param

## Key Finding
Bybit's response parsing already handles `reduce_only`, but outgoing orders don't include it.
This is an inconsistency that needs fixing.

## Exchange API References

### Binance Futures
```
POST /fapi/v1/order
Parameter: reduceOnly (BOOLEAN, optional)
- true: order will only reduce position
- Useful for closing positions with notional < $100
```

### Bybit
```
POST /v5/order/create
Parameter: reduceOnly (boolean, optional)
- true: reduce-only order
- Required for closing positions in some scenarios
```

## Error That Triggered This

```
Order submission failed: OrderError {
  code: -4164,
  msg: "Order's notional must be no smaller than 100 (unless you choose reduce only)."
}
```

Stop loss order: `90450 × 0.001 = $90.45` < $100 minimum
Solution: Add `reduceOnly=true` to bypass minimum notional for position-closing orders
