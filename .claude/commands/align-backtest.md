# Align Python to Rust Backtest

This skill helps align Python strategy backtests with Rust implementations for live trading deployment.

## Workflow

When the user asks to align a Python strategy with Rust, follow these steps:

### Phase 1: Run Both Backtests

1. **Run Python backtest** with the strategy:
   ```bash
   python strategies/<strategy>/backtest.py --data <data_file> --output /tmp/python_backtest
   ```

2. **Run Rust backtest** with the same config:
   ```bash
   cargo run --release --bin <strategy>_backtest -- \
     --config <config.toml> \
     --data <data_file> \
     --roi --roi-lb <lb> --roi-ub <ub>
   ```

### Phase 2: Compare Results

Run the comparison tool:
```bash
python tools/backtest_alignment/compare.py \
  --rust <rust_csv> \
  --python <python_npz> \
  --output ./alignment_results
```

Interpret the correlation metrics:
- **> 0.95**: Good alignment, ready for live trading
- **0.8 - 0.95**: Moderate alignment, investigate differences
- **< 0.8**: Poor alignment, needs debugging

### Phase 3: Identify Discrepancies

If alignment is poor, check these common issues:

#### 1. Standard Deviation Calculation (ddof)
- Python `np.nanstd()` uses ddof=0 (population std) by default
- Rust `WelfordRolling` has both `std()` (ddof=1) and `population_std()` (ddof=0)
- **Fix**: Use `population_std()` / `population_zscore()` in Rust

#### 2. Stop Loss Logic
- Check trigger condition: should be AND not OR
- Check recovery condition: should have hysteresis (threshold/2)
- Check z-score calculation: `abs(change - mean) / std`, not `abs(change) - mean`

#### 3. Warmup Behavior
- Original Python has NO warmup check
- Rust may have `is_ready()` check that delays trading
- **Fix**: Remove warmup check or align window requirements

#### 4. Volatility Window
- Python often hardcodes values (e.g., `change_t > 60`)
- Rust uses configurable `volatility_window`
- **Fix**: Ensure both use same window size

#### 5. Timing/Interval
- Both should update on same intervals (e.g., every 10 seconds)
- Check `update_interval_ns` and `volatility_interval_ns`

### Phase 4: Apply Fixes

Common fixes in Rust:

```rust
// 1. Use population std instead of sample std
let zscore = self.welford.population_zscore(value);
let volatility = self.volatility_factor.population_std();

// 2. Stop loss with AND condition + hysteresis
if vol_zscore >= threshold && change_zscore >= threshold * 2.0 {
    self.status = StopLossStatus::Triggered;
} else if vol_zscore <= threshold / 2.0 {
    self.status = StopLossStatus::Normal;
}

// 3. Remove warmup check to match Python
// if !self.is_ready() { return false; }  // Comment out
```

### Phase 5: Verify Alignment

Re-run comparison after fixes. Target metrics:
- PnL Pearson Correlation > 0.95
- Position Correlation > 0.95
- Stop Loss triggers at same timestamps

## Files Reference

| File | Purpose |
|------|---------|
| `tools/backtest_alignment/compare.py` | Comparison tool |
| `factor-engine/src/primitives.rs` | WelfordRolling (std calculations) |
| `factor-engine/src/factors/obi.rs` | OBI z-score |
| `factor-engine/src/factors/volatility.rs` | Volatility calculation |
| `factor-engine/src/factors/stop_loss.rs` | Stop loss logic |
| `strategies/obi_mm_rust/src/strategy.rs` | Strategy implementation |

## Example Session

```
User: 对齐 obi_mm 策略的 Python 和 Rust 回测

Claude: 我来帮你对齐策略回测。

1. 首先运行 Python 回测...
2. 然后运行 Rust 回测...
3. 使用 compare.py 比较结果...

结果显示 PnL 相关性为 0.87，需要检查以下问题：
- Stop Loss 逻辑可能不一致
- 检查 Rust 的 std 计算...

修复后重新测试，相关性提升到 0.96，对齐完成！
```

$ARGUMENTS
data_file: Path to the backtest data file (.npz)
strategy: Strategy name (e.g., obi_mm)
config: Path to Rust strategy config (.toml)
