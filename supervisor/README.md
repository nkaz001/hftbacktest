# HFT Supervisor

多Bot进程管理系统，用于管理 HFT 交易策略的生命周期。

## 目录

- [快速开始](#快速开始)
- [测试网完整示例](#测试网完整示例)
- [配置说明](#配置说明)
- [命令行工具](#命令行工具)
- [多策略管理](#多策略管理)
- [配置热重载](#配置热重载)
- [故障排查](#故障排查)

---

## 快速开始

### 1. 编译

```bash
# 编译所有组件
cargo build --release -p connector -p collector -p hft-supervisor

# 编译策略（以 obi_mm 为例）
cargo build --release -p obi_mm

# 确认二进制文件
ls -la target/release/{connector,collector,hft-supervisor,hftctl,obi_mm_testnet}
```

### 2. 验证配置

```bash
./target/release/hftctl validate examples/testnet/config/cluster_testnet.toml
```

---

## 测试网完整示例

本节演示如何在 Binance Futures Testnet 上启动 OBI MM 策略。

### 前置准备

1. **注册测试网账户**: https://testnet.binancefuture.com/
2. **获取 API Key**: 在测试网控制台创建 API Key
3. **配置 API Key**:

```bash
# 编辑 connector 配置
vim examples/testnet/config/binancefutures_testnet.toml
```

```toml
stream_url = "wss://fstream.binancefuture.com/ws"
api_url = "https://demo-fapi.binance.com"
order_prefix = "test"
api_key = "YOUR_API_KEY"
secret = "YOUR_SECRET"
```

### 步骤 1: 启动 Connector

Connector 负责与交易所建立连接，提供行情数据和订单执行。

```bash
# 启动 Binance Futures Testnet Connector
./target/release/connector \
  --name bf \
  --connector binancefutures \
  --config examples/testnet/config/binancefutures_testnet.toml
```

保持此终端运行，打开新终端继续。

### 步骤 2: 启动 Collector (可选)

Collector 录制行情数据，用于策略预热。

```bash
# 通过 IPC 从 Connector 收集数据
./target/release/collector \
  examples/testnet/data \
  --ipc bf \
  btcusdt
```

数据将保存为 `examples/testnet/data/btcusdt_YYYYMMDD.jsonl`。

### 步骤 3: 配置 cluster.toml

```bash
# 查看/编辑集群配置
vim examples/testnet/config/cluster_testnet.toml
```

```toml
[cluster]
name = "testnet-binance-futures"
data_dir = "/home/b0qi/bookdisco/hftbacktest/examples/testnet/data"
log_dir = "/home/b0qi/bookdisco/hftbacktest/examples/testnet/logs"
socket_path = "/tmp/hft-supervisor-testnet.sock"

[connector]
name = "bf"
type = "binancefutures"
config_file = "/home/b0qi/bookdisco/hftbacktest/examples/testnet/config/binancefutures_testnet.toml"

[collector]
enabled = true
mode = "ipc"
output_dir = "/home/b0qi/bookdisco/hftbacktest/examples/testnet/data"
symbols = ["btcusdt"]

[warmup]
data_dir = "/home/b0qi/bookdisco/hftbacktest/examples/testnet/data"
max_age_seconds = 86400
min_duration_seconds = 60

[[bots]]
id = "obi_mm_btcusdt_testnet"
enabled = true
strategy = "obi_mm_testnet"
symbol = "btcusdt"
config_file = "/home/b0qi/bookdisco/hftbacktest/examples/testnet/config/obi_mm_testnet.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

### 步骤 4: 启动 Supervisor

```bash
./target/release/hft-supervisor \
  --config examples/testnet/config/cluster_testnet.toml \
  --bin-dir ./target/release
```

Supervisor 会自动启动所有 `enabled = true` 的 Bot。

### 步骤 5: 管理 Bot

```bash
# 定义别名简化命令
alias hftctl='./target/release/hftctl --socket /tmp/hft-supervisor-testnet.sock'

# 查看状态
hftctl status

# 停止 Bot
hftctl stop obi_mm_btcusdt_testnet

# 启动 Bot
hftctl start obi_mm_btcusdt_testnet

# 重启 Bot
hftctl restart obi_mm_btcusdt_testnet
```

### 步骤 6: 查看日志

```bash
# Bot 日志
tail -f examples/testnet/logs/bots/obi_mm_btcusdt_testnet.log

# 实时查看策略运行状态
watch -n 1 'hftctl status'
```

---

## 配置说明

### cluster.toml 结构

```toml
[cluster]
name = "cluster-name"           # 集群名称
data_dir = "/path/to/data"      # 数据目录
log_dir = "/path/to/logs"       # 日志目录
socket_path = "/path/to.sock"   # Unix Socket 路径
config_reload_interval_secs = 30 # 配置检查间隔

[connector]
name = "bf"                     # Connector 名称 (用于 IPC)
type = "binancefutures"         # 类型: binancefutures, binancespot, bybit
config_file = "/path/to/connector.toml"

[collector]
enabled = true                  # 是否启用
mode = "ipc"                    # 收集模式: ipc 或 ws
output_dir = "/path/to/raw"     # 数据输出目录
symbols = ["btcusdt", "ethusdt"] # 收集的交易对
retention_days = 30             # 数据保留天数

[warmup]
data_dir = "/path/to/raw"       # 预热数据目录
max_age_seconds = 7200          # 数据最大年龄 (2小时)
min_duration_seconds = 3600     # 最小数据时长 (1小时)

[bot_defaults]                  # Bot 默认配置
tick_size = 0.1
lot_size = 0.001

[[bots]]                        # Bot 配置数组
id = "unique_bot_id"            # 唯一标识符
enabled = true                  # 是否启用
strategy = "obi_mm_testnet"     # 策略二进制名称
symbol = "btcusdt"              # 交易对
config_file = "/path/to/strategy.toml"  # 策略配置文件
auto_restart = true             # 崩溃后自动重启
max_restarts = 5                # 最大重启次数
restart_backoff_secs = 10       # 重启间隔
```

### 策略配置示例 (obi_mm_testnet.toml)

```toml
[global]
order_qty = 0.002
max_position = 0.02
grid_num = 3
grid_interval = 1.0

[params]
looking_depth = 0.001
window = 5
half_spread = 0.0005
skew = 0.01

[stop_loss]
volatility_threshold = 2.0
change_threshold = 2.0
```

---

## 命令行工具

### hftctl 命令

```bash
# 验证配置文件
hftctl validate /path/to/cluster.toml

# 查看集群状态
hftctl --socket /path/to/socket status

# 查看单个 Bot 状态
hftctl --socket /path/to/socket status --bot obi_mm_btcusdt

# 启动 Bot
hftctl --socket /path/to/socket start <bot_id>

# 停止 Bot
hftctl --socket /path/to/socket stop <bot_id>

# 重启 Bot
hftctl --socket /path/to/socket restart <bot_id>

# 重启所有 Bot
hftctl --socket /path/to/socket restart-all

# 重载配置
hftctl --socket /path/to/socket reload
```

### hft-supervisor 参数

```bash
hft-supervisor \
  --config /path/to/cluster.toml \  # 集群配置文件
  --bin-dir /path/to/binaries \     # 策略二进制目录
  --json-log \                      # 输出 JSON 格式日志
  --start-on-boot                   # 启动时自动启动所有 Bot (默认 true)
```

---

## 多策略管理

### 配置多个 Bot

在 `cluster.toml` 中添加多个 `[[bots]]` 段：

```toml
[[bots]]
id = "obi_mm_btcusdt"
enabled = true
strategy = "obi_mm_testnet"
symbol = "btcusdt"
config_file = "/path/to/obi_mm_btcusdt.toml"

[[bots]]
id = "obi_mm_ethusdt"
enabled = true
strategy = "obi_mm_testnet"
symbol = "ethusdt"
config_file = "/path/to/obi_mm_ethusdt.toml"

[[bots]]
id = "obi_mm_solusdt"
enabled = false  # 暂时禁用
strategy = "obi_mm_testnet"
symbol = "solusdt"
config_file = "/path/to/obi_mm_solusdt.toml"
```

### 管理多个 Bot

```bash
# 查看所有 Bot 状态
hftctl status
# 输出:
# Cluster: testnet-binance-futures (uptime: 1h 23m)
#
# ●  Connector: running
# ●  Collector: running
#
# Bots:
#   ●  obi_mm_btcusdt: running
#   ●  obi_mm_ethusdt: running
#   ○  obi_mm_solusdt: stopped

# 启动特定 Bot
hftctl start obi_mm_solusdt

# 停止所有 Bot
hftctl stop obi_mm_btcusdt
hftctl stop obi_mm_ethusdt

# 重启所有 Bot
hftctl restart-all
```

### 不同策略类型

```toml
# 做市策略
[[bots]]
id = "mm_btcusdt"
strategy = "obi_mm_testnet"
symbol = "btcusdt"
config_file = "/path/to/mm_config.toml"

# 网格策略 (如果有)
[[bots]]
id = "grid_ethusdt"
strategy = "grid_trading"
symbol = "ethusdt"
config_file = "/path/to/grid_config.toml"
```

---

## 配置热重载

Supervisor 支持两种配置重载方式：

### 方式 1: 文件监视 (自动)

编辑 `cluster.toml` 后，Supervisor 会自动检测变更并重载。

```bash
# 编辑配置
vim examples/testnet/config/cluster_testnet.toml

# 例如：禁用一个 Bot
# enabled = true -> enabled = false

# Supervisor 日志会显示:
# INFO Configuration file changed, reloading...
# INFO Configuration changed, reloading bots changed=["obi_mm_btcusdt"]
```

### 方式 2: SIGHUP 信号 (手动)

```bash
# 发送 SIGHUP 强制重载
kill -HUP $(pgrep hft-supervisor)

# 或使用 hftctl
hftctl reload
```

### 重载行为

| 变更类型 | 行为 |
|---------|------|
| 修改 `enabled` | 启动/停止对应 Bot |
| 修改 `config_file` | 重启 Bot |
| 修改 `strategy` | 重启 Bot |
| 添加新 Bot | 如果 enabled=true，自动启动 |
| 删除 Bot | 停止并移除 |

### 修改策略参数

策略配置文件 (如 `obi_mm_testnet.toml`) 的修改需要重启 Bot：

```bash
# 1. 编辑策略配置
vim examples/testnet/config/obi_mm_testnet.toml

# 2. 重启 Bot 使配置生效
hftctl restart obi_mm_btcusdt_testnet
```

---

## 故障排查

### 常见问题

#### 1. Bot 启动失败

```bash
# 检查 Bot 日志
tail -50 examples/testnet/logs/bots/obi_mm_btcusdt_testnet.log

# 常见原因:
# - 策略二进制不存在
# - 配置文件路径错误
# - Connector 未运行
```

#### 2. 无法连接到 Supervisor

```bash
# 检查 socket 文件是否存在
ls -la /tmp/hft-supervisor-testnet.sock

# 检查 Supervisor 是否运行
ps aux | grep hft-supervisor
```

#### 3. Warmup 数据不足

```bash
# 检查数据文件
ls -la examples/testnet/data/

# Bot 日志会显示:
# INFO Waiting for warmup data...

# 解决方案: 等待 Collector 收集足够数据，或降低 warmup 要求
```

#### 4. Connector 未运行

```bash
# 检查 Connector 进程
ps aux | grep connector

# 检查 IPC 服务
ls /dev/shm/ | grep iox
```

### 日志级别

```bash
# 启动时设置日志级别
RUST_LOG=debug ./target/release/hft-supervisor ...

# 日志级别: error, warn, info, debug, trace
```

### 优雅关闭

```bash
# 发送 SIGTERM 优雅关闭
kill -TERM $(pgrep hft-supervisor)

# Supervisor 会:
# 1. 停止接受新命令
# 2. 向所有 Bot 发送 SIGTERM
# 3. 等待 Bot 优雅退出 (最多 30 秒)
# 4. 清理资源后退出
```

---

## 生产部署

参考 `examples/systemd/` 目录下的 systemd 服务文件：

```bash
# 安装服务
sudo cp examples/systemd/*.service /etc/systemd/system/
sudo systemctl daemon-reload

# 启动服务
sudo systemctl start hft-connector
sudo systemctl start hft-collector
sudo systemctl start hft-supervisor

# 开机自启
sudo systemctl enable hft-connector hft-collector hft-supervisor

# 查看状态
sudo systemctl status hft-supervisor
```

---

## 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                      hft-supervisor                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Config      │  │ Manager     │  │ Control Server      │  │
│  │ Watcher     │  │ (lifecycle) │  │ (Unix Socket)       │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│         │    ┌───────────┴───────────┐        │             │
│         │    │                       │        │             │
│         ▼    ▼                       ▼        │             │
│    ┌─────────────┐             ┌─────────────┐│             │
│    │   Bot 1     │             │   Bot N     ││             │
│    │ (process)   │             │ (process)   ││             │
│    └──────┬──────┘             └──────┬──────┘│             │
└───────────┼────────────────────────────┼──────┼─────────────┘
            │                            │      │
            │         IPC (iceoryx2)     │      │ hftctl
            ▼                            ▼      ▼
     ┌─────────────┐              ┌─────────────┐
     │  Connector  │              │   User      │
     │  (exchange) │              │  (terminal) │
     └─────────────┘              └─────────────┘
```
