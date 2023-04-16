from typing import Union, Callable, NewType, List

from numba.experimental.jitclass.base import JitClassType
from numpy.typing import NDArray
from pandas import DataFrame


Data = Union[str, NDArray, DataFrame]
DataCollection = Union[Data, List[Data]]

HftBacktestType = NewType('HftBacktest', JitClassType)
Reader = NewType('Reader', JitClassType)
OrderBus = NewType('OrderBus', JitClassType)
MarketDepth = NewType('MarketDepth', JitClassType)
State = NewType('State', JitClassType)
OrderLatencyModel = NewType('OrderLatencyModel', JitClassType)
AssetType = NewType('AssetType', JitClassType)
QueueModel = NewType('QueueModel', JitClassType)

ExchangeModelInitiator = Callable[
    [
        Reader,
        OrderBus,
        OrderBus,
        MarketDepth,
        State,
        OrderLatencyModel,
        QueueModel
    ],
    JitClassType
]
