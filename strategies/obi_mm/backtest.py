"""
OBI MM Strategy Backtest - Python Version
Based on mm_logic_v18.py from Bybit_mm

This script runs the OBI MM strategy backtest with the same logic as production.
"""

import argparse
import sys
from typing import List
from dataclasses import dataclass

import numpy as np
from numba import njit, uint64, float64, int64
from numba.typed import Dict

from hftbacktest import (
    BacktestAsset,
    ROIVectorMarketDepthBacktest,
    GTX,
    LIMIT,
    MARKET,
    BUY,
    SELL,
    Recorder,
)
from hftbacktest.order import IOC


# Configuration matching production defaults
@dataclass
class GlobalConfig:
    order_qty: float = 0.001
    max_position: float = 0.015
    grid_num: int = 1
    grid_interval: float = 1.0
    roi_lb: float = 10000.0
    roi_ub: float = 150000.0


@dataclass
class StrategyParams:
    looking_depth: float = 0.025
    window: int = 30
    half_spread: float = 0.002
    skew: float = 0.000023
    c1: float = 0.00024
    power: float = 0.2
    live_seconds: float = 130.0


@dataclass
class StopLossConfig:
    threshold: float = 3.0
    volatility_mean: float = 0.0006142
    volatility_std: float = 0.0004217
    change_mean: float = 0.0000008
    change_std: float = 0.0007443


@njit
def check_stop_loss(
    status: int,
    volatility: float,
    change: float,
    threshold: float,
    volatility_mean: float,
    volatility_std: float,
    change_mean: float,
    change_std: float,
) -> int:
    """
    Stop loss check matching production logic (utils.py StopLossFunc.check_stop_loss)

    Trigger: vol_zscore >= threshold AND change_zscore >= threshold * 2
    Recovery: vol_zscore <= threshold / 2
    """
    vol_zscore = (volatility - volatility_mean) / volatility_std if volatility_std > 0 else 0.0
    change_zscore = np.abs(change - change_mean) / change_std if change_std > 0 else 0.0

    if vol_zscore >= threshold and change_zscore >= threshold * 2:
        return 1
    elif vol_zscore <= threshold / 2:
        return 0
    # Otherwise keep current status (hysteresis)
    return status


