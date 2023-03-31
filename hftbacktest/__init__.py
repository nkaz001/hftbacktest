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

__version__ = '1.4.0'


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

    if isinstance(data, pd.DataFrame):
        local_reader = DataReader(cache)
        local_reader.add_data(data.to_numpy())

        exch_reader = DataReader(cache)
        exch_reader.add_data(data.to_numpy())
    elif isinstance(data, np.ndarray):
        local_reader = DataReader(cache)
        local_reader.add_data(data)

        exch_reader = DataReader(cache)
        exch_reader.add_data(data)
    elif isinstance(data, str):
        local_reader = DataReader(cache)
        local_reader.add_file(data)

        exch_reader = DataReader(cache)
        exch_reader.add_file(data)
    elif isinstance(data, list):
        local_reader = DataReader(cache)
        exch_reader = DataReader(cache)
        for filepath in data:
            if isinstance(filepath, str):
                local_reader.add_file(filepath)
                exch_reader.add_file(filepath)
            elif isinstance(filepath, pd.DataFrame) or isinstance(filepath, np.ndarray):
                local_reader.add_data(filepath)
                exch_reader.add_data(filepath)
            else:
                raise ValueError('Unsupported data type')
    else:
        raise ValueError('Unsupported data type')

    if isinstance(snapshot, pd.DataFrame):
        assert (snapshot.columns[:6] == [
            'event',
            'exch_timestamp',
            'local_timestamp',
            'side',
            'price',
            'qty'
        ]).all()
        snapshot = snapshot.to_numpy()
    elif isinstance(snapshot, np.ndarray):
        assert snapshot.shape[1] >= 6
    elif isinstance(snapshot, str):
        if snapshot.endswith('.npy'):
            snapshot = np.load(snapshot)
        elif snapshot.endswith('.npz'):
            tmp = np.load(snapshot)
            if 'data' in tmp:
                snapshot = tmp['data']
                assert snapshot.shape[1] >= 6
            else:
                k = list(tmp.keys())[0]
                print("Snapshot is loaded from %s instead of 'data'" % k)
                snapshot = tmp[k]
                assert snapshot.shape[1] >= 6
        else:
            df = pd.read_pickle(snapshot, compression='gzip')
            assert (df.columns[:6] == [
                'event',
                'exch_timestamp',
                'local_timestamp',
                'side',
                'price',
                'qty'
            ]).all()
            snapshot = df.to_numpy()
    elif snapshot is None:
        pass
    else:
        raise ValueError('Unsupported snapshot type')

    if queue_model is None:
        queue_model = RiskAverseQueueModel()

    local_market_depth = MarketDepth(tick_size, lot_size)
    exch_market_depth = MarketDepth(tick_size, lot_size)

    if snapshot is not None:
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
