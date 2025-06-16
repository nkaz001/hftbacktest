use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::{Deserialize, Serialize};
use super::{from_str_to_side, from_str_to_status, from_str_to_tif, from_str_to_type};
use crate::utils::{from_str_to_f64, from_str_to_f64_opt, to_lowercase};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub last_update_id: i64,
    pub asks: Vec<(String,String)>,
    pub bids: Vec<(String,String)>,
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
    #[serde(
        rename = "type",
    )]
    #[serde(deserialize_with = "from_str_to_type")]
    pub order_type: OrdType,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    pub working_time: u64,
    pub self_trade_prevention_mode: String,
    pub fills: Vec<Fill>,
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