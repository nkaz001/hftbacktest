from typing import Union, NewType, List

from numba.experimental.jitclass.base import JitClassType
from numpy.typing import NDArray
from pandas import DataFrame

Data = Union[str, NDArray, DataFrame]
DataCollection = Union[Data, List[Data]]

HftBacktestType = NewType('HftBacktest', JitClassType)
