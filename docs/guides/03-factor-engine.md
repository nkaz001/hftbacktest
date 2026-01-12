# Factor Engine 因子开发

Factor Engine 是一个专为高频交易设计的流式因子计算引擎，所有因子更新都是 O(1) 时间复杂度。

## 核心概念

### 为什么需要 O(1) 复杂度？

在高频交易中，每次计算都需要在微秒级完成。传统方法如下：

```python
# Python 传统方法 - O(n) 复杂度
window_data = []
for new_value in stream:
    window_data.append(new_value)
    if len(window_data) > window_size:
        window_data.pop(0)

    mean = np.mean(window_data)    # O(n)
    std = np.std(window_data)      # O(n)
    zscore = (new_value - mean) / std
```

Factor Engine 使用增量算法：

```rust
// Rust factor-engine - O(1) 复杂度
let mut welford = WelfordRolling::new(window_size);
for new_value in stream {
    welford.update(new_value);     // O(1)
    let zscore = welford.zscore(new_value);  // O(1)
}
```

## 基础组件 (Primitives)

### RingBuffer - 环形缓冲区

```rust
use factor_engine::RingBuffer;

// 创建固定大小的环形缓冲区
let mut buffer = RingBuffer::new(100);

// 添加数据，满了会自动覆盖最旧的
buffer.push(1.0);
buffer.push(2.0);

// 获取被覆盖的旧值
let evicted = buffer.push(101.0);  // 返回 Some(1.0)

// 检查状态
assert!(buffer.is_full());
assert_eq!(buffer.len(), 100);
```

### WelfordRolling - 滚动均值/标准差

使用 Welford 在线算法，O(1) 更新：

```rust
use factor_engine::WelfordRolling;

let mut stats = WelfordRolling::new(100);  // 100 样本窗口

// 添加新数据
stats.update(100.0);
stats.update(101.0);
stats.update(99.5);

// 获取统计量
let mean = stats.mean();
let std = stats.std();
let zscore = stats.zscore(102.0);  // 计算 102.0 的 z-score

// 检查是否预热完成
if stats.is_ready() {
    println!("窗口已满，统计量可靠");
}
```

### EMA - 指数移动平均

```rust
use factor_engine::EMA;

// 创建 EMA (span=20，类似 pandas ewm)
let mut ema = EMA::new(20);

// 或者指定 alpha
let mut ema = EMA::with_alpha(0.1);

// 更新
let value = ema.update(100.0);
let value = ema.update(101.0);

// 获取当前值
let current = ema.value();
```

### TimeWindowQueue - 时间窗口队列

按时间而非样本数滑动的窗口：

```rust
use factor_engine::TimeWindowQueue;

// 创建 5 分钟窗口 (纳秒)
let mut queue = TimeWindowQueue::new(5 * 60 * 1_000_000_000);

// 添加数据 (timestamp, value)
queue.update(timestamp_ns, trade_volume);

// 获取聚合值
let total_volume = queue.sum();
let avg_volume = queue.mean();
let count = queue.count();
```

## 内置因子

### OBIFactor - 订单簿不平衡

计算买卖盘深度差异的 z-score：

```rust
use factor_engine::OBIFactor;

// 创建 OBI 因子
let mut obi = OBIFactor::new(
    0.001,      // looking_depth: 看 0.1% 深度
    0.1,        // tick_size
    0,          // roi_lb_tick: 价格下界 tick
    2000000,    // roi_ub_tick: 价格上界 tick
    3600,       // window: z-score 窗口大小
);

// 方式 1: 使用深度数组
let zscore = obi.update(
    &bid_depth_array,
    &ask_depth_array,
    best_bid_tick,
    best_ask_tick,
    mid_price,
);

// 方式 2: 使用访问器函数 (推荐，适用于 HashMapMarketDepth)
let zscore = obi.update_with_accessor(
    best_bid_tick,
    best_ask_tick,
    mid_price,
    |tick| depth.bid_qty_at_tick(tick),  // bid 访问器
    |tick| depth.ask_qty_at_tick(tick),  // ask 访问器
);

// 获取各种值
let raw_imbalance = obi.raw();      // 原始不平衡值
let zscore = obi.zscore();          // z-score 标准化值
let rolling_mean = obi.mean();      // 滚动均值
let rolling_std = obi.std();        // 滚动标准差

// 检查预热
if obi.is_ready() {
    // 因子可用
}
```

### VolatilityFactor - 波动率因子

计算价格收益率的滚动标准差：

```rust
use factor_engine::VolatilityFactor;

// 创建波动率因子
let mut vol = VolatilityFactor::new(
    60_000_000_000,  // 60 秒采样间隔
    60,              // 60 个样本窗口
);

// 更新 (每个 tick 都调用，内部自动按时间采样)
let volatility = vol.update(timestamp_ns, mid_price);

// 获取当前波动率
let current_vol = vol.value();

// 检查预热
if vol.is_ready() {
    // 波动率可靠
}
```

### StopLossChecker - 止损检测

基于波动率和价格变化的止损检测：

```rust
use factor_engine::{StopLossChecker, StopLossStatus};

// 创建止损检测器
let mut stop_loss = StopLossChecker::new(
    0.005,   // volatility_mean: 历史波动率均值
    0.002,   // volatility_std: 历史波动率标准差
    0.001,   // change_mean: 历史价格变化均值
    0.003,   // change_std: 历史价格变化标准差
    3.0,     // volatility_threshold: 波动率 z-score 阈值
    3.0,     // change_threshold: 价格变化 z-score 阈值
);

// 检查止损状态
stop_loss.check(current_volatility, minute_price_change);

match stop_loss.status() {
    StopLossStatus::Normal => {
        // 正常交易
    }
    StopLossStatus::VolatilityTriggered => {
        // 波动率过高，清仓
    }
    StopLossStatus::ChangeTriggered => {
        // 价格变化过大，清仓
    }
}

// 重置状态
stop_loss.reset();
```

