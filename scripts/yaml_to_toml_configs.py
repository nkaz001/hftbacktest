#!/usr/bin/env python3
"""
Convert fitted YAML parameters to per-symbol TOML strategy configs for live trading.

Usage:
    python scripts/yaml_to_toml_configs.py \
        --yaml /path/to/selected_coins_params_highret.yaml \
        --output deploy/docker/config/strategies/ \
        --capital 2000
"""

import argparse
import json
import math
import sys
import urllib.request
from pathlib import Path

import yaml


def fetch_prices(symbols: list[str]) -> dict[str, float]:
    """Fetch current prices from Binance Futures API."""
    url = "https://fapi.binance.com/fapi/v1/ticker/price"
    resp = urllib.request.urlopen(url)
    data = json.loads(resp.read())
    prices = {item["symbol"]: float(item["price"]) for item in data}
    result = {}
    for s in symbols:
        if s in prices:
            result[s] = prices[s]
        else:
            print(f"WARNING: {s} not found on Binance Futures", file=sys.stderr)
    return result


def fetch_exchange_info(symbols: list[str]) -> dict[str, dict]:
    """Fetch tick_size, lot_size, min_notional from Binance exchange info."""
    url = "https://fapi.binance.com/fapi/v1/exchangeInfo"
    resp = urllib.request.urlopen(url)
    data = json.loads(resp.read())
    result = {}
    for sym_info in data["symbols"]:
        if sym_info["symbol"] in symbols:
            tick_size = None
            lot_size = None
            min_notional = None
            min_qty = None
            for f in sym_info["filters"]:
                if f["filterType"] == "PRICE_FILTER":
                    tick_size = float(f["tickSize"])
                elif f["filterType"] == "LOT_SIZE":
                    lot_size = float(f["stepSize"])
                    min_qty = float(f["minQty"])
                elif f["filterType"] == "MIN_NOTIONAL":
                    min_notional = float(f.get("notional", 0))
            result[sym_info["symbol"]] = {
                "tick_size": tick_size,
                "lot_size": lot_size,
                "min_qty": min_qty,
                "min_notional": min_notional,
            }
    return result


def round_to_lot(qty: float, lot_size: float) -> float:
    """Round quantity down to nearest lot_size, with float cleanup."""
    if lot_size <= 0:
        return qty
    result = math.floor(qty / lot_size) * lot_size
    # Clean up floating point artifacts (e.g. 963.3000000000001 -> 963.3)
    decimals = max(0, -int(math.floor(math.log10(lot_size)))) if lot_size < 1 else 0
    return round(result, decimals)


