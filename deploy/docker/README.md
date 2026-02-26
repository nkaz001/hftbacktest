# HFT System Docker Deployment

One-click Docker deployment for the HFT trading system.

## Quick Start

```bash
cd deploy/docker

# 1. Configure API keys
cp .env.example .env
# Edit .env with your Binance API keys

# 2. Build and start
docker compose build    # First build takes 30-45 minutes
docker compose up -d

# 3. Access
# Dashboard: http://localhost:13000
# Login: admin / admin
```

## Services

| Service | Port | Description |
|---------|------|-------------|
| Dashboard | 13000 | Web management UI |
| ClickHouse HTTP | 18123 | Order analytics database |
| ClickHouse Native | 19000 | Native protocol |

The `hft` container includes:
- **hft-supervisor**: Process manager for all components
- **hft-server**: Web dashboard and REST API
- **connectors**: Exchange connections (supports multiple: Binance Futures, Bybit, etc.)
- **collector**: Market data recording
- **order-recorder**: Trade logging to ClickHouse
- **strategies**: Trading bots (obi_mm, template_strategy)

## Configuration

### Environment Variables (.env)

```bash
# Binance Futures API (for bf connector)
BINANCE_API_KEY=your_api_key
BINANCE_SECRET=your_secret

# Bybit API (for bybit connector)
BYBIT_API_KEY=your_api_key
BYBIT_SECRET=your_secret

# Server
JWT_SECRET=change_me_in_production
RUST_LOG=info
```

### Cluster Configuration (config/cluster.toml)

The main configuration file controls all components.

---

## Multi-Connector Support

The system supports running multiple exchange connectors simultaneously. Each bot can specify which connector to use.

### Connector Configuration

Use `[[connectors]]` array in cluster.toml to define multiple connectors:

```toml
# Binance Futures Testnet
[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/binancefutures.toml"
auto_start = true

# Bybit Demo Trading
[[connectors]]
name = "bybit"
type = "bybit_rest"
config_file = "/etc/hft/bybit.toml"
auto_start = true
```

### Bot-Connector Mapping

Each bot specifies which connector to use via the `connector` field:

```toml
# Bot using Binance Futures
[[bots]]
id = "obi_mm_btcusdt_bf"
symbol = "btcusdt"          # Binance: lowercase
connector = "bf"
# ...

# Bot using Bybit
[[bots]]
id = "obi_mm_btcusdt_bybit"
symbol = "BTCUSDT"          # Bybit: UPPERCASE
connector = "bybit"
# ...
```

### Symbol Case Sensitivity

Different exchanges have different symbol format requirements:
- **Binance**: lowercase (`btcusdt`, `ethusdt`, `solusdt`)
- **Bybit**: UPPERCASE (`BTCUSDT`, `ETHUSDT`, `SOLUSDT`)

### Connector Types

| Type | Exchange | Description |
|------|----------|-------------|
| `binancefutures` | Binance USD-M Futures | WebSocket + REST |
| `binancespot` | Binance Spot | WebSocket + REST |
| `binancepapi` | Binance Portfolio Margin | WebSocket + REST |
| `bybit` | Bybit | WebSocket for orders |
| `bybit_rest` | Bybit (REST) | REST API for orders |

### Check Connector Status

```bash
# Via hftctl
docker exec hft-system hftctl --socket /tmp/hft-supervisor.sock status

# Output shows all connectors:
# connector:bf     running (healthy)
# connector:bybit  running (healthy)
```

---

## Running Multiple Bots / Multiple Symbols

### Step 1: Update Collector Symbols

Edit `config/cluster.toml` to collect data for all symbols:

```toml
[collector]
enabled = true
mode = "ipc"
output_dir = "/var/lib/hft/data"
symbols = ["btcusdt", "ethusdt", "solusdt"]  # Add all symbols
retention_days = 7
auto_start = true
```

### Step 2: Add Bot Configurations

Add multiple `[[bots]]` sections in `config/cluster.toml`:

