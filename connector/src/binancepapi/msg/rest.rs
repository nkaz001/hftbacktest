use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::Deserialize;

use super::{from_str_to_side, from_str_to_status, from_str_to_tif, from_str_to_type};
use crate::utils::{from_str_to_f64, from_str_to_f64_opt, to_lowercase};

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum OrderResponseResult {
    Ok(OrderResponse),
    Err(ErrorResponse),
}

/// Order response for UM futures orders via PAPI
/// Many fields are made optional/defaulted because PAPI responses may differ from standard futures.
#[derive(Deserialize, Debug)]
pub struct OrderResponse {
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "cumQty")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cum_qty: f64,
    #[serde(rename = "cumQuote")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub cum_quote: Option<f64>,
    #[serde(rename = "cumBase")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub cum_base: Option<f64>,
    #[serde(rename = "executedQty")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub executed_qty: f64,
    #[serde(rename = "orderId")]
    pub order_id: i64,
    #[serde(rename = "avgPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub avg_price: Option<f64>,
    #[serde(rename = "origQty")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub orig_qty: f64,
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(rename = "reduceOnly")]
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "positionSide")]
    #[serde(default)]
    pub position_side: String,
    #[serde(deserialize_with = "from_str_to_status")]
    pub status: Status,
    #[serde(rename = "stopPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub stop_price: Option<f64>,
    #[serde(rename = "closePosition")]
    #[serde(default)]
    pub close_position: bool,
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(default)]
    pub pair: Option<String>,
    #[serde(rename = "timeInForce")]
    #[serde(deserialize_with = "from_str_to_tif")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "type")]
    #[serde(deserialize_with = "from_str_to_type")]
    pub ty: OrdType,
    #[serde(rename = "origType")]
    #[serde(default)]
    pub orig_type: Option<String>,
    #[serde(rename = "activatePrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub activate_price: Option<f64>,
    #[serde(rename = "priceRate")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub price_rate: Option<f64>,
    #[serde(rename = "updateTime")]
    #[serde(default)]
    pub update_time: i64,
    #[serde(rename = "workingType")]
    #[serde(default)]
    pub working_type: String,
    #[serde(rename = "priceProtect")]
    #[serde(default)]
    pub price_protect: bool,
    #[serde(rename = "priceMatch")]
    #[serde(default)]
    pub price_match: String,
    #[serde(rename = "selfTradePreventionMode")]
    #[serde(default)]
    pub self_trade_prevention_mode: String,
    #[serde(rename = "goodTillDate")]
    #[serde(default)]
    pub good_till_date: i64,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub code: i64,
    pub msg: String,
}

/// PAPI Balance response
#[derive(Deserialize, Debug)]
pub struct Balance {
    pub asset: String,
    #[serde(rename = "totalWalletBalance")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub total_wallet_balance: f64,
    #[serde(rename = "crossMarginAsset")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cross_margin_asset: f64,
    #[serde(rename = "crossMarginBorrowed")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cross_margin_borrowed: f64,
    #[serde(rename = "crossMarginFree")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cross_margin_free: f64,
    #[serde(rename = "crossMarginInterest")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cross_margin_interest: f64,
    #[serde(rename = "crossMarginLocked")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cross_margin_locked: f64,
    #[serde(rename = "umWalletBalance")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub um_wallet_balance: f64,
    #[serde(rename = "umUnrealizedPNL")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub um_unrealized_pnl: f64,
    #[serde(rename = "cmWalletBalance")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cm_wallet_balance: f64,
    #[serde(rename = "cmUnrealizedPNL")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cm_unrealized_pnl: f64,
    #[serde(rename = "updateTime")]
    pub update_time: i64,
    #[serde(rename = "negativeBalance")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub negative_balance: f64,
}

/// PAPI UM Position Information
/// Some fields are optional because PAPI portfolio margin responses differ from standard futures.
#[derive(Deserialize, Debug)]
pub struct UmPositionRisk {
    #[serde(rename = "entryPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub entry_price: f64,
    #[serde(rename = "breakEvenPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub breakeven_price: f64,
    #[serde(rename = "marginType")]
    #[serde(default)]
    pub margin_type: String,
    #[serde(rename = "isAutoAddMargin")]
    #[serde(default)]
    pub is_auto_add_margin: String,
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub leverage: f64,
    #[serde(rename = "liquidationPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub liquidation_price: f64,
    #[serde(rename = "markPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub mark_price: f64,
    #[serde(rename = "maxNotionalValue")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub max_notional_value: f64,
    #[serde(rename = "positionAmt")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_amount: f64,
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub notional: f64,
    #[serde(deserialize_with = "to_lowercase")]
    pub symbol: String,
    #[serde(rename = "unRealizedProfit")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub unrealized_pnl: f64,
    #[serde(rename = "positionSide")]
    #[serde(default)]
    pub position_side: String,
    #[serde(rename = "updateTime")]
    #[serde(default)]
    pub update_time: i64,
}

/// PAPI Account Information
#[derive(Deserialize, Debug)]
pub struct AccountInfo {
    #[serde(rename = "uniMMR")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub uni_mmr: f64,
    #[serde(rename = "accountEquity")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub account_equity: f64,
    #[serde(rename = "actualEquity")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub actual_equity: f64,
    #[serde(rename = "accountInitialMargin")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub account_initial_margin: f64,
    #[serde(rename = "accountMaintMargin")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub account_maint_margin: f64,
    #[serde(rename = "accountStatus")]
    pub account_status: String,
    #[serde(rename = "updateTime")]
    pub update_time: i64,
}

#[derive(Deserialize, Debug)]
pub struct Depth {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: i64,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "T")]
    pub transaction_time: i64,
    pub bids: Vec<(String, String)>,
    pub asks: Vec<(String, String)>,
}

#[derive(Deserialize, Debug)]
pub struct ListenKey {
    #[serde(rename = "listenKey")]
    pub listen_key: String,
}
