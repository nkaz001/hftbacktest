import gzip
import json
from typing import Optional

import numpy as np
from numpy.typing import NDArray

from hftbacktest.data.utils.difforderbooksnapshot import (
    DiffOrderBookSnapshot,
    CHANGED,
    INSERTED, IN_THE_BOOK_DELETION,
)
from hftbacktest.data.validation import correct_event_order, correct_local_timestamp, validate_event_order
from ...types import (
    DEPTH_EVENT,
    TRADE_EVENT,
    BUY_EVENT,
    SELL_EVENT,
    event_dtype
)


def convert(
        input_filename: str,
        tick_size: float,
        lot_size: float,
        num_levels: int = 20,
        output_filename: Optional[str] = None,
        base_latency: float = 0,
        buffer_size: int = 100_000_000,
        exch_ts_multiplier: int = 1_000_000,
        delete_out_of_book: bool = True
) -> NDArray:
    r"""
    Converts raw Hyperliquid feed stream file into a format compatible with HftBacktest.
    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size``.

    **File Format:**

    .. code-block::

        local_timestamp raw_stream
        1736682893953732482 {"channel":"trades","data":[{"coin":"HYPE","side":"A","px":"21.269","sz":"7.78","time":1736682877317,"hash":"0x0190932c67e6b94b2bc6041b482c76013b00c5a4dc99c1611ca5bdc1edcd2786","tid":185481619581124,"users":["0x010461c14e146ac35fe42271bdc1134ee31c703a","0x475d0900e45e9d9f4661f30e616e2395da592720"]},{"coin":"HYPE","side":"B","px":"21.289","sz":"3.29","time":1736682878040,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":680159081286628,"users":["0x6844b345852a97d87c2bed4e0c123ee5c935c000","0xa1cb16d2b17202c336138f765559dfc73830b1fb"]},{"coin":"HYPE","side":"B","px":"21.289","sz":"0.55","time":1736682878040,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":430781595823972,"users":["0xdbe452f1a0d0190e161b5fae8ee3a530f099d9a3","0xa1cb16d2b17202c336138f765559dfc73830b1fb"]},{"coin":"HYPE","side":"B","px":"21.289","sz":"4.65","time":1736682880335,"hash":"0xa105b47441ecc649b68c041b482ca5013f00c4ba6114fe23110f14dd364eda0a","tid":679380818995751,"users":["0x43205e893e4a958fb993c9a0df436530f47aefb8","0x739f081428592c9c4b0a6a88dec5f0308e6788a0"]},{"coin":"HYPE","side":"B","px":"21.289","sz":"2.41","time":1736682880518,"hash":"0x3fc2541a75d20cb2f607041b482ca70155001953351c0f0610392ac90a283353","tid":471300372581745,"users":["0x38b9a6c32685df04afb3282ecc103cbdba7ab9ff","0xa554e2958a076e273552cbaaa795494089e2308c"]},{"coin":"HYPE","side":"B","px":"21.289","sz":"2.24","time":1736682880518,"hash":"0x3fc2541a75d20cb2f607041b482ca70155001953351c0f0610392ac90a283353","tid":824956871283458,"users":["0x38b9a6c32685df04afb3282ecc103cbdba7ab9ff","0x739f081428592c9c4b0a6a88dec5f0308e6788a0"]},{"coin":"HYPE","side":"A","px":"21.279","sz":"18.28","time":1736682881724,"hash":"0xd65c56f65c3d8b89e085041b482cbb012c006342f8cbcc361981dfd6f5d8f4cd","tid":109504214426929,"users":["0x924b0b9147f3562065d180d6995dd30c95c12eac","0x972baee8b44ad0b1e73dee6c20f34e17dd01fe47"]},{"coin":"HYPE","side":"A","px":"21.273","sz":"68.65","time":1736682881724,"hash":"0xd65c56f65c3d8b89e085041b482cbb012c006342f8cbcc361981dfd6f5d8f4cd","tid":170899955875792,"users":["0x31ca8395cf837de08b24da3f660e77761dfb974b","0x972baee8b44ad0b1e73dee6c20f34e17dd01fe47"]},{"coin":"HYPE","side":"B","px":"21.289","sz":"19.74","time":1736682882067,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":663835145561653,"users":["0x2657a9d65dc2e832dd0f6e05a556408353a149bc","0x739f081428592c9c4b0a6a88dec5f0308e6788a0"]},{"coin":"HYPE","side":"B","px":"21.291","sz":"4.7","time":1736682882067,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":68532102663456,"users":["0x2657a9d65dc2e832dd0f6e05a556408353a149bc","0xe72d9d94337cc12659ee9d1f3aea5475035fec5a"]},{"coin":"HYPE","side":"B","px":"21.292","sz":"15.58","time":1736682882067,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":43588775356743,"users":["0x2657a9d65dc2e832dd0f6e05a556408353a149bc","0xa1cb16d2b17202c336138f765559dfc73830b1fb"]},{"coin":"HYPE","side":"B","px":"21.292","sz":"42.63","time":1736682882067,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":309212071809688,"users":["0x2657a9d65dc2e832dd0f6e05a556408353a149bc","0x2994bf69adee67aba8e3b28bec460ed2001df759"]},{"coin":"HYPE","side":"B","px":"21.294","sz":"2.87","time":1736682882930,"hash":"0x30b9df8c28987268a1a3041b482cce01440090b85064d8547ec0e75c8bab188e","tid":651550759705720,"users":["0x1341c77a124edb0c90bd98442f6a4d0baa5ba824","0x9d9cbc11c85848a3ebf181940f2bec9f5fc75396"]},{"coin":"HYPE","side":"B","px":"21.294","sz":"1.78","time":1736682882930,"hash":"0x30b9df8c28987268a1a3041b482cce01440090b85064d8547ec0e75c8bab188e","tid":701267030078115,"users":["0x1341c77a124edb0c90bd98442f6a4d0baa5ba824","0xe72d9d94337cc12659ee9d1f3aea5475035fec5a"]},{"coin":"HYPE","side":"B","px":"21.293","sz":"29.51","time":1736682883935,"hash":"0x3a8d19c41d366f4fd1c8041b482cde015a00275e602c0c3c97410f75181c87fd","tid":73469641722474,"users":["0x28547f2cce3bc73d5da5356745b6baeb6c53ed93","0x2994bf69adee67aba8e3b28bec460ed2001df759"]},{"coin":"HYPE","side":"B","px":"21.294","sz":"2.92","time":1736682883935,"hash":"0x3a8d19c41d366f4fd1c8041b482cde015a00275e602c0c3c97410f75181c87fd","tid":907055023121248,"users":["0x28547f2cce3bc73d5da5356745b6baeb6c53ed93","0xe72d9d94337cc12659ee9d1f3aea5475035fec5a"]},{"coin":"HYPE","side":"B","px":"21.299","sz":"203.8","time":1736682883935,"hash":"0x3a8d19c41d366f4fd1c8041b482cde015a00275e602c0c3c97410f75181c87fd","tid":114855171699084,"users":["0x28547f2cce3bc73d5da5356745b6baeb6c53ed93","0x023a3d058020fb76cca98f01b3c48c8938a22355"]},{"coin":"HYPE","side":"B","px":"21.3","sz":"0.84","time":1736682885011,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":414458082088382,"users":["0x6844b345852a97d87c2bed4e0c123ee5c935c000","0x9d9cbc11c85848a3ebf181940f2bec9f5fc75396"]},{"coin":"HYPE","side":"B","px":"21.3","sz":"2.29","time":1736682886016,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":516578515032737,"users":["0x2657a9d65dc2e832dd0f6e05a556408353a149bc","0x9d9cbc11c85848a3ebf181940f2bec9f5fc75396"]},{"coin":"HYPE","side":"B","px":"21.304","sz":"1.18","time":1736682886016,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":977487737665395,"users":["0x2657a9d65dc2e832dd0f6e05a556408353a149bc","0xe72d9d94337cc12659ee9d1f3aea5475035fec5a"]},{"coin":"HYPE","side":"B","px":"21.304","sz":"3.28","time":1736682892095,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":725923874460472,"users":["0x6844b345852a97d87c2bed4e0c123ee5c935c000","0xe72d9d94337cc12659ee9d1f3aea5475035fec5a"]},{"coin":"HYPE","side":"A","px":"21.286","sz":"2.45","time":1736682893100,"hash":"0x0000000000000000000000000000000000000000000000000000000000000000","tid":534132059665492,"users":["0xa1cb16d2b17202c336138f765559dfc73830b1fb","0xce43a7498e65b6f6c8741fd9a7efc3f78efc78c4"]},{"coin":"HYPE","side":"A","px":"21.286","sz":"15.97","time":1736682893100,"hash":"0x051810961666d7bb85c4041b482d72011f0031d0f6508508e450796bb5be327c","tid":1025215867692119,"users":["0xa1cb16d2b17202c336138f765559dfc73830b1fb","0xe30dc1eba82267d5e55391d3cdc7d6f72de9c227"]},{"coin":"HYPE","side":"A","px":"21.285","sz":"237.82","time":1736682893100,"hash":"0x051810961666d7bb85c4041b482d72011f0031d0f6508508e450796bb5be327c","tid":1028589439832641,"users":["0x023a3d058020fb76cca98f01b3c48c8938a22355","0xe30dc1eba82267d5e55391d3cdc7d6f72de9c227"]},{"coin":"HYPE","side":"A","px":"21.28","sz":"4.7","time":1736682893100,"hash":"0x051810961666d7bb85c4041b482d72011f0031d0f6508508e450796bb5be327c","tid":864606086107572,"users":["0xe72d9d94337cc12659ee9d1f3aea5475035fec5a","0xe30dc1eba82267d5e55391d3cdc7d6f72de9c227"]},{"coin":"HYPE","side":"A","px":"21.277","sz":"41.05","time":1736682893100,"hash":"0x051810961666d7bb85c4041b482d72011f0031d0f6508508e450796bb5be327c","tid":999086141165990,"users":["0x31ca8395cf837de08b24da3f660e77761dfb974b","0xe30dc1eba82267d5e55391d3cdc7d6f72de9c227"]},{"coin":"HYPE","side":"B","px":"21.304","sz":"0.23","time":1736682893702,"hash":"0x75013367f492e899e498041b482d7b0145005169149be7e7181a0ca2aecc305e","tid":185973078898076,"users":["0xc1995c6b6a101cf9ac7547688908afaf88b01302","0xe72d9d94337cc12659ee9d1f3aea5475035fec5a"]},{"coin":"HYPE","side":"B","px":"21.304","sz":"0.56","time":1736682893702,"hash":"0x75013367f492e899e498041b482d7b0145005169149be7e7181a0ca2aecc305e","tid":98226337118421,"users":["0xc1995c6b6a101cf9ac7547688908afaf88b01302","0xb6bf1eab724da50cc99e9489f4523de277d815b7"]},{"coin":"HYPE","side":"B","px":"21.305","sz":"18.42","time":1736682893702,"hash":"0x75013367f492e899e498041b482d7b0145005169149be7e7181a0ca2aecc305e","tid":627730381177664,"users":["0xc1995c6b6a101cf9ac7547688908afaf88b01302","0xa1cb16d2b17202c336138f765559dfc73830b1fb"]},{"coin":"HYPE","side":"B","px":"21.306","sz":"906.75","time":1736682893702,"hash":"0x75013367f492e899e498041b482d7b0145005169149be7e7181a0ca2aecc305e","tid":996720750934958,"users":["0xc1995c6b6a101cf9ac7547688908afaf88b01302","0xb9cd6cde81e55e1e6714d21b4f228faf98999eed"]}]}
        1736682893953758185 {"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"l2Book","coin":"HYPE","nSigFigs":null,"mantissa":null}}}
        1736682893953775297 {"channel":"l2Book","data":{"coin":"HYPE","time":1736682893796,"levels":[[{"px":"21.277","sz":"80.29","n":2},{"px":"21.276","sz":"317.1","n":2},{"px":"21.273","sz":"4.7","n":1},{"px":"21.271","sz":"1.0","n":1},{"px":"21.27","sz":"3.1","n":1},{"px":"21.268","sz":"94.03","n":1},{"px":"21.266","sz":"424.29","n":3},{"px":"21.265","sz":"97.96","n":3},{"px":"21.264","sz":"2.9","n":1},{"px":"21.263","sz":"23.74","n":2},{"px":"21.262","sz":"74.81","n":3},{"px":"21.261","sz":"494.98","n":2},{"px":"21.26","sz":"193.82","n":1},{"px":"21.259","sz":"54.7","n":2},{"px":"21.258","sz":"127.74","n":3},{"px":"21.255","sz":"194.67","n":2},{"px":"21.254","sz":"187.49","n":3},{"px":"21.251","sz":"0.5","n":1},{"px":"21.249","sz":"81.6","n":2},{"px":"21.247","sz":"28.43","n":1}],[{"px":"21.306","sz":"223.2","n":2},{"px":"21.309","sz":"43.18","n":1},{"px":"21.311","sz":"125.23","n":1},{"px":"21.312","sz":"2.99","n":1},{"px":"21.313","sz":"1.0","n":1},{"px":"21.317","sz":"60.47","n":1},{"px":"21.318","sz":"21.3","n":2},{"px":"21.319","sz":"339.71","n":2},{"px":"21.322","sz":"35.0","n":1},{"px":"21.323","sz":"227.41","n":1},{"px":"21.324","sz":"12.38","n":2},{"px":"21.33","sz":"46.18","n":2},{"px":"21.335","sz":"1.0","n":1},{"px":"21.336","sz":"120.23","n":2},{"px":"21.342","sz":"2.88","n":1},{"px":"21.344","sz":"1562.36","n":3},{"px":"21.345","sz":"391.37","n":2},{"px":"21.347","sz":"292.91","n":1},{"px":"21.348","sz":"13.41","n":3},{"px":"21.349","sz":"669.92","n":3}]]}}

    Args:
        input_filename: Input filename with path.
        tick_size: Tick size, minimum price increment.
        lot_size: Lot size, minimum quantity increment.
        num_levels: Number of market depth snapshot levels delivered by the ``l2Book`` channel. Default: ``20``.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        buffer_size: Sets a preallocated row size for the buffer.
        exch_ts_multiplier: Multiplier to convert exchange timestamps to nanoseconds. Default: ``1_000_000``.
        delete_out_of_book: Whether to insert a market depth delete event when an existing level moves out of the book
                            (i.e., beyond the the given number of market depth snapshot levels)

    Returns:
        Converted data compatible with HftBacktest.
    """


    tmp = np.empty(buffer_size, event_dtype)
    row_num = 0
    timestamp_slice = 19
    diff = DiffOrderBookSnapshot(num_levels, tick_size, lot_size)

    with gzip.open(input_filename, 'r') as f:
        while True:
            line = f.readline()
            if not line:
                break

            local_ts = int(line[:timestamp_slice])
            message = json.loads(line[timestamp_slice + 1:])
            if message.get("channel") == "trades":
                trades_data = message.get("data", [])
                for trade in trades_data:
                    exch_ts = trade.get("time") * exch_ts_multiplier

                    tmp[row_num] = (
                        TRADE_EVENT | (SELL_EVENT if trade.get("side") == "A" else BUY_EVENT), # trade initiator's side
                        exch_ts,
                        int(local_ts),
                        float(trade.get("px")),
                        float(trade.get("sz")),
                        0,
                        0,
                        0
                    )
                    row_num += 1

            elif message.get("channel") == "l2Book":
                depth_data = message.get("data", {})

                exch_ts = depth_data.get("time") * exch_ts_multiplier
                levels = depth_data.get("levels")
                bids = levels[0]
                asks = levels[1]

                bid_px = np.array([float(b["px"]) for b in bids])
                bid_qty = np.array([float(b["sz"]) for b in bids])
                ask_px = np.array([float(a["px"]) for a in asks])
                ask_qty = np.array([float(a["sz"]) for a in asks])

                bid, ask, bid_del, ask_del = diff.snapshot(bid_px, bid_qty, ask_px, ask_qty)

                for entry in bid:
                    if entry[2] == INSERTED or entry[2] == CHANGED:
                        tmp[row_num] = (
                            DEPTH_EVENT | BUY_EVENT,
                            exch_ts,
                            int(local_ts),
                            entry[0],
                            entry[1],
                            0,
                            0,
                            0
                        )
                        row_num += 1
                for entry in ask:
                    if entry[2] == INSERTED or entry[2] == CHANGED:
                        tmp[row_num] = (
                            DEPTH_EVENT | SELL_EVENT,
                            exch_ts,
                            int(local_ts),
                            entry[0],
                            entry[1],
                            0,
                            0,
                            0
                        )
                        row_num += 1
                for entry in bid_del:
                    if entry[1] == IN_THE_BOOK_DELETION or delete_out_of_book:
                        tmp[row_num] = (
                            BUY_EVENT | DEPTH_EVENT,
                            exch_ts,
                            int(local_ts),
                            entry[0],
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1
                for entry in ask_del:
                    if entry[1] == IN_THE_BOOK_DELETION or delete_out_of_book:
                        tmp[row_num] = (
                            SELL_EVENT | DEPTH_EVENT,
                            exch_ts,
                            int(local_ts),
                            entry[0],
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1

    tmp = tmp[:row_num]

    print('Correcting the latency')
    tmp = correct_local_timestamp(tmp, base_latency)

    print('Correcting the event order')
    data = correct_event_order(
        tmp,
        np.argsort(tmp['exch_ts'], kind='mergesort'),
        np.argsort(tmp['local_ts'], kind='mergesort')
    )

    validate_event_order(data)

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        np.savez_compressed(output_filename, data=data)

    return data
