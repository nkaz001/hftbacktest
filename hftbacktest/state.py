from numba import typeof, float64, int64
from numba.experimental import jitclass


class State_:
    def __init__(
            self,
            start_position,
            start_balance,
            start_fee,
            maker_fee,
            taker_fee,
            asset_type
    ):
        self.position = start_position
        self.balance = start_balance
        self.fee = start_fee
        self.trade_num = 0
        self.trade_qty = 0
        self.trade_amount = 0
        self.maker_fee = maker_fee
        self.taker_fee = taker_fee
        self.asset_type = asset_type

    def apply_fill(self, order):
        fee = self.maker_fee if order.limit else self.taker_fee
        fill_qty = order.qty * order.side
        amount = self.asset_type.amount(order.exec_price, order.qty)
        fill_amount = amount * order.side
        fee_amount = amount * fee
        self.position += fill_qty
        self.balance -= fill_amount
        self.fee += fee_amount
        self.trade_num += 1
        self.trade_qty += order.qty
        self.trade_amount += amount

    def equity(self, mid):
        return self.asset_type.equity(mid, self.balance, self.position, self.fee)


def State(
        start_position,
        start_balance,
        start_fee,
        maker_fee,
        taker_fee,
        asset_type
):
    jitted = jitclass(spec=[
        ('position', float64),
        ('balance', float64),
        ('fee', float64),
        ('trade_num', int64),
        ('trade_qty', float64),
        ('trade_amount', float64),
        ('maker_fee', float64),
        ('taker_fee', float64),
        ('asset_type', typeof(asset_type))
    ])(State_)
    return jitted(
        start_position,
        start_balance,
        start_fee,
        maker_fee,
        taker_fee,
        asset_type
    )
