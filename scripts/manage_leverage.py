#!/usr/bin/env python3
"""
Manage leverage for Binance PAPI (Portfolio Margin) symbols.

Queries leverage brackets, checks current leverage, and adjusts to support
a target notional position size (default $5,000 USD).

Usage:
    # Show current leverage for all configured symbols
    python scripts/manage_leverage.py --show

    # Set leverage for all configured symbols to support $5000 notional
    python scripts/manage_leverage.py --set

    # Set leverage for specific symbols
    python scripts/manage_leverage.py --set --symbols ALPINEUSDT SYSUSDT

    # Custom target notional
    python scripts/manage_leverage.py --set --target-notional 10000

    # Use env file for credentials
    python scripts/manage_leverage.py --show --env-file deploy/docker/.env.binance
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
    """Load environment variables from a .env file."""
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
                key = key.strip()
                value = value.strip().strip("'\"")
                env_vars[key] = value
    return env_vars


def get_symbols_from_cluster(cluster_path: str) -> list[str]:
    """Extract symbol list from cluster.toml."""
    path = Path(cluster_path)
    if not path.exists():
        return []

    symbols = []
    with open(path) as f:
        for line in f:
            if line.strip().startswith("symbols"):
                # Parse: symbols = ["alpineusdt", "fusdt", ...]
                start = line.index("[")
                end = line.index("]") + 1
                raw = line[start:end]
                # Simple parsing - extract quoted strings
                import re
                symbols = [m.upper() for m in re.findall(r'"([^"]+)"', raw)]
                break
    return symbols


async def main():
    parser = argparse.ArgumentParser(description="Manage Binance PAPI leverage")
    parser.add_argument("--show", action="store_true", help="Show current leverage for all symbols")
    parser.add_argument("--set", action="store_true", help="Set leverage to support target notional")
    parser.add_argument("--target-notional", type=float, default=5000,
                        help="Target notional position size in USD (default: 5000)")
    parser.add_argument("--symbols", nargs="+", help="Specific symbols (e.g., ALPINEUSDT SYSUSDT)")
    parser.add_argument("--cluster-config", type=str,
                        help="Path to cluster.toml to read symbols from")
    parser.add_argument("--env-file", type=str, help="Path to .env file with API credentials")
    parser.add_argument("--api-key", type=str, help="Binance API key (or set BINANCE_API_KEY env)")
    parser.add_argument("--api-secret", type=str, help="Binance API secret (or set BINANCE_SECRET env)")
    args = parser.parse_args()

    if not args.show and not args.set:
        parser.print_help()
        sys.exit(1)

    # Load credentials
    if args.env_file:
        env_vars = load_env_file(args.env_file)
        for k, v in env_vars.items():
            os.environ.setdefault(k, v)

    api_key = args.api_key or os.environ.get("BINANCE_API_KEY")
    api_secret = args.api_secret or os.environ.get("BINANCE_SECRET")

    if not api_key or not api_secret:
        print("Error: API credentials required. Use --env-file, --api-key/--api-secret, or set BINANCE_API_KEY/BINANCE_SECRET env vars.")
        sys.exit(1)

    # Determine symbols
    symbols = None
    if args.symbols:
        symbols = [s.upper() for s in args.symbols]
    elif args.cluster_config:
        symbols = get_symbols_from_cluster(args.cluster_config)
        if not symbols:
            print(f"Warning: No symbols found in {args.cluster_config}")

    # Create exchange instance (Binance with PAPI)
    exchange = ccxt_pro.binance({
        "apiKey": api_key,
        "secret": api_secret,
        "options": {
            "defaultType": "future",
            "portfolioMargin": True,
        },
    })

    try:
        # Load markets
        print("Loading markets...")
        await exchange.load_markets()

        # Get leverage brackets via PAPI
        print("Fetching leverage brackets...")
        brackets_response = await exchange.papi_get_um_leveragebracket()

        # Process brackets into a lookup: {symbol_id: brackets}
        brackets_by_symbol = {}
        for item in brackets_response:
            symbol_id = item.get("symbol")
            if symbol_id:
                brackets_by_symbol[symbol_id] = item.get("brackets", [])

        # Get current symbol configs via PAPI
        print("Fetching current symbol configs...")
        configs_response = await exchange.papi_get_um_symbolconfig()

        # Process configs: {symbol_id: {leverage, ...}}
        configs_by_symbol = {}
        for item in configs_response:
            symbol_id = item.get("symbol")
            if symbol_id:
                configs_by_symbol[symbol_id] = item

        # Filter symbols if specified
        if symbols:
            target_symbol_ids = [s.upper().replace("/", "") for s in symbols]
        else:
            # Use all symbols that have brackets
            target_symbol_ids = sorted(brackets_by_symbol.keys())

        if args.show:
            print(f"\n{'Symbol':<20} {'Current Lev':>12} {'Max Lev ($5k)':>14} {'Max Notional':>14}")
            print("-" * 62)

            for symbol_id in target_symbol_ids:
                brackets = brackets_by_symbol.get(symbol_id, [])
                config = configs_by_symbol.get(symbol_id, {})
                current_lev = config.get("leverage", "?")

                # Find max leverage for target notional
                target_lev = "?"
                max_notional = 0
                for bracket in brackets:
                    cap = float(bracket.get("notionalCap", 0))
                    if cap > max_notional:
                        max_notional = cap
                    floor = float(bracket.get("notionalFloor", 0))
                    bracket_cap = float(bracket.get("notionalCap", float("inf")))
                    if floor <= args.target_notional < bracket_cap:
                        target_lev = int(bracket.get("initialLeverage", 1))

                print(f"{symbol_id:<20} {str(current_lev):>12} {str(target_lev):>14} {max_notional:>14,.0f}")

        if args.set:
            print(f"\nSetting leverage for target notional: ${args.target_notional:,.0f} USD")
            print(f"{'Symbol':<20} {'Current':>8} {'Target':>8} {'Action':>10}")
            print("-" * 50)

            for symbol_id in target_symbol_ids:
                brackets = brackets_by_symbol.get(symbol_id, [])
                config = configs_by_symbol.get(symbol_id, {})
                current_lev = int(config.get("leverage", 1)) if config.get("leverage") else 1

                # Find required leverage for target notional
                required_lev = None
                for bracket in brackets:
                    floor = float(bracket.get("notionalFloor", 0))
                    cap = float(bracket.get("notionalCap", float("inf")))
                    if floor <= args.target_notional < cap:
                        required_lev = int(bracket.get("initialLeverage", 1))
                        break

                if required_lev is None:
                    print(f"{symbol_id:<20} {current_lev:>8} {'N/A':>8} {'skip':>10}  (no bracket found)")
                    continue

                if current_lev == required_lev:
                    print(f"{symbol_id:<20} {current_lev:>8} {required_lev:>8} {'ok':>10}")
                    continue

                # Set leverage
                try:
                    await exchange.papi_post_um_leverage({
                        "symbol": symbol_id,
                        "leverage": required_lev,
                    })
                    action = f"{current_lev}->{required_lev}"
                    print(f"{symbol_id:<20} {current_lev:>8} {required_lev:>8} {action:>10}")
                except Exception as e:
                    print(f"{symbol_id:<20} {current_lev:>8} {required_lev:>8} {'ERROR':>10}  ({e})")

            print("\nDone.")

    finally:
        await exchange.close()


if __name__ == "__main__":
    asyncio.run(main())
