import numpy as np


class RiskAverseQueueModel:
    r"""
    Provides a conservative queue position model, where your order's queue position advances only when trades occur at
    the same price level.
    """

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
    r"""
    Provides a probability-based queue position model as described in
    https://quant.stackexchange.com/questions/3782/how-do-we-estimate-position-of-our-order-in-order-book.

    Your order's queue position advances when a trade occurs at the same price level or the quantity at the level
    decreases. The advancement in queue position depends on the probability based on the relative queue position. To
    avoid double counting the quantity decrease caused by trades, all trade quantities occurring at the level before
    the book quantity changes will be subtracted from the book quantity changes.
    """

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
        # Reset, as quantity change by trade should be already reflected in new_qty.
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


class LogProbQueueModel(ProbQueueModel):
    r"""
    This model uses a logarithmic function ``log(1 + x)`` to adjust the probability.
    """

    def f(self, x):
        return np.log(1 + x)


class IdentityProbQueueModel(ProbQueueModel):
    r"""
    This model uses an identity function ``x`` to adjust the probability.
    """

    def f(self, x):
        return x


class SquareProbQueueModel(ProbQueueModel):
    r"""
    This model uses a square function ``x ** 2`` to adjust the probability.
    """

    def f(self, x):
        return x ** 2


class PowerProbQueueModel(ProbQueueModel):
    r"""
    This model uses a power function ``x ** n`` to adjust the probability.
    """

    def __init__(self, n):
        self.n = n

    def f(self, x):
        return x ** self.n


class ProbQueueModel2(ProbQueueModel):
    r"""
    This model is a variation of the :class:`.ProbQueueModel` that changes the probability calculation to
    f(back) / f(front + back) from f(back) / (f(front) + f(back)).
    """

    def prob(self, front, back):
        return np.divide(self.f(back), self.f(back + front))


class LogProbQueueModel2(ProbQueueModel2):
    r"""
    This model uses a logarithmic function ``log(1 + x)`` to adjust the probability.
    """

    def f(self, x):
        return np.log(1 + x)


class PowerProbQueueModel2(ProbQueueModel2):
    r"""
    This model uses a power function ``x ** n`` to adjust the probability.
    """

    def __init__(self, n):
        self.n = n
        
    def f(self, x):
        return x ** self.n


class ProbQueueModel3(ProbQueueModel):
    r"""
    This model is a variation of the :class:`.ProbQueueModel` that changes the probability calculation to
    1 - f(front / (front + back)) from f(back) / (f(front) + f(back)).
    """

    def prob(self, front, back):
        return 1 - self.f(np.divide(front, back + front))


class PowerProbQueueModel3(ProbQueueModel3):
    r"""
    This model uses a power function ``x ** n`` to adjust the probability.
    """

    def __init__(self, n):
        self.n = n

    def f(self, x):
        return x ** self.n
