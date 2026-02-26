# HFT 多Bot管理系统设计方案

## 需求概述
- **部署环境**: Linux单机服务器
- **规模**: 单个Connector (Binance Futures) + 5+个策略Bot
- **配置管理**: 优雅重启，自动warmup
- **数据管理**: 自动收集、轮换、清理
- **UI**: Web Dashboard，完整管理功能，准实时更新(1-5秒)

---

## 架构设计

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Web Browser                                   │
│                    (React Dashboard)                                 │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ HTTP/WebSocket (port 8080)
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      hft-server (Rust/Axum)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │
│  │ REST API    │  │ WebSocket   │  │ Auth/JWT    │  │ SQLite DB  │  │
│  │ /api/bots   │  │ /ws/events  │  │             │  │ (state)    │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ Unix Socket / IPC
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
│hft-connector │      │  Bot 1~N   │      │hft-collector │
│ (行情/订单)   │◄────►│ (策略进程)  │◄────►│ (数据录制)   │
└──────────────┘ IPC  └────────────┘ IPC  └──────────────┘
```

### 核心组件

| 组件 | 作用 | 技术栈 |
|------|------|--------|
| `hft-supervisor` | Bot进程管理器 | Rust |
| `hft-server` | Web API + 静态文件服务 | Rust/Axum + SQLite |
| `connector` | 交易所连接 | Rust (现有) |
| `collector` | 数据录制 | Rust (现有) |
| `dashboard` | Web前端 | React + TypeScript |

---

## Phase 1: Supervisor核心 (已完成)

### 实现的功能

1. **配置解析** (`config.rs`):
   - 统一 `cluster.toml` 格式
   - Connector, collector, warmup, bot 配置
   - 配置验证和变更检测

2. **进程管理** (`manager.rs`):
   - 启动/停止/重启单个Bot
   - SIGTERM优雅关闭，超时后SIGKILL
   - 崩溃后自动重启，可配置退避时间
   - 配置热重载

3. **Warmup协调** (`warmup.rs`):
   - 检查 `.jsonl` 文件（可边写边读）
   - 检查 `.gz` 压缩文件
   - 验证数据年龄和时长

4. **健康检查** (`health.rs`):
   - 系统资源监控
   - 进程健康检查
   - Connector/Collector状态

5. **控制接口** (`ipc.rs`):
   - Unix socket 用于 `hftctl` 命令
   - Status, start, stop, restart, reload 命令

### 命令行工具

```bash
# 验证配置
hftctl validate /etc/hft/cluster.toml

# 查看状态
hftctl status

# 控制Bot
hftctl start obi_mm_btcusdt
hftctl stop obi_mm_btcusdt
hftctl restart obi_mm_btcusdt
hftctl restart-all

# 重载配置
hftctl reload
```

---

## Phase 2: Web Server (已完成)

### 实现步骤

#### 2.1 创建 server crate
```bash
mkdir -p server/src
```

**server/Cargo.toml**:
```toml
[package]
name = "hft-server"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.7", features = ["ws"] }
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["fs", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["runtime-tokio", "sqlite"] }
jsonwebtoken = "9"
tracing = "0.1"
anyhow = "1"
```

#### 2.2 核心模块

| 文件 | 功能 |
|------|------|
| `src/main.rs` | 服务入口，路由配置 |
| `src/api/mod.rs` | REST API handlers |
| `src/api/bots.rs` | Bot 管理 API |
| `src/api/config.rs` | 配置管理 API |
| `src/api/data.rs` | 数据文件 API |
| `src/api/logs.rs` | 日志查看 API |
| `src/ws/mod.rs` | WebSocket 事件推送 |
| `src/db.rs` | SQLite 状态存储 |
| `src/auth.rs` | JWT 认证 |
| `src/supervisor.rs` | Supervisor IPC 客户端 |

#### 2.3 API 设计

```
# 集群状态
GET    /api/status              # 集群总览

# Bot 管理
GET    /api/bots                # Bot 列表
GET    /api/bots/:id            # Bot 详情
POST   /api/bots/:id/start      # 启动
POST   /api/bots/:id/stop       # 停止
POST   /api/bots/:id/restart    # 重启

