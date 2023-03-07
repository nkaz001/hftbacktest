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


@jitclass
class DataBinder:
    file_num: int64
    data: float64[:, :]

    def __init__(self, data):
        self.data = data
        self.file_num = 0

    def next(self):
        if self.file_num >= 1:
            return np.empty((0, 0), np.float64)
        self.file_num += 1
        return self.data


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
        return self.data[key]

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
    file_num: int64
    cached: Cache.class_type.instance_type

    def __init__(self, cached):
        self.file_list = List.empty_list(unicode_type)
        self.file_num = 0
        self.cached = cached

    def add_file(self, filepath):
        self.file_list.append(filepath)

    def release(self, data):
        self.cached.remove(data)

    def next(self):
        if self.file_num < len(self.file_list):
            filepath = self.file_list[self.file_num]
            if not self.cached.__contains__(self.file_num):
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
                self.cached[self.file_num] = data
            data = self.cached[self.file_num]
            self.file_num += 1
            return data
        else:
            return np.empty((0, 0,), float64)