```toml
# ============================================
# Bot 1: BTC/USDT OBI Market Making
# ============================================
[[bots]]
id = "obi_mm_btcusdt"
enabled = true
strategy = "obi_mm_live"
symbol = "btcusdt"
config_file = "/etc/hft/strategies/obi_mm_btc.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

# ============================================
# Bot 2: ETH/USDT OBI Market Making
# ============================================
[[bots]]
id = "obi_mm_ethusdt"
enabled = true
strategy = "obi_mm_live"
symbol = "ethusdt"
config_file = "/etc/hft/strategies/obi_mm_eth.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

# ============================================
# Bot 3: SOL/USDT OBI Market Making
# ============================================
[[bots]]
id = "obi_mm_solusdt"
enabled = true
strategy = "obi_mm_live"
symbol = "solusdt"
config_file = "/etc/hft/strategies/obi_mm_sol.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

### Step 3: Create Strategy Config Files

Create separate config files for each symbol in `config/strategies/`:

**config/strategies/obi_mm_btc.toml** (BTC/USDT):
```toml
[global]
order_qty = 0.002        # ~$180 per order at $90k
max_position = 0.02      # ~$1800 max position
grid_num = 2
grid_interval = 1.0      # $1 grid spacing
roi_lb = 0.0
roi_ub = 200000.0

[params]
looking_depth = 0.001
window = 5
volatility_window = 2
half_spread = 0.0005     # 5 bps
skew = 0.01
c1 = 0.0001
power = 1.0
live_seconds = 30.0
update_interval_ns = 5000000000
volatility_interval_ns = 10000000000

[stop_loss]
volatility_mean = 0.005
volatility_std = 0.002
change_mean = 0.001
change_std = 0.003
volatility_threshold = 3.0
change_threshold = 3.0
```

**config/strategies/obi_mm_eth.toml** (ETH/USDT):
```toml
[global]
order_qty = 0.02         # ~$70 per order at $3500
max_position = 0.2       # ~$700 max position
grid_num = 2
grid_interval = 0.5      # $0.50 grid spacing
roi_lb = 0.0
roi_ub = 10000.0

[params]
looking_depth = 0.001
window = 5
volatility_window = 2
half_spread = 0.0006     # 6 bps
skew = 0.01
c1 = 0.0001
power = 1.0
live_seconds = 30.0
update_interval_ns = 5000000000
volatility_interval_ns = 10000000000

[stop_loss]
volatility_mean = 0.008
volatility_std = 0.003
change_mean = 0.002
change_std = 0.005
volatility_threshold = 3.0
change_threshold = 3.0
```

**config/strategies/obi_mm_sol.toml** (SOL/USDT):
```toml
[global]
order_qty = 1.0          # ~$200 per order at $200
max_position = 10.0      # ~$2000 max position
grid_num = 2
grid_interval = 0.05     # $0.05 grid spacing
roi_lb = 0.0
roi_ub = 1000.0

[params]
looking_depth = 0.001
window = 5
volatility_window = 2
half_spread = 0.0008     # 8 bps (SOL more volatile)
skew = 0.01
c1 = 0.0001
power = 1.0
live_seconds = 30.0
update_interval_ns = 5000000000
volatility_interval_ns = 10000000000

[stop_loss]
volatility_mean = 0.012
volatility_std = 0.005
change_mean = 0.003
change_std = 0.008
volatility_threshold = 3.0
change_threshold = 3.0
```

### Step 4: Apply Configuration

```bash
# Restart to apply changes
docker compose restart hft

# Or rebuild if you added new strategy files
docker compose up -d
```

### Complete Multi-Bot Multi-Connector cluster.toml Example

```toml
# HFT Cluster Configuration - Multi-Bot Multi-Connector Example

[cluster]
name = "hft-multi-bot"
data_dir = "/var/lib/hft/data"
log_dir = "/var/lib/hft/logs"
socket_path = "/tmp/hft-supervisor.sock"
config_reload_interval_secs = 10

