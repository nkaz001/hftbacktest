"""Download and convert CryptoHFTData order book and trade data."""

from __future__ import annotations

import argparse
from pathlib import Path

from hftbacktest.data.utils.cryptohftdata import download_and_convert


def parse_args() -> argparse.Namespace:
    """Parse command-line options for the CryptoHFTData conversion example."""
    parser = argparse.ArgumentParser(
        description="Download CryptoHFTData and convert it to HftBacktest's NPZ format.",
    )
    parser.add_argument("symbol", help="Trading pair, for example BTCUSDT.")
    parser.add_argument("exchange", help="CryptoHFTData exchange identifier, for example binance_futures.")
    parser.add_argument("start_date", help="Inclusive start date in YYYY-MM-DD format.")
    parser.add_argument("end_date", help="Inclusive end date in YYYY-MM-DD format.")
    parser.add_argument("output", type=Path, help="Destination .npz file.")
    parser.add_argument(
        "--api-key",
        help="Optional CryptoHFTData API key. The SDK also reads CRYPTOHFTDATA_API_KEY.",
    )
    parser.add_argument("--max-workers", type=int, default=10, help="Maximum concurrent hourly downloads.")
    parser.add_argument("--base-latency", type=float, default=0, help="Additional feed latency in nanoseconds.")
    return parser.parse_args()


def main() -> None:
    """Download, convert, and save the requested CryptoHFTData range."""
    args = parse_args()
    if args.output.suffix != ".npz":
        raise ValueError("output must use the .npz extension")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    data = download_and_convert(
        symbol=args.symbol,
        exchange=args.exchange,
        start_date=args.start_date,
        end_date=args.end_date,
        api_key=args.api_key,
        max_workers=args.max_workers,
        output_filename=str(args.output),
        base_latency=args.base_latency,
    )
    print(f"Converted {len(data):,} events to {args.output}")


if __name__ == "__main__":
    main()
