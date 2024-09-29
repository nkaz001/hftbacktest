use std::{collections::HashMap, fmt, fmt::Debug};

use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::{
    de,
    de::{Error, Unexpected, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
};

use crate::utils::{from_str_to_f64, from_str_to_f64_opt, from_str_to_i64};

struct SideVisitor;

impl<'de> Visitor<'de> for SideVisitor {
    type Value = Side;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing \"Buy\" or \"Sell\"")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s {
            "Buy" => Ok(Side::Buy),
            "Sell" => Ok(Side::Sell),
            s => Err(Error::invalid_value(Unexpected::Other(s), &"Buy or Sell")),
        }
    }
}

fn from_str_to_side<'de, D>(deserializer: D) -> Result<Side, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(SideVisitor)
}

struct OrdTypeVisitor;

impl<'de> Visitor<'de> for OrdTypeVisitor {
    type Value = OrdType;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing \"Market\" or \"Limit\"")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s {
            "Market" => Ok(OrdType::Market),
            "Limit" => Ok(OrdType::Limit),
            s => Err(Error::invalid_value(
                Unexpected::Other(s),
                &"Market or Limit",
            )),
        }
    }
}

fn from_str_to_ord_type<'de, D>(deserializer: D) -> Result<OrdType, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(OrdTypeVisitor)
}

struct TimeInForceVisitor;

impl<'de> Visitor<'de> for TimeInForceVisitor {
    type Value = TimeInForce;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing \"IOC\" or \"GTC\"")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s {
            "IOC" => Ok(TimeInForce::IOC),
            "GTC" => Ok(TimeInForce::GTC),
            "FOK" => Ok(TimeInForce::FOK),
            "PostOnly" => Ok(TimeInForce::GTX),
            s => Err(Error::invalid_value(Unexpected::Other(s), &"IOC or GTC")),
        }
    }
}

fn from_str_to_time_in_force<'de, D>(deserializer: D) -> Result<TimeInForce, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(TimeInForceVisitor)
}

struct StatusVisitor;

impl<'de> Visitor<'de> for StatusVisitor {
    type Value = Status;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing \"IOC\" or \"GTC\"")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s {
            "New" => Ok(Status::New),
            "PartiallyFilled" => Ok(Status::PartiallyFilled),
            "Untriggered" => Ok(Status::Unsupported),
            "Rejected" => Ok(Status::Expired),
            "PartiallyFilledCanceled" => Ok(Status::Canceled),
            "Filled" => Ok(Status::Filled),
            "Cancelled" => Ok(Status::Canceled),
            "Triggered" => Ok(Status::Unsupported),
            "Deactivated" => Ok(Status::Unsupported),
            s => Err(Error::invalid_value(Unexpected::Other(s), &"IOC or GTC")),
        }
    }
}

fn from_str_to_status<'de, D>(deserializer: D) -> Result<Status, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(StatusVisitor)
}

#[derive(Serialize, Debug)]
pub struct Op {
    pub req_id: String,
    pub op: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct OpResponse {
    pub success: Option<bool>,
    pub ret_msg: Option<String>,
    pub conn_id: Option<String>,
    pub op: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub req_id: Option<String>,
    #[serde(rename = "failTopics", default)]
    pub fail_topics: Vec<String>,
    #[serde(rename = "successTopics", default)]
    pub success_topics: Vec<String>,
    #[serde(rename = "type")]
    pub ty: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum PublicStreamMsg {
    Topic(PublicStream),
    Op(OpResponse),
}

#[derive(Deserialize, Debug)]
pub struct PublicStream {
    pub topic: String,
    pub ts: i64,
    pub data: serde_json::Value,
    pub cts: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct OrderBook {
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "b")]
    pub bids: Vec<(String, String)>,
    #[serde(rename = "a")]
    pub asks: Vec<(String, String)>,
    #[serde(rename = "u")]
    pub update_id: i64,
    pub seq: i64,
}

#[derive(Deserialize, Debug)]
pub struct Trade {
    #[serde(rename = "T")]
    pub ts: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "S")]
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "v")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub trade_size: f64,
    #[serde(rename = "p")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub trade_price: f64,
    #[serde(rename = "L")]
    pub direction: String,
    #[serde(rename = "i")]
    pub trade_id: String,
    #[serde(rename = "BT")]
    pub block_trade: bool,
    #[serde(rename = "mP")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub mark_price: Option<f64>,
    #[serde(rename = "iP")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub index_price: Option<f64>,
    #[serde(rename = "mIv")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub mark_iv: Option<f64>,
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub iv: Option<f64>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum PrivateStreamMsg {
    Topic(PrivateStreamTopicMsg),
    Op(OpResponse),
}

