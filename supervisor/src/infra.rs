//! Infrastructure component lifecycle management
//!
//! Handles starting, stopping, and monitoring infrastructure components:
//! - Connector: Exchange connection for trading
//! - Collector: Market data collection
//! - OrderRecorder: Order recording to ClickHouse

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::config::{ClusterConfig, CollectorConfig, ConnectorConfig, OrderRecorderConfig};

/// Infrastructure component types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfraComponent {
    Connector,
    Collector,
    OrderRecorder,
}

impl InfraComponent {
    pub fn as_str(&self) -> &'static str {
        match self {
            InfraComponent::Connector => "connector",
            InfraComponent::Collector => "collector",
            InfraComponent::OrderRecorder => "order_recorder",
        }
    }

    pub fn all() -> &'static [InfraComponent] {
        &[
            InfraComponent::Connector,
            InfraComponent::Collector,
            InfraComponent::OrderRecorder,
        ]
    }
}

impl std::fmt::Display for InfraComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for InfraComponent {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "connector" => Ok(InfraComponent::Connector),
            "collector" => Ok(InfraComponent::Collector),
            "order_recorder" | "orderrecorder" => Ok(InfraComponent::OrderRecorder),
            _ => anyhow::bail!("Unknown infrastructure component: {}", s),
        }
    }
}

/// Component process state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    /// Component is stopped
    Stopped,
    /// Component is starting up
    Starting,
    /// Component is running
    Running,
    /// Component crashed
    Crashed {
        exit_code: Option<i32>,
        #[serde(skip)]
        restart_at: Option<Instant>,
    },
    /// Component failed permanently
    Failed { reason: String },
    /// Component is disabled in config
    Disabled,
}

impl ComponentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentState::Stopped => "stopped",
            ComponentState::Starting => "starting",
            ComponentState::Running => "running",
            ComponentState::Crashed { .. } => "crashed",
            ComponentState::Failed { .. } => "failed",
            ComponentState::Disabled => "disabled",
        }
    }
}

/// Status information for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraStatus {
    pub component: InfraComponent,
    pub state: ComponentState,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_error: Option<String>,
}

/// IPC-friendly status information (state as string for JSON serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraStatusIpc {
    pub component: String,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_error: Option<String>,
}

impl From<InfraStatus> for InfraStatusIpc {
    fn from(status: InfraStatus) -> Self {
        Self {
            component: status.component.as_str().to_string(),
            state: status.state.as_str().to_string(),
            pid: status.pid,
            uptime_secs: status.uptime_secs,
            last_error: status.last_error,
        }
    }
}

/// Managed infrastructure component
#[derive(Debug)]
struct ManagedComponent {
    component: InfraComponent,
    state: ComponentState,
    process: Option<Child>,
    pid: Option<u32>,
    started_at: Option<Instant>,
    restart_count: u32,
    last_error: Option<String>,
}

impl ManagedComponent {
    fn new(component: InfraComponent) -> Self {
        Self {
            component,
            state: ComponentState::Stopped,
            process: None,
            pid: None,
            started_at: None,
            restart_count: 0,
            last_error: None,
        }
    }

    fn status(&self) -> InfraStatus {
        InfraStatus {
            component: self.component,
            state: self.state.clone(),
            pid: self.pid,
            uptime_secs: self.started_at.map(|t| t.elapsed().as_secs()),
            last_error: self.last_error.clone(),
        }
    }
}

/// Events emitted by InfraManager
#[derive(Debug, Clone)]
pub enum InfraEvent {
    ComponentStateChanged {
        component: InfraComponent,
        old_state: ComponentState,
        new_state: ComponentState,
    },
    ComponentStarted {
        component: InfraComponent,
        pid: u32,
    },
    ComponentStopped {
        component: InfraComponent,
        exit_code: Option<i32>,
    },
    ComponentCrashed {
        component: InfraComponent,
        exit_code: Option<i32>,
        will_restart: bool,
    },
}

