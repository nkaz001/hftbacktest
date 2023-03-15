import os
import unittest

import pandas as pd
from numba import njit

from hftbacktest import DataReader, Cache

import numpy as np


@njit
def test_cache():
    cache = Cache()

    d = np.asarray([[1, 2, 3], [4, 5, 6]], dtype=np.float64)
    assert not cache.__contains__(1)
    cache.__setitem__(1, d)
    assert cache.__contains__(1)

    data = cache.__getitem__(1)
    assert (data == d).all()

    data = cache.__getitem__(1)
    # only works in Numba JIT'ed method.
    cache.remove(data)
    assert cache.__contains__(1)

    cache.remove(data)
    assert not cache.__contains__(1)


class TestReader(unittest.TestCase):
    def test_cache(self):
        test_cache()

    def test_reader(self):
        cache = Cache()

        reader = DataReader(cache)

        reader.add_data(self.data[0])
        reader.add_file('d1.npz')
        reader.add_data(self.data[2])
        reader.add_file('d3.npz')
        reader.add_data(self.data[4])
        reader.add_file('d5.npy')
        reader.add_file('d6.pkl')

        for i in range(len(self.data)):
            d = reader.next()
            assert (d == self.data[i]).all()

    def setUp(self):
        self.data = [np.random.random((1000, 1000)) for _ in range(7)]

        np.savez('d1.npz', data=self.data[1])
        np.savez('d3.npz', self.data[3])
        np.save('d5.npy', self.data[5])
        df = pd.DataFrame(self.data[6])
        df.to_pickle('d6.pkl', compression='gzip')

    def doCleanups(self):
        os.remove('d1.npz')
        os.remove('d3.npz')
        os.remove('d5.npy')
        os.remove('d6.pkl')