# 配置管理
GET    /api/config/cluster      # 获取集群配置
PUT    /api/config/cluster      # 更新集群配置
GET    /api/config/bots/:id     # 获取 Bot 配置
PUT    /api/config/bots/:id     # 更新 Bot 配置

# 数据管理
GET    /api/data/files          # 数据文件列表
DELETE /api/data/files/:name    # 删除文件
POST   /api/data/cleanup        # 清理旧数据

# 日志
GET    /api/logs/:component     # 获取日志 (支持 tail)

# WebSocket
WS     /ws/events               # 实时事件流
```

#### 2.4 WebSocket 事件

```typescript
// 事件类型
interface WsEvent {
  type: 'status_update' | 'bot_state' | 'log_line' | 'alert';
  timestamp: number;
  data: StatusUpdate | BotState | LogLine | Alert;
}

// 状态更新 (每 1-5 秒)
interface StatusUpdate {
  connector: { status: string; healthy: boolean };
  collector: { status: string; healthy: boolean };
  bots: BotState[];
  system: { cpu: number; memory: number; disk: number };
}

// Bot 状态变化
interface BotState {
  id: string;
  state: 'running' | 'stopped' | 'starting' | 'crashed';
  pid?: number;
  uptime_secs?: number;
}

// 日志行
interface LogLine {
  component: string;
  level: 'info' | 'warn' | 'error';
  message: string;
}

