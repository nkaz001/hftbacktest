"""Download and convert CryptoHFTData archives for HftBacktest replay."""

from __future__ import annotations

import os
from datetime import datetime
from typing import TYPE_CHECKING, Any

import numpy as np
import polars as pl
from numpy.typing import NDArray

from ..validation import correct_event_order, correct_local_timestamp, validate_event_order
from ...types import (
    BUY_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    SELL_EVENT,
    TRADE_EVENT,
    event_dtype,
)

if TYPE_CHECKING:
    import pandas as pd

try:
    import pandas as _pd
except ImportError:  # pragma: no cover - pandas is optional at import time.
    _pd = None

try:
    from cryptohftdata import CryptoHFTDataClient as _CryptoHFTDataClient
except ImportError:  # pragma: no cover - cryptohftdata is optional at import time.
    _CryptoHFTDataClient = None

ORDERBOOK_REQUIRED_COLUMNS = frozenset(
    {
        "received_time",
        "event_time",
        "event_type",
        "side",
        "price",
        "quantity",
    }
)
TRADES_REQUIRED_COLUMNS = frozenset(
    {
        "received_time",
        "event_time",
        "price",
        "quantity",
        "is_buyer_maker",
    }
)
ORDERBOOK_MESSAGE_ID_COLUMNS = (
    "symbol",
    "first_update_id",
    "final_update_id",
    "prev_final_update_id",
    "last_update_id",
)
ZERO_U64 = pl.lit(0, dtype=pl.UInt64)
ZERO_I64 = pl.lit(0, dtype=pl.Int64)
ZERO_F64 = pl.lit(0.0, dtype=pl.Float64)


def download(
        symbol: str,
        exchange: str,
        start_date: str | datetime,
        end_date: str | datetime,
        api_key: str | None = None,
        client: Any = None,
        max_workers: int = 10,
) -> tuple["pd.DataFrame", "pd.DataFrame"]:
    r"""
    Downloads orderbook and trades data from CryptoHFTData using the official SDK.

    The CryptoHFTData SDK returns pandas DataFrames and downloads the vendor's hourly parquet chunks internally.
    ``api_key`` is optional. If omitted, ``CRYPTOHFTDATA_API_KEY`` is used when set; otherwise the SDK uses the
    rate-limited free tier.

    Args:
        symbol: Trading pair symbol, e.g. ``BTCUSDT``.
        exchange: CryptoHFTData exchange identifier, e.g. ``binance_futures``.
        start_date: Inclusive start date/datetime understood by the SDK.
        end_date: Inclusive end date/datetime understood by the SDK.
        api_key: Optional CryptoHFTData API key. Keyless downloads use the rate-limited free tier.
        client: Optional injected SDK client for testing or custom configuration.
        max_workers: Maximum number of concurrent hourly file downloads used by the SDK.

    Returns:
        A tuple ``(orderbook_df, trades_df)``.
    """
    chd_client = _get_client(api_key, client)

    orderbook = chd_client.get_orderbook(
        symbol=symbol,
        exchange=exchange,
        start_date=start_date,
        end_date=end_date,
        max_workers=max_workers,
    )
    trades = chd_client.get_trades(
        symbol=symbol,
        exchange=exchange,
        start_date=start_date,
        end_date=end_date,
        max_workers=max_workers,
    )
    return orderbook, trades


