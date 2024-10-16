use crate::converter::ConverterBase;

use hftbacktest::types::Event;
use hftbacktest::types::{
    BUY_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, SELL_EVENT, TRADE_EVENT,
};
use serde_json::Value;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

// Everything we do below is based on the bybit API which is covered in detail
// at https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook.

// This is almost a carbon copy of the structs used in the bybit connector. In an ideal
// world they would be moved into the main rust lib and then reused here.

fn from_str_to_f64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => s.parse().map_err(Error::custom)?,
        Value::Number(num) => num
            .as_f64()
            .ok_or_else(|| Error::custom("Invalid number"))?,
        _ => return Err(Error::custom("wrong type")),
    })
}

#[derive(Deserialize, Debug)]
pub struct PublicStream {
    pub topic: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: Value,
    pub cts: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct OrderBook {
    #[serde(rename = "b")]
    pub bids: Vec<(String, String)>,
    #[serde(rename = "a")]
    pub asks: Vec<(String, String)>,
}

#[derive(Deserialize, Debug)]
pub struct Trade {
    #[serde(rename = "T")]
    pub ts: i64,
    #[serde(rename = "S")]
    pub side: String,
    #[serde(rename = "v")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub trade_size: f64,
    #[serde(rename = "p")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub trade_price: f64,
}

pub fn bybit_process(
    base: &mut ConverterBase,
    local_ts: i64,
    payload: &str,
) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    let mut result: Vec<Event> = Vec::new();

    let stream = serde_json::from_str::<PublicStream>(payload)?;

    if stream.topic.starts_with("publicTrade") {
        let trades: Vec<Trade> = serde_json::from_value(stream.data)?;
        for trade in trades {
            // adjust if necessary and detect negative latency..
            let mut exch_ts = base.convert_ts(trade.ts);
            let latency = local_ts - exch_ts;
            exch_ts = local_ts + base.latency(latency);

            let event_type = match &*trade.side {
                "Sell" => TRADE_EVENT | SELL_EVENT,
                "Buy" => TRADE_EVENT | BUY_EVENT,
                _ => TRADE_EVENT | SELL_EVENT, // Assume mm trade..
            };

            result.push(Event {
                ev: event_type,
                exch_ts,
                local_ts,
                order_id: 0,
                px: trade.trade_price,
                qty: trade.trade_size,
                ival: 0,
                fval: 0.0,
            })
        }
    } else if stream.topic.starts_with("orderbook") {
        // adjust if necessary and detect negative latency..
        let mut exch_ts = base.convert_ts(stream.cts.ok_or("Missing CTS on order book event")?);
        let latency = local_ts - exch_ts;
        exch_ts = local_ts + base.latency(latency);

        let order_book: OrderBook = serde_json::from_value(stream.data)?;

        if order_book.asks.len() > 0 {
            let (last_ask_px_str, _) = order_book.asks.last().unwrap();

            let ev = match &*stream.event_type {
                "snapshot" => DEPTH_SNAPSHOT_EVENT | SELL_EVENT,
                _ => DEPTH_EVENT | SELL_EVENT,
            };

            // Clear the books if this is a snapshot..
            if stream.event_type == "snapshot" {
                result.push(Event {
                    ev: DEPTH_CLEAR_EVENT | SELL_EVENT,
                    exch_ts,
                    local_ts,
                    order_id: 0,
                    px: last_ask_px_str.parse::<f64>()?,
                    qty: 0.0,
                    ival: 0,
                    fval: 0.0,
                })
            }

            // Insert entries..
            for (ask_px, ask_qty) in order_book.asks {
                result.push(Event {
                    ev,
                    exch_ts,
                    local_ts,
                    order_id: 0,
                    px: ask_px.parse::<f64>()?,
                    qty: ask_qty.parse::<f64>()?,
                    ival: 0,
                    fval: 0.0,
                })
            }
        }

        if order_book.bids.len() > 0 {
            let (last_bid_px_str, _) = order_book.bids.last().unwrap();

            let ev = match &*stream.event_type {
                "snapshot" => DEPTH_SNAPSHOT_EVENT | BUY_EVENT,
                _ => DEPTH_EVENT | BUY_EVENT,
            };

            // Clear the books if this is a snapshot..
            if stream.event_type == "snapshot" {
                result.push(Event {
                    ev: DEPTH_CLEAR_EVENT | BUY_EVENT,
                    exch_ts,
                    local_ts,
                    order_id: 0,
                    px: last_bid_px_str.parse::<f64>()?,
                    qty: 0.0,
                    ival: 0,
                    fval: 0.0,
                })
            }

            // Insert entries..
            for (bid_px, bid_qty) in order_book.bids {
                result.push(Event {
                    ev,
                    exch_ts,
                    local_ts,
                    order_id: 0,
                    px: bid_px.parse::<f64>()?,
                    qty: bid_qty.parse::<f64>()?,
                    ival: 0,
                    fval: 0.0,
                })
            }
        }
    }

    Ok(result)
}