@njit
def obi_mm_strategy(
    hbt,
    recorder,
    # Global config
    order_qty: float,
    max_position: float,
    grid_num: int,
    grid_interval: float,
    roi_lb: float,
    roi_ub: float,
    # Strategy params
    looking_depth: float,
    window: int,
    half_spread: float,
    skew: float,
    c1: float,
    power: float,
    live_seconds: float,
    # Stop loss config
    threshold: float,
    volatility_mean: float,
    volatility_std: float,
    change_mean: float,
    change_std: float,
):
    """
    OBI Market Making Strategy - matches mm_logic_v18.py exactly.
    """
    asset_no = 0
    tick_size = hbt.depth(asset_no).tick_size

    # Pre-allocate timeseries
    imbalance_timeseries = np.full(30_000_000, np.nan, np.float64)
    change_timeseries = np.full(30_000_000, np.nan, np.float64)

    t = 0
    change_t = 0
    roi_lb_tick = int(round(roi_lb / tick_size))
    roi_ub_tick = int(round(roi_ub / tick_size))

    last_check_period = -1
    last_change_period = -1
    last_price = 0.0
    stop_loss_status = 0
    volatility = volatility_mean
    minute_change = 0.0

    last_log_time = int64(0)
    update_interval_ns = 10_000_000_000  # 10 seconds

    # Main loop
    while hbt.elapse(1_000_000_000) == 0:  # 1 second steps
        current_timestamp = hbt.current_timestamp

        # Clear filled orders
        hbt.clear_inactive_orders(asset_no)

        # Calculate current period (every 10 seconds)
        current_period = current_timestamp // update_interval_ns

        # Check if we should update
        if current_period <= last_check_period:
            recorder.record(hbt)
            continue

        # Get market data
        depth = hbt.depth(asset_no)
        position = hbt.position(asset_no)

        best_bid = depth.best_bid
        best_ask = depth.best_ask

        # Handle NaN (when depth is not initialized)
        if not (best_bid > 0.0 and best_ask > 0.0):
            recorder.record(hbt)
            continue

        mid_price = (best_bid + best_ask) / 2.0
        best_bid_tick = depth.best_bid_tick
        best_ask_tick = depth.best_ask_tick

        # Volatility tracking (every 60 seconds = 6 periods of 10s)
        change_period = current_period // 6

        if change_period > last_change_period:
            if last_price == 0:
                change_timeseries[change_t] = 0.0
            else:
                change_timeseries[change_t] = mid_price / last_price - 1.0

            minute_change = change_timeseries[change_t]

            # Volatility window is fixed at 60 in original
            if change_t > 60:
                volatility = np.nanstd(change_timeseries[max(0, change_t + 1 - 60):change_t + 1])
            else:
                volatility = volatility_mean

            # Check stop loss with AND condition and hysteresis
            stop_loss_status = check_stop_loss(
                stop_loss_status,
                volatility,
                minute_change,
                threshold,
                volatility_mean,
                volatility_std,
                change_mean,
                change_std,
            )

            last_price = mid_price
            last_change_period = change_period
            change_t += 1

        # Calculate OBI
        sum_ask_qty = 0.0
        from_tick = max(best_ask_tick, roi_lb_tick)
        upto_tick = min(int(np.floor(mid_price * (1 + looking_depth) / tick_size)), roi_ub_tick)
        for price_tick in range(from_tick, upto_tick):
            sum_ask_qty += depth.ask_depth[price_tick - roi_lb_tick]

        sum_bid_qty = 0.0
        from_tick = min(best_bid_tick, roi_ub_tick)
        upto_tick = max(int(np.ceil(mid_price * (1 - looking_depth) / tick_size)), roi_lb_tick)
        for price_tick in range(from_tick, upto_tick, -1):
            sum_bid_qty += depth.bid_depth[price_tick - roi_lb_tick]

        imbalance_timeseries[t] = sum_bid_qty - sum_ask_qty

        # Standardize OBI (z-score) - uses np.nanstd which is ddof=0
        m = np.nanmean(imbalance_timeseries[max(0, t + 1 - window):t + 1])
        s = np.nanstd(imbalance_timeseries[max(0, t + 1 - window):t + 1])
        alpha = np.divide(imbalance_timeseries[t] - m, s) if s > 1e-10 else 0.0

        # Compute prices
        volatility_ratio = (volatility / volatility_mean) ** power
        adjusted_half_spread = half_spread * volatility_ratio

        fair_price = mid_price + c1 * alpha * mid_price
        normalized_position = position / order_qty
        reservation_price = fair_price - skew * normalized_position * mid_price

        bid_price = min(reservation_price - adjusted_half_spread * mid_price, best_bid)
        ask_price = max(reservation_price + adjusted_half_spread * mid_price, best_ask)

        bid_price = np.floor(bid_price / tick_size) * tick_size
        ask_price = np.ceil(ask_price / tick_size) * tick_size

        # Update orders based on stop loss status
        if stop_loss_status == 0:
            # Normal operation - update grid orders
            new_bid_orders = Dict.empty(uint64, float64)
            if position < max_position and np.isfinite(bid_price):
                for i in range(grid_num):
                    bid_price_tick = uint64(round(bid_price / tick_size))
                    new_bid_orders[bid_price_tick] = bid_price
                    bid_price -= grid_interval

            new_ask_orders = Dict.empty(uint64, float64)
            if position > -max_position and np.isfinite(ask_price):
                for i in range(grid_num):
                    ask_price_tick = uint64(round(ask_price / tick_size))
                    new_ask_orders[ask_price_tick] = ask_price
                    ask_price += grid_interval

            # Cancel and count orders
            holding_buy_order_num = 0
            holding_sell_order_num = 0
            orders = hbt.orders(asset_no)
            order_values = orders.values()

            while order_values.has_next():
                order = order_values.get()
                if order.cancellable:
                    dominated = False
                    if order.side == BUY:
                        dominated = order.order_id not in new_bid_orders
                    elif order.side == SELL:
                        dominated = order.order_id not in new_ask_orders

                    if dominated:
                        order_live_seconds = (current_timestamp - order.exch_timestamp) / 1_000_000_000
                        if order_live_seconds >= live_seconds:
                            hbt.cancel(asset_no, order.order_id, False)
                            if order.side == BUY:
                                holding_buy_order_num -= 1
                            else:
                                holding_sell_order_num -= 1
                        else:
                            if order.side == BUY:
                                holding_buy_order_num += 1
                            else:
                                holding_sell_order_num += 1
                    else:
                        if order.side == BUY:
                            holding_buy_order_num += 1
                        else:
                            holding_sell_order_num += 1

            holding_buy_order_num = max(0, holding_buy_order_num)
            holding_sell_order_num = max(0, holding_sell_order_num)

            # Submit new orders
            orders = hbt.orders(asset_no)
            for order_id, order_price in new_bid_orders.items():
                if order_id not in orders:
                    effective_position = position + 0.3 * holding_buy_order_num * order_qty
                    if effective_position < max_position:
                        hbt.submit_buy_order(asset_no, order_id, order_price, order_qty, GTX, LIMIT, False)

            for order_id, order_price in new_ask_orders.items():
                if order_id not in orders:
                    effective_position = position - 0.3 * holding_sell_order_num * order_qty
                    if effective_position > -max_position:
                        hbt.submit_sell_order(asset_no, order_id, order_price, order_qty, GTX, LIMIT, False)
        else:
            # Stop loss triggered - cancel all and close position
            orders = hbt.orders(asset_no)
            order_values = orders.values()
            while order_values.has_next():
                order = order_values.get()
                if order.cancellable:
                    hbt.cancel(asset_no, order.order_id, False)

            position = hbt.position(asset_no)
            order_qty_stop = np.abs(position)
            if order_qty_stop > 1e-9:
                # Log stop loss trigger
                vol_zscore = (volatility - volatility_mean) / volatility_std
                change_zscore = np.abs(minute_change - change_mean) / change_std
                print(change_t, position, current_timestamp, "止损触发", vol_zscore, change_zscore)

                if position > 0:
                    order_price = (np.round(mid_price / tick_size) - 200) * tick_size
                    hbt.submit_sell_order(asset_no, 519, order_price, order_qty_stop, IOC, MARKET, True)
                elif position < 0:
                    order_price = (np.round(mid_price / tick_size) + 200) * tick_size
                    hbt.submit_buy_order(asset_no, 519, order_price, order_qty_stop, IOC, MARKET, True)

        t += 1
        last_check_period = current_period

        # Log progress
        if current_timestamp - last_log_time >= 3600_000_000_000:  # Every hour
            print("Time:", current_timestamp // 1_000_000_000, "Position:", position, "Mid:", mid_price, "StopLoss:", stop_loss_status)
            last_log_time = current_timestamp

        if t >= len(imbalance_timeseries):
            break

        recorder.record(hbt)

    return True


def run_backtest(data_files: List[str], output_path: str = "."):
    """Run OBI MM backtest with production parameters."""

    # Use production configs
    g = GlobalConfig()
    p = StrategyParams()
    sl = StopLossConfig()

    asset = (
        BacktestAsset()
        .data(data_files)
        .linear_asset(1.0)
        .power_prob_queue_model(3.0)
        .no_partial_fill_exchange()
        .trading_value_fee_model(-0.00005, 0.0007)
        .constant_order_latency(1_000_000, 1_000_000)
        .tick_size(0.1)
        .lot_size(0.001)
        .roi_lb(g.roi_lb)
        .roi_ub(g.roi_ub)
    )

    hbt = ROIVectorMarketDepthBacktest([asset])
    recorder = Recorder(1, 100_000)

    print("Running OBI MM backtest (Python version - matching mm_logic_v18.py)...")
    print(f"Data files: {data_files}")

    success = obi_mm_strategy(
        hbt,
        recorder.recorder,
        # Global config
        g.order_qty,
        g.max_position,
        g.grid_num,
        g.grid_interval,
        g.roi_lb,
        g.roi_ub,
        # Strategy params
        p.looking_depth,
        p.window,
        p.half_spread,
        p.skew,
        p.c1,
        p.power,
        p.live_seconds,
        # Stop loss config
        sl.threshold,
        sl.volatility_mean,
        sl.volatility_std,
        sl.change_mean,
        sl.change_std,
    )

    if success:
        print("Backtest completed successfully")
    else:
        print("Backtest ended early")

    # Get final statistics
    state = hbt.state_values(0)
    position = hbt.position(0)

    print("\n=== Backtest Complete ===")
    print(f"Final Position: {position:.4f}")
    print(f"Balance: {state.balance:.4f}")
    print(f"Fee: {state.fee:.4f}")
    print(f"Trade Count: {state.num_trades}")
    print(f"Trade Volume: {state.trading_volume:.4f}")
    print(f"Trade Amount: {state.trading_value:.4f}")

    # Calculate net PnL
    net_pnl = state.balance + state.fee
    print(f"Net PnL: {net_pnl:.4f}")

    # Save recorder data
    import os
    os.makedirs(output_path, exist_ok=True)
    recorder.to_npz(f"{output_path}/obi_mm_python")

    return hbt, state


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="OBI MM Backtest (Python)")
    parser.add_argument(
        "--data",
        nargs="+",
        required=True,
        help="Data files for backtest",
    )
    parser.add_argument(
        "--output",
        default=".",
        help="Output directory for results",
    )
    args = parser.parse_args()

    run_backtest(args.data, args.output)
