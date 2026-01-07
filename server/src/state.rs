//! Application state

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::db::Database;
use crate::supervisor::SupervisorClient;
use crate::ws::WsEvent;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Path to cluster config file
    pub cluster_config_path: PathBuf,
    /// Directory for data files
    pub data_dir: PathBuf,
    /// Directory for log files
    pub log_dir: PathBuf,
    /// Supervisor socket path
    pub supervisor_socket: PathBuf,
    /// JWT secret for authentication
    pub jwt_secret: String,
    /// Server bind address
    pub bind_addr: String,
    /// Server port
    pub port: u16,
    /// Database URL
    pub database_url: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            cluster_config_path: PathBuf::from("cluster.toml"),
            data_dir: PathBuf::from("data"),
            log_dir: PathBuf::from("logs"),
            supervisor_socket: PathBuf::from("/tmp/hft-supervisor.sock"),
            jwt_secret: "change-me-in-production".to_string(),
            bind_addr: "127.0.0.1".to_string(),
            port: 3000,
            database_url: None,
        }
    }
}

impl ServerConfig {
    /// Load config from environment variables
    pub fn from_env() -> Self {
        Self {
            cluster_config_path: std::env::var("CLUSTER_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("cluster.toml")),
            data_dir: std::env::var("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data")),
            log_dir: std::env::var("LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("logs")),
            supervisor_socket: std::env::var("SUPERVISOR_SOCKET")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/tmp/hft-supervisor.sock")),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "change-me-in-production".to_string()),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            database_url: std::env::var("DATABASE_URL").ok(),
        }
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Server configuration
    pub config: Arc<ServerConfig>,
    /// Supervisor client
    pub supervisor: SupervisorClient,
    /// Event broadcast channel
    pub event_tx: broadcast::Sender<WsEvent>,
    /// Database connection (optional)
    pub db: Option<Database>,
}

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let supervisor = SupervisorClient::new(&config.supervisor_socket);
        let (event_tx, _) = broadcast::channel(1024);

        Self {
            config: Arc::new(config),
            supervisor,
            event_tx,
            db: None,
        }
    }

    pub fn with_db(mut self, db: Database) -> Self {
        self.db = Some(db);
        self
    }
}
