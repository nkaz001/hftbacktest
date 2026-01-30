# Backtest Alignment Tools

验证 Python 策略回测与 Rust 实盘实现的对齐程度。

## 快速开始

```bash
# 比较 Rust 和 Python 回测结果
python tools/backtest_alignment/compare.py \
  --rust obi_mm0.csv \
  --python obi_mm_python.npz \
  --output ./results
```

## 输出说明

### 对齐指标

| 指标 | 阈值 | 说明 |
|-----|------|------|
| PnL Pearson | > 0.95 | 线性相关性 |
| PnL Spearman | > 0.90 | 排名相关性 |
| Position Corr | > 0.95 | 持仓相关性 |

### 状态判断

- **GOOD** (> 0.95): 可以上线
- **NEEDS WORK** (0.8 - 0.95): 需要调查
- **POOR** (< 0.8): 严重问题

## 常见对齐问题

### 1. 标准差计算 (ddof)

```python
# Python - 默认 ddof=0 (总体标准差)
np.nanstd(data)
```

```rust
// Rust - 需要明确使用 population_std
let std = welford.population_std();  // ddof=0 ✓
// let std = welford.std();          // ddof=1 ✗
```

### 2. Stop Loss 触发条件

```python
# Python 原始逻辑
if vol_zscore >= threshold and change_zscore >= threshold * 2:
    status = 1  # 触发
elif vol_zscore <= threshold / 2:
    status = 0  # 恢复
```

```rust
// Rust 需要匹配
if vol_zscore >= threshold && change_zscore >= threshold * 2.0 {
    status = Triggered;
} else if vol_zscore <= threshold / 2.0 {
    status = Normal;
}
```

### 3. Warmup 行为

原始 Python 策略通常**没有 warmup 检查**，从第一个周期就开始交易。

```rust
// 如果 Rust 有 warmup 检查，需要移除以对齐
// if !self.is_ready() { return false; }  // 注释掉
```

### 4. 窗口大小

确保 Python 和 Rust 使用相同的窗口：
- OBI window: 30 samples
- Volatility window: 60 samples (每60秒更新一次，需要60分钟warmup)

## 生成的文件

| 文件 | 说明 |
|-----|------|
| `backtest_comparison.png` | 对比图表 |
| `metrics.json` | 对齐指标 (可选) |

## API 使用

```python
from tools.backtest_alignment.compare import (
    load_rust_results,
    load_python_results,
    compute_alignment_metrics,
    plot_comparison
)

# 加载数据
rust_df = load_rust_results('obi_mm0.csv')
python_df = load_python_results('obi_mm_python.npz')

# 计算指标
metrics, merged = compute_alignment_metrics(rust_df, python_df)
print(metrics.summary())

# 生成图表
plot_comparison(rust_df, python_df, merged, metrics, 'comparison.png')
```

## 参考

- [CLAUDE.md](../../CLAUDE.md) - 项目总体文档
- [align-signal/plan.md](../../.claude-plans/align-signal/plan.md) - 对齐工作记录
