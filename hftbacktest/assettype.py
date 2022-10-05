from numba.experimental import jitclass


@jitclass
class _Linear:
    def __init__(self):
        pass

    def amount(self, exec_price, qty):
        return exec_price * qty

    def equity(self, price, balance, position, fee):
        return balance + position * price - fee


@jitclass
class _Inverse:
    def __init__(self):
        pass

    def amount(self, exec_price, qty):
        return qty / exec_price

    def equity(self, price, balance, position, fee):
        return -balance - position / price - fee


Linear = _Linear()
Inverse = _Inverse()
