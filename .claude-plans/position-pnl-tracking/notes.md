# Notes: Exchange API Research

## Binance Futures APIs

### Position Information
```
GET /fapi/v2/positionRisk
```
Response:
```json
{
  "symbol": "BTCUSDT",
  "positionAmt": "0.001",
  "entryPrice": "50000.0",
  "markPrice": "51000.0",
  "unRealizedProfit": "1.0",
  "liquidationPrice": "40000.0",
  "leverage": "10",
  "maxNotionalValue": "1000000",
  "marginType": "cross",
  "isolatedMargin": "0.0",
  "isAutoAddMargin": "false",
  "positionSide": "BOTH",
  "notional": "51.0",
  "isolatedWallet": "0",
  "updateTime": 1234567890
}
```

### Income History (Key for PnL tracking)
```
GET /fapi/v1/income
```
Parameters:
- `symbol` (optional)
- `incomeType` (optional): REALIZED_PNL, COMMISSION, FUNDING_FEE, TRANSFER, etc.
- `startTime`, `endTime` (optional)
- `limit` (default 100, max 1000)

Response:
```json
[
  {
    "symbol": "BTCUSDT",
    "incomeType": "REALIZED_PNL",
    "income": "1.23456789",
    "asset": "USDT",
    "info": "trade_id:123456789",
    "time": 1234567890000,
    "tranId": 987654321,
    "tradeId": "123456789"
  }
]
```

Income Types:
- `TRANSFER` - Transfer in/out
- `WELCOME_BONUS` - Welcome bonus
- `REALIZED_PNL` - Realized profit/loss
- `FUNDING_FEE` - Funding fee
- `COMMISSION` - Trading commission
- `INSURANCE_CLEAR` - Insurance fund clear
- `REFERRAL_KICKBACK` - Referral kickback
- `COMMISSION_REBATE` - Commission rebate
- `API_REBATE` - API rebate
- `CONTEST_REWARD` - Contest reward
- `CROSS_COLLATERAL_TRANSFER` - Cross collateral transfer
- `OPTIONS_PREMIUM_FEE` - Options premium fee
- `OPTIONS_SETTLE_PROFIT` - Options settle profit
- `INTERNAL_TRANSFER` - Internal transfer
- `AUTO_EXCHANGE` - Auto exchange
- `DELIVERED_SETTELMENT` - Delivery settlement
- `COIN_SWAP_DEPOSIT` - Coin swap deposit
- `COIN_SWAP_WITHDRAW` - Coin swap withdraw

### User Data Stream Events

Position Update (in ACCOUNT_UPDATE):
```json
{
  "e": "ACCOUNT_UPDATE",
  "T": 1234567890,
  "E": 1234567891,
  "a": {
    "B": [{"a": "USDT", "wb": "100", "cw": "100"}],
    "P": [
      {
        "s": "BTCUSDT",
        "pa": "0.001",
        "ep": "50000.0",
        "up": "1.0",
        "mt": "cross",
        "iw": "0",
        "ps": "BOTH"
      }
    ]
  }
}
```

---

## Bybit APIs

### Position List
```
GET /v5/position/list
```
Parameters:
- `category`: linear, inverse
- `symbol` (optional)
- `settleCoin` (optional)

Response:
```json
{
  "result": {
    "list": [
      {
        "symbol": "BTCUSDT",
        "leverage": "10",
        "avgPrice": "50000.0",
        "liqPrice": "40000.0",
        "markPrice": "51000.0",
        "positionValue": "51.0",
        "side": "Buy",
        "size": "0.001",
        "unrealisedPnl": "1.0",
        "curRealisedPnl": "0.5",
        "positionIdx": 0,
        "updatedTime": "1234567890000"
      }
    ]
  }
}
```

### Transaction Log (for PnL tracking)
```
GET /v5/account/transaction-log
```
Parameters:
- `accountType`: UNIFIED (required for unified account)
- `category`: linear (optional)
- `baseCoin`, `symbol` (optional)
- `type` (optional): TRADE, SETTLEMENT, DELIVERY, LIQUIDATION, etc.
- `startTime`, `endTime` (optional)
- `limit` (default 20, max 50)

