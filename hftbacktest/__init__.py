from typing import Union, List, Optional

import numpy as np
import pandas as pd
from numba import boolean, int64, float64, typeof
from numba.experimental import jitclass

from .assettype import (
    LinearAsset as LinearAsset_,
    InverseAsset as InverseAsset_,
)
from .backtest import SingleAssetHftBacktest as SingleAssetHftBacktest_
from .data import (
    merge_on_local_timestamp,
    validate_data,
    correct_local_timestamp,
    correct_exch_timestamp,
    correct_exch_timestamp_adjust,
    correct,
)
from .marketdepth import MarketDepth
from .models.latencies import (
    FeedLatency as FeedLatency_,
    ConstantLatency as ConstantLatency_,
    ForwardFeedLatency as ForwardFeedLatency_,
    BackwardFeedLatency as BackwardFeedLatency_,
    IntpOrderLatency as IntpOrderLatency_
)
from .models.queue import (
    RiskAverseQueueModel as RiskAverseQueueModel_,
    LogProbQueueModel as LogProbQueueModel_,
    IdentityProbQueueModel as IdentityProbQueueModel_,
    SquareProbQueueModel as SquareProbQueueModel_,
    PowerProbQueueModel as PowerProbQueueModel_,
    LogProbQueueModel2 as LogProbQueueModel2_,
    PowerProbQueueModel2 as PowerProbQueueModel2_,
    PowerProbQueueModel3 as PowerProbQueueModel3_
)
from .order import BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, MODIFY, GTC, GTX, Order, OrderBus
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
from .typing import (
    Data,
    ExchangeModelInitiator,
    AssetType,
    OrderLatencyModel,
    QueueModel,
    DataCollection,
    HftBacktestType
)

__all__ = (
    # Columns
    'COL_EVENT',
    'COL_EXCH_TIMESTAMP',
    'COL_LOCAL_TIMESTAMP',
    'COL_SIDE',
    'COL_PRICE',
    'COL_QTY',

    # Event types
    'DEPTH_EVENT',
    'TRADE_EVENT',
    'DEPTH_CLEAR_EVENT',
    'DEPTH_SNAPSHOT_EVENT',

    # Side
    'BUY',
    'SELL',

    # Order status
    'NONE',
    'NEW',
    'EXPIRED',
    'FILLED',
    'CANCELED',
    'MODIFY',

    # Time-In-Force
    'GTC',
    'GTX',

    # Exchange models
    'NoPartialFillExchange',
    'PartialFillExchange',

    # Latency models
    'ConstantLatency',
    'FeedLatency',
    'ForwardFeedLatency',
    'BackwardFeedLatency',
    'IntpOrderLatency',

    # Asset types
    'LinearAsset',
    'InverseAsset',
    'Linear',
    'Inverse',

    # Queue models
    'RiskAverseQueueModel',
    'LogProbQueueModel',
    'IdentityProbQueueModel',
    'SquareProbQueueModel',
    'PowerProbQueueModel',
    'LogProbQueueModel2',
    'PowerProbQueueModel2',
    'PowerProbQueueModel3',

    'HftBacktest',
    'Order',
    'Stat',

    'merge_on_local_timestamp',
    'validate_data',
    'correct_local_timestamp',
    'correct_exch_timestamp',
    'correct_exch_timestamp_adjust',
    'correct'
)

__version__ = '1.8.2'


# JIT'ed latency models
ConstantLatency = jitclass()(ConstantLatency_)
FeedLatency = jitclass()(FeedLatency_)
ForwardFeedLatency = jitclass()(ForwardFeedLatency_)
BackwardFeedLatency = jitclass()(BackwardFeedLatency_)
IntpOrderLatency = jitclass()(IntpOrderLatency_)

# JIT'ed queue models
RiskAverseQueueModel = jitclass()(RiskAverseQueueModel_)
LogProbQueueModel = jitclass()(LogProbQueueModel_)
IdentityProbQueueModel = jitclass()(IdentityProbQueueModel_)
SquareProbQueueModel = jitclass()(SquareProbQueueModel_)
PowerProbQueueModel = jitclass(spec=[('n', float64)])(PowerProbQueueModel_)
LogProbQueueModel2 = jitclass()(LogProbQueueModel2_)
PowerProbQueueModel2 = jitclass(spec=[('n', float64)])(PowerProbQueueModel2_)
PowerProbQueueModel3 = jitclass(spec=[('n', float64)])(PowerProbQueueModel3_)

# JIT'ed asset types
LinearAsset = jitclass()(LinearAsset_)
InverseAsset = jitclass()(InverseAsset_)

Linear = LinearAsset()
Inverse = InverseAsset()

# JIT'ed HftBacktest
def SingleAssetHftBacktest(local, exch):
    jitted = jitclass(spec=[
        ('run', boolean),
        ('current_timestamp', int64),
        ('local', typeof(local)),
        ('exch', typeof(exch)),
    ])(SingleAssetHftBacktest_)
    return jitted(local, exch)


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
        order_latency: Order latency model. See :doc:`Order Latency Models <order_latency_models>`.
        asset_type: Either ``Linear`` or ``Inverse``. See :doc:`Asset types <asset_types>`.
        queue_model: Queue model with default set as :class:`.models.queue.RiskAverseQueueModel`. See :doc:`Queue Models <queue_models>`.
        snapshot: The initial market depth snapshot.
        start_position: Starting position.
        start_balance: Starting balance.
        start_fee: Starting cumulative fees.
        trade_list_size: Buffer size for storing market trades; the default value of ``0`` indicates that market trades
                         will not be stored in the buffer.
        exchange_model: Exchange model with default set as ``NoPartialFillExchange``.

    Returns:
         JIT'ed :class:`.SingleAssetHftBacktest`
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
        hbt: HftBacktestType,
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
