#!/usr/bin/env python3
"""
Close all PAPI UM positions and cancel open orders on Binance.

Usage:
  # Read from env vars
  export BINANCE_API_KEY=xxx BINANCE_SECRET=xxx
  python close_positions.py

  # Read from .env file
  python close_positions.py --env-file .env.binance

  # Dry run (check only, no actions)
  python close_positions.py --dry-run

  # Skip confirmation prompt
  python close_positions.py --yes
"""

import argparse
import hashlib
import hmac
import json
import sys
import time
import os
from urllib.parse import urlencode

try:
    import requests
except ImportError:
    print("ERROR: requests not installed. Run: pip install requests")
    sys.exit(1)

BASE_URL = "https://papi.binance.com"


def load_env_file(path):
    """Load KEY=VALUE pairs from a .env file into environment."""
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, _, value = line.partition("=")
                os.environ[key.strip()] = value.strip()


def sign(params: dict, secret: str) -> dict:
    """Add timestamp and HMAC-SHA256 signature to params."""
    params["timestamp"] = int(time.time() * 1000)
    query = urlencode(params)
    sig = hmac.new(secret.encode(), query.encode(), hashlib.sha256).hexdigest()
    params["signature"] = sig
    return params


def api_get(path, params, key, secret):
    params = sign(params, secret)
    r = requests.get(f"{BASE_URL}{path}", params=params,
                     headers={"X-MBX-APIKEY": key}, timeout=10)
    r.raise_for_status()
    return r.json()


def api_post(path, params, key, secret):
    params = sign(params, secret)
    r = requests.post(f"{BASE_URL}{path}", params=params,
                      headers={"X-MBX-APIKEY": key}, timeout=10)
    r.raise_for_status()
    return r.json()


def api_delete(path, params, key, secret):
    params = sign(params, secret)
    r = requests.delete(f"{BASE_URL}{path}", params=params,
                        headers={"X-MBX-APIKEY": key}, timeout=10)
    r.raise_for_status()
    return r.json()


def get_positions(key, secret):
    """Get all UM positions with non-zero quantity."""
    data = api_get("/papi/v1/um/positionRisk", {}, key, secret)
    return [p for p in data if abs(float(p.get("positionAmt", 0))) > 1e-10]


def get_open_orders(key, secret):
    """Get all UM open orders."""
    return api_get("/papi/v1/um/openOrders", {}, key, secret)


def cancel_all_orders(symbol, key, secret):
    """Cancel all UM open orders for a symbol."""
    return api_delete("/papi/v1/um/allOpenOrders", {"symbol": symbol}, key, secret)


def close_position(symbol, side, qty, key, secret):
    """Close a position with a market order."""
    params = {
        "symbol": symbol,
        "side": side,
        "type": "MARKET",
        "quantity": qty,
        "reduceOnly": "true",
    }
    return api_post("/papi/v1/um/order", params, key, secret)


def main():
    parser = argparse.ArgumentParser(description="Close all Binance PAPI UM positions")
    parser.add_argument("--env-file", help="Path to .env file with BINANCE_API_KEY and BINANCE_SECRET")
    parser.add_argument("--dry-run", action="store_true", help="Check only, no actions")
    parser.add_argument("--yes", "-y", action="store_true", help="Skip confirmation prompt")
    args = parser.parse_args()

    if args.env_file:
        load_env_file(args.env_file)

    key = os.environ.get("BINANCE_API_KEY")
    secret = os.environ.get("BINANCE_SECRET")
    if not key or not secret:
        print("ERROR: BINANCE_API_KEY and BINANCE_SECRET must be set")
        print("  Use --env-file or export them as environment variables")
        sys.exit(1)

    print(f"API Key: {key[:8]}...{key[-4:]}")
    print()

    # ── 1. Check open orders ──
    print("=" * 60)
    print("  OPEN ORDERS")
    print("=" * 60)
    orders = get_open_orders(key, secret)
    if orders:
        symbols_with_orders = set()
        for o in orders:
            sym = o["symbol"]
            symbols_with_orders.add(sym)
            print(f"  {sym:15s}  {o['side']:4s}  {o['type']:6s}  "
                  f"price={o['price']}  qty={o['origQty']}  id={o['orderId']}")
        print(f"\nTotal: {len(orders)} open orders across {len(symbols_with_orders)} symbols")
    else:
        symbols_with_orders = set()
        print("  (none)")
    print()

    # ── 2. Check positions ──
    print("=" * 60)
    print("  POSITIONS")
    print("=" * 60)
    positions = get_positions(key, secret)
    if positions:
        total_notional = 0.0
        for p in positions:
            amt = float(p["positionAmt"])
            entry = float(p.get("entryPrice", 0))
            mark = float(p.get("markPrice", 0))
            notional = abs(amt * mark)
            upnl = float(p.get("unRealizedProfit", 0))
            side = "LONG" if amt > 0 else "SHORT"
            total_notional += notional
            print(f"  {p['symbol']:15s}  {side:5s}  amt={amt:>12.4f}  "
                  f"entry={entry:>10.4f}  mark={mark:>10.4f}  "
                  f"notional=${notional:>8.2f}  upnl=${upnl:>+8.4f}")
        print(f"\nTotal: {len(positions)} positions, ${total_notional:.2f} notional")
    else:
        print("  (none)")
    print()

    # ── 3. Summary ──
    if not orders and not positions:
        print("Nothing to do — all clean!")
        return

    if args.dry_run:
        print("[DRY RUN] No actions taken.")
        return

    # ── 4. Confirm ──
    if not args.yes:
        print("=" * 60)
        actions = []
        if orders:
            actions.append(f"Cancel {len(orders)} open orders")
        if positions:
            actions.append(f"Close {len(positions)} positions (market order)")
        print("  ACTIONS:")
        for a in actions:
            print(f"    - {a}")
        print("=" * 60)
        confirm = input("\nProceed? [y/N] ").strip().lower()
        if confirm != "y":
            print("Aborted.")
            return

    # ── 5. Cancel all open orders ──
    if orders:
        print("\nCancelling open orders...")
        for sym in sorted(symbols_with_orders):
            try:
                result = cancel_all_orders(sym, key, secret)
                n = len(result) if isinstance(result, list) else 1
                print(f"  {sym}: cancelled {n} orders")
            except Exception as e:
                print(f"  {sym}: ERROR - {e}")

    # ── 6. Close positions ──
    if positions:
        print("\nClosing positions...")
        for p in positions:
            sym = p["symbol"]
            amt = float(p["positionAmt"])
            # To close: sell if long, buy if short
            side = "SELL" if amt > 0 else "BUY"
            qty = str(abs(amt))
            try:
                result = close_position(sym, side, qty, key, secret)
                print(f"  {sym}: {side} {qty} @ MARKET -> {result.get('status', 'OK')}")
            except requests.exceptions.HTTPError as e:
                print(f"  {sym}: ERROR - {e.response.text}")
            except Exception as e:
                print(f"  {sym}: ERROR - {e}")

    # ── 7. Verify ──
    print("\nVerifying...")
    time.sleep(1)
    remaining_orders = get_open_orders(key, secret)
    remaining_positions = get_positions(key, secret)
    print(f"  Open orders: {len(remaining_orders)}")
    print(f"  Open positions: {len(remaining_positions)}")
    if not remaining_orders and not remaining_positions:
        print("\nAll clean!")
    else:
        print("\nWARNING: Some orders/positions remain. Check manually.")
        for p in remaining_positions:
            print(f"  REMAINING: {p['symbol']} amt={p['positionAmt']}")


if __name__ == "__main__":
    main()
