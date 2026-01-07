# HFT Server - Web API 服务

HFT Server 是 HFT 多Bot管理系统的 Web API 服务组件，提供 REST API 和 WebSocket 实时事件推送，用于管理和监控交易Bot集群。

## 架构概览

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Web Browser / API Client                      │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ HTTP/WebSocket
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      hft-server (Rust/Axum)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │
│  │ REST API    │  │ WebSocket   │  │ Auth/JWT    │  │ SQLite DB  │  │
│  │ /api/*      │  │ /ws         │  │             │  │ (state)    │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ Unix Socket IPC
                              ▼
                    ┌──────────────────┐
                    │  hft-supervisor  │
                    │  (进程管理器)     │
                    └────────┬─────────┘
                             │
    ┌────────────────────────┼────────────────────────┐
    │                        │                        │
    ▼                        ▼                        ▼
┌──────────────┐      ┌────────────┐      ┌──────────────┐
│  connector   │      │  Bot 1~N   │      │  collector   │
│ (行情/订单)   │◄────►│ (策略进程)  │      │ (数据录制)   │
└──────────────┘ IPC  └────────────┘      └──────────────┘
```

## 构建

```bash
# 构建所有组件
cargo build --release

# 仅构建 server
cargo build --release -p hft-server
```

构建产物位于 `target/release/`:
- `hft-server` - Web API 服务
- `hft-supervisor` - 进程管理器
- `connector` - 交易所连接器
- `collector` - 数据收集器

## 从零开始的完整启动流程

### 1. 准备配置文件

#### 1.1 集群配置 (`cluster.toml`)

```toml
[cluster]
name = "testnet-binance-futures"
data_dir = "/path/to/data"
log_dir = "/path/to/logs"
socket_path = "/tmp/hft-supervisor.sock"
config_reload_interval_secs = 10

[connector]
name = "bf"
type = "binancefutures"
config_file = "/path/to/binancefutures.toml"

[collector]
enabled = true
mode = "ipc"
output_dir = "/path/to/data"
symbols = ["btcusdt"]
retention_days = 7

[warmup]
data_dir = "/path/to/data"
max_age_seconds = 86400
min_duration_seconds = 60

[bot_defaults]
tick_size = 0.1
lot_size = 0.001

[[bots]]
id = "obi_mm_btcusdt"
enabled = true
strategy = "obi_mm_testnet"
symbol = "btcusdt"
config_file = "/path/to/obi_mm.toml"
auto_restart = true
max_restarts = 3
restart_backoff_secs = 5
```

#### 1.2 Connector 配置 (`binancefutures.toml`)

```toml
stream_url = "wss://fstream.binancefuture.com/ws"
api_url = "https://demo-fapi.binance.com"  # testnet
order_prefix = "test"
api_key = "YOUR_API_KEY"
secret = "YOUR_SECRET"
```

#### 1.3 策略配置 (`obi_mm.toml`)

```toml
[global]
order_qty = 0.002
max_position = 0.02
grid_num = 3
grid_interval = 1.0
roi_lb = 0.0
roi_ub = 200000.0

[params]
looking_depth = 0.001
window = 5
volatility_window = 2
half_spread = 0.0005
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

### 2. 启动服务 (按顺序)

#### Step 1: 启动 Supervisor (进程管理器)

```bash
./target/release/hft-supervisor \
  -c /path/to/cluster.toml \
  --bin-dir ./target/release
```

参数说明:
- `-c, --config`: 集群配置文件路径
- `--bin-dir`: 策略二进制文件目录
- `--start-on-boot`: 启动时自动启动所有 enabled 的 bot

#### Step 2: 启动 Server (Web API)

```bash
./target/release/hft-server \
  --config /path/to/cluster.toml \
  --socket /tmp/hft-supervisor.sock \
  --database-url ./hft-server.db \
  --port 3001
```

参数说明:
- `--config`: 集群配置文件路径
- `--socket`: Supervisor Unix socket 路径
- `--database-url`: SQLite 数据库路径
- `--port`: HTTP 监听端口 (默认 3000)
- `--bind`: 绑定地址 (默认 127.0.0.1)
- `--no-auth`: 禁用认证 (仅开发环境)

#### Step 3: 启动 Connector (交易所连接)

```bash
./target/release/connector bf binancefutures /path/to/binancefutures.toml
```

参数说明:
- 第1个参数: connector 名称 (用于 IPC 通信)
- 第2个参数: connector 类型 (binancefutures/bybit/binancespot)
- 第3个参数: connector 配置文件

#### Step 4: 启动 Collector (可选，数据录制)

```bash
./target/release/collector /path/to/data --ipc bf btcusdt
```

参数说明:
- 第1个参数: 数据输出目录
- `--ipc`: IPC 模式，指定 connector 名称
- 后续参数: 需要录制的交易对

### 3. 通过 API 控制 Bot

#### 3.1 登录获取 Token

```bash
# 默认用户名/密码: admin/admin
curl -X POST http://127.0.0.1:3001/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin"}'
```

响应:
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "expires_at": "2026-01-08T00:00:00Z",
  "user": {"id": 1, "username": "admin", "role": "admin"}
}
```

#### 3.2 查看 Bot 状态

```bash
TOKEN="your_jwt_token"
curl http://127.0.0.1:3001/api/bots -H "Authorization: Bearer $TOKEN"
```

#### 3.3 启动/停止/重启 Bot

```bash
# 启动
curl -X POST http://127.0.0.1:3001/api/bots/obi_mm_btcusdt/start \
  -H "Authorization: Bearer $TOKEN"

# 停止
curl -X POST http://127.0.0.1:3001/api/bots/obi_mm_btcusdt/stop \
  -H "Authorization: Bearer $TOKEN"

# 重启
curl -X POST http://127.0.0.1:3001/api/bots/obi_mm_btcusdt/restart \
  -H "Authorization: Bearer $TOKEN"
```

### 4. 完整启动脚本示例

```bash
#!/bin/bash
set -e

# 配置路径
CONFIG_DIR="./examples/testnet/config"
BIN_DIR="./target/release"
DATA_DIR="./examples/testnet/data"
DB_PATH="./examples/testnet/hft-server.db"
SOCKET_PATH="/tmp/hft-supervisor-testnet.sock"

# 创建必要目录
mkdir -p "$DATA_DIR"

# 1. 启动 Supervisor
echo "Starting Supervisor..."
$BIN_DIR/hft-supervisor \
  -c $CONFIG_DIR/cluster_testnet.toml \
  --bin-dir $BIN_DIR &
sleep 2

# 2. 启动 Server
echo "Starting Server..."
$BIN_DIR/hft-server \
  --config $CONFIG_DIR/cluster_testnet.toml \
  --socket $SOCKET_PATH \
  --database-url $DB_PATH \
  --port 3001 &
sleep 2

# 3. 启动 Connector
echo "Starting Connector..."
$BIN_DIR/connector bf binancefutures $CONFIG_DIR/binancefutures_testnet.toml &
sleep 3

# 4. 启动 Collector (可选)
echo "Starting Collector..."
$BIN_DIR/collector $DATA_DIR --ipc bf btcusdt &

echo "All services started!"
echo "API available at: http://127.0.0.1:3001"
echo "Default login: admin / admin"
```

## API 参考

### 认证

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/auth/login` | POST | 登录获取 JWT Token |
| `/api/auth/logout` | POST | 登出 |
| `/api/auth/me` | GET | 获取当前用户信息 |

### Bot 管理

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/bots` | GET | 获取所有 Bot 状态 |
| `/api/bots/:id` | GET | 获取单个 Bot 详情 |
| `/api/bots/:id/start` | POST | 启动 Bot |
| `/api/bots/:id/stop` | POST | 停止 Bot |
| `/api/bots/:id/restart` | POST | 重启 Bot |

### 配置管理

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/config/cluster` | GET | 获取集群配置 |
| `/api/config/cluster` | PUT | 更新集群配置 |
| `/api/config/strategies` | GET | 列出策略配置 |
| `/api/config/strategies/:bot_id` | GET | 获取策略配置 |
| `/api/config/strategies/:bot_id` | PUT | 更新策略配置 |

### 集群状态

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/status` | GET | 获取集群总览 |

### 审计日志

| 端点 | 方法 | 描述 |
|------|------|------|
| `/api/logs/audit` | GET | 获取审计日志 |

## WebSocket 实时事件

连接地址: `ws://127.0.0.1:3001/ws?token=YOUR_JWT_TOKEN`

### 事件格式

```json
{
  "type": "status_update",
  "cluster_name": "testnet-binance-futures",
  "uptime_secs": 3600,
  "connector": {
    "status": "running",
    "healthy": true
  },
  "collector": {
    "status": "running",
    "healthy": true
  },
  "bots": [
    {
      "id": "obi_mm_btcusdt",
      "symbol": "btcusdt",
      "state": "running",
      "pid": 12345,
      "uptime_secs": 3600,
      "restart_count": 0
    }
  ]
}
```

### Python WebSocket 示例

```python
import asyncio
import websockets
import json

async def listen_events():
    token = "your_jwt_token"
    uri = f"ws://127.0.0.1:3001/ws?token={token}"

    async with websockets.connect(uri) as ws:
        print("Connected to WebSocket")
        while True:
            msg = await ws.recv()
            event = json.loads(msg)
            print(f"Event: {event['type']}")
            for bot in event.get('bots', []):
                print(f"  Bot {bot['id']}: {bot['state']}")

asyncio.run(listen_events())
```

## 数据库

Server 使用 SQLite 存储:
- 用户认证信息
- 会话管理
- 审计日志
- Bot 状态历史

首次启动时会自动创建数据库和默认管理员用户 (`admin/admin`)。

### 数据库表

```sql
-- 用户表
users (id, username, password_hash, role, created_at, updated_at)

-- 会话表
sessions (id, user_id, token_hash, expires_at, created_at)

-- 审计日志
audit_logs (id, user_id, action, resource_type, resource_id, details, ip_address, created_at)

-- Bot 状态历史
bot_state_history (id, bot_id, state, previous_state, triggered_by, created_at)
```

## 目录结构

```
server/
├── Cargo.toml
├── README.md
├── migrations/           # SQLite 迁移文件
│   └── 001_init.sql
├── src/
│   ├── main.rs          # 入口，路由配置
│   ├── lib.rs           # 库导出
│   ├── state.rs         # 应用状态
│   ├── supervisor.rs    # Supervisor IPC 客户端
│   ├── api/             # REST API
│   │   ├── mod.rs
│   │   ├── bots.rs      # Bot 管理
│   │   ├── config.rs    # 配置管理
│   │   ├── data.rs      # 数据文件
│   │   ├── logs.rs      # 日志查看
│   │   └── status.rs    # 集群状态
│   ├── auth/            # 认证
│   │   └── mod.rs
│   ├── db/              # 数据库
│   │   └── mod.rs
│   └── ws/              # WebSocket
│       └── mod.rs
├── tests/               # 集成测试
│   ├── api_tests.rs
│   ├── db_tests.rs
│   └── common/
└── scripts/
    └── test_api.sh      # API 测试脚本
```

## 环境变量

| 变量 | 描述 | 默认值 |
|------|------|--------|
| `CLUSTER_CONFIG` | 集群配置文件路径 | - |
| `SUPERVISOR_SOCKET` | Supervisor socket 路径 | - |
| `DATABASE_URL` | SQLite 数据库路径 | - |
| `JWT_SECRET` | JWT 签名密钥 | 随机生成 |
| `BIND_ADDR` | 绑定地址 | 127.0.0.1 |
| `PORT` | 监听端口 | 3000 |

## 安全注意事项

1. **生产环境必须设置 `JWT_SECRET`** - 否则每次重启后旧 token 将失效
2. **修改默认密码** - 首次登录后立即修改 admin 密码
3. **限制绑定地址** - 生产环境建议绑定到内网地址或通过反向代理
4. **启用 HTTPS** - 使用 nginx/caddy 等反向代理提供 HTTPS

## 测试

```bash
# 运行单元测试
cargo test -p hft-server

# 运行集成测试
cargo test -p hft-server --test api_tests
cargo test -p hft-server --test db_tests
```

## 相关文档

- [SUPERVISOR_DESIGN.md](../docs/SUPERVISOR_DESIGN.md) - 系统整体设计
- [LIVE_TRADING_ARCHITECTURE.md](../docs/LIVE_TRADING_ARCHITECTURE.md) - 实盘交易架构
- [STRATEGY_GUIDE.md](../docs/STRATEGY_GUIDE.md) - 策略开发指南
