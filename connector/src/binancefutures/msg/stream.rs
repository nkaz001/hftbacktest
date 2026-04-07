use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::Deserialize;

use super::{from_str_to_side, from_str_to_status, from_str_to_tif, from_str_to_type};
use crate::utils::{from_str_to_f64, to_lowercase};

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum Stream {
    EventStream(EventStream),
    Result(Result),
}

#[derive(Deserialize, Debug)]
pub struct Result {
    pub result: Option<String>,
    pub id: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "e")]
pub enum EventStream {
    #[serde(rename = "depthUpdate")]
    DepthUpdate(Depth),
    #[serde(rename = "trade")]
    Trade(Trade),
    #[serde(rename = "ORDER_TRADE_UPDATE")]
    OrderTradeUpdate(OrderTradeUpdate),
    #[serde(rename = "TRADE_LITE")]
    TradeLite(TradeLite),
    #[serde(rename = "ACCOUNT_UPDATE")]
    AccountUpdate(AccountUpdate),
    #[serde(rename = "bookTicker")]
    BookTicker(BookTicker),
    #[serde(rename = "listenKeyExpired")]
    ListenKeyExpired(ListenKeyStream),
}

#[derive(Deserialize, Debug)]
pub struct BookTicker {
    #[serde(rename = "T")]
    pub transaction_time: i64,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "b")]
    pub bid_price: String,
    #[serde(rename = "B")]
    pub bid_qty: String,
    #[serde(rename = "a")]
    pub ask_price: String,
    #[serde(rename = "A")]
    pub ask_qty: String,
}

#[derive(Deserialize, Debug)]
pub struct Depth {
    #[serde(rename = "T")]
    pub transaction_time: i64,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    // for Coin-M futures
    // #[serde(rename = "ps")]
    // pub pair: String,
    #[serde(rename = "U")]
    pub first_update_id: i64,
    #[serde(rename = "u")]
    pub last_update_id: i64,
    #[serde(rename = "pu")]
    pub prev_update_id: i64,
    #[serde(rename = "b")]
    pub bids: Vec<(String, String)>,
    #[serde(rename = "a")]
    pub asks: Vec<(String, String)>,
}

#[derive(Deserialize, Debug)]
pub struct Trade {
    #[serde(rename = "T")]
    pub transaction_time: i64,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "t")]
    pub id: i64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub qty: String,
    #[serde(rename = "X")]
    pub type_: String,
    #[serde(rename = "m")]
    pub is_the_buyer_the_market_maker: bool,
}

#[derive(Deserialize, Debug)]
pub struct TradeLite {
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "T")]
    pub transaction_time: i64,
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "q")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub qty: f64,
    #[serde(rename = "p")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(rename = "m")]
    pub is_this_trade_the_market_maker: bool,
    #[serde(rename = "c")]
    pub client_order_id: String,
    #[serde(rename = "S")]
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "L")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub last_filled_price: f64,
    #[serde(rename = "l")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub last_filled_qty: f64,
    #[serde(rename = "t")]
    pub trade_id: i64,
    #[serde(rename = "i")]
    pub order_id: i64,
}

#[derive(Deserialize, Debug)]
pub struct AccountUpdate {
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "T")]
    pub transaction_time: i64,
    #[serde(rename = "a")]
    pub account: Account,
}

#[derive(Deserialize, Debug)]
pub struct Account {
    #[serde(rename = "m")]
    pub ev_reason: String,
    #[serde(rename = "B")]
    pub balance: Vec<Balance>,
    #[serde(rename = "P")]
    pub position: Vec<Position>,
}

#[derive(Deserialize, Debug)]
pub struct Balance {
    #[serde(rename = "a")]
    pub asset: String,
    #[serde(rename = "wb")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub wallet_balance: f64,
    #[serde(rename = "cw")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cross_wallet_balance: f64,
    #[serde(rename = "bc")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub balance_change: f64,
}

#[derive(Deserialize, Debug)]
pub struct Position {
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "pa")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_amount: f64,
    #[serde(rename = "ep")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub entry_price: f64,
    #[serde(rename = "bep")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub breakeven_price: f64,
    #[serde(rename = "cr")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub prefee_accumulated_realized: f64,
    #[serde(rename = "up")]
    pub unrealized_pnl: String,
    #[serde(rename = "mt")]
    pub margin_type: String,
    #[serde(rename = "iw")]
    pub isolated_wallet: Option<String>,
    #[serde(rename = "ps")]
    pub position_side: String,
}

#[derive(Deserialize, Debug)]
pub struct OrderTradeUpdate {
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "T")]
    pub transaction_time: i64,
    #[serde(rename = "o")]
    pub order: Order,
}

