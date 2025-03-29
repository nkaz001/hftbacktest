import gzip
import json
import datetime
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
    timestamp_mul = 1000000  # Multiplier to convert ms to ns
    
    tmp = np.empty(buffer_size, event_dtype)
    row_num = 0
    with gzip.open(input_filename, 'r') as f:
        while True:
            line = f.readline()
            if not line:
                break

            try:
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
                    exch_timestamp = int(ms_t) * timestamp_mul  # Convert to nanoseconds

                    # For snapshots, add depth clear events before processing
                    if data.get('type') == 'snapshot':
                        # We need to add these *before* the snapshot data
                        if 'bids' in data and data['bids']:
                            bid_prices = [float(bid['price']) for bid in data['bids']]
                            # For bids, the clear should be up to the lowest price (max price for comparison)
                            bid_clear_upto = min(bid_prices)
                                
                            # Insert the clear event
                            tmp[row_num] = (
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
                            # For asks, the clear should be up to the highest price (min price for comparison)
                            ask_clear_upto = max(ask_prices)
                                
                            # Insert the clear event
                            tmp[row_num] = (
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
                
                # Process trade data
                elif isinstance(data, list) and 'futures/trade' in group:
                    for trade in data:
                        if 'deal_price' in trade and 'deal_vol' in trade:
                            # Parse timestamp from created_at field
                            created_at = trade.get('created_at', '')
                            if created_at:
                                try:
                                    # Format: "2025-03-10T18:17:14.656686827Z"
                                    # Convert to nanoseconds
                                    dt = datetime.datetime.strptime(created_at.split('.')[0], "%Y-%m-%dT%H:%M:%S")
                                    # Set to UTC
                                    dt = dt.replace(tzinfo=datetime.timezone.utc)
                                    nanos_part = created_at.split('.')[1].rstrip('Z')
                                    nanos = int(nanos_part.ljust(9, '0')[:9])  # Ensure 9 digits for nanos
                                    
                                    # Convert to Unix timestamp in nanoseconds
                                    exch_timestamp = int(dt.timestamp()) * 1000000000 + nanos
                                except (ValueError, IndexError):
                                    # Fallback to ms_t if available, otherwise use local_timestamp
                                    exch_timestamp = int(trade.get('ms_t', local_timestamp // 1000)) * timestamp_mul
                            else:
                                # Fallback to ms_t if available, otherwise use local_timestamp
                                exch_timestamp = int(trade.get('ms_t', local_timestamp // 1000)) * timestamp_mul
                            
                            price = trade['deal_price']
                            qty = trade['deal_vol']
                            
                            # Determine trade side using the 'way' and 'm' fields
                            way = trade.get('way', 0)
                            is_buyer_maker = trade.get('m', False)
                            
                            # BitMart way field meanings:
                            # 1 = buy_open_long sell_open_short
                            # 2 = buy_open_long sell_close_long
                            # 3 = buy_close_short sell_open_short
                            # 4 = buy_close_short sell_close_long
                            # 5 = sell_open_short buy_open_long
                            # 6 = sell_open_short buy_close_short
                            # 7 = sell_close_long buy_open_long
                            # 8 = sell_close_long buy_close_short
                            
                            # The 'm' field: true is "buyer is maker", false is "seller is maker"
                            # For HftBacktest, we need to indicate the initiator's side (the taker)
                            
                            # Determine the taker side based on is_buyer_maker
                            # If buyer is maker (m=true), then seller is taker -> SELL_EVENT
                            # If seller is maker (m=false), then buyer is taker -> BUY_EVENT
                            side_event = SELL_EVENT if is_buyer_maker else BUY_EVENT
                            
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
            except (json.JSONDecodeError, ValueError, KeyError, IndexError) as e:
                print(f"Error processing line: {e}")
                continue

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