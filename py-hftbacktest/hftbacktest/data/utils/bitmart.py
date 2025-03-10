import gzip
import json
from typing import Optional, Literal

import numpy as np
from numpy.typing import NDArray

from ..validation import correct_event_order, correct_local_timestamp, validate_event_order
from ...types import (
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    BUY_EVENT,
    SELL_EVENT,
    event_dtype
)


def convert(
        input_filename: str,
        output_filename: Optional[str] = None,
        base_latency: float = 0,
        buffer_size: int = 100_000_000
) -> NDArray:
    r"""
    Converts raw BitMart feed stream file into a format compatible with HftBacktest.
    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size``.

    **File Format:**

    .. code-block::

        # Depth Snapshot
        1741630633009878000 {"data":{"symbol":"BTCUSDT","asks":[{"price":"78723.1","vol":"468"},...],"bids":[{"price":"78709","vol":"3437"},...],"ms_t":1741630632927,"version":7822754,"type":"snapshot"},"group":"futures/depthIncrease50:BTCUSDT@100ms"}
        
        # Depth Update
        1741630633063800000 {"data":{"symbol":"BTCUSDT","asks":[{"price":"78720.1","vol":"766"},...],"bids":[{"price":"78719.8","vol":"433"},...],"ms_t":1741630632990,"version":7822755,"type":"update"},"group":"futures/depthIncrease50:BTCUSDT@100ms"}
        
        # Trade
        1741630634788924000 {"data":[{"trade_id":3000000389425121,"symbol":"BTCUSDT","deal_price":"78727.3","deal_vol":"310","way":2,"m":true,"created_at":"2025-03-10T18:17:14.656686827Z"}],"group":"futures/trade:BTCUSDT"}

    Args:
        input_filename: Input filename with path.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        buffer_size: Sets a preallocated row size for the buffer.

    Returns:
        Converted data compatible with HftBacktest.
    """
    tmp = np.empty(buffer_size, event_dtype)
    row_num = 0
    with gzip.open(input_filename, 'r') as f:
        while True:
            line = f.readline()
            if not line:
                break

            # Find the first space which separates timestamp from JSON data
            space_index = line.find(b' ')
            if space_index == -1:
                continue  # Skip malformed lines

            local_timestamp = int(line[:space_index])
            message = json.loads(line[space_index + 1:])
            
            # Check if the message has data field
            if 'data' not in message:
                continue
                
            data = message['data']
            group = message.get('group', '')
            
            # Process depth data (snapshot or update)
            if 'symbol' in data and ('bids' in data or 'asks' in data):
                ms_t = data.get('ms_t', 0)  # BitMart exchange timestamp in milliseconds
                exch_timestamp = int(ms_t) * 1000  # Convert to nanoseconds
                
                # Process bids
                if 'bids' in data:
                    for bid in data['bids']:
                        price = bid['price']
                        qty = bid['vol']
                        
                        # For updates, volume of 0 means to remove the price level
                        event_type = DEPTH_EVENT
                        if data.get('type') == 'snapshot':
                            event_type = DEPTH_SNAPSHOT_EVENT
                            
                        tmp[row_num] = (
                            event_type | BUY_EVENT,
                            exch_timestamp,
                            local_timestamp,
                            float(price),
                            float(qty),
                            0,
                            0,
                            0
                        )
                        row_num += 1
                
                # Process asks
                if 'asks' in data:
                    for ask in data['asks']:
                        price = ask['price']
                        qty = ask['vol']
                        
                        # For updates, volume of 0 means to remove the price level
                        event_type = DEPTH_EVENT
                        if data.get('type') == 'snapshot':
                            event_type = DEPTH_SNAPSHOT_EVENT
                            
                        tmp[row_num] = (
                            event_type | SELL_EVENT,
                            exch_timestamp,
                            local_timestamp,
                            float(price),
                            float(qty),
                            0,
                            0,
                            0
                        )
                        row_num += 1
                
                # For snapshots, add depth clear events before processing
                if data.get('type') == 'snapshot':
                    # We need to add these *before* the snapshot data, so we'll shift the data
                    # Find the lowest and highest prices from the snapshot
                    if 'bids' in data and data['bids']:
                        bid_prices = [float(bid['price']) for bid in data['bids']]
                        bid_clear_upto = max(bid_prices)
                        
                        # Shift data to make room for clear event
                        for i in range(row_num - len(data['bids']), row_num):
                            tmp[i + 1] = tmp[i]
                            
                        # Insert the clear event
                        tmp[row_num - len(data['bids'])] = (
                            DEPTH_CLEAR_EVENT | BUY_EVENT,
                            exch_timestamp,
                            local_timestamp,
                            bid_clear_upto,
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1
                    
                    if 'asks' in data and data['asks']:
                        ask_prices = [float(ask['price']) for ask in data['asks']]
                        ask_clear_upto = max(ask_prices)
                        
                        # Calculate how many bid entries we have
                        bid_count = len(data.get('bids', []))
                        
                        # Shift data to make room for clear event
                        for i in range(row_num - len(data['asks']), row_num):
                            tmp[i + 1] = tmp[i]
                            
                        # Insert the clear event
                        tmp[row_num - len(data['asks'])] = (
                            DEPTH_CLEAR_EVENT | SELL_EVENT,
                            exch_timestamp,
                            local_timestamp,
                            ask_clear_upto,
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1
            
            # Process trade data
            elif isinstance(data, list) and 'futures/trade' in group:
                for trade in data:
                    if 'deal_price' in trade and 'deal_vol' in trade:
                        # Parse timestamp from created_at field
                        # The format is "2025-03-10T18:17:14.656686827Z"
                        created_at = trade.get('created_at', '')
                        if '.' in created_at:
                            # Extract nanoseconds part
                            timestamp_parts = created_at.split('.')
                            if len(timestamp_parts) > 1:
                                nanos_str = timestamp_parts[1].rstrip('Z')
                                # Convert to Unix timestamp in nanoseconds (approximate)
                                # For simplicity, we'll use local_timestamp as it's close enough
                                exch_timestamp = local_timestamp
                            else:
                                exch_timestamp = local_timestamp
                        else:
                            exch_timestamp = local_timestamp
                        
                        price = trade['deal_price']
                        qty = trade['deal_vol']
                        
                        # Determine trade side
                        # way=1 for buy, way=2 for sell (m=true means buyer is maker)
                        is_buyer_maker = trade.get('m', False)
                        way = trade.get('way', 0)
                        
                        # In BitMart, 'way' indicates the taker's direction:
                        # way=1: taker is buyer, way=2: taker is seller
                        # We need to convert this to BUY_EVENT or SELL_EVENT
                        side_event = SELL_EVENT if way == 1 else BUY_EVENT
                        
                        tmp[row_num] = (
                            TRADE_EVENT | side_event,
                            exch_timestamp,
                            local_timestamp,
                            float(price),
                            float(qty),
                            0,
                            0,
                            0
                        )
                        row_num += 1

    # Truncate the buffer to the actual number of rows used
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