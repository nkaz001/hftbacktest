## tardis-convert — Tardis CSV to HftBacktest NPZ

CLI tool that converts Tardis.dev CSV data (trades + incremental_book_L2) into HftBacktest `.npz` event files.

### Quick Reference

```bash
tardis-convert --list                                # list available symbols
tardis-convert --dry-run                             # preview what needs converting
tardis-convert --symbol BTCUSDT                      # convert all dates for one symbol
tardis-convert --symbol BTCUSDT --date 2025-06-30    # convert single day
tardis-convert --force --symbol BTCUSDT              # re-convert even if output exists
tardis-convert                                       # convert everything
```

### Data Paths

| Type | Path Pattern |
|------|-------------|
| Trades input | `/mnt/data/samba/users/Data/tardis/datasets/binance-futures/trades/{SYMBOL}/{DATE}.csv.gz` |
| Depth input | `/mnt/data/samba/users/Data/tardis/datasets/binance-futures/incremental_book_L2/{SYMBOL}/{DATE}.csv.gz` |
| NPZ output | `/mnt/data/samba/users/Data/tardis/datasets/binance-futures/hft_npz/{SYMBOL}/{SYMBOL}_{YYYYMMDD}.npz` |
| Last snapshot | `/mnt/data/samba/users/Data/tardis/datasets/binance-futures/hft_npz/{SYMBOL}/{SYMBOL}_{YYYYMMDD}_last.npz` |

### Build (if binary missing)

```bash
cargo build --release -p hftbacktest --example tardis_batch_convert \
  --no-default-features --features backtest \
  --manifest-path /home/b0qi/bookdiso/hftbacktest/Cargo.toml
```

### Notes

- Skips already-converted dates unless `--force`
- Generates a `_last.npz` snapshot file alongside each output (final orderbook state, used as next-day warmup)
- Source: `hftbacktest/examples/tardis_batch_convert.rs`
- Wrapper: `~/bin/tardis-convert`
