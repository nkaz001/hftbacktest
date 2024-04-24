import gzip
import json
from typing import Optional, Literal

import numpy as np
from numpy.typing import NDArray

from .. import validate_data
from ..validation import correct_event_order, convert_to_struct_arr
from ... import (
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    COL_EVENT,
    COL_EXCH_TIMESTAMP,
    COL_LOCAL_TIMESTAMP,
    correct_local_timestamp
)


def convert(
        input_filename: str,
        output_filename: Optional[str] = None,
        opt: Literal['', 'm', 't', 'mt'] = '',
        base_latency: float = 0,
        compress: bool = False,
        structured_array: bool = False,
        timestamp_unit: Literal['us', 'ns'] = 'us',
        combined_stream: bool = True
) -> NDArray:
    r"""
    Converts raw Binance Futures feed stream file into a format compatible with HftBacktest.

    File Format:

    .. code-block::

        local_timestamp raw_stream
        1660228023037049 {"stream":"btcusdt@depth@0ms","data":{"e":"depthUpdate","E":1660228023941,"T":1660228023931,"s":"BTCUSDT","U":1801732831593,"u":1801732832589,"pu":1801732831561,"b":[["2467.10","0.000"],["12006.00","0.001"],["24427.70","4.350"],["24620.30","0.172"],["24644.00","44.832"],["24645.40","0.203"],["24652.80","4.900"],["24664.10","4.279"],["24666.50","0.554"],["24666.80","6.764"],["24668.70","7.428"],["24670.90","2.000"],["24671.00","0.000"],["24672.70","0.000"],["24688.30","0.000"]],"a":[["24653.60","0.000"],["24669.80","0.000"],["24670.20","0.000"],["24670.70","0.000"],["24670.90","0.000"],["24671.00","20.812"],["24672.10","0.000"],["24672.30","0.001"],["24674.60","1.520"],["24674.80","0.000"],["24684.20","4.519"],["24684.30","0.202"],["24685.00","0.937"],["24690.90","4.827"],["24693.60","1.500"],["24729.10","0.171"]]}}
        1660228023038319 {"stream":"btcusdt@depth@0ms","data":{"e":"depthUpdate","E":1660228023977,"T":1660228023966,"s":"BTCUSDT","U":1801732832805,"u":1801732834115,"pu":1801732832589,"b":[["2467.10","0.008"],["24643.00","4.457"],["24656.30","0.010"],["24657.70","0.005"],["24658.80","1.000"],["24658.90","1.500"],["24659.50","3.781"],["24659.70","1.806"],["24659.90","0.105"],["24660.60","0.787"],["24666.30","5.033"],["24666.40","0.012"],["24666.50","0.556"],["24668.70","7.426"],["24668.90","0.000"],["24670.90","2.535"],["24680.00","0.000"],["24688.30","0.000"]],"a":[["24653.60","0.000"],["24670.10","0.000"],["24670.60","0.000"],["24670.70","0.000"],["24670.90","0.000"],["24671.00","20.642"],["24672.00","0.000"],["24672.10","0.000"],["24673.50","0.145"],["24673.60","1.567"],["24674.50","3.746"],["24674.60","1.520"],["24678.30","1.304"],["24678.40","0.001"],["24678.80","0.546"],["24678.90","0.002"],["24681.60","0.020"],["24681.70","0.613"],["24681.90","0.077"],["24682.10","3.000"],["24682.20","0.000"],["24683.70","0.163"],["24683.80","4.162"],["24684.00","1.227"],["24684.20","4.519"],["24684.30","0.202"],["24684.90","1.331"],["24685.70","0.156"],["24685.80","0.325"],["24686.70","0.648"],["24692.60","0.040"],["24700.00","47.420"],["24729.10","0.006"]]}}
        1660228023043260 {"stream":"btcusdt@trade","data":{"e":"trade","E":1660228023980,"T":1660228023973,"s":"BTCUSDT","t":2691833663,"p":"24670.90","q":"0.022","X":"MARKET","m":true}}
        1660228023052991 {"stream":"btcusdt@trade","data":{"e":"trade","E":1660228023991,"T":1660228023983,"s":"BTCUSDT","t":2691833664,"p":"24671.00","q":"0.001","X":"MARKET","m":false}}
        1660228023071108 {"stream":"btcusdt@depth@0ms","data":{"e":"depthUpdate","E":1660228024010,"T":1660228024002,"s":"BTCUSDT","U":1801732834136,"u":1801732835323,"pu":1801732834115,"b":[["2467.10","0.000"],["12006.00","0.000"],["24599.40","0.641"],["24603.20","0.104"],["24625.50","0.152"],["24645.20","0.476"],["24646.80","0.081"],["24652.60","0.254"],["24664.10","4.279"],["24666.50","0.878"],["24668.80","0.004"],["24670.90","2.513"],["24688.30","0.000"],["24787.00","0.000"]],"a":[["24653.60","0.000"],["24668.10","0.000"],["24668.70","0.000"],["24669.50","0.000"],["24669.80","0.000"],["24670.00","0.000"],["24670.60","0.000"],["24670.70","0.000"],["24670.90","0.000"],["24671.00","20.641"],["24672.20","0.000"],["24672.30","0.001"],["24673.50","0.040"],["24673.90","0.105"],["24674.70","2.139"],["24674.80","0.000"],["24683.70","0.963"],["24683.90","0.009"],["24685.70","0.556"],["24709.30","0.254"],["24723.80","0.000"],["24728.30","0.193"],["24729.50","4.477"],["24739.40","0.807"],["24743.20","0.235"],["24795.00","0.130"]]}}
        1660228023117894 {"stream":"btcusdt@depth@0ms","data":{"e":"depthUpdate","E":1660228024044,"T":1660228024034,"s":"BTCUSDT","U":1801732835406,"u":1801732836571,"pu":1801732835323,"b":[["2467.10","0.000"],["24337.10","2.462"],["24616.50","1.050"],["24619.00","0.235"],["24640.00","5.148"],["24649.80","2.805"],["24650.00","14.374"],["24651.90","3.000"],["24653.30","1.400"],["24658.70","1.142"],["24658.80","0.000"],["24659.60","3.263"],["24659.70","0.006"],["24660.50","0.840"],["24660.60","0.387"],["24662.20","0.202"],["24663.10","7.147"],["24664.00","0.922"],["24664.20","0.131"],["24664.50","0.027"],["24666.20","7.066"],["24666.40","0.012"],["24668.80","0.002"],["24669.30","0.002"],["24670.20","0.811"],["24670.90","5.817"],["24688.30","0.000"]],"a":[["24653.60","0.000"],["24669.80","0.000"],["24669.90","0.000"],["24670.90","0.000"],["24671.00","20.121"],["24672.10","0.000"],["24672.80","0.000"],["24674.60","1.520"],["24675.30","0.421"],["24681.20","0.239"],["24681.50","1.343"],["24681.60","0.020"],["24681.70","0.213"],["24683.60","2.929"],["24683.70","0.163"],["24683.80","2.162"],["24684.70","0.646"],["24684.90","0.731"],["24692.90","0.321"],["24693.10","0.040"],["24700.70","0.537"],["24703.60","0.210"],["24721.50","7.245"]]}}
        1660228023125009 {"stream":"btcusdt@trade","data":{"e":"trade","E":1660228024062,"T":1660228024055,"s":"BTCUSDT","t":2691833665,"p":"24670.90","q":"0.002","X":"MARKET","m":true}}
        1660228023128966 {"stream":"btcusdt@trade","data":{"e":"trade","E":1660228024067,"T":1660228024061,"s":"BTCUSDT","t":2691833666,"p":"24670.90","q":"0.020","X":"MARKET","m":true}}
        1660228023138740 {"stream":"btcusdt@depth@0ms","data":{"e":"depthUpdate","E":1660228024077,"T":1660228024066,"s":"BTCUSDT","U":1801732836639,"u":1801732837803,"pu":1801732836571,"b":[["2467.10","0.000"],["24659.00","0.000"],["24659.30","2.500"],["24663.00","1.038"],["24664.20","0.118"],["24666.20","7.065"],["24666.50","0.554"],["24666.70","3.987"],["24666.80","7.088"],["24666.90","0.014"],["24667.40","1.506"],["24668.90","0.006"],["24670.10","0.272"],["24670.90","6.726"],["24688.30","0.000"]],"a":[["24653.60","0.000"],["24668.70","0.000"],["24670.30","0.000"],["24670.50","0.000"],["24670.90","0.000"],["24679.00","0.001"],["24703.10","1.500"],["24710.50","0.057"],["24728.30","0.028"],["24768.50","0.318"],["24980.10","5.446"],["25050.00","119.300"]]}}
        1660228023149748 {"stream":"btcusdt@trade","data":{"e":"trade","E":1660228024088,"T":1660228024081,"s":"BTCUSDT","t":2691833667,"p":"24671.00","q":"0.063","X":"MARKET","m":false}}

    Args:
        input_filename: Input filename with path.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        opt: Additional processing options:

             - ``m``: Processes ``markPriceUpdate`` stream with the following custom event IDs.

                - index: ``100``
                - mark price: ``101``
                - funding rate: ``102``

             - ``t``: Processes ``bookTicker`` stream with the following custom event IDs.

                - best bid: ``103``
                - best ask: ``104``

        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        compress: If this is set to True, the output file will be compressed.
        structured_array: If this is set to True, the output is converted into the new format(currently only Rust impl).
        timestamp_unit: The timestamp unit for exchange timestamp to be converted in. Binance provides timestamps in
                        milliseconds. Both local timestamp and exchange timestamp should be in the same unit.
        combined_stream: Raw stream type.
                         combined stream:
                         {"stream":"solusdt@bookTicker","data":{"e":"bookTicker","u":4456408609867,"s":"SOLUSDT","b":"142.4440","B":"50","a":"142.4450","A":"3","T":1713571200009,"E":1713571200010}}
                         regular stream:
                         {"e":"bookTicker","u":4456408609867,"s":"SOLUSDT","b":"142.4440","B":"50","a":"142.4450","A":"3","T":1713571200009,"E":1713571200010}

    Returns:
        Converted data compatible with HftBacktest.
    """
    if timestamp_unit == 'us':
        timestamp_slice = 16
        timestamp_mul = 1000
    elif timestamp_unit == 'ns':
        timestamp_slice = 19
        timestamp_mul = 1000000
    else:
        raise ValueError
    rows = []
    with gzip.open(input_filename, 'r') as f:
        while True:
            line = f.readline()
            if not line:
                break
            local_timestamp = int(line[:timestamp_slice])
            message = json.loads(line[timestamp_slice + 1:])
            if combined_stream:
                data = message.get('data')
            else:
                data = message
            if data is not None:
                evt = data['e']
                if evt == 'trade':
                    if data['X'] != 'MARKET':
                        continue
                    # event_time = data['E']
                    transaction_time = data['T']
                    price = data['p']
                    qty = data['q']
                    side = -1 if data['m'] else 1  # trade initiator's side
                    exch_timestamp = int(transaction_time) * timestamp_mul
                    rows.append([TRADE_EVENT, exch_timestamp, local_timestamp, side, float(price), float(qty)])
                elif evt == 'depthUpdate':
                    # event_time = data['E']
                    transaction_time = data['T']
                    bids = data['b']
                    asks = data['a']
                    exch_timestamp = int(transaction_time) * timestamp_mul
                    rows += [[DEPTH_EVENT, exch_timestamp, local_timestamp, 1, float(bid[0]), float(bid[1])] for bid in bids]
                    rows += [[DEPTH_EVENT, exch_timestamp, local_timestamp, -1, float(ask[0]), float(ask[1])] for ask in asks]
                elif evt == 'markPriceUpdate' and 'm' in opt:
                    # event_time = data['E']
                    transaction_time = data['T']
                    index = data['i']
                    mark_price = data['p']
                    # est_settle_price = data['P']
                    funding_rate = data['r']
                    rows.append([100, -1, local_timestamp, 0, float(index), 0])
                    rows.append([101, -1, local_timestamp, 0, float(mark_price), 0])
                    rows.append([102, -1, local_timestamp, 0, float(funding_rate), 0])
                elif evt == 'bookTicker' and 't' in opt:
                    # event_time = data['E']
                    transaction_time = data['T']
                    bid_price = data['b']
                    bid_qty = data['B']
                    ask_price = data['a']
                    ask_qty = data['A']
                    exch_timestamp = int(transaction_time) * timestamp_mul
                    rows.append([103, exch_timestamp, local_timestamp, 1, float(bid_price), float(bid_qty)])
                    rows.append([104, exch_timestamp, local_timestamp, -1, float(ask_price), float(ask_qty)])
            else:
                # snapshot
                # event_time = msg['E']
                transaction_time = message['T']
                bids = message['bids']
                asks = message['asks']
                exch_timestamp = int(transaction_time) * timestamp_mul
                if len(bids) > 0:
                    bid_clear_upto = float(bids[-1][0])
                    # clears the existing market depth upto the prices in the snapshot.
                    rows.append([DEPTH_CLEAR_EVENT, exch_timestamp, local_timestamp, 1, bid_clear_upto, 0])
                    # inserts the snapshot.
                    rows += [[DEPTH_SNAPSHOT_EVENT, exch_timestamp, local_timestamp, 1, float(bid[0]), float(bid[1])]
                             for bid in bids]
                if len(asks) > 0:
                    ask_clear_upto = float(asks[-1][0])
                    # clears the existing market depth upto the prices in the snapshot.
                    rows.append([DEPTH_CLEAR_EVENT, exch_timestamp, local_timestamp, -1, ask_clear_upto, 0])
                    # inserts the snapshot.
                    rows += [[DEPTH_SNAPSHOT_EVENT, exch_timestamp, local_timestamp, -1, float(ask[0]), float(ask[1])]
                             for ask in asks]

    data = np.asarray(rows, np.float64)

    print('Correcting the latency')
    merged = correct_local_timestamp(data, base_latency)

    print('Correcting the event order')
    sorted_exch_ts = merged[np.argsort(merged[:, COL_EXCH_TIMESTAMP], kind='mergesort')]
    sorted_local_ts = merged[np.argsort(merged[:, COL_LOCAL_TIMESTAMP], kind='mergesort')]

    data = correct_event_order(sorted_exch_ts, sorted_local_ts, structured_array)

    if not structured_array:
        # Validate again.
        num_corr = validate_data(data)
        if num_corr < 0:
            raise ValueError

    if structured_array:
        data = convert_to_struct_arr(data)

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        if compress:
            np.savez_compressed(output_filename, data=data)
        else:
            np.savez(output_filename, data=data)

    return data
