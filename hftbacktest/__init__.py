from .latencies import ConstantLatency, FeedLatency
from .assettype import Linear, Inverse
from .queue import RiskAverseQueueModel, LogProbQueueModel, IdentityProbQueueModel, SquareProbQueueModel
from .backtest import COL_EVENT, COL_EXCH_TIMESTAMP, COL_LOCAL_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY,\
    DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT, TRADE_EVENT, BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, GTC, GTX, Order, \
    HftBacktest as _HftBacktest, hbt_cls_spec
from .stat import Stat
from .data import validate_data, correct_local_timestamp, correct_exch_timestamp, correct
from numba.experimental import jitclass

__all__ = ('COL_EVENT', 'COL_EXCH_TIMESTAMP', 'COL_LOCAL_TIMESTAMP', 'COL_SIDE', 'COL_PRICE', 'COL_QTY',
           'DEPTH_EVENT', 'TRADE_EVENT', 'DEPTH_CLEAR_EVENT', 'DEPTH_SNAPSHOT_EVENT', 'BUY', 'SELL',
           'NONE', 'NEW', 'EXPIRED', 'FILLED', 'CANCELED', 'GTC', 'GTX',
           'Order', 'HftBacktest',
           'FeedLatency', 'ConstantLatency',
           'Linear', 'Inverse',
           'RiskAverseQueueModel', 'LogProbQueueModel', 'IdentityProbQueueModel', 'SquareProbQueueModel',
           'Stat',
           'validate_data', 'correct_local_timestamp', 'correct_exch_timestamp', 'correct')

__version__ = '1.1.0'


def HftBacktest(df, tick_size, lot_size, maker_fee, taker_fee, order_latency, asset_type, queue_model=None,
                snapshot=None, start_row=0, start_position=0, start_balance=0, start_fee=0):
    assert (df.columns[:6] == ['event', 'exch_timestamp', 'local_timestamp', 'side', 'price', 'qty']).all()
    if queue_model is None:
        queue_model = RiskAverseQueueModel()
    spec = hbt_cls_spec + [
        ('order_latency', order_latency._numba_type_),
        ('asset_type', asset_type._numba_type_),
        ('queue_model', queue_model._numba_type_)
    ]
    hbt = jitclass(spec=spec)(_HftBacktest)
    # hbt = _HftBacktest
    return hbt(df.to_numpy(), tick_size, lot_size, maker_fee, taker_fee, order_latency, asset_type, queue_model,
               snapshot.values if snapshot is not None else None,
               start_row, start_position, start_balance, start_fee)
