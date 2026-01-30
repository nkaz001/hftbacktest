#!/usr/bin/env python3
"""
Backtest Alignment Tool - Compare Rust and Python backtest results

This tool computes correlation metrics and generates comparison charts
to verify alignment between Python and Rust strategy implementations.

Usage:
    python compare.py --rust <rust_csv> --python <python_npz> [--output <dir>]

Example:
    python compare.py --rust obi_mm0.csv --python obi_mm_python.npz --output ./results
"""

import argparse
import sys
from pathlib import Path
from dataclasses import dataclass
from typing import Optional, Tuple

import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
from scipy import stats


@dataclass
class AlignmentMetrics:
    """Alignment metrics between two backtests."""
    pnl_pearson: float
    pnl_spearman: float
    position_corr: float
    balance_corr: float
    final_pnl_diff: float
    final_position_diff: float
    max_position_diff: float
    valid_samples: int

    def summary(self) -> str:
        return f"""
=== Alignment Metrics ===
PnL Pearson Correlation:    {self.pnl_pearson:.6f}
PnL Spearman Correlation:   {self.pnl_spearman:.6f}
Position Correlation:       {self.position_corr:.6f}
Balance Correlation:        {self.balance_corr:.6f}

Final PnL Difference:       {self.final_pnl_diff:.4f} USDT
Final Position Difference:  {self.final_position_diff:.6f} BTC
Max Position Difference:    {self.max_position_diff:.6f} BTC
Valid Samples:              {self.valid_samples}

Alignment Status: {'GOOD' if self.pnl_pearson > 0.95 else 'NEEDS WORK' if self.pnl_pearson > 0.8 else 'POOR'}
"""


def load_rust_results(path: str) -> pd.DataFrame:
    """Load Rust backtest results from CSV."""
    df = pd.read_csv(path)

    # Standardize column names
    if 'price' in df.columns and 'mid' not in df.columns:
        df['mid'] = df['price']

    # Handle NaN in mid price
    df['mid'] = df['mid'].ffill().bfill()

    # Calculate equity and PnL
    df['equity'] = df['balance'] + df['fee'] + df['position'] * df['mid']
    first_valid = df['equity'].first_valid_index()
    baseline = df.loc[first_valid, 'equity'] if first_valid is not None else 0
    df['pnl'] = df['equity'] - baseline

    # Convert timestamp to datetime
    df['datetime'] = pd.to_datetime(df['timestamp'], unit='ns')

    return df


def load_python_results(path: str) -> pd.DataFrame:
    """Load Python backtest results from NPZ."""
    data = np.load(path)

    # Handle different NPZ formats
    if '0' in data.files:
        # Structured array format
        arr = data['0']
        df = pd.DataFrame({
            'timestamp': arr['timestamp'],
            'mid': arr['price'],
            'balance': arr['balance'],
            'position': arr['position'],
            'fee': arr['fee'],
            'num_trades': arr['num_trades'],
            'trading_volume': arr['trading_volume'],
            'trading_value': arr['trading_value']
        })
    else:
        # Separate arrays format
        df = pd.DataFrame({
            'timestamp': data['timestamp'],
            'mid': data.get('mid', data.get('price', np.nan)),
            'balance': data['balance'],
            'position': data['position'],
            'fee': data['fee'],
        })

    # Handle NaN in mid price
    df['mid'] = df['mid'].ffill().bfill()

    # Calculate equity and PnL
    df['equity'] = df['balance'] + df['fee'] + df['position'] * df['mid']
    first_valid = df['equity'].first_valid_index()
    baseline = df.loc[first_valid, 'equity'] if first_valid is not None else 0
    df['pnl'] = df['equity'] - baseline

    # Convert timestamp to datetime
    df['datetime'] = pd.to_datetime(df['timestamp'], unit='ns')

    return df


