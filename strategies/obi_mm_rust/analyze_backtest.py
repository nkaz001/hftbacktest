#!/usr/bin/env python3
"""
Backtest Result Analyzer for OBI MM Strategy (Rust version)

Generates visualization and metrics similar to Python backtest output.
"""

import pandas as pd
import numpy as np
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
from datetime import datetime
import json
import argparse
from pathlib import Path


def load_backtest_data(csv_path: str) -> pd.DataFrame:
    """Load backtest CSV data."""
    df = pd.read_csv(csv_path)

    # Convert timestamp (nanoseconds) to datetime
    df['datetime'] = pd.to_datetime(df['timestamp'], unit='ns')
    df.set_index('datetime', inplace=True)

    return df


def calculate_metrics(df: pd.DataFrame, initial_capital: float = 10000.0) -> dict:
    """Calculate key performance metrics."""

    # Calculate equity = balance + position * price
    df['equity'] = df['balance'] + df['position'] * df['price']

    # Calculate equity without fee (balance - fee + position * price)
    df['equity_wo_fee'] = df['balance'] - df['fee'] + df['position'] * df['price']

    # Add initial capital to equity for proper return calculation
    df['equity_total'] = initial_capital + df['equity']
    df['equity_total_wo_fee'] = initial_capital + df['equity_wo_fee']

    # Calculate returns relative to initial capital
    df['cumulative_returns'] = df['equity'] / initial_capital
    df['cumulative_returns_wo_fee'] = df['equity_wo_fee'] / initial_capital

    # For metrics calculation, use the total equity series
    equity = df['equity_total']

    # Calculate returns for Sharpe ratio (use 1-minute resampled data)
    equity_resampled = equity.resample('1min').last().dropna()

    # Filter out zero/near-zero values to avoid division issues
    equity_resampled = equity_resampled[equity_resampled > 1]

    returns = equity_resampled.pct_change().dropna()
    returns = returns[np.isfinite(returns)]  # Remove inf/nan

    # Sharpe Ratio (annualized, assuming 525600 minutes per year)
    if len(returns) > 1 and returns.std() > 0:
        sharpe_ratio = returns.mean() / returns.std() * np.sqrt(525600)
    else:
        sharpe_ratio = 0.0

    # Sortino Ratio (only downside deviation)
    downside_returns = returns[returns < 0]
    if len(downside_returns) > 1 and downside_returns.std() > 0:
        sortino_ratio = returns.mean() / downside_returns.std() * np.sqrt(525600)
    else:
        sortino_ratio = 0.0

    # Max Drawdown (based on total equity)
    cummax = equity.cummax()
    cummax = cummax.replace(0, np.nan)  # Avoid division by zero
    drawdown = (equity - cummax) / cummax
    max_drawdown = abs(drawdown.min()) if not drawdown.empty and np.isfinite(drawdown.min()) else 0.0

    # Total Return
    total_return = (equity.iloc[-1] - equity.iloc[0]) / initial_capital if initial_capital > 0 else 0

    # Return/Drawdown Ratio
    return_dd_ratio = total_return / max_drawdown if max_drawdown > 0 else 0

    # Trading stats
    total_trades = df['num_trades'].iloc[-1]
    total_volume = df['trading_volume'].iloc[-1]
    total_value = df['trading_value'].iloc[-1]

    # Time range
    time_range_hours = (df.index[-1] - df.index[0]).total_seconds() / 3600

    # Daily averages (extrapolated)
    daily_trades = total_trades / time_range_hours * 24 if time_range_hours > 0 else 0
    daily_volume = total_volume / time_range_hours * 24 if time_range_hours > 0 else 0

    # Final values
    final_balance = df['balance'].iloc[-1]
    final_position = df['position'].iloc[-1]
    final_fee = df['fee'].iloc[-1]

    metrics = {
        '夏普比率': round(sharpe_ratio, 3),
        '索提诺比率': round(sortino_ratio, 3),
        '总收益率': round(total_return, 4),
        '最大回撤': round(max_drawdown, 4),
        '收益回撤比': round(return_dd_ratio, 2),
        '日均交易': round(daily_trades, 0),
        '日均换手': round(daily_volume, 2),
        '总交易次数': int(total_trades),
        '总交易量': round(total_volume, 4),
        '总交易额': round(total_value, 2),
        '最终余额': round(final_balance, 4),
        '最终持仓': round(final_position, 6),
        '手续费收入': round(-final_fee, 4),  # Negative fee means rebate
    }

    return metrics, df


