from numba import int64
from numba.experimental import jitclass


@jitclass
class LinearAsset:
    contract_size: int64

    def __init__(self, contract_size=1):
        self.contract_size = contract_size

    def amount(self, exec_price, qty):
        return self.contract_size * exec_price * qty

    def equity(self, price, balance, position, fee):
        return balance + self.contract_size * position * price - fee


@jitclass
class InverseAsset:
    contract_size: int64

    def __init__(self, contract_size=1):
        self.contract_size = contract_size

    def amount(self, exec_price, qty):
        return self.contract_size * qty / exec_price

    def equity(self, price, balance, position, fee):
        return -balance - self.contract_size * position / price - fee


Linear = LinearAsset()
Inverse = InverseAsset()
