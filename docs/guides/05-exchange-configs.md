# 交易所配置参考

本章节详细介绍各交易所连接器的配置方法。

## 支持的交易所

| 交易所 | 连接器类型 | 订单方式 | 状态 |
|--------|-----------|----------|------|
| Binance USD-M Futures | `binancefutures` | WebSocket | 生产就绪 |
| Binance Spot | `binancespot` | WebSocket | 生产就绪 |
| Binance Portfolio Margin | `binancepapi` | WebSocket | 生产就绪 |
| Bybit Linear | `bybit` | WebSocket | 生产就绪 |
| Bybit Linear (REST) | `bybit_rest` | REST API | 生产就绪 |

## Binance USD-M Futures

### 配置文件 (binancefutures.toml)

```toml
# WebSocket 数据流
stream_url = "wss://fstream.binancefuture.com/ws"  # 测试网
# stream_url = "wss://fstream.binance.com/public/ws"  # 主网 (2026-04-23 后必须用 /public/ws)

# REST API
api_url = "https://demo-fapi.binance.com"          # 测试网
# api_url = "https://fapi.binance.com"             # 主网

# 订单前缀 (用于识别系统订单)
order_prefix = "hft"

# API 凭证 (由 entrypoint.sh 从环境变量替换)
api_key = "${BINANCE_API_KEY}"
secret = "${BINANCE_SECRET}"
```

### cluster.toml 配置

```toml
[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/binancefutures.toml"
auto_start = true
recording_service = "hft_recording"  # 订单记录
position_sync_interval = 5           # 仓位同步间隔(秒)
income_sync_interval = 60            # 收入同步间隔(秒)
```

### Symbol 格式

- **格式**: 小写，无分隔符
- **示例**: `btcusdt`, `ethusdt`, `solusdt`, `xrpusdt`

### API 权限要求

- Enable Futures
- Enable Reading
- Enable Spot & Margin Trading

### 测试网申请

1. 访问 https://testnet.binancefuture.com
2. 注册并获取 API 密钥
3. 领取测试币

## Binance Spot

### 配置文件 (binancespot.toml)

```toml
# WebSocket 数据流
stream_url = "wss://stream.binance.com:9443/ws"    # 主网
# stream_url = "wss://testnet.binance.vision/ws"   # 测试网

# REST API
api_url = "https://api.binance.com"                # 主网
# api_url = "https://testnet.binance.vision"       # 测试网

order_prefix = "hft"
api_key = "${BINANCE_SPOT_API_KEY}"
secret = "${BINANCE_SPOT_SECRET}"
```

### cluster.toml 配置

```toml
[[connectors]]
name = "spot"
type = "binancespot"
config_file = "/etc/hft/binancespot.toml"
auto_start = true
```

### Symbol 格式

- **格式**: 小写，无分隔符
- **示例**: `btcusdt`, `ethusdt`, `bnbusdt`

## Bybit Linear (REST)

推荐用于 Demo Trading，因为 WebSocket Trade Stream 不支持 Demo 环境。

### 配置文件 (bybit.toml)

```toml
# 公共数据流 (Demo 和主网共用)
public_url = "wss://stream.bybit.com/v5/public/linear"

# 私有数据流 (仓位/订单更新)
private_url = "wss://stream-demo.bybit.com/v5/private"  # Demo
# private_url = "wss://stream.bybit.com/v5/private"     # 主网

# REST API (订单提交)
rest_url = "https://api-demo.bybit.com"                 # Demo
# rest_url = "https://api.bybit.com"                    # 主网

# 合约类型
category = "linear"  # USDT 永续

# 订单前缀 (最多 8 字符，不能包含 '/')
order_prefix = "hft"

api_key = "${BYBIT_API_KEY}"
secret = "${BYBIT_SECRET}"
```

### cluster.toml 配置

```toml
[[connectors]]
name = "bybit"
type = "bybit_rest"
config_file = "/etc/hft/bybit.toml"
auto_start = true
recording_service = "hft_recording"
position_sync_interval = 5
income_sync_interval = 60
```

### Symbol 格式

- **格式**: 大写，无分隔符
- **示例**: `BTCUSDT`, `ETHUSDT`, `SOLUSDT`, `XRPUSDT`

### Demo Trading 申请

1. 访问 https://testnet.bybit.com
2. 注册账户
3. 在 API Management 创建 API 密钥
4. 系统会自动充值测试 USDT

## 多连接器配置

可以同时运行多个交易所连接器：