def compute_alignment_metrics(
    rust_df: pd.DataFrame,
    python_df: pd.DataFrame,
    tolerance_ns: int = 10_000_000_000  # 10 seconds
) -> Tuple[AlignmentMetrics, pd.DataFrame]:
    """Compute alignment metrics between Rust and Python results."""

    # Merge on timestamp
    merged = pd.merge_asof(
        python_df.sort_values('timestamp'),
        rust_df.sort_values('timestamp'),
        on='timestamp',
        suffixes=('_py', '_rust'),
        direction='nearest',
        tolerance=tolerance_ns
    )

    # Find valid rows
    valid_mask = (
        merged['pnl_py'].notna() &
        merged['pnl_rust'].notna() &
        merged['position_py'].notna() &
        merged['position_rust'].notna()
    )

    valid_data = merged[valid_mask]

    if len(valid_data) < 10:
        raise ValueError(f"Not enough valid data points for comparison: {len(valid_data)}")

    # Compute correlations
    pnl_pearson = valid_data['pnl_py'].corr(valid_data['pnl_rust'])
    pnl_spearman, _ = stats.spearmanr(valid_data['pnl_py'], valid_data['pnl_rust'])
    position_corr = valid_data['position_py'].corr(valid_data['position_rust'])
    balance_corr = valid_data['balance_py'].corr(valid_data['balance_rust'])

    # Compute differences
    position_diff = valid_data['position_py'] - valid_data['position_rust']
    pnl_diff = valid_data['pnl_py'] - valid_data['pnl_rust']

    metrics = AlignmentMetrics(
        pnl_pearson=pnl_pearson,
        pnl_spearman=pnl_spearman,
        position_corr=position_corr,
        balance_corr=balance_corr,
        final_pnl_diff=pnl_diff.iloc[-1],
        final_position_diff=position_diff.iloc[-1],
        max_position_diff=position_diff.abs().max(),
        valid_samples=len(valid_data)
    )

    return metrics, merged


def plot_comparison(
    rust_df: pd.DataFrame,
    python_df: pd.DataFrame,
    merged: pd.DataFrame,
    metrics: AlignmentMetrics,
    output_path: Optional[str] = None
) -> None:
    """Generate comparison plots."""

    fig, axes = plt.subplots(3, 2, figsize=(14, 12))
    fig.suptitle(f'Backtest Alignment Comparison (PnL Corr: {metrics.pnl_pearson:.4f})', fontsize=14)

    # 1. PnL comparison
    ax1 = axes[0, 0]
    ax1.plot(python_df['datetime'], python_df['pnl'], label='Python', alpha=0.8, linewidth=1)
    ax1.plot(rust_df['datetime'], rust_df['pnl'], label='Rust', alpha=0.8, linewidth=1)
    ax1.set_xlabel('Time')
    ax1.set_ylabel('PnL (USDT)')
    ax1.set_title('PnL Comparison')
    ax1.legend()
    ax1.grid(True, alpha=0.3)

    # 2. PnL scatter plot
    ax2 = axes[0, 1]
    valid_mask = merged['pnl_py'].notna() & merged['pnl_rust'].notna()
    ax2.scatter(merged.loc[valid_mask, 'pnl_py'], merged.loc[valid_mask, 'pnl_rust'],
                alpha=0.3, s=1, c='blue')
    min_val = min(merged.loc[valid_mask, 'pnl_py'].min(), merged.loc[valid_mask, 'pnl_rust'].min())
    max_val = max(merged.loc[valid_mask, 'pnl_py'].max(), merged.loc[valid_mask, 'pnl_rust'].max())
    ax2.plot([min_val, max_val], [min_val, max_val], 'r--', alpha=0.5, label='y=x')
    ax2.set_xlabel('Python PnL')
    ax2.set_ylabel('Rust PnL')
    ax2.set_title(f'PnL Scatter (Corr: {metrics.pnl_pearson:.4f})')
    ax2.legend()
    ax2.grid(True, alpha=0.3)

    # 3. Position comparison
    ax3 = axes[1, 0]
    ax3.plot(python_df['datetime'], python_df['position'], label='Python', alpha=0.8, linewidth=1)
    ax3.plot(rust_df['datetime'], rust_df['position'], label='Rust', alpha=0.8, linewidth=1)
    ax3.set_xlabel('Time')
    ax3.set_ylabel('Position (BTC)')
    ax3.set_title('Position Comparison')
    ax3.legend()
    ax3.grid(True, alpha=0.3)

    # 4. Position difference
    ax4 = axes[1, 1]
    dt_col = 'datetime_py' if 'datetime_py' in merged.columns else 'datetime'
    pos_diff = merged['position_py'] - merged['position_rust']
    ax4.plot(merged[dt_col], pos_diff, alpha=0.8, linewidth=1)
    ax4.axhline(y=0, color='r', linestyle='--', alpha=0.5)
    ax4.set_xlabel('Time')
    ax4.set_ylabel('Position Diff (Python - Rust)')
    ax4.set_title(f'Position Difference (Max: {metrics.max_position_diff:.6f})')
    ax4.grid(True, alpha=0.3)

    # 5. Balance comparison
    ax5 = axes[2, 0]
    ax5.plot(python_df['datetime'], python_df['balance'], label='Python', alpha=0.8, linewidth=1)
    ax5.plot(rust_df['datetime'], rust_df['balance'], label='Rust', alpha=0.8, linewidth=1)
    ax5.set_xlabel('Time')
    ax5.set_ylabel('Balance (USDT)')
    ax5.set_title('Balance Comparison')
    ax5.legend()
    ax5.grid(True, alpha=0.3)

    # 6. PnL difference over time
    ax6 = axes[2, 1]
    pnl_diff = merged['pnl_py'] - merged['pnl_rust']
    ax6.plot(merged[dt_col], pnl_diff, alpha=0.8, linewidth=1)
    ax6.axhline(y=0, color='r', linestyle='--', alpha=0.5)
    ax6.set_xlabel('Time')
    ax6.set_ylabel('PnL Diff (Python - Rust)')
    ax6.set_title(f'PnL Difference Over Time (Final: {metrics.final_pnl_diff:.2f})')
    ax6.grid(True, alpha=0.3)

    plt.tight_layout()

    if output_path:
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved plot to: {output_path}")

    return fig


