//! IPC Event types for order recording
//!
//! These events are published by the strategy bot and consumed by the recording service.

use bincode::{Decode, Encode};
use iceoryx2::prelude::ZeroCopySend;
use serde::{Deserialize, Serialize};

/// Side of the order
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[repr(i8)]
pub enum Side {
    Buy = 1,
    Sell = -1,
}

impl Side {
    pub fn as_i8(&self) -> i8 {
        match self {
            Side::Buy => 1,
            Side::Sell => -1,
        }
    }
}

/// Order status
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[repr(u8)]
pub enum OrderStatus {
    New = 1,
    PartiallyFilled = 2,
    Filled = 3,
    Canceled = 4,
    Rejected = 5,
    Expired = 6,
}

impl OrderStatus {
    pub fn as_u8(&self) -> u8 {
        match self {
            OrderStatus::New => 1,
            OrderStatus::PartiallyFilled => 2,
            OrderStatus::Filled => 3,
            OrderStatus::Canceled => 4,
            OrderStatus::Rejected => 5,
            OrderStatus::Expired => 6,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::New => "New",
            OrderStatus::PartiallyFilled => "PartiallyFilled",
            OrderStatus::Filled => "Filled",
            OrderStatus::Canceled => "Canceled",
            OrderStatus::Rejected => "Rejected",
            OrderStatus::Expired => "Expired",
        }
    }
}

/// Strategy state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[repr(u8)]
pub enum StrategyState {
    Starting = 1,
    Running = 2,
    Paused = 3,
    Stopping = 4,
    Stopped = 5,
    Error = 6,
}

impl StrategyState {
    pub fn as_u8(&self) -> u8 {
        match self {
            StrategyState::Starting => 1,
            StrategyState::Running => 2,
            StrategyState::Paused => 3,
            StrategyState::Stopping => 4,
            StrategyState::Stopped => 5,
            StrategyState::Error => 6,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            StrategyState::Starting => "Starting",
            StrategyState::Running => "Running",
            StrategyState::Paused => "Paused",
            StrategyState::Stopping => "Stopping",
            StrategyState::Stopped => "Stopped",
            StrategyState::Error => "Error",
        }
    }
}

/// Order update event - sent when order status changes
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct OrderUpdate {
    /// Local timestamp (nanoseconds)
    pub timestamp: i64,
    /// Strategy identifier
    pub strategy_id: String,
    /// Order ID
    pub order_id: u64,
    /// Trading symbol
    pub symbol: String,
    /// Order side
    pub side: Side,
    /// Order price
    pub price: f64,
    /// Order quantity
    pub qty: f64,
    /// Current order status
    pub status: OrderStatus,
    /// Previous order status
    pub prev_status: OrderStatus,
    /// Remaining quantity
    pub leaves_qty: f64,
    /// Executed quantity
    pub exec_qty: f64,
    /// Average execution price
    pub exec_price: f64,
    /// Exchange timestamp (nanoseconds)
    pub exch_time: i64,
    /// Round-trip latency (nanoseconds)
    pub latency_ns: i64,
}

/// Fill event - sent when order is (partially) filled
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct FillEvent {
    /// Local timestamp (nanoseconds)
    pub timestamp: i64,
    /// Strategy identifier
    pub strategy_id: String,
    /// Fill ID (unique)
    pub fill_id: u64,
    /// Order ID
    pub order_id: u64,
    /// Trading symbol
    pub symbol: String,
    /// Order side
    pub side: Side,
    /// Fill price
    pub price: f64,
    /// Fill quantity
    pub qty: f64,
    /// Trading fee
    pub fee: f64,
    /// Whether this was a maker fill
    pub is_maker: bool,
    /// Exchange timestamp (nanoseconds)
    pub exch_time: i64,
    /// Mid price at fill time (for slippage calculation)
    pub mid_price: f64,
}

