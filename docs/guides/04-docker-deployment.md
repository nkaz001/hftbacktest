# Docker 实盘部署

本章节介绍如何将 Rust 策略部署到基于 Docker 的实盘交易系统。

## 系统要求

- Docker 20.10+
- Docker Compose 2.0+
- 至少 4GB 内存
- 稳定的网络连接

## 快速部署

```bash
cd deploy/docker

# 1. 配置 API 密钥
cp .env.example .env
# 编辑 .env 填入你的 API 密钥

# 2. 构建并启动
docker compose build    # 首次构建约 30-45 分钟
docker compose up -d

# 3. 检查状态
docker compose exec hft hftctl --socket /tmp/hft-supervisor.sock status
```

## 配置文件详解

### 目录结构

```
deploy/docker/
├── .env                    # API 密钥 (不要提交到 git)
├── .env.example            # API 密钥模板
├── docker-compose.yml      # Docker 服务定义
├── Dockerfile              # 构建配置
├── entrypoint.sh           # 容器启动脚本
├── config/
│   ├── cluster.toml        # 主配置文件
│   ├── binancefutures.toml # Binance 配置
│   ├── bybit.toml          # Bybit 配置
│   └── strategies/
│       ├── obi_mm_btc.toml # BTC 策略参数
│       ├── obi_mm_eth.toml # ETH 策略参数
│       └── obi_mm_sol.toml # SOL 策略参数
├── data/                   # 市场数据 (自动创建)
└── logs/                   # 日志文件 (自动创建)
```

### .env 文件

```bash
# Binance Futures API (用于 bf connector)
BINANCE_API_KEY=your_binance_api_key
BINANCE_SECRET=your_binance_secret

# Bybit API (用于 bybit connector)
BYBIT_API_KEY=your_bybit_api_key
BYBIT_SECRET=your_bybit_secret

# 系统配置
JWT_SECRET=change_me_in_production
RUST_LOG=info
```

### cluster.toml 主配置

```toml
# 集群基本配置
[cluster]
name = "hft-docker"
data_dir = "/var/lib/hft/data"
log_dir = "/var/lib/hft/logs"
socket_path = "/tmp/hft-supervisor.sock"

# ============================================
# Connectors - 交易所连接器
# ============================================

# Binance Futures Testnet
[[connectors]]
name = "bf"                               # 连接器名称
type = "binancefutures"                   # 类型
config_file = "/etc/hft/binancefutures.toml"
auto_start = true
recording_service = "hft_recording"       # 订单记录服务
position_sync_interval = 5                # 仓位同步间隔(秒)
income_sync_interval = 60                 # 收入同步间隔(秒)

# Bybit Demo Trading
[[connectors]]
name = "bybit"
type = "bybit_rest"                       # REST API 方式
config_file = "/etc/hft/bybit.toml"
auto_start = true
recording_service = "hft_recording"

# ============================================
# Collector - 市场数据收集
# ============================================
[collector]
enabled = true
mode = "ipc"
output_dir = "/var/lib/hft/data"
symbols = ["btcusdt", "ethusdt"]          # 收集的交易对
retention_days = 7                        # 数据保留天数
auto_start = true

# ============================================
# Order Recorder - 订单记录
# ============================================
[order_recorder]
enabled = true
auto_start = true
ipc_service = "hft_recording"
clickhouse_url = "http://clickhouse:8123"
database = "hft"
batch_size = 100
flush_interval_secs = 5

# ============================================
# Warmup - 策略预热配置
# ============================================
[warmup]
data_dir = "/var/lib/hft/data"
max_age_seconds = 86400                   # 最多使用 24 小时内的数据
min_duration_seconds = 60                 # 至少需要 60 秒数据

# ============================================
# Bot 默认配置
# ============================================
[bot_defaults]
tick_size = 0.1
lot_size = 0.001

# ============================================
# Bots - 交易策略
# ============================================

[[bots]]
id = "obi_mm_btcusdt_bf"                  # 唯一标识
enabled = true                            # 是否启用
strategy = "obi_mm_testnet"               # 策略二进制名称
symbol = "btcusdt"                        # 交易对 (Binance 用小写)
connector = "bf"                          # 使用的连接器
config_file = "/etc/hft/strategies/obi_mm_btc.toml"
auto_restart = true                       # 崩溃后自动重启
max_restarts = 3                          # 最大重启次数
restart_backoff_secs = 5                  # 重启间隔

[[bots]]
id = "obi_mm_btcusdt_bybit"
enabled = true
strategy = "obi_mm_testnet"
symbol = "BTCUSDT"                        # 交易对 (Bybit 用大写)
connector = "bybit"
config_file = "/etc/hft/strategies/obi_mm_btc.toml"
```

