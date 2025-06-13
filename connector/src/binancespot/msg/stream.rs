use serde::{Deserialize, Serialize};
use crate::utils::{from_str_to_f64, to_lowercase};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "e")]
pub enum EventStream {
    #[serde(rename = "depthUpdate")]
    DepthUpdate(Depth),
    #[serde(rename = "aggTrade")]
    AggTrade(AggTrade),
    #[serde(rename = "trade")]
    Trade(Trade),
    #[serde(rename = "kline")]
    Kline(KlineEvent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Result {
    pub result: Option<String>,
    pub id: String,     
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Stream {
    EventStream(EventStream),
    Result(Result),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Depth {
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    // 币本位
    // #[serde(rename = "ps")]
    // pub pair: String,
    #[serde(rename = "U")]
    pub first_update_id: i64,
    #[serde(rename = "u")]
    pub last_update_id: i64,
    #[serde(rename = "b")]
    pub bids: Vec<(String, String)>,
    #[serde(rename = "a")]
    pub asks: Vec<(String, String)>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AggTrade {
    #[serde(rename = "E")]
    pub event_time: i64, 
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String, 
    #[serde(rename = "a")]
    pub aggregated_trade_id: i64, 
    #[serde(rename = "p")]
    pub price: String, 
    #[serde(rename = "q")]
    pub quantity: String, 
    #[serde(rename = "f")]
    pub first_trade_id: i64, 
    #[serde(rename = "l")]
    pub last_trade_id: i64, 
    #[serde(rename = "T")]
    pub filled_time: i64, 
    #[serde(rename = "m")]
    pub is_market_maker: bool, 
    #[serde(rename = "M")]
    pub ignore: bool, 
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Trade {
    #[serde(rename = "E")]
    pub event_time: i64, 
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String, 
    #[serde(rename = "t")]
    pub trade_id: i64, 
    #[serde(rename = "p")]
    pub price: String, 
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "T")]
    pub trade_time: i64,
    #[serde(rename = "m")]
    pub is_market_maker: bool, 
    #[serde(rename = "M")]
    pub ignore: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KlineEvent {
    #[serde(rename = "E")]
    pub event_time: i64, 
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "k")]
    pub kline: Kline,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Kline {
    #[serde(rename = "t")]
    pub start_time: i64, 
    #[serde(rename = "T")]
    pub end_time: i64, 
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String, 
    #[serde(rename = "i")]
    pub interval: String,
    #[serde(rename = "f")]
    pub first_trade_id: i64, 
    #[serde(rename = "L")]
    pub last_trade_id: i64, 
    #[serde(rename = "o")]
    pub open_price: String,
    #[serde(rename = "c")]
    pub close_price: String, 
    #[serde(rename = "h")]
    pub high_price: String, 
    #[serde(rename = "l")]
    pub low_price: String, 
    #[serde(rename = "v")]
    pub volume: String, 
    #[serde(rename = "n")]
    pub trade_count: i64, 
    #[serde(rename = "x")]
    pub is_closed: bool, 
    #[serde(rename = "q")]
    pub quote_asset_volume: String, 
    #[serde(rename = "V")]
    pub taker_buy_base_asset_volume: String, 
    #[serde(rename = "Q")]
    pub taker_buy_quote_asset_volume: String, 
    #[serde(rename = "B")]
    pub ignore: String, 
}