from typing import Union, List, Optional

import numpy as np
import pandas as pd

from .assettype import Linear, Inverse
from .backtest import SingleAssetHftBacktest
from .data import validate_data, correct_local_timestamp, correct_exch_timestamp, correct
from .marketdepth import MarketDepth
from .models.latencies import FeedLatency, ConstantLatency, ForwardFeedLatency, BackwardFeedLatency, IntpOrderLatency
from .models.queue import RiskAverseQueueModel, LogProbQueueModel, IdentityProbQueueModel, SquareProbQueueModel
from .order import BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, GTC, GTX, Order, OrderBus
from .proc.local import Local
from .proc.nopartialfillexchange import NoPartialFillExchange
from .proc.partialfillexchange import PartialFillExchange
from .reader import (
    COL_EVENT,
    COL_EXCH_TIMESTAMP,
    COL_LOCAL_TIMESTAMP,
    COL_SIDE,
    COL_PRICE,
    COL_QTY,
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    DataReader,
    Cache
)
from .stat import Stat
from .state import State

__all__ = (
    'COL_EVENT',
    'COL_EXCH_TIMESTAMP',
    'COL_LOCAL_TIMESTAMP',
    'COL_SIDE',
    'COL_PRICE',
    'COL_QTY',
    'DEPTH_EVENT',
    'TRADE_EVENT',
    'DEPTH_CLEAR_EVENT',
    'DEPTH_SNAPSHOT_EVENT',
    'BUY',
    'SELL',
    'NONE',
    'NEW',
    'EXPIRED',
    'FILLED',
    'CANCELED',
    'GTC',
    'GTX',
    'Order',
    'HftBacktest',
    'NoPartialFillExchange',
    'PartialFillExchange',
    'ConstantLatency',
    'FeedLatency',
    'ForwardFeedLatency',
    'BackwardFeedLatency',
    'IntpOrderLatency',
    'Linear',
    'Inverse',
    'RiskAverseQueueModel',
    'LogProbQueueModel',
    'IdentityProbQueueModel',
    'SquareProbQueueModel',
    'Stat',
    'validate_data',
    'correct_local_timestamp',
    'correct_exch_timestamp',
    'correct'
)

__version__ = '1.5.1'

from .typing import Data, ExchangeModelInitiator, AssetType, OrderLatencyModel, QueueModel, DataCollection


def HftBacktest(
        data: DataCollection,
        tick_size: float,
        lot_size: float,
        maker_fee: float,
        taker_fee: float,
        order_latency: OrderLatencyModel,
        asset_type: AssetType,
        queue_model: Optional[QueueModel] = None,
        snapshot: Optional[Data] = None,
        start_position: float = 0,
        start_balance: float = 0,
        start_fee: float = 0,
        trade_list_size: int = 0,
        exchange_model: ExchangeModelInitiator = None
):
    r"""
    Create a HftBacktest instance.

    Args:
        data: Data to be fed.
        tick_size: Minimum price increment for the given asset.
        lot_size: Minimum order quantity for the given asset.
        maker_fee: Maker fee rate; a negative value indicates rebates.
        taker_fee: Taker fee rate; a negative value indicates rebates.
        order_latency: Order latency model. See :mod:`.models.latencies`.
        asset_type: Either ``Linear`` or ``Inverse``. See :mod:`.assettype`.
        queue_model: Queue model with default set as ``RiskAverseQueueModel``. See the :mod:`.models.queue`.
        snapshot: The initial market depth snapshot.
        start_position: Starting position.
        start_balance: Starting balance.
        start_fee: Starting cumulative fees.
        trade_list_size: Buffer size for storing market trades; the default value of ``0`` indicates that market trades
                         will not be stored in the buffer.
        exchange_model: Exchange model with default set as ``NoPartialFillExchange``.

    Returns:
         JIT'ed :class:`.backtest.SingleAssetHftBacktest_`
    """

    cache = Cache()

    if isinstance(data, list):
        local_reader = DataReader(cache)
        exch_reader = DataReader(cache)
        for item in data:
            if isinstance(item, str):
                local_reader.add_file(item)
                exch_reader.add_file(item)
            elif isinstance(item, pd.DataFrame) or isinstance(item, np.ndarray):
                local_reader.add_data(item)
                exch_reader.add_data(item)
            else:
                raise ValueError('Unsupported data type')
    elif isinstance(data, str):
        local_reader = DataReader(cache)
        local_reader.add_file(data)

        exch_reader = DataReader(cache)
        exch_reader.add_file(data)
    else:
        data = __load_data(data)
        local_reader = DataReader(cache)
        local_reader.add_data(data)

        exch_reader = DataReader(cache)
        exch_reader.add_data(data)

    if queue_model is None:
        queue_model = RiskAverseQueueModel()

    local_market_depth = MarketDepth(tick_size, lot_size)
    exch_market_depth = MarketDepth(tick_size, lot_size)

    if snapshot is not None:
        snapshot = __load_data(snapshot)
        local_market_depth.apply_snapshot(snapshot)
        exch_market_depth.apply_snapshot(snapshot)

    local_state = State(
        start_position,
        start_balance,
        start_fee,
        maker_fee,
        taker_fee,
        asset_type
    )
    exch_state = State(
        start_position,
        start_balance,
        start_fee,
        maker_fee,
        taker_fee,
        asset_type
    )

    exch_to_local_orders = OrderBus()
    local_to_exch_orders = OrderBus()

    local = Local(
        local_reader,
        local_to_exch_orders,
        exch_to_local_orders,
        local_market_depth,
        local_state,
        order_latency,
        trade_list_size
    )

    if exchange_model is None:
        exchange_model = NoPartialFillExchange

    exch = exchange_model(
        exch_reader,
        exch_to_local_orders,
        local_to_exch_orders,
        exch_market_depth,
        exch_state,
        order_latency,
        queue_model
    )

    return SingleAssetHftBacktest(local, exch)