### 策略配置 (obi_mm_btc.toml)

```toml
[global]
order_qty = 0.002        # 每单数量 (~$180 @ $90k)
max_position = 0.02      # 最大持仓 (~$1800)
grid_num = 2             # 网格层数
grid_interval = 1.0      # 网格间距 ($1)
roi_lb = 0.0             # ROI 下界
roi_ub = 200000.0        # ROI 上界

[params]
looking_depth = 0.001    # OBI 深度 (0.1%)
window = 5               # OBI 窗口 (测试网用小值加速预热)
volatility_window = 2    # 波动率窗口
half_spread = 0.0005     # 半价差 (5 bps)
skew = 0.01              # 仓位偏斜系数
c1 = 0.0001              # Alpha 系数
power = 1.0              # 波动率调整幂
live_seconds = 30.0      # 订单最小存活时间
update_interval_ns = 5000000000      # 更新间隔 (5秒)
volatility_interval_ns = 10000000000 # 波动率计算间隔 (10秒)

[stop_loss]
volatility_mean = 0.005
volatility_std = 0.002
change_mean = 0.001
change_std = 0.003
volatility_threshold = 3.0
change_threshold = 3.0
```

## 添加新交易对

以添加 XRPUSDT 为例：

### 步骤 1: 创建策略配置

```bash
cp config/strategies/obi_mm_btc.toml config/strategies/obi_mm_xrp.toml
```

编辑 `config/strategies/obi_mm_xrp.toml`:

```toml
[global]
order_qty = 50.0         # XRP 价格约 $2，50 XRP ≈ $100
max_position = 500.0     # 最大持仓 500 XRP ≈ $1000
grid_num = 2
grid_interval = 0.001    # $0.001 网格间距
roi_lb = 0.0
roi_ub = 100.0           # XRP 价格范围

[params]
looking_depth = 0.001
window = 5
volatility_window = 2
half_spread = 0.001      # 10 bps (XRP 波动较大)
skew = 0.01
c1 = 0.0001
power = 1.0
live_seconds = 30.0
update_interval_ns = 5000000000
volatility_interval_ns = 10000000000

[stop_loss]
volatility_mean = 0.015  # XRP 波动率较高
volatility_std = 0.005
change_mean = 0.003
change_std = 0.008
volatility_threshold = 3.0
change_threshold = 3.0
```

### 步骤 2: 更新 cluster.toml

添加 collector symbols:

```toml
[collector]
symbols = ["btcusdt", "ethusdt", "xrpusdt"]  # 添加 xrpusdt
```

添加 bot 配置:

```toml
[[bots]]
id = "obi_mm_xrpusdt_bf"
enabled = true
strategy = "obi_mm_testnet"
symbol = "xrpusdt"                        # Binance: 小写
connector = "bf"
config_file = "/etc/hft/strategies/obi_mm_xrp.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

### 步骤 3: 应用配置

```bash
docker compose restart hft
```

### 步骤 4: 验证

```bash
# 检查状态
docker compose exec hft hftctl --socket /tmp/hft-supervisor.sock status

# 查看日志
docker compose exec hft tail -f /var/lib/hft/logs/bots/obi_mm_xrpusdt_bf.log
```

## 管理命令

### hftctl 命令

```bash
# 进入容器
docker compose exec hft bash

# 或直接执行命令
docker compose exec hft hftctl --socket /tmp/hft-supervisor.sock <command>
```

| 命令 | 说明 |
|------|------|
| `status` | 查看集群状态 |
| `start <bot_id>` | 启动指定 bot |
| `stop <bot_id>` | 停止指定 bot |
| `restart <bot_id>` | 重启指定 bot |
| `infra start connector` | 启动连接器 |
| `infra stop collector` | 停止数据收集器 |

### Docker Compose 命令

```bash
# 启动服务
docker compose up -d

