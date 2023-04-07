# These models are implemented as described in
# https://quant.stackexchange.com/questions/3782/how-do-we-estimate-position-of-our-order-in-order-book
# http://www.math.ualberta.ca/~cfrei/PIMS/Almgren5.pdf

from numba.experimental import jitclass

import numpy as np


@jitclass
class RiskAverseQueueModel:
    def __init__(self):
        pass

    def new(self, order, proc):
        if order.side == 1:
            order.q[0] = proc.bid_depth.get(order.price_tick, 0)
        else:
            order.q[0] = proc.ask_depth.get(order.price_tick, 0)

    def trade(self, order, qty, proc):
        order.q[0] -= qty

    def depth(self, order, prev_qty, new_qty, proc):
        order.q[0] = min(order.q[0], new_qty)

    def is_filled(self, order, proc):
        return round(order.q[0] / proc.lot_size) < 0

    def reset(self):
        pass


class ProbQueueModel:
    def __init__(self):
        pass

    def new(self, order, proc):
        if order.side == 1:
            order.q[0] = proc.bid_depth.get(order.price_tick, 0)
        else:
            order.q[0] = proc.ask_depth.get(order.price_tick, 0)

    def trade(self, order, qty, proc):
        order.q[0] -= qty
        order.q[1] += qty

    def depth(self, order, prev_qty, new_qty, proc):
        chg = prev_qty - new_qty
        # In order to avoid duplicate order queue position adjustment, subtract queue position change by trades.
        chg = chg - order.q[1]
        # Reset, as quantity change by trade should be already reflected in qty.
        order.q[1] = 0
        # For an increase of the quantity, front queue doesn't change by the quantity change.
        if chg < 0:
            order.q[0] = min(order.q[0], new_qty)
            return

        front = order.q[0]
        back = prev_qty - front

        prob = self.prob(front, back)
        if not np.isfinite(prob):
            prob = 1

        est_front = front - (1 - prob) * chg + min(back - prob * chg, 0)
        order.q[0] = min(est_front, new_qty)

    def is_filled(self, order, proc):
        return round(order.q[0] / proc.lot_size) < 0

    def prob(self, front, back):
        return np.divide(self.f(back), self.f(back) + self.f(front))

    def reset(self):
        pass


@jitclass
class LogProbQueueModel(ProbQueueModel):
    def f(self, x):
        return np.log(1 + x)


@jitclass
class IdentityProbQueueModel(ProbQueueModel):
    def f(self, x):
        return x


@jitclass
class SquareProbQueueModel(ProbQueueModel):
    def f(self, x):
        return x ** 2
