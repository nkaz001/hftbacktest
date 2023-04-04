import numpy as np
from numba import njit

from .validation import validate_data, correct_local_timestamp, correct_exch_timestamp, correct


@njit
def merge_on_local_timestamp(a, b):
    a_shape = a.shape
    b_shape = b.shape
    assert a_shape[1] == b_shape[1]
    tmp = np.empty((a_shape[0] + b_shape[0], a_shape[1]), np.float64)
    i = 0
    j = 0
    k = 0
    while True:
        if i < len(a) and j < len(b):
            if a[i, 2] < b[j, 2]:
                tmp[k] = a[i]
                i += 1
                k += 1
            elif a[i, 2] > b[j, 2]:
                tmp[k] = b[j]
                j += 1
                k += 1
            elif a[i, 1] < b[j, 1]:
                tmp[k] = a[i]
                i += 1
                k += 1
            else:
                tmp[k] = b[j]
                j += 1
                k += 1
        elif i < len(a):
            tmp[k] = a[i]
            i += 1
            k += 1
        elif j < len(b):
            tmp[k] = b[j]
            j += 1
            k += 1
        else:
            break
    return tmp[:k]
