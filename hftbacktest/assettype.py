from numba import int64


class LinearAsset:
    r"""
    Linear asset: the common type of asset.

    Args:
        contract_size: Contract size of the asset.
    """

    contract_size: int64

    def __init__(self, contract_size=1):
        self.contract_size = contract_size

    def amount(self, exec_price, qty):
        return self.contract_size * exec_price * qty

    def equity(self, price, balance, position, fee):
        return balance + self.contract_size * position * price - fee


class InverseAsset:
    r"""
    Inverse asset: the contract's notional value is denominated in the quote currency.

    Args:
        contract_size: Contract size of the asset.
    """

    contract_size: int64

    def __init__(self, contract_size=1):
        self.contract_size = contract_size

    def amount(self, exec_price, qty):
        return self.contract_size * qty / exec_price

    def equity(self, price, balance, position, fee):
        return -balance - self.contract_size * position / price - fee
