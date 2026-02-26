# HFTBacktest Testnet Setup Guide

This guide provides a complete setup for running HFTBacktest on exchange testnets.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Testnet Registration](#testnet-registration)
3. [Configuration](#configuration)
4. [Architecture Overview](#architecture-overview)
5. [Quick Start](#quick-start)
6. [Data Recording](#data-recording)
7. [Running Strategies](#running-strategies)
8. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### System Requirements
- Linux (Ubuntu 20.04+ recommended)
- Rust 1.91.1+
- 4GB+ RAM

### Build the Project
```bash
cd /path/to/hftbacktest

# Build all components
cargo build --release

# Verify builds
cargo build --release --package connector
cargo build --release --package collector
cargo build --release --example testnet_simple_mm
```

---

## Testnet Registration

### Binance Futures Testnet
1. Visit: https://testnet.binancefuture.com/
2. Register/Login with email
3. Go to API Management
4. Create new API key
5. Save API Key and Secret

### Binance Spot Testnet
1. Visit: https://testnet.binance.vision/
2. Login with GitHub account
3. Generate API keys
4. Save API Key and Secret

### Bybit Testnet
1. Visit: https://testnet.bybit.com/
2. Register/Login
3. Go to API Management
4. Create new API key (Read-Write permissions)
5. Save API Key and Secret

---

## Configuration

### 1. Edit Configuration Files

Copy and edit the config files with your API keys:

```bash
cd examples/testnet/config

# For Binance Futures
cp binancefutures_testnet.toml my_binancefutures.toml
nano my_binancefutures.toml
```

Example configuration:
```toml
stream_url = "wss://fstream.binancefuture.com/ws"
api_url = "https://demo-fapi.binance.com"

order_prefix = "test"
api_key = "your_api_key_here"
secret = "your_secret_here"
```

### 2. Make Scripts Executable

```bash
chmod +x examples/testnet/scripts/*.sh
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Exchange (Testnet)                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │   REST API      │  │  Market Data WS │  │  User Data WS   │  │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘  │
└───────────┼────────────────────┼────────────────────┼───────────┘
            │                    │                    │
            └────────────────────┼────────────────────┘
                                 │
┌────────────────────────────────┼────────────────────────────────┐
│                           Connector                              │
│  ┌─────────────────────────────┴─────────────────────────────┐  │
│  │  • Order Management (submit, cancel, modify)               │  │
│  │  • Position Tracking                                       │  │
│  │  • Market Data Processing                                  │  │
│  │  • Order Book Synchronization                              │  │
│  └─────────────────────────────┬─────────────────────────────┘  │
└────────────────────────────────┼────────────────────────────────┘
                                 │
                          IPC (iceoryx2)
                                 │
         ┌───────────────────────┼───────────────────────┐
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
│   Trading Bot   │   │    Collector    │   │   Other Bots    │
│  (Strategy)     │   │  (Data Record)  │   │                 │
└─────────────────┘   └─────────────────┘   └─────────────────┘
```

### Component Roles

| Component | Role |
|-----------|------|
| **Connector** | Bridges exchange APIs with local processes via IPC |
| **Collector** | Records market data for backtesting |
| **Trading Bot** | Executes trading strategy |

---

## Quick Start

### Terminal Layout (Recommended)

Use tmux or multiple terminal windows:

```
┌──────────────────────┬──────────────────────┐
│   Terminal 1         │   Terminal 2         │
│   (Connector)        │   (Strategy)         │
├──────────────────────┼──────────────────────┤
│   Terminal 3         │   Terminal 4         │
│   (Collector)        │   (Monitoring)       │
└──────────────────────┴──────────────────────┘
```

### Step 1: Start Connector

```bash
# Terminal 1
cd /path/to/hftbacktest

# Start Binance Futures connector with name "bf"
./examples/testnet/scripts/start_connector.sh bf binancefutures \
    examples/testnet/config/binancefutures_testnet.toml

# Or manually:
RUST_LOG=info cargo run --release --package connector -- \
    bf binancefutures examples/testnet/config/binancefutures_testnet.toml
```

Expected output:
```
INFO connector: Connected to market data stream
INFO connector: Connected to user data stream
INFO connector: Listening for bot connections...
```

### Step 2: Start Data Collector (Optional)

```bash
# Terminal 3 - IPC mode (recommended, uses same data as bot)
./examples/testnet/scripts/start_collector_ipc.sh bf ./data btcusdt

# Or direct exchange connection (independent data stream)
./examples/testnet/scripts/start_collector.sh binancefutures ./data btcusdt ethusdt
```

### Step 3: Run Trading Strategy

```bash
# Terminal 2
cd /path/to/hftbacktest

# Run the simple market making strategy
RUST_LOG=info cargo run --release --example testnet_simple_mm
```

Expected output:
```
======================================
 Simple Market Making Strategy
======================================
Connector: bf
Symbol: btcusdt
Half Spread: 0.05%
Grid Num: 5
Order Qty: 0.001
Max Position: 0.01
======================================

Strategy started. Waiting for market data...
[    10] Mid: 97234.50 | Bid: 97234.40 | Ask: 97234.60 | Pos: 0.0000
[    20] Mid: 97235.10 | Bid: 97235.00 | Ask: 97235.20 | Pos: 0.0010
```

---

## Data Recording

### Recording Modes

#### 1. Direct Exchange Connection
Records data directly from exchange WebSocket:
```bash
./examples/testnet/scripts/start_collector.sh binancefutures ./data btcusdt
```

#### 2. IPC Mode (Recommended)
Records same data that bot receives via connector:
```bash
# Start connector first, then:
./examples/testnet/scripts/start_collector_ipc.sh bf ./data btcusdt
```

### Data Output Format

Collected data is saved as gzip-compressed JSON:
```
./data/
├── btcusdt_20241225_143022.gz
├── btcusdt_20241225_150000.gz
└── ...
```

### Converting Data for Backtesting

Convert raw data to NPZ format:
```bash
./examples/testnet/scripts/convert_data.sh ./data/btcusdt_20241225_143022.gz ./data/btcusdt.npz

# Or manually:
cargo run --release --example convert_collector_data -- \
    ./data/btcusdt_20241225_143022.gz \
    ./data/btcusdt.npz
```

---

## Running Strategies

### Strategy Configuration

Edit the strategy source code to configure:

```rust
// examples/testnet/src/simple_mm.rs
let config = Config {
    connector_name: "bf",           // Must match connector name
    symbol: "btcusdt",              // Trading pair (lowercase)
    tick_size: 0.1,                 // Price precision
    lot_size: 0.001,                // Quantity precision
    half_spread: 0.0005,            // 0.05% half spread
    grid_interval: 0.0002,          // 0.02% grid interval
    grid_num: 5,                    // Orders per side
    order_qty: 0.001,               // Order quantity
    max_position: 0.01,             // Max position
};
```

### Common Symbols and Specs

| Exchange | Symbol | Tick Size | Lot Size |
|----------|--------|-----------|----------|
| Binance Futures | BTCUSDT | 0.1 | 0.001 |
| Binance Futures | ETHUSDT | 0.01 | 0.001 |
| Binance Spot | BTCUSDT | 0.01 | 0.00001 |
| Bybit | BTCUSDT | 0.1 | 0.001 |

### Multiple Symbols

To trade multiple symbols, register multiple instruments:

```rust
let mut hbt = LiveBotBuilder::new()
    .register(Instrument::new(
        "bf",
        "btcusdt",
        0.1, 0.001,
        HashMapMarketDepth::new(0.1, 0.001),
        0,
    ))
    .register(Instrument::new(
        "bf",
        "ethusdt",
        0.01, 0.001,
        HashMapMarketDepth::new(0.01, 0.001),
        1,
    ))
    .build()
    .unwrap();
```

---

## Troubleshooting

### Common Issues

#### 1. "Connection refused" when starting bot

**Cause**: Connector not running or wrong name

**Solution**:
```bash
# Verify connector is running
ps aux | grep connector

# Ensure connector name matches
# Connector: --name bf
# Bot config: connector_name: "bf"
```

#### 2. "API key invalid" error

**Cause**: Wrong testnet URL or incorrect API keys

**Solution**:
- Verify testnet URL in config (not mainnet!)
- Re-generate API keys from testnet website
- Check for extra whitespace in config file

#### 3. "Market depth is incomplete"

**Cause**: Symbol not subscribed or waiting for initial data

**Solution**:
- Wait a few seconds for data to arrive
- Verify symbol is lowercase: `btcusdt` not `BTCUSDT`
- Check connector logs for subscription confirmation

#### 4. Orders not being placed

**Cause**: Insufficient testnet balance or invalid order parameters

**Solution**:
- Get free testnet funds from faucet
- Check minimum order size requirements
- Verify tick_size and lot_size are correct

#### 5. IPC connection fails

**Cause**: iceoryx2 shared memory issue

**Solution**:
```bash
# Clean up shared memory
rm -rf /dev/shm/iox2_*

# Restart connector and bot
```

### Logging

Enable detailed logging:
```bash
# Trace level (very verbose)
RUST_LOG=trace cargo run --release --package connector -- ...

# Debug level
RUST_LOG=debug cargo run --release --example testnet_simple_mm

# Specific module
RUST_LOG=connector=debug,hftbacktest=info cargo run ...
```

### Health Check Commands

```bash
# Check connector process
ps aux | grep connector

# Check shared memory
ls -la /dev/shm/iox2_*

# Check network connections
ss -tuln | grep ESTABLISHED

# Monitor CPU/memory
top -p $(pgrep -f connector)
```

---

## File Structure

```
examples/testnet/
├── README.md                           # This guide
├── config/
│   ├── binancefutures_testnet.toml    # Binance Futures config
│   ├── binancespot_testnet.toml       # Binance Spot config
│   └── bybit_testnet.toml             # Bybit config
├── scripts/
│   ├── start_connector.sh             # Start connector
│   ├── start_collector.sh             # Start collector (direct)
│   ├── start_collector_ipc.sh         # Start collector (IPC)
│   └── convert_data.sh                # Convert recorded data
├── src/
│   └── simple_mm.rs                   # Simple MM strategy
└── data/                              # Recorded data (generated)
```

---

## OBI MM Strategy

The OBI (Order Book Imbalance) Market Making strategy uses order book imbalance as an alpha signal to adjust fair price, with volatility-based spread adjustment and stop loss protection.

### Running OBI MM Strategy

```bash
# Terminal 1: Start connector (same as before)
./examples/testnet/scripts/start_connector.sh bf binancefutures \
    examples/testnet/config/binancefutures_testnet.toml

# Terminal 2: Start OBI MM strategy
./examples/testnet/scripts/start_obi_mm.sh --connector bf --symbol btcusdt

# Or with dry-run mode (no actual orders)
./examples/testnet/scripts/start_obi_mm.sh --dry-run

# Or manually with cargo:
RUST_LOG=info cargo run --release -p obi-mm --bin obi_mm_live --features live -- \
    --connector bf --symbol btcusdt
```

### OBI MM Configuration

Create a custom config file (optional):

```toml
# obi_mm_config.toml
[global]
order_qty = 0.002        # Order size (~$180 at BTC $90k)
max_position = 0.02      # Maximum position
grid_num = 3             # Orders per side
grid_interval = 1.0      # Grid spacing in dollars
roi_lb = 0.0
roi_ub = 200000.0

[params]
looking_depth = 0.001    # 0.1% depth for OBI calculation
window = 360             # Z-score window (shorter for testnet)
half_spread = 0.0005     # 0.05% half spread
skew = 0.01              # Position-based skew
c1 = 0.0001              # Alpha coefficient
power = 1.0              # Volatility power
live_seconds = 30.0      # Order lifetime
update_interval_ns = 10000000000     # 10 seconds
volatility_interval_ns = 60000000000 # 60 seconds

[stop_loss]
volatility_mean = 0.005
volatility_std = 0.002
change_mean = 0.001
change_std = 0.003
volatility_threshold = 3.0
change_threshold = 3.0
```

Run with custom config:
```bash
./examples/testnet/scripts/start_obi_mm.sh --config ./obi_mm_config.toml
```

### OBI MM Features

- **Order Book Imbalance**: Uses depth imbalance to predict short-term price direction
- **Z-Score Normalization**: Normalizes OBI signal using rolling statistics
- **Volatility Adjustment**: Widens spreads during high volatility
- **Position Skew**: Adjusts quotes based on current position
- **Stop Loss**: Automatically closes position during extreme market moves

---

## Next Steps

1. **Customize Strategy**: Modify `simple_mm.rs` or use OBI MM for more sophisticated trading
2. **Backtest**: Use converted data to backtest strategies
3. **Monitor**: Add logging and monitoring for production
4. **Production**: Switch to mainnet URLs when ready

For more examples, see:
- `hftbacktest/examples/gridtrading_live.rs`
- `hftbacktest/examples/algo.rs`
- `strategies/obi_mm_rust/` - OBI MM strategy implementation
