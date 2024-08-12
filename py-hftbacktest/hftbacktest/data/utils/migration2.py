import numpy as np
from numba import njit
from numpy.typing import NDArray

from hftbacktest import BUY_EVENT, SELL_EVENT, EXCH_EVENT, LOCAL_EVENT, event_dtype


@njit
def convert_(inp, ts_mul):
    out = np.zeros(len(inp), event_dtype)
    for i in range(len(inp)):
        ev = int(inp[i, 0])
        if inp[i, 3] == 1:
            ev |= BUY_EVENT
        elif inp[i, 3] == -1:
            ev |= SELL_EVENT
        if inp[i, 1] > 0:
            ev |= EXCH_EVENT
        if inp[i, 2] > 0:
            ev |= LOCAL_EVENT
        out[i].ev = ev
        out[i].exch_ts = inp[i, 1] * ts_mul
        out[i].local_ts = inp[i, 2] * ts_mul
        out[i].px = inp[i, 4]
        out[i].qty = inp[i, 5]
    return out


def convert(
        input_file: str,
        output_filename: str | None = None,
        ts_mul: float = 1000
) -> NDArray:
    r"""
    Converts HftBacktest v1 data file into HftBacktest v2 data.

    Since v1 data uses `-1` in timestamps to indicate the invalidity of the event on that processor side, there will be
    a loss of timestamp information. Furthermore, it cannot check its validity due to this.
    Validity should be confirmed during the v1 generation.

    Args:
        input_file: Input filename for HftBacktest v1 data.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        ts_mul: The value is multiplied by the v1 format timestamp to adjust the timestamp unit.
                Typically, v1 uses microseconds, while v2 uses nanoseconds, so the default value is 1000.

    Returns:
        Converted data compatible with HftBacktest.
    """

    data_v1 = np.load(input_file)['data']
    data_v2 = convert_(data_v1, ts_mul)

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        np.savez_compressed(output_filename, data=data_v2)

    return data_v2