#[derive(Deserialize, Debug)]
pub struct Order {
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "c")]
    pub client_order_id: String,
    #[serde(rename = "S")]
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "o")]
    #[serde(deserialize_with = "from_str_to_type")]
    pub order_type: OrdType,
    #[serde(rename = "f")]
    #[serde(deserialize_with = "from_str_to_tif")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "q")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub original_qty: f64,
    #[serde(rename = "p")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub original_price: f64,
    #[serde(rename = "ap")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub average_price: f64,
    #[serde(rename = "sp")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub stop_price: f64,
    #[serde(rename = "x")]
    pub execution_type: String,
    #[serde(rename = "X")]
    #[serde(deserialize_with = "from_str_to_status")]
    pub order_status: Status,
    #[serde(rename = "i")]
    pub order_id: i64,
    #[serde(rename = "l")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub order_last_filled_qty: f64,
    #[serde(rename = "z")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub order_filled_accumulated_qty: f64,
    #[serde(rename = "L")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub last_filled_price: f64,
    // #[serde(rename = "N")]
    // pub commission_asset: Option<String>,
    // #[serde(rename = "n")]
    // pub commission: Option<String>,
    #[serde(rename = "T")]
    pub order_trade_time: i64,
    #[serde(rename = "t")]
    pub trade_id: i64,
    // #[serde(rename = "b")]
    // pub bid_notional: String,
    // #[serde(rename = "a")]
    // pub ask_notional: String,
    #[serde(rename = "m")]
    #[serde(default)]
    pub is_maker_side: bool,
    /// Realized profit from this trade
    #[serde(rename = "rp")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub realized_profit: f64,
    /// Commission asset
    #[serde(rename = "N")]
    #[serde(default)]
    pub commission_asset: Option<String>,
    /// Commission amount
    #[serde(rename = "n")]
    #[serde(default)]
    pub commission: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ListenKey {
    #[serde(rename = "listenKey")]
    pub listen_key: String,
}

#[derive(Deserialize, Debug)]
pub struct ListenKeyStream {
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "listenKey")]
    pub listen_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real Binance Futures bookTicker payload from the WebSocket docs.
    const BOOKTICKER_JSON: &str = r#"{
        "e": "bookTicker",
        "u": 400900217,
        "E": 1568014460893,
        "T": 1568014460891,
        "s": "BTCUSDT",
        "b": "25001.50",
        "B": "1.200",
        "a": "25002.00",
        "A": "0.800"
    }"#;

    #[test]
    fn parse_bookticker_struct() {
        let bt: BookTicker = serde_json::from_str(BOOKTICKER_JSON).unwrap();
        assert_eq!(bt.symbol, "btcusdt");
        assert_eq!(bt.transaction_time, 1568014460891);
        assert_eq!(bt.event_time, 1568014460893);
        assert_eq!(bt.bid_price, "25001.50");
        assert_eq!(bt.bid_qty, "1.200");
        assert_eq!(bt.ask_price, "25002.00");
        assert_eq!(bt.ask_qty, "0.800");
    }

    #[test]
    fn parse_bookticker_via_event_stream_enum() {
        let ev: EventStream = serde_json::from_str(BOOKTICKER_JSON).unwrap();
        match ev {
            EventStream::BookTicker(bt) => {
                assert_eq!(bt.symbol, "btcusdt");
                assert_eq!(bt.bid_price, "25001.50");
                assert_eq!(bt.ask_price, "25002.00");
            }
            other => panic!("expected BookTicker, got {:?}", other),
        }
    }

    #[test]
    fn parse_bookticker_via_stream_envelope() {
        let stream: Stream = serde_json::from_str(BOOKTICKER_JSON).unwrap();
        match stream {
            Stream::EventStream(EventStream::BookTicker(bt)) => {
                assert_eq!(bt.symbol, "btcusdt");
                assert_eq!(bt.transaction_time, 1568014460891);
            }
            other => panic!("expected Stream::EventStream(BookTicker), got {:?}", other),
        }
    }

    #[test]
    fn parse_bookticker_small_prices() {
        // Edge case: very small altcoin prices
        let json = r#"{
            "e": "bookTicker", "u": 1,
            "E": 1000, "T": 999,
            "s": "SIRENUSDT",
            "b": "0.00234", "B": "100000.0",
            "a": "0.00235", "A": "50000.0"
        }"#;
        let bt: BookTicker = serde_json::from_str(json).unwrap();
        assert_eq!(bt.symbol, "sirenusdt");
        assert_eq!(bt.bid_price, "0.00234");
        assert_eq!(bt.ask_price, "0.00235");
    }

    /// Ensure depthUpdate still parses correctly alongside bookTicker.
    #[test]
    fn parse_depth_update_not_broken() {
        let json = r#"{
            "e": "depthUpdate",
            "E": 1000, "T": 999,
            "s": "BTCUSDT",
            "U": 100, "u": 105, "pu": 99,
            "b": [["25000.0", "1.0"]],
            "a": [["25001.0", "2.0"]]
        }"#;
        let ev: EventStream = serde_json::from_str(json).unwrap();
        assert!(matches!(ev, EventStream::DepthUpdate(_)));
    }
}
