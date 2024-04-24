use serde::Deserialize;

use super::{
    from_str_to_f32,
    from_str_to_f32_opt,
    from_str_to_f64,
    from_str_to_side,
    from_str_to_status,
    from_str_to_tif,
    from_str_to_type,
};
use crate::types::{OrdType, Side, Status, TimeInForce};

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum OrderResponseResult {
    Ok(OrderResponse),
    Err(ErrorResponse),
}

#[derive(Deserialize, Debug)]
pub struct OrderResponse {
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "cumQty")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub cum_qty: f32,
    /// New Order and Cancel Order responses only field
    #[serde(rename = "cumQuote")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f32_opt")]
    pub cum_quote: Option<f32>,
    /// Modify Order response only field
    #[serde(rename = "cumBase")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f32_opt")]
    pub cum_base: Option<f32>,
    #[serde(rename = "executedQty")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub executed_qty: f32,
    #[serde(rename = "orderId")]
    pub order_id: i64,
    /// New Order and Modify Order responses only field
    #[serde(rename = "avgPrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f32_opt")]
    pub avg_price: Option<f32>,
    #[serde(rename = "origQty")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub orig_qty: f32,
    #[serde(deserialize_with = "from_str_to_f32")]
    pub price: f32,
    #[serde(rename = "reduceOnly")]
    pub reduce_only: bool,
    #[serde(deserialize_with = "from_str_to_side")]
    pub side: Side,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(deserialize_with = "from_str_to_status")]
    pub status: Status,
    #[serde(rename = "stopPrice")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub stop_price: f32,
    #[serde(rename = "closePosition")]
    pub close_position: bool,
    pub symbol: String,
    // for Coin-M futures
    // pub pair: String,
    /// Modify Order response only field
    #[serde(default)]
    pub pair: Option<String>,
    #[serde(rename = "timeInForce")]
    #[serde(deserialize_with = "from_str_to_tif")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "type")]
    #[serde(deserialize_with = "from_str_to_type")]
    pub ty: OrdType,
    #[serde(rename = "origType")]
    #[serde(deserialize_with = "from_str_to_type")]
    pub orig_type: OrdType,
    /// New Order and Cancel Order responses only field
    #[serde(rename = "activatePrice")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f32_opt")]
    pub activate_price: Option<f32>,
    /// New Order and Cancel Order responses only field
    #[serde(rename = "priceRate")]
    #[serde(default)]
    #[serde(deserialize_with = "from_str_to_f32_opt")]
    pub price_rate: Option<f32>,
    #[serde(rename = "updateTime")]
    pub update_time: i64,
    #[serde(rename = "workingType")]
    pub working_type: String,
    #[serde(rename = "priceProtect")]
    pub price_protect: bool,
    #[serde(rename = "priceMatch")]
    pub price_match: String,
    #[serde(rename = "selfTradePreventionMode")]
    pub self_trade_prevention_mode: String,
    #[serde(rename = "goodTillDate")]
    pub good_till_date: i64,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub code: i64,
    pub msg: String,
}