def convert(
        orderbook: pl.DataFrame | "pd.DataFrame",
        trades: pl.DataFrame | "pd.DataFrame",
        output_filename: str | None = None,
        base_latency: float = 0,
) -> NDArray:
    r"""
    Converts CryptoHFTData orderbook and trades data into a format compatible with HftBacktest.

    Timestamp mapping is fixed as follows:

    * orderbook ``local_ts = received_time``
    * orderbook ``exch_ts = coalesce(transaction_time, event_time) * 1_000_000``
    * trades ``local_ts = received_time``
    * trades ``exch_ts = coalesce(trade_time, event_time) * 1_000_000``

    Trade direction follows the existing HftBacktest convention: ``is_buyer_maker=True`` becomes
    ``SELL_EVENT | TRADE_EVENT``.

    Args:
        orderbook: CryptoHFTData orderbook data as pandas or Polars DataFrame.
        trades: CryptoHFTData trades data as pandas or Polars DataFrame.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`..validation.correct_local_timestamp`.

    Returns:
        Converted data compatible with HftBacktest.
    """
    orderbook_df = _normalize_orderbook(_to_polars(orderbook, "orderbook"))
    trades_df = _normalize_trades(_to_polars(trades, "trades"))

    trade_arr = _convert_trades(trades_df)
    depth_update_arr = _convert_orderbook_updates(orderbook_df)
    snapshot_arr = _convert_orderbook_snapshots(orderbook_df)

    data = np.empty(len(trade_arr) + len(depth_update_arr) + len(snapshot_arr), event_dtype)

    row_num = 0
    if len(trade_arr) > 0:
        data[row_num:row_num + len(trade_arr)] = trade_arr
        row_num += len(trade_arr)
    if len(depth_update_arr) > 0:
        data[row_num:row_num + len(depth_update_arr)] = depth_update_arr
        row_num += len(depth_update_arr)
    if len(snapshot_arr) > 0:
        data[row_num:row_num + len(snapshot_arr)] = snapshot_arr
        row_num += len(snapshot_arr)

    data = data[:row_num]

    if len(data) == 0:
        if output_filename is not None:
            print("Saving to %s" % output_filename)
            np.savez_compressed(output_filename, data=data)
        return data

    print("Correcting the latency")
    data = correct_local_timestamp(data, base_latency)

    print("Correcting the event order")
    data = correct_event_order(
        data,
        np.argsort(data["exch_ts"], kind="mergesort"),
        np.argsort(data["local_ts"], kind="mergesort"),
    )

    validate_event_order(data)

    if output_filename is not None:
        print("Saving to %s" % output_filename)
        np.savez_compressed(output_filename, data=data)

    return data


def download_and_convert(
        symbol: str,
        exchange: str,
        start_date: str | datetime,
        end_date: str | datetime,
        api_key: str | None = None,
        client: Any = None,
        max_workers: int = 10,
        output_filename: str | None = None,
        base_latency: float = 0,
) -> NDArray:
    r"""
    Downloads CryptoHFTData orderbook and trades data and converts them for HftBacktest.

    Args:
        symbol: Trading pair symbol, e.g. ``BTCUSDT``.
        exchange: CryptoHFTData exchange identifier, e.g. ``binance_futures``.
        start_date: Inclusive start date/datetime understood by the SDK.
        end_date: Inclusive end date/datetime understood by the SDK.
        api_key: Optional CryptoHFTData API key. Keyless downloads use the rate-limited free tier.
        client: Optional injected SDK client for testing or custom configuration.
        max_workers: Maximum number of concurrent hourly file downloads used by the SDK.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`..validation.correct_local_timestamp`.

    Returns:
        Converted data compatible with HftBacktest.
    """
    orderbook, trades = download(
        symbol=symbol,
        exchange=exchange,
        start_date=start_date,
        end_date=end_date,
        api_key=api_key,
        client=client,
        max_workers=max_workers,
    )
    return convert(
        orderbook=orderbook,
        trades=trades,
        output_filename=output_filename,
        base_latency=base_latency,
    )


def _get_client(api_key: str | None, client: Any) -> Any:
    if client is not None:
        return client

    if _CryptoHFTDataClient is None:
        raise ImportError(
            "cryptohftdata is required for download support. "
            "Install it with `pip install \"hftbacktest[cryptohftdata]\"`."
        )

    resolved_api_key = api_key or os.environ.get("CRYPTOHFTDATA_API_KEY")
    return _CryptoHFTDataClient(api_key=resolved_api_key)


