//! Position and Income sync module
//!
//! This module provides periodic synchronization of positions and income data
//! to the order-recorder service for analysis and visualization.

#[cfg(feature = "recording")]
pub mod recording {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use order_recorder::events::{IncomeEvent, PositionSnapshot, RecordingEvent};
    use order_recorder::service::RecordingPublisher;
    use tokio::time::interval;
    use tracing::{debug, error, info, warn};

    /// Configuration for position/income sync
    #[derive(Clone, Debug)]
    pub struct SyncConfig {
        /// IPC service name for recording events
        pub recording_service: String,
        /// Connector name (e.g., "bf", "bybit")
        pub connector_name: String,
        /// Strategy ID (for filtering)
        pub strategy_id: String,
        /// Position sync interval
        pub position_interval: Duration,
        /// Income sync interval
        pub income_interval: Duration,
    }

    impl Default for SyncConfig {
        fn default() -> Self {
            Self {
                recording_service: "hft_recording".to_string(),
                connector_name: "bf".to_string(),
                strategy_id: String::new(),
                position_interval: Duration::from_secs(5),
                income_interval: Duration::from_secs(60),
            }
        }
    }

    /// Trait for position sync providers
    pub trait PositionProvider: Send + Sync {
        /// Get all positions
        fn get_positions(
            &self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PositionData>, String>> + Send + '_>>;
    }

    /// Trait for income sync providers
    pub trait IncomeProvider: Send + Sync {
        /// Get income since last sync
        fn get_income(
            &self,
            since: Option<i64>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<IncomeData>, String>> + Send + '_>>;
    }

    /// Position data from exchange
    #[derive(Clone, Debug)]
    pub struct PositionData {
        pub symbol: String,
        pub position: f64,
        pub entry_price: f64,
        pub mark_price: f64,
        pub unrealized_pnl: f64,
        pub update_time: i64,
    }

    /// Income data from exchange
    #[derive(Clone, Debug)]
    pub struct IncomeData {
        pub symbol: String,
        pub income_type: String,
        pub income: f64,
        pub asset: String,
        pub info: String,
        pub time: i64,
        pub tran_id: i64,
        pub trade_id: String,
    }

    /// Position and Income sync task
    pub struct SyncTask {
        config: SyncConfig,
        publisher: RecordingPublisher,
        symbols: Arc<Mutex<HashSet<String>>>,
        last_income_time: Arc<Mutex<i64>>,
    }

    impl SyncTask {
        /// Create a new sync task
        pub fn new(
            config: SyncConfig,
            symbols: Arc<Mutex<HashSet<String>>>,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            let publisher = RecordingPublisher::new(&config.recording_service)?;
            Ok(Self {
                config,
                publisher,
                symbols,
                last_income_time: Arc::new(Mutex::new(0)),
            })
        }