// 告警
interface Alert {
  severity: 'info' | 'warning' | 'critical';
  title: string;
  message: string;
}
```

#### 2.5 数据库 Schema

```sql
-- 用户认证
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  username TEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 操作日志
CREATE TABLE audit_logs (
  id INTEGER PRIMARY KEY,
  user_id INTEGER,
  action TEXT NOT NULL,
  target TEXT,
  details TEXT,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Bot 历史状态 (用于图表)
CREATE TABLE bot_history (
  id INTEGER PRIMARY KEY,
  bot_id TEXT NOT NULL,
  state TEXT NOT NULL,
  timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

---

## Phase 3: React Dashboard (待实现)

### 实现步骤

#### 3.1 项目初始化

```bash
cd server
npm create vite@latest dashboard -- --template react-ts
cd dashboard
npm install
npm install @tanstack/react-query zustand lucide-react
npx shadcn-ui@latest init
```

#### 3.2 目录结构

```
dashboard/
├── src/
│   ├── components/
│   │   ├── ui/              # shadcn/ui 组件
│   │   ├── layout/          # 布局组件
│   │   │   ├── Sidebar.tsx
│   │   │   ├── Header.tsx
│   │   │   └── Layout.tsx
│   │   ├── bots/            # Bot 相关组件
│   │   │   ├── BotCard.tsx
│   │   │   ├── BotList.tsx
│   │   │   └── BotDetail.tsx
│   │   ├── config/          # 配置编辑组件
│   │   ├── data/            # 数据管理组件
│   │   └── logs/            # 日志查看组件
│   ├── pages/
│   │   ├── Dashboard.tsx    # 首页总览
│   │   ├── Bots.tsx         # Bot 管理
│   │   ├── Config.tsx       # 配置管理
│   │   ├── Data.tsx         # 数据管理
│   │   ├── Logs.tsx         # 日志查看
│   │   └── Settings.tsx     # 系统设置
│   ├── hooks/
│   │   ├── useWebSocket.ts  # WebSocket hook
│   │   ├── useBots.ts       # Bot 数据 hook
│   │   └── useStatus.ts     # 状态 hook
│   ├── stores/
│   │   └── useStore.ts      # Zustand store
│   ├── api/
│   │   └── client.ts        # API 客户端
│   ├── App.tsx
│   └── main.tsx
├── package.json
└── vite.config.ts
```

#### 3.3 页面设计

**Dashboard 首页**:
```
┌─────────────────────────────────────────────────────────────┐
│  HFT Dashboard                              [User] [Logout] │
├─────────┬───────────────────────────────────────────────────┤
│         │  Cluster: prod-binance-futures    Uptime: 3d 2h   │
│ [Home]  │  ┌─────────┐ ┌─────────┐ ┌─────────┐             │
│ [Bots]  │  │Connector│ │Collector│ │ 3 Bots  │             │
│ [Config]│  │  ● OK   │ │  ● OK   │ │ Running │             │
│ [Data]  │  └─────────┘ └─────────┘ └─────────┘             │
│ [Logs]  │                                                   │
│         │  System Resources                                 │
│         │  CPU: ████░░░░░░ 42%   Memory: ██████░░░░ 58%    │
│         │  Disk: ███░░░░░░░ 31%                            │
│         │                                                   │
│         │  Active Bots                                      │
│         │  ┌──────────────────────────────────────────────┐│
│         │  │ ● obi_mm_btcusdt   Running  PID:12345  2h 3m ││
│         │  │ ● obi_mm_ethusdt   Running  PID:12346  2h 3m ││
│         │  │ ○ obi_mm_solusdt   Stopped                   ││
│         │  └──────────────────────────────────────────────┘│
└─────────┴───────────────────────────────────────────────────┘
```

**Bot 管理页**:
```
┌─────────────────────────────────────────────────────────────┐
│  Bots                                    [+ Add Bot]        │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────┐  │
│  │ obi_mm_btcusdt                                        │  │
│  │ Strategy: obi_mm_live  Symbol: BTCUSDT             │  │
│  │ Status: ● Running   PID: 12345   Uptime: 2h 3m        │  │
│  │                                                        │  │
│  │ [Stop] [Restart] [View Logs] [Edit Config]            │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ obi_mm_ethusdt                                        │  │
│  │ Strategy: obi_mm_live  Symbol: ETHUSDT             │  │
│  │ Status: ● Running   PID: 12346   Uptime: 2h 3m        │  │
│  │                                                        │  │
│  │ [Stop] [Restart] [View Logs] [Edit Config]            │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

#### 3.4 核心组件

**BotCard.tsx**:
```tsx
interface BotCardProps {
  bot: {
    id: string;
    strategy: string;
    symbol: string;
    state: 'running' | 'stopped' | 'crashed';
    pid?: number;
    uptime_secs?: number;
  };
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
}
```

**useWebSocket.ts**:
```tsx
function useWebSocket() {
  const [status, setStatus] = useState<ClusterStatus>();

  useEffect(() => {
    const ws = new WebSocket('ws://localhost:8080/ws/events');
    ws.onmessage = (e) => {
      const event = JSON.parse(e.data);
      if (event.type === 'status_update') {
        setStatus(event.data);
      }
    };
    return () => ws.close();
  }, []);

  return status;
}
```

---

## Phase 4: 生产部署 (待实现)

### 实现步骤

#### 4.1 构建脚本

**scripts/build.sh**:
```bash
#!/bin/bash
set -e

# 构建 Rust 组件
cargo build --release -p connector -p collector -p hft-supervisor -p hft-server

# 构建前端
cd server/dashboard
npm run build
cd ../..

# 复制到部署目录
mkdir -p dist/bin dist/static
cp target/release/{connector,collector,hft-supervisor,hft-server,hftctl} dist/bin/
cp -r server/dashboard/dist/* dist/static/
```

#### 4.2 安装脚本

**scripts/install.sh**:
```bash
#!/bin/bash
set -e

# 创建用户
useradd --system --no-create-home --shell /sbin/nologin hft || true

# 创建目录
mkdir -p /opt/hft/bin
mkdir -p /etc/hft/{connector,strategies}
mkdir -p /var/lib/hft/data/raw
mkdir -p /var/log/hft/bots
mkdir -p /run/hft

# 安装二进制
cp dist/bin/* /opt/hft/bin/
chmod +x /opt/hft/bin/*

# 安装静态文件
cp -r dist/static /opt/hft/

# 设置权限
chown -R hft:hft /var/lib/hft /var/log/hft
chmod 750 /etc/hft

# 安装 systemd 服务
cp supervisor/examples/systemd/*.service /etc/systemd/system/
systemctl daemon-reload

# 创建 hftctl 链接
ln -sf /opt/hft/bin/hftctl /usr/local/bin/hftctl
```

#### 4.3 数据保留脚本

**scripts/data-retention.sh**:
```bash
#!/bin/bash
# 清理超过 RETENTION_DAYS 天的数据

RETENTION_DAYS=${1:-30}
DATA_DIR="/var/lib/hft/data/raw"

find "$DATA_DIR" -name "*.gz" -mtime +$RETENTION_DAYS -delete
find "$DATA_DIR" -name "*.jsonl" -mtime +$RETENTION_DAYS -delete

echo "Cleaned files older than $RETENTION_DAYS days"
```

添加到 crontab:
```bash
# 每天凌晨 3 点清理
0 3 * * * /opt/hft/scripts/data-retention.sh 30
```

#### 4.4 监控集成

**Prometheus metrics** (可选):
```rust
// server/src/metrics.rs
use prometheus::{Counter, Gauge, register_counter, register_gauge};

lazy_static! {
    static ref BOT_STATE: GaugeVec = register_gauge_vec!(
        "hft_bot_state",
        "Bot state (1=running, 0=stopped)",
        &["bot_id"]
    ).unwrap();

    static ref API_REQUESTS: CounterVec = register_counter_vec!(
        "hft_api_requests_total",
        "Total API requests",
        &["endpoint", "method", "status"]
    ).unwrap();
}
```

#### 4.5 备份策略

```bash
#!/bin/bash
# 备份配置和数据库

BACKUP_DIR="/var/backups/hft"
DATE=$(date +%Y%m%d)

mkdir -p "$BACKUP_DIR"

# 备份配置
tar -czf "$BACKUP_DIR/config-$DATE.tar.gz" /etc/hft/

# 备份数据库
sqlite3 /var/lib/hft/server.db ".backup '$BACKUP_DIR/db-$DATE.sqlite'"

# 保留最近 7 天
find "$BACKUP_DIR" -mtime +7 -delete
```

---

## 实现优先级

| Phase | 组件 | 优先级 | 状态 |
|-------|------|--------|------|
| 1 | Supervisor 核心 | ✅ 已完成 | - |
| 2.1 | REST API 基础 | ✅ 已完成 | Bot管理、配置、状态 API |
| 2.2 | WebSocket 事件 | ✅ 已完成 | 实时状态推送 |
| 2.3 | 认证/授权 | ✅ 已完成 | JWT + Argon2 |
| 3.1 | Dashboard 框架 | ✅ 已完成 | React + TypeScript + Vite |
| 3.2 | Bot 管理页 | ✅ 已完成 | 启动/停止/重启控制 |
| 3.3 | 配置编辑页 | ✅ 已完成 | 集群/策略配置查看 |
| 3.4 | 日志/数据页 | ✅ 已完成 | 审计日志查看 |
| 4.1 | systemd 集成 | ✅ 已完成 | - |
| 4.2 | 部署脚本 | 待实现 | - |
| 4.3 | 监控集成 | 待实现 | - |

---

## 统一配置文件: `cluster.toml`

```toml
[cluster]
name = "prod-binance-futures"
data_dir = "/var/lib/hft/data"
config_reload_interval_secs = 30

[connector]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/connector/binancefutures.toml"

[collector]
enabled = true
mode = "ipc"
output_dir = "/var/lib/hft/data/raw"
symbols = ["btcusdt", "ethusdt"]
retention_days = 30

[warmup]
data_dir = "/var/lib/hft/data/raw"
max_age_seconds = 7200
min_duration_seconds = 3600

[bot_defaults]
tick_size = 0.1
lot_size = 0.001

[[bots]]
id = "obi_mm_btcusdt"
enabled = true
strategy = "obi_mm"
symbol = "btcusdt"
config_file = "/etc/hft/strategies/obi_mm_btcusdt.toml"
```

---

## systemd服务文件

位于 `supervisor/examples/systemd/`:
- `hft-connector.service`
- `hft-collector.service`
- `hft-supervisor.service`

---

## 目录结构

```
/etc/hft/
├── cluster.toml
├── connector/
│   └── binancefutures.toml
└── strategies/
    └── obi_mm_btcusdt.toml

/var/lib/hft/data/raw/
├── btcusdt_20260106.jsonl
└── btcusdt_20260105.gz

/var/log/hft/
├── supervisor.log
└── bots/
    └── obi_mm_btcusdt.log

/run/hft/
└── supervisor.sock
```