def _to_polars(data: pl.DataFrame | "pd.DataFrame", name: str) -> pl.DataFrame:
    if isinstance(data, pl.DataFrame):
        return data.clone()
    if _pd is not None and isinstance(data, _pd.DataFrame):
        return pl.from_pandas(data, include_index=False)
    raise TypeError(f"{name} must be a pandas or Polars DataFrame.")


def _normalize_orderbook(df: pl.DataFrame) -> pl.DataFrame:
    _validate_columns(df, ORDERBOOK_REQUIRED_COLUMNS, "orderbook")

    if "transaction_time" in df.columns:
        transaction_expr = pl.col("transaction_time").cast(pl.Int64, strict=False)
    else:
        transaction_expr = pl.lit(None, dtype=pl.Int64)

    select_exprs: list[pl.Expr] = [
        pl.col("received_time").cast(pl.Int64, strict=False).alias("local_ts"),
        (pl.coalesce([transaction_expr, pl.col("event_time").cast(pl.Int64, strict=False)]) * 1_000_000)
            .cast(pl.Int64, strict=False)
            .alias("exch_ts"),
        pl.col("event_type").cast(pl.String, strict=False).str.to_lowercase().alias("event_type"),
        pl.when(pl.col("side").cast(pl.String, strict=False).str.to_lowercase().is_in(["bid", "buy"]))
            .then(pl.lit(BUY_EVENT, dtype=pl.UInt64))
            .when(pl.col("side").cast(pl.String, strict=False).str.to_lowercase().is_in(["ask", "sell"]))
            .then(pl.lit(SELL_EVENT, dtype=pl.UInt64))
            .otherwise(pl.lit(None, dtype=pl.UInt64))
            .alias("side_code"),
        pl.col("price").cast(pl.Float64, strict=False).alias("px"),
        pl.col("quantity").cast(pl.Float64, strict=False).alias("qty"),
    ]

    for column in ORDERBOOK_MESSAGE_ID_COLUMNS:
        if column in df.columns:
            if column == "symbol":
                select_exprs.append(pl.col(column).cast(pl.String, strict=False).alias(column))
            else:
                select_exprs.append(pl.col(column).cast(pl.Int64, strict=False).alias(column))

    normalized = df.select(select_exprs)
    _validate_non_null(normalized, ("local_ts", "exch_ts", "event_type", "side_code", "px", "qty"), "orderbook")

    unsupported = normalized.filter(~pl.col("event_type").is_in(["update", "snapshot"]))
    if unsupported.height > 0:
        raise ValueError(
            "orderbook contains unsupported event_type values; expected only 'update' or 'snapshot'."
        )

    return normalized


def _normalize_trades(df: pl.DataFrame) -> pl.DataFrame:
    _validate_columns(df, TRADES_REQUIRED_COLUMNS, "trades")

    if "trade_time" in df.columns:
        trade_time_expr = pl.col("trade_time").cast(pl.Int64, strict=False)
    else:
        trade_time_expr = pl.lit(None, dtype=pl.Int64)

    normalized = df.select(
        pl.col("received_time").cast(pl.Int64, strict=False).alias("local_ts"),
        (pl.coalesce([trade_time_expr, pl.col("event_time").cast(pl.Int64, strict=False)]) * 1_000_000)
            .cast(pl.Int64, strict=False)
            .alias("exch_ts"),
        pl.col("price").cast(pl.Float64, strict=False).alias("px"),
        pl.col("quantity").cast(pl.Float64, strict=False).alias("qty"),
        pl.col("is_buyer_maker").cast(pl.Boolean, strict=False).alias("is_buyer_maker"),
    )
    _validate_non_null(normalized, ("local_ts", "exch_ts", "px", "qty", "is_buyer_maker"), "trades")
    return normalized


def _validate_columns(df: pl.DataFrame, required: frozenset[str], label: str) -> None:
    missing = sorted(required.difference(df.columns))
    if missing:
        raise ValueError(f"{label} is missing required columns: {', '.join(missing)}.")


