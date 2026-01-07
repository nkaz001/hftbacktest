//! HFT Server - Web API for HFT Supervisor management
//!
//! Usage:
//!   hft-server [OPTIONS]
//!
//! Environment variables:
//!   CLUSTER_CONFIG     - Path to cluster config (default: cluster.toml)
//!   DATA_DIR           - Data directory (default: data)
//!   LOG_DIR            - Log directory (default: logs)
//!   SUPERVISOR_SOCKET  - Supervisor socket path (default: /tmp/hft-supervisor.sock)
//!   JWT_SECRET         - JWT secret for authentication
//!   BIND_ADDR          - Server bind address (default: 127.0.0.1)
//!   PORT               - Server port (default: 3000)
//!   DATABASE_URL       - SQLite database URL (default: sqlite:hft-server.db)

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use axum::{
    routing::{get, post, put},
    Router,
};
use clap::Parser;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use hft_server::api::{bots, config, data, logs, status};
use hft_server::auth;
use hft_server::db::Database;
use hft_server::state::{AppState, ServerConfig};
use hft_server::supervisor::status_updater;
use hft_server::ws;

#[derive(Parser, Debug)]
#[command(name = "hft-server")]
#[command(about = "Web API server for HFT Supervisor management")]
struct Args {
    /// Cluster config file path
    #[arg(long, env = "CLUSTER_CONFIG")]
    config: Option<PathBuf>,

    /// Data directory
    #[arg(long, env = "DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Log directory
    #[arg(long, env = "LOG_DIR")]
    log_dir: Option<PathBuf>,

    /// Supervisor socket path
    #[arg(long, env = "SUPERVISOR_SOCKET")]
    socket: Option<PathBuf>,

    /// Server bind address
    #[arg(long, env = "BIND_ADDR", default_value = "127.0.0.1")]
    bind: String,

    /// Server port
    #[arg(short, long, env = "PORT", default_value = "3000")]
    port: u16,

    /// JWT secret
    #[arg(long, env = "JWT_SECRET")]
    jwt_secret: Option<String>,

    /// Database URL
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Disable authentication (for development only)
    #[arg(long, default_value = "false")]
    no_auth: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hft_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Build config from args and env
    let mut config = ServerConfig::from_env();

    if let Some(path) = args.config {
        config.cluster_config_path = path;
    }
    if let Some(path) = args.data_dir {
        config.data_dir = path;
    }
    if let Some(path) = args.log_dir {
        config.log_dir = path;
    }
    if let Some(path) = args.socket {
        config.supervisor_socket = path;
    }
    if let Some(secret) = args.jwt_secret {
        config.jwt_secret = secret;
    }
    if let Some(url) = args.database_url {
        config.database_url = Some(url);
    }
    config.bind_addr = args.bind;
    config.port = args.port;

    tracing::info!(
        cluster_config = %config.cluster_config_path.display(),
        supervisor_socket = %config.supervisor_socket.display(),
        "Starting HFT Server"
    );

    // Create app state
    let mut state = AppState::new(config.clone());

    // Initialize database if URL is provided
    if let Some(ref db_url) = config.database_url {
        tracing::info!(database = %db_url, "Connecting to database");
        match Database::new(db_url).await {
            Ok(db) => {
                state = state.with_db(db);
                tracing::info!("Database connected successfully");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to connect to database, running without persistence");
            }
        }
    }

    // Start background tasks
    let supervisor_client = state.supervisor.clone();
    tokio::spawn(async move {
        status_updater(supervisor_client, 5).await;
    });

    let state_for_ws = state.clone();
    tokio::spawn(async move {
        ws::status_broadcaster(state_for_ws).await;
    });

    // Build router
    let app = Router::new()
        // Health check (no auth)
        .route("/api/health", get(status::health_check))
        // Auth endpoints (no auth required)
        .route("/api/auth/login", post(auth::login))
        // Auth endpoints (auth required)
        .route("/api/auth/me", get(auth::get_current_user))
        .route("/api/auth/logout", post(auth::logout))
        // Status
        .route("/api/status", get(status::get_status))
        // Bots
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/:id", get(bots::get_bot))
        .route("/api/bots/:id/start", post(bots::start_bot))
        .route("/api/bots/:id/stop", post(bots::stop_bot))
        .route("/api/bots/:id/restart", post(bots::restart_bot))
        .route("/api/bots/restart-all", post(bots::restart_all_bots))
        // Config
        .route("/api/config/cluster", get(config::get_cluster_config))
        .route("/api/config/cluster", put(config::update_cluster_config))
        .route("/api/config/strategies", get(config::list_strategy_configs))
        .route(
            "/api/config/strategies/:bot_id",
            get(config::get_strategy_config),
        )
        .route(
            "/api/config/strategies/:bot_id",
            put(config::update_strategy_config),
        )
        // Data
        .route("/api/data/files", get(data::list_data_files))
        .route("/api/data/stats", get(data::get_data_stats))
        .route("/api/data/warmup/:symbol", get(data::get_warmup_status))
        // Logs
        .route("/api/logs/files", get(logs::list_log_files))
        .route("/api/logs/:component", get(logs::read_logs))
        .route("/api/logs/:component/tail", get(logs::tail_logs))
        // WebSocket
        .route("/ws", get(ws::ws_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.bind_addr, config.port).parse()?;
    tracing::info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}
