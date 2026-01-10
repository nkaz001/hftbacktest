//! Infrastructure component lifecycle management
//!
//! Handles starting, stopping, and monitoring infrastructure components:
//! - Connector: Exchange connection for trading
//! - Collector: Market data collection
//! - OrderRecorder: Order recording to ClickHouse

use std::collections::HashMap;
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfraComponent {
    /// Named connector (supports multiple connectors)
    Connector(String),
    Collector,
    OrderRecorder,
}

impl InfraComponent {
    pub fn as_str(&self) -> String {
        match self {
            InfraComponent::Connector(name) => format!("connector:{}", name),
            InfraComponent::Collector => "collector".to_string(),
            InfraComponent::OrderRecorder => "order_recorder".to_string(),
        }
    }

    pub fn short_name(&self) -> &str {
        match self {
            InfraComponent::Connector(_) => "connector",
            InfraComponent::Collector => "collector",
            InfraComponent::OrderRecorder => "order_recorder",
        }
    }

    pub fn connector_name(&self) -> Option<&str> {
        match self {
            InfraComponent::Connector(name) => Some(name),
            _ => None,
        }
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
        let s_lower = s.to_lowercase();
        if s_lower == "collector" {
            Ok(InfraComponent::Collector)
        } else if s_lower == "order_recorder" || s_lower == "orderrecorder" {
            Ok(InfraComponent::OrderRecorder)
        } else if s_lower.starts_with("connector:") {
            let name = s_lower.strip_prefix("connector:").unwrap().to_string();
            Ok(InfraComponent::Connector(name))
        } else if s_lower == "connector" {
            // Legacy: connector without name - will be resolved to first connector
            Ok(InfraComponent::Connector(String::new()))
        } else {
            anyhow::bail!("Unknown infrastructure component: {}", s)
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
            component: self.component.clone(),
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
    /// Managed components (keyed by component identifier)
    components: Arc<RwLock<HashMap<InfraComponent, ManagedComponent>>>,
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

        // Initialize components map with all connectors + collector + order_recorder
        let mut components = HashMap::new();

        // Add all configured connectors
        for connector in config.all_connectors() {
            let component = InfraComponent::Connector(connector.name.clone());
            components.insert(component.clone(), ManagedComponent::new(component));
        }

        // Add collector and order recorder
        components.insert(
            InfraComponent::Collector,
            ManagedComponent::new(InfraComponent::Collector),
        );
        components.insert(
            InfraComponent::OrderRecorder,
            ManagedComponent::new(InfraComponent::OrderRecorder),
        );

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

    /// Get all connector names
    pub async fn connector_names(&self) -> Vec<String> {
        let config = self.config.read().await;
        config.all_connectors().iter().map(|c| c.name.clone()).collect()
    }

    /// Subscribe to infrastructure events
    pub fn subscribe(&self) -> broadcast::Receiver<InfraEvent> {
        self.event_tx.subscribe()
    }

    /// Get status of all components
    pub async fn get_all_status(&self) -> Vec<InfraStatus> {
        let components = self.components.read().await;
        components.values().map(|c| c.status()).collect()
    }

    /// Get status of all components (IPC-friendly format)
    pub async fn get_all_status_ipc(&self) -> Vec<InfraStatusIpc> {
        self.get_all_status().await.into_iter().map(Into::into).collect()
    }

    /// Get status of a specific component
    pub async fn get_status(&self, component: &InfraComponent) -> Option<InfraStatus> {
        let components = self.components.read().await;
        components.get(component).map(|c| c.status())
    }

    /// Get status of a specific component (IPC-friendly format)
    pub async fn get_status_ipc(&self, component: &InfraComponent) -> Option<InfraStatusIpc> {
        self.get_status(component).await.map(Into::into)
    }

    /// Resolve a connector component (handle legacy "connector" without name)
    async fn resolve_connector(&self, component: &InfraComponent) -> Option<InfraComponent> {
        if let InfraComponent::Connector(name) = component {
            if name.is_empty() {
                // Legacy: resolve to first connector
                let config = self.config.read().await;
                config.default_connector_name().map(|n| InfraComponent::Connector(n.to_string()))
            } else {
                Some(component.clone())
            }
        } else {
            Some(component.clone())
        }
    }

    /// Check if a component is enabled in config
    async fn is_enabled(&self, component: &InfraComponent) -> bool {
        let config = self.config.read().await;
        match component {
            InfraComponent::Connector(name) => {
                config.get_connector(name).is_some()
            }
            InfraComponent::Collector => config.collector.enabled,
            InfraComponent::OrderRecorder => config
                .order_recorder
                .as_ref()
                .map(|c| c.enabled)
                .unwrap_or(false),
        }
    }

    /// Check if a component should auto-start
    async fn should_auto_start(&self, component: &InfraComponent) -> bool {
        let config = self.config.read().await;
        match component {
            InfraComponent::Connector(name) => {
                config.get_connector(name).map(|c| c.auto_start).unwrap_or(false)
            }
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
        // Resolve connector if needed
        let component = self.resolve_connector(&component).await
            .ok_or_else(|| anyhow::anyhow!("No connectors configured"))?;

        // Check if enabled
        if !self.is_enabled(&component).await {
            anyhow::bail!("{} is disabled in configuration", component);
        }

        {
            let mut components = self.components.write().await;
            let comp = components.get_mut(&component)
                .ok_or_else(|| anyhow::anyhow!("Component not found: {}", component))?;

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
                component: component.clone(),
                old_state,
                new_state: ComponentState::Starting,
            });
        }

        // Spawn the process
        let result = self.spawn_component(&component).await;

        if let Err(e) = &result {
            let mut components = self.components.write().await;
            if let Some(comp) = components.get_mut(&component) {
                comp.state = ComponentState::Failed {
                    reason: e.to_string(),
                };
                comp.last_error = Some(e.to_string());
            }
        }

        result
    }

    /// Stop a component
    pub async fn stop_component(&self, component: InfraComponent) -> Result<()> {
        // Resolve connector if needed
        let component = self.resolve_connector(&component).await
            .ok_or_else(|| anyhow::anyhow!("No connectors configured"))?;

        let mut components = self.components.write().await;
        let comp = components.get_mut(&component)
            .ok_or_else(|| anyhow::anyhow!("Component not found: {}", component))?;

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
            component: component.clone(),
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
        self.stop_component(component.clone()).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.start_component(component).await?;
        Ok(())
    }

    /// Start all auto-start components in dependency order
    pub async fn start_auto_start(&self) -> Result<()> {
        // Get all connectors first
        let connector_names = self.connector_names().await;

        // Start connectors first (in order)
        for name in &connector_names {
            let component = InfraComponent::Connector(name.clone());
            if self.should_auto_start(&component).await {
                info!(component = %component, "Auto-starting connector");
                if let Err(e) = self.start_component(component.clone()).await {
                    error!(component = %component, error = %e, "Failed to auto-start connector");
                } else {
                    // Wait a bit for component to stabilize
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        // Then start collector and order recorder
        for component in [InfraComponent::Collector, InfraComponent::OrderRecorder] {
            if self.should_auto_start(&component).await {
                info!(component = %component, "Auto-starting component");
                if let Err(e) = self.start_component(component.clone()).await {
                    error!(component = %component, error = %e, "Failed to auto-start component");
                } else {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        Ok(())
    }

    /// Stop all components in reverse dependency order
    pub async fn stop_all(&self) -> Result<()> {
        // Stop order recorder and collector first
        for component in [InfraComponent::OrderRecorder, InfraComponent::Collector] {
            if let Some(status) = self.get_status(&component).await {
                if matches!(status.state, ComponentState::Running | ComponentState::Starting) {
                    info!(component = %component, "Stopping component");
                    if let Err(e) = self.stop_component(component.clone()).await {
                        error!(component = %component, error = %e, "Failed to stop component");
                    }
                }
            }
        }

        // Then stop all connectors
        let connector_names = self.connector_names().await;
        for name in connector_names.into_iter().rev() {
            let component = InfraComponent::Connector(name);
            if let Some(status) = self.get_status(&component).await {
                if matches!(status.state, ComponentState::Running | ComponentState::Starting) {
                    info!(component = %component, "Stopping connector");
                    if let Err(e) = self.stop_component(component.clone()).await {
                        error!(component = %component, error = %e, "Failed to stop connector");
                    }
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
                for (_key, comp) in components.iter_mut() {
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
                                        component: comp.component.clone(),
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
                .filter_map(|(_key, comp)| {
                    if let ComponentState::Crashed {
                        restart_at: Some(restart_at),
                        ..
                    } = &comp.state
                    {
                        if Instant::now() >= *restart_at {
                            return Some(comp.component.clone());
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
                if let Some(comp) = components.get_mut(&component) {
                    comp.state = ComponentState::Starting;
                }
            }

            if let Err(e) = self.start_component(component.clone()).await {
                error!(component = %component, error = %e, "Failed to restart component");
            }
        }
    }

    /// Spawn a component process
    async fn spawn_component(&self, component: &InfraComponent) -> Result<()> {
        let config = self.config.read().await;

        let (binary_name, args, log_name) = match component {
            InfraComponent::Connector(name) => {
                let connector_config = config.get_connector(name)
                    .ok_or_else(|| anyhow::anyhow!("Connector '{}' not found in config", name))?;
                let (bin, args) = self.build_connector_args(connector_config)?;
                // Use connector name for log file (e.g., connector_bf.log)
                (bin, args, format!("connector_{}", name))
            }
            InfraComponent::Collector => {
                // Use first connector for collector's IPC
                let connectors = config.all_connectors();
                let first_connector = connectors.first()
                    .ok_or_else(|| anyhow::anyhow!("No connectors configured"))?;
                let (bin, args) = self.build_collector_args(first_connector, &config.collector)?;
                (bin, args, "collector".to_string())
            }
            InfraComponent::OrderRecorder => {
                let order_recorder = config
                    .order_recorder
                    .as_ref()
                    .context("Order recorder not configured")?;
                let (bin, args) = self.build_order_recorder_args(order_recorder)?;
                (bin, args, "order_recorder".to_string())
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

        let log_file = std::fs::File::create(log_dir.join(format!("{}.log", log_name)))?;
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
        let comp = components.get_mut(component)
            .ok_or_else(|| anyhow::anyhow!("Component not found: {}", component))?;
        comp.process = Some(child);
        comp.pid = Some(pid);
        comp.started_at = Some(Instant::now());

        let old_state = std::mem::replace(&mut comp.state, ComponentState::Running);

        self.emit_event(InfraEvent::ComponentStarted { component: component.clone(), pid });
        self.emit_event(InfraEvent::ComponentStateChanged {
            component: component.clone(),
            old_state,
            new_state: ComponentState::Running,
        });

        Ok(())
    }

    /// Build connector command arguments
    fn build_connector_args(&self, config: &ConnectorConfig) -> Result<(String, Vec<String>)> {
        // connector --name <NAME> --connector <CONNECTOR> --config <CONFIG>
        let mut args = vec![
            "--name".to_string(),
            config.name.clone(),
            "--connector".to_string(),
            config.connector_type.clone(),
            "--config".to_string(),
            config.config_file.to_string_lossy().to_string(),
        ];

        // Add recording service if configured (enables position/income sync)
        if let Some(ref recording_service) = config.recording_service {
            args.push("--recording-service".to_string());
            args.push(recording_service.clone());
            args.push("--position-sync-interval".to_string());
            args.push(config.position_sync_interval.to_string());
            args.push("--income-sync-interval".to_string());
            args.push(config.income_sync_interval.to_string());
        }

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
            "--max-publishers".to_string(),
            config.max_publishers.to_string(),
        ];
        Ok(("order-recorder".to_string(), args))
    }

    fn emit_event(&self, event: InfraEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// Status of a single connector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorHealth {
    pub name: String,
    pub state: ComponentState,
}

/// IPC-friendly connector health (state as string)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorHealthIpc {
    pub name: String,
    pub state: String,
}

impl From<ConnectorHealth> for ConnectorHealthIpc {
    fn from(health: ConnectorHealth) -> Self {
        Self {
            name: health.name,
            state: health.state.as_str().to_string(),
        }
    }
}

/// Cluster health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    /// All connector statuses
    pub connectors: Vec<ConnectorHealth>,
    pub collector: ComponentState,
    pub order_recorder: ComponentState,
    pub bots_running: usize,
    pub bots_total: usize,
    pub all_healthy: bool,
}

/// IPC-friendly cluster health status (states as strings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealthIpc {
    pub connectors: Vec<ConnectorHealthIpc>,
    pub collector: String,
    pub order_recorder: String,
    pub bots_running: usize,
    pub bots_total: usize,
    pub all_healthy: bool,
}

impl From<ClusterHealth> for ClusterHealthIpc {
    fn from(health: ClusterHealth) -> Self {
        Self {
            connectors: health.connectors.into_iter().map(Into::into).collect(),
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
        // Collect all connector statuses
        let connectors: Vec<ConnectorHealth> = infra_status
            .iter()
            .filter_map(|s| {
                if let InfraComponent::Connector(name) = &s.component {
                    Some(ConnectorHealth {
                        name: name.clone(),
                        state: s.state.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

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

        // All connectors must be running for cluster to be healthy
        let connectors_healthy = !connectors.is_empty()
            && connectors.iter().all(|c| matches!(c.state, ComponentState::Running));
        let collector_healthy = matches!(
            collector,
            ComponentState::Running | ComponentState::Disabled | ComponentState::Stopped
        );
        let order_recorder_healthy = matches!(
            order_recorder,
            ComponentState::Running | ComponentState::Disabled | ComponentState::Stopped
        );

        let all_healthy = connectors_healthy && collector_healthy && order_recorder_healthy;

        Self {
            connectors,
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
        // Legacy connector without name
        assert_eq!(
            "connector".parse::<InfraComponent>().unwrap(),
            InfraComponent::Connector(String::new())
        );
        // Named connector
        assert_eq!(
            "connector:bf".parse::<InfraComponent>().unwrap(),
            InfraComponent::Connector("bf".to_string())
        );
        assert_eq!(
            "connector:bybit".parse::<InfraComponent>().unwrap(),
            InfraComponent::Connector("bybit".to_string())
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
    fn test_component_as_str() {
        assert_eq!(
            InfraComponent::Connector("bf".to_string()).as_str(),
            "connector:bf"
        );
        assert_eq!(InfraComponent::Collector.as_str(), "collector");
        assert_eq!(InfraComponent::OrderRecorder.as_str(), "order_recorder");
    }

    #[test]
    fn test_component_state_as_str() {
        assert_eq!(ComponentState::Stopped.as_str(), "stopped");
        assert_eq!(ComponentState::Running.as_str(), "running");
        assert_eq!(ComponentState::Starting.as_str(), "starting");
    }
}
