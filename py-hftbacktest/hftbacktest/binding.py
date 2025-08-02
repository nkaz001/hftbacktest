from ctypes import (
    c_void_p,
    c_bool,
    c_double,
    c_uint8,
    c_uint64,
    c_int64,
    POINTER,
    CDLL
)
from typing import Tuple, Any

import numba
import numpy as np
from numba import (
    carray,
    uint64,
    int64,
    float64,
    uint8,
    from_dtype
)
from numba.core.types import voidptr
from numba.experimental import jitclass

from . import _hftbacktest
from .intrinsic import ptr_from_val, address_as_void_pointer, val_from_ptr, is_null_ptr
from .order import order_dtype, Order, Order_
from .state import StateValues, StateValues_
from .types import event_dtype, state_values_dtype, EVENT_ARRAY, DEPTH_EVENT, BUY_EVENT, SELL_EVENT

LIVE_FEATURE = 'build_hashmap_livebot' in dir(_hftbacktest)

lib = CDLL(_hftbacktest.__file__)

hashmapdepth_best_bid_tick = lib.hashmapdepth_best_bid_tick
hashmapdepth_best_bid_tick.restype = c_int64
hashmapdepth_best_bid_tick.argtypes = [c_void_p]

hashmapdepth_best_ask_tick = lib.hashmapdepth_best_ask_tick
hashmapdepth_best_ask_tick.restype = c_int64
hashmapdepth_best_ask_tick.argtypes = [c_void_p]

hashmapdepth_best_bid = lib.hashmapdepth_best_bid
hashmapdepth_best_bid.restype = c_double
hashmapdepth_best_bid.argtypes = [c_void_p]

hashmapdepth_best_ask = lib.hashmapdepth_best_ask
hashmapdepth_best_ask.restype = c_double
hashmapdepth_best_ask.argtypes = [c_void_p]

hashmapdepth_best_bid_qty = lib.hashmapdepth_best_bid_qty
hashmapdepth_best_bid_qty.restype = c_double
hashmapdepth_best_bid_qty.argtypes = [c_void_p]

hashmapdepth_best_ask_qty = lib.hashmapdepth_best_ask_qty
hashmapdepth_best_ask_qty.restype = c_double
hashmapdepth_best_ask_qty.argtypes = [c_void_p]

hashmapdepth_tick_size = lib.hashmapdepth_tick_size
hashmapdepth_tick_size.restype = c_double
hashmapdepth_tick_size.argtypes = [c_void_p]

hashmapdepth_lot_size = lib.hashmapdepth_lot_size
hashmapdepth_lot_size.restype = c_double
hashmapdepth_lot_size.argtypes = [c_void_p]

hashmapdepth_bid_qty_at_tick = lib.hashmapdepth_bid_qty_at_tick
hashmapdepth_bid_qty_at_tick.restype = c_double
hashmapdepth_bid_qty_at_tick.argtypes = [c_void_p, c_int64]

hashmapdepth_ask_qty_at_tick = lib.hashmapdepth_ask_qty_at_tick
hashmapdepth_ask_qty_at_tick.restype = c_double
hashmapdepth_ask_qty_at_tick.argtypes = [c_void_p, c_int64]

hashmapdepth_snapshot = lib.hashmapdepth_snapshot
hashmapdepth_snapshot.restype = c_void_p
hashmapdepth_snapshot.argtypes = [c_void_p, POINTER(c_uint64)]

hashmapdepth_snapshot_free = lib.hashmapdepth_snapshot_free
hashmapdepth_snapshot_free.restype = c_void_p
hashmapdepth_snapshot_free.argtypes = [c_void_p, c_uint64]


class HashMapMarketDepth:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    @property
    def best_bid_tick(self) -> int64:
        """
        Returns the best bid price in ticks.
        """
        return hashmapdepth_best_bid_tick(self.ptr)

    @property
    def best_ask_tick(self) -> int64:
        """
        Returns the best ask price in ticks.
        """
        return hashmapdepth_best_ask_tick(self.ptr)

    @property
    def best_bid(self) -> float64:
        """
        Returns the best bid price.
        """
        return hashmapdepth_best_bid(self.ptr)

    @property
    def best_ask(self) -> float64:
        """
        Returns the best ask price.
        """
        return hashmapdepth_best_ask(self.ptr)

    @property
    def best_bid_qty(self) -> float64:
        """
        Returns the quantity at the best bid price.
        """
        return hashmapdepth_best_bid_qty(self.ptr)

    @property
    def best_ask_qty(self) -> float64:
        """
        Returns the quantity at the best ask price.
        """
        return hashmapdepth_best_ask_qty(self.ptr)

    @property
    def tick_size(self) -> float64:
        """
        Returns the tick size.
        """
        return hashmapdepth_tick_size(self.ptr)

    @property
    def lot_size(self) -> float64:
        """
        Returns the lot size.
        """
        return hashmapdepth_lot_size(self.ptr)

    def bid_qty_at_tick(self, price_tick: int64) -> float64:
        """
        Returns the quantity at the bid market depth for a given price in ticks.

        Args:
            price_tick: Price in ticks.

        Returns:
            The quantity at the specified price.
        """
        return hashmapdepth_bid_qty_at_tick(self.ptr, price_tick)

    def ask_qty_at_tick(self, price_tick: int64) -> float64:
        """
        Returns the quantity at the ask market depth for a given price in ticks.

        Args:
            price_tick: Price in ticks.

        Returns:
            The quantity at the specified price.
        """
        return hashmapdepth_ask_qty_at_tick(self.ptr, price_tick)

    def snapshot(self) -> EVENT_ARRAY:
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = hashmapdepth_snapshot(self.ptr, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            event_dtype
        )

    def snapshot_free(self, arr: EVENT_ARRAY):
        hashmapdepth_snapshot_free(arr.ctypes.data, len(arr))


HashMapMarketDepth_ = jitclass(HashMapMarketDepth)


roivecdepth_best_bid_tick = lib.roivecdepth_best_bid_tick
roivecdepth_best_bid_tick.restype = c_int64
roivecdepth_best_bid_tick.argtypes = [c_void_p]

roivecdepth_best_ask_tick = lib.roivecdepth_best_ask_tick
roivecdepth_best_ask_tick.restype = c_int64
roivecdepth_best_ask_tick.argtypes = [c_void_p]

roivecdepth_best_bid = lib.roivecdepth_best_bid
roivecdepth_best_bid.restype = c_double
roivecdepth_best_bid.argtypes = [c_void_p]

roivecdepth_best_ask = lib.roivecdepth_best_ask
roivecdepth_best_ask.restype = c_double
roivecdepth_best_ask.argtypes = [c_void_p]

roivecdepth_best_bid_qty = lib.roivecdepth_best_bid_qty
roivecdepth_best_bid_qty.restype = c_double
roivecdepth_best_bid_qty.argtypes = [c_void_p]

roivecdepth_best_ask_qty = lib.roivecdepth_best_ask_qty
roivecdepth_best_ask_qty.restype = c_double
roivecdepth_best_ask_qty.argtypes = [c_void_p]

roivecdepth_tick_size = lib.roivecdepth_tick_size
roivecdepth_tick_size.restype = c_double
roivecdepth_tick_size.argtypes = [c_void_p]

roivecdepth_lot_size = lib.roivecdepth_lot_size
roivecdepth_lot_size.restype = c_double
roivecdepth_lot_size.argtypes = [c_void_p]

roivecdepth_bid_qty_at_tick = lib.roivecdepth_bid_qty_at_tick
roivecdepth_bid_qty_at_tick.restype = c_double
roivecdepth_bid_qty_at_tick.argtypes = [c_void_p, c_int64]

roivecdepth_ask_qty_at_tick = lib.roivecdepth_ask_qty_at_tick
roivecdepth_ask_qty_at_tick.restype = c_double
roivecdepth_ask_qty_at_tick.argtypes = [c_void_p, c_int64]

roivecdepth_bid_depth = lib.roivecdepth_bid_depth
roivecdepth_bid_depth.restype = c_void_p
roivecdepth_bid_depth.argtypes = [c_void_p, POINTER(c_uint64)]

roivecdepth_ask_depth = lib.roivecdepth_ask_depth
roivecdepth_ask_depth.restype = c_void_p
roivecdepth_ask_depth.argtypes = [c_void_p, POINTER(c_uint64)]

roivecdepth_roi_lb_tick = lib.roivecdepth_roi_lb_tick
roivecdepth_roi_lb_tick.restype = c_int64
roivecdepth_roi_lb_tick.argtypes = [c_void_p]

roivecdepth_roi_ub_tick = lib.roivecdepth_roi_ub_tick
roivecdepth_roi_ub_tick.restype = c_int64
roivecdepth_roi_ub_tick.argtypes = [c_void_p]


