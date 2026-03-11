# Deploy & Operations

## Docker Deployment

Three production servers, each with its own Docker Compose stack:

| Compose File | Server | SSH Alias | IP | Exchange | Symbols |
|---|---|---|---|---|---|
| `docker-compose.binance.yml` | Vultr | `Vultur_mmtest0` | 45.32.34.210 | Binance PAPI | 9 (binance-prod) |
| `docker-compose.binance.yml` | AWS Tokyo | `AWS_LEE_ACCOUNT` | 18.177.52.187 | Binance PAPI | 15 (binance-expansion) |
| `docker-compose.mainnet.yml` | Hetzner | `Hetzner_bybit` | 5.223.54.76 | Bybit | 3 (bybit-mainnet) |

**Docker stack:** hft (supervisor + bots + connector + collector), ClickHouse, Grafana, nginx, cloudflared

**Docker images** are built by GitHub Actions (`docker.yml`) on tag push or manual trigger, pushed to `ghcr.io/bookdisco/hftbacktest`. Servers pull pre-built images — no local builds needed.

**To build locally** (rare): `git submodule update --init && docker build -f deploy/docker/Dockerfile .`

## hft-configs Repository

Strategy configs live in a separate repo: [`bookdisco/hft-configs`](https://github.com/bookdisco/hft-configs) (also submodule at `submodules/hft-configs/`)

```
hft-configs/
├── symbols/                              # 标的参数 (共享，不区分服务器)
│   ├── binance/
│   │   ├── alpine.toml                   # [global] + [params] + [stop_loss]
│   │   ├── agt.toml, aio.toml, ...      # 24 symbols total
│   │   └── ...
│   └── bybit/
│       ├── btc_mainnet.toml, eth_mainnet.toml, ada_mainnet.toml
├── deployments/                          # 部署配置 (per-server)
│   ├── binance-prod.yaml                 # 清单: 9 symbols @ $1200/coin
│   ├── binance-prod/
│   │   ├── cluster.toml                  # Supervisor: 9 bots
│   │   └── connector.toml               # API endpoint + key placeholders
│   ├── binance-expansion.yaml            # 清单: 15 new symbols @ $1000/coin
│   ├── binance-expansion/
│   │   ├── cluster.toml                  # Supervisor: 15 bots
│   │   └── connector.toml
│   ├── bybit-mainnet.yaml                # 清单: 3 bybit symbols
│   └── bybit-mainnet/
│       ├── cluster.toml                  # Supervisor: 3 bots
│       └── connector.toml
└── scripts/
    ├── generate_configs.py               # 从 manifest + params YAML 生成 TOML
    └── manage_leverage.py                # 管理 Binance PAPI 杠杆
```

### TOML Config Structure
```toml
[global]          # 仓位: order_qty, max_position, tick_size, lot_size, grid_num, grid_interval
[params]          # 策略: half_spread, skew, c1, power, live_seconds, window, looking_depth
[stop_loss]       # 止损: volatility_mean/std, change_mean/std, thresholds
```

### Config Generation
```bash
# 从 deployment manifest + Optuna 参数生成 TOML
python scripts/generate_configs.py \
    --deployment deployments/binance-expansion.yaml \
    --params /path/to/selected_coins_params.yaml \
    --stoploss-json /path/to/stoploss_params.json

# 选项: --book-size 1200, --max-ratio 20, --symbols ALPINEUSDT TAUSDT, --dry-run
```

**Pipeline:** Optuna 搜参 → `selected_coins_params.yaml` → `generate_configs.py` (自动获取 Binance 实时价格/tick_size/lot_size) → TOML

### Docker Config Mounting

Docker compose mounts two directories — deployment config (per-server) and symbols (shared):
```yaml
volumes:
  - ${HFT_DEPLOYMENT_DIR}:/etc/hft:ro          # cluster.toml + connector.toml
  - ${HFT_SYMBOLS_DIR}:/etc/hft/symbols:ro     # 标的参数 TOML
```

cluster.toml 引用标的配置：`config_file = "/etc/hft/symbols/esports.toml"`

**Environment files** (`.env.binance` / `.env.mainnet`, NOT in git):
```bash
BINANCE_API_KEY=xxx
BINANCE_SECRET=xxx
HFT_DEPLOYMENT_DIR=./hft-configs/deployments/binance-prod
HFT_SYMBOLS_DIR=./hft-configs/symbols/binance
CLOUDFLARE_TUNNEL_TOKEN=xxx
```

**API key substitution:** `entrypoint.sh` runs `sed` on `/etc/hft/connector.toml` at startup to replace `${BINANCE_API_KEY}` etc. with env values.

## Strategy Configuration Parameters

| Parameter | Location | Purpose |
|-----------|----------|---------|
| `live_seconds` | strategy `[params]` | Order lifetime - cancel unfilled orders after this time |
| `min_duration_seconds` | cluster `[warmup]` | Historical data duration for factor warmup |
| `window` | strategy `[params]` | OBI rolling window size (in samples) |
| `volatility_window` | strategy `[params]` | Volatility rolling window size (in samples) |

**Warmup vs Order Lifetime:**
- `live_seconds` is NOT warmup — it controls how long orders stay before cancellation
- Warmup: `[warmup].min_duration_seconds` in cluster config, set >= factor window size

## Deployment

### Automated (recommended)
```bash
# Config-only update (no new image)
./scripts/deploy.sh binance --skip-pull

# Full deploy with new image
./scripts/deploy.sh binance --tag v1.2.0

# Deploy latest image
./scripts/deploy.sh bybit
```

### Building a new image
```bash
# Option 1: Tag push triggers CI
git tag v1.2.0 && git push origin v1.2.0

# Option 2: Manual trigger via GitHub Actions UI
# Go to Actions → "Build & Push Docker Image" → Run workflow

# Option 3: Local build (rare)
docker build -f deploy/docker/Dockerfile -t ghcr.io/bookdisco/hftbacktest:test .
```

### Manual config-only update (no image change)
```bash
# 1. Push config changes to hft-configs repo
cd submodules/hft-configs && git push origin main

# 2. Deploy (pulls configs + restarts)
./scripts/deploy.sh binance --skip-pull
```

**Important:** Always sync config changes back to git. Avoid server-only edits.

## hftctl Commands

```bash
hftctl status                          # Cluster status
hftctl start <bot_id>                  # Start bot
hftctl stop <bot_id>                   # Stop bot
hftctl restart <bot_id>                # Restart bot
hftctl infra start connector           # Start infrastructure
hftctl infra stop collector
hftctl infra restart order_recorder
```

## Server Operations

### Binance PAPI — Vultr (9 symbols, binance-prod)
```bash
cd ~/hftbacktest/deploy/docker
docker compose -f docker-compose.binance.yml --env-file .env.binance up -d
docker compose -f docker-compose.binance.yml --env-file .env.binance exec hft \
  hftctl --socket /tmp/hft-supervisor.sock status
```

### Binance PAPI — AWS Tokyo (15 symbols, binance-expansion)
```bash
cd ~/hftbacktest/deploy/docker
docker compose -f docker-compose.binance.yml --env-file .env.bn2 up -d
docker compose -f docker-compose.binance.yml --env-file .env.bn2 exec hft \
  hftctl --socket /tmp/hft-supervisor.sock status
```

### Bybit (Hetzner)
```bash
cd ~/hftbacktest/deploy/docker
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet up -d
docker compose -f docker-compose.mainnet.yml exec hft \
  hftctl --socket /tmp/hft-supervisor.sock status
docker compose -f docker-compose.mainnet.yml logs -f hft
```

**Bybit Connector Types:**
- `type = "bybit"`: WebSocket Trade stream (lower latency)
- `type = "bybit_rest"`: REST API

### Common Server Operations (Hetzner example)
```bash
# Bot logs
docker compose -f docker-compose.mainnet.yml exec hft tail -f /var/lib/hft/logs/bots/obi_mm_btcusdt.log
# Connector logs
docker compose -f docker-compose.mainnet.yml exec hft tail -f /var/lib/hft/logs/infra/connector.log
# Restart after config changes
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet restart hft
# Full restart
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet down
docker compose -f docker-compose.mainnet.yml --env-file .env.mainnet up -d
```

### Server Paths

| | Vultr (Binance 9) | AWS Tokyo (Binance 15) | Hetzner (Bybit) |
|---|---|---|---|
| SSH | `Vultur_mmtest0` | `AWS_LEE_ACCOUNT` | `Hetzner_bybit` |
| User | root | ubuntu | root |
| Code | `~/hftbacktest/deploy/docker/` | `~/hftbacktest/deploy/docker/` | `~/hftbacktest/deploy/docker/` |
| Config | `./hft-configs/` | `./hft-configs/` | `~/hft-configs/` |
| Env file | `.env.binance` | `.env.bn2` | `.env.mainnet` |
| Deployment | `binance-prod` | `binance-expansion` | `bybit-mainnet` |

## Remote Server Management with tmux-cli

```bash
# Launch and connect
tmux-cli launch "zsh"
tmux-cli send "ssh Hetzner_bybit" --pane=<pane_id>
tmux-cli wait_idle --pane=<pane_id> --idle-time=3.0
tmux-cli capture --pane=<pane_id>

# Send commands
tmux-cli send "cd ~/hftbacktest/deploy/docker" --pane=<pane_id>
tmux-cli send "docker compose -f docker-compose.mainnet.yml exec hft hftctl --socket /tmp/hft-supervisor.sock status" --pane=<pane_id>

# List panes/windows
tmux-cli list_panes
tmux-cli list_windows
```
