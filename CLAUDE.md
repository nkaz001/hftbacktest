# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

HftBacktest is a high-frequency trading and market-making backtesting framework. The core is written in Rust with Python bindings via PyO3/Maturin. It focuses on accurate simulation by accounting for feed/order latencies and order queue positions using full order book tick data.

## Build Commands

### Rust (主仓库)
```bash
# Build all workspace members (核心框架 + 实盘组件)
cargo build

# Build with release optimizations
cargo build --release

# Run tests
cargo test

# Build specific packages
cargo build --release -p connector
cargo build --release -p collector
cargo build --release -p order-recorder
```

### Rust (策略, 在 submodules/hft-strategies/ 下)
```bash
# 策略是独立 workspace，需要 cd 进去编译
cd submodules/hft-strategies

# 回测
cargo build --release -p obi-mm

# 实盘 (需要 live feature)
cargo build --release -p obi-mm --features live

# path override 由 .cargo/config.toml 自动处理，无需手动配置
```

### Python
```bash
# Install in development mode (requires maturin)
cd py-hftbacktest
maturin develop --release

# Run Python tests
pytest py-hftbacktest/tests/

# Run Python strategy
python submodules/hft-strategies/gridtrading.py
```

### Running Live Trading
```bash
# 1. Start connector (must run on same machine as bot)
./target/release/connector --name bf --connector binancefutures --config config.toml

# 2. Run strategy (built from submodules/hft-strategies/)
./submodules/hft-strategies/target/release/obi_mm_live --config config.toml
```

## Architecture

### Multi-Repo Structure

The project is split across multiple private repos with git submodules:

```
bookdisco/hftbacktest          ← 主仓库 (本仓库: 核心框架 + 实盘组件)
bookdisco/factor-engine        ← 独立仓库 (因子计算引擎)
bookdisco/hft-strategies       ← 独立仓库 (交易策略: Rust + Python)
bookdisco/hft-configs          ← 独立仓库 (策略参数 TOML)
```

**Git submodules** (在 `submodules/` 下，用于本地开发):
```
submodules/
├── factor-engine/       → git@github.com:bookdisco/factor-engine.git
└── hft-strategies/      → git@github.com:bookdisco/hft-strategies.git
```

**克隆时需要初始化 submodules:**
```bash
git clone --recurse-submodules git@github.com:bookdisco/hftbacktest.git
# 或已克隆后:
git submodule update --init
```

