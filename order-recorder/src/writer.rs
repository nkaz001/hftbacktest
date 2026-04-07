//! ClickHouse writer for order recording
//!
//! Provides async batched writing to ClickHouse for high throughput.

use crate::events::{FillEvent, IncomeEvent, OrderUpdate, PositionSnapshot, StrategyStatus};
use clickhouse::{Client, Row};
use serde::Serialize;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Convert nanoseconds since epoch to OffsetDateTime
fn nanos_to_datetime(nanos: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(nanos as i128).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

/// ClickHouse row for orders table
#[derive(Row, Serialize, Debug)]
pub struct OrderRow {
    pub order_id: u64,
    pub symbol: String,
    pub side: i8,
    pub price: f64,
    pub qty: f64,
    pub status: u8,
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub create_time: OffsetDateTime,
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub update_time: OffsetDateTime,
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub exch_time: OffsetDateTime,
    pub leaves_qty: f64,
    pub exec_qty: f64,
    pub exec_price: f64,
    pub latency_ns: i64,
    pub strategy_id: String,
}

impl From<&OrderUpdate> for OrderRow {
    fn from(e: &OrderUpdate) -> Self {
        Self {
            order_id: e.order_id,
            symbol: e.symbol.clone(),
            side: e.side.as_i8(),
            price: e.price,
            qty: e.qty,
            status: e.status.as_u8(),
            create_time: nanos_to_datetime(e.timestamp),
            update_time: nanos_to_datetime(e.timestamp),
            exch_time: nanos_to_datetime(e.exch_time),
            leaves_qty: e.leaves_qty,
            exec_qty: e.exec_qty,
            exec_price: e.exec_price,
            latency_ns: e.latency_ns,
            strategy_id: e.strategy_id.clone(),
        }
    }
}

/// ClickHouse row for fills table
#[derive(Row, Serialize, Debug)]
pub struct FillRow {
    pub fill_id: u64,
    pub order_id: u64,
    pub symbol: String,
    pub side: i8,
    pub price: f64,
    pub qty: f64,
    pub fee: f64,
    pub is_maker: u8,
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub exch_time: OffsetDateTime,
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub local_time: OffsetDateTime,
    pub slippage_bps: f64,
    pub strategy_id: String,
}

impl From<&FillEvent> for FillRow {
    fn from(e: &FillEvent) -> Self {
        Self {
            fill_id: e.fill_id,
            order_id: e.order_id,
            symbol: e.symbol.clone(),
            side: e.side.as_i8(),
            price: e.price,
            qty: e.qty,
            fee: e.fee,
            is_maker: if e.is_maker { 1 } else { 0 },
            exch_time: nanos_to_datetime(e.exch_time),
            local_time: nanos_to_datetime(e.timestamp),
            slippage_bps: e.slippage_bps(),
            strategy_id: e.strategy_id.clone(),
        }
    }
}

/// ClickHouse row for positions table
#[derive(Row, Serialize, Debug)]
pub struct PositionRow {
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub timestamp: OffsetDateTime,
    pub connector: String,
    pub symbol: String,
    pub position: f64,
    pub avg_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub strategy_id: String,
    pub entry_price: f64,
    pub mark_price: f64,
}

impl From<&PositionSnapshot> for PositionRow {
    fn from(e: &PositionSnapshot) -> Self {
        Self {
            timestamp: nanos_to_datetime(e.timestamp),
            connector: e.connector.clone(),
            symbol: e.symbol.clone(),
            position: e.position,
            avg_price: e.avg_price,
            unrealized_pnl: e.unrealized_pnl,
            realized_pnl: e.realized_pnl,
            strategy_id: e.strategy_id.clone(),
            entry_price: e.entry_price,
            mark_price: e.mark_price,
        }
    }
}

/// ClickHouse row for strategy_status table
#[derive(Row, Serialize, Debug)]
pub struct StatusRow {
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub timestamp: OffsetDateTime,
    pub strategy_id: String,
    pub status: u8,
    pub message: String,
}

impl From<&StrategyStatus> for StatusRow {
    fn from(e: &StrategyStatus) -> Self {
        Self {
            timestamp: nanos_to_datetime(e.timestamp),
            strategy_id: e.strategy_id.clone(),
            status: e.state.as_u8(),
            message: e.message.clone(),
        }
    }
}

/// ClickHouse row for income table
#[derive(Row, Serialize, Debug)]
pub struct IncomeRow {
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    pub timestamp: OffsetDateTime,
    pub connector: String,
    pub strategy_id: String,
    pub symbol: String,
    pub income_type: String,
    pub income: f64,
    pub asset: String,
    pub info: String,
    pub tran_id: i64,
    pub trade_id: String,
}

impl From<&IncomeEvent> for IncomeRow {
    fn from(e: &IncomeEvent) -> Self {
        Self {
            timestamp: nanos_to_datetime(e.timestamp),
            connector: e.connector.clone(),
            strategy_id: e.strategy_id.clone(),
            symbol: e.symbol.clone(),
            income_type: e.income_type.clone(),
            income: e.income,
            asset: e.asset.clone(),
            info: e.info.clone(),
            tran_id: e.tran_id,
            trade_id: e.trade_id.clone(),
        }
    }
}

/// Write command sent to the writer task
enum WriteCommand {
    Order(OrderRow),
    Fill(FillRow),
    Position(PositionRow),
    Status(StatusRow),
    Income(IncomeRow),
    Flush,
    Shutdown,
}

/// Error type for writer operations
#[derive(Debug, Clone)]
pub enum WriterError {
    /// The writer channel was closed
    ChannelClosed,
}

impl std::fmt::Display for WriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriterError::ChannelClosed => write!(f, "Writer channel closed"),
        }
    }
}

