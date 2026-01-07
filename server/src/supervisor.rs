//! Supervisor IPC client
//!
//! Communicates with hft-supervisor via Unix socket.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;

/// Request to supervisor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum SupervisorRequest {
    Status,
    BotStatus { bot_id: String },
    StartBot { bot_id: String },
    StopBot { bot_id: String },
    RestartBot { bot_id: String },
    RestartAll,
    Reload,
    WarmupStatus { symbol: Option<String> },
    // Infrastructure commands
    ListInfra,
    InfraStatus { component: String },
    StartInfra { component: String },
    StopInfra { component: String },
    RestartInfra { component: String },
    StartAll,
    StopAll,
    ClusterHealth,
}

/// Response from supervisor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SupervisorResponse {
    Ok { data: Option<serde_json::Value> },
    Error { message: String },
}

/// Cluster status from supervisor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub cluster_name: String,
    pub uptime_secs: u64,
    pub connector: ComponentStatus,
    pub collector: ComponentStatus,
    pub bots: Vec<BotStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub status: String,
    pub healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotStatus {
    pub id: String,
    pub symbol: String,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub restart_count: u32,
}

/// Infrastructure component types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfraComponent {
    Connector,
    Collector,
    OrderRecorder,
}

impl std::fmt::Display for InfraComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InfraComponent::Connector => write!(f, "connector"),
            InfraComponent::Collector => write!(f, "collector"),
            InfraComponent::OrderRecorder => write!(f, "order_recorder"),
        }
    }
}

/// Infrastructure component status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraStatus {
    pub component: InfraComponent,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_error: Option<String>,
}

/// Cluster health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealthStatus {
    pub connector: String,
    pub collector: String,
    pub order_recorder: String,
    pub bots_running: usize,
    pub bots_total: usize,
    pub all_healthy: bool,
}

/// Supervisor client for communicating with hft-supervisor
#[derive(Clone)]
pub struct SupervisorClient {
    socket_path: PathBuf,
    /// Cached status for quick access
    cached_status: Arc<RwLock<Option<ClusterStatus>>>,
}

impl SupervisorClient {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            cached_status: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if supervisor is available
    pub async fn is_available(&self) -> bool {
        self.socket_path.exists()
    }

    /// Send a request to supervisor and get response
    pub async fn send(&self, request: SupervisorRequest) -> Result<SupervisorResponse> {
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

        let response: SupervisorResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Get cluster status
    pub async fn get_status(&self) -> Result<ClusterStatus> {
        let response = self.send(SupervisorRequest::Status).await?;

        match response {
            SupervisorResponse::Ok { data: Some(data) } => {
                let status: ClusterStatus = serde_json::from_value(data)?;
                // Update cache
                *self.cached_status.write().await = Some(status.clone());
                Ok(status)
            }
            SupervisorResponse::Ok { data: None } => {
                anyhow::bail!("No status data returned")
            }
            SupervisorResponse::Error { message } => {
                anyhow::bail!("Supervisor error: {}", message)
            }
        }
    }

    /// Get cached status (for quick access without IPC)
    pub async fn get_cached_status(&self) -> Option<ClusterStatus> {
        self.cached_status.read().await.clone()
    }

    /// Get bot status
    pub async fn get_bot_status(&self, bot_id: &str) -> Result<BotStatus> {
        let response = self
            .send(SupervisorRequest::BotStatus {
                bot_id: bot_id.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { data: Some(data) } => {
                let status: BotStatus = serde_json::from_value(data)?;
                Ok(status)
            }
            SupervisorResponse::Ok { data: None } => {
                anyhow::bail!("No bot status returned")
            }
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Start a bot
    pub async fn start_bot(&self, bot_id: &str) -> Result<()> {
        let response = self
            .send(SupervisorRequest::StartBot {
                bot_id: bot_id.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Stop a bot
    pub async fn stop_bot(&self, bot_id: &str) -> Result<()> {
        let response = self
            .send(SupervisorRequest::StopBot {
                bot_id: bot_id.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Restart a bot
    pub async fn restart_bot(&self, bot_id: &str) -> Result<()> {
        let response = self
            .send(SupervisorRequest::RestartBot {
                bot_id: bot_id.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Restart all bots
    pub async fn restart_all(&self) -> Result<()> {
        let response = self.send(SupervisorRequest::RestartAll).await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Reload configuration
    pub async fn reload(&self) -> Result<()> {
        let response = self.send(SupervisorRequest::Reload).await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    // Infrastructure management methods

    /// List all infrastructure components
    pub async fn list_infra(&self) -> Result<Vec<InfraStatus>> {
        let response = self.send(SupervisorRequest::ListInfra).await?;

        match response {
            SupervisorResponse::Ok { data: Some(data) } => {
                let status: Vec<InfraStatus> = serde_json::from_value(data)?;
                Ok(status)
            }
            SupervisorResponse::Ok { data: None } => {
                anyhow::bail!("No infra status returned")
            }
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Get status of a specific infrastructure component
    pub async fn get_infra_status(&self, component: &str) -> Result<InfraStatus> {
        let response = self
            .send(SupervisorRequest::InfraStatus {
                component: component.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { data: Some(data) } => {
                let status: InfraStatus = serde_json::from_value(data)?;
                Ok(status)
            }
            SupervisorResponse::Ok { data: None } => {
                anyhow::bail!("No infra status returned")
            }
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Start an infrastructure component
    pub async fn start_infra(&self, component: &str) -> Result<()> {
        let response = self
            .send(SupervisorRequest::StartInfra {
                component: component.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Stop an infrastructure component
    pub async fn stop_infra(&self, component: &str) -> Result<()> {
        let response = self
            .send(SupervisorRequest::StopInfra {
                component: component.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Restart an infrastructure component
    pub async fn restart_infra(&self, component: &str) -> Result<()> {
        let response = self
            .send(SupervisorRequest::RestartInfra {
                component: component.to_string(),
            })
            .await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Start all components (infra + bots)
    pub async fn start_all(&self) -> Result<()> {
        let response = self.send(SupervisorRequest::StartAll).await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Stop all components (bots + infra)
    pub async fn stop_all(&self) -> Result<()> {
        let response = self.send(SupervisorRequest::StopAll).await?;

        match response {
            SupervisorResponse::Ok { .. } => Ok(()),
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }

    /// Get cluster health status
    pub async fn get_cluster_health(&self) -> Result<ClusterHealthStatus> {
        let response = self.send(SupervisorRequest::ClusterHealth).await?;

        match response {
            SupervisorResponse::Ok { data: Some(data) } => {
                let health: ClusterHealthStatus = serde_json::from_value(data)?;
                Ok(health)
            }
            SupervisorResponse::Ok { data: None } => {
                anyhow::bail!("No health status returned")
            }
            SupervisorResponse::Error { message } => {
                anyhow::bail!("{}", message)
            }
        }
    }
}

/// Background task to periodically update cached status
pub async fn status_updater(client: SupervisorClient, interval_secs: u64) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

    loop {
        interval.tick().await;

        if client.is_available().await {
            if let Err(e) = client.get_status().await {
                tracing::warn!(error = %e, "Failed to update cached status");
            }
        }
    }
}
