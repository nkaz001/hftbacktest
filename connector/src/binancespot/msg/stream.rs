use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::{Deserialize, Serialize};

use super::{from_str_to_side, from_str_to_status, from_str_to_tif, from_str_to_type};
use crate::utils::{from_str_to_f64, to_lowercase};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "e")]
pub enum MarketEventStream {
    #[serde(rename = "depthUpdate")]
    DepthUpdate(Depth),
    #[serde(rename = "aggTrade")]
    AggTrade(AggTrade),
    #[serde(rename = "trade")]
    Trade(Trade),
    #[serde(rename = "kline")]
    Kline(KlineEvent),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "e")]
pub enum UserEventStream {
    #[serde(rename = "outboundAccountPosition")]
    OutboundAccountPosition(OutboundAccountPosition),
    #[serde(rename = "balanceUpdate")]
    BalanceUpdate(BalanceUpdate),
    #[serde(rename = "executionReport")]
    ExecutionReport(ExecutionReport),
    #[serde(rename = "listStatus")]
    ListStatus(ListStatus),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Result {
    pub result: Option<String>,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MarketStream {
    EventStream(MarketEventStream),
    Result(Result),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum UserStream {
    EventStream(UserDataEvent),
    AuthResponse(AuthResponse),
    SubscribeResponse(SubscribeResponse),
}

#[derive(Debug, Deserialize, Clone)]
pub struct UserDataEvent {
    pub event: UserEventStream,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub id: String,
    pub status: i32,
    pub result: Option<SessionLogonResult>,
    pub rate_limits: Option<Vec<RateLimit>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    pub rate_limit_type: String,
    pub interval: String,
    pub interval_num: u32,
    pub limit: u32,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionLogonResult {
    pub api_key: String,
    pub authorized_since: u64,
    pub connected_since: u64,
    pub return_rate_limits: bool,
    pub server_time: u64,
    pub user_data_stream: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResponse {
    pub id: String,
    pub status: i32,
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeRequest {
    pub id: String,
    pub method: String,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutboundAccountPosition {
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "u")]
    pub last_update_time: i64,
    #[serde(rename = "B")]
    pub balances: Vec<Balance>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Balance {
    #[serde(rename = "a")]
    #[serde(deserialize_with = "to_lowercase")]
    pub asset: String,
    #[serde(rename = "f")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub free: f64,
    #[serde(rename = "l")]
    pub locked: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExecutionReport {
    #[serde(rename = "E")]
    pub event_time: i64, // 事件时间
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
    pub quantity: f64,
    #[serde(rename = "p")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(rename = "P")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub stop_price: f64,
    #[serde(rename = "F")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub iceberg_quantity: f64,
    #[serde(rename = "g")]
    pub order_list_id: i64,
    #[serde(rename = "C")]
    pub original_client_order_id: Option<String>,
    #[serde(rename = "x")]
    pub execution_type: String,
    #[serde(rename = "X")]
    #[serde(deserialize_with = "from_str_to_status")]
    pub order_status: Status,
    #[serde(rename = "r")]
    pub rejection_reason: String,
    #[serde(rename = "i")]
    pub order_id: u64,
    #[serde(rename = "l")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub order_last_filled_quantity: f64,
    #[serde(rename = "z")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub order_filled_accumulated_quantity: f64,
    #[serde(rename = "L")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub last_filled_price: f64,
    #[serde(rename = "n")]
    pub commission: String,
    #[serde(rename = "N")]
    pub commission_asset: Option<String>,
    #[serde(rename = "T")]
    pub order_trade_time: u64,
    pub t: i64,
    #[serde(rename = "I")]
    pub execution_id: u64,
    #[serde(rename = "w")]
    pub is_on_order_book: bool,
    #[serde(rename = "m")]
    pub is_maker: bool,
    #[serde(rename = "M")]
    pub ignore: bool,
    #[serde(rename = "O")]
    pub order_creation_time: u64,
    #[serde(rename = "Z")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cumulative_filled_amount: f64,
    #[serde(rename = "Y")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub last_filled_amount: f64,
    #[serde(rename = "Q")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub quote_order_quantity: f64,
    #[serde(rename = "D")]
    pub trailing_time: Option<i64>,
    #[serde(rename = "d")]
    pub trailing_delta: Option<i64>,
    #[serde(rename = "j")]
    pub strategy_id: Option<i64>,
    #[serde(rename = "J")]
    pub strategy_type: Option<i64>,
    #[serde(rename = "v")]
    pub prevented_match_id: Option<i64>,
    #[serde(rename = "A")]
    pub prevented_quantity: Option<String>,
    #[serde(rename = "B")]
    pub last_prevented_quantity: Option<String>,
    #[serde(rename = "u")]
    pub trade_group_id: Option<i64>,
    #[serde(rename = "U")]
    pub counter_order_id: Option<i64>,
    #[serde(rename = "Cs")]
    pub counter_symbol: Option<String>,
    #[serde(rename = "pl")]
    pub preventedexecution_quantity: Option<String>,
    #[serde(rename = "pL")]
    pub prevented_execution_price: Option<String>,
    #[serde(rename = "pY")]
    pub prevented_execution_quote_qty: Option<String>,
    #[serde(rename = "W")]
    pub working_time: Option<u64>,
    #[serde(rename = "b")]
    pub match_type: Option<String>,
    #[serde(rename = "a")]
    pub allocation_id: Option<i64>,
    #[serde(rename = "k")]
    pub working_floor: Option<String>,
    #[serde(rename = "uS")]
    pub used_sor: Option<bool>,
    #[serde(rename = "V")]
    pub self_trade_prevention_mode: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BalanceUpdate {
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "a")]
    #[serde(deserialize_with = "to_lowercase")]
    pub asset: String,
    #[serde(rename = "d")]
    pub balance_delta: String,
    #[serde(rename = "T")]
    pub clear_time: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListStatus {
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "g")]
    pub order_list_id: u64,
    #[serde(rename = "c")]
    pub contingency_type: String,
    #[serde(rename = "l")]
    pub list_status_type: String,
    #[serde(rename = "L")]
    pub list_order_status: String,
    #[serde(rename = "r")]
    pub rejection_reason: String,
    #[serde(rename = "C")]
    pub list_client_order_id: String,
    #[serde(rename = "T")]
    pub transaction_time: u64,
    #[serde(rename = "O")]
    pub orders: Vec<ListOrder>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListOrder {
    #[serde(rename = "s")]
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub order_id: u64,
    #[serde(rename = "c")]
    pub client_order_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    pub id: String,
    pub method: String,
    pub params: SignParams,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignParams {
    pub api_key: String,
    pub signature: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserDataRequest {
    pub id: String,
    pub method: String,
    pub params: SignParams,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserStreamSubscribeRequest {
    pub id: String,
    pub method: String,
}
