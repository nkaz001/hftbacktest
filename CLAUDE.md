# CLAUDE.md - HftBacktest Development Guide

## Project Overview

HftBacktest is a high-frequency trading (HFT) and market-making backtesting framework. It provides tick-by-tick simulation with accurate order fill modeling that accounts for:
- Feed and order latencies
- Order queue position for fill simulation
- Full order book reconstruction from L2 (Market-By-Price) feeds

The project has two implementations:
- **Python package** (`hftbacktest/`): Production-stable, uses Numba JIT compilation
- **Rust crate** (`rust/`): Experimental, supports multi-asset/multi-exchange backtesting and live trading

## Repository Structure

```
hftbacktest-sankha/
├── hftbacktest/           # Python package
│   ├── __init__.py        # Main entry point, exports public API
│   ├── backtest.py        # SingleAssetHftBacktest implementation
│   ├── marketdepth.py     # Order book reconstruction
│   ├── order.py           # Order types and OrderBus
│   ├── reader.py          # Data reader with caching
│   ├── stat.py            # Performance statistics
│   ├── state.py           # Trading state (position, balance, fees)
│   ├── typing.py          # Type definitions
│   ├── assettype.py       # Linear/Inverse asset types
│   ├── models/            # Simulation models
│   │   ├── latencies.py   # Feed and order latency models
│   │   └── queue.py       # Queue position models
│   ├── proc/              # Processing engines
│   │   ├── local.py       # Local (client-side) processor
│   │   ├── nopartialfillexchange.py  # Exchange without partial fills
│   │   └── partialfillexchange.py    # Exchange with partial fills
│   └── data/              # Data utilities
│       ├── validation.py  # Data validation
│       └── utils/         # Exchange-specific data utilities
├── rust/                  # Rust crate (experimental)
│   ├── Cargo.toml         # Rust dependencies and features
│   ├── src/
│   │   ├── lib.rs         # Crate entry point
│   │   ├── types.rs       # Core types (Order, Event, Side, etc.)
│   │   ├── prelude.rs     # Common imports
│   │   ├── backtest/      # Backtesting implementation
│   │   ├── live/          # Live trading bot
│   │   ├── connector/     # Exchange connectors (Binance Futures)
│   │   └── depth/         # Market depth implementations
│   └── examples/          # Rust examples
├── examples/              # Python examples (Jupyter notebooks)
├── tests/                 # Python unit tests
├── docs/                  # Sphinx documentation
├── setup.cfg              # Python package configuration
└── setup.py               # Python setup script
```

## Build & Development Commands

### Python

```bash
# Install package
pip install hftbacktest

# Install from source
pip install -e .

# Run tests
python -m pytest tests/

# Run a specific test
python -m pytest tests/test_marketdepth.py

# Run example
python examples/example.py
```

### Rust

```bash
# Build (from rust/ directory)
cd rust && cargo build

# Build with release optimizations
cd rust && cargo build --release

# Run tests
cd rust && cargo test

# Run example
cd rust && cargo run --example gridtrading_backtest

# Build with specific features
cd rust && cargo build --features "backtest live binancefutures"
```

### Documentation

```bash
# Build docs
cd docs && make html
```

## Key Architecture Concepts

### Data Format
Event data requires 6 columns minimum:
1. `event` - Event type flags (DEPTH_EVENT, TRADE_EVENT, etc.)
2. `exch_timestamp` - Exchange timestamp
3. `local_timestamp` - Local receipt timestamp
4. `side` - BUY (1 << 29) or SELL (1 << 28)
5. `price` - Price
6. `qty` - Quantity

Supported formats: `.npy`, `.npz`, `.pkl` (gzipped pandas)

### Event Types
```python
DEPTH_EVENT = 1           # Market depth change
TRADE_EVENT = 2           # Market trade
DEPTH_CLEAR_EVENT = 3     # Depth cleared
DEPTH_SNAPSHOT_EVENT = 4  # Full depth snapshot
```

### Order Lifecycle
- `NONE` (0): Initial state
- `NEW` (1): Order accepted
- `EXPIRED` (2): Order expired
- `FILLED` (3): Fully filled
- `CANCELED` (4): Cancelled

### Time-In-Force Options
- `GTC`: Good 'Til Canceled
- `GTX`: Post-only (maker only)
- `FOK`: Fill or Kill
- `IOC`: Immediate or Cancel

### Asset Types
- `Linear`: Standard linear contracts (e.g., BTCUSDT)
- `Inverse`: Inverse contracts (e.g., BTCUSD)