#[derive(Deserialize, Debug)]
pub struct PositionInformationV2 {
    #[serde(rename = "entryPrice")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub entry_price: f32,
    #[serde(rename = "breakEvenPrice")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub breakeven_price: f32,
    #[serde(rename = "marginType")]
    pub margin_type: String,
    #[serde(rename = "isAutoAddMargin")]
    pub is_auto_add_margin: String,
    #[serde(deserialize_with = "from_str_to_f32")]
    pub leverage: f32,
    #[serde(rename = "liquidationPrice")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub liquidation_price: f32,
    #[serde(rename = "markPrice")]
    #[serde(deserialize_with = "from_str_to_f32")]
    pub mark_price: f32,
    #[serde(rename = "maxNotionalValue")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub max_notional_value: f64,
    #[serde(rename = "positionAmt")]
    #[serde(deserialize_with = "from_str_to_f64")]
    pub position_amount: f64,
    #[serde(deserialize_with = "from_str_to_f64")]
    pub notional: f64,
    #[serde(rename = "isolatedWallet")]
    pub isolated_wallet: String,
    pub symbol: String,
    #[serde(rename = "unRealizedProfit")]
    pub unrealized_pnl: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
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

/// https://binance-docs.github.io/apidocs/futures/en/#error-codes
pub enum ErrorCode {
    Unknown,
    Disconnected,
    Unauthorized,
    TooManyRequests,
    DuplicateIp,
    NoSuchIp,
    UnexpectedResp,
    Timeout,
    ServerBusy,
    ErrorMsgReceived,
    NonWhiteList,
    InvalidMessage,
    UnknownOrderComposition,
    TooManyOrders,
    ServiceShuttingDown,
    UnsupportedOperation,
    InvalidTimestamp,
    InvalidSignature,
    StartTimeGreaterThanEndTime,
    NotFound,
    // 11xx - Request issues
    IllegalChars,
    TooManyParameters,
    MandatoryParamEmptyOrMalformed,
    UnknownParam,
    UnreadParameters,
    ParamEmpty,
    ParamNotRequired,
    BadAsset,
    BadAccount,
    BadInstrumentType,
    BadPrecision,
    NoDepth,
    WithdrawNotNegative,
    TifNotRequired,
    InvalidTif,
    InvalidOrderType,
    InvalidSide,
    EmptyNewClOrdId,
    EmptyOrgClOrdId,
    BadInterval,
    BadSymbol,
    InvalidSymbolStatus,
    InvalidListenKey,
    AssetNotSupported,
    MoreThanXxHours,
    OptionalParamsBadCombo,
    InvalidParameter,
    InvalidNewOrderRespType,
    // 20xx - Processing issues
    NewOrderRejected,
    CancelRejected,
    CancelAllFail,
    NoSuchOrder,
    BadApiKeyFmt,
    RejectedMbxKey,
    NoTradingWindow,
    ApiKeysLocked,
    BalanceNotSufficient,
    MarginNotSufficient,
    UnableToFill,
    OrderWouldImmediatelyTrigger,
    ReduceOnlyReject,
    UserInLiquidation,
    PositionNotSufficient,
    MaxOpenOrderExceeded,
    ReduceOnlyOrderTypeNotSupported,
    MaxLeverageRatio,
    MinLeverageRatio,
    // 40xx Filters and other issues
    InvalidOrderStatus,
    PriceLessThanZero,
    PriceGreaterThanMaxPrice,
    QtyLessThanZero,
    QtyLessThanMinQty,
    QtyGreaterThanMaxQty,
    StopPriceLessThanZero,
    StopPriceGreaterThanMaxPrice,
    TickSizeLessThanZero,
    MaxPriceLEssThanMinPrice,
    MaxQtyLessThanMinQty,
    StepSizeLessThanZero,
    MaxNumOrdersLessThanZero,
    PriceLessThanMinPrice,
    PriceNotIncreasedByTickSize,
    InvalidClOrdIdLen,
    PriceHigherThanMultiplierUp,
    MultiplierUpLessThanZero,
    CompositeScaleOverflow,
    TargetStrategyInvalid,
    InvalidDepthLimit,
    WrongMarketStatus,
    QtyNotIncreasedByStepSize,
    PriceLowerThanMultiplierDown,
    MultiplierDecimalLessThanZero,
    CommissionInvalid,
    InvalidAccountType,
    InvalidLeverage,
    InvalidTickSizePrecision,
    InvalidStepSizePrecision,
    InvalidWorkingType,
    ExceedMaxCancelOrderSize,
    InsuranceAccountNotFound,
    InvalidBalanceType,
    MaxStopOrderExceeded,
    NoNeedToChangeMarginType,
    ThereExistsOpenOrders,
    ThereExistsQuantity,
    AddIsolatedMarginReject,
    CrossBalanceInsufficient,
    NoNeedToChangeAutoAddMargin,
}
