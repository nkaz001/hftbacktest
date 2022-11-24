from numba import njit

from .latencies import ConstantLatency, FeedLatency
from .assettype import Linear, Inverse
from .queue import RiskAverseQueueModel, LogProbQueueModel, IdentityProbQueueModel, SquareProbQueueModel
from .backtest import COL_EVENT, COL_EXCH_TIMESTAMP, COL_LOCAL_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY,\
    DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT, TRADE_EVENT, BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, GTC, GTX, Order, \
    HftBacktest as _HftBacktest, hbt_cls_spec
from .stat import Stat
from numba.experimental import jitclass

__all__ = ('COL_EVENT', 'COL_EXCH_TIMESTAMP', 'COL_LOCAL_TIMESTAMP', 'COL_SIDE', 'COL_PRICE', 'COL_QTY',
           'DEPTH_EVENT', 'TRADE_EVENT', 'DEPTH_CLEAR_EVENT', 'DEPTH_SNAPSHOT_EVENT', 'BUY', 'SELL',
           'NONE', 'NEW', 'EXPIRED', 'FILLED', 'CANCELED', 'GTC', 'GTX',
           'Order', 'HftBacktest',
           'FeedLatency', 'ConstantLatency',
           'Linear', 'Inverse',
           'RiskAverseQueueModel', 'LogProbQueueModel', 'IdentityProbQueueModel', 'SquareProbQueueModel',
           'Stat',
           'validate_data')

__version__ = '1.0.2'


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
    return hbt(df.values, tick_size, lot_size, maker_fee, taker_fee, order_latency, asset_type, queue_model,
               snapshot.values if snapshot is not None else None,
               start_row, start_position, start_balance, start_fee)


@njit
def _validate_data(values, tick_size=None, lot_size=None, err_bound=1e-8):
    num_reversed_exch_timestamp = 0
    prev_exch_timestamp = 0
    prev_local_timestamp = 0
    for row_num in range(len(values)):
        event = values[row_num, COL_EVENT]
        exch_timestamp = values[row_num, COL_EXCH_TIMESTAMP]
        local_timestamp = values[row_num, COL_LOCAL_TIMESTAMP]
        price = values[row_num, COL_PRICE]
        qty = values[row_num, COL_QTY]

        if exch_timestamp > local_timestamp and event in [TRADE_EVENT, DEPTH_EVENT, DEPTH_CLEAR_EVENT,
                                                          DEPTH_SNAPSHOT_EVENT]:
            print('found a row that local_timestamp is ahead of exch_timestamp. row_num =', row_num)
            return -1
        if prev_local_timestamp > local_timestamp:
            print('found a row that local_timestamp is ahead of the previous local_timestamp. row_num =', row_num)
            return -1

        if exch_timestamp < prev_exch_timestamp:
            num_reversed_exch_timestamp += 1

        if event in [TRADE_EVENT, DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT]:
            if tick_size is not None:
                v = price / tick_size
                e = v - round(v)
                if e > err_bound:
                    print('found a row that price does not match tick size. row_num =', row_num)
                    return -1
            if lot_size is not None:
                v = qty / lot_size
                e = v - round(v)
                if e > err_bound:
                    print('found a row that qty does not match lot size. row_num =', row_num)
                    return -1

        prev_local_timestamp = local_timestamp
        prev_exch_timestamp = exch_timestamp
    return num_reversed_exch_timestamp


def validate_data(df, tick_size=None, lot_size=None):
    num_inverse_exch_timestamp = _validate_data(df.values, tick_size, lot_size)
    if num_inverse_exch_timestamp > 0:
        print('found %d rows that exch_timestamp is ahead of the previous exch_timestamp' % num_inverse_exch_timestamp)