def reset(
        hbt,
        data: Union[List[Data], Data],
        tick_size: Optional[float] = None,
        lot_size: Optional[float] = None,
        maker_fee: Optional[float] = None,
        taker_fee: Optional[float] = None,
        snapshot: Optional[Data] = None,
        start_position: Optional[float] = 0,
        start_balance: Optional[float] = 0,
        start_fee: Optional[float] = 0,
        trade_list_size: Optional[int] = None,
):
    """
    Reset the HftBacktest for reuse. This can help reduce Ahead-of-Time (AOT) compilation time by using the
    ``cache=True`` option in the ``@njit`` decorator.

    Args:
        hbt: HftBacktest instance to be reset.
        data: Data to be fed.
        tick_size: Minimum price increment for the given asset.
        lot_size: Minimum order quantity for the given asset.
        maker_fee: Maker fee rate; a negative value indicates rebates.
        taker_fee: Taker fee rate; a negative value indicates rebates.
        snapshot: The initial market depth snapshot.
        start_position: Starting position.
        start_balance: Starting balance.
        start_fee: Starting cumulative fees.
        trade_list_size: Buffer size for storing market trades; the default value of ``0`` indicates that market trades
                         will not be stored in the buffer.
    """
    cache = Cache()

    if isinstance(data, list):
        local_reader = DataReader(cache)
        exch_reader = DataReader(cache)
        for item in data:
            if isinstance(item, str):
                local_reader.add_file(item)
                exch_reader.add_file(item)
            elif isinstance(item, pd.DataFrame) or isinstance(item, np.ndarray):
                local_reader.add_data(item)
                exch_reader.add_data(item)
            else:
                raise ValueError('Unsupported data type')
    elif isinstance(data, str):
        local_reader = DataReader(cache)
        local_reader.add_file(data)

        exch_reader = DataReader(cache)
        exch_reader.add_file(data)
    else:
        data = __load_data(data)
        local_reader = DataReader(cache)
        local_reader.add_data(data)

        exch_reader = DataReader(cache)
        exch_reader.add_data(data)

    snapshot = __load_data(snapshot) if snapshot is not None else None

    hbt.reset(
        local_reader,
        exch_reader,
        start_position,
        start_balance,
        start_fee,
        maker_fee,
        taker_fee,
        tick_size,
        lot_size,
        snapshot,
        trade_list_size,
    )


def __load_data(data):
    if isinstance(data, pd.DataFrame):
        assert (data.columns[:6] == [
            'event',
            'exch_timestamp',
            'local_timestamp',
            'side',
            'price',
            'qty'
        ]).all()
        data = data.to_numpy()
    elif isinstance(data, np.ndarray):
        assert data.shape[1] >= 6
    elif isinstance(data, str):
        if data.endswith('.npy'):
            data = np.load(data)
        elif data.endswith('.npz'):
            tmp = np.load(data)
            if 'data' in tmp:
                data = tmp['data']
                assert data.shape[1] >= 6
            else:
                k = list(tmp.keys())[0]
                print("Data is loaded from %s instead of 'data'" % k)
                data = tmp[k]
                assert data.shape[1] >= 6
        else:
            df = pd.read_pickle(data, compression='gzip')
            assert (df.columns[:6] == [
                'event',
                'exch_timestamp',
                'local_timestamp',
                'side',
                'price',
                'qty'
            ]).all()
            data = df.to_numpy()
    else:
        raise ValueError('Unsupported data type')
    return data