```toml
# cluster.toml

# Binance Futures
[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/binancefutures.toml"
auto_start = true

# Bybit
[[connectors]]
name = "bybit"
type = "bybit_rest"
config_file = "/etc/hft/bybit.toml"
auto_start = true

# 同一策略在不同交易所运行
[[bots]]
id = "obi_mm_btcusdt_bf"
symbol = "btcusdt"        # Binance: 小写
connector = "bf"
# ...

[[bots]]
id = "obi_mm_btcusdt_bybit"
symbol = "BTCUSDT"        # Bybit: 大写
connector = "bybit"
# ...
```

## 环境变量配置

### .env 文件

```bash
# Binance Futures
BINANCE_API_KEY=your_binance_futures_api_key
BINANCE_SECRET=your_binance_futures_secret

# Binance Spot (如果需要)
BINANCE_SPOT_API_KEY=your_binance_spot_api_key
BINANCE_SPOT_SECRET=your_binance_spot_secret

# Bybit
BYBIT_API_KEY=your_bybit_api_key
BYBIT_SECRET=your_bybit_secret

# 系统配置
JWT_SECRET=your_jwt_secret_for_web_dashboard
RUST_LOG=info
```

## 参数调整建议

### 不同交易对的参数差异

| 交易对 | tick_size | lot_size | order_qty | max_position | half_spread |
|--------|-----------|----------|-----------|--------------|-------------|
| BTCUSDT | 0.1 | 0.001 | 0.002 | 0.02 | 0.0005 |
| ETHUSDT | 0.01 | 0.001 | 0.02 | 0.2 | 0.0006 |
| SOLUSDT | 0.001 | 0.01 | 1.0 | 10.0 | 0.0008 |
| XRPUSDT | 0.0001 | 1.0 | 50.0 | 500.0 | 0.001 |

### 波动性调整

高波动品种 (SOL, XRP, DOGE):
```toml
[params]
half_spread = 0.001      # 更宽的价差
volatility_window = 30   # 更短的窗口

[stop_loss]
volatility_mean = 0.015  # 更高的基准波动率
volatility_threshold = 2.5
```

低波动品种 (BTC, ETH):
```toml
[params]
half_spread = 0.0005     # 更窄的价差
volatility_window = 60   # 更长的窗口

[stop_loss]
volatility_mean = 0.005
volatility_threshold = 3.0
```

## 网络优化

### 延迟敏感配置

```toml
# 使用就近的 API 端点
# 亚洲: fapi.binance.com (新加坡)
# 欧洲: fapi.binance.com (法兰克福)
# 美国: fapi.binance.com (弗吉尼亚)

# Bybit 也有多区域端点
# api.bybit.com (全球)
# api.bytick.com (备用)
```

### 连接稳定性

连接器已内置自动重连机制:
- WebSocket 断线后自动重连
- 自动重新订阅所有 symbols
- 指数退避重试

## 故障排查

### 连接问题

```bash
# 检查连接器状态
docker compose exec hft hftctl status

# 查看连接器日志
docker compose exec hft tail -f /var/lib/hft/logs/infra/connector_bf.log
```

### 常见错误

| 错误 | 原因 | 解决方案 |
|------|------|----------|
| `-2015` Invalid API-key | API 密钥无效 | 检查 .env 文件 |
| `-1021` Timestamp error | 时间不同步 | 检查服务器时间 |
| `-5022` Post Only rejected | 订单会吃单 | 正常现象，策略会重试 |
| `listenKeyExpired` | 监听密钥过期 | 连接器会自动刷新 |

### API 限制

Binance:
- 订单限制: 10 订单/秒
- 取消限制: 100 取消/秒
- WebSocket 连接: 5 连接/IP

Bybit:
- 订单限制: 120 请求/分钟 (REST)
- WebSocket: 500 订阅/连接

## 安全建议

1. **API 权限最小化**: 只开启需要的权限
2. **IP 白名单**: 在交易所设置 API 的 IP 白名单
3. **定期轮换**: 定期更换 API 密钥
4. **监控异常**: 设置异常交易告警
5. **测试先行**: 先在测试网充分验证

## 参考链接

- [Binance Futures API 文档](https://binance-docs.github.io/apidocs/futures/cn/)
- [Bybit API 文档](https://bybit-exchange.github.io/docs/v5/intro)
- [Binance 测试网](https://testnet.binancefuture.com)
- [Bybit Demo Trading](https://testnet.bybit.com)