## 自定义因子开发

### 实现 Factor Trait

```rust
use factor_engine::factors::Factor;

pub struct MyCustomFactor {
    window: WelfordRolling,
    current_value: f64,
}

impl MyCustomFactor {
    pub fn new(window_size: usize) -> Self {
        Self {
            window: WelfordRolling::new(window_size),
            current_value: 0.0,
        }
    }

    pub fn update(&mut self, input: f64) -> f64 {
        // 自定义计算逻辑
        self.current_value = input * 2.0;  // 示例
        self.window.update(self.current_value);
        self.window.zscore(self.current_value)
    }
}

impl Factor for MyCustomFactor {
    fn value(&self) -> f64 {
        self.current_value
    }

    fn reset(&mut self) {
        self.window.reset();
        self.current_value = 0.0;
    }

    fn is_ready(&self) -> bool {
        self.window.is_ready()
    }
}
```

### 实际示例：Trade Imbalance Factor

```rust
use factor_engine::{TimeWindowQueue, WelfordRolling, Factor};

/// 交易不平衡因子 - 买入成交量 vs 卖出成交量
pub struct TradeImbalanceFactor {
    buy_volume: TimeWindowQueue,
    sell_volume: TimeWindowQueue,
    normalizer: WelfordRolling,
    current_value: f64,
}

impl TradeImbalanceFactor {
    pub fn new(window_ns: i64, zscore_window: usize) -> Self {
        Self {
            buy_volume: TimeWindowQueue::new(window_ns),
            sell_volume: TimeWindowQueue::new(window_ns),
            normalizer: WelfordRolling::new(zscore_window),
            current_value: 0.0,
        }
    }

    pub fn update(&mut self, timestamp: i64, side: Side, volume: f64) -> f64 {
        match side {
            Side::Buy => self.buy_volume.update(timestamp, volume),
            Side::Sell => self.sell_volume.update(timestamp, volume),
        };

        let buy_sum = self.buy_volume.sum();
        let sell_sum = self.sell_volume.sum();
        let total = buy_sum + sell_sum;

        // 计算不平衡 [-1, 1]
        self.current_value = if total > 0.0 {
            (buy_sum - sell_sum) / total
        } else {
            0.0
        };

        // Z-score 标准化
        self.normalizer.update(self.current_value);
        self.normalizer.zscore(self.current_value)
    }
}

impl Factor for TradeImbalanceFactor {
    fn value(&self) -> f64 {
        self.current_value
    }

    fn reset(&mut self) {
        self.buy_volume.clear();
        self.sell_volume.clear();
        self.normalizer.reset();
        self.current_value = 0.0;
    }

    fn is_ready(&self) -> bool {
        self.normalizer.is_ready()
    }
}
```

## 因子组合

### LinearCombiner - 线性组合

```rust
use factor_engine::LinearCombiner;

// 创建因子组合器
let combiner = LinearCombiner::new(&[0.6, -0.4]);

// 组合多个因子值
let obi_value = obi_factor.value();
let vol_value = volatility_factor.value();
let alpha = combiner.compute(&[obi_value, vol_value]);

// alpha = 0.6 * obi_value - 0.4 * vol_value
```

## 集成到策略

```rust
use factor_engine::{OBIFactor, VolatilityFactor, StopLossChecker};

pub struct MyStrategy {
    obi: OBIFactor,
    volatility: VolatilityFactor,
    stop_loss: StopLossChecker,
    tick_size: f64,
}

impl MyStrategy {
    pub fn new(params: &StrategyParams, tick_size: f64) -> Self {
        Self {
            obi: OBIFactor::new(
                params.looking_depth,
                tick_size,
                params.roi_lb_tick,
                params.roi_ub_tick,
                params.obi_window,
            ),
            volatility: VolatilityFactor::new(
                params.volatility_interval_ns,
                params.volatility_window,
            ),
            stop_loss: StopLossChecker::new(
                params.vol_mean, params.vol_std,
                params.change_mean, params.change_std,
                params.vol_threshold, params.change_threshold,
            ),
            tick_size,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.obi.is_ready() && self.volatility.is_ready()
    }

    pub fn update<MD>(&mut self, bot: &mut impl Bot<MD>, asset_no: usize) -> bool
    where
        MD: MarketDepth + L2MarketDepth,
    {
        let depth = bot.depth(asset_no);
        let timestamp = bot.current_timestamp();
        let mid_price = (depth.best_bid() + depth.best_ask()) / 2.0;

        // 更新因子
        let alpha = self.obi.update_with_accessor(
            depth.best_bid_tick(),
            depth.best_ask_tick(),
            mid_price,
            |tick| depth.bid_qty_at_tick(tick),
            |tick| depth.ask_qty_at_tick(tick),
        );

        let volatility = self.volatility.update(timestamp, mid_price);

        // 检查是否可以交易
        if !self.is_ready() {
            return false;
        }

        // 使用因子值计算订单价格
        let fair_price = mid_price * (1.0 + params.c1 * alpha);
        // ... 下单逻辑

        true
    }
}
```

## 性能提示

1. **预热检查**：在 `is_ready()` 返回 true 之前不要使用因子值
2. **批量更新**：如果有多个因子，可以并行更新
3. **避免重复计算**：缓存 `best_bid_tick`, `mid_price` 等值
4. **使用访问器**：`update_with_accessor` 比数组访问更通用

## 下一步

现在你已经了解了 Factor Engine，接下来阅读 [Docker 实盘部署](04-docker-deployment.md) 学习如何将策略部署到实盘。