/// Infrastructure component manager
pub struct InfraManager {
    /// Cluster configuration
    config: Arc<RwLock<ClusterConfig>>,
    /// Managed components
    components: Arc<RwLock<[ManagedComponent; 3]>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<InfraEvent>,
    /// Binary directory
    bin_dir: PathBuf,
    /// Log directory
    log_dir: PathBuf,
    /// Auto-restart settings
    max_restarts: u32,
    restart_backoff_secs: u64,
}

impl InfraManager {
    pub fn new(config: ClusterConfig, bin_dir: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let log_dir = config.cluster.log_dir.clone();

        let components = [
            ManagedComponent::new(InfraComponent::Connector),
            ManagedComponent::new(InfraComponent::Collector),
            ManagedComponent::new(InfraComponent::OrderRecorder),
        ];

        Self {
            config: Arc::new(RwLock::new(config)),
            components: Arc::new(RwLock::new(components)),
            event_tx,
            bin_dir,
            log_dir,
            max_restarts: 5,
            restart_backoff_secs: 10,
        }
    }

    /// Subscribe to infrastructure events
    pub fn subscribe(&self) -> broadcast::Receiver<InfraEvent> {
        self.event_tx.subscribe()
    }

    /// Get status of all components
    pub async fn get_all_status(&self) -> Vec<InfraStatus> {
        let components = self.components.read().await;
        components.iter().map(|c| c.status()).collect()
    }

    /// Get status of all components (IPC-friendly format)
    pub async fn get_all_status_ipc(&self) -> Vec<InfraStatusIpc> {
        self.get_all_status().await.into_iter().map(Into::into).collect()
    }

    /// Get status of a specific component
    pub async fn get_status(&self, component: InfraComponent) -> InfraStatus {
        let components = self.components.read().await;
        let idx = component as usize;
        components[idx].status()
    }

    /// Get status of a specific component (IPC-friendly format)
    pub async fn get_status_ipc(&self, component: InfraComponent) -> InfraStatusIpc {
        self.get_status(component).await.into()
    }

    /// Check if a component is enabled in config
    async fn is_enabled(&self, component: InfraComponent) -> bool {
        let config = self.config.read().await;
        match component {
            InfraComponent::Connector => true, // Connector is always required
            InfraComponent::Collector => config.collector.enabled,
            InfraComponent::OrderRecorder => config
                .order_recorder
                .as_ref()
                .map(|c| c.enabled)
                .unwrap_or(false),
        }
    }

    /// Check if a component should auto-start
    async fn should_auto_start(&self, component: InfraComponent) -> bool {
        let config = self.config.read().await;
        match component {
            InfraComponent::Connector => config.connector.auto_start,
            InfraComponent::Collector => config.collector.enabled && config.collector.auto_start,
            InfraComponent::OrderRecorder => config
                .order_recorder
                .as_ref()
                .map(|c| c.enabled && c.auto_start)
                .unwrap_or(false),
        }
    }

    /// Start a component
    pub async fn start_component(&self, component: InfraComponent) -> Result<()> {
        // Check if enabled
        if !self.is_enabled(component).await {
            anyhow::bail!("{} is disabled in configuration", component);
        }

        let mut components = self.components.write().await;
        let idx = component as usize;
        let comp = &mut components[idx];

        match &comp.state {
            ComponentState::Running | ComponentState::Starting => {
                anyhow::bail!("{} is already running or starting", component);
            }
            _ => {}
        }

        // Update state to starting
        let old_state = std::mem::replace(&mut comp.state, ComponentState::Starting);
        drop(components);

        self.emit_event(InfraEvent::ComponentStateChanged {
            component,
            old_state,
            new_state: ComponentState::Starting,
        });

        // Spawn the process
        let result = self.spawn_component(component).await;

        if let Err(e) = &result {
            let mut components = self.components.write().await;
            let comp = &mut components[component as usize];
            comp.state = ComponentState::Failed {
                reason: e.to_string(),
            };
            comp.last_error = Some(e.to_string());
        }

        result
    }

