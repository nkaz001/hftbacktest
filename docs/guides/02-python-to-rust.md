# Python 到 Rust 策略迁移

本章节以 OBI (Order Book Imbalance) 市场做市策略为例，展示如何将 Python 策略迁移到 Rust。

## 策略代码对比

### 核心逻辑对比

**Python 版本** (`strategies/obi_mm/obi_mm.py`):

```python
@njit
def obi_mm(hbt, stat, order_container, stop_loss_func, g, p):
    asset_no = 0
    imbalance_timeseries = np.full(30_000_000, np.nan, np.float64)
    tick_size = hbt.depth(0).tick_size

    while hbt.wait_next_feed(False, 1_000_000_000) in [0, 2, 3]:
        current_timestamp = hbt.current_timestamp
        current_period = current_timestamp // (10 * 1_000_000_000)

        if current_period > last_check_period:
            depth = hbt.depth(asset_no)
            position = hbt.position(asset_no)
            best_bid = depth.best_bid
            best_ask = depth.best_ask
            mid_price = (best_bid + best_ask) / 2.0

            # 计算 OBI
            sum_bid_qty = 0.0
            for price_tick in range(...):
                sum_bid_qty += depth.bid_depth[price_tick - roi_lb_tick]

            sum_ask_qty = 0.0
            for price_tick in range(...):
                sum_ask_qty += depth.ask_depth[price_tick - roi_lb_tick]

            imbalance = sum_bid_qty - sum_ask_qty

            # Z-score 标准化
            m = np.nanmean(imbalance_timeseries[...])
            s = np.nanstd(imbalance_timeseries[...])
            alpha = (imbalance - m) / s

            # 计算价格
            fair_price = mid_price + p.c1 * alpha * mid_price
            reservation_price = fair_price - p.skew * normalized_position * mid_price
            bid_price = min(reservation_price - half_spread * mid_price, best_bid)
            ask_price = max(reservation_price + half_spread * mid_price, best_ask)

            # 下单
            hbt.submit_buy_order(asset_no, order_id, bid_price, g.order_qty, GTX, LIMIT, False)
            hbt.submit_sell_order(asset_no, order_id, ask_price, g.order_qty, GTX, LIMIT, False)
```

**Rust 版本** (`strategies/obi_mm_rust/src/strategy.rs`):

```rust
impl ObiMmStrategy {
    pub fn update<MD>(&mut self, bot: &mut impl Bot<MD>, asset_no: usize) -> bool
    where
        MD: MarketDepth + L2MarketDepth,
    {
        let current_timestamp = bot.current_timestamp();
        let current_period = current_timestamp / self.params.update_interval_ns;

        if current_period <= self.last_check_period {
            return false;
        }

        let depth = bot.depth(asset_no);
        let position = bot.position(asset_no);
        let best_bid = depth.best_bid();
        let best_ask = depth.best_ask();
        let mid_price = (best_bid + best_ask) / 2.0;

        // 使用 factor-engine 计算 OBI (O(1) 复杂度)
        let alpha = self.obi_factor.update_with_accessor(
            best_bid_tick, best_ask_tick, mid_price,
            |tick| depth.bid_qty_at_tick(tick),
            |tick| depth.ask_qty_at_tick(tick),
        );

        // 计算价格
        let fair_price = mid_price + self.params.c1 * alpha * mid_price;
        let normalized_position = position / self.global.order_qty;
        let reservation_price = fair_price - self.params.skew * normalized_position * mid_price;

        let bid_price = (reservation_price - half_spread * mid_price).min(best_bid);
        let ask_price = (reservation_price + half_spread * mid_price).max(best_ask);

        // 下单
        bot.submit_buy_order(
            asset_no, order_id, bid_price, self.global.order_qty,
            TimeInForce::GTX, OrdType::Limit, false, false,
        );
        bot.submit_sell_order(
            asset_no, order_id, ask_price, self.global.order_qty,
            TimeInForce::GTX, OrdType::Limit, false, false,
        );

        true
    }
}
```

## 迁移步骤

### 第 1 步：定义配置结构