# Binance Futures Testnet
[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/binancefutures.toml"
auto_start = true

# Bybit Demo Trading
[[connectors]]
name = "bybit"
type = "bybit_rest"
config_file = "/etc/hft/bybit.toml"
auto_start = true

[collector]
enabled = true
mode = "ipc"
output_dir = "/var/lib/hft/data"
# Collector uses first connector (Binance), so lowercase symbols
symbols = ["btcusdt", "ethusdt", "solusdt"]
retention_days = 7
auto_start = true

[order_recorder]
enabled = true
auto_start = true
ipc_service = "hft_recording"
clickhouse_url = "http://clickhouse:8123"
database = "hft"
batch_size = 100
flush_interval_secs = 5

[warmup]
data_dir = "/var/lib/hft/data"
max_age_seconds = 86400
min_duration_seconds = 60

[bot_defaults]
tick_size = 0.1
lot_size = 0.001

# ============================================
# Bots using Binance Futures (lowercase symbols)
# ============================================

[[bots]]
id = "obi_mm_btcusdt_bf"
enabled = true
strategy = "obi_mm_live"
symbol = "btcusdt"
connector = "bf"
config_file = "/etc/hft/strategies/obi_mm_btc.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

[[bots]]
id = "obi_mm_ethusdt_bf"
enabled = true
strategy = "obi_mm_live"
symbol = "ethusdt"
connector = "bf"
config_file = "/etc/hft/strategies/obi_mm_eth.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

# ============================================
# Bots using Bybit (UPPERCASE symbols)
# ============================================

[[bots]]
id = "obi_mm_btcusdt_bybit"
enabled = true
strategy = "obi_mm_live"
symbol = "BTCUSDT"
connector = "bybit"
config_file = "/etc/hft/strategies/obi_mm_btc.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

[[bots]]
id = "obi_mm_solusdt_bybit"
enabled = true
strategy = "obi_mm_live"
symbol = "SOLUSDT"
connector = "bybit"
config_file = "/etc/hft/strategies/obi_mm_sol.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

---

## Commands

```bash
# Start all services
docker compose up -d

# Stop all services
docker compose down

# Restart HFT container (reload config)
docker compose restart hft

# Rebuild after code changes
docker compose build hft
docker compose up -d

# Check container status
docker compose ps

# Enter container shell
docker exec -it hft-system bash

# Check cluster status via hftctl
docker exec hft-system hftctl --socket /tmp/hft-supervisor.sock status
```

## Viewing Logs

### Docker Compose Logs (stdout/stderr)

```bash
# View all service logs
docker compose logs

# Follow logs in real-time
docker compose logs -f

# View specific service logs
docker compose logs hft
docker compose logs clickhouse

# Follow with last N lines
docker compose logs -f --tail 100 hft
```

### HFT System Internal Logs

Log files are stored in `/var/lib/hft/logs/` inside the container:

```
/var/lib/hft/logs/
├── infra/
│   ├── connector_bf.log     # Binance Futures connector
│   ├── connector_bybit.log  # Bybit connector
│   ├── collector.log        # Market data collection
│   ├── order_recorder.log   # Order recording to ClickHouse
│   └── server.log           # Web dashboard API server
└── bots/
    ├── obi_mm_btcusdt_bf.log
    ├── obi_mm_ethusdt_bf.log
    ├── obi_mm_btcusdt_bybit.log
    └── obi_mm_solusdt_bybit.log
```

**Infrastructure Logs:**

```bash
# Connector logs (each connector has its own log)
docker exec hft-system cat /var/lib/hft/logs/infra/connector_bf.log
docker exec hft-system cat /var/lib/hft/logs/infra/connector_bybit.log
docker exec hft-system tail -f /var/lib/hft/logs/infra/connector_bf.log

# Collector logs (market data)
docker exec hft-system cat /var/lib/hft/logs/infra/collector.log
docker exec hft-system tail -f /var/lib/hft/logs/infra/collector.log

# Order recorder logs
docker exec hft-system cat /var/lib/hft/logs/infra/order_recorder.log

# Web server logs
docker exec hft-system cat /var/lib/hft/logs/infra/server.log
```

**Bot Logs:**

```bash
# View specific bot log
docker exec hft-system cat /var/lib/hft/logs/bots/obi_mm_btcusdt_bf.log
docker exec hft-system cat /var/lib/hft/logs/bots/obi_mm_btcusdt_bybit.log

# Follow bot log in real-time
docker exec hft-system tail -f /var/lib/hft/logs/bots/obi_mm_btcusdt_bf.log

# List all log files
docker exec hft-system ls -la /var/lib/hft/logs/infra/
docker exec hft-system ls -la /var/lib/hft/logs/bots/
```

### ClickHouse Logs

```bash
# ClickHouse server log
docker exec hft-clickhouse cat /var/log/clickhouse-server/clickhouse-server.log

# ClickHouse error log
docker exec hft-clickhouse cat /var/log/clickhouse-server/clickhouse-server.err.log

# Follow ClickHouse logs
docker exec hft-clickhouse tail -f /var/log/clickhouse-server/clickhouse-server.log
```

### View Logs via API

```bash
# Get bot logs via REST API (last 100 lines)
curl "http://localhost:13000/api/logs/obi_mm_btcusdt?lines=100"
```

## API Endpoints

```bash
# Health check
curl http://localhost:13000/api/health

# List all bots
curl http://localhost:13000/api/bots

# Get specific bot status
curl http://localhost:13000/api/bots/obi_mm_btcusdt

# Start/Stop/Restart bot
curl -X POST http://localhost:13000/api/bots/obi_mm_btcusdt/start
curl -X POST http://localhost:13000/api/bots/obi_mm_btcusdt/stop
curl -X POST http://localhost:13000/api/bots/obi_mm_btcusdt/restart

# Restart all bots
curl -X POST http://localhost:13000/api/bots/restart-all

# Infrastructure status
curl http://localhost:13000/api/infra/connector
curl http://localhost:13000/api/infra/collector
curl http://localhost:13000/api/infra/order_recorder

# View logs via API
curl "http://localhost:13000/api/logs/obi_mm_btcusdt?lines=100"
```

## Directory Structure

```
deploy/docker/
├── Dockerfile              # Multi-stage build
├── docker-compose.yml      # Service definitions
├── entrypoint.sh          # Container startup script
├── .env                   # API keys (not in git)
├── .env.example           # Template for .env
├── README.md              # This file
├── config/
│   ├── cluster.toml       # Main cluster config
│   ├── binancefutures.toml # Binance Futures connector config
│   ├── bybit.toml         # Bybit connector config
│   └── strategies/
│       ├── obi_mm.toml    # Default strategy config
│       ├── obi_mm_btc.toml
│       ├── obi_mm_eth.toml
│       └── obi_mm_sol.toml
├── scripts/
│   └── init-clickhouse.sql # ClickHouse schema
├── data/                  # Collected market data (auto-created)
└── logs/                  # Bot and infra logs (auto-created)
```

## Volumes and Bind Mounts

| Host Path | Container Path | Description |
|-----------|----------------|-------------|
| ./data | /var/lib/hft/data | Collected market data (bind mount) |
| ./logs | /var/lib/hft/logs | Bot and infra logs (bind mount) |
| clickhouse_data (volume) | /var/lib/clickhouse | Order database |

**Accessing data from host:**

```bash
# View collected market data
ls -la deploy/docker/data/

# View logs directly
ls -la deploy/docker/logs/infra/
ls -la deploy/docker/logs/bots/
cat deploy/docker/logs/infra/connector.log
tail -f deploy/docker/logs/bots/obi_mm_btcusdt.log
```

## Testnet vs Mainnet

Default configuration uses **Binance Futures Testnet** and **Bybit Demo Trading**.

### Binance Futures

Edit `config/binancefutures.toml`:
```toml
# Testnet (default)
stream_url = "wss://fstream.binancefuture.com/ws"
api_url = "https://demo-fapi.binance.com"

# Mainnet (USE WITH CAUTION!)
# stream_url = "wss://fstream.binance.com/ws"
# api_url = "https://fapi.binance.com"
```

### Bybit

Edit `config/bybit.toml`:
```toml
# Demo Trading (default)
api_url = "https://api-demo.bybit.com"

# Mainnet (USE WITH CAUTION!)
# api_url = "https://api.bybit.com"
```

**Warning**: Use mainnet with real funds at your own risk.

## Troubleshooting

### Bot stuck in "waiting_warmup"
The collector needs time to gather market data. Wait 60+ seconds after startup.

### "Permission denied" errors
Config files are processed by entrypoint.sh. Check logs:
```bash
docker compose logs hft | head -50
```

### Bot crashes immediately
Check bot logs for config errors:
```bash
docker exec hft-system cat /var/lib/hft/logs/bots/<bot_id>.log
```

### Cannot connect to exchange
Verify API keys in .env and check connector logs:
```bash
docker exec hft-system cat /var/lib/hft/logs/infra/connector.log
```

### Dashboard login fails
Default credentials: `admin` / `admin`
Database is auto-initialized on first start.

### Port already in use
Default ports are mapped to avoid conflicts:
- 13000 (Dashboard, internal 3000)
- 18123 (ClickHouse HTTP, internal 8123)
- 19000 (ClickHouse Native, internal 9000)

## Production Recommendations

1. **Change default password** - Update admin password after first login
2. **Set strong JWT_SECRET** - Use a random 32+ character string
3. **Use mainnet API keys** - Switch from testnet when ready
4. **Monitor logs** - Set up log aggregation
5. **Backup ClickHouse data** - Regular snapshots of order history
6. **Resource limits** - Add memory/CPU limits in docker-compose.yml

```yaml
# Example resource limits
services:
  hft:
    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G
```

## Security Notes

1. Never commit `.env` to version control
2. Use strong JWT_SECRET in production
3. Consider restricting port exposure in production
4. Container runs as non-root user (hft)
5. Config files are read-only mounted
