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
use crate::infra::{ClusterHealth, ClusterHealthIpc, ComponentState, InfraComponent, InfraManager};
use crate::manager::{BotInfo, BotManager};

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

    // Infrastructure commands
    /// List all infrastructure components
    ListInfra,
    /// Get status of a specific infrastructure component
    InfraStatus { component: String },
    /// Start an infrastructure component
    StartInfra { component: String },
    /// Stop an infrastructure component
    StopInfra { component: String },
    /// Restart an infrastructure component
    RestartInfra { component: String },
    /// Start all components (infra + bots) in dependency order
    StartAll,
    /// Stop all components (bots + infra) in reverse order
    StopAll,
    /// Get cluster health status
    ClusterHealth,
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
    infra_manager: Arc<InfraManager>,
    health_checker: Arc<HealthChecker>,
    cluster_name: String,
    start_time: std::time::Instant,
}

impl ControlServer {
    pub fn new(
        socket_path: std::path::PathBuf,
        manager: Arc<BotManager>,
        infra_manager: Arc<InfraManager>,
        health_checker: Arc<HealthChecker>,
        cluster_name: String,
    ) -> Self {
        Self {
            socket_path,
            manager,
            infra_manager,
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
                            let infra_manager = self.infra_manager.clone();
                            let health_checker = self.health_checker.clone();
                            let cluster_name = self.cluster_name.clone();
                            let uptime = self.start_time.elapsed().as_secs();

                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(
                                    stream,
                                    manager,
                                    infra_manager,
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
    infra_manager: Arc<InfraManager>,
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
            &infra_manager,
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
    infra_manager: &InfraManager,
    health_checker: &HealthChecker,
    cluster_name: &str,
    uptime_secs: u64,
) -> ControlResponse {
    match request {
        ControlRequest::Status => {
            let bot_infos = manager.get_all_bot_info().await;
            let infra_status = infra_manager.get_all_status().await;

            // Convert BotInfo to BotStatusInfo
            let bots: Vec<BotStatusInfo> = bot_infos
                .into_iter()
                .map(|info| BotStatusInfo {
                    id: info.id,
                    symbol: info.symbol,
                    state: info.state,
                    pid: info.pid,
                    uptime_secs: info.uptime_secs,
                    restart_count: info.restart_count,
                })
                .collect();

            // Get connector and collector status from InfraManager
            let connector_status = infra_status
                .iter()
                .find(|s| s.component == InfraComponent::Connector);
            let collector_status = infra_status
                .iter()
                .find(|s| s.component == InfraComponent::Collector);

            let status = StatusResponse {
                cluster_name: cluster_name.to_string(),
                uptime_secs,
                connector: ComponentStatus {
                    status: connector_status
                        .map(|s| s.state.as_str().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    healthy: connector_status
                        .map(|s| matches!(s.state, ComponentState::Running))
                        .unwrap_or(false),
                },
                collector: ComponentStatus {
                    status: collector_status
                        .map(|s| s.state.as_str().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    healthy: collector_status
                        .map(|s| matches!(s.state, ComponentState::Running))
                        .unwrap_or(false),
                },
                bots,
            };

            ControlResponse::ok_with_data(status)
        }

        ControlRequest::BotStatus { bot_id } => {
            match manager.get_bot_info(&bot_id).await {
                Some(info) => {
                    let status_info = BotStatusInfo {
                        id: info.id,
                        symbol: info.symbol,
                        state: info.state,
                        pid: info.pid,
                        uptime_secs: info.uptime_secs,
                        restart_count: info.restart_count,
                    };
                    ControlResponse::ok_with_data(status_info)
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

        ControlRequest::WarmupStatus { symbol: _ } => {
            // TODO: implement warmup status check
            ControlResponse::error("Not implemented yet")
        }

        ControlRequest::Shutdown => {
            // This should trigger supervisor shutdown
            ControlResponse::ok()
        }

        // Infrastructure commands
        ControlRequest::ListInfra => {
            let status = infra_manager.get_all_status_ipc().await;
            ControlResponse::ok_with_data(status)
        }

        ControlRequest::InfraStatus { component } => {
            match component.parse::<InfraComponent>() {
                Ok(comp) => {
                    let status = infra_manager.get_status_ipc(comp).await;
                    ControlResponse::ok_with_data(status)
                }
                Err(e) => ControlResponse::error(e.to_string()),
            }
        }

        ControlRequest::StartInfra { component } => {
            match component.parse::<InfraComponent>() {
                Ok(comp) => match infra_manager.start_component(comp).await {
                    Ok(()) => ControlResponse::ok(),
                    Err(e) => ControlResponse::error(e.to_string()),
                },
                Err(e) => ControlResponse::error(e.to_string()),
            }
        }

        ControlRequest::StopInfra { component } => {
            match component.parse::<InfraComponent>() {
                Ok(comp) => match infra_manager.stop_component(comp).await {
                    Ok(()) => ControlResponse::ok(),
                    Err(e) => ControlResponse::error(e.to_string()),
                },
                Err(e) => ControlResponse::error(e.to_string()),
            }
        }

        ControlRequest::RestartInfra { component } => {
            match component.parse::<InfraComponent>() {
                Ok(comp) => match infra_manager.restart_component(comp).await {
                    Ok(()) => ControlResponse::ok(),
                    Err(e) => ControlResponse::error(e.to_string()),
                },
                Err(e) => ControlResponse::error(e.to_string()),
            }
        }

        ControlRequest::StartAll => {
            // Start infra first, then bots
            if let Err(e) = infra_manager.start_auto_start().await {
                return ControlResponse::error(format!("Failed to start infrastructure: {}", e));
            }
            match manager.start_all().await {
                Ok(()) => ControlResponse::ok(),
                Err(e) => ControlResponse::error(format!("Failed to start bots: {}", e)),
            }
        }

        ControlRequest::StopAll => {
            // Stop bots first, then infra
            if let Err(e) = manager.stop_all().await {
                warn!(error = %e, "Error stopping bots");
            }
            match infra_manager.stop_all().await {
                Ok(()) => ControlResponse::ok(),
                Err(e) => ControlResponse::error(format!("Failed to stop infrastructure: {}", e)),
            }
        }

        ControlRequest::ClusterHealth => {
            let infra_status = infra_manager.get_all_status().await;
            let bot_states = manager.get_bot_states().await;

            let bots_running = bot_states
                .values()
                .filter(|s| s.as_str() == "running")
                .count();
            let bots_total = bot_states.len();

            let health = ClusterHealth::new(&infra_status, bots_running, bots_total);
            let health_ipc: ClusterHealthIpc = health.into();
            ControlResponse::ok_with_data(health_ipc)
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
