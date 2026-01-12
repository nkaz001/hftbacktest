# 策略研究员实盘上手指南

本文档面向策略研究员，帮助你从零开始将 Python 策略迁移到 Rust，并部署到基于 Docker 的实盘交易系统。

## 目录

1. [Rust 基础知识](guides/01-rust-basics.md)
2. [Python 到 Rust 策略迁移](guides/02-python-to-rust.md)
3. [Factor Engine 因子开发](guides/03-factor-engine.md)
4. [Docker 实盘部署](guides/04-docker-deployment.md)
5. [交易所配置参考](guides/05-exchange-configs.md)
6. [实战示例：部署新交易对](guides/06-deployment-example.md)

---

## 快速开始

### 前提条件

- 基本的 Python 编程经验
- Linux/macOS 终端操作能力
- Docker 和 Docker Compose 已安装

### 整体流程

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Python 策略    │ -> │   Rust 策略     │ -> │   Docker 实盘   │
│  (研究/回测)    │    │   (回测验证)    │    │   (实盘交易)    │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### 1. 策略开发流程

```bash
# 1. 在 Python 中开发和验证策略逻辑
cd strategies/obi_mm
python backtest.py --data /path/to/data

# 2. 将策略翻译成 Rust
cd strategies/obi_mm_rust
# 编辑 src/strategy.rs

# 3. 使用 Rust 回测验证
cargo run --release --bin backtest -- \
    --config config.example.toml \
    --data /path/to/btcusdt_*.npz \
    --tick-size 0.1 \
    --lot-size 0.001

# 4. 部署到 Docker 实盘
cd deploy/docker
docker compose up -d
```

### 2. 关键文件位置

| 用途 | 路径 |
|------|------|
| Python 策略示例 | `strategies/obi_mm/` |
| Rust 策略示例 | `strategies/obi_mm_rust/` |
| 因子引擎 | `factor-engine/` |
| Docker 部署 | `deploy/docker/` |
| 策略配置模板 | `deploy/docker/config/strategies/` |

### 3. 快速部署新 Symbol

以部署 XRPUSDT 为例：

```bash
# 1. 创建策略配置
cp deploy/docker/config/strategies/obi_mm_btc.toml \
   deploy/docker/config/strategies/obi_mm_xrp.toml

# 2. 编辑配置参数（根据 XRP 特性调整）
# - order_qty: 根据价格调整数量
# - roi_ub: 根据价格范围调整
# - half_spread: 根据波动性调整

# 3. 在 cluster.toml 中添加 bot 配置
[[bots]]
id = "obi_mm_xrpusdt_bf"
enabled = true
strategy = "obi_mm_testnet"
symbol = "xrpusdt"          # Binance 用小写
connector = "bf"
config_file = "/etc/hft/strategies/obi_mm_xrp.toml"

# 4. 重启服务
docker compose restart hft
```

---

## 系统架构概览

```
┌──────────────────────────────────────────────────────────┐
│                    Docker Container                       │
│  ┌────────────────────────────────────────────────────┐  │
│  │               hft-supervisor (进程管理)             │  │
│  │  ┌──────────┐ ┌──────────┐ ┌────────────────────┐  │  │
│  │  │connector │ │connector │ │  order-recorder    │  │  │
│  │  │   (bf)   │ │ (bybit)  │ │   (ClickHouse)     │  │  │
│  │  └────┬─────┘ └────┬─────┘ └─────────▲──────────┘  │  │
│  │       │            │                  │             │  │
│  │       └──────┬─────┘       ┌──────────┴──────────┐  │  │
│  │              │             │                     │  │  │
│  │       ┌──────▼──────┐      │                     │  │  │
│  │       │  IPC Bus    │◄─────┤   Trading Bots     │  │  │
│  │       │ (iceoryx2)  │      │  (obi_mm_testnet)  │  │  │
│  │       └──────┬──────┘      └─────────────────────┘  │  │
│  │              │                                      │  │
│  │       ┌──────▼──────┐                              │  │
│  │       │  collector  │                              │  │
│  │       │ (数据收集)   │                              │  │
│  │       └─────────────┘                              │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌──────────────┐  ┌───────────────┐                    │
│  │  hft-server  │  │   Grafana     │                    │
│  │  (Web API)   │  │  (Dashboard)  │                    │
│  └──────────────┘  └───────────────┘                    │
└──────────────────────────────────────────────────────────┘
```

### 核心组件

| 组件 | 功能 |
|------|------|
| **hft-supervisor** | 进程管理器，管理所有子进程生命周期 |
| **connector** | 交易所连接器，处理 WebSocket 和 REST API |
| **collector** | 市场数据收集器，保存到本地文件 |
| **order-recorder** | 订单记录器，保存到 ClickHouse |
| **trading bots** | 交易策略进程 |
| **hft-server** | Web 管理界面后端 |

---

## 下一步

1. 如果你是 Rust 新手，请先阅读 [Rust 基础知识](guides/01-rust-basics.md)
2. 如果你已有 Python 策略，请阅读 [Python 到 Rust 策略迁移](guides/02-python-to-rust.md)
3. 如果你想开发新因子，请阅读 [Factor Engine 因子开发](guides/03-factor-engine.md)
4. 如果你想部署到实盘，请阅读 [Docker 实盘部署](guides/04-docker-deployment.md)