impl std::error::Error for WriterError {}

/// Configuration for the ClickHouse writer
#[derive(Clone, Debug)]
pub struct WriterConfig {
    /// ClickHouse URL (e.g., "http://localhost:8123")
    pub url: String,
    /// Database name
    pub database: String,
    /// Batch size before flushing
    pub batch_size: usize,
    /// Maximum time before flushing (even if batch not full)
    pub flush_interval: Duration,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8123".to_string(),
            database: "hft".to_string(),
            batch_size: 100,
            flush_interval: Duration::from_secs(5),
        }
    }
}

/// ClickHouse writer with async batching
pub struct ClickHouseWriter {
    tx: mpsc::Sender<WriteCommand>,
}

impl ClickHouseWriter {
    /// Create a new writer and spawn the background task
    pub fn new(config: WriterConfig) -> Self {
        let (tx, rx) = mpsc::channel(10000);

        // Spawn the writer task
        tokio::spawn(Self::writer_task(config, rx));

        Self { tx }
    }

    /// Write an order update
    pub async fn write_order(&self, order: &OrderUpdate) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Order(OrderRow::from(order)))
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Write a fill event
    pub async fn write_fill(&self, fill: &FillEvent) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Fill(FillRow::from(fill)))
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Write a position snapshot
    pub async fn write_position(&self, pos: &PositionSnapshot) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Position(PositionRow::from(pos)))
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Write a strategy status
    pub async fn write_status(&self, status: &StrategyStatus) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Status(StatusRow::from(status)))
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Write an income event
    pub async fn write_income(&self, income: &IncomeEvent) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Income(IncomeRow::from(income)))
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Force flush all pending writes
    pub async fn flush(&self) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Flush)
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Shutdown the writer
    pub async fn shutdown(&self) -> Result<(), WriterError> {
        self.tx
            .send(WriteCommand::Shutdown)
            .await
            .map_err(|_| WriterError::ChannelClosed)
    }

    /// Background writer task
    async fn writer_task(config: WriterConfig, mut rx: mpsc::Receiver<WriteCommand>) {
        let client = Client::default()
            .with_url(&config.url)
            .with_database(&config.database);

        let mut orders: Vec<OrderRow> = Vec::with_capacity(config.batch_size);
        let mut fills: Vec<FillRow> = Vec::with_capacity(config.batch_size);
        let mut positions: Vec<PositionRow> = Vec::with_capacity(config.batch_size);
        let mut statuses: Vec<StatusRow> = Vec::with_capacity(config.batch_size);
        let mut incomes: Vec<IncomeRow> = Vec::with_capacity(config.batch_size);

        let mut flush_interval = tokio::time::interval(config.flush_interval);

        info!(
            "ClickHouse writer started, url={}, db={}",
            config.url, config.database
        );

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        WriteCommand::Order(row) => {
                            orders.push(row);
                            if orders.len() >= config.batch_size {
                                Self::flush_orders(&client, &mut orders).await;
                            }
                        }
                        WriteCommand::Fill(row) => {
                            fills.push(row);
                            if fills.len() >= config.batch_size {
                                Self::flush_fills(&client, &mut fills).await;
                            }
                        }
                        WriteCommand::Position(row) => {
                            positions.push(row);
                            if positions.len() >= config.batch_size {
                                Self::flush_positions(&client, &mut positions).await;
                            }
                        }
                        WriteCommand::Status(row) => {
                            statuses.push(row);
                            if statuses.len() >= config.batch_size {
                                Self::flush_statuses(&client, &mut statuses).await;
                            }
                        }
                        WriteCommand::Income(row) => {
                            incomes.push(row);
                            if incomes.len() >= config.batch_size {
                                Self::flush_incomes(&client, &mut incomes).await;
                            }
                        }
                        WriteCommand::Flush => {
                            Self::flush_all(&client, &mut orders, &mut fills, &mut positions, &mut statuses, &mut incomes).await;
                        }
                        WriteCommand::Shutdown => {
                            info!("ClickHouse writer shutting down...");
                            Self::flush_all(&client, &mut orders, &mut fills, &mut positions, &mut statuses, &mut incomes).await;
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    Self::flush_all(&client, &mut orders, &mut fills, &mut positions, &mut statuses, &mut incomes).await;
                }
            }
        }

        info!("ClickHouse writer stopped");
    }

    async fn flush_orders(client: &Client, orders: &mut Vec<OrderRow>) {
        if orders.is_empty() {
            return;
        }

        let count = orders.len();
        match client.insert("orders") {
            Ok(mut inserter) => {
                for row in orders.drain(..) {
                    if let Err(e) = inserter.write(&row).await {
                        error!("Failed to write order row: {}", e);
                    }
                }
                if let Err(e) = inserter.end().await {
                    error!("Failed to flush orders: {}", e);
                } else {
                    debug!("Flushed {} orders", count);
                }
            }
            Err(e) => {
                error!("Failed to create orders inserter: {}", e);
                orders.clear();
            }
        }
    }

    async fn flush_fills(client: &Client, fills: &mut Vec<FillRow>) {
        if fills.is_empty() {
            return;
        }

        let count = fills.len();
        match client.insert("fills") {
            Ok(mut inserter) => {
                for row in fills.drain(..) {
                    if let Err(e) = inserter.write(&row).await {
                        error!("Failed to write fill row: {}", e);
                    }
                }
                if let Err(e) = inserter.end().await {
                    error!("Failed to flush fills: {}", e);
                } else {
                    debug!("Flushed {} fills", count);
                }
            }
            Err(e) => {
                error!("Failed to create fills inserter: {}", e);
                fills.clear();
            }
        }
    }

    async fn flush_positions(client: &Client, positions: &mut Vec<PositionRow>) {
        if positions.is_empty() {
            return;
        }

        let count = positions.len();
        match client.insert("positions") {
            Ok(mut inserter) => {
                for row in positions.drain(..) {
                    if let Err(e) = inserter.write(&row).await {
                        error!("Failed to write position row: {}", e);
                    }
                }
                if let Err(e) = inserter.end().await {
                    error!("Failed to flush positions: {}", e);
                } else {
                    debug!("Flushed {} positions", count);
                }
            }
            Err(e) => {
                error!("Failed to create positions inserter: {}", e);
                positions.clear();
            }
        }
    }

    async fn flush_statuses(client: &Client, statuses: &mut Vec<StatusRow>) {
        if statuses.is_empty() {
            return;
        }

        let count = statuses.len();
        match client.insert("strategy_status") {
            Ok(mut inserter) => {
                for row in statuses.drain(..) {
                    if let Err(e) = inserter.write(&row).await {
                        error!("Failed to write status row: {}", e);
                    }
                }
                if let Err(e) = inserter.end().await {
                    error!("Failed to flush statuses: {}", e);
                } else {
                    debug!("Flushed {} statuses", count);
                }
            }
            Err(e) => {
                error!("Failed to create statuses inserter: {}", e);
                statuses.clear();
            }
        }
    }

    async fn flush_incomes(client: &Client, incomes: &mut Vec<IncomeRow>) {
        if incomes.is_empty() {
            return;
        }

        let count = incomes.len();
        match client.insert("income") {
            Ok(mut inserter) => {
                for row in incomes.drain(..) {
                    if let Err(e) = inserter.write(&row).await {
                        error!("Failed to write income row: {}", e);
                    }
                }
                if let Err(e) = inserter.end().await {
                    error!("Failed to flush incomes: {}", e);
                } else {
                    debug!("Flushed {} incomes", count);
                }
            }
            Err(e) => {
                error!("Failed to create incomes inserter: {}", e);
                incomes.clear();
            }
        }
    }

    async fn flush_all(
        client: &Client,
        orders: &mut Vec<OrderRow>,
        fills: &mut Vec<FillRow>,
        positions: &mut Vec<PositionRow>,
        statuses: &mut Vec<StatusRow>,
        incomes: &mut Vec<IncomeRow>,
    ) {
        Self::flush_orders(client, orders).await;
        Self::flush_fills(client, fills).await;
        Self::flush_positions(client, positions).await;
        Self::flush_statuses(client, statuses).await;
        Self::flush_incomes(client, incomes).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{OrderStatus, Side};

    #[test]
    fn test_order_row_from_update() {
        let update = OrderUpdate {
            timestamp: 1000000000,
            strategy_id: "test".to_string(),
            order_id: 123,
            symbol: "btcusdt".to_string(),
            side: Side::Buy,
            price: 50000.0,
            qty: 0.1,
            status: OrderStatus::Filled,
            prev_status: OrderStatus::New,
            leaves_qty: 0.0,
            exec_qty: 0.1,
            exec_price: 50000.0,
            exch_time: 999000000,
            latency_ns: 1000000,
        };

        let row = OrderRow::from(&update);
        assert_eq!(row.order_id, 123);
        assert_eq!(row.side, 1);
        assert_eq!(row.status, 3);
    }
}