def _validate_non_null(df: pl.DataFrame, columns: tuple[str, ...], label: str) -> None:
    invalid = [column for column in columns if df[column].null_count() > 0]
    if invalid:
        raise ValueError(
            f"{label} contains null or unsupported values in required fields: {', '.join(sorted(invalid))}."
        )


def _convert_trades(df: pl.DataFrame) -> NDArray:
    if df.height == 0:
        return np.empty(0, event_dtype)

    return (
        df.with_columns(
            pl.when(pl.col("is_buyer_maker"))
                .then(pl.lit(SELL_EVENT | TRADE_EVENT, dtype=pl.UInt64))
                .otherwise(pl.lit(BUY_EVENT | TRADE_EVENT, dtype=pl.UInt64))
                .alias("ev"),
            ZERO_U64.alias("order_id"),
            ZERO_I64.alias("ival"),
            ZERO_F64.alias("fval"),
        )
        .select(["ev", "exch_ts", "local_ts", "px", "qty", "order_id", "ival", "fval"])
        .to_numpy(structured=True)
    )


def _convert_orderbook_updates(df: pl.DataFrame) -> NDArray:
    updates = df.filter(pl.col("event_type") == "update")
    if updates.height == 0:
        return np.empty(0, event_dtype)

    return (
        updates.with_columns(
            (pl.col("side_code") + pl.lit(DEPTH_EVENT, dtype=pl.UInt64)).alias("ev"),
            ZERO_U64.alias("order_id"),
            ZERO_I64.alias("ival"),
            ZERO_F64.alias("fval"),
        )
        .select(["ev", "exch_ts", "local_ts", "px", "qty", "order_id", "ival", "fval"])
        .to_numpy(structured=True)
    )


def _convert_orderbook_snapshots(df: pl.DataFrame) -> NDArray:
    snapshots = df.filter(pl.col("event_type") == "snapshot")
    if snapshots.height == 0:
        return np.empty(0, event_dtype)

    group_keys = ["local_ts", "exch_ts", "side_code"]
    for column in ORDERBOOK_MESSAGE_ID_COLUMNS:
        if column in snapshots.columns:
            group_keys.append(column)

    group_count = snapshots.group_by(group_keys, maintain_order=True).len().height
    out = np.empty(snapshots.height + group_count, event_dtype)

    ordered = snapshots.with_columns(
        pl.when(pl.col("side_code") == BUY_EVENT)
            .then(pl.lit(0, dtype=pl.UInt8))
            .otherwise(pl.lit(1, dtype=pl.UInt8))
            .alias("_side_sort")
    ).sort(["local_ts", "exch_ts", "_side_sort"])

    row_num = 0
    for group in ordered.partition_by(group_keys, maintain_order=True):
        row = group.row(0, named=True)
        local_ts = int(row["local_ts"])
        exch_ts = int(row["exch_ts"])
        side_code = int(row["side_code"])

        if side_code == BUY_EVENT:
            clear_px = float(group["px"].min())
            snapshot_ev = DEPTH_SNAPSHOT_EVENT | BUY_EVENT
            clear_ev = DEPTH_CLEAR_EVENT | BUY_EVENT
            ordered_group = group.sort("px", descending=True)
        elif side_code == SELL_EVENT:
            clear_px = float(group["px"].max())
            snapshot_ev = DEPTH_SNAPSHOT_EVENT | SELL_EVENT
            clear_ev = DEPTH_CLEAR_EVENT | SELL_EVENT
            ordered_group = group.sort("px")
        else:  # pragma: no cover - protected by normalization.
            raise ValueError(f"Unsupported side_code in snapshot group: {side_code}")

        out[row_num] = (clear_ev, exch_ts, local_ts, clear_px, 0.0, 0, 0, 0.0)
        row_num += 1

        for px, qty in ordered_group.select(["px", "qty"]).iter_rows():
            out[row_num] = (snapshot_ev, exch_ts, local_ts, float(px), float(qty), 0, 0, 0.0)
            row_num += 1

    return out[:row_num]


__all__ = ("download", "convert", "download_and_convert")