def generate_toml(symbol: str, params: dict, price: float, capital_per_coin: float,
                  exchange_info: dict) -> str:
    """Generate TOML config string for one symbol."""

    # --- Extract from YAML ---
    c1 = params["c1"]
    depth = params["depth"]
    half_spread = params["half_spread"]
    interval_sec = params["interval_sec"]
    live_seconds = params["live_seconds"]
    power = params["power"]
    skew = params["skew"]
    window_minutes = params["window_minutes"]
    stop_loss_threshold = params["stop_loss_threshold"]

    stop_loss_param = params["stop_loss_param"]
    symbol_config = params["symbol_config"]
    fixed = symbol_config["fixed_params"]

    # Exchange info
    info = exchange_info.get(symbol, {})
    lot_size = info.get("lot_size", symbol_config.get("lot_size", 1.0))
    min_qty = info.get("min_qty", 0.0)
    min_notional = info.get("min_notional", 5.0)

    # --- Compute derived params ---
    # Window: minutes -> samples (one sample per interval_sec)
    window = int(window_minutes * 60 / interval_sec)

    # Intervals in nanoseconds
    update_interval_ns = int(interval_sec * 1_000_000_000)
    volatility_interval_ns = int(interval_sec * 6 * 1_000_000_000)  # 6x update interval

    # --- Scale order_qty and max_position ---
    # Backtest values
    bt_order_qty = fixed["order_qty"]
    bt_max_position = fixed["max_position"]
    bt_ratio = bt_max_position / bt_order_qty  # typically 20

    # Target: max_position_usdt = capital_per_coin
    # Then: order_qty = max_position / ratio
    max_position_usdt = capital_per_coin
    order_qty_usdt = max_position_usdt / bt_ratio

    # Convert to coin units
    order_qty_coins = order_qty_usdt / price
    max_position_coins = max_position_usdt / price

    # Round to lot_size
    order_qty_coins = round_to_lot(order_qty_coins, lot_size)
    max_position_coins = round_to_lot(max_position_coins, lot_size)

    # Check minimum notional
    order_notional = order_qty_coins * price
    warnings = []

    if order_notional < min_notional:
        # Bump order_qty to meet minimum notional
        min_order_coins = round_to_lot(
            math.ceil(min_notional / price / lot_size) * lot_size, lot_size
        )
        warnings.append(
            f"order_qty bumped from {order_qty_coins} to {min_order_coins} "
            f"(notional {order_qty_coins * price:.2f} < min {min_notional})"
        )
        order_qty_coins = min_order_coins
        # Also adjust max_position to maintain at least ratio of 2
        if max_position_coins < order_qty_coins * 2:
            max_position_coins = round_to_lot(order_qty_coins * 2, lot_size)
            warnings.append(f"max_position adjusted to {max_position_coins} (2x order_qty)")

    # Check min_qty
    if order_qty_coins < min_qty:
        order_qty_coins = min_qty
        warnings.append(f"order_qty bumped to min_qty={min_qty}")

    # Final notional check
    final_notional = order_qty_coins * price
    max_notional = max_position_coins * price

    # ROI bounds: use defaults for live trading (HashMapMarketDepth doesn't need ROI clipping).
    # The YAML roi_lb/roi_ub are for ROIVectorMarketDepth (backtest) array sizing only.
    # Using them in live causes OBI to produce 0 when price drifts outside the backtest range.
    roi_lb = 0.0
    roi_ub = 200000.0

    # Grid interval: with grid_num=1, not used. Set to tick_size * 100 as placeholder.
    tick_size = info.get("tick_size", symbol_config.get("tick_size", 0.0001))
    tick_decimals = max(0, -int(math.floor(math.log10(tick_size)))) if tick_size < 1 else 0
    grid_interval = round(tick_size * 100, max(0, tick_decimals - 2))

    # --- Print summary ---
    print(f"\n{'='*60}")
    print(f"  {symbol}")
    print(f"{'='*60}")
    print(f"  Price:          ${price}")
    print(f"  Tick size:      {tick_size}")
    print(f"  Lot size:       {lot_size}")
    print(f"  Min notional:   ${min_notional}")
    print(f"  Backtest ratio: {bt_ratio:.0f}x (max_pos/order_qty)")
    print(f"  ---")
    print(f"  order_qty:      {order_qty_coins} coins (${final_notional:.2f})")
    print(f"  max_position:   {max_position_coins} coins (${max_notional:.2f})")
    print(f"  grid_num:       {fixed['grid_num']}")
    print(f"  window:         {window} samples ({window_minutes} min)")
    for w in warnings:
        print(f"  WARNING: {w}")

    # --- Generate TOML ---
    toml = f"""# {symbol} - OBI MM Strategy Config
# Generated from fitted parameters
# Capital allocation: ${capital_per_coin:.0f} per coin
# Price at generation: ${price}
# Order notional: ${final_notional:.2f}, Max position: ${max_notional:.2f}

[global]
tick_size = {tick_size}
lot_size = {lot_size}
order_qty = {order_qty_coins}
max_position = {max_position_coins}
grid_num = {fixed['grid_num']}
grid_interval = {grid_interval}
roi_lb = {roi_lb}
roi_ub = {roi_ub}

[params]
looking_depth = {depth}
window = {window}
volatility_window = 60
half_spread = {half_spread}
skew = {skew}
c1 = {c1}
power = {power}
live_seconds = {live_seconds}
update_interval_ns = {update_interval_ns}
volatility_interval_ns = {volatility_interval_ns}

[stop_loss]
volatility_mean = {stop_loss_param['volatility_mean']}
volatility_std = {stop_loss_param['volatility_std']}
change_mean = {stop_loss_param['change_mean']}
change_std = {stop_loss_param['change_std']}
volatility_threshold = {stop_loss_threshold}
change_threshold = {stop_loss_threshold}
"""
    return toml, warnings