def plot_backtest_results(
    df: pd.DataFrame,
    metrics: dict,
    symbol: str,
    params: dict,
    output_path: str
):
    """Generate backtest visualization."""

    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(14, 10), sharex=True)

    # Get date range for title
    start_date = df.index[0].strftime('%Y-%m-%d_%H%M%S')
    end_date = df.index[-1].strftime('%Y-%m-%d_%H%M%S')

    fig.suptitle(f'{symbol} Backtest Results ({start_date}_{end_date})', fontsize=14, fontweight='bold')

    # Resample for cleaner plotting (every minute)
    df_plot = df.resample('1min').last().dropna()

    # === Upper plot: Equity and Price ===
    ax1_twin = ax1.twinx()

    # Plot price on right axis (gray, background)
    ax1_twin.plot(df_plot.index, df_plot['price'], color='gray', alpha=0.5, linewidth=1, label='Price')
    ax1_twin.set_ylabel('Price (USDT)', color='gray')
    ax1_twin.tick_params(axis='y', labelcolor='gray')

    # Plot cumulative returns on left axis
    ax1.plot(df_plot.index, df_plot['cumulative_returns'] * 100, color='blue', linewidth=1.5, label='Equity')
    ax1.plot(df_plot.index, df_plot['cumulative_returns_wo_fee'] * 100, color='green', linewidth=1.5, label='Equity w/o fee')

    ax1.set_ylabel('Cumulative Returns (%)')
    ax1.legend(loc='upper left')
    ax1_twin.legend(loc='upper right')
    ax1.grid(True, alpha=0.3)
    ax1.axhline(y=0, color='black', linestyle='-', linewidth=0.5)

    # === Lower plot: Position and Price ===
    ax2_twin = ax2.twinx()

    # Plot price on right axis (gray, background)
    ax2_twin.plot(df_plot.index, df_plot['price'], color='gray', alpha=0.5, linewidth=1, label='Price')
    ax2_twin.set_ylabel('Price (USDT)', color='gray')
    ax2_twin.tick_params(axis='y', labelcolor='gray')

    # Plot position on left axis
    ax2.plot(df_plot.index, df_plot['position'], color='red', linewidth=1, label='Position')
    ax2.set_ylabel('Position (Qty)')
    ax2.legend(loc='upper left')
    ax2_twin.legend(loc='upper right')
    ax2.grid(True, alpha=0.3)
    ax2.axhline(y=0, color='black', linestyle='-', linewidth=0.5)

    ax2.set_xlabel('Time')

    # Format x-axis
    ax2.xaxis.set_major_formatter(mdates.DateFormatter('%m-%d %H'))
    plt.xticks(rotation=45)

    # Add parameter and metrics text box
    param_text = f"Parameters: half_spread={params.get('half_spread', 'N/A')}, skew={params.get('skew', 'N/A')}, c1={params.get('c1', 'N/A')}, depth={params.get('looking_depth', 'N/A')}\n"
    param_text += f"interval={params.get('update_interval_ns', 0) // 1_000_000_000}s, window={params.get('window', 'N/A')}\n"
    param_text += f"Sharpe Ratio: {metrics['夏普比率']}, Max Drawdown: {metrics['最大回撤']}"

    fig.text(0.02, 0.02, param_text, fontsize=9,
             bbox=dict(boxstyle='round', facecolor='wheat', alpha=0.5),
             verticalalignment='bottom')

    plt.tight_layout()
    plt.subplots_adjust(bottom=0.15)

    # Save figure
    plt.savefig(output_path, dpi=150, bbox_inches='tight')
    plt.close()

    print(f"Plot saved to: {output_path}")


