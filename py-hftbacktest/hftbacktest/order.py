from typing import Any

import numpy as np
from numba import float32, int32, int64, uint8, from_dtype
from numba.experimental import jitclass


UNSUPPORTED = 255

BUY = 1
SELL = -1

NONE = 0
NEW = 1
EXPIRED = 2
FILLED = 3
CANCELED = 4
PARTIALLY_FILLED = 5
MODIFIED = 6
REJECTED = 7

GTC = 0  # Good 'till cancel
GTX = 1  # Post only
FOK = 2  # Fill or kill
IOC = 3  # Immediate or cancel

LIMIT = 0
MARKET = 1

order_dtype = np.dtype([
    ('qty', 'f8'),
    ('leaves_qty', 'f8'),
    ('exec_qty', 'f8'),
    ('exec_price_tick', 'i8'),
    ('price_tick', 'i8'),
    ('tick_size', 'f8'),
    ('exch_timestamp', 'i8'),
    ('local_timestamp', 'i8'),
    ('order_id', 'i8'),
    ('_q1', 'u8'),
    ('_q2', 'u8'),
    ('maker', 'bool'),
    ('order_type', 'u1'),
    ('req', 'u1'),
    ('status', 'u1'),
    ('side', 'u1'),
    ('time_in_force', 'u1'),
])


class Order:
    arr: from_dtype(order_dtype)[:]

    def __init__(self, arr: np.ndarray[Any, order_dtype]):
        self.arr = arr

    @property
    def price(self) -> float32:
        """
        Returns the order price.
        """
        return self.arr[0].price_tick * self.arr[0].tick_size

    @property
    def exec_price(self) -> float32:
        """
        Returns the executed price. This is only valid if :obj:`status` is :const:`FILLED` or :const:`PARTIALLY_FILLED`.
        """
        return self.arr[0].exec_price_tick * self.arr[0].tick_size

    @property
    def cancellable(self) -> bool:
        """
        Returns whether this order can be canceled. The order can be canceled only if it is active, meaning its
        :obj:`status` should be :const:`NEW` or :const:`PARTIALLY_FILLED`. It is not necessary for there to be no
        ongoing requests on the order to cancel it. However, HftBacktest currently enforces that there are no ongoing
        requests to cancel this order to simplify the implementation.
        """
        return (self.arr[0].status == NEW or self.arr[0].status == PARTIALLY_FILLED) and self.arr[0].req == NONE

    @property
    def qty(self) -> float32:
        """
        Returns the order quantity.
        """
        return self.arr[0].qty

    @property
    def leaves_qty(self) -> float32:
        """
        Returns the remaining active quantity after the order has been partially filled. In backtesting, this is only
        valid in exchange models that support partial fills, such as `PartialFillExchange` model.
        """
        return self.arr[0].leaves_qty

    @property
    def price_tick(self) -> int32:
        """
        Returns the order price in ticks.
        """
        return self.arr[0].price_tick

    @property
    def tick_size(self) -> float32:
        """
        Returns the tick size.
        """
        return self.arr[0].price_tick

    @property
    def exch_timestamp(self) -> int64:
        """
        Returns the timestamp when the order is processed by the exchange.
        """
        return self.arr[0].exch_timestamp

    @property
    def local_timestamp(self) -> int64:
        """
        Returns the timestamp when the order request is made by the local.
        """
        return self.arr[0].local_timestamp

    @property
    def exec_price_tick(self) -> int32:
        """
        Returns the executed price in ticks. This is only valid if :obj:`status` is :const:`FILLED` or
        :const:`PARTIALLY_FILLED`.
        """
        return self.arr[0].exec_price_tick

    @property
    def exec_qty(self) -> float32:
        """
        Returns the executed quantity. This is only valid if :obj:`status` is :const:`FILLED` or
        :const:`PARTIALLY_FILLED`.
        """
        return self.arr[0].exec_qty

    @property
    def order_id(self) -> int64:
        """
        Returns the order ID.
        """
        return self.arr[0].order_id

    @property
    def order_type(self) -> uint8:
        """
        Returns the order type. This can be one of the following values, but may vary depending on the exchange model.

            * :const:`MARKET`
            * :const:`LIMIT`
        """
        return self.arr[0].order_type

    @property
    def req(self) -> uint8:
        """
        Returns the type of the current ongoing request. This can be one of the following values, but may vary depending
        on the exchange model.

            * :const:`NONE` for no ongoing request.
            * :const:`NEW` for submitting a new order.
            * :const:`CANCELED` for canceling the order.
        """
        return self.arr[0].req

    @property
    def status(self) -> uint8:
        """
        Returns the order status. This can be one of the following values, but may vary depending on the exchange model.

            * :const:`NONE`
            * :const:`NEW`
            * :const:`EXPIRED`
            * :const:`FILLED`
            * :const:`CANCELED`
            * :const:`PARTIALLY_FILLED`
        """
        return self.arr[0].status

    @property
    def side(self) -> uint8:
        """
        Returns the order side.

            * :const:`BUY`
            * :const:`SELL`
        """
        return self.arr[0].side

    @property
    def time_in_force(self) -> uint8:
        """
        Returns the Time-In-Force of the order. This can be one of the following values, but may vary depending on the
        exchange model.

            * :const:`GTC`
            * :const:`GTX`
            * :const:`FOK`
            * :const:`IOC`
        """
        return self.arr[0].time_in_force


Order_ = jitclass(Order)
