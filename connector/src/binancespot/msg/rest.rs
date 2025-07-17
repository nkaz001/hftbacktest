use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::{Deserialize, Serialize};

use super::{from_str_to_side, from_str_to_status, from_str_to_tif, from_str_to_type};
use crate::utils::{from_str_to_f64, to_lowercase};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub last_update_id: i64,
    pub asks: Vec<(String, String)>,
    pub bids: Vec<(String, String)>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum OrderResponseResult {
    Ok(OrderResponse),
    Err(ErrorResponse),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    pub order_id: u64,
    pub order_list_id: i64,
    pub client_order_id: String,
    pub transact_time: i64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub orig_qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub executed_qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub orig_quote_order_qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cummulative_quote_qty: f64,
    #[serde(deserialize_with = "from_str_to_status")]
    pub status: Status,
    #[serde(deserialize_with = "from_str_to_tif")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "type")]
    #[serde(deserialize_with = "from_str_to_type")]
    pub order_type: OrdType,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    pub working_time: i64,
    pub self_trade_prevention_mode: String,
    pub fills: Vec<Fill>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub code: i64,
    pub msg: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum CancelOrderResponseResult {
    Ok(CancelOrderResponse),
    Err(ErrorResponse),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderResponse {
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    pub order_id: u64,
    pub order_list_id: i64,
    pub orig_client_order_id: String,
    pub client_order_id: String,
    pub transact_time: i64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub orig_qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub executed_qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub orig_quote_order_qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cummulative_quote_qty: f64,
    #[serde(deserialize_with = "from_str_to_status")]
    pub status: Status,
    #[serde(deserialize_with = "from_str_to_tif")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "type")]
    #[serde(deserialize_with = "from_str_to_type")]
    pub order_type: OrdType,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    pub self_trade_prevention_mode: String,
}

impl From<CancelOrderResponse> for OrderResponse {
    fn from(order: CancelOrderResponse) -> Self {
        Self {
            symbol: order.symbol,
            order_id: order.order_id,
            order_list_id: order.order_list_id,
            client_order_id: order.client_order_id,
            transact_time: order.transact_time,
            price: order.price,
            orig_qty: order.orig_qty,
            executed_qty: order.executed_qty,
            orig_quote_order_qty: order.orig_quote_order_qty,
            cummulative_quote_qty: order.cummulative_quote_qty,
            status: order.status,
            time_in_force: order.time_in_force,
            order_type: order.order_type,
            side: order.side,
            working_time: order.transact_time,
            self_trade_prevention_mode: order.self_trade_prevention_mode,
            fills: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub qty: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub commission: f64,
    #[serde(deserialize_with = "to_lowercase")]
    pub commission_asset: String,
    pub trade_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfomation {
    pub maker_commission: u64,
    pub taker_commission: u64,
    pub buyer_commission: u64,
    pub seller_commission: u64,
    pub commission_rates: CommissionRates,
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub can_deposit: bool,
    pub brokered: bool,
    pub require_self_trade_prevention: bool,
    pub prevent_sor: bool,
    pub update_time: i64,
    pub account_type: String, // Consider using an enum if account types are fixed
    pub balances: Vec<BalanceEntry>,
    pub permissions: Vec<String>, // Consider using an enum if permissions are fixed
    pub uid: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommissionRates {
    #[serde(deserialize_with = "from_str_to_f64")]
    pub maker: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub taker: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub buyer: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub seller: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BalanceEntry {
    #[serde(deserialize_with = "to_lowercase")]
    pub asset: String,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub free: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub locked: f64,
}
