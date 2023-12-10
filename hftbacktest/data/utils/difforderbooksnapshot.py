import sys
from typing import Tuple

import numpy as np
from numba.experimental import jitclass
from numba import int64, float64, boolean

spec = [
    ('num_levels', int64),
    ('curr_bids', float64[:, :]),
    ('curr_asks', float64[:, :]),
    ('prev_bids', float64[:, :]),
    ('prev_asks', float64[:, :]),
    ('bid_delete_lvs', float64[:, :]),
    ('ask_delete_lvs', float64[:, :]),
    ('curr_bid_lv', int64),
    ('curr_ask_lv', int64),
    ('prev_bid_lv', int64),
    ('prev_ask_lv', int64),
    ('init', boolean),
    ('tick_size', float64),
    ('lot_size', float64)
]

UNCHANGED = 0
CHANGED = 1
INSERTED = 2
IN_THE_BOOK_DELETION = 0
OUT_OF_BOOK_DELETION_BELOW = 1
OUT_OF_BOOK_DELETION_ABOVE = 2


@jitclass(spec)
class DiffOrderBookSnapshot:
    def __init__(self, levels: int, tick_size: float, lot_size: float) -> None:
        self.num_levels = levels
        # [num_levels, {price, qty, is_updated}]
        self.curr_bids = np.zeros((levels, 3), float64)
        self.curr_asks = np.zeros((levels, 3), float64)
        self.prev_bids = np.zeros((levels, 3), float64)
        self.prev_asks = np.zeros((levels, 3), float64)
        # [num_levels, {price, delete_type}]
        self.bid_delete_lvs = np.zeros((levels, 2), float64)
        self.ask_delete_lvs = np.zeros((levels, 2), float64)
        self.curr_bid_lv = 0
        self.curr_ask_lv = 0
        self.prev_bid_lv = 0
        self.prev_ask_lv = 0
        self.init = True
        self.tick_size = tick_size
        self.lot_size = lot_size

    def snapshot(
            self,
            bid_px: np.ndarray,
            bid_qty: np.ndarray,
            ask_px: np.ndarray,
            ask_qty: np.ndarray
    ) -> Tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
        if len(bid_px) != len(bid_qty) or len(ask_px) != len(ask_qty):
            raise ValueError

        self.curr_bid_lv, self.prev_bid_lv = 0, self.curr_bid_lv
        self.curr_ask_lv, self.prev_ask_lv = 0, self.curr_ask_lv
        # Swap the snapshots.
        self.curr_bids, self.prev_bids = self.prev_bids, self.curr_bids
        self.curr_asks, self.prev_asks = self.prev_asks, self.curr_asks

        self.curr_bid_lv = 0
        for i in range(len(bid_px)):
            self.curr_bids[self.curr_bid_lv, 0] = bid_px[i]
            self.curr_bids[self.curr_bid_lv, 1] = bid_qty[i]
            self.curr_bids[self.curr_bid_lv, 2] = CHANGED
            self.curr_bid_lv += 1

        self.curr_ask_lv = 0
        for i in range(len(ask_px)):
            self.curr_asks[self.curr_ask_lv, 0] = ask_px[i]
            self.curr_asks[self.curr_ask_lv, 1] = ask_qty[i]
            self.curr_asks[self.curr_ask_lv, 2] = CHANGED
            self.curr_ask_lv += 1

        if self.init:
            self.init = False
            return (
                self.curr_bids,
                self.curr_asks,
                self.bid_delete_lvs[:0],
                self.ask_delete_lvs[:0]
            )

        # Processes the bid snapshot
        curr_high_px_tick = 0
        curr_low_px_tick = sys.maxsize
        for curr_lv in range(self.curr_bid_lv):
            curr_px_tick = round(self.curr_bids[curr_lv, 0] / self.tick_size)
            if curr_px_tick < curr_low_px_tick:
                curr_low_px_tick = curr_px_tick
            if curr_px_tick > curr_high_px_tick:
                curr_high_px_tick = curr_px_tick

        # Checks which levels are deleted.
        bids_delete_lv = 0
        for prev_lv in range(self.prev_bid_lv):
            prev_px_tick = round(self.prev_bids[prev_lv, 0] / self.tick_size)
            if prev_px_tick < curr_low_px_tick:
                self.bid_delete_lvs[bids_delete_lv, 0] = prev_px_tick * self.tick_size
                self.bid_delete_lvs[bids_delete_lv, 1] = OUT_OF_BOOK_DELETION_BELOW
                bids_delete_lv += 1
            elif prev_px_tick > curr_high_px_tick:
                self.bid_delete_lvs[bids_delete_lv, 0] = prev_px_tick * self.tick_size
                self.bid_delete_lvs[bids_delete_lv, 1] = OUT_OF_BOOK_DELETION_ABOVE
                bids_delete_lv += 1
            else:
                exist = False
                for curr_lv in range(self.curr_bid_lv):
                    curr_px_tick = round(self.curr_bids[curr_lv, 0] / self.tick_size)
                    if prev_px_tick == curr_px_tick:
                        exist = True
                        break
                if not exist:
                    self.bid_delete_lvs[bids_delete_lv, 0] = prev_px_tick * self.tick_size
                    self.bid_delete_lvs[bids_delete_lv, 1] = IN_THE_BOOK_DELETION
                    bids_delete_lv += 1

        # Sets the update flag.
        for curr_lv in range(self.curr_bid_lv):
            curr_px_tick = round(self.curr_bids[curr_lv, 0] / self.tick_size)
            exist = False
            for prev_lv in range(self.prev_bid_lv):
                prev_px_tick = round(self.prev_bids[prev_lv, 0] / self.tick_size)
                if prev_px_tick == curr_px_tick:
                    exist = True
                    if (round(self.curr_bids[curr_lv, 1] / self.lot_size) ==
                            round(self.prev_bids[prev_lv, 1] / self.lot_size)):
                        self.curr_bids[curr_lv, 2] = UNCHANGED
                    break
            if not exist:
                self.curr_bids[curr_lv, 2] = INSERTED

        # Processes the ask snapshot
        curr_high_px_tick = 0
        curr_low_px_tick = sys.maxsize
        for curr_lv in range(self.curr_ask_lv):
            curr_px_tick = round(self.curr_asks[curr_lv, 0] / self.tick_size)
            if curr_px_tick < curr_low_px_tick:
                curr_low_px_tick = curr_px_tick
            if curr_px_tick > curr_high_px_tick:
                curr_high_px_tick = curr_px_tick

        # Checks which levels are deleted.
        asks_delete_lv = 0
        for prev_lv in range(self.prev_ask_lv):
            prev_px_tick = round(self.prev_asks[prev_lv, 0] / self.tick_size)
            if prev_px_tick < curr_low_px_tick:
                self.ask_delete_lvs[asks_delete_lv, 0] = prev_px_tick * self.tick_size
                self.ask_delete_lvs[asks_delete_lv, 1] = OUT_OF_BOOK_DELETION_BELOW
                asks_delete_lv += 1
            elif prev_px_tick > curr_high_px_tick:
                self.ask_delete_lvs[asks_delete_lv, 0] = prev_px_tick * self.tick_size
                self.ask_delete_lvs[asks_delete_lv, 1] = OUT_OF_BOOK_DELETION_ABOVE
                asks_delete_lv += 1
            else:
                exist = False
                for curr_lv in range(self.curr_ask_lv):
                    curr_px_tick = round(self.curr_asks[curr_lv, 0] / self.tick_size)
                    if prev_px_tick == curr_px_tick:
                        exist = True
                        break
                if not exist:
                    self.ask_delete_lvs[asks_delete_lv, 0] = prev_px_tick * self.tick_size
                    self.ask_delete_lvs[asks_delete_lv, 1] = IN_THE_BOOK_DELETION
                    asks_delete_lv += 1

        # Sets the update flag.
        for curr_lv in range(self.curr_ask_lv):
            curr_px_tick = round(self.curr_asks[curr_lv, 0] / self.tick_size)
            exist = False
            for prev_lv in range(self.prev_ask_lv):
                prev_px_tick = round(self.prev_asks[prev_lv, 0] / self.tick_size)
                if prev_px_tick == curr_px_tick:
                    exist = True
                    if (round(self.curr_asks[curr_lv, 1] / self.lot_size) ==
                            round(self.prev_asks[prev_lv, 1] / self.lot_size)):
                        self.curr_asks[curr_lv, 2] = UNCHANGED
                    break
            if not exist:
                self.curr_asks[curr_lv, 2] = INSERTED

        return (
            self.curr_bids,
            self.curr_asks,
            self.bid_delete_lvs[:bids_delete_lv, :],
            self.ask_delete_lvs[:asks_delete_lv, :]
        )
