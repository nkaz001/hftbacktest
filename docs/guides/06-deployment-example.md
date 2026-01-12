# 实战示例：部署 LINKUSDT 策略

本文档记录了实际部署 LINKUSDT 策略的完整过程，包括遇到的问题和解决方案。

## 部署概述

**目标**: 在 Binance Futures 测试网上部署 LINK/USDT 做市策略

**起始状态**: 系统已运行 BTC, ETH, SOL 策略

## 第一步：创建策略配置文件

```bash
# 复制现有配置作为模板
cp config/strategies/obi_mm_btc.toml config/strategies/obi_mm_link.toml
```

编辑 `config/strategies/obi_mm_link.toml`:

```toml
# OBI MM Strategy Configuration - LINK/USDT
# Optimized for Chainlink trading

[global]
order_qty = 3.0          # ~$60 per order at $20 LINK
max_position = 30.0      # ~$600 max position
grid_num = 2             # 2 orders each side
grid_interval = 0.01     # $0.01 grid spacing
roi_lb = 0.0
roi_ub = 200.0           # LINK price range

[params]
looking_depth = 0.001    # 0.1% depth for OBI calculation
window = 5               # Z-score window
volatility_window = 2    # Volatility window
half_spread = 0.0008     # 8 bps half spread (LINK more volatile)
skew = 0.01              # Position skew factor
c1 = 0.0001              # OBI impact coefficient
power = 1.0              # Volatility power
live_seconds = 30.0      # Order lifetime
update_interval_ns = 5000000000      # 5 seconds
volatility_interval_ns = 10000000000 # 10 seconds

[stop_loss]
volatility_mean = 0.010  # LINK is more volatile than BTC
volatility_std = 0.004
change_mean = 0.002
change_std = 0.006
volatility_threshold = 3.0
change_threshold = 3.0
```

**关键参数说明**:
- `order_qty = 3.0`: LINK 价格约 $20，3 LINK ≈ $60
- `roi_ub = 200.0`: LINK 价格范围上限
- `half_spread = 0.0008`: 比 BTC (0.0005) 更宽，因为 LINK 波动更大
- `volatility_mean = 0.010`: 比 BTC (0.005) 更高的基准波动率

## 第二步：更新 cluster.toml

### 2.1 添加 collector symbols

```toml
[collector]
symbols = ["btcusdt", "ethusdt", "solusdt", "linkusdt"]  # 添加 linkusdt
```

### 2.2 添加 bot 配置

```toml
[[bots]]
id = "obi_mm_linkusdt_bf"
enabled = true
strategy = "obi_mm_testnet"
symbol = "linkusdt"          # Binance 用小写
connector = "bf"
config_file = "/etc/hft/strategies/obi_mm_link.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

## 第三步：应用配置

```bash
docker compose restart hft
```

## 第四步：验证部署

### 4.1 检查集群状态

```bash
$ docker compose exec hft hftctl --socket /tmp/hft-supervisor.sock status

Cluster: hft-docker (uptime: 16s)

Connectors:
●  bf: running
●  bybit: running

●  Collector: running

Bots:
  ●  obi_mm_solusdt_bybit: running (8s) [pid 159]
  ◐  obi_mm_linkusdt_bf: waiting_warmup    <-- 新 bot 正在预热
  ●  obi_mm_btcusdt_bf: running (8s) [pid 160]
  ●  obi_mm_ethusdt_bf: running (8s) [pid 161]
  ●  obi_mm_btcusdt_bybit: running (8s) [pid 162]
```

### 4.2 检查 connector 日志

```bash
$ docker compose exec hft cat /var/lib/hft/logs/infra/connector_bf.log | grep -i link

2026-01-12T07:32:32.082837Z INFO connector: Received RegisterInstrument request from bot symbol=linkusdt tick_size=0.01 lot_size=0.001
2026-01-12T07:32:32.082935Z INFO connector::binancefutures::orderbookmanager: Order book reset to initializing state symbol=linkusdt
2026-01-12T07:32:32.082941Z INFO connector::binancefutures::market_data_stream: Subscribed to trade and depth streams symbol=linkusdt
```

### 4.3 检查 collector 日志

```bash
$ docker compose exec hft tail -20 /var/lib/hft/logs/infra/collector.log

2026-01-12T07:32:32.080079Z INFO collector::ipc_collector: Starting IPC collection connector_name=bf symbols=["btcusdt", "ethusdt", "solusdt", "linkusdt"]
2026-01-12T07:32:32.082780Z INFO collector::ipc_collector: Registering symbol with connector symbol=linkusdt
...
2026-01-12T07:35:58.110912Z INFO collector::ipc_collector: Events received count=20000
```

### 4.4 检查数据文件

```bash
$ docker compose exec hft ls -la /var/lib/hft/data/link*

-rw-r--r-- 1 hft hft 326255 Jan 12 07:38 /var/lib/hft/data/linkusdt_20260112.jsonl
```

## 注意事项：Warmup 等待时间

新添加的交易对需要等待数据收集完成才能开始交易。这是因为：

1. **Collector 需要时间收集数据**: 新交易对之前没有历史数据
2. **策略需要预热数据**: `warmup.min_duration_seconds = 60` 要求至少 60 秒数据
3. **因子需要预热**: OBI 和波动率因子需要填满窗口才能产生有效信号

**预期等待时间**: 约 1-2 分钟

**状态变化**:
```
waiting_warmup -> running
```

### 加速预热的方法

1. **减小因子窗口** (测试环境):
```toml
[params]
window = 5               # 减小 OBI 窗口
volatility_window = 2    # 减小波动率窗口
```

2. **使用历史数据** (如果有):
将历史 `.gz` 数据文件放入 `/var/lib/hft/data/` 目录

## 常见问题

### Bot 一直显示 waiting_warmup

**原因**: 没有足够的 warmup 数据

**解决**:
1. 等待 collector 收集足够数据 (至少 `min_duration_seconds`)
2. 检查 collector 是否正常运行: `hftctl status`
3. 检查数据文件是否在增长

### Symbol 订阅失败

**检查 connector 日志**:
```bash
docker compose exec hft grep -i "error\|failed" /var/lib/hft/logs/infra/connector_bf.log
```

**常见原因**:
- Symbol 名称错误 (Binance 要求小写)
- 交易所不支持该交易对
- API 连接问题

### Order book 同步问题

**现象**: 日志中有 "Gap detected during snapshot processing"

**说明**: 这是正常现象，connector 会自动重试获取最新快照

```
WARN Gap between snapshot and first event! Need to re-fetch snapshot symbol=linkusdt
```

## 完整配置文件参考

### cluster.toml (新增部分)

```toml
# collector symbols 添加 linkusdt
[collector]
symbols = ["btcusdt", "ethusdt", "solusdt", "linkusdt"]

# 新增 LINK bot
[[bots]]
id = "obi_mm_linkusdt_bf"
enabled = true
strategy = "obi_mm_testnet"
symbol = "linkusdt"
connector = "bf"
config_file = "/etc/hft/strategies/obi_mm_link.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

### 文件清单

部署新交易对需要创建/修改的文件:

| 文件 | 操作 | 说明 |
|------|------|------|
| `config/strategies/obi_mm_link.toml` | 新建 | 策略参数配置 |
| `config/cluster.toml` | 修改 | 添加 collector symbols 和 bot 配置 |

## 下一步

- 等待 bot 状态变为 `running`
- 检查 bot 日志确认订单正常提交
- 在 Grafana 监控面板查看 PnL
