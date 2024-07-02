from datetime import datetime

import requests
import json
import pprint
import sys

# Retrieves ticker information.
resp = requests.get('https://fapi.binance.com/fapi/v1/ticker/24hr')
if resp.status_code != 200:
    sys.exit(1)

tickers = resp.json()

resp = requests.get('https://fapi.binance.com/fapi/v1/exchangeInfo')
if resp.status_code != 200:
    sys.exit(1)

exch_info = resp.json()

# Reorganizes the ticker information to include what we need, such as tick size, lot size, daily volume, and
# average price.
ticker_info = {}

for ticker in tickers:
    symbol = ticker['symbol']
    ticker_info[symbol] = info = {}
    info['weighted_avg_price'] = ticker['weightedAvgPrice']
    info['quote_volume'] = ticker['quoteVolume']

for ticker in exch_info['symbols']:
    symbol = ticker['symbol']
    info = ticker_info.get(symbol)
    if info is None:
        continue

    info['onboard_date'] = datetime.fromtimestamp(ticker['onboardDate'] / 1000).strftime('%Y%m%d')
    for item in ticker['filters']:
        if item['filterType'] == 'PRICE_FILTER':
            info['tick_size'] = item['tickSize']
        if item['filterType'] == 'LOT_SIZE':
            info['lot_size'] = item['stepSize']
            info['min_qty'] = item['minQty']
        if item['filterType'] == 'MARKET_LOT_SIZE':
            if info['lot_size'] != item['stepSize'] or info['min_qty'] != item['minQty']:
                raise ValueError('MARKET_LOT_SIZE != LOT_SIZE')

# Chooses only altcoins and choose the given number of top pairs based on daily trading volume. To avoid selecting pairs
# with a spike in volume, it is recommended to calculate and use the average daily volume; you may select your own
# trading universe pairs here.
num_tickers = 50

alts_tickers = {
    symbol: info for symbol, info in ticker_info.items()
    if not symbol.startswith('BTCUSD') and not symbol.startswith('ETHUSD')
}
alts_tickers = dict(sorted(alts_tickers.items(), key=lambda item: float(item[1]['quote_volume']), reverse=True))
alts_tickers = dict(list(alts_tickers.items())[:num_tickers])

pprint.pprint(alts_tickers, compact=True)

with open('tickers.json', 'w') as f:
    json.dump(alts_tickers, f)
