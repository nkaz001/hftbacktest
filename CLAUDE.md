# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

HftBacktest is a high-frequency trading and market-making backtesting framework. The core is written in Rust with Python bindings via PyO3/Maturin. It focuses on accurate simulation by accounting for feed/order latencies and order queue positions using full order book tick data.

## Build Commands

### Rust (主仓库)
```bash
cargo build                              # Build all workspace members
cargo build --release                    # Release build
cargo test                               # Run tests
cargo build --release -p connector       # Build specific package
```

### Rust (策略, 在 submodules/hft-strategies/ 下)
```bash
cd submodules/hft-strategies
cargo build --release -p obi-mm                    # 回测
cargo build --release -p obi-mm --features live    # 实盘
# path override 由 .cargo/config.toml 自动处理
```

### Python
```bash
cd py-hftbacktest && maturin develop --release     # Dev install
pytest py-hftbacktest/tests/                       # Tests
```

### Running Live Trading
```bash
./target/release/connector --name bf --connector binancefutures --config config.toml
./submodules/hft-strategies/target/release/obi_mm_live --config config.toml
```

## Architecture

### Multi-Repo Structure

```
bookdisco/hftbacktest          ← 主仓库 (本仓库: 核心框架 + 实盘组件)
bookdisco/factor-engine        ← 独立仓库 (因子计算引擎)
bookdisco/hft-strategies       ← 独立仓库 (交易策略: Rust + Python)
bookdisco/hft-configs          ← 独立仓库 (策略参数 TOML + 部署清单)
```

**Git submodules** (在 `submodules/` 下):
```
submodules/
├── factor-engine/       → git@github.com:bookdisco/factor-engine.git
├── hft-configs/         → git@github.com:bookdisco/hft-configs.git
└── hft-strategies/      → git@github.com:bookdisco/hft-strategies.git
```

```bash
git clone --recurse-submodules git@github.com:bookdisco/hftbacktest.git
# 或: git submodule update --init
```

