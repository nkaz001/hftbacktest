from numba import int64, boolean, typeof
from numba.experimental import jitclass

from . import BUY
from .order import LIMIT, SELL
from .reader import WAIT_ORDER_RESPONSE_NONE, COL_LOCAL_TIMESTAMP, UNTIL_END_OF_DATA


class SingleInstHftBacktest_:
    def __init__(self, local, exch):
        self.local = local
        self.exch = exch

        self.run = True
        self.current_timestamp = self.local.next_data[0, COL_LOCAL_TIMESTAMP]

    @property
    def start_timestamp(self):
        # fixme: deprecated.
        return self.local.data[0, COL_LOCAL_TIMESTAMP]

    @property
    def last_timestamp(self):
        # fixme: deprecated.
        return UNTIL_END_OF_DATA

    @property
    def position(self):
        return self.local.state.position

    @property
    def balance(self):
        return self.local.state.balance

    @property
    def fee(self):
        return self.local.state.fee

    @property
    def trade_num(self):
        return self.local.state.trade_num

    @property
    def trade_qty(self):
        return self.local.state.trade_qty

    @property
    def trade_amount(self):
        return self.local.state.trade_amount

    @property
    def orders(self):
        return self.local.orders

    @property
    def tick_size(self):
        return self.local.depth.tick_size

    @property
    def high_ask_tick(self):
        return self.local.depth.high_ask_tick

    @property
    def low_bid_tick(self):
        return self.local.depth.low_bid_tick

    @property
    def best_bid_tick(self):
        return self.local.depth.best_bid_tick

    @property
    def best_ask_tick(self):
        return self.local.depth.best_ask_tick

    @property
    def best_bid(self):
        return self.best_bid_tick * self.tick_size

    @property
    def best_ask(self):
        return self.best_ask_tick * self.tick_size

    @property
    def bid_depth(self):
        return self.local.depth.bid_depth

    @property
    def ask_depth(self):
        return self.local.depth.ask_depth

    @property
    def mid(self):
        return (self.best_bid + self.best_ask) / 2.0

    @property
    def equity(self):
        return self.local.state.equity(self.mid)

    @property
    def last_trade(self):
        if self.local.trade_len > 0:
            return self.last_trades[self.local.trade_len - 1]
        else:
            return None

    @property
    def last_trades(self):
        return self.local.last_trades[:self.local.trade_len]

    @property
    def local_timestamp(self):
        return self.current_timestamp

    def submit_buy_order(self, order_id, price, qty, time_in_force, order_type=LIMIT, wait=False):
        self.local.submit_order(order_id, BUY, price, qty, order_type, time_in_force, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def submit_sell_order(self, order_id, price, qty, time_in_force, order_type=LIMIT, wait=False):
        self.local.submit_order(order_id, SELL, price, qty, order_type, time_in_force, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def cancel(self, order_id, wait=False):
        self.local.cancel(order_id, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def wait_order_response(self, order_id, timeout=-1):
        if self.local.orders_from.__contains__(order_id):
            timestamp = self.local.orders_from.get(order_id)
            return self.goto(timestamp)

        if not self.local.orders_to.__contains__(order_id):
            return True

        if timeout >= 0:
            timestamp = self.current_timestamp + timeout
        else:
            timestamp = UNTIL_END_OF_DATA

        return self.goto(timestamp, wait_order_response=order_id)

    # todo: implement
    # def wait_next_feed(self, include_order_resp, timeout=-1):
    #     raise NotImplementedError

    def clear_inactive_orders(self):
        self.local.clear_inactive_orders()

    def clear_last_trades(self):
        self.local.clear_last_trades()

    def get_user_data(self, event):
        return self.local.get_user_data(event)

    def elapse(self, duration):
        return self.goto(self.current_timestamp + duration)

    def goto(self, timestamp, wait_order_response=WAIT_ORDER_RESPONSE_NONE):
        found_order_resp_timestamp = False
        while True:
            # Select which side will be processed next.
            next_local_timestamp = self.local.next_timestamp()
            next_exch_timestamp = self.exch.next_timestamp()

            # print(next_local_timestamp, next_exch_timestamp)

            # Local will be processed.
            if (0 < next_local_timestamp < next_exch_timestamp) \
                    or (next_local_timestamp > 0 >= next_exch_timestamp):
                if next_local_timestamp > timestamp:
                    break
                resp_timestamp = self.local.process(WAIT_ORDER_RESPONSE_NONE)

            # Exchange will be processed.
            elif (0 < next_exch_timestamp <= next_local_timestamp) \
                    or (next_exch_timestamp > 0 >= next_local_timestamp):
                if next_exch_timestamp > timestamp:
                    break
                resp_timestamp = self.exch.process(
                    wait_order_response if not found_order_resp_timestamp else WAIT_ORDER_RESPONSE_NONE
                )

            # No more data or orders to be processed.
            else:
                self.run = False
                break

            if resp_timestamp > 0:
                found_order_resp_timestamp = True
                timestamp = resp_timestamp

        self.current_timestamp = timestamp

        if not self.run:
            return False
        return True


def SingleInstHftBacktest(local, exch):
    jitted = jitclass(spec=[
        ('run', boolean),
        ('current_timestamp', int64),
        ('local', typeof(local)),
        ('exch', typeof(exch)),
    ])(SingleInstHftBacktest_)
    return jitted(local, exch)
