import numpy as np

from hftbacktest import HftBacktest, Linear, ConstantLatency


def load_ndarray(filepath):
    data = np.load(filepath)
    if isinstance(data, np.ndarray):
        return data
    elif isinstance(data, np.lib.npyio.NpzFile):
        if 'data' in data:
            return data['data']
        else:
            return data[list(data.keys())[0]]
    else:
        raise ValueError('unknown data type')


def create_last_snapshot(data_filename, output_snapshot_filename, initial_snapshot_filename=None, tick_size=0.000001, lot_size=0.0001):
    data = load_ndarray(data_filename)
    snapshot = load_ndarray(initial_snapshot_filename) if initial_snapshot_filename is not None else None

    # Just to reconstruct order book from the given snapshot to the end of the given data.
    hbt = HftBacktest(data, tick_size, lot_size, 0, 0, ConstantLatency(0, 0), Linear, snapshot=snapshot)

    # Go to the end of the data.
    hbt.goto(hbt.last_timestamp + 1)

    snapshot = []
    snapshot += [[4, hbt.last_timestamp, -1, 1, float(bid), float(qty)]
                 for bid, qty in sorted(hbt.bid_depth.items(), key=lambda v: -float(v[0]))]
    snapshot += [[4, hbt.last_timestamp, -1, -1, float(ask), float(qty)]
                 for ask, qty in sorted(hbt.ask_depth.items(), key=lambda v: float(v[0]))]

    np.savez(output_snapshot_filename, data=np.asarray(snapshot, np.float64))
    print('Done')