#[derive(Deserialize, Debug)]
#[serde(tag = "topic")]
pub enum PrivateStreamTopicMsg {
    #[serde(rename = "position")]
    Position(PrivateStream<Vec<Position>>),
    #[serde(rename = "execution")]
    Execution(PrivateStream<Vec<Execution>>),
    #[serde(rename = "execution.fast")]
    FastExecution(PrivateStream<Vec<FastExecution>>),
    #[serde(rename = "order")]
    Order(PrivateStream<Vec<PrivateOrder>>),
}

#[derive(Deserialize, Debug)]
#[serde(bound = "for <'a> T: Deserialize<'a>")]
pub struct PrivateStream<T>
where
    for<'a> T: Deserialize<'a> + Debug,
{
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "creationTime")]
    pub creation_time: i64,
    pub data: T,
}

#[derive(Deserialize, Debug)]
pub struct Position {
    #[serde(rename = "positionIdx")]
    pub position_idx: i64,
    #[serde(rename = "tradeMode")]
    pub trade_mode: i64,
    #[serde(rename = "riskId")]
    pub risk_id: i64,
    #[serde(rename = "riskLimitValue")]
    pub risk_limit_value: String,
    pub symbol: String,
    pub side: String,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub size: f64,
    #[serde(rename = "entryPrice", default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub entry_price: Option<f64>,
    pub leverage: String,
    #[serde(rename = "positionValue")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_value: f64,
    #[serde(rename = "positionBalance")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_balance: f64,
    #[serde(rename = "markPrice")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub mark_price: f64,
    #[serde(rename = "positionIM")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_im: f64,
    #[serde(rename = "positionMM")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_mm: f64,
    #[serde(rename = "takeProfit")]
    pub take_profit: String,
    #[serde(rename = "stopLoss")]
    pub stop_loss: String,
    #[serde(rename = "trailingStop")]
    pub trailing_stop: String,
    #[serde(rename = "unrealisedPnl")]
    pub unrealised_pnl: String,
    #[serde(rename = "curRealisedPnl")]
    pub cur_realised_pnl: String,
    #[serde(rename = "cumRealisedPnl")]
    pub cum_realised_pnl: String,
    #[serde(rename = "sessionAvgPrice")]
    pub session_avg_price: String,
    #[serde(rename = "createdTime")]
    pub created_time: String,
    #[serde(rename = "updatedTime")]
    pub updated_time: String,
    #[serde(rename = "tpslMode")]
    pub tpsl_mode: String,
    #[serde(rename = "liqPrice", default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub liq_price: Option<f64>,
    #[serde(rename = "bustPrice", default)]
    #[serde(deserialize_with = "from_str_to_f64_opt")]
    pub bust_price: Option<f64>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(rename = "positionStatus")]
    pub position_status: String,
    #[serde(rename = "adlRankIndicator")]
    pub adl_rank_indicator: i64,
    #[serde(rename = "autoAddMargin")]
    pub auto_add_margin: i64,
    #[serde(rename = "leverageSysUpdatedTime")]
    pub leverage_sys_updated_time: String,
    #[serde(rename = "mmrSysUpdatedTime")]
    pub mmr_sys_updated_time: String,
    pub seq: i64,
    #[serde(rename = "isReduceOnly")]
    pub is_reduce_only: bool,
}

#[derive(Deserialize, Debug)]
pub struct Execution {
    pub category: String,
    pub symbol: String,
    #[serde(rename = "execFee")]
    pub exec_fee: String,
    #[serde(rename = "execId")]
    pub exec_id: String,
    #[serde(rename = "execPrice")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub exec_price: f64,
    #[serde(rename = "execQty")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub exec_qty: f64,
    #[serde(rename = "execType")]
    pub exec_type: String,
    #[serde(rename = "execValue")]
    pub exec_value: String,
    #[serde(rename = "isMaker")]
    pub is_maker: bool,
    #[serde(rename = "feeRate")]
    pub fee_rate: String,
    #[serde(rename = "tradeIv")]
    pub trade_iv: String,
    #[serde(rename = "markIv")]
    pub mark_iv: String,
    #[serde(rename = "blockTradeId")]
    pub block_trade_id: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "indexPrice")]
    pub index_price: String,
    #[serde(rename = "underlyingPrice")]
    pub underlying_price: String,
    #[serde(rename = "leavesQty")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub leaves_qty: f64,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
    #[serde(rename = "orderPrice")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub order_price: f64,
    #[serde(rename = "orderQty")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub order_qty: f64,
    #[serde(rename = "orderType")]
    pub order_type: String,
    #[serde(rename = "stopOrderType")]
    pub stop_order_type: String,
    pub side: String,
    #[serde(rename = "execTime")]
    #[serde(deserialize_with = "from_str_to_i64")]
    pub exec_time: i64,
    #[serde(rename = "isLeverage")]
    pub is_leverage: String,
    #[serde(rename = "closedSize")]
    pub closed_size: String,
    pub seq: i64,
}

