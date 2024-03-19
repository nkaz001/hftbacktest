import sys

import numpy as np
import pandas as pd
from numba import int64, float64, objmode
from numba.experimental import jitclass
from numba.typed import Dict, List
from numba.types import DictType, ListType, unicode_type

COL_EVENT = 0
COL_EXCH_TIMESTAMP = 1
COL_LOCAL_TIMESTAMP = 2
COL_SIDE = 3
COL_PRICE = 4
COL_QTY = 5

DEPTH_EVENT = 1
TRADE_EVENT = 2
DEPTH_CLEAR_EVENT = 3
DEPTH_SNAPSHOT_EVENT = 4
USER_DEFINED_EVENT = 100

WAIT_ORDER_RESPONSE_NONE = -1
WAIT_ORDER_RESPONSE_ANY = -2

UNTIL_END_OF_DATA = sys.maxsize

EXCH_EVENT = 1 << 31
LOCAL_EVENT = 1 << 30

BUY = 1 << 29
SELL = 1 << 28


@jitclass
class Cache:
    data: DictType(int64, float64[:, :])
    ref: DictType(int64, int64)

    def __init__(self):
        self.data = Dict.empty(int64, np.empty((0, 0,), float64))
        self.ref = Dict.empty(int64, int64)

    def __setitem__(self, key, value):
        self.data[key] = value
        self.ref[key] = 0

    def __getitem__(self, key):
        self.ref[key] += 1
        data = self.data[key]
        return data

    def __contains__(self, key):
        return key in self.data

    def remove(self, data):
        for i, d in self.data.items():
            if d is data:
                self.ref[i] -= 1
                if self.ref[i] == 0:
                    del self.data[i]
                    del self.ref[i]
                return


@jitclass
class DataReader:
    file_list: ListType(unicode_type)
    data_num: int64
    cache: Cache.class_type.instance_type

    def __init__(self, cache):
        self.file_list = List.empty_list(unicode_type)
        self.data_num = 0
        self.cache = cache

    def add_file(self, filepath):
        if filepath == '':
            raise ValueError
        self.file_list.append(filepath)

    def add_data(self, data):
        self.cache[len(self.file_list)] = data
        self.file_list.append('')

    def release(self, data):
        self.cache.remove(data)

    def next(self):
        if self.data_num < len(self.file_list):
            filepath = self.file_list[self.data_num]
            if not self.cache.__contains__(self.data_num):
                if filepath == '':
                    raise ValueError
                with objmode(data='float64[:, :]'):
                    print('Load %s' % filepath)
                    if filepath.endswith('.npy'):
                        data = np.load(filepath)
                    elif filepath.endswith('.npz'):
                        tmp = np.load(filepath)
                        if 'data' in tmp:
                            data = tmp['data']
                        else:
                            k = list(tmp.keys())[0]
                            print("Data is loaded from %s instead of 'data'" % k)
                            data = tmp[k]
                    else:
                        df = pd.read_pickle(filepath, compression='gzip')
                        data = df.to_numpy()
                if data.shape[1] < 6:
                    raise ValueError
                self.cache[self.data_num] = data
            data = self.cache[self.data_num]
            self.data_num += 1
            return data
        else:
            return np.empty((0, 0,), float64)
