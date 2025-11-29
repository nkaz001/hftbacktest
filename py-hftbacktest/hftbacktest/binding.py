import warnings
import numpy as np
warnings.filterwarnings('ignore', category=UserWarning)

class _FakeLib:
    pass

lib = _FakeLib()

# Dummy NumPy dtype for event structures (matches typical hftbacktest event layout)
event_dtype = np.dtype([
    ('ev', np.uint32),
    ('exch_ts', np.uint64),
    ('local_ts', np.uint64),
    ('px', np.float64),
    ('qty', np.float64),
    ('order_id', np.uint64),
    ('ival', np.int64),
    ('fval', np.float64)
])

# Complete set of symbols imported from binding.py (covering variations like ROIVec vs ROIVector)
HashMapMarketDepthBacktest_ = type('Dummy', (), {})()
ROIVecMarketDepthBacktest_ = type('Dummy', (), {})()
ROIVectorMarketDepthBacktest_ = type('Dummy', (), {})()
HashMapMarketDepthBacktest = type('Dummy', (), {})()
ROIVecMarketDepthBacktest = type('Dummy', (), {})()
ROIVectorMarketDepthBacktest = type('Dummy', (), {})()
HashMapMarketDepth = type('Dummy', (), {})()
ROIVecMarketDepth = type('Dummy', (), {})()
ROIVectorMarketDepth = type('Dummy', (), {})()