#[derive(Deserialize, Debug)]
pub struct FastExecution {
    pub category: String,
    pub symbol: String,
    #[serde(rename = "execId")]
    pub exec_id: String,
    #[serde(rename = "execPrice")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub exec_price: f64,
    #[serde(rename = "execQty")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub exec_qty: f64,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "execTime")]
    #[serde(deserialize_with = "from_str_to_i64")]
    pub exec_time: i64,
    pub seq: i64,
}

#[derive(Deserialize, Debug)]
pub struct PrivateOrder {
    pub symbol: String,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "orderType")]
    #[serde(deserialize_with = "from_str_to_ord_type")]
    pub order_type: OrdType,
    #[serde(rename = "cancelType")]
    pub cancel_type: String,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub qty: f64,
    #[serde(rename = "orderIv")]
    pub order_iv: String,
    #[serde(rename = "timeInForce")]
    #[serde(deserialize_with = "from_str_to_time_in_force")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "orderStatus")]
    #[serde(deserialize_with = "from_str_to_status")]
    pub order_status: Status,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
    #[serde(rename = "lastPriceOnCreated")]
    pub last_price_on_created: String,
    #[serde(rename = "reduceOnly")]
    pub reduce_only: bool,
    #[serde(rename = "leavesQty")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub leaves_qty: f64,
    #[serde(rename = "leavesValue")]
    pub leaves_value: String,
    #[serde(rename = "cumExecQty")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cum_exec_qty: f64,
    #[serde(rename = "cumExecValue")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub cum_exec_value: f64,
    #[serde(rename = "avgPrice")]
    pub avg_price: String,
    #[serde(rename = "blockTradeId")]
    pub block_trade_id: String,
    #[serde(rename = "positionIdx")]
    pub position_idx: i64,
    #[serde(rename = "cumExecFee")]
    pub cum_exec_fee: String,
    #[serde(rename = "createdTime")]
    #[serde(deserialize_with = "from_str_to_i64")]
    pub created_time: i64,
    #[serde(rename = "updatedTime")]
    #[serde(deserialize_with = "from_str_to_i64")]
    pub updated_time: i64,
    #[serde(rename = "rejectReason")]
    pub reject_reason: String,
    #[serde(rename = "stopOrderType")]
    pub stop_order_type: String,
    #[serde(rename = "tpslMode")]
    pub tpsl_mode: String,
    #[serde(rename = "triggerPrice")]
    pub trigger_price: String,
    #[serde(rename = "takeProfit")]
    pub take_profit: String,
    #[serde(rename = "stopLoss")]
    pub stop_loss: String,
    #[serde(rename = "tpTriggerBy")]
    pub tp_trigger_by: String,
    #[serde(rename = "slTriggerBy")]
    pub sl_trigger_by: String,
    #[serde(rename = "tpLimitPrice")]
    pub tp_limit_price: String,
    #[serde(rename = "slLimitPrice")]
    pub sl_limit_price: String,
    #[serde(rename = "triggerDirection")]
    pub trigger_direction: i64,
    #[serde(rename = "triggerBy")]
    pub trigger_by: String,
    #[serde(rename = "closeOnTrigger")]
    pub close_on_trigger: bool,
    pub category: String,
    #[serde(rename = "placeType")]
    pub place_type: String,
    #[serde(rename = "smpType")]
    pub smp_type: String,
    #[serde(rename = "smpGroup")]
    pub smp_group: i64,
    #[serde(rename = "smpOrderId")]
    pub smp_order_id: String,
    #[serde(rename = "feeCurrency")]
    pub fee_currency: String,
}

#[derive(Deserialize, Debug)]
pub struct OrderResponseData {
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
}

#[derive(Deserialize, Debug)]
pub struct TradeStreamMsg {
    #[serde(rename = "reqId")]
    pub req_id: Option<String>,
    #[serde(rename = "retCode")]
    pub ret_code: i64,
    #[serde(rename = "retMsg")]
    pub ret_msg: String,
    pub op: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub header: HashMap<String, String>,
    #[serde(rename = "connId")]
    pub conn_id: String,
}

#[derive(Serialize, Debug)]
pub struct TradeOp<T>
where
    T: Serialize + Debug,
{
    #[serde(rename = "reqId")]
    pub req_id: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub header: HashMap<String, String>,
    pub op: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<T>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Order {
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(rename = "orderType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    pub category: String,
    #[serde(rename = "timeInForce")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
}

#[derive(Deserialize, Debug)]
pub struct RestResult {
    pub list: Option<serde_json::Value>,
    #[serde(default)]
    pub success: String,
    #[serde(rename = "next_page_cursor")]
    #[serde(default)]
    pub next_page_cursor: String,
    #[serde(default)]
    pub category: String,
}

#[derive(Deserialize, Debug)]
pub struct RestResponse {
    #[serde(rename = "retCode")]
    pub ret_code: i64,
    #[serde(rename = "retMsg")]
    pub ret_msg: String,
    pub result: RestResult,
    #[serde(rename = "retExtInfo")]
    pub ret_ext_info: serde_json::Value,
    pub time: i64,
}