def main():
    parser = argparse.ArgumentParser(description="Convert YAML params to TOML strategy configs")
    parser.add_argument("--yaml", required=True, help="Path to YAML parameter file")
    parser.add_argument("--output", required=True, help="Output directory for TOML files")
    parser.add_argument("--capital", type=float, default=2000, help="Total capital in USDT (default: 2000)")
    parser.add_argument("--dry-run", action="store_true", help="Print configs without writing files")
    args = parser.parse_args()

    # Load YAML
    with open(args.yaml) as f:
        data = yaml.safe_load(f)

    symbols = list(data.keys())
    num_coins = len(symbols)
    capital_per_coin = args.capital / num_coins

    print(f"Total capital: ${args.capital}")
    print(f"Coins: {num_coins}")
    print(f"Capital per coin: ${capital_per_coin:.2f}")

    # Fetch current prices and exchange info
    print("\nFetching prices from Binance...")
    prices = fetch_prices(symbols)
    print("Fetching exchange info from Binance...")
    exchange_info = fetch_exchange_info(symbols)

    output_dir = Path(args.output)
    if not args.dry_run:
        output_dir.mkdir(parents=True, exist_ok=True)

    all_warnings = {}
    for symbol, params in data.items():
        if symbol not in prices:
            print(f"\nSKIPPING {symbol}: no price available")
            continue

        toml_content, warnings = generate_toml(
            symbol, params, prices[symbol], capital_per_coin, exchange_info
        )
        all_warnings[symbol] = warnings

        # Write file
        slug = symbol.lower().replace("usdt", "")
        filename = f"obi_mm_{slug}.toml"
        filepath = output_dir / filename

        if args.dry_run:
            print(f"\n--- {filepath} ---")
            print(toml_content)
        else:
            filepath.write_text(toml_content)
            print(f"  -> Written to {filepath}")

    # Summary
    print(f"\n{'='*60}")
    print("  SUMMARY")
    print(f"{'='*60}")
    total_order_notional = 0
    total_max_notional = 0
    for symbol in symbols:
        if symbol not in prices:
            continue
        info = exchange_info.get(symbol, {})
        params = data[symbol]
        fixed = params["symbol_config"]["fixed_params"]
        bt_ratio = fixed["max_position"] / fixed["order_qty"]
        price = prices[symbol]
        lot_size = info.get("lot_size", params["symbol_config"].get("lot_size", 1.0))

        max_pos_coins = round_to_lot(capital_per_coin / price, lot_size)
        ord_qty_coins = round_to_lot((capital_per_coin / bt_ratio) / price, lot_size)

        min_notional = info.get("min_notional", 5.0)
        if ord_qty_coins * price < min_notional:
            ord_qty_coins = round_to_lot(
                math.ceil(min_notional / price / lot_size) * lot_size, lot_size
            )
            if max_pos_coins < ord_qty_coins * 2:
                max_pos_coins = round_to_lot(ord_qty_coins * 2, lot_size)

        ord_usd = ord_qty_coins * price
        max_usd = max_pos_coins * price
        total_order_notional += ord_usd
        total_max_notional += max_usd

        warn_str = " !!!" if all_warnings.get(symbol) else ""
        print(f"  {symbol:15s}  order=${ord_usd:>8.2f}  max_pos=${max_usd:>8.2f}{warn_str}")

    print(f"  {'':15s}  ────────────  ────────────")
    print(f"  {'TOTAL':15s}  order=${total_order_notional:>8.2f}  max_pos=${total_max_notional:>8.2f}")
    print()

    if any(all_warnings.values()):
        print("WARNINGS:")
        for sym, warns in all_warnings.items():
            for w in warns:
                print(f"  {sym}: {w}")


if __name__ == "__main__":
    main()
