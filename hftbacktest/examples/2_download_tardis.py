import json
import os.path
import requests
from datetime import datetime, timedelta

from tqdm import tqdm

date_from = 20240501
date_to = 20240531

# The path where the downloaded files will be located.
csv_path = '.'

exchange = 'binance-futures'

key = os.environ['TARDIS_KEY']

with open('tickers.json', 'r') as f:
    tickers = json.load(f)


# Downloads the data from Tardis.
def download(exchange, data_type, symbol, yyyymmdd, filepath, key):
    yyyymmdd = str(yyyymmdd)
    url = f'https://datasets.tardis.dev/v1/{exchange}/{data_type}/{yyyymmdd[:4]}/{yyyymmdd[4:6]}/{yyyymmdd[6:]}/{symbol}.csv.gz'

    response = requests.get(url, stream=True, headers={'Authorization': f'Bearer {key}'})
    if response.status_code != 200:
        raise RuntimeError('Could not download file')

    total_size = int(response.headers.get('content-length', 0))
    block_size = 1024

    with tqdm(total=total_size, unit='B', unit_scale=True) as progress_bar:
        with open(filepath, 'wb') as file:
            for data in response.iter_content(block_size):
                progress_bar.update(len(data))
                file.write(data)

    if total_size != 0 and progress_bar.n != total_size:
        raise RuntimeError('Could not download file')


for symbol, info in tickers.items():
    date = datetime.strptime(str(date_from), '%Y%m%d')
    date_to_ = datetime.strptime(str(date_to), '%Y%m%d')

    if int(info['onboard_date']) > int(date.strftime('%Y%m%d')):
        print(f'{symbol} has been listed since {info["onboard_date"]}')
        continue

    while date <= date_to_:
        yyyymmdd = date.strftime('%Y%m%d')
        print(f'Downloading {symbol} for {yyyymmdd}')
        for data_type in ['incremental_book_L2', 'trades']:
            try:
                download(
                    exchange,
                    data_type,
                    symbol,
                    yyyymmdd,
                    os.path.join(csv_path, f'{exchange}_{data_type}_{symbol}_{yyyymmdd}.csv.gz'),
                    key
                )
            except Exception as e:
                print(e, symbol, yyyymmdd, data_type)
        date += timedelta(days=1)
