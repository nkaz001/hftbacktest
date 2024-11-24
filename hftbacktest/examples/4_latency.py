import os.path
from datetime import datetime, timedelta

import numpy as np
import polars as pl
from hftbacktest import EXCH_EVENT, LOCAL_EVENT
from numba import njit

date_from = 20240501
date_to = 20240531

# Target symbol used to generate order latency based on its feed data latency.
# To obtain realistic backtesting results, it is best to use the actual historical order latency for each pair.
# For example purposes, we use the feed latency of one pair as the order latency.
symbol = 'SOLUSDT'

# Order latency can differ significantly from feed latency. To artificially generate order latency, multiply the given
# multiplier by the order entry latency and order response latency.
mul_entry = 4
mul_resp = 3

# The path for the converted npz files for the Rust version.
npz_path = '.'

# The path for the generated latency files for the Rust version.
latency_path = '.'


@njit
def generate_order_latency_nb(data, order_latency, mul_entry, offset_entry, mul_resp, offset_resp):
    for i in range(len(data)):
        exch_ts = data[i].exch_ts
        local_ts = data[i].local_ts
        feed_latency = local_ts - exch_ts
        order_entry_latency = mul_entry * feed_latency + offset_entry
        order_resp_latency = mul_resp * feed_latency + offset_resp

        req_ts = local_ts
        order_exch_ts = req_ts + order_entry_latency
        resp_ts = order_exch_ts + order_resp_latency

        order_latency[i].req_ts = req_ts
        order_latency[i].exch_ts = order_exch_ts
        order_latency[i].resp_ts = resp_ts


def generate_order_latency(feed_file, output_file=None, mul_entry=1, offset_entry=0, mul_resp=1, offset_resp=0):
    data = np.load(feed_file)['data']
    df = pl.DataFrame(data)

    df = df.filter(
        (pl.col('ev') & EXCH_EVENT == EXCH_EVENT) & (pl.col('ev') & LOCAL_EVENT == LOCAL_EVENT)
    ).with_columns(
        pl.col('local_ts').alias('ts')
    ).group_by_dynamic(
        'ts', every='1000000000i'
    ).agg(
        pl.col('exch_ts').last(),
        pl.col('local_ts').last()
    ).drop('ts')

    data = df.to_numpy(structured=True)

    order_latency = np.zeros(
        len(data),
        dtype=[('req_ts', 'i8'), ('exch_ts', 'i8'), ('resp_ts', 'i8'), ('_padding', 'i8')]
    )
    generate_order_latency_nb(data, order_latency, mul_entry, offset_entry, mul_resp, offset_resp)

    if output_file is not None:
        np.savez_compressed(output_file, data=order_latency)

    return order_latency


date = datetime.strptime(str(date_from), '%Y%m%d')
date_to = datetime.strptime(str(date_to), '%Y%m%d')
while date <= date_to:
    yyyymmdd = date.strftime('%Y%m%d')
    print(f'Generating order latency for {yyyymmdd} from the feed latency')
    try:
        generate_order_latency(
            os.path.join(npz_path, f'{symbol}_{yyyymmdd}.npz'),
            output_file=os.path.join(latency_path, f'latency_{yyyymmdd}.npz'),
            mul_entry=mul_entry,
            mul_resp=mul_resp
        )
    except Exception as e:
        print(e, yyyymmdd)
    date += timedelta(days=1)