class ROIVectorMarketDepth:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    @property
    def best_bid_tick(self) -> int64:
        """
        Returns the best bid price in ticks.
        """
        return roivecdepth_best_bid_tick(self.ptr)

    @property
    def best_ask_tick(self) -> int64:
        """
        Returns the best ask price in ticks.
        """
        return roivecdepth_best_ask_tick(self.ptr)

    @property
    def best_bid(self) -> float64:
        """
        Returns the best bid price.
        """
        return roivecdepth_best_bid(self.ptr)

    @property
    def best_ask(self) -> float64:
        """
        Returns the best ask price.
        """
        return roivecdepth_best_ask(self.ptr)

    @property
    def best_bid_qty(self) -> float64:
        """
        Returns the quantity at the best bid price.
        """
        return roivecdepth_best_bid(self.ptr)

    @property
    def best_ask_qty(self) -> float64:
        """
        Returns the quantity at the best ask price.
        """
        return roivecdepth_best_ask(self.ptr)

    @property
    def tick_size(self) -> float64:
        """
        Returns the tick size.
        """
        return roivecdepth_tick_size(self.ptr)

    @property
    def lot_size(self) -> float64:
        """
        Returns the lot size.
        """
        return roivecdepth_lot_size(self.ptr)

    def bid_qty_at_tick(self, price_tick: int64) -> float64:
        """
        Returns the quantity at the bid market depth for a given price in ticks.

        Args:
            price_tick: Price in ticks.

        Returns:
            The quantity at the specified price.
        """
        return roivecdepth_bid_qty_at_tick(self.ptr, price_tick)

    def ask_qty_at_tick(self, price_tick: int64) -> float64:
        """
        Returns the quantity at the ask market depth for a given price in ticks.

        Args:
            price_tick: Price in ticks.

        Returns:
            The quantity at the specified price.
        """
        return roivecdepth_ask_qty_at_tick(self.ptr, price_tick)

    @property
    def bid_depth(self) -> np.ndarray[Any, float64]:
        """
        Returns the bid market depth array, which contains the quantity at each price. Its length is
        `ROI upper bound in ticks + 1 - ROI lower bound in ticks`, the array contains the quantities at prices from
        the ROI lower bound to the ROI upper bound. The index is calculated as
        `price in ticks - ROI lower bound in ticks`. Respectively, the price is
        `(index + ROI lower bound in ticks) * tick_size`.
        """
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = roivecdepth_bid_depth(self.ptr, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            float64
        )

    @property
    def ask_depth(self) -> np.ndarray[Any, float64]:
        """
        Returns the ask market depth array, which contains the quantity at each price. Its length is
        `ROI upper bound in ticks + 1 - ROI lower bound in ticks`, the array contains the quantities at prices from
        the ROI lower bound to the ROI upper bound. The index is calculated as
        `price in ticks - ROI lower bound in ticks`. Respectively, the price is
        `(index + ROI lower bound in ticks) * tick_size`.
        """
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = roivecdepth_ask_depth(self.ptr, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            float64
        )

    @property
    def roi_lb_tick(self) -> int64:
        """
        Returns the lower bound of the range of interest, in ticks.
        """
        return roivecdepth_roi_lb_tick(self.ptr)

    @property
    def roi_ub_tick(self) -> int64:
        """
        Returns the upper bound of the range of interest, in ticks.
        """
        return roivecdepth_roi_ub_tick(self.ptr)


ROIVectorMarketDepth_ = jitclass(ROIVectorMarketDepth)

orders_get = lib.orders_get
orders_get.restype = c_void_p
orders_get.argtypes = [c_void_p, c_uint64]

orders_contains = lib.orders_contains
orders_contains.restype = c_bool
orders_contains.argtypes = [c_void_p, c_uint64]

orders_len = lib.orders_len
orders_len.restype = c_uint64
orders_len.argtypes = [c_void_p]

orders_values = lib.orders_values
orders_values.restype = c_void_p
orders_values.argtypes = [c_void_p]

orders_values_next = lib.orders_values_next
orders_values_next.restype = c_void_p
orders_values_next.argtypes = [c_void_p]


class Values:
    ptr: voidptr
    order_ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr
        self.order_ptr = 0

    # def __next__(self) -> Order:
    #     if is_null_ptr(self.ptr):
    #         return None
    #     order_ptr = orders_values_next(self.ptr)
    #     if is_null_ptr(order_ptr):
    #         self.ptr = 0
    #         raise StopIteration
    #     else:
    #         arr = carray(
    #             address_as_void_pointer(order_ptr),
    #             1,
    #             dtype=order_dtype
    #         )
    #         return Order_(arr)

    def next(self) -> Order | None:
        if is_null_ptr(self.ptr):
            return None
        order_ptr = orders_values_next(self.ptr)
        if is_null_ptr(order_ptr):
            self.ptr = 0
            return None
        else:
            arr = carray(
                address_as_void_pointer(order_ptr),
                1,
                dtype=order_dtype
            )
            return Order_(arr)

    def has_next(self) -> bool:
        if is_null_ptr(self.ptr):
            return False
        self.order_ptr = orders_values_next(self.ptr)
        if is_null_ptr(self.order_ptr):
            self.ptr = 0
            return False
        return True

    def get(self) -> Order:
        if is_null_ptr(self.order_ptr):
            raise RuntimeError
        arr = carray(
            address_as_void_pointer(self.order_ptr),
            1,
            dtype=order_dtype
        )
        return Order_(arr)


Values_ = jitclass(Values)


class OrderDict:
    """
    This is a wrapper for the order dictionary. It only supports :func:`get` method, ``in`` operator through
    :func:`__contains__`, and :func:`values` method for iterating over values. Please note the limitations of the values
    iterator.
    """
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    def values(self) -> Values:
        """
        Since `numba` does not support ``__next__`` method in `njit`, you need to manually iterate using the
        ``next``, which returns the next order value if it exists; otherwise, it returns `None`.

        **Example**

        .. code-block:: python

            values = order_dict.values()
            while True:
                order = values.next()
                if order is None:
                    break
                # Do what you need with the order.

        Alternatively, ``has_next`` returns ``True`` if there is a next element and ``False`` otherwise, while also
        moving the iterator to the next element internally. ``get`` method then returns the element moved to by the
        ``has_next`` method. Since ``has_next`` internally moves the iterator, it should not be used solely to check if
        there is a next element.

        **Example**

        .. code-block:: python

            values = order_dict.values()
            while values.has_next():
                order = values.get()
                # Do what you need with the order.

        """
        return Values_(orders_values(self.ptr))

    def get(self, order_id: uint64) -> Order | None:
        """
        Args:
            order_id: Order ID

        Returns:
            Order with the specified order ID; `None` if it does not exist.
        """
        order_ptr = orders_get(self.ptr, order_id)
        if is_null_ptr(order_ptr):
            return None
        else:
            arr = carray(
                address_as_void_pointer(order_ptr),
                1,
                dtype=order_dtype
            )
            return Order_(arr)

    def __len__(self) -> uint64:
        return orders_len(self.ptr)

    def __contains__(self, order_id: uint64) -> bool:
        """
        Args:
            order_id: Order ID
        Returns:
            `True` if the order with the specified order ID exists; otherwise, `False`.
        """
        return orders_contains(self.ptr, order_id)


OrderDict_ = jitclass(OrderDict)

hashmapbt_elapse = lib.hashmapbt_elapse
hashmapbt_elapse.restype = c_int64
hashmapbt_elapse.argtypes = [c_void_p, c_uint64]

hashmapbt_elapse_bt = lib.hashmapbt_elapse_bt
hashmapbt_elapse_bt.restype = c_int64
hashmapbt_elapse_bt.argtypes = [c_void_p, c_uint64]

hashmapbt_hashmapbt_wait_order_response = lib.hashmapbt_wait_order_response
hashmapbt_hashmapbt_wait_order_response.restype = c_int64
hashmapbt_hashmapbt_wait_order_response.argtypes = [c_void_p, c_uint64, c_uint64, c_int64]

hashmapbt_wait_next_feed = lib.hashmapbt_wait_next_feed
hashmapbt_wait_next_feed.restype = c_int64
hashmapbt_wait_next_feed.argtypes = [c_void_p, c_bool, c_int64]

hashmapbt_close = lib.hashmapbt_close
hashmapbt_close.restype = c_int64
hashmapbt_close.argtypes = [c_void_p]

hashmapbt_position = lib.hashmapbt_position
hashmapbt_position.restype = c_double
hashmapbt_position.argtypes = [c_void_p, c_uint64]

hashmapbt_current_timestamp = lib.hashmapbt_current_timestamp
hashmapbt_current_timestamp.restype = c_int64
hashmapbt_current_timestamp.argtypes = [c_void_p]

hashmapbt_depth = lib.hashmapbt_depth
hashmapbt_depth.restype = c_void_p
hashmapbt_depth.argtypes = [c_void_p, c_uint64]

hashmapbt_last_trades = lib.hashmapbt_last_trades
hashmapbt_last_trades.restype = c_void_p
hashmapbt_last_trades.argtypes = [c_void_p, c_uint64, POINTER(c_uint64)]

hashmapbt_num_assets = lib.hashmapbt_num_assets
hashmapbt_num_assets.restype = c_uint64
hashmapbt_num_assets.argtypes = [c_void_p]

hashmapbt_submit_buy_order = lib.hashmapbt_submit_buy_order
hashmapbt_submit_buy_order.restype = c_int64
hashmapbt_submit_buy_order.argtypes = [
    c_void_p,
    c_uint64,
    c_uint64,
    c_double,
    c_double,
    c_uint8,
    c_uint8,
    c_bool
]

hashmapbt_submit_sell_order = lib.hashmapbt_submit_sell_order
hashmapbt_submit_sell_order.restype = c_int64
hashmapbt_submit_sell_order.argtypes = [
    c_void_p,
    c_uint64,
    c_uint64,
    c_double,
    c_double,
    c_uint8,
    c_uint8,
    c_bool
]

hashmapbt_modify = lib.hashmapbt_modify
hashmapbt_modify.restype = c_int64
hashmapbt_modify.argtypes = [c_void_p, c_uint64, c_uint64, c_double, c_double, c_bool]

