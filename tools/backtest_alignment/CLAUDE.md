# Backtest Alignment Tools

Tools for verifying alignment between Python strategy backtests and Rust implementations before live deployment.

## Quick Start
```bash
python tools/backtest_alignment/compare.py \
  --rust obi_mm0.csv \
  --python obi_mm_python.npz \
  --output ./results
```

**Claude Skill:** Use `/align-backtest` to invoke the guided alignment workflow.

## Alignment Thresholds

| Metric | Threshold | Status |
|--------|-----------|--------|
| PnL Pearson | > 0.95 | GOOD - ready for live |
| PnL Pearson | 0.8 - 0.95 | NEEDS WORK - investigate |
| PnL Pearson | < 0.8 | POOR - significant issues |

## Common Alignment Issues

1. **Standard Deviation (ddof)**
   - Python `np.nanstd()` uses ddof=0 (population std)
   - Rust: use `welford.population_std()` not `welford.std()`

2. **Stop Loss Logic**
   - Trigger: `vol_zscore >= threshold AND change_zscore >= threshold * 2`
   - Recovery: `vol_zscore <= threshold / 2` (hysteresis)

3. **Warmup Behavior**
   - Original Python often has NO warmup check
   - Remove `is_ready()` check in Rust if Python trades immediately

4. **Volatility Window Timing**
   - Python: `change_t > 60` (needs 61+ samples)
   - Rust: use `sample_count() > volatility_window`, not `>=`

## API Usage
```python
from tools.backtest_alignment import (
    load_rust_results,
    load_python_results,
    compute_alignment_metrics,
    plot_comparison
)

rust_df = load_rust_results('obi_mm0.csv')
python_df = load_python_results('obi_mm_python.npz')
metrics, merged = compute_alignment_metrics(rust_df, python_df)
print(metrics.summary())
plot_comparison(rust_df, python_df, merged, metrics, 'comparison.png')
```