**Python** 通常使用字典或简单类:

```python
class Params:
    def __init__(self):
        self.looking_depth = 0.001
        self.window = 3600
        self.half_spread = 0.0002
        self.skew = 0.01
        self.c1 = 0.0001
```

**Rust** 使用结构体 + serde 序列化:

```rust
// src/config.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    pub looking_depth: f64,
    pub window: usize,
    pub half_spread: f64,
    pub skew: f64,
    pub c1: f64,
    pub power: f64,
    pub live_seconds: f64,
    pub update_interval_ns: i64,
    pub volatility_interval_ns: i64,
}

impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            looking_depth: 0.001,
            window: 3600,
            half_spread: 0.0002,
            skew: 0.01,
            c1: 0.0001,
            power: 1.0,
            live_seconds: 30.0,
            update_interval_ns: 10_000_000_000,
            volatility_interval_ns: 60_000_000_000,
        }
    }
}

impl StrategyConfig {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }
}
```

### 第 2 步：定义策略结构体

```rust
// src/strategy.rs
use factor_engine::{OBIFactor, VolatilityFactor, StopLossChecker};

pub struct ObiMmStrategy {
    pub global: GlobalConfig,
    pub params: StrategyParams,
    pub obi_factor: OBIFactor,           // O(1) OBI 计算
    pub volatility_factor: VolatilityFactor,  // O(1) 波动率计算
    pub stop_loss: StopLossChecker,      // 止损检查
    pub last_check_period: i64,
    pub last_change_period: i64,
    pub last_price: f64,
    pub tick_size: f64,
}

impl ObiMmStrategy {
    pub fn new(
        global: GlobalConfig,
        params: StrategyParams,
        stop_loss_config: StopLossConfig,
        tick_size: f64,
    ) -> Self {
        let roi_lb_tick = (global.roi_lb / tick_size).round() as i64;
        let roi_ub_tick = (global.roi_ub / tick_size).round() as i64;

        Self {
            obi_factor: OBIFactor::new(
                params.looking_depth,
                tick_size,
                roi_lb_tick,
                roi_ub_tick,
                params.window,
            ),
            volatility_factor: VolatilityFactor::new(
                params.volatility_interval_ns,
                params.volatility_window,
            ),
            stop_loss: StopLossChecker::new(...),
            global,
            params,
            last_check_period: -1,
            last_change_period: -1,
            last_price: 0.0,
            tick_size,
        }
    }
}
```

### 第 3 步：实现更新逻辑

```rust
impl ObiMmStrategy {
    pub fn update<MD>(&mut self, bot: &mut impl Bot<MD>, asset_no: usize) -> bool
    where
        MD: MarketDepth + L2MarketDepth,
    {
        let current_timestamp = bot.current_timestamp();
        bot.clear_inactive_orders(Some(asset_no));

        let current_period = current_timestamp / self.params.update_interval_ns;
        if current_period <= self.last_check_period {
            return false;
        }

        // 获取市场数据
        let depth = bot.depth(asset_no);
        let position = bot.position(asset_no);
        let best_bid = depth.best_bid();
        let best_ask = depth.best_ask();

        if best_bid <= 0.0 || best_ask <= 0.0 {
            return false;
        }

        let mid_price = (best_bid + best_ask) / 2.0;

        // 更新因子
        let alpha = self.obi_factor.update_with_accessor(
            depth.best_bid_tick(),
            depth.best_ask_tick(),
            mid_price,
            |tick| depth.bid_qty_at_tick(tick),
            |tick| depth.ask_qty_at_tick(tick),
        );

        self.last_check_period = current_period;

        // 检查预热是否完成
        if !self.is_ready() {
            return false;
        }

        // 计算价格
        let fair_price = mid_price + self.params.c1 * alpha * mid_price;
        let normalized_position = position / self.global.order_qty;
        let reservation_price = fair_price - self.params.skew * normalized_position * mid_price;

        let mut bid_price = (reservation_price - self.params.half_spread * mid_price).min(best_bid);
        let mut ask_price = (reservation_price + self.params.half_spread * mid_price).max(best_ask);

        // 对齐到 tick_size
        bid_price = (bid_price / self.tick_size).floor() * self.tick_size;
        ask_price = (ask_price / self.tick_size).ceil() * self.tick_size;

        // 更新订单
        self.update_grid_orders(bot, asset_no, bid_price, ask_price, position, current_timestamp);

        true
    }
}
```

