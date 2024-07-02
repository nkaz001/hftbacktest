import json
import multiprocessing
import os
import os.path
import subprocess
from datetime import timedelta, datetime

import numpy as np
from sklearn.model_selection import ParameterGrid

date_from = 20240501
date_to = 20240531

# The path for the converted npz files for the Rust version.
npz_path = '.'

# The path where the backtesting result is saved.
out_path = '.'

# The path where the example backtesting program, "gridtrading_backtest_args", is located.
backtest_program = './gridtrading_backtest_args'

# Sets the number of processors for parallel processing during backtesting. The backtesting program itself doesn't use
# multiprocessing, but it runs multiple backtests for each pair in parallel to speed up the process.
num_processors = 8


with open('tickers.json', 'r') as f:
    tickers = json.load(f)


def backtest_rust(
        symbol,
        date_from_,
        date_to_,
        tick_size,
        lot_size,
        rel_half_spread,
        rel_grid_interval,
        grid_num,
        skew,
        order_qty,
        max_position
):
    date = datetime.strptime(str(date_from_), '%Y%m%d')
    date_to_ = datetime.strptime(str(date_to_), '%Y%m%d')
    dates = []
    while date <= date_to_:
        dates.append(date.strftime('%Y%m%d'))
        date += timedelta(days=1)
    data_files = ' '.join([os.path.join(npz_path, f'{symbol}_{yyyymmdd}.npz') for yyyymmdd in dates])
    latency_files = ' '.join([os.path.join(npz_path, f'latency_{yyyymmdd}.npz') for yyyymmdd in dates])
    cmd = (
        f'{backtest_program} '
        f'--name {symbol}_{rel_half_spread}_{rel_grid_interval}_{grid_num}_{skew}_ '
        f'--data-files {data_files} '
        f'--latency-files {latency_files} '
        f'--output-path {out_path} '
        f'--tick-size {tick_size} '
        f'--lot-size {lot_size} '
        f'--relative-half-spread {rel_half_spread} '
        f'--relative-grid-interval {rel_grid_interval} '
        f'--grid-num {grid_num} '
        f'--skew {skew} '
        f'--order-qty {order_qty} '
        f'--max-position {max_position} '
    )
    return_code = subprocess.call(cmd, shell=True)
    print(f'{symbol}: {return_code}\n')


# Sets parameters for the given symbol. To reduce the number of parameter sets to search, there uses the same value for
# grid_interval as for rel_half_spread, and skew is normalized by grid_num and rel_half_spread. You can also search
# these parameters to broaden the search space. However, this may increase the risk of overfitting. You need to
# carefully select and limit the search space.
def params(symbol, rel_half_spread, grid_num):
    tick_size = tickers[symbol]['tick_size']
    lot_size = tickers[symbol]['lot_size']
    min_qty = tickers[symbol]['min_qty']
    rel_half_spread = rel_half_spread
    grid_num = grid_num
    rel_grid_interval = rel_half_spread
    skew = rel_half_spread / grid_num

    # Order quantity is set to be equivalent to about $100.
    if symbol.startswith('1000'):
        order_qty100 = round(
            (100 / (1000 * float(tickers[symbol]['weighted_avg_price']))) / float(lot_size)
        ) * float(lot_size)
    else:
        order_qty100 = round((100 / float(tickers[symbol]['weighted_avg_price'])) / float(lot_size)) * float(lot_size)
    order_qty = max(float(min_qty), order_qty100)
    max_position = grid_num * order_qty

    return (
        symbol,
        date_from,
        date_to,
        tick_size,
        lot_size,
        rel_half_spread,
        rel_grid_interval,
        grid_num,
        skew,
        order_qty,
        max_position
    )


param_grid = {
    'symbol': list(tickers.keys()),
    'rel_half_spread': [0.0004, 0.0005, 0.0006, 0.0007, 0.0008],
    'grid_num': [5, 10, 15, 20]
}

grid = ParameterGrid(param_grid)
args = [params(**p) for p in grid]
with multiprocessing.Pool(num_processors) as pool:
    pool.starmap(backtest_rust, args)