    /// Stop a component
    pub async fn stop_component(&self, component: InfraComponent) -> Result<()> {
        let mut components = self.components.write().await;
        let idx = component as usize;
        let comp = &mut components[idx];

        if let Some(pid) = comp.pid {
            info!(component = %component, pid = pid, "Sending SIGTERM to component");

            // Send SIGTERM
            if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                warn!(component = %component, pid = pid, error = %e, "Failed to send SIGTERM");
            }

            // Wait for graceful shutdown
            let shutdown_timeout = Duration::from_secs(30);
            let start = Instant::now();

            while start.elapsed() < shutdown_timeout {
                if let Some(ref mut process) = comp.process {
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            info!(
                                component = %component,
                                exit_code = status.code(),
                                "Component exited gracefully"
                            );
                            break;
                        }
                        Ok(None) => {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        Err(e) => {
                            warn!(component = %component, error = %e, "Error waiting for component");
                            break;
                        }
                    }
                }
            }

            // Force kill if still running
            if let Some(ref mut process) = comp.process {
                if process.try_wait().ok().flatten().is_none() {
                    warn!(component = %component, "Component did not exit gracefully, sending SIGKILL");
                    if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL) {
                        error!(component = %component, error = %e, "Failed to send SIGKILL");
                    }
                    let _ = process.wait().await;
                }
            }
        }

        let old_state = std::mem::replace(&mut comp.state, ComponentState::Stopped);
        comp.process = None;
        comp.pid = None;
        comp.started_at = None;

        self.emit_event(InfraEvent::ComponentStopped {
            component,
            exit_code: None,
        });
        self.emit_event(InfraEvent::ComponentStateChanged {
            component,
            old_state,
            new_state: ComponentState::Stopped,
        });

        Ok(())
    }

    /// Restart a component
    pub async fn restart_component(&self, component: InfraComponent) -> Result<()> {
        self.stop_component(component).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.start_component(component).await?;
        Ok(())
    }

    /// Start all auto-start components in dependency order
    pub async fn start_auto_start(&self) -> Result<()> {
        // Start in dependency order: Connector -> Collector -> OrderRecorder
        let startup_order = [
            InfraComponent::Connector,
            InfraComponent::Collector,
            InfraComponent::OrderRecorder,
        ];

        for component in startup_order {
            if self.should_auto_start(component).await {
                info!(component = %component, "Auto-starting component");
                if let Err(e) = self.start_component(component).await {
                    error!(component = %component, error = %e, "Failed to auto-start component");
                } else {
                    // Wait a bit for component to stabilize
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        Ok(())
    }

    /// Stop all components in reverse dependency order
    pub async fn stop_all(&self) -> Result<()> {
        // Stop in reverse dependency order
        let shutdown_order = [
            InfraComponent::OrderRecorder,
            InfraComponent::Collector,
            InfraComponent::Connector,
        ];

        for component in shutdown_order {
            let state = self.get_status(component).await.state;
            if matches!(state, ComponentState::Running | ComponentState::Starting) {
                info!(component = %component, "Stopping component");
                if let Err(e) = self.stop_component(component).await {
                    error!(component = %component, error = %e, "Failed to stop component");
                }
            }
        }

        Ok(())
    }

    /// Monitor running components and handle crashes
    pub async fn monitor(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let mut updates = Vec::new();

            {
                let mut components = self.components.write().await;
                for comp in components.iter_mut() {
                    // Check running components for exit
                    if let ComponentState::Running = comp.state {
                        if let Some(ref mut process) = comp.process {
                            match process.try_wait() {
                                Ok(Some(status)) => {
                                    let exit_code = status.code();
                                    warn!(
                                        component = %comp.component,
                                        exit_code = exit_code,
                                        "Component process exited"
                                    );

                                    let will_restart = comp.restart_count < self.max_restarts;

                                    if will_restart {
                                        let restart_at =
                                            Instant::now() + Duration::from_secs(self.restart_backoff_secs);
                                        comp.state = ComponentState::Crashed {
                                            exit_code,
                                            restart_at: Some(restart_at),
                                        };
                                        comp.restart_count += 1;
                                    } else {
                                        comp.state = ComponentState::Failed {
                                            reason: format!(
                                                "Exceeded max restarts ({})",
                                                self.max_restarts
                                            ),
                                        };
                                    }

                                    comp.process = None;
                                    comp.pid = None;

                                    updates.push(InfraEvent::ComponentCrashed {
                                        component: comp.component,
                                        exit_code,
                                        will_restart,
                                    });
                                }
                                Ok(None) => {} // Still running
                                Err(e) => {
                                    error!(
                                        component = %comp.component,
                                        error = %e,
                                        "Error checking component status"
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Emit events
            for event in updates {
                self.emit_event(event);
            }

            // Handle restarts
            self.handle_pending_restarts().await;
        }
    }

    /// Handle crashed components that need restart
    async fn handle_pending_restarts(&self) {
        let components_to_restart: Vec<InfraComponent> = {
            let components = self.components.read().await;
            components
                .iter()
                .filter_map(|comp| {
                    if let ComponentState::Crashed {
                        restart_at: Some(restart_at),
                        ..
                    } = &comp.state
                    {
                        if Instant::now() >= *restart_at {
                            return Some(comp.component);
                        }
                    }
                    None
                })
                .collect()
        };

        for component in components_to_restart {
            info!(component = %component, "Attempting to restart crashed component");

            // Update state to starting
            {
                let mut components = self.components.write().await;
                let comp = &mut components[component as usize];
                comp.state = ComponentState::Starting;
            }

            if let Err(e) = self.start_component(component).await {
                error!(component = %component, error = %e, "Failed to restart component");
            }
        }
    }

    /// Spawn a component process
    async fn spawn_component(&self, component: InfraComponent) -> Result<()> {
        let config = self.config.read().await;

        let (binary_name, args) = match component {
            InfraComponent::Connector => {
                self.build_connector_args(&config.connector)?
            }
            InfraComponent::Collector => {
                self.build_collector_args(&config.connector, &config.collector)?
            }
            InfraComponent::OrderRecorder => {
                let order_recorder = config
                    .order_recorder
                    .as_ref()
                    .context("Order recorder not configured")?;
                self.build_order_recorder_args(order_recorder)?
            }
        };

        let binary_path = self.bin_dir.join(&binary_name);
        if !binary_path.exists() {
            anyhow::bail!("Binary not found: {}", binary_path.display());
        }

        // Build command
        let mut cmd = Command::new(&binary_path);
        cmd.args(&args);

        // Set up logging
        let log_dir = self.log_dir.join("infra");
        std::fs::create_dir_all(&log_dir)?;

        let log_file = std::fs::File::create(log_dir.join(format!("{}.log", component)))?;
        cmd.stdout(Stdio::from(log_file.try_clone()?));
        cmd.stderr(Stdio::from(log_file));

        // Spawn process
        let child = cmd.spawn().with_context(|| {
            format!("Failed to spawn {} process", component)
        })?;

        let pid = child.id().unwrap_or(0);
        info!(component = %component, pid = pid, "Component process started");

        // Update state
        let mut components = self.components.write().await;
        let comp = &mut components[component as usize];
        comp.process = Some(child);
        comp.pid = Some(pid);
        comp.started_at = Some(Instant::now());

        let old_state = std::mem::replace(&mut comp.state, ComponentState::Running);

        self.emit_event(InfraEvent::ComponentStarted { component, pid });
        self.emit_event(InfraEvent::ComponentStateChanged {
            component,
            old_state,
            new_state: ComponentState::Running,
        });

        Ok(())
    }

    /// Build connector command arguments
    fn build_connector_args(&self, config: &ConnectorConfig) -> Result<(String, Vec<String>)> {
        // connector <NAME> <CONNECTOR> <CONFIG>
        let args = vec![
            config.name.clone(),
            config.connector_type.clone(),
            config.config_file.to_string_lossy().to_string(),
        ];
        Ok(("connector".to_string(), args))
    }

    /// Build collector command arguments
    fn build_collector_args(
        &self,
        connector_config: &ConnectorConfig,
        config: &CollectorConfig,
    ) -> Result<(String, Vec<String>)> {
        // collector [OPTIONS] <PATH> [EXCHANGE] [SYMBOLS]...
        // IPC mode: collector <output_dir> "" <symbols...> --ipc <connector_name>
        // Direct mode: collector <output_dir> <exchange> [symbols...]

        let mut args = vec![
            config.output_dir.to_string_lossy().to_string(),
        ];

        if config.mode == "ipc" {
            // IPC mode - subscribe to running connector
            // Need empty string for exchange placeholder before symbols
            args.push(String::new());
            // Symbols are required in IPC mode
            for symbol in &config.symbols {
                args.push(symbol.clone());
            }
            // IPC option at the end
            args.push("--ipc".to_string());
            args.push(connector_config.name.clone());
        } else {
            // Direct mode - connect to exchange directly
            args.push(connector_config.connector_type.clone());
            // Add symbols as positional arguments
            for symbol in &config.symbols {
                args.push(symbol.clone());
            }
        }

        Ok(("collector".to_string(), args))
    }

    /// Build order recorder command arguments
    fn build_order_recorder_args(&self, config: &OrderRecorderConfig) -> Result<(String, Vec<String>)> {
        let args = vec![
            "--ipc-service".to_string(),
            config.ipc_service.clone(),
            "--clickhouse-url".to_string(),
            config.clickhouse_url.clone(),
            "--database".to_string(),
            config.database.clone(),
            "--batch-size".to_string(),
            config.batch_size.to_string(),
            "--flush-interval".to_string(),
            config.flush_interval_secs.to_string(),
        ];
        Ok(("order-recorder".to_string(), args))
    }

    fn emit_event(&self, event: InfraEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// Cluster health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    pub connector: ComponentState,
    pub collector: ComponentState,
    pub order_recorder: ComponentState,
    pub bots_running: usize,
    pub bots_total: usize,
    pub all_healthy: bool,
}

/// IPC-friendly cluster health status (states as strings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealthIpc {
    pub connector: String,
    pub collector: String,
    pub order_recorder: String,
    pub bots_running: usize,
    pub bots_total: usize,
    pub all_healthy: bool,
}

impl From<ClusterHealth> for ClusterHealthIpc {
    fn from(health: ClusterHealth) -> Self {
        Self {
            connector: health.connector.as_str().to_string(),
            collector: health.collector.as_str().to_string(),
            order_recorder: health.order_recorder.as_str().to_string(),
            bots_running: health.bots_running,
            bots_total: health.bots_total,
            all_healthy: health.all_healthy,
        }
    }
}

impl ClusterHealth {
    pub fn new(
        infra_status: &[InfraStatus],
        bots_running: usize,
        bots_total: usize,
    ) -> Self {
        let connector = infra_status
            .iter()
            .find(|s| s.component == InfraComponent::Connector)
            .map(|s| s.state.clone())
            .unwrap_or(ComponentState::Disabled);

        let collector = infra_status
            .iter()
            .find(|s| s.component == InfraComponent::Collector)
            .map(|s| s.state.clone())
            .unwrap_or(ComponentState::Disabled);

        let order_recorder = infra_status
            .iter()
            .find(|s| s.component == InfraComponent::OrderRecorder)
            .map(|s| s.state.clone())
            .unwrap_or(ComponentState::Disabled);

        // All healthy if connector is running (required) and enabled components are running
        let connector_healthy = matches!(connector, ComponentState::Running);
        let collector_healthy = matches!(
            collector,
            ComponentState::Running | ComponentState::Disabled | ComponentState::Stopped
        );
        let order_recorder_healthy = matches!(
            order_recorder,
            ComponentState::Running | ComponentState::Disabled | ComponentState::Stopped
        );

        let all_healthy = connector_healthy && collector_healthy && order_recorder_healthy;

        Self {
            connector,
            collector,
            order_recorder,
            bots_running,
            bots_total,
            all_healthy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_from_str() {
        assert_eq!(
            "connector".parse::<InfraComponent>().unwrap(),
            InfraComponent::Connector
        );
        assert_eq!(
            "collector".parse::<InfraComponent>().unwrap(),
            InfraComponent::Collector
        );
        assert_eq!(
            "order_recorder".parse::<InfraComponent>().unwrap(),
            InfraComponent::OrderRecorder
        );
        assert!("invalid".parse::<InfraComponent>().is_err());
    }

    #[test]
    fn test_component_state_as_str() {
        assert_eq!(ComponentState::Stopped.as_str(), "stopped");
        assert_eq!(ComponentState::Running.as_str(), "running");
        assert_eq!(ComponentState::Starting.as_str(), "starting");
    }
}