### 第 4 步：创建回测入口

```rust
// src/backtest.rs
use clap::Parser;
use hftbacktest::prelude::*;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(short, long, required = true)]
    data: Vec<PathBuf>,

    #[arg(long, default_value = "0.1")]
    tick_size: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 加载配置
    let config = match &args.config {
        Some(path) => StrategyConfig::from_file(path.to_str().unwrap())?,
        None => StrategyConfig::default(),
    };

    // 构建回测器
    let hbt: Backtest<HashMapMarketDepth> = Backtest::builder()
        .add_asset(
            L2AssetBuilder::new()
                .data(data_sources)
                .latency_model(ConstantLatency::new(1_000_000, 1_000_000))
                .asset_type(LinearAsset::new(1.0))
                .fee_model(TradingValueFeeModel::new(CommonFees::new(-0.00005, 0.0007)))
                .exchange(ExchangeKind::NoPartialFillExchange)
                .queue_model(ProbQueueModel::new(PowerProbQueueFunc3::new(3.0)))
                .depth(|| HashMapMarketDepth::new(tick_size, lot_size))
                .build()?,
        )
        .build()?;

    // 创建策略
    let mut strategy = ObiMmStrategy::new(
        config.global, config.params, config.stop_loss, tick_size
    );

    // 运行回测循环
    loop {
        match hbt.elapse(1_000_000_000) {
            Ok(ElapseResult::Ok) => {
                strategy.update(&mut hbt, 0);
                recorder.record(&hbt)?;
            }
            Ok(ElapseResult::EndOfData) => break,
            Err(e) => return Err(e.into()),
        }
    }

    // 输出结果
    recorder.to_csv("results", &output_path)?;
    Ok(())
}
```

## 运行回测

```bash
cd strategies/obi_mm_rust

# 编译
cargo build --release

# 运行回测
cargo run --release --bin backtest -- \
    --config config.example.toml \
    --data /path/to/btcusdt_20250101.npz \
    --data /path/to/btcusdt_20250102.npz \
    --tick-size 0.1 \
    --lot-size 0.001 \
    --output ./results/

# 查看帮助
cargo run --release --bin backtest -- --help
```

### 回测参数说明

| 参数 | 说明 | 示例 |
|------|------|------|
| `--config` | 策略配置文件 | `config.example.toml` |
| `--data` | 市场数据文件 (可多个) | `btcusdt_*.npz` |
| `--tick-size` | 价格最小变动单位 | BTC: 0.1, ETH: 0.01 |
| `--lot-size` | 数量最小变动单位 | 0.001 |
| `--output` | 结果输出目录 | `./results/` |
| `--roi` | 使用 ROIVectorMarketDepth | 适合价格范围固定的品种 |
| `--roi-lb` | ROI 下界 | 0.0 |
| `--roi-ub` | ROI 上界 | 200000.0 |

### 回测输出

回测完成后会在输出目录生成 CSV 文件：

```
results/
└── obi_mm_0.csv    # 包含每个时间点的: timestamp, price, position, balance, fee
```

## 关键差异总结

| 方面 | Python | Rust |
|------|--------|------|
| 类型 | 动态类型 | 静态类型 |
| 性能 | numba JIT | 原生编译 |
| 因子计算 | 手动 O(n) | factor-engine O(1) |
| 内存 | 预分配大数组 | 滚动窗口 |
| 配置 | 代码内硬编码 | TOML 文件 |
| 回测/实盘 | 不同代码 | 同一策略代码 |

## 下一步

- 了解 Factor Engine 的详细用法：[Factor Engine 因子开发](03-factor-engine.md)
- 准备部署到实盘：[Docker 实盘部署](04-docker-deployment.md)
