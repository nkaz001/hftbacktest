from typing import Tuple

import numpy as np
from numba.experimental import jitclass
from numba import int64, float64, boolean

spec = [
    ('bids', float64[:, :, :]),
    ('asks', float64[:, :, :]),
    ('bid_delete_lvs', float64[:, :]),
    ('ask_delete_lvs', float64[:, :]),
    ('ob', int64),
    ('update_bid_lv', int64),
    ('update_ask_lv', int64),
    ('init', boolean),
]


@jitclass(spec)
class DiffOrderBookSnapshot:
    def __init__(self, num_levels: int) -> None:
        # [num_levels, {current, next}, {price, qty, is_updated}]
        self.bids = np.zeros((num_levels, 2, 3), float64)
        self.asks = np.zeros((num_levels, 2, 3), float64)
        # [num_levels, {price, delete_type}]
        self.bid_delete_lvs = np.zeros((num_levels, 2), float64)
        self.ask_delete_lvs = np.zeros((num_levels, 2), float64)
        self.ob = 0
        self.update_bid_lv = 0
        self.update_ask_lv = 0
        self.init = True

    @property
    def next_ob(self) -> int:
        return 0 if self.ob == 1 else 1

    def begin_update(self) -> None:
        self.update_bid_lv = 0
        self.update_ask_lv = 0

    def done_update(self) -> Tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
        if self.update_bid_lv != len(self.bids) or self.update_ask_lv != len(self.asks):
            raise ValueError

        if self.init:
            self.init = False
            # Switches the order book
            self.ob = self.next_ob
            return (
                self.bids[:, self.ob, :],
                self.asks[:, self.ob, :],
                self.bid_delete_lvs[:0, :],
                self.ask_delete_lvs[:0, :]
            )

        prev_ob = self.ob
        next_ob = self.next_ob

        # Processes bid-side
        bids_delete_lv = 0
        next_lv = 0
        for prev_lv in range(len(self.bids)):
            prev_px = self.bids[prev_lv, prev_ob, 0]
            is_processed = False
            while next_lv < len(self.bids):
                next_px = self.bids[next_lv, next_ob, 0]
                if prev_px == next_px:
                    # If the quantity also matches the previous value, sets no update.
                    if self.bids[prev_lv, prev_ob, 1] == self.bids[next_lv, next_ob, 1]:
                        self.bids[next_lv, next_ob, 2] = 0  # No update
                    next_lv += 1
                    is_processed = True
                    break
                elif prev_px > next_px:
                    # There is no price level in the next.
                    self.bid_delete_lvs[bids_delete_lv, 0] = prev_px
                    self.bid_delete_lvs[bids_delete_lv, 1] = 0  # In-the-book delete
                    bids_delete_lv += 1
                    is_processed = True
                    break
                else:  # if prev_price < next_price
                    next_lv += 1
            if not is_processed:
                self.bid_delete_lvs[bids_delete_lv, 0] = prev_px
                self.bid_delete_lvs[bids_delete_lv, 1] = 1  # Out-of-the-book delete
                bids_delete_lv += 1

        # Processes ask-side
        asks_delete_lv = 0
        next_lv = 0
        for prev_lv in range(len(self.asks)):
            prev_px = self.asks[prev_lv, prev_ob, 0]
            is_processed = False
            while next_lv < len(self.asks):
                next_px = self.asks[next_lv, next_ob, 0]
                if prev_px == next_px:
                    # If the quantity also matches the previous value, sets no update.
                    if self.asks[prev_lv, prev_ob, 1] == self.asks[next_lv, next_ob, 1]:
                        self.asks[next_lv, next_ob, 2] = 0  # No update
                    is_processed = True
                    next_lv += 1
                    break
                elif prev_px < next_px:
                    # There is no price level in the next.
                    self.ask_delete_lvs[asks_delete_lv, 0] = prev_px
                    self.ask_delete_lvs[asks_delete_lv, 1] = 0  # In-the-book delete
                    asks_delete_lv += 1
                    is_processed = True
                    break
                else:  # if prev_price > next_price
                    next_lv += 1
            if not is_processed:
                self.ask_delete_lvs[asks_delete_lv, 0] = prev_px
                self.ask_delete_lvs[asks_delete_lv, 1] = 1  # Out-of-the-book delete
                asks_delete_lv += 1

        # Switches the order book
        self.ob = next_ob
        return (
            self.bids[:, self.ob, :],
            self.asks[:, self.ob, :],
            self.bid_delete_lvs[:bids_delete_lv, :],
            self.ask_delete_lvs[:asks_delete_lv, :]
        )

    def update_bid(self, price: float, qty: float) -> None:
        self.bids[self.update_bid_lv, self.next_ob, 0] = price
        self.bids[self.update_bid_lv, self.next_ob, 1] = qty
        self.bids[self.update_bid_lv, self.next_ob, 2] = 1  # Update
        self.update_bid_lv += 1

    def update_ask(self, price: float, qty: float) -> None:
        self.asks[self.update_ask_lv, self.next_ob, 0] = price
        self.asks[self.update_ask_lv, self.next_ob, 1] = qty
        self.asks[self.update_ask_lv, self.next_ob, 2] = 1  # Update
        self.update_ask_lv += 1