impl FillEvent {
    /// Calculate slippage in basis points
    pub fn slippage_bps(&self) -> f64 {
        if self.mid_price <= 0.0 {
            return 0.0;
        }
        let slippage = match self.side {
            Side::Buy => (self.price - self.mid_price) / self.mid_price,
            Side::Sell => (self.mid_price - self.price) / self.mid_price,
        };
        slippage * 10000.0 // Convert to basis points
    }
}

/// Position snapshot - sent periodically or on position change
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct PositionSnapshot {
    /// Local timestamp (nanoseconds)
    pub timestamp: i64,
    /// Connector name (e.g., "bf", "bybit")
    pub connector: String,
    /// Strategy identifier
    pub strategy_id: String,
    /// Trading symbol
    pub symbol: String,
    /// Current position (positive = long, negative = short)
    pub position: f64,
    /// Average entry price
    pub avg_price: f64,
    /// Current mid/mark price
    pub mid_price: f64,
    /// Unrealized PnL
    pub unrealized_pnl: f64,
    /// Realized PnL
    pub realized_pnl: f64,
    /// Entry price (if different from avg_price)
    #[serde(default)]
    pub entry_price: f64,
    /// Mark price
    #[serde(default)]
    pub mark_price: f64,
}

/// Account snapshot - sent periodically for account-level equity/margin metrics
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct AccountSnapshot {
    /// Local timestamp (nanoseconds)
    pub timestamp: i64,
    /// Connector name (e.g., "bf", "bybit")
    pub connector: String,
    /// Strategy identifier
    pub strategy_id: String,
    /// Account equity
    pub account_equity: f64,
    /// Actual/wallet equity
    pub actual_equity: f64,
    /// Maintenance margin ratio
    pub uni_mmr: f64,
    /// Initial margin
    pub account_initial_margin: f64,
    /// Maintenance margin
    pub account_maint_margin: f64,
}

/// Strategy status event - sent on state changes
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct StrategyStatus {
    /// Local timestamp (nanoseconds)
    pub timestamp: i64,
    /// Strategy identifier
    pub strategy_id: String,
    /// Current state
    pub state: StrategyState,
    /// Optional message
    pub message: String,
}

/// Factor snapshot - periodic factor values from strategy for alignment verification
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct FactorSnapshot {
    /// Local timestamp (nanoseconds)
    pub timestamp: i64,
    /// Connector name (e.g., "bf", "bybit")
    pub connector: String,
    /// Strategy identifier
    pub strategy_id: String,
    /// Trading symbol
    pub symbol: String,
    /// Factor values as key-value pairs
    /// Keys follow convention: "obi.zscore", "obi.raw", "vol.value", "sl.status"
    pub factors: Vec<(String, f64)>,
}

/// Income event - realized PnL, commissions, funding fees, etc.
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub struct IncomeEvent {
    /// Exchange timestamp (nanoseconds)
    pub timestamp: i64,
    /// Connector name (e.g., "bf", "bybit")
    pub connector: String,
    /// Strategy identifier (empty if not associated with a strategy)
    pub strategy_id: String,
    /// Trading symbol
    pub symbol: String,
    /// Income type: REALIZED_PNL, COMMISSION, FUNDING_FEE, TRANSFER, etc.
    pub income_type: String,
    /// Income amount (positive = profit, negative = loss/fee)
    pub income: f64,
    /// Asset currency (e.g., "USDT", "BTC")
    pub asset: String,
    /// Additional info (e.g., trade ID)
    pub info: String,
    /// Transaction ID (unique identifier from exchange)
    pub tran_id: i64,
    /// Trade ID (for REALIZED_PNL, COMMISSION)
    pub trade_id: String,
}

/// Recording event - union of all event types
#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize)]
pub enum RecordingEvent {
    Order(OrderUpdate),
    Fill(FillEvent),
    Position(PositionSnapshot),
    Account(AccountSnapshot),
    Status(StrategyStatus),
    Income(IncomeEvent),
    Factor(FactorSnapshot),
}

