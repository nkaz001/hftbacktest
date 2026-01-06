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

## Phase 2: Web Server (待实现)

### API 设计
```
GET    /api/status              # 集群总览状态
GET    /api/bots                # Bot列表
GET    /api/bots/:id            # Bot详情
POST   /api/bots/:id/start      # 启动Bot
POST   /api/bots/:id/stop       # 停止Bot
POST   /api/bots/:id/restart    # 重启Bot

GET    /api/config/cluster      # 获取集群配置
PUT    /api/config/cluster      # 更新集群配置
GET    /api/config/strategies   # 策略配置列表

GET    /api/data/files          # 数据文件列表
DELETE /api/data/files/:name    # 删除文件
POST   /api/data/cleanup        # 清理旧数据

GET    /api/logs/:component     # 获取日志
WS     /ws/events               # 实时事件推送
```

### WebSocket 事件
```typescript
interface WsEvent {
  type: 'status_update' | 'bot_state' | 'log_line' | 'alert';
  data: StatusUpdate | BotState | LogLine | Alert;
}
```

---

## Phase 3: React Dashboard (待实现)

### 页面结构
```
/                    # Dashboard首页 - 总览
├── /bots            # Bot列表和管理
│   └── /bots/:id    # 单个Bot详情
├── /config          # 配置管理
├── /data            # 数据管理
├── /logs            # 日志查看
└── /settings        # 系统设置
```

### 技术选型
| 层 | 技术 | 原因 |
|----|------|------|
| Web框架 | Axum | Rust生态，性能好，类型安全 |
| 数据库 | SQLite | 轻量，无需额外依赖 |
| 前端 | React + TypeScript | 生态成熟，组件丰富 |
| UI库 | Shadcn/ui | 现代美观，可定制 |
| 状态管理 | Zustand | 轻量，简单 |
| 构建 | Vite | 快速，开箱即用 |

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
