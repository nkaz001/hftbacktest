import numpy as np
import pandas as pd

from .assettype import Linear, Inverse
from .reader import COL_EVENT, COL_EXCH_TIMESTAMP, COL_LOCAL_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY, \
    DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT, TRADE_EVENT, DataReader, DataBinder, Cache
from .order import BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, GTC, GTX, Order, order_ty, OrderBus
from .backtest import SingleInstHftBacktest
from .data import validate_data, correct_local_timestamp, correct_exch_timestamp, correct
from .exchange import NoPartialFillExch
from .latencies import ConstantLatency, FeedLatency
from .local import Local
from .marketdepth import MarketDepth
from .state import State
from .queue import RiskAverseQueueModel, LogProbQueueModel, IdentityProbQueueModel, SquareProbQueueModel
from .stat import Stat

__all__ = ('COL_EVENT', 'COL_EXCH_TIMESTAMP', 'COL_LOCAL_TIMESTAMP', 'COL_SIDE', 'COL_PRICE', 'COL_QTY',
           'DEPTH_EVENT', 'TRADE_EVENT', 'DEPTH_CLEAR_EVENT', 'DEPTH_SNAPSHOT_EVENT', 'BUY', 'SELL',
           'NONE', 'NEW', 'EXPIRED', 'FILLED', 'CANCELED', 'GTC', 'GTX',
           'Order', 'HftBacktest',
           'FeedLatency', 'ConstantLatency',
           'Linear', 'Inverse',
           'RiskAverseQueueModel', 'LogProbQueueModel', 'IdentityProbQueueModel', 'SquareProbQueueModel',
           'Stat',
           'validate_data', 'correct_local_timestamp', 'correct_exch_timestamp', 'correct')

__version__ = '2.0.0'


def HftBacktest(data, tick_size, lot_size, maker_fee, taker_fee, order_latency, asset_type, queue_model=None,
                snapshot=None, start_position=0, start_balance=0, start_fee=0, trade_list_size=0):

    cached = Cache()

    if isinstance(data, pd.DataFrame):
        assert (data.columns[:6] == ['event', 'exch_timestamp', 'local_timestamp', 'side', 'price', 'qty']).all()
        local_reader = DataBinder(data.to_numpy())
        exch_reader = DataBinder(data.to_numpy())
    elif isinstance(data, np.ndarray):
        assert data.shape[1] >= 6
        local_reader = DataBinder(data)
        exch_reader = DataBinder(data)
    elif isinstance(data, str):
        local_reader = DataReader(cached)
        local_reader.add_file(data)

        exch_reader = DataReader(cached)
        exch_reader.add_file(data)
    elif isinstance(data, list):
        local_reader = DataReader(cached)
        exch_reader = DataReader(cached)
        for filepath in data:
            assert isinstance(filepath, str)
            local_reader.add_file(filepath)
            exch_reader.add_file(filepath)
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
            assert (snapshot.columns[:6] == [
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
        order_latency
    )
    exch = NoPartialFillExch(
        exch_reader,
        exch_to_local_orders,
        local_to_exch_orders,
        exch_market_depth,
        exch_state,
        order_latency,
        queue_model
    )

    return SingleInstHftBacktest(local, exch)
