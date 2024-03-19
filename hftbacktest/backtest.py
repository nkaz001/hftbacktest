from numba import int64, boolean, float64

from .order import LIMIT, BUY, SELL
from .reader import WAIT_ORDER_RESPONSE_NONE, COL_LOCAL_TIMESTAMP, UNTIL_END_OF_DATA


class SingleAssetHftBacktest:
    r"""
    Single Asset HftBacktest.

    .. warning::
        This has to be constructed by :func:`.HftBacktest`.

    Args:
        local: Local processor.
        exch: Exchange processor.
    """

    def __init__(self, local, exch):
        self.local = local
        self.exch = exch

        #: Whether a backtest has finished.
        self.run = True
        #: Current timestamp
        self.current_timestamp = self.start_timestamp

    @property
    def start_timestamp(self):
        # fixme: deprecated.
        # it returns the timestamp of the first row of the data that is currently processed.
        for i in range(len(self.local.data)):
            timestamp = self.local.data[i, COL_LOCAL_TIMESTAMP]
            if timestamp > 0:
                return timestamp
        for i in range(len(self.local.next_data)):
            timestamp = self.local.next_data[i, COL_LOCAL_TIMESTAMP]
            if timestamp > 0:
                return timestamp
        return 0

    @property
    def last_timestamp(self):
        # fixme: deprecated.
        # it returns the timestamp of the last row of the data that is currently processed.
        for i in range(len(self.local.data) - 1, -1, -1):
            timestamp = self.local.data[i, COL_LOCAL_TIMESTAMP]
            if timestamp > 0:
                return timestamp
        for i in range(len(self.local.next_data) - 1, -1, -1):
            timestamp = self.local.next_data[i, COL_LOCAL_TIMESTAMP]
            if timestamp > 0:
                return timestamp
        return 0

    @property
    def position(self):
        """
        Current position.
        """
        #
        return self.local.state.position

    @property
    def balance(self):
        """
        Current balance..
        """
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
        """
        Orders dictionary.
        """
        return self.local.orders

    @property
    def tick_size(self):
        """
        Tick size
        """
        return self.local.depth.tick_size

    @property
    def lot_size(self):
        """
        Lot size
        """
        return self.local.depth.lot_size

    @property
    def high_ask_tick(self):
        """
        The highest ask price in the market depth in tick.
        """
        return self.local.depth.high_ask_tick

    @property
    def low_bid_tick(self):
        """
        The lowest bid price in the market depth in tick.
        """
        return self.local.depth.low_bid_tick

    @property
    def best_bid_tick(self):
        """
        The best bid price in tick.
        """
        return self.local.depth.best_bid_tick

    @property
    def best_ask_tick(self):
        """
        The best ask price in tick.
        """
        return self.local.depth.best_ask_tick

    @property
    def best_bid(self):
        """
        The best bid price.
        """
        return self.best_bid_tick * self.tick_size

    @property
    def best_ask(self):
        """
        The best ask price.
        """
        return self.best_ask_tick * self.tick_size

    @property
    def bid_depth(self):
        """
        Bid market depth.
        """
        return self.local.depth.bid_depth

    @property
    def ask_depth(self):
        """
        Ask market depth.
        """
        return self.local.depth.ask_depth

    @property
    def mid(self):
        """
        Mid-price of BBO.
        """
        return (self.best_bid + self.best_ask) / 2.0

    @property
    def equity(self):
        """
        Current equity value.
        """
        return self.local.state.equity(self.mid)

    @property
    def last_trade(self):
        """
        Last market trade. If ``None``, no last market trade.
        """
        if self.local.trade_len > 0:
            return self.last_trades[self.local.trade_len - 1]
        else:
            return None

    @property
    def last_trades(self):
        """
        An array of last market trades.
        """
        return self.local.last_trades[:self.local.trade_len]

    @property
    def local_timestamp(self):
        return self.current_timestamp

    def submit_buy_order(
            self,
            order_id: int64,
            price: float64,
            qty: float64,
            time_in_force: int64,
            order_type: int64 = LIMIT,
            wait: boolean = False
    ):
        r"""
        Places a buy order.

        Args:
            order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                      exchange sides.
            price: Order price.
            qty: Quantity to buy.
            time_in_force: Available Time-In-Force options vary depending on the exchange model. See to the exchange
                           model for details.

                           - ``GTX``: Post-only
                           - ``GTC``: Good 'till Cancel
                           - ``FOK``: Fill or Kill
                           - ``IOC``: Immediate or Cancel
            order_type: Currently, only ``LIMIT`` is supported. To simulate a ``MARKET`` order, set the price very high.
            wait: If ``True``, wait until the order placement response is received.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        self.local.submit_order(order_id, BUY, price, qty, order_type, time_in_force, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def submit_sell_order(
            self,
            order_id: int64,
            price: float64,
            qty: float64,
            time_in_force: int64,
            order_type: int64 = LIMIT,
            wait: boolean = False
    ):
        r"""
        Places a sell order.

        Args:
            order_id: The unique order ID; there should not be any existing order with the same ID on both local and
                      exchange sides.
            price: Order price.
            qty: Quantity to sell.
            time_in_force: Available Time-In-Force options vary depending on the exchange model. See to the exchange
                           model for details.

                           - ``GTX``: Post-only
                           - ``GTC``: Good 'till Cancel
                           - ``FOK``: Fill or Kill
                           - ``IOC``: Immediate or Cancel
            order_type: Currently, only ``LIMIT`` is supported. To simulate a ``MARKET`` order, set the price very low.
            wait: If ``True``, wait until the order placement response is received.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        self.local.submit_order(order_id, SELL, price, qty, order_type, time_in_force, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def modify(self, order_id: int64, price: float64, qty: float64, wait: boolean = False):
        r"""
        Modify the specified order.

        - If the adjusted total quantity(leaves_qty + executed_qty) is less than or equal to
          the quantity already executed, the order will be considered expired. Be aware that this adjustment doesn't
          affect the remaining quantity in the market, it only changes the total quantity.
        - Modified orders will be reordered in the match queue.

        Args:
            order_id: Order ID to modify.
            price: Order price.
            qty: Quantity to sell.
            wait: If ``True``, wait until the order placement response is received.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        self.local.modify_order(order_id, price, qty, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def cancel(self, order_id: int64, wait: boolean = False):
        r"""
        Cancel the specified order.

        Args:
            order_id: Order ID to cancel.
            wait: If ``True``, wait until the order placement response is received.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        self.local.cancel(order_id, self.current_timestamp)

        if wait:
            return self.goto(UNTIL_END_OF_DATA, wait_order_response=order_id)
        return True

    def wait_order_response(self, order_id: int64, timeout: int64 = -1):
        r"""
        Wait for the specified order response by order ID.

        Args:
            order_id: The order ID to wait for.
            timeout: Maximum waiting time; The default value of `-1` indicates no timeout.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
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

    def wait_next_feed(self, include_order_resp: bool, timeout: int = -1):
        """
        Waits until the next feed is received.

        Args:
            include_order_resp: Whether to include order responses in the feed to wait for.
            timeout: Maximum waiting time; The default value of `-1` indicates no timeout.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        if include_order_resp:
            timestamp = self.local.next_timestamp()
        else:
            timestamp = self.local._next_data_timestamp()
        if timestamp == -1:
            return False
        if timeout >= 0:
            timestamp = min(timestamp, self.current_timestamp + timeout)
        return self.goto(timestamp)

    def clear_inactive_orders(self):
        r"""
        Clear inactive(``CANCELED``, ``FILLED``, ``EXPIRED``, or ``REJECTED``) orders from the local ``orders``
        dictionary.
        """
        self.local.clear_inactive_orders()

    def clear_last_trades(self):
        r"""
        Clears the last trades(market trades) from the buffer.
        """
        self.local.clear_last_trades()

    def get_user_data(self, event: int64):
        r"""
        Retrieve custom user event data.

        Args:
            event: Event identifier. Refer to the data documentation for details on incorporating custom user data with
                   the market feed data.

        Returns:
            The latest event data for the specified event.
        """
        return self.local.get_user_data(event)

    def elapse(self, duration: float64):
        r"""
        Elapses the specified duration.

        Args:
            duration: Duration to elapse. Unit should be the same as the feed data's timestamp unit.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        return self.goto(self.current_timestamp + duration)

    def goto(self, timestamp: float64, wait_order_response: int64 = WAIT_ORDER_RESPONSE_NONE):
        r"""
        Goes to a specified timestamp.

        This method moves to the specified timestamp, updating the backtesting state to match the corresponding time. If
        ``wait_order_response`` is provided, the method will stop and return when it receives the response for the
        specified order.

        Args:
            timestamp: The target timestamp to go to. The timestamp unit should be the same as the feed data's timestamp
                       unit.
            wait_order_response: Order ID to wait for; the default value is ``WAIT_ORDER_RESPONSE_NONE``, which means
                                 not waiting for any order response.

        Returns:
            ``True`` if the method reaches the specified timestamp within the data. If the end of the data is reached
            before the specified timestamp, it returns ``False``.
        """
        found_order_resp_timestamp = False
        while True:
            # Select which side will be processed next.
            next_local_timestamp = self.local.next_timestamp()
            next_exch_timestamp = self.exch.next_timestamp()

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

    def reset(
            self,
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
    ):
        self.local.reader = local_reader
        self.exch.reader = exch_reader

        self.local.reset(
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
        self.exch.reset(
            start_position,
            start_balance,
            start_fee,
            maker_fee,
            taker_fee,
            tick_size,
            lot_size,
            snapshot
        )
        self.current_timestamp = self.start_timestamp
        self.run = True