Response:
```json
{
  "result": {
    "list": [
      {
        "symbol": "BTCUSDT",
        "side": "Buy",
        "funding": "0",
        "orderLinkId": "",
        "orderId": "123456",
        "fee": "-0.01",
        "change": "1.23",
        "cashFlow": "0",
        "transactionTime": "1234567890000",
        "type": "TRADE",
        "feeRate": "0.0001",
        "tradePrice": "50000.0",
        "qty": "0.001",
        "cashBalance": "100.0",
        "category": "linear"
      }
    ]
  }
}
```

Transaction Types:
- `TRADE` - Trade P&L and fee
- `SETTLEMENT` - Settlement (for inverse)
- `DELIVERY` - Delivery (for options)
- `LIQUIDATION` - Liquidation
- `BONUS` - Bonus
- `FEE_REFUND` - Fee refund
- `INTEREST` - Interest
- `CURRENCY_BUY` - Currency conversion buy
- `CURRENCY_SELL` - Currency conversion sell

### Closed PnL
```
GET /v5/position/closed-pnl
```
More detailed realized PnL per trade:
```json
{
  "result": {
    "list": [
      {
        "symbol": "BTCUSDT",
        "orderId": "123456",
        "side": "Sell",
        "qty": "0.001",
        "orderPrice": "51000.0",
        "orderType": "Market",
        "execType": "Trade",
        "closedSize": "0.001",
        "cumEntryValue": "50.0",
        "avgEntryPrice": "50000.0",
        "cumExitValue": "51.0",
        "avgExitPrice": "51000.0",
        "closedPnl": "1.0",
        "fillCount": "1",
        "leverage": "10",
        "createdTime": "1234567890000",
        "updatedTime": "1234567891000"
      }
    ]
  }
}
```

### Private WebSocket Events

Position Update:
```json
{
  "topic": "position",
  "data": [
    {
      "symbol": "BTCUSDT",
      "side": "Buy",
      "size": "0.001",
      "avgPrice": "50000.0",
      "positionValue": "50.0",
      "unrealisedPnl": "1.0",
      "markPrice": "51000.0",
      "liqPrice": "40000.0"
    }
  ]
}
```

---

## Unified Data Mapping

### Income Type Mapping

| Binance | Bybit | Unified |
|---------|-------|---------|
| REALIZED_PNL | RealisedPnl (from closed-pnl) | REALIZED_PNL |
| COMMISSION | Fee (from transaction-log) | COMMISSION |
| FUNDING_FEE | Funding (from execution) | FUNDING_FEE |
| TRANSFER | Transfer | TRANSFER |
| INSURANCE_CLEAR | Settlement | SETTLEMENT |
| REFERRAL_KICKBACK | BONUS | BONUS |

### Position Fields Mapping

| Unified Field | Binance | Bybit |
|---------------|---------|-------|
| symbol | symbol | symbol |
| position | positionAmt | size (with side) |
| entry_price | entryPrice | avgPrice |
| mark_price | markPrice | markPrice |
| unrealized_pnl | unRealizedProfit | unrealisedPnl |
| liquidation_price | liquidationPrice | liqPrice |
| leverage | leverage | leverage |
| margin_type | marginType | isIsolated ? "isolated" : "cross" |
| update_time | updateTime | updatedTime |

---

## Rate Limits

### Binance Futures
- `/fapi/v1/income`: 30 requests/minute
- `/fapi/v2/positionRisk`: 5 requests/minute (with limit)

Recommended sync intervals:
- Position: 30 seconds
- Income: 60 seconds (batch by time range)

### Bybit
- `/v5/position/list`: 10 requests/second
- `/v5/account/transaction-log`: 10 requests/second
- `/v5/position/closed-pnl`: 10 requests/second

More generous limits, but still recommend:
- Position: 30 seconds
- Income: 60 seconds (to match Binance)

---

## Implementation Notes

### Deduplication Strategy
1. Track `last_income_time` per connector
2. Query income with `startTime = last_income_time + 1`
3. Use `tranId` (Binance) / `id` (Bybit) as unique key
4. ClickHouse ReplacingMergeTree could help but INSERT is idempotent if we use same keys

### Error Handling
- Network failures: Exponential backoff retry
- API errors: Log and skip, don't crash
- Rate limit: Respect `Retry-After` header

### Memory Considerations
- Income history could be large
- Use cursor/pagination for initial sync
- For ongoing sync, just get last 60 seconds of data