### Workspace Structure (本仓库)
- **hftbacktest/**: Core Rust library - backtesting engine, live trading bot, market depth implementations
- **hftbacktest-derive/**: Procedural macros (NpyDTyped derive)
- **py-hftbacktest/**: Python bindings (PyO3) and pure Python utilities
- **collector/**: Standalone binary for collecting market data from exchanges (Binance, Bybit, Hyperliquid)
- **connector/**: Standalone binary for live trading connections (Binance Futures/Spot/PAPI, Bybit)
- **order-recorder/**: Order recording service for live trading analytics (writes to ClickHouse)
- **supervisor/**: Process supervisor for managing connectors, collectors, and trading bots (hft-supervisor + hftctl)
- **server/**: HTTP API server for monitoring and control (dashboard backend)
- **deploy/**: Deployment configurations (Docker, systemd)

### External Repos (git submodules)
- **submodules/factor-engine/**: Streaming O(1) factor computation library (OBI, volatility, trade imbalance) — `bookdisco/factor-engine`
- **submodules/hft-strategies/**: Trading strategies — `bookdisco/hft-strategies`
  - `obi_mm_rust/`: Rust OBI market making strategy (backtest + live)
  - `template_strategy/`: Template for new strategies
  - `obi_mm/`: Python OBI strategy
  - `gridtrading.py`: Python grid trading example

### Core Concepts

**Dual-Mode Architecture**: Same algorithm code runs in both backtesting and live trading via the `Bot` trait, which abstracts order submission, position tracking, and market depth access.

**Local/Exchange Processors**: Backtesting simulates two sides - `Local` (your trading system) and `Exchange` (the remote exchange). Orders flow between them with configurable latency models.

**Market Depth Implementations**:
- `HashMapMarketDepth`: General purpose, handles any price levels
- `ROIVectorMarketDepth`: Optimized for instruments with bounded price ranges
- `BTreeMarketDepth`: For L3 (Market-By-Order) data

**Data Format**: Uses numpy-compatible `.npz` files with `Event` struct (64 bytes: ev flags, timestamps, price, qty, order_id, etc.). The `EXCH_EVENT`/`LOCAL_EVENT` flags indicate which processor sees each event.

**IPC for Live Trading**: Bot and connector processes communicate via iceoryx2 shared memory (zero-copy). The connector handles exchange WebSocket/REST; the bot runs the strategy. Both must run on the same machine.

### Feature Flags (Rust)
- `backtest` (default): Backtesting functionality
- `live` (default): Live trading bot
- `s3`: Load data from S3

### Key Traits
- `Bot<MD>`: Core interface for both backtest and live - submit/cancel orders, access depth, elapse time
- `MarketDepth`/`L2MarketDepth`/`L3MarketDepth`: Order book interfaces
- `LatencyModel`: Models order round-trip latency
- `QueueModel`/`L3QueueModel`: Models queue position for fill simulation
- `FeeModel`: Trading fee calculation

### Python Integration
Python code uses Numba JIT compilation. The `@njit` decorated functions receive opaque pointers to Rust structures, calling through `cffi` bindings defined in `py-hftbacktest/hftbacktest/binding.py`.

### Factor Engine (独立仓库: `bookdisco/factor-engine`)
The `factor-engine` crate provides streaming O(1) incremental computation. It has **zero dependencies** on the main workspace and is maintained in its own repo.
- **Primitives**: `RingBuffer`, `WelfordRolling` (mean/std), `EMA`, `TimeWindowQueue`
- **Factors**: `OBIFactor` (order book imbalance), `VolatilityFactor`, `StopLossChecker`
- **Combiner**: `LinearCombiner` for multi-factor weighted combination
- **Local path**: `submodules/factor-engine/`

### Connector Order Tracking

Connectors track orders using multiple ID mappings to handle edge cases:

- **client_order_id**: Generated by connector (prefix + random string), sent to exchange
- **order_id_map**: Maps `(symbol, bot_order_id)` → `client_order_id`
- **exchange_order_id_map**: Maps `(symbol, bot_order_id)` → exchange's `orderId` (fallback for cancellation)

When canceling orders, the connector first tries `client_order_id`. If not found (e.g., after restart), it falls back to `exchange_order_id`.

### Docker Deployment

The system is deployed via Docker on two servers. Each server has its own compose file and env file.

**Compose files** (in `deploy/docker/`):

| File | Server | Exchange |
|------|--------|----------|
| `docker-compose.binance.yml` | Vultr | Binance PAPI |
| `docker-compose.mainnet.yml` | Hetzner | Bybit |

**Docker stack:** hft (supervisor + bots + connector + collector), ClickHouse, Grafana, nginx, cloudflared

**Docker build requires submodules:** The Dockerfile copies `submodules/factor-engine` and `submodules/hft-strategies` into the build context. Run `git submodule update --init` before building.

### Configuration Repository

Runtime configs (API keys, strategy params, cluster topology) live in a **separate private repo**: [`bookdisco/hft-configs`](https://github.com/bookdisco/hft-configs)

**Why separated:** Strategy configs change frequently and differ per server. Code changes less often. Mixing them causes unnecessary rebuilds and deployment complexity.

**Repo structure:**
```
hft-configs/
├── binance/              # Vultr server (Binance PAPI)
│   ├── cluster.toml      # Supervisor config: which bots to run, warmup settings
│   ├── connector.toml    # API endpoint + credential placeholders (${BINANCE_API_KEY})
│   └── strategies/       # Per-symbol strategy configs
│       ├── obi_mm_alpine.toml
│       ├── obi_mm_esports.toml
│       ├── obi_mm_f.toml
│       ├── obi_mm_idol.toml
│       ├── obi_mm_naoris.toml
│       ├── obi_mm_sys.toml
│       ├── obi_mm_ta.toml
│       ├── obi_mm_xpin.toml
│       └── obi_mm_zerebro.toml
└── bybit/                # Hetzner server (Bybit)
    ├── cluster.toml
    ├── connector.toml
    └── strategies/
        ├── obi_mm_btc_mainnet.toml
        ├── obi_mm_eth_mainnet.toml
        └── obi_mm_ada_mainnet.toml
```

**Mount mechanism:** Docker compose mounts the config repo subdirectory as `/etc/hft:ro`:
```yaml
volumes:
  - ${HFT_CONFIG_DIR:-./config}:/etc/hft:ro
```

**Environment files** (`.env.binance` / `.env.mainnet`, NOT in git):
```bash
# API credentials (substituted by entrypoint.sh into connector.toml)
BINANCE_API_KEY=xxx
BINANCE_SECRET=xxx
# Config repo path relative to deploy/docker/
HFT_CONFIG_DIR=./hft-configs/binance
# Other services
CLOUDFLARE_TUNNEL_TOKEN=xxx
GF_ROOT_URL=https://hft-bn.bookdisco.space/grafana/
```

**API key substitution:** `entrypoint.sh` runs `sed` on `/etc/hft/connector.toml` at container startup to replace `${BINANCE_API_KEY}`, `${BINANCE_SECRET}`, `${BYBIT_API_KEY}`, `${BYBIT_SECRET}` with actual values from environment.

### Cross-Repo Development

**Strategy repo** (`hft-strategies`) uses SSH git dependencies:
```toml
hftbacktest    = { git = "ssh://git@github.com/bookdisco/hftbacktest", features = ["backtest"] }
factor-engine  = { git = "ssh://git@github.com/bookdisco/factor-engine" }
order-recorder = { git = "ssh://git@github.com/bookdisco/hftbacktest", optional = true }
```

**Local development** uses path overrides via `submodules/hft-strategies/.cargo/config.toml`:
```toml
[patch."ssh://git@github.com/bookdisco/hftbacktest"]
hftbacktest = { path = "../../hftbacktest" }
order-recorder = { path = "../../order-recorder" }

[patch."ssh://git@github.com/bookdisco/factor-engine"]
factor-engine = { path = "../factor-engine" }
```

This means changes to `hftbacktest/` or `submodules/factor-engine/` are **immediately visible** when building strategies locally — no need to push/pull.

**Workspace layout:**
- Main workspace (`Cargo.toml`): only framework crates, `exclude = ["submodules"]`
- Strategy workspace (`submodules/hft-strategies/Cargo.toml`): independent, uses git deps + path overrides
- `cargo build` in root → builds framework only
- `cargo build` in `submodules/hft-strategies/` → builds strategies with local overrides

### Config Update Workflow

**To update strategy parameters or add/remove bots:**

```bash
# 1. On the server, cd to the config repo
cd ~/hftbacktest/deploy/docker/hft-configs

# 2. Edit configs (or pull changes from git)
git pull origin main
# Or edit directly: vim binance/strategies/obi_mm_sys.toml

# 3. Commit and push if edited directly
git add -A && git commit -m "update SYS params" && git push

# 4. Restart the hft container to pick up changes
cd ~/hftbacktest/deploy/docker
docker compose -f docker-compose.binance.yml --env-file .env.binance restart hft

# 5. Verify
docker compose -f docker-compose.binance.yml --env-file .env.binance exec hft \
  hftctl --socket /tmp/hft-supervisor.sock status
```

**To generate strategy configs from fitted YAML:**
```bash
python scripts/yaml_to_toml_configs.py \
  --yaml /path/to/selected_coins_params.yaml \
  --output hft-configs/binance/strategies/ \
  --capital 2000
```

**Important:** Always sync config changes back to git so both servers and local dev stay consistent. Avoid modifying configs only on the server without committing — they will be lost on next `git pull`.

### hftctl Commands

The `hftctl` CLI manages the supervisor:

```bash
# Check cluster status
hftctl status

# Bot management
hftctl start <bot_id>
hftctl stop <bot_id>
hftctl restart <bot_id>

# Infrastructure management
hftctl infra start connector
hftctl infra stop collector
hftctl infra restart order_recorder
```

### Strategy Configuration Parameters

**Important parameter distinctions:**

| Parameter | Location | Purpose |
|-----------|----------|---------|
| `live_seconds` | strategy config `[params]` | Order lifetime - cancel unfilled orders after this time |
| `min_duration_seconds` | cluster config `[warmup]` | Historical data duration to load for factor warmup |
| `window` | strategy config `[params]` | OBI rolling window size (in samples) |
| `volatility_window` | strategy config `[params]` | Volatility rolling window size (in samples) |

**Warmup vs Order Lifetime:**
- `live_seconds` is NOT warmup time - it controls how long orders stay before cancellation
- Warmup is controlled by `[warmup].min_duration_seconds` in cluster config
- Set `min_duration_seconds` >= factor window size (e.g., 1800 for 30-min window)

### Server Operations

**Binance PAPI (Vultr):**
```bash
cd ~/hftbacktest/deploy/docker
docker compose -f docker-compose.binance.yml --env-file .env.binance up -d
docker compose -f docker-compose.binance.yml --env-file .env.binance exec hft \
  hftctl --socket /tmp/hft-supervisor.sock status
docker compose -f docker-compose.binance.yml --env-file .env.binance logs -f hft
```

**Bybit (Hetzner):**
```bash
cd ~/hftbacktest/deploy/docker
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet up -d
docker compose -f docker-compose.mainnet.yml exec hft \
  hftctl --socket /tmp/hft-supervisor.sock status
docker compose -f docker-compose.mainnet.yml logs -f hft
```

**Bybit Connector Types:**
- `type = "bybit"`: WebSocket Trade stream for order submission (lower latency)
- `type = "bybit_rest"`: REST API for order submission

### Remote Server Management with tmux-cli

Use `tmux-cli` to manage the mainnet server (Hetzner_bybit) from Claude Code:

```bash
# Launch a shell and connect to Hetzner
tmux-cli launch "zsh"
tmux-cli send "ssh Hetzner_bybit" --pane=<pane_id>

# Wait for connection and capture output
tmux-cli wait_idle --pane=<pane_id> --idle-time=3.0
tmux-cli capture --pane=<pane_id>

# Send commands to the server
tmux-cli send "cd ~/hftbacktest/deploy/docker" --pane=<pane_id>
tmux-cli send "docker compose -f docker-compose.mainnet.yml exec hft hftctl --socket /tmp/hft-supervisor.sock status" --pane=<pane_id>

# List panes and windows
tmux-cli list_panes
tmux-cli list_windows
```

**Common operations on Hetzner_bybit:**
```bash
# Check service status
docker compose -f docker-compose.mainnet.yml exec hft hftctl --socket /tmp/hft-supervisor.sock status

# View logs
docker compose -f docker-compose.mainnet.yml logs -f hft

# View bot logs
docker compose -f docker-compose.mainnet.yml exec hft tail -f /var/lib/hft/logs/bots/obi_mm_btcusdt.log

# View connector logs
docker compose -f docker-compose.mainnet.yml exec hft tail -f /var/lib/hft/logs/infra/connector.log

# Restart service (after config changes)
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet restart hft

# Full restart (down + up)
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet down
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet up -d
```

**Server info:**
- **SSH alias:** `Hetzner_bybit`
- **IP:** 5.223.54.76
- **Bybit latency:** ~17ms
- **Code path:** `~/hftbacktest/deploy/docker/`
- **Config repo:** `~/hft-configs/` (cloned from `bookdisco/hft-configs`)
- **Compose file:** `docker-compose.mainnet.yml` + `.env.mainnet`

### Backtest Alignment Tools

Tools for verifying alignment between Python strategy backtests and Rust implementations before live deployment.

**Quick Start:**
```bash
# Compare Rust and Python backtest results
python tools/backtest_alignment/compare.py \
  --rust obi_mm0.csv \
  --python obi_mm_python.npz \
  --output ./results
```

**Claude Skill:** Use `/align-backtest` to invoke the alignment workflow, which guides through:
1. Running both Python and Rust backtests
2. Computing correlation metrics (PnL, position, balance)
3. Identifying common discrepancies
4. Applying fixes and verifying alignment

**Alignment Thresholds:**
| Metric | Threshold | Status |
|--------|-----------|--------|
| PnL Pearson | > 0.95 | GOOD - ready for live |
| PnL Pearson | 0.8 - 0.95 | NEEDS WORK - investigate |
| PnL Pearson | < 0.8 | POOR - significant issues |

**Common Alignment Issues:**

1. **Standard Deviation (ddof)**
   - Python `np.nanstd()` uses ddof=0 (population std)
   - Rust: use `welford.population_std()` not `welford.std()`

2. **Stop Loss Logic**
   - Trigger: `vol_zscore >= threshold AND change_zscore >= threshold * 2`
   - Recovery: `vol_zscore <= threshold / 2` (hysteresis)

3. **Warmup Behavior**
   - Original Python often has NO warmup check
   - Remove `is_ready()` check in Rust if Python trades immediately

4. **Volatility Window Timing**
   - Python: `change_t > 60` (needs 61+ samples)
   - Rust: use `sample_count() > volatility_window`, not `>=`

**API Usage:**
```python
from tools.backtest_alignment import (
    load_rust_results,
    load_python_results,
    compute_alignment_metrics,
    plot_comparison
)

rust_df = load_rust_results('obi_mm0.csv')
python_df = load_python_results('obi_mm_python.npz')
metrics, merged = compute_alignment_metrics(rust_df, python_df)
print(metrics.summary())
plot_comparison(rust_df, python_df, merged, metrics, 'comparison.png')
```

### Binance PAPI Test Server (Vultr)

Use `tmux-cli` to manage the Binance PAPI test server:

```bash
# Launch a shell and connect
tmux-cli launch "zsh"
tmux-cli send "ssh Vultur_mmtest0" --pane=<pane_id>

# Wait and capture output
tmux-cli wait_idle --pane=<pane_id> --idle-time=3.0
tmux-cli capture --pane=<pane_id>
```

**Server info:**
- **SSH alias:** `Vultur_mmtest0`
- **IP:** 45.32.34.210
- **Purpose:** Binance PAPI mainnet (9 altcoins)
- **Code path:** `~/hftbacktest/deploy/docker/`
- **Config repo:** `~/hftbacktest/deploy/docker/hft-configs/` (cloned from `bookdisco/hft-configs`)
- **Compose file:** `docker-compose.binance.yml` + `.env.binance`

### Key Documentation
- `docs/LIVE_TRADING_ARCHITECTURE.md`: Detailed live trading system architecture (Chinese)
- `docs/FACTOR_FRAMEWORK_DESIGN.md`: Factor computation framework design
- `docs/STRATEGY_GUIDE.md`: Strategy implementation guide
- `deploy/docker/README.md`: Docker deployment guide
- `tools/backtest_alignment/README.md`: Backtest alignment tools guide

### Related Repositories
| Repo | Purpose | URL |
|------|---------|-----|
| `bookdisco/factor-engine` | O(1) factor computation | `git@github.com:bookdisco/factor-engine.git` |
| `bookdisco/hft-strategies` | Trading strategies (Rust + Python) | `git@github.com:bookdisco/hft-strategies.git` |
| `bookdisco/hft-configs` | Strategy parameters (TOML) | `git@github.com:bookdisco/hft-configs.git` |
