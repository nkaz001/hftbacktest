# Infrastructure Integration Implementation Plan

将 Connector、Collector、Order Recorder 集成到统一管理系统，实现一站式启动和监控。

## 1. 目标

- 通过 Supervisor 统一管理所有基础设施组件的生命周期
- 通过 Web API 和 Dashboard 实现远程控制
- 支持组件依赖关系和启动顺序
- 实现一键启动/停止所有组件

## 2. 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                         Dashboard                            │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌──────────────────┐  │
│  │ Bots    │ │ Config  │ │ Logs    │ │ Infrastructure   │  │
│  └─────────┘ └─────────┘ └─────────┘ └──────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
                         REST API / WebSocket
                              │
┌─────────────────────────────────────────────────────────────┐
│                       HFT Server                             │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ /api/infra/*  - Infrastructure Management Endpoints     ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
                              │
                         Unix Socket IPC
                              │
┌─────────────────────────────────────────────────────────────┐
│                       Supervisor                             │
│  ┌───────────────┐  ┌───────────────────────────────────┐  │
│  │ BotManager    │  │ InfraManager                      │  │
│  │ - Bot 进程    │  │ - Connector 进程                  │  │
│  │ - 状态监控    │  │ - Collector 进程                  │  │
│  │ - 自动重启    │  │ - OrderRecorder 进程              │  │
│  └───────────────┘  │ - 依赖管理                        │  │
│                     │ - 健康检查                        │  │
│                     └───────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## 3. 配置文件扩展

### 3.1 扩展 cluster.toml

```toml
[cluster]
name = "testnet-cluster"
data_dir = "./data"
log_dir = "./logs"

[connector]
name = "bf"
type = "binancefutures"
config_path = "./binancefutures.toml"
auto_start = true

[collector]
enabled = true
symbols = ["btcusdt", "ethusdt"]
output_dir = "./data/collected"
auto_start = true

# 新增: Order Recorder 配置
[order_recorder]
enabled = true
config_path = "./order-recorder.toml"
auto_start = true

[bot_defaults]
tick_size = 0.1
lot_size = 0.001

[[bots]]
id = "obi_mm_btcusdt"
strategy = "obi_mm_rust"
config_path = "./obi_mm.toml"
symbols = ["btcusdt"]
```

### 3.2 Order Recorder 配置文件 (order-recorder.toml)

```toml
[clickhouse]
url = "http://localhost:8123"
database = "hft"
user = "default"
password = ""

[recorder]
batch_size = 100
flush_interval_ms = 1000

[ipc]
socket_path = "/tmp/hft-order-recorder.sock"
```

## 4. 数据结构

### 4.1 InfraComponent 枚举

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfraComponent {
    Connector,
    Collector,
    OrderRecorder,
}
```

### 4.2 InfraStatus 结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraStatus {
    pub component: InfraComponent,
    pub status: ProcessStatus,  // Running, Stopped, Failed, Starting
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_error: Option<String>,
    pub config: InfraComponentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraComponentConfig {
    pub enabled: bool,
    pub auto_start: bool,
    pub config_path: Option<String>,
}
```

## 5. IPC 协议扩展

### 5.1 新增命令

```rust
pub enum SupervisorCommand {
    // 现有命令
    ListBots,
    GetBotStatus(String),
    StartBot(String),
    StopBot(String),
    RestartBot(String),

    // 新增: 基础设施命令
    ListInfra,
    GetInfraStatus(InfraComponent),
    StartInfra(InfraComponent),
    StopInfra(InfraComponent),
    RestartInfra(InfraComponent),

    // 新增: 全局控制
    StartAll,       // 启动所有组件 (按依赖顺序)
    StopAll,        // 停止所有组件 (反向顺序)
    GetClusterHealth,
}
```

### 5.2 响应类型

```rust
pub enum SupervisorResponse {
    // 现有响应
    BotList(Vec<BotInfo>),
    BotStatus(BotStatus),
    Success,
    Error(String),

    // 新增响应
    InfraList(Vec<InfraStatus>),
    InfraStatus(InfraStatus),
    ClusterHealth(ClusterHealthStatus),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealthStatus {
    pub connector: ProcessStatus,
    pub collector: ProcessStatus,
    pub order_recorder: ProcessStatus,
    pub bots_running: usize,
    pub bots_total: usize,
    pub all_healthy: bool,
}
```

## 6. API 设计

### 6.1 Infrastructure Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/infra | 列出所有基础设施组件状态 |
| GET | /api/infra/:component | 获取单个组件状态 |
| POST | /api/infra/:component/start | 启动组件 |
| POST | /api/infra/:component/stop | 停止组件 |
| POST | /api/infra/:component/restart | 重启组件 |
| POST | /api/cluster/start-all | 一键启动所有组件 |
| POST | /api/cluster/stop-all | 一键停止所有组件 |
| GET | /api/cluster/health | 获取集群健康状态 |

### 6.2 响应格式

```json
// GET /api/infra
{
  "infra": [
    {
      "component": "connector",
      "status": "running",
      "pid": 12345,
      "uptime_secs": 3600,
      "config": {
        "enabled": true,
        "auto_start": true,
        "config_path": "./binancefutures.toml"
      }
    },
    {
      "component": "collector",
      "status": "stopped",
      "pid": null,
      "config": {
        "enabled": true,
        "auto_start": false
      }
    },
    {
      "component": "order_recorder",
      "status": "running",
      "pid": 12346,
      "uptime_secs": 3500,
      "config": {
        "enabled": true,
        "auto_start": true,
        "config_path": "./order-recorder.toml"
      }
    }
  ]
}

// GET /api/cluster/health
{
  "connector": "running",
  "collector": "stopped",
  "order_recorder": "running",
  "bots_running": 2,
  "bots_total": 3,
  "all_healthy": false
}
```

## 7. Dashboard Infrastructure 页面

### 7.1 页面布局

```
┌────────────────────────────────────────────────────────────────┐
│ Infrastructure                        [Start All] [Stop All]  │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ Cluster Health                                          │  │
│  │ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │  │
│  │ │Connector │ │Collector │ │OrderRec  │ │ Bots     │   │  │
│  │ │  ✅ Up   │ │  ⏹ Down │ │  ✅ Up   │ │  2/3     │   │  │
│  │ └──────────┘ └──────────┘ └──────────┘ └──────────┘   │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                                │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ Components                                               │  │
│  │                                                          │  │
│  │ ┌────────────────────────────────────────────────────┐  │  │
│  │ │ 🔌 Connector (bf)                    [Stop][Restart]│  │  │
│  │ │ Status: Running  PID: 12345  Uptime: 1h 30m        │  │  │
│  │ │ Type: binancefutures                               │  │  │
│  │ │ Config: ./binancefutures.toml                      │  │  │
│  │ └────────────────────────────────────────────────────┘  │  │
│  │                                                          │  │
│  │ ┌────────────────────────────────────────────────────┐  │  │
│  │ │ 📊 Collector                              [Start]  │  │  │
│  │ │ Status: Stopped                                    │  │  │
│  │ │ Symbols: btcusdt, ethusdt                          │  │  │
│  │ │ Output: ./data/collected                           │  │  │
│  │ └────────────────────────────────────────────────────┘  │  │
│  │                                                          │  │
│  │ ┌────────────────────────────────────────────────────┐  │  │
│  │ │ 📝 Order Recorder                    [Stop][Restart]│  │  │
│  │ │ Status: Running  PID: 12346  Uptime: 1h 25m        │  │  │
│  │ │ ClickHouse: http://localhost:8123                  │  │  │
│  │ │ Config: ./order-recorder.toml                      │  │  │
│  │ └────────────────────────────────────────────────────┘  │  │
│  └─────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

## 8. 启动顺序和依赖

### 8.1 组件依赖关系

```
Connector ─────────────────────────────────┐
     │                                     │
     ├──────> Collector (依赖行情连接)      │
     │                                     ▼
     └──────> Bots (依赖行情/交易连接) ────> Order Recorder
```

### 8.2 启动顺序 (Start All)

1. **Connector** - 建立交易所连接
2. **等待** Connector 健康检查通过 (行情流建立)
3. **Collector** (如果启用) - 开始数据收集
4. **Order Recorder** (如果启用) - 启动订单记录服务
5. **Bots** - 启动所有配置的 Bot

### 8.3 停止顺序 (Stop All)

1. **Bots** - 先停止所有交易 Bot (确保无未完成订单)
2. **Order Recorder** - 确保所有订单记录完成
3. **Collector** - 停止数据收集
4. **Connector** - 最后断开交易所连接

## 9. 实现步骤

### Step 1: 扩展配置结构 (supervisor/src/config.rs)

- [ ] 添加 `OrderRecorderConfig` 结构
- [ ] 更新 `ClusterConfig` 包含 order_recorder 字段
- [ ] 添加配置验证逻辑

### Step 2: 创建 InfraManager (supervisor/src/infra.rs)

- [ ] 定义 `InfraComponent` 枚举
- [ ] 定义 `InfraStatus` 结构
- [ ] 实现 `InfraManager` 结构
  - [ ] `start_connector()` - 启动 Connector 进程
  - [ ] `start_collector()` - 启动 Collector 进程
  - [ ] `start_order_recorder()` - 启动 Order Recorder 进程
  - [ ] `stop_component()` - 停止指定组件
  - [ ] `get_status()` - 获取组件状态
  - [ ] `health_check()` - 健康检查

### Step 3: 扩展 IPC 协议 (supervisor/src/ipc.rs)

- [ ] 添加基础设施相关命令
- [ ] 添加响应类型
- [ ] 实现命令处理逻辑

### Step 4: 更新 Supervisor 主逻辑 (supervisor/src/main.rs)

- [ ] 初始化 InfraManager
- [ ] 处理 auto_start 配置
- [ ] 实现 start_all / stop_all 逻辑

### Step 5: 添加 Server API (server/src/api/infra.rs)

- [ ] 实现 GET /api/infra
- [ ] 实现 GET /api/infra/:component
- [ ] 实现 POST /api/infra/:component/start
- [ ] 实现 POST /api/infra/:component/stop
- [ ] 实现 POST /api/infra/:component/restart
- [ ] 实现 POST /api/cluster/start-all
- [ ] 实现 POST /api/cluster/stop-all
- [ ] 实现 GET /api/cluster/health

### Step 6: 更新 Dashboard

- [ ] 添加 Infrastructure 类型定义 (types/index.ts)
- [ ] 添加 API 客户端方法 (api/client.ts)
- [ ] 创建 useInfra hook (hooks/useInfra.ts)
- [ ] 创建 Infrastructure 页面 (pages/Infrastructure.tsx)
- [ ] 更新路由和侧边栏

### Step 7: 测试

- [ ] 单元测试: InfraManager
- [ ] 集成测试: API endpoints
- [ ] 端到端测试: Dashboard 控制

## 10. 文件变更列表

| 文件 | 操作 | 描述 |
|------|------|------|
| supervisor/src/config.rs | 修改 | 添加 OrderRecorderConfig |
| supervisor/src/infra.rs | 新增 | InfraManager 实现 |
| supervisor/src/ipc.rs | 修改 | 添加基础设施命令 |
| supervisor/src/main.rs | 修改 | 集成 InfraManager |
| supervisor/src/lib.rs | 修改 | 导出新模块 |
| server/src/api/mod.rs | 修改 | 添加 infra 模块 |
| server/src/api/infra.rs | 新增 | Infrastructure API |
| server/src/supervisor.rs | 修改 | 添加 infra 相关方法 |
| server/dashboard/src/types/index.ts | 修改 | 添加 Infra 类型 |
| server/dashboard/src/api/client.ts | 修改 | 添加 infra API |
| server/dashboard/src/hooks/useInfra.ts | 新增 | Infra hooks |
| server/dashboard/src/pages/Infrastructure.tsx | 新增 | Infrastructure 页面 |
| server/dashboard/src/App.tsx | 修改 | 添加路由 |
| server/dashboard/src/components/layout/Sidebar.tsx | 修改 | 添加导航项 |

## 11. 时间估算

本方案不提供时间估算，实现顺序按照 Step 1-7 进行。