### Workspace Structure (本仓库)
- **hftbacktest/**: Core Rust library - backtesting engine, live trading bot, market depth
- **hftbacktest-derive/**: Procedural macros (NpyDTyped derive)
- **py-hftbacktest/**: Python bindings (PyO3) and pure Python utilities
- **collector/**: Market data collection from exchanges (Binance, Bybit, Hyperliquid)
- **connector/**: Live trading connections (Binance Futures/Spot/PAPI, Bybit)
- **order-recorder/**: Order recording to ClickHouse
- **supervisor/**: Process supervisor (hft-supervisor + hftctl)
- **server/**: HTTP API server (dashboard backend)
- **deploy/**: Deployment configurations (Docker, systemd)

### External Repos (git submodules)
- **submodules/factor-engine/**: Streaming O(1) factor computation (OBI, volatility, trade imbalance)
  - Primitives: `RingBuffer`, `WelfordRolling`, `EMA`, `TimeWindowQueue`
  - Factors: `OBIFactor`, `VolatilityFactor`, `StopLossChecker`
  - `LinearCombiner` for multi-factor weighted combination
- **submodules/hft-strategies/**: Trading strategies
  - `obi_mm_rust/`: Rust OBI market making (backtest + live)
  - `template_strategy/`: Template for new strategies
  - `obi_mm/`: Python OBI strategy
  - `gridtrading.py`: Python grid trading example

### Core Concepts

**Dual-Mode Architecture**: Same algorithm code runs in both backtesting and live trading via the `Bot` trait.

**Local/Exchange Processors**: Backtesting simulates two sides - `Local` (your system) and `Exchange` (remote). Orders flow between them with configurable latency models.

**Market Depth Implementations**:
- `HashMapMarketDepth`: General purpose, any price levels
- `ROIVectorMarketDepth`: Optimized for bounded price ranges
- `BTreeMarketDepth`: For L3 (Market-By-Order) data

**Data Format**: Numpy-compatible `.npz` files with `Event` struct (64 bytes). `EXCH_EVENT`/`LOCAL_EVENT` flags indicate processor routing.

**IPC for Live Trading**: Bot ↔ connector via iceoryx2 shared memory (zero-copy). Both must run on same machine.

### Feature Flags (Rust)
- `backtest` (default): Backtesting functionality
- `live` (default): Live trading bot
- `s3`: Load data from S3

### Key Traits
- `Bot<MD>`: Core interface - submit/cancel orders, access depth, elapse time
- `MarketDepth`/`L2MarketDepth`/`L3MarketDepth`: Order book interfaces
- `LatencyModel`: Order round-trip latency models
- `QueueModel`/`L3QueueModel`: Queue position for fill simulation
- `FeeModel`: Trading fee calculation

### Python Integration
Python uses Numba JIT. `@njit` functions receive opaque pointers to Rust structures via `cffi` bindings in `py-hftbacktest/hftbacktest/binding.py`.

### Cross-Repo Development

**Strategy repo** uses SSH git deps with local path overrides:
```toml
# submodules/hft-strategies/Cargo.toml
hftbacktest    = { git = "ssh://git@github.com/bookdisco/hftbacktest", features = ["backtest"] }
factor-engine  = { git = "ssh://git@github.com/bookdisco/factor-engine" }

# submodules/hft-strategies/.cargo/config.toml (path overrides for local dev)
[patch."ssh://git@github.com/bookdisco/hftbacktest"]
hftbacktest = { path = "../../hftbacktest" }
order-recorder = { path = "../../order-recorder" }
[patch."ssh://git@github.com/bookdisco/factor-engine"]
factor-engine = { path = "../factor-engine" }
```

Changes to `hftbacktest/` or `submodules/factor-engine/` are **immediately visible** when building strategies locally.

**Workspace layout:**
- `cargo build` in root → builds framework only (main workspace excludes `submodules/`)
- `cargo build` in `submodules/hft-strategies/` → builds strategies with local overrides

### Connector Order Tracking

Connectors track orders via multiple ID mappings:
- **client_order_id**: Generated by connector (prefix + random string), sent to exchange
- **order_id_map**: `(symbol, bot_order_id)` → `client_order_id`
- **exchange_order_id_map**: `(symbol, bot_order_id)` → exchange `orderId` (fallback for cancellation)

## Topic-Specific Docs (read on demand)

| Topic | File | When to read |
|-------|------|-------------|
| Deployment & Server Ops | `deploy/CLAUDE.md` | Docker, hft-configs, hftctl, server management, tmux-cli |
| Backtest Alignment | `tools/backtest_alignment/CLAUDE.md` | Python↔Rust backtest comparison |
| Live Trading Architecture | `docs/LIVE_TRADING_ARCHITECTURE.md` | Detailed system design (Chinese) |
| Factor Framework | `docs/FACTOR_FRAMEWORK_DESIGN.md` | Factor computation design |
| Strategy Guide | `docs/STRATEGY_GUIDE.md` | Strategy implementation |

### Versioning & Release Tags

两条 tag 线，分别触发对应 CI：

| 用途 | Tag 格式 | CI Workflow | 示例 |
|------|---------|-------------|------|
| 主版本 (Rust crate + Docker image) | `v*` | `docker.yml` → GHCR | `v0.10.0` |
| Python wheel | `py-v*` | `release-python-private.yml` → Private PyPI | `py-v2.6.0` |

发布流程：
```bash
# Rust + Docker
git tag v0.10.0 && git push origin v0.10.0

# Python wheel
git tag py-v2.6.0 && git push origin py-v2.6.0
```

注意：
- 不再使用 `rust-v*` 前缀（已清理）
- `v1.x` 系列为 upstream 遗留（已清理）
- Rust crate 版本在 `hftbacktest/Cargo.toml`，Python 版本在 `py-hftbacktest/pyproject.toml` + `py-hftbacktest/Cargo.toml`

### Related Repositories
| Repo | Purpose | URL |
|------|---------|-----|
| `bookdisco/factor-engine` | O(1) factor computation | `git@github.com:bookdisco/factor-engine.git` |
| `bookdisco/hft-strategies` | Trading strategies (Rust + Python) | `git@github.com:bookdisco/hft-strategies.git` |
| `bookdisco/hft-configs` | Strategy params + deployment manifests | `git@github.com:bookdisco/hft-configs.git` |