hashmapbt_cancel = lib.hashmapbt_cancel
hashmapbt_cancel.restype = c_int64
hashmapbt_cancel.argtypes = [c_void_p, c_uint64, c_uint64, c_bool]

hashmapbt_clear_last_trades = lib.hashmapbt_clear_last_trades
hashmapbt_clear_last_trades.restype = c_void_p
hashmapbt_clear_last_trades.argtypes = [c_void_p, c_uint64]

hashmapbt_clear_inactive_orders = lib.hashmapbt_clear_inactive_orders
hashmapbt_clear_inactive_orders.restype = c_void_p
hashmapbt_clear_inactive_orders.argtypes = [c_void_p, c_uint64]

hashmapbt_orders = lib.hashmapbt_orders
hashmapbt_orders.restype = c_void_p
hashmapbt_orders.argtypes = [c_void_p, c_uint64]

hashmapbt_state_values = lib.hashmapbt_state_values
hashmapbt_state_values.restype = c_void_p
hashmapbt_state_values.argtypes = [c_void_p, c_uint64]

hashmapbt_feed_latency = lib.hashmapbt_feed_latency
hashmapbt_feed_latency.restype = c_bool
hashmapbt_feed_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64)]

hashmapbt_order_latency = lib.hashmapbt_order_latency
hashmapbt_order_latency.restype = c_bool
hashmapbt_order_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64), POINTER(c_int64)]

hashmapbt_goto_end = lib.hashmapbt_goto_end
hashmapbt_goto_end.restype = c_int64
hashmapbt_goto_end.argtypes = [c_void_p]


class HashMapMarketDepthBacktest:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    @property
    def current_timestamp(self) -> int64:
        """
        In backtesting, this timestamp reflects the time at which the backtesting is conducted within the provided data.
        """
        return hashmapbt_current_timestamp(self.ptr)

    def depth(self, asset_no: uint64) -> HashMapMarketDepth:
        """
        Args:
            asset_no: Asset number from which the market depth will be retrieved.

        Returns:
            The depth of market of the specific asset.
        """
        return HashMapMarketDepth_(hashmapbt_depth(self.ptr, asset_no))

    @property
    def num_assets(self) -> uint64:
        """
        Returns the number of assets.
        """
        return hashmapbt_num_assets(self.ptr)

    def position(self, asset_no: uint64) -> float64:
        """
        Args:
            asset_no: Asset number from which the position will be retrieved.

        Returns:
            The quantity of the held position.
        """
        return hashmapbt_position(self.ptr, asset_no)

    def state_values(self, asset_no: uint64) -> StateValues:
        """
        Args:
            asset_no: Asset number from which the state values will be retrieved.

        Returns:
            The state’s values.
        """
        ptr = hashmapbt_state_values(self.ptr, asset_no)
        arr = numba.carray(
            address_as_void_pointer(ptr),
            1,
            state_values_dtype
        )
        return StateValues_(arr)

    def last_trades(self, asset_no: uint64) -> EVENT_ARRAY:
        """
        Args:
            asset_no: Asset number from which the trades will be retrieved.

        Returns:
            An array of `Event` representing trades occurring in the market for the specific asset.
        """
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = hashmapbt_last_trades(self.ptr, asset_no, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            event_dtype
        )

    def clear_last_trades(self, asset_no: uint64) -> None:
        """
        Clears the last trades occurring in the market from the buffer for :func:`last_trades`.

        Args:
            asset_no: Asset number at which this command will be executed.
                      If :const:`ALL_ASSETS <hftbacktest.types.ALL_ASSETS>`,
                      all last trades in any assets will be cleared.
        """
        hashmapbt_clear_last_trades(self.ptr, asset_no)

    def orders(self, asset_no: uint64) -> OrderDict:
        """
        Args:
            asset_no: Asset number from which orders will be retrieved.

        Returns:
            An order dictionary where the keys are order IDs and the corresponding values are
            :class:`Order <hftbacktest.order.Order>`.
        """
        return OrderDict_(hashmapbt_orders(self.ptr, asset_no))

    def submit_buy_order(
            self,
            asset_no: uint64,
            order_id: uint64,
            price: float64,
            qty: float64,
            time_in_force: uint8,
            order_type: uint8,
            wait: bool
    ) -> int64:
        """
        Submits a buy order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                      exchange sides.
            price: Order price.
            qty: Quantity to buy.
            time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`GTC <hftbacktest.order.GTC>`
                * :const:`GTX <hftbacktest.order.GTX>`
                * :const:`FOK <hftbacktest.order.FOK>`
                * :const:`IOC <hftbacktest.order.IOC>`

            order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`LIMIT <hftbacktest.order.LIMIT>`
                * :const:`MARKET <hftbacktest.order.MARKET>`

            wait: If `True`, wait until the order placement response is received.

        Returns:
            * `0` when it successfully submits an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return hashmapbt_submit_buy_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def submit_sell_order(
            self,
            asset_no: uint64,
            order_id: uint64,
            price: float64,
            qty: float64,
            time_in_force: uint8,
            order_type: uint8,
            wait: bool
    ) -> int64:
        """
        Submits a sell order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                      exchange sides.
            price: Order price.
            qty: Quantity to sell.
            time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`GTC <hftbacktest.order.GTC>`
                * :const:`GTX <hftbacktest.order.GTX>`
                * :const:`FOK <hftbacktest.order.FOK>`
                * :const:`IOC <hftbacktest.order.IOC>`

            order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`LIMIT <hftbacktest.order.LIMIT>`
                * :const:`MARKET <hftbacktest.order.MARKET>`

            wait: If `True`, wait until the order placement response is received.

        Returns:
            * `0` when it successfully submits an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return hashmapbt_submit_sell_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def modify(self, asset_no: uint64, order_id: uint64, price: float, qty: float, wait: bool) -> int64:
        """
        Modifies the specified order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: Order ID to modify.
            price: Order price.
            qty: Order quantity.
            wait: If `True`, wait until the order cancel response is received.

        Returns:
            * `0` when it successfully modifies an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return hashmapbt_modify(self.ptr, asset_no, order_id, price, qty, wait)

    def cancel(self, asset_no: uint64, order_id: uint64, wait: bool) -> int64:
        """
        Cancels the specified order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: Order ID to cancel.
            wait: If `True`, wait until the order cancel response is received.

        Returns:
            * `0` when it successfully cancels an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return hashmapbt_cancel(self.ptr, asset_no, order_id, wait)

    def clear_inactive_orders(self, asset_no: uint64) -> None:
        """
        Clears inactive orders from the local order dictionary whose status is neither
        :const:`NEW <hftbacktest.order.NEW>` nor :const:`PARTIALLY_FILLED <hftbacktest.order.PARTIALLY_FILLED>`.

        Args:
            asset_no: Asset number at which this command will be executed.
                      If :const:`ALL_ASSETS <hftbacktest.types.ALL_ASSETS>`,
                      all inactive orders in any assets will be cleared.
        """
        hashmapbt_clear_inactive_orders(self.ptr, asset_no)

    def wait_order_response(self, asset_no: uint64, order_id: uint64, timeout: int64) -> int64:
        """
        Waits for the response of the order with the given order ID until timeout.

        Args:
            asset_no: Asset number where an order with `order_id` exists.
            order_id: Order ID to wait for the response.
            timeout: Timeout for waiting for the order response. Nanoseconds is the default unit. However, unit should
                     be the same as the data’s timestamp unit.

        Returns:
            * `0` when it receives an order response for the specified order ID of the specified asset number, or
              reaches the timeout.
            * `1` when it reaches the end of the data.
            * Otherwise, an error occurred.
        """
        return hashmapbt_hashmapbt_wait_order_response(self.ptr, asset_no, order_id, timeout)

    def wait_next_feed(self, include_order_resp: bool, timeout: int64) -> int64:
        """
        Waits until the next feed is received, or until timeout.

        Args:
            include_order_resp: If set to `True`, it will return when any order response is received, in addition to the
                                next feed.
            timeout: Timeout for waiting for the next feed or an order response. Nanoseconds is the default unit.
                     However, unit should be the same as the data’s timestamp unit.

        Returns:
            * `0` when it reaches the timeout.
            * `1` when it reaches the end of the data.
            * `2` when it receives a market feed.
            * `3` when it receives an order response if `include_order_resp` is `True`.
            * Otherwise, an error occurred.
        """
        return hashmapbt_wait_next_feed(self.ptr, include_order_resp, timeout)

    def elapse(self, duration: uint64) -> int64:
        """
        Elapses the specified duration.

        Args:
            duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                      data’s timestamp unit.

        Returns:
            * `0` when it successfully elapses the given duration.
            * `1` when it reaches the end of the data.
            * Otherwise, an error occurred.
        """
        return hashmapbt_elapse(self.ptr, duration)

    def elapse_bt(self, duration: int64) -> int64:
        """
        Elapses time only in backtesting. In live mode, it is ignored. (Supported only in the Rust implementation)

        The `elapse` method exclusively manages time during backtesting, meaning that factors such as computing time are
        not properly accounted for. So, this method can be utilized to simulate such processing times.

        Args:
            duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                      data’s timestamp unit.

        Returns:
            * `0` when it successfully elapses the given duration.
            * `1` when it reaches the end of the data.
            * Otherwise, an error occurred.
        """
        return hashmapbt_elapse_bt(self.ptr, duration)

    def close(self) -> int64:
        """
        Closes this backtester or bot.

        Returns:
            * `0` when it successfully closes the bot.
            * Otherwise, an error occurred.
        """
        return hashmapbt_close(self.ptr)

    def feed_latency(self, asset_no: uint64) -> Tuple[int64, int64] | None:
        """
        Args:
            asset_no: Asset number from which the last feed latency will be retrieved.

        Returns:
            The last feed’s exchange timestamp and local receipt timestamp if a feed has been received; otherwise,
            returns `None`.
        """
        exch_ts = int64(0)
        local_ts = int64(0)
        exch_ts_ptr = ptr_from_val(exch_ts)
        local_ts_ptr = ptr_from_val(local_ts)
        if hashmapbt_feed_latency(self.ptr, asset_no, exch_ts_ptr, local_ts_ptr):
            return val_from_ptr(exch_ts_ptr), val_from_ptr(local_ts_ptr)
        return None

    def order_latency(self, asset_no: uint64) -> Tuple[int64, int64, int64] | None:
        """
        Args:
            asset_no: Asset number from which the last order latency will be retrieved.

        Returns:
            The last order’s request timestamp, exchange timestamp, and response receipt timestamp if there has been an
            order submission; otherwise, returns `None`.
        """
        req_ts = int64(0)
        exch_ts = int64(0)
        resp_ts = int64(0)
        req_ts_ptr = ptr_from_val(req_ts)
        exch_ts_ptr = ptr_from_val(exch_ts)
        resp_ts_ptr = ptr_from_val(resp_ts)
        if hashmapbt_order_latency(self.ptr, asset_no, req_ts_ptr, exch_ts_ptr, resp_ts_ptr):
            return val_from_ptr(req_ts_ptr), val_from_ptr(exch_ts_ptr), val_from_ptr(resp_ts_ptr)
        return None

    def _goto_end(self) -> int64:
        return hashmapbt_goto_end(self.ptr)


