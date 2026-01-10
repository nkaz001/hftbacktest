//! Order Recorder Service
//!
//! Standalone service that subscribes to IPC recording events and writes to ClickHouse.
//!
//! Usage:
//!   order-recorder [OPTIONS]
//!
//! Options:
//!   --ipc-service <NAME>     IPC service name (default: hft_recording)
//!   --clickhouse-url <URL>   ClickHouse HTTP URL (default: http://localhost:8123)
//!   --database <NAME>        ClickHouse database (default: hft)
//!   --batch-size <N>         Batch size before flushing (default: 100)
//!   --flush-interval <SECS>  Flush interval in seconds (default: 5)

use clap::Parser;
use order_recorder::{RecordingService, WriterConfig};
use order_recorder::service::ServiceConfig;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "order-recorder")]
#[command(about = "Order recording service for HFT strategies")]
struct Args {
    /// IPC service name
    #[arg(long, default_value = "hft_recording")]
    ipc_service: String,

    /// ClickHouse HTTP URL
    #[arg(long, default_value = "http://localhost:8123")]
    clickhouse_url: String,

    /// ClickHouse database name
    #[arg(long, default_value = "hft")]
    database: String,

    /// Batch size before flushing to ClickHouse
    #[arg(long, default_value = "100")]
    batch_size: usize,

    /// Flush interval in seconds
    #[arg(long, default_value = "5")]
    flush_interval: u64,

    /// Maximum number of publishers (bots) that can connect
    #[arg(long, default_value = "16")]
    max_publishers: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("order_recorder=info".parse()?)
                .add_directive("clickhouse=warn".parse()?),
        )
        .init();

    let args = Args::parse();

    println!("======================================");
    println!(" Order Recorder Service");
    println!("======================================");
    println!("IPC Service: {}", args.ipc_service);
    println!("ClickHouse: {}/{}", args.clickhouse_url, args.database);
    println!("Batch Size: {}", args.batch_size);
    println!("Flush Interval: {}s", args.flush_interval);
    println!("Max Publishers: {}", args.max_publishers);
    println!("======================================\n");

    let config = ServiceConfig {
        ipc_service_name: args.ipc_service,
        max_publishers: args.max_publishers,
        writer: WriterConfig {
            url: args.clickhouse_url,
            database: args.database,
            batch_size: args.batch_size,
            flush_interval: Duration::from_secs(args.flush_interval),
        },
    };

    let service = RecordingService::new(config);

    // Handle Ctrl+C
    tokio::spawn(async {
        tokio::signal::ctrl_c().await.ok();
        info!("Received Ctrl+C, shutting down...");
        std::process::exit(0);
    });

    // Run the service
    service.run().await
}
