//! Recording Service - subscribes to IPC events and writes to ClickHouse

use crate::events::{RecordingEvent, RecordingMessage};
use crate::writer::{ClickHouseWriter, WriterConfig};
use iceoryx2::prelude::*;
use std::time::Duration;
use tracing::{debug, error, info};

/// Default max publishers for IPC service
pub const DEFAULT_MAX_PUBLISHERS: usize = 16;

/// Recording service configuration
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    /// IPC service name for recording events
    pub ipc_service_name: String,
    /// Maximum number of publishers (bots) that can connect
    pub max_publishers: usize,
    /// ClickHouse writer configuration
    pub writer: WriterConfig,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            ipc_service_name: "hft_recording".to_string(),
            max_publishers: DEFAULT_MAX_PUBLISHERS,
            writer: WriterConfig::default(),
        }
    }
}

/// Recording service that subscribes to IPC and writes to ClickHouse
pub struct RecordingService {
    config: ServiceConfig,
    writer: ClickHouseWriter,
}

impl RecordingService {
    /// Create a new recording service
    pub fn new(config: ServiceConfig) -> Self {
        let writer = ClickHouseWriter::new(config.writer.clone());
        Self { config, writer }
    }

    /// Run the recording service (blocking)
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Recording Service...");
        info!("IPC service: {}", self.config.ipc_service_name);
        info!(
            "ClickHouse: {}/{}",
            self.config.writer.url, self.config.writer.database
        );

        // Create IPC subscriber
        let node = NodeBuilder::new().create::<ipc::Service>()?;

        let service_name = ServiceName::new(&self.config.ipc_service_name)?;

        let service = node
            .service_builder(&service_name)
            .publish_subscribe::<RecordingMessage>()
            .max_publishers(self.config.max_publishers)
            .max_subscribers(2)  // Recording service + potential monitoring
            .open_or_create()?;

        let subscriber = service.subscriber_builder().create()?;

        info!("Recording Service started, waiting for events...");

        let mut event_count: u64 = 0;
        let mut last_log_time = std::time::Instant::now();

        loop {
            // Try to receive messages
            match subscriber.receive()? {
                Some(sample) => {
                    let msg = sample.payload();
                    match msg.decode() {
                        Ok(event) => {
                            self.handle_event(event).await;
                            event_count += 1;
                        }
                        Err(e) => {
                            error!("Failed to decode message: {}", e);
                        }
                    }
                }
                None => {
                    // No message, sleep briefly
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }

            // Log stats every 10 seconds
            if last_log_time.elapsed() >= Duration::from_secs(10) {
                info!("Events processed: {}", event_count);
                last_log_time = std::time::Instant::now();
            }
        }
    }

    /// Handle a recording event
    async fn handle_event(&self, event: RecordingEvent) {
        match event {
            RecordingEvent::Order(ref order) => {
                // Debug: log full order details
                info!(
                    "RECV Order: id={} symbol={} side={:?} price={} qty={} status={:?} latency={}",
                    order.order_id, order.symbol, order.side, order.price, order.qty, order.status, order.latency_ns
                );
                if let Err(e) = self.writer.write_order(order).await {
                    error!("Failed to write order: {:?}", e);
                }
            }
            RecordingEvent::Fill(ref fill) => {
                debug!(
                    "Fill: {} {} {} @ {} slippage={:.2}bps",
                    fill.symbol,
                    fill.side.as_i8(),
                    fill.qty,
                    fill.price,
                    fill.slippage_bps()
                );
                if let Err(e) = self.writer.write_fill(fill).await {
                    error!("Failed to write fill: {:?}", e);
                }
            }
            RecordingEvent::Position(ref pos) => {
                debug!(
                    "Position: {} pos={} pnl={:.2}",
                    pos.symbol, pos.position, pos.unrealized_pnl
                );
                if let Err(e) = self.writer.write_position(pos).await {
                    error!("Failed to write position: {:?}", e);
                }
            }
            RecordingEvent::Status(ref status) => {
                info!(
                    "Strategy {}: {:?} - {}",
                    status.strategy_id, status.state, status.message
                );
                if let Err(e) = self.writer.write_status(status).await {
                    error!("Failed to write status: {:?}", e);
                }
            }
            RecordingEvent::Income(ref income) => {
                debug!(
                    "Income: {} {} {} {} = {:.4} {}",
                    income.connector, income.symbol, income.income_type, income.trade_id, income.income, income.asset
                );
                if let Err(e) = self.writer.write_income(income).await {
                    error!("Failed to write income: {:?}", e);
                }
            }
        }
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Shutting down Recording Service...");
        self.writer.flush().await.ok();
        self.writer.shutdown().await.ok();
        Ok(())
    }
}

/// Publisher for recording events (used by strategy bot)
pub struct RecordingPublisher {
    publisher: iceoryx2::port::publisher::Publisher<
        ipc::Service,
        RecordingMessage,
        (),
    >,
}

impl RecordingPublisher {
    /// Create a new publisher with default max_publishers
    pub fn new(service_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_max_publishers(service_name, DEFAULT_MAX_PUBLISHERS)
    }

    /// Create a new publisher with custom max_publishers
    pub fn with_max_publishers(service_name: &str, max_publishers: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let node = NodeBuilder::new().create::<ipc::Service>()?;
        let service_name = ServiceName::new(service_name)?;

        let service = node
            .service_builder(&service_name)
            .publish_subscribe::<RecordingMessage>()
            .max_publishers(max_publishers)
            .max_subscribers(2)  // Recording service + potential monitoring
            .open_or_create()?;

        let publisher = service.publisher_builder().create()?;

        Ok(Self { publisher })
    }

    /// Publish a recording event
    pub fn publish(&self, event: &RecordingEvent) -> Result<(), Box<dyn std::error::Error>> {
        let msg = RecordingMessage::new(event)?;
        let sample = self.publisher.loan_uninit()?;
        let sample = sample.write_payload(msg);
        sample.send()?;
        Ok(())
    }

    /// Publish an order update
    pub fn publish_order(&self, order: crate::events::OrderUpdate) -> Result<(), Box<dyn std::error::Error>> {
        self.publish(&RecordingEvent::Order(order))
    }

    /// Publish a fill event
    pub fn publish_fill(&self, fill: crate::events::FillEvent) -> Result<(), Box<dyn std::error::Error>> {
        self.publish(&RecordingEvent::Fill(fill))
    }

    /// Publish a position snapshot
    pub fn publish_position(&self, pos: crate::events::PositionSnapshot) -> Result<(), Box<dyn std::error::Error>> {
        self.publish(&RecordingEvent::Position(pos))
    }

    /// Publish a strategy status
    pub fn publish_status(&self, status: crate::events::StrategyStatus) -> Result<(), Box<dyn std::error::Error>> {
        self.publish(&RecordingEvent::Status(status))
    }

    /// Publish an income event
    pub fn publish_income(&self, income: crate::events::IncomeEvent) -> Result<(), Box<dyn std::error::Error>> {
        self.publish(&RecordingEvent::Income(income))
    }
}