HashMapMarketDepthBacktest_ = jitclass(HashMapMarketDepthBacktest)


roivecbt_elapse = lib.roivecbt_elapse
roivecbt_elapse.restype = c_int64
roivecbt_elapse.argtypes = [c_void_p, c_uint64]

roivecbt_elapse_bt = lib.roivecbt_elapse_bt
roivecbt_elapse_bt.restype = c_int64
roivecbt_elapse_bt.argtypes = [c_void_p, c_uint64]

roivecbt_roivecbt_wait_order_response = lib.roivecbt_wait_order_response
roivecbt_roivecbt_wait_order_response.restype = c_int64
roivecbt_roivecbt_wait_order_response.argtypes = [c_void_p, c_uint64, c_uint64, c_int64]

roivecbt_wait_next_feed = lib.roivecbt_wait_next_feed
roivecbt_wait_next_feed.restype = c_int64
roivecbt_wait_next_feed.argtypes = [c_void_p, c_bool, c_int64]

roivecbt_close = lib.roivecbt_close
roivecbt_close.restype = c_int64
roivecbt_close.argtypes = [c_void_p]

roivecbt_position = lib.roivecbt_position
roivecbt_position.restype = c_double
roivecbt_position.argtypes = [c_void_p, c_uint64]

roivecbt_current_timestamp = lib.roivecbt_current_timestamp
roivecbt_current_timestamp.restype = c_int64
roivecbt_current_timestamp.argtypes = [c_void_p]

roivecbt_depth = lib.roivecbt_depth
roivecbt_depth.restype = c_void_p
roivecbt_depth.argtypes = [c_void_p, c_uint64]

roivecbt_last_trades = lib.roivecbt_last_trades
roivecbt_last_trades.restype = c_void_p
roivecbt_last_trades.argtypes = [c_void_p, c_uint64, POINTER(c_uint64)]

roivecbt_num_assets = lib.roivecbt_num_assets
roivecbt_num_assets.restype = c_uint64
roivecbt_num_assets.argtypes = [c_void_p]

roivecbt_submit_buy_order = lib.roivecbt_submit_buy_order
roivecbt_submit_buy_order.restype = c_int64
roivecbt_submit_buy_order.argtypes = [
    c_void_p,
    c_uint64,
    c_uint64,
    c_double,
    c_double,
    c_uint8,
    c_uint8,
    c_bool
]

roivecbt_submit_sell_order = lib.roivecbt_submit_sell_order
roivecbt_submit_sell_order.restype = c_int64
roivecbt_submit_sell_order.argtypes = [
    c_void_p,
    c_uint64,
    c_uint64,
    c_double,
    c_double,
    c_uint8,
    c_uint8,
    c_bool
]

roivecbt_modify = lib.roivecbt_modify
roivecbt_modify.restype = c_int64
roivecbt_modify.argtypes = [c_void_p, c_uint64, c_uint64, c_double, c_double, c_bool]

roivecbt_cancel = lib.roivecbt_cancel
roivecbt_cancel.restype = c_int64
roivecbt_cancel.argtypes = [c_void_p, c_uint64, c_uint64, c_bool]

roivecbt_clear_last_trades = lib.roivecbt_clear_last_trades
roivecbt_clear_last_trades.restype = c_void_p
roivecbt_clear_last_trades.argtypes = [c_void_p, c_uint64]

roivecbt_clear_inactive_orders = lib.roivecbt_clear_inactive_orders
roivecbt_clear_inactive_orders.restype = c_void_p
roivecbt_clear_inactive_orders.argtypes = [c_void_p, c_uint64]

roivecbt_orders = lib.roivecbt_orders
roivecbt_orders.restype = c_void_p
roivecbt_orders.argtypes = [c_void_p, c_uint64]

roivecbt_state_values = lib.roivecbt_state_values
roivecbt_state_values.restype = c_void_p
roivecbt_state_values.argtypes = [c_void_p, c_uint64]

roivecbt_feed_latency = lib.roivecbt_feed_latency
roivecbt_feed_latency.restype = c_bool
roivecbt_feed_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64)]

roivecbt_order_latency = lib.roivecbt_order_latency
roivecbt_order_latency.restype = c_bool
roivecbt_order_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64), POINTER(c_int64)]


