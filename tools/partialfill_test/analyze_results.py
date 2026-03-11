#!/usr/bin/env python3
"""Analyze PartialFill vs NoPartialFill comparison results."""

import os
import sys
import glob
import re
import pandas as pd
import numpy as np

RESULTS_DIR = "../../results/partialfill_comparison"


def load_csv(path):
    """Load a backtest result CSV."""
    df = pd.read_csv(path)
    # Compute equity = balance + position * price - fee
    df["equity"] = df["balance"] + df["position"] * df["price"] - df["fee"]
    return df


def compute_metrics(df):
    """Compute key metrics from a backtest result DataFrame."""
    if len(df) < 2:
        return {}

    equity = df["equity"].values
    returns = np.diff(equity)

    total_pnl = equity[-1] - equity[0]
    num_trades = int(df["num_trades"].iloc[-1])
    trading_volume = df["trading_volume"].iloc[-1]
    trading_value = df["trading_value"].iloc[-1]
    total_fee = df["fee"].iloc[-1]
    max_position = df["position"].abs().max()

    # Sharpe (annualized, assuming 1-sec records)
    if returns.std() > 0:
        sharpe = returns.mean() / returns.std() * np.sqrt(86400 * 365)
    else:
        sharpe = 0.0

    # Max drawdown
    cummax = np.maximum.accumulate(equity)
    drawdown = (cummax - equity) / np.where(cummax > 0, cummax, 1.0)
    max_dd = drawdown.max()

    # Average PnL per trade
    avg_pnl_per_trade = total_pnl / num_trades if num_trades > 0 else 0.0

    return {
        "total_pnl": total_pnl,
        "num_trades": num_trades,
        "trading_volume": trading_volume,
        "trading_value": trading_value,
        "total_fee": total_fee,
        "sharpe": sharpe,
        "max_drawdown": max_dd,
        "max_position": max_position,
        "avg_pnl_per_trade": avg_pnl_per_trade,
        "duration_hours": (df["timestamp"].iloc[-1] - df["timestamp"].iloc[0]) / 3.6e12,
    }


def parse_filename(fname):
    """Parse SYMBOL_EXCH_MULTx__0.csv into components."""
    # Pattern: SYMBOL_exchange_MULTx_0.csv (prefix ends with _)
    m = re.match(r"(.+?)_(nopartial|partial)_(\d+)x_0\.csv", fname)
    if m:
        return m.group(1), m.group(2), int(m.group(3))
    return None, None, None


def main():
    results_dir = sys.argv[1] if len(sys.argv) > 1 else RESULTS_DIR

    csv_files = sorted(glob.glob(os.path.join(results_dir, "*_0.csv")))
    if not csv_files:
        print(f"No CSV files found in {results_dir}")
        sys.exit(1)

    rows = []
    for path in csv_files:
        fname = os.path.basename(path)
        symbol, exch, mult = parse_filename(fname)
        if symbol is None:
            continue

        df = load_csv(path)
        metrics = compute_metrics(df)
        metrics["symbol"] = symbol
        metrics["exchange"] = exch
        metrics["qty_multiplier"] = mult
        metrics["file"] = fname
        rows.append(metrics)

    results = pd.DataFrame(rows)
    if results.empty:
        print("No valid results found.")
        sys.exit(1)

    # Summary table
    print("=" * 80)
    print("SUMMARY: PartialFill vs NoPartialFill Comparison")
    print("=" * 80)

    cols = ["symbol", "exchange", "qty_multiplier", "total_pnl", "num_trades",
            "sharpe", "max_drawdown", "avg_pnl_per_trade", "max_position"]
    print(results[cols].to_string(index=False, float_format="%.4f"))

    # Scale sensitivity analysis
    print("\n" + "=" * 80)
    print("SCALE SENSITIVITY (PnL ratio vs 1x baseline)")
    print("=" * 80)

    for symbol in results["symbol"].unique():
        print(f"\n--- {symbol} ---")
        sym_data = results[results["symbol"] == symbol]

        for exch in ["nopartial", "partial"]:
            exch_data = sym_data[sym_data["exchange"] == exch].sort_values("qty_multiplier")
            baseline = exch_data[exch_data["qty_multiplier"] == 1]
            if baseline.empty:
                continue

            base_pnl = baseline["total_pnl"].values[0]
            base_trades = baseline["num_trades"].values[0]

            print(f"  {exch}:")
            for _, row in exch_data.iterrows():
                mult = row["qty_multiplier"]
                pnl_ratio = row["total_pnl"] / base_pnl if base_pnl != 0 else float("inf")
                trade_ratio = row["num_trades"] / base_trades if base_trades > 0 else 0
                print(f"    {mult:2d}x: PnL={row['total_pnl']:10.2f}  "
                      f"ratio={pnl_ratio:6.2f}x  "
                      f"trades={row['num_trades']:6d}  "
                      f"trade_ratio={trade_ratio:.3f}  "
                      f"sharpe={row['sharpe']:.2f}")

    # PartialFill vs NoPartialFill at 1x (correlation)
    print("\n" + "=" * 80)
    print("PARTIAL vs NOPARTIAL AT 1x")
    print("=" * 80)

    for symbol in results["symbol"].unique():
        sym_data = results[results["symbol"] == symbol]
        np1 = sym_data[(sym_data["exchange"] == "nopartial") & (sym_data["qty_multiplier"] == 1)]
        p1 = sym_data[(sym_data["exchange"] == "partial") & (sym_data["qty_multiplier"] == 1)]
        if np1.empty or p1.empty:
            continue

        np_pnl = np1["total_pnl"].values[0]
        p_pnl = p1["total_pnl"].values[0]
        np_trades = np1["num_trades"].values[0]
        p_trades = p1["num_trades"].values[0]
        fill_rate = p_trades / np_trades if np_trades > 0 else 0

        print(f"  {symbol}:")
        print(f"    NoPartial PnL: {np_pnl:.2f}, trades: {np_trades}")
        print(f"    Partial   PnL: {p_pnl:.2f}, trades: {p_trades}")
        print(f"    Fill rate (partial/nopartial): {fill_rate:.3f}")

    # Save summary CSV
    summary_path = os.path.join(results_dir, "summary.csv")
    results.to_csv(summary_path, index=False)
    print(f"\nSummary saved to {summary_path}")


if __name__ == "__main__":
    main()