def save_metrics_json(
    metrics: dict,
    symbol: str,
    params: dict,
    date_range: str,
    output_path: str
):
    """Save metrics to JSON file."""

    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')

    output = {
        'symbol': symbol,
        'strategy_params': params,
        'date_range': date_range,
        'timestamp': timestamp,
        'key_metrics': {
            '夏普比率': metrics['夏普比率'],
            '索提诺比率': metrics['索提诺比率'],
            '总收益率': metrics['总收益率'],
            '最大回撤': metrics['最大回撤'],
            '收益回撤比': metrics['收益回撤比'],
            '日均交易': metrics['日均交易'],
            '日均换手': metrics['日均换手'],
        },
        'detailed_metrics': metrics
    }

    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(output, f, indent=2, ensure_ascii=False)

    print(f"Metrics saved to: {output_path}")


def main():
    parser = argparse.ArgumentParser(description='Analyze OBI MM backtest results')
    parser.add_argument('--csv', type=str, default='/tmp/backtest_results/obi_mm0.csv',
                        help='Path to backtest CSV file')
    parser.add_argument('--output-dir', type=str, default='/tmp/backtest_results',
                        help='Output directory for plots and metrics')
    parser.add_argument('--symbol', type=str, default='BTCUSDT',
                        help='Trading symbol')
    parser.add_argument('--initial-capital', type=float, default=10000.0,
                        help='Initial capital for return calculation')

    # Strategy parameters (for documentation in output)
    parser.add_argument('--half-spread', type=float, default=0.0002)
    parser.add_argument('--skew', type=float, default=0.01)
    parser.add_argument('--c1', type=float, default=0.0001)
    parser.add_argument('--looking-depth', type=float, default=0.001)
    parser.add_argument('--window', type=int, default=3600)
    parser.add_argument('--update-interval-ns', type=int, default=10_000_000_000)

    args = parser.parse_args()

    # Load data
    print(f"Loading data from: {args.csv}")
    df = load_backtest_data(args.csv)
    print(f"Loaded {len(df)} rows, from {df.index[0]} to {df.index[-1]}")

    # Calculate metrics
    print("Calculating metrics...")
    metrics, df = calculate_metrics(df, args.initial_capital)

    # Print metrics
    print("\n" + "=" * 50)
    print("BACKTEST METRICS")
    print("=" * 50)
    for key, value in metrics.items():
        print(f"  {key}: {value}")
    print("=" * 50 + "\n")

    # Prepare parameters dict
    params = {
        'half_spread': args.half_spread,
        'skew': args.skew,
        'c1': args.c1,
        'looking_depth': args.looking_depth,
        'window': args.window,
        'update_interval_ns': args.update_interval_ns,
    }

    # Create output directory
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Generate date range string
    start_date = df.index[0].strftime('%Y-%m-%d_%H%M%S')
    end_date = df.index[-1].strftime('%Y-%m-%d_%H%M%S')
    date_range = f"{start_date}_{end_date}"

    # Generate plot
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    plot_path = output_dir / f"{args.symbol}_backtest_{timestamp}.png"
    plot_backtest_results(df, metrics, args.symbol, params, str(plot_path))

    # Save metrics JSON
    json_path = output_dir / f"strategy_params_{timestamp}.json"
    save_metrics_json(metrics, args.symbol, params, date_range, str(json_path))

    print(f"\nOutput files:")
    print(f"  Plot: {plot_path}")
    print(f"  Metrics: {json_path}")


if __name__ == '__main__':
    main()
