import multiprocessing
import os.path
import os
import glob
import re

from hftbacktest.data.utils import tardis

# Tardis data includes an SOD snapshot in every incremental file. Therefore, if you are processing the SOD snapshot,
# there is no need to input a separate initial snapshot. However, having the SOD snapshot in the feed file slows down
# the backtesting process. We process the SOD snapshot only for the date when we begin our backtesting.
sod_date = 20240501

# The path for the Tardis csv files.
csv_path = '.'

# The path for the converted npz files for the Rust version.
npz_path = '.'

# Data conversion requires a lot of memory. Check the memory usage before you run, and choose the appropriate number of
# processors
num_processors = 2

# Depending on the size of the pairs data, you may need to increase the buffer size, which is the number of events for
# the day.
buffer_size = 100_000_000


# Converts the downloaded Tardis data into a format that can be consumed by hftbacktest (Rust version).
def convert(in_path, out_path, exchange, symbol, yyyymmdd, sod):
    files = [
        os.path.join(in_path, f'{exchange}_incremental_book_L2_{symbol}_{yyyymmdd}.csv.gz'),
        os.path.join(in_path, f'{exchange}_trades_{symbol}_{yyyymmdd}.csv.gz')
    ]
    try:
        tardis.convert(
            files,
            output_filename=os.path.join(out_path, f'{symbol}_{yyyymmdd}.npz'),
            buffer_size=buffer_size,
            snapshot_mode='process' if sod else 'ignore',
        )
        # Deletes the input csv files.
        for file in files:
            os.remove(file)
    except Exception as e:
        print(e, exchange, symbol, yyyymmdd)


args = []
pat = re.compile(r'^([\w\-]+)_incremental_book_L2_(\w+)_([0-9]{8})\.csv\.gz$')
for file in glob.glob(os.path.join(csv_path, '*.csv.gz')):
    match = pat.match(os.path.basename(file))
    if match is not None:
        exchange = match.group(1)
        symbol = match.group(2)
        yyyymmdd = match.group(3)
        args.append((csv_path, npz_path, exchange, symbol, yyyymmdd, yyyymmdd == sod_date))

with multiprocessing.Pool(num_processors) as pool:
    pool.starmap(convert, args)
