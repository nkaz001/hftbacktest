# 策略 Bot 集成指南

本指南详细说明如何将新的交易策略集成到 HFT 管理系统中。

## 目录

1. [概述](#概述)
2. [快速开始](#快速开始)
3. [详细步骤](#详细步骤)
4. [配置说明](#配置说明)
5. [API 参考](#api-参考)
6. [最佳实践](#最佳实践)
7. [故障排除](#故障排除)

---

## 概述

### 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                      Dashboard (React)                       │
│                    http://localhost:5175                     │
└─────────────────────────┬───────────────────────────────────┘
                          │ REST API
┌─────────────────────────▼───────────────────────────────────┐
│                    HFT Server (Axum)                         │
│                  http://localhost:3000                       │
└─────────────────────────┬───────────────────────────────────┘
                          │ Unix Socket IPC
┌─────────────────────────▼───────────────────────────────────┐
│                    HFT Supervisor                            │
│              /tmp/hft-supervisor.sock                        │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Connector  │  │  Collector  │  │   Order Recorder    │  │
│  │    (bf)     │  │             │  │                     │  │
│  └──────┬──────┘  └─────────────┘  └─────────────────────┘  │
│         │ iceoryx2 IPC                                       │
│  ┌──────▼──────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Bot 1     │  │   Bot 2     │  │       Bot N         │  │
│  │ (策略实例)  │  │ (策略实例)  │  │     (策略实例)      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 组件说明

| 组件 | 作用 |
|------|------|
| Dashboard | Web 管理界面，用于监控和控制 |
| Server | REST API 服务，处理认证和请求转发 |
| Supervisor | 进程管理器，负责启动/停止/监控所有组件 |
| Connector | 交易所连接器，处理行情订阅和订单执行 |
| Collector | 数据采集器，将行情数据保存到本地 |
| Order Recorder | 订单记录器，将交易数据写入 ClickHouse |
| Bot | 策略实例，执行具体的交易逻辑 |

---

## 快速开始

### 1. 复制模板

```bash
cd /path/to/hftbacktest/strategies

# 复制模板目录
cp -r template_strategy my_strategy

# 重命名包名
cd my_strategy
sed -i 's/template_strategy/my_strategy/g' Cargo.toml
sed -i 's/template_strategy/my_strategy/g' src/main.rs
```

### 2. 修改策略逻辑

编辑 `src/main.rs`，修改 `calculate_quotes()` 函数实现你的策略逻辑。

### 3. 编译

```bash
cd /path/to/hftbacktest
cargo build --release -p my_strategy
```

### 4. 创建符号链接

```bash
sudo ln -sf /path/to/hftbacktest/target/release/my_strategy /opt/hft/bin/my_strategy
```

### 5. 创建配置文件

```bash
cp strategies/my_strategy/config.example.toml examples/testnet/config/my_strategy.toml
# 编辑配置文件，调整参数
```

### 6. 添加到集群配置

编辑 `examples/testnet/config/cluster_testnet.toml`：

```toml
[[bots]]
id = "my_strategy_btcusdt"
enabled = true
strategy = "my_strategy"
symbol = "btcusdt"
config_file = "/path/to/examples/testnet/config/my_strategy.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

### 7. 启动

通过 Dashboard 或 API 启动：

```bash
# API 方式
curl -X POST http://localhost:3000/api/bots/my_strategy_btcusdt/start \
  -H "Authorization: Bearer $TOKEN"
```

---

## 详细步骤

### 步骤 1: 创建策略项目

#### 目录结构

```
strategies/my_strategy/
├── Cargo.toml              # 依赖配置
├── config.example.toml     # 配置模板
└── src/
    └── main.rs             # 策略代码
```

#### Cargo.toml 必需依赖

```toml
[dependencies]
# 核心库 - 必需
hftbacktest = { path = "../../hftbacktest", features = ["live"] }
clap = { version = "4", features = ["derive"] }
toml = "0.8"
serde = { version = "1", features = ["derive"] }
tracing = "0.1"
anyhow = "1"

# 可选依赖
factor-engine = { path = "../../factor-engine" }      # 因子计算
order-recorder = { path = "../../order-recorder" }    # 订单记录
tokio = { version = "1", features = ["full"] }        # 异步运行时
```

### 步骤 2: 实现策略代码

#### 必需的 CLI 参数

策略必须支持以下命令行参数（Supervisor 会传递这些参数）：

```rust
#[derive(Parser)]
struct Args {
    /// 配置文件路径 - 必需
    #[arg(long)]
    config: String,

    /// 交易品种 - 必需
    #[arg(long)]
    symbol: String,
}
```

#### 策略主循环模板

```rust
fn run_strategy(args: &Args, config: &Config) -> Result<()> {
    // 1. 构建 LiveBot
    let mut bot: LiveBot<HashMapMarketDepth> = LiveBotBuilder::new()
        .register("bf", &args.symbol)  // connector 名称
        .depth(|builder| builder.with_hash_map_market_depth())
        .build()?;

    // 2. 主循环
    loop {
        // 处理事件
        bot.elapse(Duration::from_millis(100))?;

        // 获取市场数据
        let depth = bot.depth(0);
        let best_bid = depth.best_bid();
        let best_ask = depth.best_ask();

        // 获取持仓
        let position = bot.position(0);

        // 计算报价（你的策略逻辑）
        let (bid_price, ask_price) = calculate_quotes(...);

        // 提交订单
        bot.submit_buy_order(0, 0, bid_price, qty, TimeInForce::GTX, OrdType::Limit, false)?;
        bot.submit_sell_order(0, 0, ask_price, qty, TimeInForce::GTX, OrdType::Limit, false)?;
    }
}
```

### 步骤 3: 配置文件

#### 配置文件位置

- 开发/测试: `examples/testnet/config/my_strategy.toml`
- 生产: `config/my_strategy.toml`

#### 配置文件结构

```toml
[global]
order_qty = 0.002          # 每单数量
max_position = 0.02        # 最大持仓
tick_size = 0.1            # 价格精度
lot_size = 0.001           # 数量精度

[params]
# 策略特定参数
half_spread = 0.0005       # 半边价差
skew = 0.01                # 持仓偏移系数

[risk]
kill_switch = false        # 紧急停止开关
```

### 步骤 4: 集群配置

#### 在 cluster.toml 中添加 Bot

```toml
[[bots]]
id = "my_strategy_btcusdt"           # 唯一标识符
enabled = true                        # 是否启用
strategy = "my_strategy"              # 二进制文件名 (在 /opt/hft/bin/ 下)
symbol = "btcusdt"                    # 交易品种
config_file = "/path/to/config.toml" # 策略配置文件路径
auto_restart = true                   # 崩溃后自动重启
max_restarts = 3                      # 最大重启次数
restart_backoff_secs = 5              # 重启间隔
```

#### 配置字段说明

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| id | string | 是 | 唯一标识符，用于 API 调用 |
| enabled | bool | 是 | 是否启用此 bot |
| strategy | string | 是 | 二进制文件名 |
| symbol | string | 是 | 交易品种（小写） |
| config_file | string | 是 | 策略配置文件绝对路径 |
| auto_restart | bool | 否 | 默认 false |
| max_restarts | u32 | 否 | 默认 3 |
| restart_backoff_secs | u64 | 否 | 默认 5 |

### 步骤 5: 编译和部署

```bash
# 编译
cargo build --release -p my_strategy

# 创建符号链接
sudo ln -sf $(pwd)/target/release/my_strategy /opt/hft/bin/my_strategy

# 验证
ls -la /opt/hft/bin/my_strategy
/opt/hft/bin/my_strategy --help
```

---

## 配置说明

### Supervisor 启动参数

当 Supervisor 启动 Bot 时，会传递以下参数：

```bash
/opt/hft/bin/my_strategy --config /path/to/config.toml --symbol btcusdt
```

### 日志输出

Bot 的 stdout 和 stderr 会被重定向到日志文件：

```
logs/bots/my_strategy_btcusdt.log
```

### 环境变量

可用的环境变量：

| 变量 | 说明 |
|------|------|
| RUST_LOG | 日志级别 (debug, info, warn, error) |
| RUST_BACKTRACE | 启用 backtrace (1 或 full) |

---

## API 参考

### Bot 管理 API

#### 列出所有 Bots

```bash
GET /api/bots
Authorization: Bearer <token>

# 响应
[
  {
    "id": "my_strategy_btcusdt",
    "symbol": "btcusdt",
    "state": "running",
    "pid": 12345,
    "uptime_secs": 3600,
    "restart_count": 0
  }
]
```

#### 获取单个 Bot 状态

```bash
GET /api/bots/:id
Authorization: Bearer <token>
```

#### 启动 Bot

```bash
POST /api/bots/:id/start
Authorization: Bearer <token>

# 响应
{
  "success": true,
  "bot_id": "my_strategy_btcusdt",
  "action": "start",
  "message": "Bot start command sent"
}
```

#### 停止 Bot

```bash
POST /api/bots/:id/stop
Authorization: Bearer <token>
```

#### 重启 Bot

```bash
POST /api/bots/:id/restart
Authorization: Bearer <token>
```

### 配置管理 API

#### 获取策略配置

```bash
GET /api/config/strategies/:bot_id
Authorization: Bearer <token>
```

#### 获取原始 TOML

```bash
GET /api/config/strategies/:bot_id/raw
Authorization: Bearer <token>
```

#### 更新策略配置（会自动重启 Bot）

```bash
PUT /api/config/strategies/:bot_id
Authorization: Bearer <token>
Content-Type: application/json

{
  "content": "[global]\norder_qty = 0.003\n..."
}
```

---

## 最佳实践

### 1. 错误处理

```rust
// 使用 anyhow 进行错误传播
use anyhow::{Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path))?;
    // ...
}
```

### 2. 日志规范

```rust
use tracing::{info, warn, error, debug};

// 使用结构化日志
info!(symbol = %args.symbol, "Strategy started");
warn!(order_id = %id, reason = %e, "Order rejected");
error!(error = %e, "Fatal error, shutting down");
```

### 3. 优雅关闭

```rust
use tokio::signal;

// 处理 SIGTERM
tokio::select! {
    _ = signal::ctrl_c() => {
        info!("Received SIGINT, shutting down");
    }
    _ = run_strategy() => {}
}

// 清理资源
bot.cancel_all(0)?;
```

### 4. 配置验证

```rust
fn validate_config(config: &Config) -> Result<()> {
    if config.global.order_qty <= 0.0 {
        anyhow::bail!("order_qty must be positive");
    }
    if config.global.order_qty < config.global.lot_size {
        anyhow::bail!("order_qty must be >= lot_size");
    }
    Ok(())
}
```

### 5. 风险控制

```rust
// 在主循环中检查风险限制
if position.abs() >= config.global.max_position {
    warn!("Position limit reached: {}", position);
    // 只允许减仓订单
}

if config.risk.kill_switch {
    warn!("Kill switch enabled, cancelling all orders");
    bot.cancel_all(0)?;
    continue;
}
```

---

## 故障排除

### 问题：Bot 显示 "stopped" 状态

**可能原因：**
1. 二进制文件不存在
2. 配置文件路径错误
3. 权限问题

**排查步骤：**
```bash
# 检查二进制文件
ls -la /opt/hft/bin/my_strategy

# 检查配置文件
cat /path/to/config.toml

# 查看日志
tail -f logs/bots/my_strategy_btcusdt.log
```

### 问题：Bot 启动后立即崩溃

**可能原因：**
1. 配置文件格式错误
2. Connector 未启动
3. IPC 连接失败

**排查步骤：**
```bash
# 直接运行查看错误
/opt/hft/bin/my_strategy --config /path/to/config.toml --symbol btcusdt

# 检查 Connector 状态
curl http://localhost:3000/api/infra/connector -H "Authorization: Bearer $TOKEN"
```

### 问题：无法连接到 Connector

**可能原因：**
1. Connector 未启动
2. IPC 服务名称不匹配
3. iceoryx2 资源问题

**排查步骤：**
```bash
# 启动 Connector
curl -X POST http://localhost:3000/api/infra/connector/start -H "Authorization: Bearer $TOKEN"

# 清理 IPC 资源（如果需要）
sudo rm -rf /dev/shm/iox2_*
```

### 问题：订单被拒绝

**可能原因：**
1. 价格精度错误
2. 数量精度错误
3. 资金不足
4. 超出限制

**排查步骤：**
```bash
# 查看 Bot 日志
tail -f logs/bots/my_strategy_btcusdt.log | grep -i "reject\|error"

# 检查交易所规则
# Binance Futures BTCUSDT:
# - tick_size: 0.1
# - lot_size: 0.001
# - min_notional: $100
```

---

## 附录

### 完整示例：OBI 做市策略

参考 `strategies/obi_mm_rust/` 目录下的完整实现：

- `src/main.rs` - 主入口和策略循环
- `src/config.rs` - 配置结构定义
- `src/strategy.rs` - 策略逻辑实现

### 有用的链接

- [HFT Backtest 文档](../README.md)
- [Factor Engine 设计](./FACTOR_FRAMEWORK_DESIGN.md)
- [Live Trading 架构](./LIVE_TRADING_ARCHITECTURE.md)
- [Supervisor 设计](./SUPERVISOR_DESIGN.md)

### 交易所特定信息

#### Binance Futures

| 品种 | tick_size | lot_size | min_notional |
|------|-----------|----------|--------------|
| BTCUSDT | 0.1 | 0.001 | $100 |
| ETHUSDT | 0.01 | 0.001 | $100 |

#### Bybit

| 品种 | tick_size | lot_size | min_notional |
|------|-----------|----------|--------------|
| BTCUSDT | 0.5 | 0.001 | $5 |
| ETHUSDT | 0.05 | 0.01 | $5 |

---

## 多 Bot 管理

### 配置多个 Bot

在 `cluster.toml` 中添加多个 `[[bots]]` 条目：

```toml
# Bot 1: BTC 做市策略
[[bots]]
id = "mm_btcusdt"
enabled = true
strategy = "obi_mm"
symbol = "btcusdt"
config_file = "/path/to/config/btc_mm.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

# Bot 2: ETH 做市策略 (同策略不同品种)
[[bots]]
id = "mm_ethusdt"
enabled = true
strategy = "obi_mm"
symbol = "ethusdt"
config_file = "/path/to/config/eth_mm.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

# Bot 3: 网格策略 (不同策略同品种)
[[bots]]
id = "grid_btcusdt"
enabled = true
strategy = "grid_strategy"
symbol = "btcusdt"
config_file = "/path/to/config/btc_grid.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5

# Bot 4: 测试 Bot (禁用状态，不会启动)
[[bots]]
id = "test_bot"
enabled = false
strategy = "test_strategy"
symbol = "btcusdt"
config_file = "/path/to/config/test.toml"
auto_restart = false
```

### 常见多 Bot 场景

| 场景 | 配置方式 | 说明 |
|------|----------|------|
| 同策略多品种 | 同一 `strategy`，不同 `symbol` 和 `config_file` | 如 BTC/ETH/SOL 都运行做市策略 |
| 同品种多策略 | 不同 `strategy`，同一 `symbol` | 如 BTC 同时运行做市和网格策略 |
| A/B 测试 | 两个 bot 相同品种和策略，不同参数配置 | 比较不同参数效果 |
| 主备切换 | 一个 `enabled=true`，一个 `enabled=false` | 快速切换策略版本 |
| 多交易所 | 不同 `connector` 配置 | 需要多个 connector 实例 |

### 批量控制 API

```bash
# 获取 Token
TOKEN=$(curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}' | jq -r '.token')

# 列出所有 Bots
curl -s http://localhost:3000/api/bots \
  -H "Authorization: Bearer $TOKEN" | jq .

# 重启所有 Bots
curl -s -X POST http://localhost:3000/api/bots/restart-all \
  -H "Authorization: Bearer $TOKEN"

# 一键启动所有组件 (Infra + Bots)
curl -s -X POST http://localhost:3000/api/cluster/start-all \
  -H "Authorization: Bearer $TOKEN"

# 一键停止所有组件
curl -s -X POST http://localhost:3000/api/cluster/stop-all \
  -H "Authorization: Bearer $TOKEN"

# 获取集群健康状态
curl -s http://localhost:3000/api/cluster/health \
  -H "Authorization: Bearer $TOKEN"
# 响应: {"connector":"running","collector":"running","order_recorder":"stopped",
#        "bots_running":3,"bots_total":4,"all_healthy":true}
```

### Dashboard 多 Bot 视图

Dashboard 的 Bots 页面会显示所有配置的 Bot：

```
┌─────────────────────────────────────────────────────────────┐
│  Bots Management                                             │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │ mm_btcusdt                              [Running]   │    │
│  │ Symbol: btcusdt  PID: 12345  Uptime: 2h 30m        │    │
│  │ Restarts: 0                                         │    │
│  │                            [Stop] [Restart]         │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ mm_ethusdt                              [Running]   │    │
│  │ Symbol: ethusdt  PID: 12346  Uptime: 2h 28m        │    │
│  │ Restarts: 0                                         │    │
│  │                            [Stop] [Restart]         │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ grid_btcusdt                            [Stopped]  │    │
│  │ Symbol: btcusdt  PID: -     Uptime: -              │    │
│  │ Restarts: 0                                         │    │
│  │                            [Start]                  │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ test_bot                                [Disabled] │    │
│  │ Symbol: btcusdt  (Bot is disabled in config)       │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### 多 Bot 资源管理

#### 内存估算

每个 Bot 的内存占用取决于：
- Order book 深度配置
- 订单历史缓存
- 因子计算窗口大小

典型内存占用：
| 配置 | 内存占用 |
|------|----------|
| 基础做市 Bot | ~50-100 MB |
| 带因子计算 | ~100-200 MB |
| 深度 L3 数据 | ~200-500 MB |

#### CPU 估算

- 每个 Bot 主要是单线程运行
- 建议每 CPU 核心运行 2-4 个 Bot
- 高频策略建议独占 CPU 核心

#### 启动顺序

Supervisor 按配置文件顺序启动 Bot，建议：
1. 先启动低频策略
2. 后启动高频策略
3. 测试 Bot 放最后

### 多 Bot 监控脚本

```bash
#!/bin/bash
# monitor_bots.sh - 监控所有 Bot 状态

TOKEN="your_token_here"
API="http://localhost:3000/api"

while true; do
    clear
    echo "=== HFT Cluster Status ==="
    echo "Time: $(date)"
    echo ""

    # 集群健康
    health=$(curl -s "$API/cluster/health" -H "Authorization: Bearer $TOKEN")
    echo "Cluster Health:"
    echo "  Connector: $(echo $health | jq -r '.connector')"
    echo "  Collector: $(echo $health | jq -r '.collector')"
    echo "  Order Recorder: $(echo $health | jq -r '.order_recorder')"
    echo "  Bots: $(echo $health | jq -r '.bots_running')/$(echo $health | jq -r '.bots_total')"
    echo "  All Healthy: $(echo $health | jq -r '.all_healthy')"
    echo ""

    # Bot 列表
    echo "Bots:"
    curl -s "$API/bots" -H "Authorization: Bearer $TOKEN" | \
        jq -r '.[] | "  \(.id): \(.state) (PID: \(.pid // "-"), Uptime: \(.uptime_secs // 0)s)"'

    sleep 5
done
```

### 动态添加 Bot

1. 编辑 `cluster.toml` 添加新的 `[[bots]]` 条目
2. Supervisor 会自动检测配置变化（每 10 秒检查一次）
3. 新 Bot 会出现在 Dashboard 中
4. 手动启动新 Bot 或等待下次 restart-all

```bash
# 或者通过 API 触发配置重载
curl -s -X POST http://localhost:3000/api/cluster/reload \
  -H "Authorization: Bearer $TOKEN"
```