def main():
    parser = argparse.ArgumentParser(
        description='Compare Rust and Python backtest results for alignment verification'
    )
    parser.add_argument('--rust', required=True, help='Path to Rust backtest CSV')
    parser.add_argument('--python', required=True, help='Path to Python backtest NPZ')
    parser.add_argument('--output', default='.', help='Output directory for plots')
    parser.add_argument('--tolerance', type=int, default=10,
                        help='Timestamp tolerance in seconds (default: 10)')
    parser.add_argument('--show', action='store_true', help='Show plot interactively')

    args = parser.parse_args()

    # Load data
    print(f"Loading Rust results from: {args.rust}")
    rust_df = load_rust_results(args.rust)
    print(f"  Loaded {len(rust_df)} rows")

    print(f"Loading Python results from: {args.python}")
    python_df = load_python_results(args.python)
    print(f"  Loaded {len(python_df)} rows")

    # Compute metrics
    print("\nComputing alignment metrics...")
    metrics, merged = compute_alignment_metrics(
        rust_df, python_df,
        tolerance_ns=args.tolerance * 1_000_000_000
    )

    print(metrics.summary())

    # Generate plots
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)
    plot_path = output_dir / 'backtest_comparison.png'

    fig = plot_comparison(rust_df, python_df, merged, metrics, str(plot_path))

    if args.show:
        plt.show()

    # Return exit code based on alignment quality
    if metrics.pnl_pearson < 0.8:
        print("\n[WARNING] Poor alignment - PnL correlation < 0.8")
        return 1
    elif metrics.pnl_pearson < 0.95:
        print("\n[INFO] Moderate alignment - consider further investigation")
        return 0
    else:
        print("\n[SUCCESS] Good alignment achieved!")
        return 0


if __name__ == '__main__':
    sys.exit(main())