impl RecordingEvent {
    pub fn timestamp(&self) -> i64 {
        match self {
            RecordingEvent::Order(e) => e.timestamp,
            RecordingEvent::Fill(e) => e.timestamp,
            RecordingEvent::Position(e) => e.timestamp,
            RecordingEvent::Account(e) => e.timestamp,
            RecordingEvent::Status(e) => e.timestamp,
            RecordingEvent::Income(e) => e.timestamp,
            RecordingEvent::Factor(e) => e.timestamp,
        }
    }

    pub fn strategy_id(&self) -> &str {
        match self {
            RecordingEvent::Order(e) => &e.strategy_id,
            RecordingEvent::Fill(e) => &e.strategy_id,
            RecordingEvent::Position(e) => &e.strategy_id,
            RecordingEvent::Account(e) => &e.strategy_id,
            RecordingEvent::Status(e) => &e.strategy_id,
            RecordingEvent::Income(e) => &e.strategy_id,
            RecordingEvent::Factor(e) => &e.strategy_id,
        }
    }
}

/// IPC message wrapper with fixed size for iceoryx2
#[derive(Clone, Debug, ZeroCopySend)]
#[repr(C)]
pub struct RecordingMessage {
    /// Message length
    pub len: u32,
    /// Serialized RecordingEvent (bincode)
    pub data: [u8; Self::MAX_SIZE],
}

impl RecordingMessage {
    pub const MAX_SIZE: usize = 1024;

    pub fn new(event: &RecordingEvent) -> Result<Self, bincode::error::EncodeError> {
        let config = bincode::config::standard();
        let encoded = bincode::encode_to_vec(event, config)?;

        if encoded.len() > Self::MAX_SIZE {
            return Err(bincode::error::EncodeError::Other("Message too large"));
        }

        let mut msg = RecordingMessage {
            len: encoded.len() as u32,
            data: [0u8; Self::MAX_SIZE],
        };
        msg.data[..encoded.len()].copy_from_slice(&encoded);
        Ok(msg)
    }

    pub fn decode(&self) -> Result<RecordingEvent, bincode::error::DecodeError> {
        let config = bincode::config::standard();
        let (event, _) = bincode::decode_from_slice(&self.data[..self.len as usize], config)?;
        Ok(event)
    }
}

impl Default for RecordingMessage {
    fn default() -> Self {
        Self {
            len: 0,
            data: [0u8; Self::MAX_SIZE],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_update_roundtrip() {
        let event = RecordingEvent::Order(OrderUpdate {
            timestamp: 1234567890,
            strategy_id: "test_strategy".to_string(),
            order_id: 12345,
            symbol: "btcusdt".to_string(),
            side: Side::Buy,
            price: 50000.0,
            qty: 0.1,
            status: OrderStatus::Filled,
            prev_status: OrderStatus::New,
            leaves_qty: 0.0,
            exec_qty: 0.1,
            exec_price: 50000.0,
            exch_time: 1234567800,
            latency_ns: 1000000,
        });

        let msg = RecordingMessage::new(&event).unwrap();
        let decoded = msg.decode().unwrap();

        if let RecordingEvent::Order(o) = decoded {
            assert_eq!(o.order_id, 12345);
            assert_eq!(o.symbol, "btcusdt");
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_fill_slippage() {
        let fill = FillEvent {
            timestamp: 0,
            strategy_id: "test".to_string(),
            fill_id: 1,
            order_id: 1,
            symbol: "btcusdt".to_string(),
            side: Side::Buy,
            price: 50010.0,
            qty: 0.1,
            fee: 0.0,
            is_maker: false,
            exch_time: 0,
            mid_price: 50000.0,
        };

        // Buy at 50010 when mid is 50000 = 2 bps slippage
        assert!((fill.slippage_bps() - 2.0).abs() < 0.01);
    }
}
