//! IPC module for supervisor control
//!
//! Provides Unix socket based communication between hft-supervisor and hftctl.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::health::HealthChecker;
use crate::manager::BotManager;

/// Request from hftctl to supervisor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ControlRequest {
    /// Get cluster status
    Status,
    /// Get status of a specific bot
    BotStatus { bot_id: String },
    /// Start a bot
    StartBot { bot_id: String },
    /// Stop a bot
    StopBot { bot_id: String },
    /// Restart a bot
    RestartBot { bot_id: String },
    /// Restart all bots
    RestartAll,
    /// Reload configuration
    Reload,
    /// Check warmup data status
    WarmupStatus { symbol: Option<String> },
    /// Shutdown supervisor
    Shutdown,
}

/// Response from supervisor to hftctl
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControlResponse {
    /// Success with optional data
    Ok { data: Option<serde_json::Value> },
    /// Error with message
    Error { message: String },
}

impl ControlResponse {
    pub fn ok() -> Self {
        ControlResponse::Ok { data: None }
    }

    pub fn ok_with_data<T: Serialize>(data: T) -> Self {
        ControlResponse::Ok {
            data: Some(serde_json::to_value(data).unwrap_or_default()),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        ControlResponse::Error {
            message: message.into(),
        }
    }
}

/// Status response data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub cluster_name: String,
    pub uptime_secs: u64,
    pub connector: ComponentStatus,
    pub collector: ComponentStatus,
    pub bots: Vec<BotStatusInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub status: String,
    pub healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotStatusInfo {
    pub id: String,
    pub symbol: String,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub restart_count: u32,
}

/// Control server for Unix socket communication
pub struct ControlServer {
    socket_path: std::path::PathBuf,
    manager: Arc<BotManager>,
    health_checker: Arc<HealthChecker>,
    cluster_name: String,
    start_time: std::time::Instant,
}

impl ControlServer {
    pub fn new(
        socket_path: std::path::PathBuf,
        manager: Arc<BotManager>,
        health_checker: Arc<HealthChecker>,
        cluster_name: String,
    ) -> Self {
        Self {
            socket_path,
            manager,
            health_checker,
            cluster_name,
            start_time: std::time::Instant::now(),
        }
    }

    /// Start the control server
    pub async fn run(&self, mut shutdown_rx: mpsc::Receiver<()>) -> Result<()> {
        // Remove existing socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // Create parent directory if needed
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("Failed to bind to {}", self.socket_path.display()))?;

        info!(socket = %self.socket_path.display(), "Control server listening");

        // Set socket permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let manager = self.manager.clone();
                            let health_checker = self.health_checker.clone();
                            let cluster_name = self.cluster_name.clone();
                            let uptime = self.start_time.elapsed().as_secs();

                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(
                                    stream,
                                    manager,
                                    health_checker,
                                    cluster_name,
                                    uptime,
                                ).await {
                                    warn!(error = %e, "Error handling control connection");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "Error accepting connection");
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Control server shutting down");
                    break;
                }
            }
        }

        // Cleanup socket file
        let _ = std::fs::remove_file(&self.socket_path);

        Ok(())
    }
}

/// Handle a single control connection
async fn handle_connection(
    stream: UnixStream,
    manager: Arc<BotManager>,
    health_checker: Arc<HealthChecker>,
    cluster_name: String,
    uptime_secs: u64,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: ControlRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                let response = ControlResponse::error(format!("Invalid request: {}", e));
                let response_json = serde_json::to_string(&response)?;
                writer.write_all(response_json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                line.clear();
                continue;
            }
        };

        debug!(?request, "Received control request");

        let response = handle_request(
            request,
            &manager,
            &health_checker,
            &cluster_name,
            uptime_secs,
        )
        .await;

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        line.clear();
    }

    Ok(())
}

/// Handle a control request
async fn handle_request(
    request: ControlRequest,
    manager: &BotManager,
    health_checker: &HealthChecker,
    cluster_name: &str,
    uptime_secs: u64,
) -> ControlResponse {
    match request {
        ControlRequest::Status => {
            let bot_states = manager.get_bot_states().await;

            let mut bots: Vec<BotStatusInfo> = Vec::new();
            for (id, state) in bot_states {
                bots.push(BotStatusInfo {
                    id: id.clone(),
                    symbol: String::new(), // TODO: get from config
                    state: state.as_str().to_string(),
                    pid: None, // TODO: get from manager
                    uptime_secs: None,
                    restart_count: 0,
                });
            }

            let connector_health = health_checker.check_connector("bf");
            let collector_health = health_checker.check_collector();

            let status = StatusResponse {
                cluster_name: cluster_name.to_string(),
                uptime_secs,
                connector: ComponentStatus {
                    status: if connector_health.healthy {
                        "running".to_string()
                    } else {
                        "stopped".to_string()
                    },
                    healthy: connector_health.healthy,
                },
                collector: ComponentStatus {
                    status: if collector_health.healthy {
                        "running".to_string()
                    } else {
                        "stopped".to_string()
                    },
                    healthy: collector_health.healthy,
                },
                bots,
            };

            ControlResponse::ok_with_data(status)
        }

        ControlRequest::BotStatus { bot_id } => {
            let bot_states = manager.get_bot_states().await;
            match bot_states.get(&bot_id) {
                Some(state) => {
                    let info = BotStatusInfo {
                        id: bot_id,
                        symbol: String::new(),
                        state: state.as_str().to_string(),
                        pid: None,
                        uptime_secs: None,
                        restart_count: 0,
                    };
                    ControlResponse::ok_with_data(info)
                }
                None => ControlResponse::error(format!("Bot not found: {}", bot_id)),
            }
        }

        ControlRequest::StartBot { bot_id } => match manager.start_bot(&bot_id).await {
            Ok(()) => ControlResponse::ok(),
            Err(e) => ControlResponse::error(e.to_string()),
        },

        ControlRequest::StopBot { bot_id } => match manager.stop_bot(&bot_id).await {
            Ok(()) => ControlResponse::ok(),
            Err(e) => ControlResponse::error(e.to_string()),
        },

        ControlRequest::RestartBot { bot_id } => match manager.restart_bot(&bot_id).await {
            Ok(()) => ControlResponse::ok(),
            Err(e) => ControlResponse::error(e.to_string()),
        },

        ControlRequest::RestartAll => match manager.stop_all().await {
            Ok(()) => match manager.start_all().await {
                Ok(()) => ControlResponse::ok(),
                Err(e) => ControlResponse::error(e.to_string()),
            },
            Err(e) => ControlResponse::error(e.to_string()),
        },

        ControlRequest::Reload => {
            // TODO: implement config reload
            ControlResponse::error("Not implemented yet")
        }

        ControlRequest::WarmupStatus { symbol } => {
            // TODO: implement warmup status check
            ControlResponse::error("Not implemented yet")
        }

        ControlRequest::Shutdown => {
            // This should trigger supervisor shutdown
            ControlResponse::ok()
        }
    }
}

/// Control client for hftctl
pub struct ControlClient {
    socket_path: std::path::PathBuf,
}

impl ControlClient {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    /// Send a request and receive response
    pub async fn send(&self, request: ControlRequest) -> Result<ControlResponse> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to supervisor at {}",
                    self.socket_path.display()
                )
            })?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Send request
        let request_json = serde_json::to_string(&request)?;
        writer.write_all(request_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        // Read response
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: ControlResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }
}
