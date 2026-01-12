# Research Findings: Strategy Researcher Guide

## 1. Strategy Code Structure

### Python Strategy (strategies/obi_mm/)
- `obi_mm.py` - Main strategy logic using numba @njit
- `backtest.py` - Backtest runner
- `utils.py` - Helper classes (OrderContainer, StopLossFunc, etc.)

### Rust Strategy (strategies/obi_mm_rust/)
- `src/strategy.rs` - Core strategy implementation using factor-engine
- `src/config.rs` - Configuration structures (GlobalConfig, StrategyParams, StopLossConfig)
- `src/backtest.rs` - Backtest binary with clap CLI
- `src/testnet.rs` - Live trading binary
- `config.example.toml` - Example configuration

### Template Strategy (strategies/template_strategy/)
- Minimal template for creating new strategies

## 2. Factor Engine (factor-engine/)

### Primitives (primitives.rs)
- `RingBuffer` - Fixed-size circular buffer
- `WelfordRolling` - O(1) rolling mean/std using Welford's algorithm
- `EMA` - Exponential moving average
- `TimeWindowQueue` - Time-based sliding window

### Factors (factors/)
- `OBIFactor` - Order Book Imbalance with z-score normalization
- `VolatilityFactor` - Rolling volatility from price returns
- `StopLossChecker` - Stop loss detection based on volatility/change thresholds
- `Factor` trait - Common interface for all factors

### Key Design Patterns
- All factors are O(1) update complexity
- Use `update()` method to feed new data
- Use `value()` to get current factor value
- Use `is_ready()` to check if warmup is complete

## 3. Docker Deployment (deploy/docker/)

### Core Files
- `docker-compose.yml` - Service definitions
- `Dockerfile` - Multi-stage build
- `entrypoint.sh` - Container startup (env substitution)
- `.env` / `.env.example` - API keys

### Configuration
- `config/cluster.toml` - Main cluster config (connectors, bots, etc.)
- `config/binancefutures.toml` - Binance Futures connector config
- `config/bybit.toml` - Bybit connector config
- `config/strategies/*.toml` - Strategy configs per symbol

### Services
- **hft-supervisor** - Process manager
- **hft-server** - Web dashboard + REST API
- **connector** - Exchange connections (bf, bybit)
- **collector** - Market data recording
- **order-recorder** - Trade logging to ClickHouse

### Key Commands
```bash
# Build and start
docker compose build
docker compose up -d

# Check status
docker exec hft-system hftctl --socket /tmp/hft-supervisor.sock status

# Bot management
docker exec hft-system hftctl start <bot_id>
docker exec hft-system hftctl stop <bot_id>

# View logs
docker exec hft-system tail -f /var/lib/hft/logs/bots/<bot_id>.log
```

## 4. Symbol Naming Conventions
- **Binance**: lowercase (`btcusdt`, `ethusdt`)
- **Bybit**: UPPERCASE (`BTCUSDT`, `ETHUSDT`)

## 5. Connector Types
| Type | Exchange | Order Method |
|------|----------|--------------|
| `binancefutures` | Binance USD-M Futures | WebSocket |
| `binancespot` | Binance Spot | WebSocket |
| `bybit` | Bybit | WebSocket |
| `bybit_rest` | Bybit | REST API |

## 6. Configuration Parameters

### GlobalConfig
- `order_qty` - Order size per grid level
- `max_position` - Maximum position allowed
- `grid_num` - Number of grid levels
- `grid_interval` - Price interval between grids
- `roi_lb`, `roi_ub` - Region of Interest bounds

### StrategyParams
- `looking_depth` - OBI depth percentage (e.g., 0.001 = 0.1%)
- `window` - OBI z-score normalization window
- `volatility_window` - Volatility calculation window
- `half_spread` - Half spread as fraction of mid price
- `skew` - Position skew factor
- `c1` - Alpha coefficient for fair price
- `power` - Volatility adjustment power
- `live_seconds` - Min order live time before cancel
- `update_interval_ns` - Strategy update interval (10s default)
- `volatility_interval_ns` - Volatility calc interval (60s default)

### StopLossConfig
- `volatility_mean`, `volatility_std` - Historical volatility stats
- `change_mean`, `change_std` - Historical price change stats
- `volatility_threshold`, `change_threshold` - Z-score thresholds
