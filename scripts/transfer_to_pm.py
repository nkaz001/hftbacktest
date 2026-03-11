#!/usr/bin/env python3
"""
Transfer funds between Binance accounts (Spot ↔ Portfolio Margin).

Usage:
    # Show balances across all accounts
    python scripts/transfer_to_pm.py --show --env-file deploy/docker/.env.binance

    # Transfer 1000 USDT from Spot to PM (UM futures wallet)
    python scripts/transfer_to_pm.py --transfer --amount 1000 --env-file deploy/docker/.env.binance

    # Transfer from PM back to Spot
    python scripts/transfer_to_pm.py --transfer --amount 500 --direction pm-to-spot --env-file deploy/docker/.env.binance

    # Transfer a different asset
    python scripts/transfer_to_pm.py --transfer --amount 0.1 --asset BTC --env-file deploy/docker/.env.binance
"""

import argparse
import asyncio
import os
import sys
from pathlib import Path

try:
    import ccxt.pro as ccxt_pro
except ImportError:
    print("Error: ccxt not installed. Run: pip install ccxt")
    sys.exit(1)


def load_env_file(env_file: str) -> dict:
    env_vars = {}
    path = Path(env_file)
    if not path.exists():
        print(f"Warning: env file not found: {env_file}")
        return env_vars
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, _, value = line.partition("=")
                env_vars[key.strip()] = value.strip().strip("'\"")
    return env_vars


async def show_balances(exchange):
    """Show balances across Spot, UM Futures (PM), and CM Futures (PM)."""
    print("Fetching balances...\n")

    # Spot balance
    try:
        spot = await exchange.fetch_balance({"type": "spot"})
        print("=== Spot Account ===")
        for asset, bal in spot["total"].items():
            if bal and float(bal) > 0:
                free = float(spot["free"].get(asset, 0))
                used = float(spot["used"].get(asset, 0))
                print(f"  {asset:<8} total={float(bal):>12.4f}  free={free:>12.4f}  used={used:>12.4f}")
    except Exception as e:
        print(f"  Error fetching spot balance: {e}")

    # PM balance
    try:
        pm = await exchange.papi_get_balance()
        print("\n=== Portfolio Margin Account ===")
        for item in pm:
            total = float(item.get("totalWalletBalance", 0))
            if total > 0.0001:
                asset = item.get("asset", "?")
                avail = float(item.get("availableBalance", 0))
                pnl = float(item.get("unrealizedProfit", 0))
                print(f"  {asset:<8} total={total:>12.4f}  available={avail:>12.4f}  unrealizedPnL={pnl:>10.4f}")
    except Exception as e:
        print(f"  Error fetching PM balance: {e}")

    # PM account info (margin ratio)
    try:
        info = await exchange.papi_get_account()
        uni_mmr = float(info.get("uniMMR", 0))
        equity = float(info.get("accountEquity", 0))
        maint = float(info.get("accountMaintMargin", 0))
        status = info.get("accountStatus", "?")
        print(f"\n=== PM Summary ===")
        print(f"  Equity:       ${equity:,.2f}")
        print(f"  Maint Margin: ${maint:,.2f}")
        print(f"  uniMMR:       {uni_mmr:.4f}")
        print(f"  Status:       {status}")
    except Exception as e:
        print(f"  Error fetching PM account info: {e}")


async def transfer(exchange, asset: str, amount: float, direction: str):
    """Transfer between Spot and PM."""
    # Binance universal transfer types for Portfolio Margin:
    #   MAIN_PORTFOLIO_MARGIN  = Spot → PM
    #   PORTFOLIO_MARGIN_MAIN  = PM → Spot
    if direction == "spot-to-pm":
        transfer_type = "MAIN_PORTFOLIO_MARGIN"
        desc = f"Spot → PM"
    elif direction == "pm-to-spot":
        transfer_type = "PORTFOLIO_MARGIN_MAIN"
        desc = f"PM → Spot"
    else:
        print(f"Unknown direction: {direction}")
        return

    print(f"Transferring {amount} {asset} ({desc})...")

    try:
        result = await exchange.sapi_post_asset_transfer({
            "type": transfer_type,
            "asset": asset,
            "amount": str(amount),
        })
        tran_id = result.get("tranId", "?")
        print(f"Success! Transaction ID: {tran_id}")
    except Exception as e:
        print(f"Transfer failed: {e}")


async def main():
    parser = argparse.ArgumentParser(description="Binance Spot ↔ PM fund transfer")
    parser.add_argument("--show", action="store_true", help="Show balances")
    parser.add_argument("--transfer", action="store_true", help="Execute transfer")
    parser.add_argument("--amount", type=float, help="Amount to transfer")
    parser.add_argument("--asset", type=str, default="USDT", help="Asset to transfer (default: USDT)")
    parser.add_argument("--direction", type=str, default="spot-to-pm",
                        choices=["spot-to-pm", "pm-to-spot"],
                        help="Transfer direction (default: spot-to-pm)")
    parser.add_argument("--env-file", type=str, help="Path to .env file")
    args = parser.parse_args()

    if not args.show and not args.transfer:
        parser.print_help()
        sys.exit(1)

    if args.transfer and not args.amount:
        print("Error: --amount is required for transfer")
        sys.exit(1)

    # Load credentials
    if args.env_file:
        env_vars = load_env_file(args.env_file)
        for k, v in env_vars.items():
            os.environ.setdefault(k, v)

    api_key = os.environ.get("BINANCE_API_KEY")
    api_secret = os.environ.get("BINANCE_SECRET")
    if not api_key or not api_secret:
        print("Error: Set BINANCE_API_KEY/BINANCE_SECRET or use --env-file")
        sys.exit(1)

    exchange = ccxt_pro.binance({
        "apiKey": api_key,
        "secret": api_secret,
        "options": {
            "defaultType": "future",
            "portfolioMargin": True,
        },
    })

    try:
        if args.show:
            await show_balances(exchange)

        if args.transfer:
            await transfer(exchange, args.asset, args.amount, args.direction)
            print()
            await show_balances(exchange)

    finally:
        await exchange.close()


if __name__ == "__main__":
    asyncio.run(main())
