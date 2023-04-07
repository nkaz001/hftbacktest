import numpy as np
import pandas as pd

from .assettype import Linear, Inverse
from .reader import COL_EVENT, COL_EXCH_TIMESTAMP, COL_LOCAL_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY, \
    DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT, TRADE_EVENT, DataReader, Cache
from .order import BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, GTC, GTX, Order, OrderBus
from .backtest import SingleAssetHftBacktest
from .data import validate_data, correct_local_timestamp, correct_exch_timestamp, correct
from .proc.local import Local
from .proc.nopartialfillexchange import NoPartialFillExchange
from .proc.partialfillexchange import PartialFillExchange
from .marketdepth import MarketDepth
from .state import State
from .models.latencies import FeedLatency, ConstantLatency, ForwardFeedLatency, BackwardFeedLatency, IntpOrderLatency
from .models.queue import RiskAverseQueueModel, LogProbQueueModel, IdentityProbQueueModel, SquareProbQueueModel
from .stat import Stat

__all__ = ('COL_EVENT', 'COL_EXCH_TIMESTAMP', 'COL_LOCAL_TIMESTAMP', 'COL_SIDE', 'COL_PRICE', 'COL_QTY',
           'DEPTH_EVENT', 'TRADE_EVENT', 'DEPTH_CLEAR_EVENT', 'DEPTH_SNAPSHOT_EVENT',
           'BUY', 'SELL',
           'NONE', 'NEW', 'EXPIRED', 'FILLED', 'CANCELED',
           'GTC', 'GTX',
           'Order', 'HftBacktest',
           'NoPartialFillExchange', 'PartialFillExchange',
           'ConstantLatency', 'FeedLatency', 'ForwardFeedLatency', 'BackwardFeedLatency', 'IntpOrderLatency',
           'Linear', 'Inverse',
           'RiskAverseQueueModel', 'LogProbQueueModel', 'IdentityProbQueueModel', 'SquareProbQueueModel',
           'Stat',
           'validate_data', 'correct_local_timestamp', 'correct_exch_timestamp', 'correct',)

__version__ = '1.5.0'


def HftBacktest(
        data,
        tick_size,
        lot_size,
        maker_fee,
        taker_fee,
        order_latency,
        asset_type,
        queue_model=None,
        snapshot=None,
        start_position=0,
        start_balance=0,
        start_fee=0,
        trade_list_size=0,
        exchange_model=None
):
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
        data,
        tick_size=None,
        lot_size=None,
        maker_fee=None,
        taker_fee=None,
        snapshot=None,
        start_position=0,
        start_balance=0,
        start_fee=0,
        trade_list_size=None,
):
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