### Queue Models
Control how order queue position is estimated:
- `RiskAverseQueueModel`: Conservative, position advances only on trades
- `ProbQueueModel` variants: Probability-based position estimation
  - `LogProbQueueModel`, `SquareProbQueueModel`, `PowerProbQueueModel`

### Latency Models
- `ConstantLatency`: Fixed latency value
- `FeedLatency`: Models feed/order latency relationship
- `IntpOrderLatency`: Interpolated from historical latency data

## Code Patterns and Conventions

### Python

1. **Numba JIT Compilation**: Core backtesting code uses `@njit` decorator for performance
   ```python
   from numba import njit

   @njit
   def trading_algo(hbt):
       while hbt.elapse(100_000):  # 100ms in nanoseconds
           # Trading logic
           pass
   ```

2. **JIT Classes**: Models use `jitclass` for JIT compatibility
   ```python
   from numba.experimental import jitclass

   @jitclass(spec=[('n', float64)])
   class PowerProbQueueModel:
       pass
   ```

3. **Type Hints**: Use `typing.py` for custom type aliases

### Rust

1. **Feature Flags**: Use cargo features for optional functionality
   ```rust
   #[cfg(feature = "backtest")]
   pub mod backtest;

   #[cfg(feature = "live")]
   pub mod live;
   ```

2. **Builder Pattern**: Asset and backtest configuration uses builders
   ```rust
   let hbt = MultiAssetMultiExchangeBacktest::builder()
       .add(AssetBuilder::new()
           .latency_model(latency_model)
           .queue_model(queue_model)
           .build()?)
       .build()?;
   ```

3. **Trait-Based Design**: Core functionality defined via traits
   - `Interface<Q, MD>`: Main backtester/bot interface
   - `MarketDepth`: Order book implementations
   - `QueueModel`: Queue position models
   - `LatencyModel`: Latency simulation

4. **Error Handling**: Use `thiserror` for error types
   ```rust
   #[derive(Error, Debug)]
   pub enum BacktestError {
       #[error("Order not found")]
       OrderNotFound,
   }
   ```

## Testing Guidelines

### Python Tests
- Located in `tests/`
- Use `unittest.TestCase` pattern
- Test market depth operations, reader functionality

### Rust Tests
- Inline tests with `#[cfg(test)]` modules
- Use `tracing_subscriber` for debug logging in examples

## Important Notes for AI Assistants

1. **Nanosecond Timestamps**: All timestamps are in nanoseconds by default

2. **Tick-Based Pricing**: Prices are often handled in "ticks" (price / tick_size) for precision

3. **Dual-Processor Model**: Backtesting uses separate local and exchange processors that communicate via `OrderBus`

4. **Data Caching**: `Cache` and `DataReader` classes handle efficient data loading and caching

5. **No Partial Fills by Default**: `NoPartialFillExchange` is the default; use `PartialFillExchange` if needed

6. **Rust Experimental Features**: The Rust implementation supports features not in Python:
   - Multi-asset backtesting
   - Multi-exchange backtesting
   - Live trading with same algo code
   - Binance Futures connector

7. **Performance Considerations**:
   - Python: Numba JIT compilation can have overhead on first run
   - Rust: Use `--release` profile for production performance

## GitHub Workflows

- **CodeQL**: Security analysis on push/PR to master, runs weekly
  - Analyzes Python code for security vulnerabilities

## Dependencies

### Python (from setup.cfg)
- Python >= 3.10
- numba ~= 0.59
- numpy >= 1.22, < 1.27
- pandas
- matplotlib

### Rust (from Cargo.toml)
Core:
- tracing, anyhow, thiserror

Optional (by feature):
- `backtest`: zip
- `live`: chrono, tokio, futures-util
- `binancefutures`: serde, tokio-tungstenite, reqwest, hmac, sha2

## Common Development Tasks

### Adding a New Queue Model (Python)
1. Inherit from `ProbQueueModel` in `hftbacktest/models/queue.py`
2. Implement `f(x)` method for probability calculation
3. Export via `__init__.py`

### Adding a New Latency Model (Python)
1. Add class to `hftbacktest/models/latencies.py`
2. Implement `entry_latency()` and `response_latency()` methods
3. Use `jitclass` decorator for JIT compatibility

### Adding Exchange Connector (Rust)
1. Create module under `rust/src/connector/`
2. Implement message parsing, WebSocket handling, REST API
3. Add feature flag to `Cargo.toml`
4. Connect via `live::Bot`

## License

MIT License