# 停止服务
docker compose down

# 重启 HFT 容器 (重载配置)
docker compose restart hft

# 重新构建
docker compose build hft
docker compose up -d

# 查看日志
docker compose logs -f hft

# 进入容器
docker compose exec hft bash
```

## 查看日志

### 位置

```
/var/lib/hft/logs/
├── infra/
│   ├── connector_bf.log      # Binance 连接器
│   ├── connector_bybit.log   # Bybit 连接器
│   ├── collector.log         # 数据收集器
│   ├── order_recorder.log    # 订单记录器
│   └── server.log            # Web 服务器
└── bots/
    ├── obi_mm_btcusdt_bf.log
    ├── obi_mm_ethusdt_bf.log
    └── ...
```

### 查看命令

```bash
# 连接器日志
docker compose exec hft tail -f /var/lib/hft/logs/infra/connector_bf.log

# Bot 日志
docker compose exec hft tail -f /var/lib/hft/logs/bots/obi_mm_btcusdt_bf.log

# 最近错误
docker compose exec hft grep -i error /var/lib/hft/logs/infra/connector_bf.log | tail -20
```

### 日志级别

在 `.env` 中设置:

```bash
RUST_LOG=info           # 默认
RUST_LOG=debug          # 详细调试
RUST_LOG=warn           # 只看警告和错误
```

## Web Dashboard

访问 `http://localhost:13000`

- 默认账号: `admin`
- 默认密码: `admin`

### REST API

```bash
# 健康检查
curl http://localhost:13000/api/health

# 列出所有 bots
curl http://localhost:13000/api/bots

# 获取 bot 状态
curl http://localhost:13000/api/bots/obi_mm_btcusdt_bf

# 启动/停止/重启 bot
curl -X POST http://localhost:13000/api/bots/obi_mm_btcusdt_bf/start
curl -X POST http://localhost:13000/api/bots/obi_mm_btcusdt_bf/stop
curl -X POST http://localhost:13000/api/bots/obi_mm_btcusdt_bf/restart

# 查看日志 (最后 100 行)
curl "http://localhost:13000/api/logs/obi_mm_btcusdt_bf?lines=100"
```

## 测试网 vs 主网

### 当前默认: 测试网

- **Binance Futures Testnet**: `https://demo-fapi.binance.com`
- **Bybit Demo Trading**: `https://api-demo.bybit.com`

### 切换到主网

编辑 `config/binancefutures.toml`:

```toml
# 测试网 (默认)
stream_url = "wss://fstream.binancefuture.com/ws"
api_url = "https://demo-fapi.binance.com"

# 主网 (谨慎使用!)
# stream_url = "wss://fstream.binance.com/ws"
# api_url = "https://fapi.binance.com"
```

编辑 `config/bybit.toml`:

```toml
# Demo Trading (默认)
rest_url = "https://api-demo.bybit.com"

# 主网 (谨慎使用!)
# rest_url = "https://api.bybit.com"
```

**警告**: 主网会使用真实资金，请确保充分测试后再切换!

## 常见问题

### Bot 卡在 "waiting_warmup"

数据收集器需要时间收集市场数据。等待 60 秒以上。

检查 collector 是否正常:
```bash
docker compose exec hft cat /var/lib/hft/logs/infra/collector.log
```

### "Connection refused" 错误

检查连接器是否正常:
```bash
docker compose exec hft hftctl status
docker compose exec hft cat /var/lib/hft/logs/infra/connector_bf.log
```

### API 密钥错误

1. 检查 `.env` 文件格式
2. 确保没有多余空格
3. 重启容器: `docker compose restart hft`

### 端口冲突

默认端口映射:
- 13000: Web Dashboard
- 18123: ClickHouse HTTP
- 19000: ClickHouse Native

如有冲突，编辑 `docker-compose.yml` 修改端口映射。

## 下一步

- 了解不同交易所的详细配置：[交易所配置参考](05-exchange-configs.md)
- 查看 Grafana 监控面板配置
- 设置告警通知