        /// Run position sync loop
        pub async fn run_position_sync<P: PositionProvider>(
            &self,
            provider: Arc<P>,
        ) {
            // Wait for order-recorder to be ready
            tokio::time::sleep(Duration::from_secs(10)).await;

            info!(
                "Starting position sync task (interval: {:?})",
                self.config.position_interval
            );

            let mut interval = interval(self.config.position_interval);
            loop {
                interval.tick().await;

                match provider.get_positions().await {
                    Ok(positions) => {
                        let registered = self.symbols.lock().unwrap().clone();
                        for pos in positions {
                            // Only publish positions for registered symbols
                            if !registered.is_empty() && !registered.contains(&pos.symbol.to_lowercase()) {
                                continue;
                            }

                            let snapshot = PositionSnapshot {
                                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                                connector: self.config.connector_name.clone(),
                                strategy_id: self.config.strategy_id.clone(),
                                symbol: pos.symbol.clone(),
                                position: pos.position,
                                avg_price: pos.entry_price, // Use entry price as avg price
                                mid_price: pos.mark_price,
                                unrealized_pnl: pos.unrealized_pnl,
                                realized_pnl: 0.0, // Will be tracked via income events
                                entry_price: pos.entry_price,
                                mark_price: pos.mark_price,
                            };

                            debug!(
                                "Position sync: {} pos={} pnl={:.2}",
                                snapshot.symbol, snapshot.position, snapshot.unrealized_pnl
                            );

                            if let Err(e) = self.publisher.publish(&RecordingEvent::Position(snapshot)) {
                                error!("Failed to publish position: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get positions: {}", e);
                    }
                }
            }
        }

        /// Run income sync loop
        pub async fn run_income_sync<I: IncomeProvider>(
            &self,
            provider: Arc<I>,
        ) {
            // Wait for order-recorder to be ready
            tokio::time::sleep(Duration::from_secs(10)).await;

            info!(
                "Starting income sync task (interval: {:?})",
                self.config.income_interval
            );

            let mut interval = interval(self.config.income_interval);
            loop {
                interval.tick().await;

                let since = {
                    let last = self.last_income_time.lock().unwrap();
                    if *last > 0 {
                        Some(*last)
                    } else {
                        // Start from 24 hours ago
                        Some(chrono::Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000)
                    }
                };

                match provider.get_income(since).await {
                    Ok(incomes) => {
                        let registered = self.symbols.lock().unwrap().clone();
                        let mut max_time = since.unwrap_or(0);

                        if incomes.is_empty() {
                            info!(
                                "Income sync: no new entries (since={})",
                                since.unwrap_or(0)
                            );
                        } else {
                            // Log income types for debugging
                            let type_counts: std::collections::HashMap<String, usize> = incomes.iter()
                                .fold(std::collections::HashMap::new(), |mut acc, i| {
                                    *acc.entry(i.income_type.clone()).or_insert(0) += 1;
                                    acc
                                });
                            info!(
                                "Income sync: received {} entries (since={}), types: {:?}",
                                incomes.len(),
                                since.unwrap_or(0),
                                type_counts
                            );
                        }

                        let mut published = 0;
                        let mut skipped = 0;
                        for inc in incomes {
                            // Only publish income for registered symbols (or if no symbols registered)
                            if !registered.is_empty() && !registered.contains(&inc.symbol.to_lowercase()) {
                                skipped += 1;
                                continue;
                            }

                            max_time = max_time.max(inc.time);

                            let event = IncomeEvent {
                                timestamp: inc.time * 1_000_000, // Convert ms to ns
                                connector: self.config.connector_name.clone(),
                                strategy_id: self.config.strategy_id.clone(),
                                symbol: inc.symbol.clone(),
                                income_type: inc.income_type.clone(),
                                income: inc.income,
                                asset: inc.asset.clone(),
                                info: inc.info.clone(),
                                tran_id: inc.tran_id,
                                trade_id: inc.trade_id.clone(),
                            };

                            debug!(
                                "Income sync: {} {} {} = {:.4} {}",
                                event.symbol, event.income_type, event.trade_id, event.income, event.asset
                            );

                            match self.publisher.publish(&RecordingEvent::Income(event)) {
                                Ok(_) => {
                                    published += 1;
                                    // Small delay to prevent overwhelming the subscriber
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                                Err(e) => error!("Failed to publish income: {:?}", e),
                            }
                        }
                        if published > 0 || skipped > 0 {
                            info!(
                                "Income sync: published {} entries, skipped {} (registered={:?})",
                                published, skipped, registered
                            );
                        }

                        // Update last sync time
                        if max_time > 0 {
                            *self.last_income_time.lock().unwrap() = max_time + 1; // +1 to avoid duplicates
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get income: {}", e);
                    }
                }
            }
        }
    }

    /// Binance Futures position/income provider
    #[cfg(feature = "binancefutures")]
    pub mod binance {
        use super::*;
        use crate::binancefutures::rest::BinanceFuturesClient;

        pub struct BinanceFuturesProvider {
            client: BinanceFuturesClient,
        }

        impl BinanceFuturesProvider {
            pub fn new(client: BinanceFuturesClient) -> Self {
                Self { client }
            }
        }

        impl PositionProvider for BinanceFuturesProvider {
            fn get_positions(
                &self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PositionData>, String>> + Send + '_>> {
                Box::pin(async move {
                    match self.client.get_position_information().await {
                        Ok(positions) => {
                            Ok(positions
                                .into_iter()
                                .filter(|p| p.position_amount.abs() > 0.0)
                                .map(|p| PositionData {
                                    symbol: p.symbol.to_lowercase(),
                                    position: p.position_amount,
                                    entry_price: p.entry_price,
                                    mark_price: p.mark_price,
                                    unrealized_pnl: p.unrealized_pnl.parse().unwrap_or(0.0),
                                    update_time: p.update_time,
                                })
                                .collect())
                        }
                        Err(e) => Err(format!("Failed to get positions: {:?}", e)),
                    }
                })
            }
        }

        impl IncomeProvider for BinanceFuturesProvider {
            fn get_income(
                &self,
                since: Option<i64>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<IncomeData>, String>> + Send + '_>> {
                Box::pin(async move {
                    match self.client.get_income(None, None, since, None, Some(1000)).await {
                        Ok(incomes) => {
                            Ok(incomes
                                .into_iter()
                                .map(|i| IncomeData {
                                    symbol: i.symbol,
                                    income_type: i.income_type,
                                    income: i.income,
                                    asset: i.asset,
                                    info: i.info,
                                    time: i.time,
                                    tran_id: i.tran_id,
                                    trade_id: i.trade_id,
                                })
                                .collect())
                        }
                        Err(e) => Err(format!("Failed to get income: {:?}", e)),
                    }
                })
            }
        }
    }

    /// Binance PAPI position/income provider
    #[cfg(feature = "binancepapi")]
    pub mod binancepapi {
        use super::*;
        use crate::binancepapi::rest::BinancePapiClient;

        pub struct BinancePapiProvider {
            client: BinancePapiClient,
        }

        impl BinancePapiProvider {
            pub fn new(client: BinancePapiClient) -> Self {
                Self { client }
            }
        }

        impl PositionProvider for BinancePapiProvider {
            fn get_positions(
                &self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PositionData>, String>> + Send + '_>> {
                Box::pin(async move {
                    match self.client.get_um_position_risk().await {
                        Ok(positions) => {
                            Ok(positions
                                .into_iter()
                                .filter(|p| p.position_amount.abs() > 0.0)
                                .map(|p| PositionData {
                                    symbol: p.symbol.to_lowercase(),
                                    position: p.position_amount,
                                    entry_price: p.entry_price,
                                    mark_price: p.mark_price,
                                    unrealized_pnl: p.unrealized_pnl,
                                    update_time: p.update_time,
                                })
                                .collect())
                        }
                        Err(e) => Err(format!("Failed to get PAPI positions: {:?}", e)),
                    }
                })
            }
        }

        impl IncomeProvider for BinancePapiProvider {
            fn get_income(
                &self,
                since: Option<i64>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<IncomeData>, String>> + Send + '_>> {
                Box::pin(async move {
                    match self.client.get_um_income(None, None, since, None, Some(1000)).await {
                        Ok(incomes) => {
                            Ok(incomes
                                .into_iter()
                                .map(|i| IncomeData {
                                    symbol: i.symbol,
                                    income_type: i.income_type,
                                    income: i.income,
                                    asset: i.asset,
                                    info: i.info,
                                    time: i.time,
                                    tran_id: i.tran_id,
                                    trade_id: i.trade_id,
                                })
                                .collect())
                        }
                        Err(e) => Err(format!("Failed to get PAPI income: {:?}", e)),
                    }
                })
            }
        }
    }

    /// Bybit position/income provider
    #[cfg(feature = "bybit")]
    pub mod bybit {
        use super::*;
        use crate::bybit::rest::BybitClient;

        pub struct BybitProvider {
            client: BybitClient,
            category: String,
        }

        impl BybitProvider {
            pub fn new(client: BybitClient, category: &str) -> Self {
                Self {
                    client,
                    category: category.to_string(),
                }
            }
        }

        impl PositionProvider for BybitProvider {
            fn get_positions(
                &self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PositionData>, String>> + Send + '_>> {
                let category = self.category.clone();
                Box::pin(async move {
                    // settleCoin is required for unified account
                    match self.client.get_all_positions(&category, Some("USDT")).await {
                        Ok(positions) => {
                            Ok(positions
                                .into_iter()
                                .filter(|p| p.size.abs() > 0.0)
                                .map(|p| PositionData {
                                    symbol: p.symbol.to_lowercase(),
                                    position: p.size,
                                    entry_price: p.entry_price.unwrap_or(0.0),
                                    mark_price: p.mark_price,
                                    unrealized_pnl: p.unrealised_pnl.parse().unwrap_or(0.0),
                                    update_time: p.updated_time,
                                })
                                .collect())
                        }
                        Err(e) => Err(format!("Failed to get positions: {:?}", e)),
                    }
                })
            }
        }

        impl IncomeProvider for BybitProvider {
            fn get_income(
                &self,
                since: Option<i64>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<IncomeData>, String>> + Send + '_>> {
                let category = self.category.clone();
                Box::pin(async move {
                    match self
                        .client
                        .get_transaction_log("UNIFIED", Some(&category), None, since, None, Some(50))
                        .await
                    {
                        Ok(logs) => {
                            Ok(logs
                                .into_iter()
                                .map(|l| {
                                    // Map Bybit tx_type to unified income type
                                    let income_type = match l.tx_type.as_str() {
                                        "TRADE" => "REALIZED_PNL",
                                        "SETTLEMENT" => "FUNDING_FEE",
                                        "FEE" | "DELIVERY" => "COMMISSION",
                                        other => other,
                                    };
                                    IncomeData {
                                        symbol: l.symbol.to_lowercase(),
                                        income_type: income_type.to_string(),
                                        income: l.change,
                                        asset: "USDT".to_string(), // Bybit unified uses USDT
                                        info: format!("fee={}", l.fee),
                                        time: l.transaction_time,
                                        tran_id: l.transaction_time, // Use time as tran_id
                                        trade_id: l.trade_id,
                                    }
                                })
                                .collect())
                        }
                        Err(e) => Err(format!("Failed to get transaction log: {:?}", e)),
                    }
                })
            }
        }
    }
}