class ROIVectorMarketDepthBacktest:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    @property
    def current_timestamp(self) -> int64:
        """
        In backtesting, this timestamp reflects the time at which the backtesting is conducted within the provided data.
        """
        return roivecbt_current_timestamp(self.ptr)

    def depth(self, asset_no: uint64) -> ROIVectorMarketDepth:
        """
        Args:
            asset_no: Asset number from which the market depth will be retrieved.

        Returns:
            The depth of market of the specific asset.
        """
        return ROIVectorMarketDepth_(roivecbt_depth(self.ptr, asset_no))

    @property
    def num_assets(self) -> uint64:
        """
        Returns the number of assets.
        """
        return roivecbt_num_assets(self.ptr)

    def position(self, asset_no: uint64) -> float64:
        """
        Args:
            asset_no: Asset number from which the position will be retrieved.

        Returns:
            The quantity of the held position.
        """
        return roivecbt_position(self.ptr, asset_no)

    def state_values(self, asset_no: uint64) -> StateValues:
        """
        Args:
            asset_no: Asset number from which the state values will be retrieved.

        Returns:
            The state’s values.
        """
        ptr = roivecbt_state_values(self.ptr, asset_no)
        arr = numba.carray(
            address_as_void_pointer(ptr),
            1,
            state_values_dtype
        )
        return StateValues_(arr)

    def last_trades(self, asset_no: uint64) -> EVENT_ARRAY:
        """
        Args:
            asset_no: Asset number from which the trades will be retrieved.

        Returns:
            An array of `Event` representing trades occurring in the market for the specific asset.
        """
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = roivecbt_last_trades(self.ptr, asset_no, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            event_dtype
        )

    def clear_last_trades(self, asset_no: uint64) -> None:
        """
        Clears the last trades occurring in the market from the buffer for :func:`last_trades`.

        Args:
            asset_no: Asset number at which this command will be executed.
                      If :const:`ALL_ASSETS <hftbacktest.types.ALL_ASSETS>`,
                      all last trades in any assets will be cleared.
        """
        roivecbt_clear_last_trades(self.ptr, asset_no)

    def orders(self, asset_no: uint64) -> OrderDict:
        """
        Args:
            asset_no: Asset number from which orders will be retrieved.

        Returns:
            An order dictionary where the keys are order IDs and the corresponding values are
            :class:`Order <hftbacktest.order.Order>`.
        """
        return OrderDict_(roivecbt_orders(self.ptr, asset_no))

    def submit_buy_order(
            self,
            asset_no: uint64,
            order_id: uint64,
            price: float64,
            qty: float64,
            time_in_force: uint8,
            order_type: uint8,
            wait: bool
    ) -> int64:
        """
        Submits a buy order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                      exchange sides.
            price: Order price.
            qty: Quantity to buy.
            time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`GTC <hftbacktest.order.GTC>`
                * :const:`GTX <hftbacktest.order.GTX>`
                * :const:`FOK <hftbacktest.order.FOK>`
                * :const:`IOC <hftbacktest.order.IOC>`

            order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`LIMIT <hftbacktest.order.LIMIT>`
                * :const:`MARKET <hftbacktest.order.MARKET>`

            wait: If `True`, wait until the order placement response is received.

        Returns:
            * `0` when it successfully submits an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return roivecbt_submit_buy_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def submit_sell_order(
            self,
            asset_no: uint64,
            order_id: uint64,
            price: float64,
            qty: float64,
            time_in_force: uint8,
            order_type: uint8,
            wait: bool
    ) -> int64:
        """
        Submits a sell order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                      exchange sides.
            price: Order price.
            qty: Quantity to sell.
            time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`GTC <hftbacktest.order.GTC>`
                * :const:`GTX <hftbacktest.order.GTX>`
                * :const:`FOK <hftbacktest.order.FOK>`
                * :const:`IOC <hftbacktest.order.IOC>`

            order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                * :const:`LIMIT <hftbacktest.order.LIMIT>`
                * :const:`MARKET <hftbacktest.order.MARKET>`

            wait: If `True`, wait until the order placement response is received.

        Returns:
            * `0` when it successfully submits an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return roivecbt_submit_sell_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def modify(self, asset_no: uint64, order_id: uint64, price: float, qty: float, wait: bool) -> int64:
        """
        Modifies the specified order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: Order ID to modify.
            price: Order price.
            qty: Order quantity.
            wait: If `True`, wait until the order cancel response is received.

        Returns:
            * `0` when it successfully modifies an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return roivecbt_modify(self.ptr, asset_no, order_id, price, qty, wait)

    def cancel(self, asset_no: uint64, order_id: uint64, wait: bool) -> int64:
        """
        Cancels the specified order.

        Args:
            asset_no: Asset number at which this command will be executed.
            order_id: Order ID to cancel.
            wait: If `True`, wait until the order cancel response is received.

        Returns:
            * `0` when it successfully cancels an order.
            * `1` when it reaches the end of the data, if `wait` is `True`.
            * Otherwise, an error occurred.
        """
        return roivecbt_cancel(self.ptr, asset_no, order_id, wait)

    def clear_inactive_orders(self, asset_no: uint64) -> None:
        """
        Clears inactive orders from the local order dictionary whose status is neither
        :const:`NEW <hftbacktest.order.NEW>` nor :const:`PARTIALLY_FILLED <hftbacktest.order.PARTIALLY_FILLED>`.

        Args:
            asset_no: Asset number at which this command will be executed.
                      If :const:`ALL_ASSETS <hftbacktest.types.ALL_ASSETS>`,
                      all inactive orders in any assets will be cleared.
        """
        roivecbt_clear_inactive_orders(self.ptr, asset_no)

    def wait_order_response(self, asset_no: uint64, order_id: uint64, timeout: int64) -> int64:
        """
        Waits for the response of the order with the given order ID until timeout.

        Args:
            asset_no: Asset number where an order with `order_id` exists.
            order_id: Order ID to wait for the response.
            timeout: Timeout for waiting for the order response. Nanoseconds is the default unit. However, unit should
                     be the same as the data’s timestamp unit.

        Returns:
            * `0` when it receives an order response for the specified order ID of the specified asset number, or
              reaches the timeout.
            * `1` when it reaches the end of the data.
            * Otherwise, an error occurred.
        """
        return roivecbt_roivecbt_wait_order_response(self.ptr, asset_no, order_id, timeout)

    def wait_next_feed(self, include_order_resp: bool, timeout: int64) -> int64:
        """
        Waits until the next feed is received, or until timeout.

        Args:
            include_order_resp: If set to `True`, it will return when any order response is received, in addition to the
                                next feed.
            timeout: Timeout for waiting for the next feed or an order response. Nanoseconds is the default unit.
                     However, unit should be the same as the data’s timestamp unit.

        Returns:
            * `0` when it reaches the timeout.
            * `1` when it reaches the end of the data.
            * `2` when it receives a market feed.
            * `3` when it receives an order response if `include_order_resp` is `True`.
            * Otherwise, an error occurred.
        """
        return roivecbt_wait_next_feed(self.ptr, include_order_resp, timeout)

    def elapse(self, duration: uint64) -> int64:
        """
        Elapses the specified duration.

        Args:
            duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                      data’s timestamp unit.

        Returns:
            * `0` when it successfully elapses the given duration.
            * `1` when it reaches the end of the data.
            * Otherwise, an error occurred.
        """
        return roivecbt_elapse(self.ptr, duration)

    def elapse_bt(self, duration: int64) -> int64:
        """
        Elapses time only in backtesting. In live mode, it is ignored. (Supported only in the Rust implementation)

        The `elapse` method exclusively manages time during backtesting, meaning that factors such as computing time are
        not properly accounted for. So, this method can be utilized to simulate such processing times.

        Args:
            duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                      data’s timestamp unit.

        Returns:
            * `0` when it successfully elapses the given duration.
            * `1` when it reaches the end of the data.
            * Otherwise, an error occurred.
        """
        return roivecbt_elapse_bt(self.ptr, duration)

    def close(self) -> int64:
        """
        Closes this backtester or bot.

        Returns:
            * `0` when it successfully closes the bot.
            * Otherwise, an error occurred.
        """
        return roivecbt_close(self.ptr)

    def feed_latency(self, asset_no: uint64) -> Tuple[int64, int64] | None:
        """
        Args:
            asset_no: Asset number from which the last feed latency will be retrieved.

        Returns:
            The last feed’s exchange timestamp and local receipt timestamp if a feed has been received; otherwise,
            returns `None`.
        """
        exch_ts = int64(0)
        local_ts = int64(0)
        exch_ts_ptr = ptr_from_val(exch_ts)
        local_ts_ptr = ptr_from_val(local_ts)
        if roivecbt_feed_latency(self.ptr, asset_no, exch_ts_ptr, local_ts_ptr):
            return val_from_ptr(exch_ts_ptr), val_from_ptr(local_ts_ptr)
        return None

    def order_latency(self, asset_no: uint64) -> Tuple[int64, int64, int64] | None:
        """
        Args:
            asset_no: Asset number from which the last order latency will be retrieved.

        Returns:
            The last order’s request timestamp, exchange timestamp, and response receipt timestamp if there has been an
            order submission; otherwise, returns `None`.
        """
        req_ts = int64(0)
        exch_ts = int64(0)
        resp_ts = int64(0)
        req_ts_ptr = ptr_from_val(req_ts)
        exch_ts_ptr = ptr_from_val(exch_ts)
        resp_ts_ptr = ptr_from_val(resp_ts)
        if roivecbt_order_latency(self.ptr, asset_no, req_ts_ptr, exch_ts_ptr, resp_ts_ptr):
            return val_from_ptr(req_ts_ptr), val_from_ptr(exch_ts_ptr), val_from_ptr(resp_ts_ptr)
        return None


ROIVectorMarketDepthBacktest_ = jitclass(ROIVectorMarketDepthBacktest)


fusemarketdepth_new = lib.fusemarketdepth_new
fusemarketdepth_new.restype = c_void_p
fusemarketdepth_new.argtypes = [c_double, c_double]

fusemarketdepth_free = lib.fusemarketdepth_free
fusemarketdepth_free.restype = c_void_p
fusemarketdepth_free.argtypes = [c_void_p]

fusemarketdepth_process_event = lib.fusemarketdepth_process_event
fusemarketdepth_process_event.restype = c_bool
fusemarketdepth_process_event.argtypes = [c_void_p, c_void_p, c_bool]

fusemarketdepth_fused_events = lib.fusemarketdepth_fused_events
fusemarketdepth_fused_events.restype = c_void_p
fusemarketdepth_fused_events.argtypes = [c_void_p, POINTER(c_uint64)]


class FuseMarketDepth:
    """
    This combines the real-time Level-1 book ticker stream with the conflated Level-2 depth stream to produce the
    most frequent and granular depth events possible.

    Args:
        tick_size: tick size for the asset being processed.
        lot_size: lot size for the asset being processed.
    """

    ptr: voidptr
    buf: from_dtype(event_dtype)[:]

    def __init__(self, tick_size: float64, lot_size: float64):
        self.ptr = fusemarketdepth_new(tick_size, lot_size)
        self.buf = np.zeros(1, event_dtype)

    # def __del__(self):
    #     fusemarketdepth_free(self.ptr)

    def close(self) -> None:
        """
        Releases resources associated with this `FuseMarketDepth` instance.

        This method must be called to free the underlying memory allocated by the native implementation.
        """
        fusemarketdepth_free(self.ptr)

    def process_event(self, ev: EVENT_ARRAY, index: uint64, add: bool) -> None:
        """
        Processes a market event at the given index.

        Args:
            ev: The array of events to process.
            index: The index of the event in the array to process.
            add: If `True`, the event is added to the fused events.
                 If `False`, the event is used to update market depth for future processing, but is not included in the
                 fused output.
        """
        ev_ptr = ev.ctypes.data + 64 * index
        ok = fusemarketdepth_process_event(self.ptr, ev_ptr, add)
        if not ok:
            raise ValueError

    @property
    def fused_events(self) -> EVENT_ARRAY:
        """
        Returns the array of fused events generated so far.
        """
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = fusemarketdepth_fused_events(self.ptr, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            event_dtype
        )

FuseMarketDepth_ = jitclass(FuseMarketDepth)


if LIVE_FEATURE:
    hashmaplive_elapse = lib.hashmaplive_elapse
    hashmaplive_elapse.restype = c_int64
    hashmaplive_elapse.argtypes = [c_void_p, c_uint64]

    hashmaplive_elapse_bt = lib.hashmaplive_elapse_bt
    hashmaplive_elapse_bt.restype = c_int64
    hashmaplive_elapse_bt.argtypes = [c_void_p, c_uint64]

    hashmaplive_hashmaplive_wait_order_response = lib.hashmaplive_wait_order_response
    hashmaplive_hashmaplive_wait_order_response.restype = c_int64
    hashmaplive_hashmaplive_wait_order_response.argtypes = [c_void_p, c_uint64, c_uint64, c_int64]

    hashmaplive_wait_next_feed = lib.hashmaplive_wait_next_feed
    hashmaplive_wait_next_feed.restype = c_int64
    hashmaplive_wait_next_feed.argtypes = [c_void_p, c_bool, c_int64]

    hashmaplive_close = lib.hashmaplive_close
    hashmaplive_close.restype = c_int64
    hashmaplive_close.argtypes = [c_void_p]

    hashmaplive_position = lib.hashmaplive_position
    hashmaplive_position.restype = c_double
    hashmaplive_position.argtypes = [c_void_p, c_uint64]

    hashmaplive_current_timestamp = lib.hashmaplive_current_timestamp
    hashmaplive_current_timestamp.restype = c_int64
    hashmaplive_current_timestamp.argtypes = [c_void_p]

    hashmaplive_depth = lib.hashmaplive_depth
    hashmaplive_depth.restype = c_void_p
    hashmaplive_depth.argtypes = [c_void_p, c_uint64]

    hashmaplive_last_trades = lib.hashmaplive_last_trades
    hashmaplive_last_trades.restype = c_void_p
    hashmaplive_last_trades.argtypes = [c_void_p, c_uint64, POINTER(c_uint64)]

    hashmaplive_num_assets = lib.hashmaplive_num_assets
    hashmaplive_num_assets.restype = c_uint64
    hashmaplive_num_assets.argtypes = [c_void_p]

    hashmaplive_submit_buy_order = lib.hashmaplive_submit_buy_order
    hashmaplive_submit_buy_order.restype = c_int64
    hashmaplive_submit_buy_order.argtypes = [
        c_void_p,
        c_uint64,
        c_uint64,
        c_double,
        c_double,
        c_uint8,
        c_uint8,
        c_bool
    ]

    hashmaplive_submit_sell_order = lib.hashmaplive_submit_sell_order
    hashmaplive_submit_sell_order.restype = c_int64
    hashmaplive_submit_sell_order.argtypes = [
        c_void_p,
        c_uint64,
        c_uint64,
        c_double,
        c_double,
        c_uint8,
        c_uint8,
        c_bool
    ]

    hashmaplive_modify = lib.hashmaplive_modify
    hashmaplive_modify.restype = c_int64
    hashmaplive_modify.argtypes = [c_void_p, c_uint64, c_uint64, c_double, c_double, c_bool]

    hashmaplive_cancel = lib.hashmaplive_cancel
    hashmaplive_cancel.restype = c_int64
    hashmaplive_cancel.argtypes = [c_void_p, c_uint64, c_uint64, c_bool]

    hashmaplive_clear_last_trades = lib.hashmaplive_clear_last_trades
    hashmaplive_clear_last_trades.restype = c_void_p
    hashmaplive_clear_last_trades.argtypes = [c_void_p, c_uint64]

    hashmaplive_clear_inactive_orders = lib.hashmaplive_clear_inactive_orders
    hashmaplive_clear_inactive_orders.restype = c_void_p
    hashmaplive_clear_inactive_orders.argtypes = [c_void_p, c_uint64]

    hashmaplive_orders = lib.hashmaplive_orders
    hashmaplive_orders.restype = c_void_p
    hashmaplive_orders.argtypes = [c_void_p, c_uint64]

    hashmaplive_state_values = lib.hashmaplive_state_values
    hashmaplive_state_values.restype = c_void_p
    hashmaplive_state_values.argtypes = [c_void_p, c_uint64]

    hashmaplive_feed_latency = lib.hashmaplive_feed_latency
    hashmaplive_feed_latency.restype = c_bool
    hashmaplive_feed_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64)]

    hashmaplive_order_latency = lib.hashmaplive_order_latency
    hashmaplive_order_latency.restype = c_bool
    hashmaplive_order_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64), POINTER(c_int64)]


    class HashMapMarketDepthLiveBot:
        ptr: voidptr

        def __init__(self, ptr: voidptr):
            self.ptr = ptr

        @property
        def current_timestamp(self) -> int64:
            """
            In LiveBoting, this timestamp reflects the time at which the LiveBoting is conducted within the provided data.
            """
            return hashmaplive_current_timestamp(self.ptr)

        def depth(self, asset_no: uint64) -> HashMapMarketDepth:
            """
            Args:
                asset_no: Asset number from which the market depth will be retrieved.

            Returns:
                The depth of market of the specific asset.
            """
            return HashMapMarketDepth_(hashmaplive_depth(self.ptr, asset_no))

        @property
        def num_assets(self) -> uint64:
            """
            Returns the number of assets.
            """
            return hashmaplive_num_assets(self.ptr)

        def position(self, asset_no: uint64) -> float64:
            """
            Args:
                asset_no: Asset number from which the position will be retrieved.

            Returns:
                The quantity of the held position.
            """
            return hashmaplive_position(self.ptr, asset_no)

        def state_values(self, asset_no: uint64) -> StateValues:
            """
            Args:
                asset_no: Asset number from which the state values will be retrieved.

            Returns:
                The state’s values.
            """
            ptr = hashmaplive_state_values(self.ptr, asset_no)
            arr = numba.carray(
                address_as_void_pointer(ptr),
                1,
                state_values_dtype
            )
            return StateValues_(arr)

        def last_trades(self, asset_no: uint64) -> EVENT_ARRAY:
            """
            Args:
                asset_no: Asset number from which the trades will be retrieved.

            Returns:
                An array of `Event` representing trades occurring in the market for the specific asset.
            """
            length = uint64(0)
            len_ptr = ptr_from_val(length)
            ptr = hashmaplive_last_trades(self.ptr, asset_no, len_ptr)
            return numba.carray(
                address_as_void_pointer(ptr),
                val_from_ptr(len_ptr),
                event_dtype
            )

        def clear_last_trades(self, asset_no: uint64) -> None:
            """
            Clears the last trades occurring in the market from the buffer for :func:`last_trades`.

            Args:
                asset_no: Asset number at which this command will be executed.
                          If :const:`ALL_ASSETS <hftLiveBot.types.ALL_ASSETS>`,
                          all last trades in any assets will be cleared.
            """
            hashmaplive_clear_last_trades(self.ptr, asset_no)

        def orders(self, asset_no: uint64) -> OrderDict:
            """
            Args:
                asset_no: Asset number from which orders will be retrieved.

            Returns:
                An order dictionary where the keys are order IDs and the corresponding values are
                :class:`Order <hftLiveBot.order.Order>`.
            """
            return OrderDict_(hashmaplive_orders(self.ptr, asset_no))

        def submit_buy_order(
                self,
                asset_no: uint64,
                order_id: uint64,
                price: float64,
                qty: float64,
                time_in_force: uint8,
                order_type: uint8,
                wait: bool
        ) -> int64:
            """
            Submits a buy order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                          exchange sides.
                price: Order price.
                qty: Quantity to buy.
                time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`GTC <hftLiveBot.order.GTC>`
                    * :const:`GTX <hftLiveBot.order.GTX>`
                    * :const:`FOK <hftLiveBot.order.FOK>`
                    * :const:`IOC <hftLiveBot.order.IOC>`

                order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`LIMIT <hftLiveBot.order.LIMIT>`
                    * :const:`MARKET <hftLiveBot.order.MARKET>`

                wait: If `True`, wait until the order placement response is received.

            Returns:
                * `0` when it successfully submits an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return hashmaplive_submit_buy_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

        def submit_sell_order(
                self,
                asset_no: uint64,
                order_id: uint64,
                price: float64,
                qty: float64,
                time_in_force: uint8,
                order_type: uint8,
                wait: bool
        ) -> int64:
            """
            Submits a sell order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                          exchange sides.
                price: Order price.
                qty: Quantity to sell.
                time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`GTC <hftLiveBot.order.GTC>`
                    * :const:`GTX <hftLiveBot.order.GTX>`
                    * :const:`FOK <hftLiveBot.order.FOK>`
                    * :const:`IOC <hftLiveBot.order.IOC>`

                order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`LIMIT <hftLiveBot.order.LIMIT>`
                    * :const:`MARKET <hftLiveBot.order.MARKET>`

                wait: If `True`, wait until the order placement response is received.

            Returns:
                * `0` when it successfully submits an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return hashmaplive_submit_sell_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

        def modify(self, asset_no: uint64, order_id: uint64, price: float, qty: float, wait: bool) -> int64:
            """
            Modifies the specified order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: Order ID to modify.
                price: Order price.
                qty: Order quantity.
                wait: If `True`, wait until the order cancel response is received.

            Returns:
                * `0` when it successfully modifies an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return hashmaplive_modify(self.ptr, asset_no, order_id, price, qty, wait)

        def cancel(self, asset_no: uint64, order_id: uint64, wait: bool) -> int64:
            """
            Cancels the specified order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: Order ID to cancel.
                wait: If `True`, wait until the order cancel response is received.

            Returns:
                * `0` when it successfully cancels an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return hashmaplive_cancel(self.ptr, asset_no, order_id, wait)

        def clear_inactive_orders(self, asset_no: uint64) -> None:
            """
            Clears inactive orders from the local order dictionary whose status is neither
            :const:`NEW <hftLiveBot.order.NEW>` nor :const:`PARTIALLY_FILLED <hftLiveBot.order.PARTIALLY_FILLED>`.

            Args:
                asset_no: Asset number at which this command will be executed.
                          If :const:`ALL_ASSETS <hftLiveBot.types.ALL_ASSETS>`,
                          all inactive orders in any assets will be cleared.
            """
            hashmaplive_clear_inactive_orders(self.ptr, asset_no)

        def wait_order_response(self, asset_no: uint64, order_id: uint64, timeout: int64) -> int64:
            """
            Waits for the response of the order with the given order ID until timeout.

            Args:
                asset_no: Asset number where an order with `order_id` exists.
                order_id: Order ID to wait for the response.
                timeout: Timeout for waiting for the order response. Nanoseconds is the default unit. However, unit should
                         be the same as the data’s timestamp unit.

            Returns:
                * `0` when it receives an order response for the specified order ID of the specified asset number, or
                  reaches the timeout.
                * `1` when it reaches the end of the data.
                * Otherwise, an error occurred.
            """
            return hashmaplive_hashmaplive_wait_order_response(self.ptr, asset_no, order_id, timeout)

        def wait_next_feed(self, include_order_resp: bool, timeout: int64) -> int64:
            """
            Waits until the next feed is received, or until timeout.

            Args:
                include_order_resp: If set to `True`, it will return when any order response is received, in addition to
                                    the next feed.
                timeout: Timeout for waiting for the next feed or an order response. Nanoseconds is the default unit.
                         However, unit should be the same as the data’s timestamp unit.

            Returns:
                * `0` when it reaches the timeout.
                * `1` when it reaches the end of the data.
                * `2` when it receives a market feed.
                * `3` when it receives an order response if `include_order_resp` is `True`.
                * Otherwise, an error occurred.
            """
            return hashmaplive_wait_next_feed(self.ptr, include_order_resp, timeout)

        def elapse(self, duration: uint64) -> int64:
            """
            Elapses the specified duration.

            Args:
                duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                          data’s timestamp unit.

            Returns:
                * `0` when it successfully elapses the given duration.
                * `1` when it reaches the end of the data.
                * Otherwise, an error occurred.
            """
            return hashmaplive_elapse(self.ptr, duration)

        def elapse_bt(self, duration: int64) -> int64:
            """
            Elapses time only in LiveBoting. In live mode, it is ignored. (Supported only in the Rust implementation)

            The `elapse` method exclusively manages time during LiveBoting, meaning that factors such as computing time are
            not properly accounted for. So, this method can be utilized to simulate such processing times.

            Args:
                duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                          data’s timestamp unit.

            Returns:
                * `0` when it successfully elapses the given duration.
                * `1` when it reaches the end of the data.
                * Otherwise, an error occurred.
            """
            return hashmaplive_elapse_bt(self.ptr, duration)

        def close(self) -> int64:
            """
            Closes this LiveBoter or bot.

            Returns:
                * `0` when it successfully closes the bot.
                * Otherwise, an error occurred.
            """
            return hashmaplive_close(self.ptr)

        def feed_latency(self, asset_no: uint64) -> Tuple[int64, int64] | None:
            """
            Args:
                asset_no: Asset number from which the last feed latency will be retrieved.

            Returns:
                The last feed’s exchange timestamp and local receipt timestamp if a feed has been received; otherwise,
                returns `None`.
            """
            exch_ts = int64(0)
            local_ts = int64(0)
            exch_ts_ptr = ptr_from_val(exch_ts)
            local_ts_ptr = ptr_from_val(local_ts)
            if hashmaplive_feed_latency(self.ptr, asset_no, exch_ts_ptr, local_ts_ptr):
                return val_from_ptr(exch_ts_ptr), val_from_ptr(local_ts_ptr)
            return None

        def order_latency(self, asset_no: uint64) -> Tuple[int64, int64, int64] | None:
            """
            Args:
                asset_no: Asset number from which the last order latency will be retrieved.

            Returns:
                The last order’s request timestamp, exchange timestamp, and response receipt timestamp if there has been an
                order submission; otherwise, returns `None`.
            """
            req_ts = int64(0)
            exch_ts = int64(0)
            resp_ts = int64(0)
            req_ts_ptr = ptr_from_val(req_ts)
            exch_ts_ptr = ptr_from_val(exch_ts)
            resp_ts_ptr = ptr_from_val(resp_ts)
            if hashmaplive_order_latency(self.ptr, asset_no, req_ts_ptr, exch_ts_ptr, resp_ts_ptr):
                return val_from_ptr(req_ts_ptr), val_from_ptr(exch_ts_ptr), val_from_ptr(resp_ts_ptr)
            return None

        def _goto_end(self) -> int64:
            return hashmaplive_goto_end(self.ptr)


    HashMapMarketDepthLiveBot_ = jitclass(HashMapMarketDepthLiveBot)


    roiveclive_elapse = lib.roiveclive_elapse
    roiveclive_elapse.restype = c_int64
    roiveclive_elapse.argtypes = [c_void_p, c_uint64]

    roiveclive_elapse_bt = lib.roiveclive_elapse_bt
    roiveclive_elapse_bt.restype = c_int64
    roiveclive_elapse_bt.argtypes = [c_void_p, c_uint64]

    roiveclive_roiveclive_wait_order_response = lib.roiveclive_wait_order_response
    roiveclive_roiveclive_wait_order_response.restype = c_int64
    roiveclive_roiveclive_wait_order_response.argtypes = [c_void_p, c_uint64, c_uint64, c_int64]

    roiveclive_wait_next_feed = lib.roiveclive_wait_next_feed
    roiveclive_wait_next_feed.restype = c_int64
    roiveclive_wait_next_feed.argtypes = [c_void_p, c_bool, c_int64]

    roiveclive_close = lib.roiveclive_close
    roiveclive_close.restype = c_int64
    roiveclive_close.argtypes = [c_void_p]

    roiveclive_position = lib.roiveclive_position
    roiveclive_position.restype = c_double
    roiveclive_position.argtypes = [c_void_p, c_uint64]

    roiveclive_current_timestamp = lib.roiveclive_current_timestamp
    roiveclive_current_timestamp.restype = c_int64
    roiveclive_current_timestamp.argtypes = [c_void_p]

    roiveclive_depth = lib.roiveclive_depth
    roiveclive_depth.restype = c_void_p
    roiveclive_depth.argtypes = [c_void_p, c_uint64]

    roiveclive_last_trades = lib.roiveclive_last_trades
    roiveclive_last_trades.restype = c_void_p
    roiveclive_last_trades.argtypes = [c_void_p, c_uint64, POINTER(c_uint64)]

    roiveclive_num_assets = lib.roiveclive_num_assets
    roiveclive_num_assets.restype = c_uint64
    roiveclive_num_assets.argtypes = [c_void_p]

    roiveclive_submit_buy_order = lib.roiveclive_submit_buy_order
    roiveclive_submit_buy_order.restype = c_int64
    roiveclive_submit_buy_order.argtypes = [
        c_void_p,
        c_uint64,
        c_uint64,
        c_double,
        c_double,
        c_uint8,
        c_uint8,
        c_bool
    ]

    roiveclive_submit_sell_order = lib.roiveclive_submit_sell_order
    roiveclive_submit_sell_order.restype = c_int64
    roiveclive_submit_sell_order.argtypes = [
        c_void_p,
        c_uint64,
        c_uint64,
        c_double,
        c_double,
        c_uint8,
        c_uint8,
        c_bool
    ]

    roiveclive_modify = lib.roiveclive_modify
    roiveclive_modify.restype = c_int64
    roiveclive_modify.argtypes = [c_void_p, c_uint64, c_uint64, c_double, c_double, c_bool]

    roiveclive_cancel = lib.roiveclive_cancel
    roiveclive_cancel.restype = c_int64
    roiveclive_cancel.argtypes = [c_void_p, c_uint64, c_uint64, c_bool]

    roiveclive_clear_last_trades = lib.roiveclive_clear_last_trades
    roiveclive_clear_last_trades.restype = c_void_p
    roiveclive_clear_last_trades.argtypes = [c_void_p, c_uint64]

    roiveclive_clear_inactive_orders = lib.roiveclive_clear_inactive_orders
    roiveclive_clear_inactive_orders.restype = c_void_p
    roiveclive_clear_inactive_orders.argtypes = [c_void_p, c_uint64]

    roiveclive_orders = lib.roiveclive_orders
    roiveclive_orders.restype = c_void_p
    roiveclive_orders.argtypes = [c_void_p, c_uint64]

    roiveclive_state_values = lib.roiveclive_state_values
    roiveclive_state_values.restype = c_void_p
    roiveclive_state_values.argtypes = [c_void_p, c_uint64]

    roiveclive_feed_latency = lib.roiveclive_feed_latency
    roiveclive_feed_latency.restype = c_bool
    roiveclive_feed_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64)]

    roiveclive_order_latency = lib.roiveclive_order_latency
    roiveclive_order_latency.restype = c_bool
    roiveclive_order_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64), POINTER(c_int64)]


    class ROIVectorMarketDepthLiveBot:
        ptr: voidptr

        def __init__(self, ptr: voidptr):
            self.ptr = ptr

        @property
        def current_timestamp(self) -> int64:
            """
            In LiveBoting, this timestamp reflects the time at which the LiveBoting is conducted within the provided data.
            """
            return roiveclive_current_timestamp(self.ptr)

        def depth(self, asset_no: uint64) -> ROIVectorMarketDepth:
            """
            Args:
                asset_no: Asset number from which the market depth will be retrieved.

            Returns:
                The depth of market of the specific asset.
            """
            return ROIVectorMarketDepth_(roiveclive_depth(self.ptr, asset_no))

        @property
        def num_assets(self) -> uint64:
            """
            Returns the number of assets.
            """
            return roiveclive_num_assets(self.ptr)

        def position(self, asset_no: uint64) -> float64:
            """
            Args:
                asset_no: Asset number from which the position will be retrieved.

            Returns:
                The quantity of the held position.
            """
            return roiveclive_position(self.ptr, asset_no)

        def state_values(self, asset_no: uint64) -> StateValues:
            """
            Args:
                asset_no: Asset number from which the state values will be retrieved.

            Returns:
                The state’s values.
            """
            ptr = roiveclive_state_values(self.ptr, asset_no)
            arr = numba.carray(
                address_as_void_pointer(ptr),
                1,
                state_values_dtype
            )
            return StateValues_(arr)

        def last_trades(self, asset_no: uint64) -> EVENT_ARRAY:
            """
            Args:
                asset_no: Asset number from which the trades will be retrieved.

            Returns:
                An array of `Event` representing trades occurring in the market for the specific asset.
            """
            length = uint64(0)
            len_ptr = ptr_from_val(length)
            ptr = roiveclive_last_trades(self.ptr, asset_no, len_ptr)
            return numba.carray(
                address_as_void_pointer(ptr),
                val_from_ptr(len_ptr),
                event_dtype
            )

        def clear_last_trades(self, asset_no: uint64) -> None:
            """
            Clears the last trades occurring in the market from the buffer for :func:`last_trades`.

            Args:
                asset_no: Asset number at which this command will be executed.
                          If :const:`ALL_ASSETS <hftLiveBot.types.ALL_ASSETS>`,
                          all last trades in any assets will be cleared.
            """
            roiveclive_clear_last_trades(self.ptr, asset_no)

        def orders(self, asset_no: uint64) -> OrderDict:
            """
            Args:
                asset_no: Asset number from which orders will be retrieved.

            Returns:
                An order dictionary where the keys are order IDs and the corresponding values are
                :class:`Order <hftLiveBot.order.Order>`.
            """
            return OrderDict_(roiveclive_orders(self.ptr, asset_no))

        def submit_buy_order(
                self,
                asset_no: uint64,
                order_id: uint64,
                price: float64,
                qty: float64,
                time_in_force: uint8,
                order_type: uint8,
                wait: bool
        ) -> int64:
            """
            Submits a buy order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                          exchange sides.
                price: Order price.
                qty: Quantity to buy.
                time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`GTC <hftLiveBot.order.GTC>`
                    * :const:`GTX <hftLiveBot.order.GTX>`
                    * :const:`FOK <hftLiveBot.order.FOK>`
                    * :const:`IOC <hftLiveBot.order.IOC>`

                order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`LIMIT <hftLiveBot.order.LIMIT>`
                    * :const:`MARKET <hftLiveBot.order.MARKET>`

                wait: If `True`, wait until the order placement response is received.

            Returns:
                * `0` when it successfully submits an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return roiveclive_submit_buy_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

        def submit_sell_order(
                self,
                asset_no: uint64,
                order_id: uint64,
                price: float64,
                qty: float64,
                time_in_force: uint8,
                order_type: uint8,
                wait: bool
        ) -> int64:
            """
            Submits a sell order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                          exchange sides.
                price: Order price.
                qty: Quantity to sell.
                time_in_force: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`GTC <hftLiveBot.order.GTC>`
                    * :const:`GTX <hftLiveBot.order.GTX>`
                    * :const:`FOK <hftLiveBot.order.FOK>`
                    * :const:`IOC <hftLiveBot.order.IOC>`

                order_type: Available options vary depending on the exchange model. See to the exchange model for details.

                    * :const:`LIMIT <hftLiveBot.order.LIMIT>`
                    * :const:`MARKET <hftLiveBot.order.MARKET>`

                wait: If `True`, wait until the order placement response is received.

            Returns:
                * `0` when it successfully submits an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return roiveclive_submit_sell_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

        def modify(self, asset_no: uint64, order_id: uint64, price: float, qty: float, wait: bool) -> int64:
            """
            Modifies the specified order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: Order ID to modify.
                price: Order price.
                qty: Order quantity.
                wait: If `True`, wait until the order cancel response is received.

            Returns:
                * `0` when it successfully modifies an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return roiveclive_modify(self.ptr, asset_no, order_id, price, qty, wait)

        def cancel(self, asset_no: uint64, order_id: uint64, wait: bool) -> int64:
            """
            Cancels the specified order.

            Args:
                asset_no: Asset number at which this command will be executed.
                order_id: Order ID to cancel.
                wait: If `True`, wait until the order cancel response is received.

            Returns:
                * `0` when it successfully cancels an order.
                * `1` when it reaches the end of the data, if `wait` is `True`.
                * Otherwise, an error occurred.
            """
            return roiveclive_cancel(self.ptr, asset_no, order_id, wait)

        def clear_inactive_orders(self, asset_no: uint64) -> None:
            """
            Clears inactive orders from the local order dictionary whose status is neither
            :const:`NEW <hftLiveBot.order.NEW>` nor :const:`PARTIALLY_FILLED <hftLiveBot.order.PARTIALLY_FILLED>`.

            Args:
                asset_no: Asset number at which this command will be executed.
                          If :const:`ALL_ASSETS <hftLiveBot.types.ALL_ASSETS>`,
                          all inactive orders in any assets will be cleared.
            """
            roiveclive_clear_inactive_orders(self.ptr, asset_no)

        def wait_order_response(self, asset_no: uint64, order_id: uint64, timeout: int64) -> int64:
            """
            Waits for the response of the order with the given order ID until timeout.

            Args:
                asset_no: Asset number where an order with `order_id` exists.
                order_id: Order ID to wait for the response.
                timeout: Timeout for waiting for the order response. Nanoseconds is the default unit. However, unit should
                         be the same as the data’s timestamp unit.

            Returns:
                * `0` when it receives an order response for the specified order ID of the specified asset number, or
                  reaches the timeout.
                * `1` when it reaches the end of the data.
                * Otherwise, an error occurred.
            """
            return roiveclive_roiveclive_wait_order_response(self.ptr, asset_no, order_id, timeout)

        def wait_next_feed(self, include_order_resp: bool, timeout: int64) -> int64:
            """
            Waits until the next feed is received, or until timeout.

            Args:
                include_order_resp: If set to `True`, it will return when any order response is received, in addition to
                                    the next feed.
                timeout: Timeout for waiting for the next feed or an order response. Nanoseconds is the default unit.
                         However, unit should be the same as the data’s timestamp unit.

            Returns:
                * `0` when it reaches the timeout.
                * `1` when it reaches the end of the data.
                * `2` when it receives a market feed.
                * `3` when it receives an order response if `include_order_resp` is `True`.
                * Otherwise, an error occurred.
            """
            return roiveclive_wait_next_feed(self.ptr, include_order_resp, timeout)

        def elapse(self, duration: uint64) -> int64:
            """
            Elapses the specified duration.

            Args:
                duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                          data’s timestamp unit.

            Returns:
                * `0` when it successfully elapses the given duration.
                * `1` when it reaches the end of the data.
                * Otherwise, an error occurred.
            """
            return roiveclive_elapse(self.ptr, duration)

        def elapse_bt(self, duration: int64) -> int64:
            """
            Elapses time only in LiveBoting. In live mode, it is ignored. (Supported only in the Rust implementation)

            The `elapse` method exclusively manages time during LiveBoting, meaning that factors such as computing time are
            not properly accounted for. So, this method can be utilized to simulate such processing times.

            Args:
                duration: Duration to elapse. Nanoseconds is the default unit. However, unit should be the same as the
                          data’s timestamp unit.

            Returns:
                * `0` when it successfully elapses the given duration.
                * `1` when it reaches the end of the data.
                * Otherwise, an error occurred.
            """
            return roiveclive_elapse_bt(self.ptr, duration)

        def close(self) -> int64:
            """
            Closes this LiveBoter or bot.

            Returns:
                * `0` when it successfully closes the bot.
                * Otherwise, an error occurred.
            """
            return roiveclive_close(self.ptr)

        def feed_latency(self, asset_no: uint64) -> Tuple[int64, int64] | None:
            """
            Args:
                asset_no: Asset number from which the last feed latency will be retrieved.

            Returns:
                The last feed’s exchange timestamp and local receipt timestamp if a feed has been received; otherwise,
                returns `None`.
            """
            exch_ts = int64(0)
            local_ts = int64(0)
            exch_ts_ptr = ptr_from_val(exch_ts)
            local_ts_ptr = ptr_from_val(local_ts)
            if roiveclive_feed_latency(self.ptr, asset_no, exch_ts_ptr, local_ts_ptr):
                return val_from_ptr(exch_ts_ptr), val_from_ptr(local_ts_ptr)
            return None

        def order_latency(self, asset_no: uint64) -> Tuple[int64, int64, int64] | None:
            """
            Args:
                asset_no: Asset number from which the last order latency will be retrieved.

            Returns:
                The last order’s request timestamp, exchange timestamp, and response receipt timestamp if there has been an
                order submission; otherwise, returns `None`.
            """
            req_ts = int64(0)
            exch_ts = int64(0)
            resp_ts = int64(0)
            req_ts_ptr = ptr_from_val(req_ts)
            exch_ts_ptr = ptr_from_val(exch_ts)
            resp_ts_ptr = ptr_from_val(resp_ts)
            if roiveclive_order_latency(self.ptr, asset_no, req_ts_ptr, exch_ts_ptr, resp_ts_ptr):
                return val_from_ptr(req_ts_ptr), val_from_ptr(exch_ts_ptr), val_from_ptr(resp_ts_ptr)
            return None


    ROIVectorMarketDepthLiveBot_ = jitclass(ROIVectorMarketDepthLiveBot)
