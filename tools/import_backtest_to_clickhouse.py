#!/usr/bin/env python3
"""Import backtest CSV equity curve into ClickHouse table hft.backtest_equity."""

from __future__ import annotations

import argparse
import csv
import math
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable, List, Sequence, Tuple


INSERT_COLUMNS: Sequence[str] = (
    "timestamp",
    "strategy_id",
    "symbol",
    "backtest_id",
    "equity",
    "pnl",
    "position",
    "mid_price",
    "balance",
    "fee",
    "num_trades",
    "trading_volume",
)


@dataclass
class ImportStats:
    total_rows: int
    skipped_nan_price: int
    imported_rows: int
    first_timestamp: datetime
    last_timestamp: datetime
    final_pnl: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Import backtest CSV into ClickHouse hft.backtest_equity",
    )
    parser.add_argument("--file", required=True, help="Path to backtest CSV file")
    parser.add_argument("--strategy-id", required=True, help="Strategy identifier")
    parser.add_argument("--symbol", required=True, help="Symbol, e.g. ADAUSDT")
    parser.add_argument("--backtest-id", required=True, help="Backtest run identifier")
    parser.add_argument(
        "--clickhouse-host",
        default="localhost",
        help="ClickHouse host (default: localhost)",
    )
    parser.add_argument(
        "--clickhouse-port",
        type=int,
        default=8123,
        help="ClickHouse HTTP port (default: 8123)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse and summarize rows without inserting into ClickHouse",
    )
    return parser.parse_args()


def parse_float(value: str, field: str) -> float:
    try:
        return float(value)
    except (TypeError, ValueError) as exc:
        raise ValueError(f"Invalid float in column '{field}': {value!r}") from exc


def parse_int(value: str, field: str) -> int:
    try:
        return int(float(value))
    except (TypeError, ValueError) as exc:
        raise ValueError(f"Invalid int in column '{field}': {value!r}") from exc


def ns_to_utc_datetime(timestamp_ns: int) -> datetime:
    seconds, nanoseconds = divmod(timestamp_ns, 1_000_000_000)
    return datetime.fromtimestamp(seconds, tz=timezone.utc).replace(
        microsecond=nanoseconds // 1000
    )


def load_rows(
    csv_path: Path,
    strategy_id: str,
    symbol: str,
    backtest_id: str,
) -> Tuple[List[Tuple[object, ...]], ImportStats]:
    rows: List[Tuple[object, ...]] = []
    total_rows = 0
    skipped_nan_price = 0
    first_timestamp: datetime | None = None
    last_timestamp: datetime | None = None
    initial_equity: float | None = None
    final_pnl = 0.0

    with csv_path.open("r", newline="") as f:
        reader = csv.DictReader(f)

        required_columns = {
            "timestamp",
            "balance",
            "position",
            "fee",
            "num_trades",
            "trading_volume",
            "price",
        }
        missing = required_columns.difference(reader.fieldnames or [])
        if missing:
            missing_list = ", ".join(sorted(missing))
            raise ValueError(f"Missing required CSV columns: {missing_list}")

        for record in reader:
            total_rows += 1

            price = parse_float(record.get("price", ""), "price")
            if math.isnan(price):
                skipped_nan_price += 1
                continue

            timestamp_ns = parse_int(record.get("timestamp", ""), "timestamp")
            timestamp = ns_to_utc_datetime(timestamp_ns)
            balance = parse_float(record.get("balance", ""), "balance")
            position = parse_float(record.get("position", ""), "position")
            fee = parse_float(record.get("fee", ""), "fee")
            num_trades = parse_int(record.get("num_trades", "0"), "num_trades")
            trading_volume = parse_float(
                record.get("trading_volume", "0"), "trading_volume"
            )

            equity = balance + fee + position * price
            if initial_equity is None:
                initial_equity = equity
            pnl = equity - initial_equity
            final_pnl = pnl

            if first_timestamp is None:
                first_timestamp = timestamp
            last_timestamp = timestamp

            rows.append(
                (
                    timestamp,
                    strategy_id,
                    symbol,
                    backtest_id,
                    equity,
                    pnl,
                    position,
                    price,
                    balance,
                    fee,
                    num_trades,
                    trading_volume,
                )
            )

    if not rows:
        raise ValueError("No valid rows to import after filtering NaN prices")

    assert first_timestamp is not None
    assert last_timestamp is not None

    stats = ImportStats(
        total_rows=total_rows,
        skipped_nan_price=skipped_nan_price,
        imported_rows=len(rows),
        first_timestamp=first_timestamp,
        last_timestamp=last_timestamp,
        final_pnl=final_pnl,
    )
    return rows, stats


def insert_rows(
    rows: Iterable[Tuple[object, ...]],
    host: str,
    port: int,
) -> None:
    try:
        import clickhouse_connect
    except ImportError as exc:
        raise RuntimeError(
            "clickhouse-connect is required for insertion. Install with: pip install clickhouse-connect"
        ) from exc

    client = clickhouse_connect.get_client(host=host, port=port, database="hft")
    client.insert(
        table="hft.backtest_equity",
        data=list(rows),
        column_names=list(INSERT_COLUMNS),
    )


def print_summary(stats: ImportStats, dry_run: bool) -> None:
    mode = "would be imported" if dry_run else "imported"
    print("Backtest import summary")
    print(f"- CSV rows read: {stats.total_rows}")
    print(f"- Rows skipped (NaN price): {stats.skipped_nan_price}")
    print(f"- Rows {mode}: {stats.imported_rows}")
    print(
        "- Time range (UTC): "
        f"{stats.first_timestamp.isoformat()} -> {stats.last_timestamp.isoformat()}"
    )
    print(f"- Final PnL: {stats.final_pnl:.8f}")


def main() -> int:
    args = parse_args()
    csv_path = Path(args.file)

    if not csv_path.exists():
        print(f"Error: file not found: {csv_path}", file=sys.stderr)
        return 1

    try:
        rows, stats = load_rows(
            csv_path=csv_path,
            strategy_id=args.strategy_id,
            symbol=args.symbol,
            backtest_id=args.backtest_id,
        )
    except Exception as exc:  # noqa: BLE001 - CLI tool should report clear failures.
        print(f"Error parsing CSV: {exc}", file=sys.stderr)
        return 1

    print_summary(stats, dry_run=args.dry_run)

    if args.dry_run:
        print("- Dry run: no rows inserted")
        return 0

    try:
        insert_rows(rows, host=args.clickhouse_host, port=args.clickhouse_port)
    except Exception as exc:  # noqa: BLE001 - CLI tool should report clear failures.
        print(f"Error inserting rows: {exc}", file=sys.stderr)
        return 1

    print("Insert completed successfully")
    return 0


if __name__ == "__main__":
    sys.exit(main())
