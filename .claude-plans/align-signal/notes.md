# Notes: 策略对齐差异分析

## Phase 1: Signal计算差异

### 差异1: 标准差计算方法 (ddof) - 重要!

| 项目 | Python (numpy) | Rust (WelfordRolling) |
|------|----------------|----------------------|
| 函数 | `np.nanstd()` | `WelfordRolling::std()` |
| 公式 | `sqrt(sum((x-mean)^2) / N)` | `sqrt(M2 / (N-1))` |
| ddof | **ddof=0** (总体标准差) | **ddof=1** (样本标准差) |

**位置:**
- Python: `mm_logic_v18.py:131` - `s = np.nanstd(...)`
- Rust: `primitives.rs:165` - `self.m2 / (n - 1) as f64`

**影响:** 当窗口大小为3600时，差异约为 `sqrt(3600/3599) ≈ 1.00014`，影响较小但存在。

---

### 差异2: Volatility标准差也有ddof差异

同样的问题存在于volatility计算中：
- Python: `mm_logic_v18.py:103` - `np.nanstd(change_timeseries[...])`，使用 ddof=0
- Rust: `volatility.rs:91` - 使用 `WelfordRolling::std()`，即 ddof=1

---

### 差异3: OBI循环边界 - 已确认一致 ✓

---

### 差异4: Update时机 - 已确认一致 ✓

---

## Phase 2: Position计算差异

### 订单生成逻辑对比

#### Python (mm_logic_v18.py:162-182)
```python
new_bid_orders = Dict.empty(uint64, float64)
if position < g.max_position and np.isfinite(bid_price):
    for i in range(g.grid_num):
        bid_price_tick = round(bid_price / tick_size)
        new_bid_orders[uint64(bid_price_tick)] = bid_price
        bid_price -= g.grid_interval
```

#### Rust (strategy.rs:246-263)
```rust
let mut new_bid_orders: HashMap<u64, f64> = HashMap::new();
if position < self.global.max_position && bid_price.is_finite() {
    for _ in 0..self.global.grid_num {
        let bid_price_tick = (bid_price / self.tick_size).round() as u64;
        new_bid_orders.insert(bid_price_tick, bid_price);
        bid_price -= self.global.grid_interval;
    }
}
```

**逻辑一致** ✓

### Effective Position计算

#### Python (mm_logic_v18.py:220)
```python
if (position + 0.3 * holding_buy_order_num * g.order_qty) < g.max_position:
```

#### Rust (strategy.rs:327-328)
```rust
let effective_position = position + 0.3 * holding_buy_order_num as f64 * self.global.order_qty;
if effective_position < self.global.max_position {
```

**逻辑一致** ✓

### holding_order_num计算 - 需要仔细核对

两者都在循环中计算，逻辑相似。

---

## Phase 3: StopLoss差异 - 重大差异!

### 差异5: 止损触发条件不同!

#### Python (core/utils.py:67-72)
```python
def check_stop_loss(self, volatility, change, timestamp, price, symbol_id):
    prev_status = self.status
    # 触发条件: volatility zscore >= threshold AND change zscore >= threshold * 2
    if (volatility - self.volatility_mean) / self.volatility_std >= self.threshold \
       and np.abs(change - self.change_mean) / self.change_std >= self.threshold * 2:
        self.status = 1
    # 恢复条件: volatility zscore <= threshold / 2
    elif (volatility - self.volatility_mean) / self.volatility_std <= self.threshold / 2:
        self.status = 0
    return self.status
```

#### Rust (stop_loss.rs:109-131)
```rust
pub fn check(&mut self, volatility: f64, minute_change: f64) -> StopLossStatus {
    let vol_zscore = (volatility - self.volatility_mean) / self.volatility_std;
    let change_zscore = (minute_change.abs() - self.change_mean) / self.change_std;

    // 触发条件: vol_zscore > threshold OR change_zscore > threshold
    if vol_zscore > self.volatility_threshold || change_zscore > self.change_threshold {
        self.status = StopLossStatus::Triggered;
    } else {
        self.status = StopLossStatus::Normal;
    }
    self.status
}
```

### 关键差异总结

| 特性 | Python | Rust |
|------|--------|------|
| 触发逻辑 | `vol >= thresh AND change >= thresh*2` | `vol > thresh OR change > thresh` |
| 比较运算符 | `>=` | `>` |
| change计算 | `abs(change - change_mean)` | `abs(change) - change_mean` |
| 恢复逻辑 | `vol <= thresh/2` 时恢复 | 无滞后，立即恢复 |
| 状态保持 | 有滞后机制 | 无滞后机制 |

### 差异6: change zscore计算方式不同

#### Python
```python
np.abs(change - self.change_mean) / self.change_std
```
先减去mean，再取绝对值

#### Rust
```rust
(minute_change.abs() - self.change_mean) / self.change_std
```
先取绝对值，再减去mean

**这是一个重要差异!**

---

## 需要修复的差异汇总

| 序号 | 差异 | 影响 | 优先级 |
|------|------|------|--------|
| 1 | ddof (0 vs 1) | OBI z-score, Volatility | 中 |
| 2 | 止损触发逻辑 (AND vs OR) | 止损时机完全不同 | **高** |
| 3 | 止损恢复逻辑 (有/无滞后) | 止损持续时间不同 | **高** |
| 4 | change zscore计算 | 止损触发条件 | **高** |
| 5 | 比较运算符 (>= vs >) | 边界情况 | 低 |

---

## 修复方案

### 方案A: 修改Rust以匹配Python

1. 添加 `population_std()` 方法到 WelfordRolling
2. 修改 StopLossChecker:
   - 改为 AND 逻辑
   - 添加滞后恢复机制
   - 修正 change zscore 计算
   - 使用 `>=` 而非 `>`

### 方案B: 修改Python以匹配Rust

(如果Rust的逻辑更合理)

---

## 验证计划

1. 修复差异后，使用相同数据运行两个回测
2. 打印每次止损触发的时间戳进行对比
3. 对比最终position和PnL
